// Author: Julian Bolivar
// Version: 4.1.0
// Date: 2026-09-09

use crate::error::ProviderError;
use crate::provider::{
    Completion, CompletionConfig, CompletionTelemetry, LlmProvider, ReasoningControl,
    ReasoningState, resolve_claude_alias,
};
use serde::Deserialize;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// LLM provider that communicates with Claude via the `claude` CLI tool
/// as a subprocess using `tokio::process`.
///
/// Supports model alias resolution (e.g., "sonnet" maps to "claude-sonnet-4-6"),
/// detects nested session environments via the `CLAUDECODE` env var, and handles
/// double-nested JSON parsing from the CLI output envelope.
///
/// Feature-gated behind `claude-cli`.
///
/// # Security
///
/// `tokio::process::Command::new("claude")` calls the executable directly without
/// invoking a shell, preventing shell injection even with user-controlled prompts.
/// User prompts are sent via stdin, never as command-line arguments (which could
/// be visible in process listings).
///
/// # Windows Limitation
///
/// `child.kill()` on Windows uses `TerminateProcess`, which does not propagate to
/// grandchild processes. If the `claude` CLI spawns subprocesses, those may survive
/// a timeout kill.
///
/// # Examples
///
/// ```no_run
/// use magi_core::providers::claude_cli::ClaudeCliProvider;
/// use magi_core::provider::{LlmProvider, CompletionConfig};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let provider = ClaudeCliProvider::new("sonnet")?;
/// assert_eq!(provider.name(), "claude-cli");
/// assert_eq!(provider.model(), "claude-sonnet-4-6");
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct ClaudeCliProvider {
    /// The resolved model identifier sent to the CLI.
    model_id: String,
    /// The executable to launch. `"claude"` for every constructor a consumer can
    /// reach; the test-only constructor points it at a stub instead.
    ///
    /// It exists because the defect this provider carries is a deadlock between
    /// TWO PROCESSES, and a mock cannot reproduce one: the test needs a real child
    /// that writes to stderr before it reads stdin. Nothing else about the
    /// provider changes -- the default is the same literal that was hard-coded.
    binary: String,
}

/// The executable every consumer-reachable constructor launches.
const DEFAULT_BINARY: &str = "claude";

impl ClaudeCliProvider {
    /// Creates a new `ClaudeCliProvider` with model alias resolution.
    ///
    /// # Model Aliases
    ///
    /// - `"sonnet"` maps to `"claude-sonnet-4-6"`
    /// - `"opus"` maps to `"claude-opus-4-7"`
    /// - `"haiku"` maps to `"claude-haiku-4-5-20251001"`
    /// - Any string containing `"claude-"` is passed through as-is
    /// - Anything else returns `ProviderError::Auth`
    ///
    /// # Errors
    ///
    /// - `ProviderError::NestedSession` if `CLAUDECODE` env var is set
    /// - `ProviderError::Auth` if the model alias is unknown
    pub fn new(model: impl Into<String>) -> Result<Self, ProviderError> {
        if std::env::var("CLAUDECODE").is_ok() {
            return Err(ProviderError::NestedSession);
        }

        let model = model.into();
        let model_id = resolve_claude_alias(&model)?;

        Ok(Self {
            model_id,
            binary: DEFAULT_BINARY.to_string(),
        })
    }

    /// Creates a provider that launches `binary` instead of `claude`.
    ///
    /// Test-only, and behind `test-utils`, which is NOT in `default` -- so a consumer
    /// reaches it only by asking for it. It is not invisible, though: `docs.rs` builds
    /// this crate with every feature on, so it renders there like any other published
    /// item. What the gate buys is that nobody depends on it by accident, not that it
    /// cannot be seen.
    ///
    /// It exists for one reason: the deadlock this provider can hit is between two
    /// processes, and reproducing it needs a real child whose behaviour the test
    /// controls.
    ///
    /// **The nested-session guard is KEPT deliberately.** Pointing the provider at
    /// a stub does not make the caller any less nested, and dropping the check here
    /// would mean the test path and the production path disagree about when
    /// construction is legal -- so a test could pass in a shape production refuses.
    /// The consequence is real and the caller has to handle it: a development
    /// machine that IS a Claude Code session sets `CLAUDECODE`, so every
    /// integration test has to clear it around the construction. That is what
    /// [`crate::test_support::without_claudecode`] is for.
    ///
    /// # Errors
    ///
    /// - [`ProviderError::NestedSession`] if `CLAUDECODE` is set.
    /// - [`ProviderError::Auth`] if the model alias is unknown.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn with_binary(
        binary: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self, ProviderError> {
        if std::env::var("CLAUDECODE").is_ok() {
            return Err(ProviderError::NestedSession);
        }

        let model = model.into();
        let model_id = resolve_claude_alias(&model)?;

        Ok(Self {
            model_id,
            binary: binary.into(),
        })
    }

    /// Returns the provider name.
    pub fn name(&self) -> &str {
        "claude-cli"
    }

    /// Returns the resolved model identifier.
    pub fn model(&self) -> &str {
        &self.model_id
    }

    /// Builds the CLI arguments for launching the `claude` subprocess.
    ///
    /// Returns a `Vec<String>` containing:
    /// `["--print", "--output-format", "json", "--model", model_id, "--system-prompt", system_prompt]`
    ///
    /// **`system_prompt` travels on argv here**, so it is subject to the
    /// operating system's command-line length limit — about 32,767
    /// characters for the whole command line on Windows, versus roughly
    /// 131,072 bytes per argument on Linux. Whoever edits this function
    /// should keep that in mind: a caller that grows the system prompt
    /// enough (for instance, a large custom prompt loaded through
    /// `from_directory`) can exhaust that budget, and the failure would
    /// surface as an opaque process-spawn error rather than a message about
    /// prompt length.
    fn build_args(&self, system_prompt: &str) -> Vec<String> {
        vec![
            "--print".to_string(),
            "--output-format".to_string(),
            "json".to_string(),
            "--model".to_string(),
            self.model_id.clone(),
            "--system-prompt".to_string(),
            system_prompt.to_string(),
        ]
    }
}

/// Deserializes the CLI envelope ONCE and applies its error convention.
///
/// Split out so a caller that also wants the envelope's `usage` does not pay for a second full
/// parse of the same bytes into the same type — which is what it did, and the envelope carries a
/// whole completion's worth of text.
fn parse_envelope(raw: &str) -> Result<CliOutput, ProviderError> {
    let output: CliOutput = serde_json::from_str(raw).map_err(|e| ProviderError::Process {
        exit_code: None,
        // CAPPED, and this is the producer that made the field's guarantee false:
        // `serde_json` embeds the offending value VERBATIM, and the offending value is
        // the child's stdout -- text this crate did not author, with no bound of its
        // own. Measured before this line: a 100 KB value produced a 100,095-byte
        // diagnosis on its way to `failed_agents` and the serialized report.
        stderr: cap_to_error_budget(format!("failed to parse CLI output: {e}")),
    })?;

    if output.is_error {
        // No process to speak of here: this is the envelope alone, parsed from a
        // string. Whoever has an exit code passes it.
        return Err(classify_envelope_error(output, None));
    }

    Ok(output)
}

/// The lowest status this provider will hand to [`ProviderError::Http`].
const MIN_HTTP_STATUS: i64 = 100;

/// The highest status this provider will hand to [`ProviderError::Http`].
const MAX_HTTP_STATUS: i64 = 599;

/// Turns a failing envelope into the error its `api_error_status` describes.
///
/// # Why the status decides, and why it is gated by range
///
/// The CLI passes the upstream API's HTTP status through, so a rate limit reported
/// this way is the same condition the HTTP provider reports as `429` -- and it goes
/// through the same table. Before this, every in-band failure became `Process`, which
/// is hard-coded non-retryable, so the identical condition was retried on one path
/// and abandoned on the other with nothing declaring the asymmetry.
///
/// The range gate is not decoration. `Http.status` is `u16` and governs LINEAGE
/// CONDEMNATION, so admitting a negative or a `99999` would not be "an odd status" --
/// it would fabricate one no server returned and let it decide which lineage is
/// condemned. `u16::try_from` and never `as u16`: the cast truncates in silence, and
/// a `99999` would become `34463`, a perfectly valid `u16` and a status nobody sent.
///
/// A real status outside the range, if one ever existed, falls to `Process`. That is
/// the safe direction: `Process` is mage-local and condemns no lineage, so the error
/// is paid in diagnosis rather than in reach.
///
/// # Complexity
/// O(n) in the length of `result`, from the cap's single copy.
fn classify_envelope_error(output: CliOutput, exit_code: Option<i32>) -> ProviderError {
    let status = match output.api_error_status {
        ApiErrorStatus::Value(n) if (MIN_HTTP_STATUS..=MAX_HTTP_STATUS).contains(&n) => {
            u16::try_from(n).ok()
        }
        ApiErrorStatus::Value(_) => None,
        ApiErrorStatus::Unreadable => {
            // PER EVENT, and deliberately not latched: the other warnings in this
            // crate name a state the builder fixes, so a second telling would describe
            // something that provably did not change. This one names an ENVELOPE, and
            // every completion brings a different one -- latching it would hide the
            // second occurrence, which is the datum that says the backend changed its
            // wire format mid-run.
            tracing::warn!(
                "the CLI envelope carried an api_error_status that is not an integer; classifying as a local process failure"
            );
            None
        }
        // The field never came, so this failure never reached the API.
        ApiErrorStatus::Absent => None,
    };

    match status {
        Some(status) => ProviderError::Http {
            status,
            body: cap_to_error_budget(output.result),
            retry_after_raw: Vec::new(),
            received_at: None,
        },
        // LABELLED, like the exit != 0 path, and CAPPED, like its sibling arm above.
        // This is the dominant in-band failure path, and leaving it raw made the
        // published contract of `ProviderError::Process.stderr` false: that field now
        // states that this provider labels whatever is not really stderr. The label
        // caps as a side effect, so the bound comes with it.
        // The exit code is the CALLER's to supply, and it is not always `None`: an
        // envelope that classifies a failure does not erase the fact that the child
        // ALSO died with its own code. `exit_code: None` is defined by this crate's
        // published rustdoc as "the process did not fail", so hard-coding it here made
        // that field say the opposite of what happened on the non-zero-exit path.
        None => ProviderError::Process {
            exit_code,
            stderr: label_envelope_diagnosis(&output.result),
        },
    }
}

/// Bounds UNLABELLED text to the budget every `ProviderError` body fits within.
///
/// Only caps when the text EXCEEDS the bound: text that fits travels whole and RAW.
/// The three `label_*` functions are the labelled counterpart; this is what everything
/// else goes through, and "everything else" is the load-bearing word -- `Process.stderr`
/// publishes a boundedness guarantee, and a guarantee is only as true as its least
/// careful producer. It was named `cap_envelope_body` while the envelope's `result` was
/// its only caller; the name stopped being true when the arbitrary-sized producers
/// started using it.
fn cap_to_error_budget(text: String) -> String {
    if text.len() <= crate::error::MAX_ERROR_BODY_PREFIX_BYTES {
        return text;
    }
    crate::error::mark_within_cap(&text, crate::error::MAX_ERROR_BODY_PREFIX_BYTES)
}

/// Strips code fences from text.
///
/// Removes `` ```json\n `` prefix and `` \n``` `` suffix if present.
/// Also handles plain `` ``` `` without language tag.
fn strip_code_fences(text: &str) -> &str {
    let stripped = text
        .strip_prefix("```json\n")
        .or_else(|| text.strip_prefix("```\n"));

    match stripped {
        Some(inner) => inner.strip_suffix("\n```").unwrap_or(inner),
        None => text,
    }
}

/// The three states of the envelope's `api_error_status`, which an `Option` could
/// not tell apart without the reader remembering which `None` was which.
///
/// * `Absent` -- the field did not come. A local CLI failure never reached the API.
/// * `Unreadable` -- it came and is not an integer.
/// * `Value(n)` -- it came and is an integer; whether it is a usable status is a
///   later question, and one this type deliberately does not answer.
#[derive(Debug, Default, PartialEq)]
pub(crate) enum ApiErrorStatus {
    #[default]
    Absent,
    Unreadable,
    Value(i64),
}

/// Reads `api_error_status` FAIL-SOFT, in both dimensions, and never returns `Err`.
///
/// The field comes from JSON this crate did not write, so a shape it cannot read must
/// not kill the envelope: `result` is the only diagnosis the CLI gives, and losing it
/// to a wire-format surprise is the same damage the whole REQ exists to avoid, entering
/// through the type instead of through the range.
///
/// Anything that is not a JSON integer becomes [`ApiErrorStatus::Unreadable`] -- a
/// string, `null`, a container, a bool, a float, and the overflow case, which is the
/// one nobody writes: an integer that is a perfectly good JSON number and does not fit
/// `i64`. It is `Unreadable` and not `Absent` because "the field did not come" and
/// "it came and I could not read it" are a local CLI failure and a wire-format
/// regression, and collapsing them is what an `Option` did.
///
/// Whether a `Value(n)` is a USABLE status is a later question, and deliberately not
/// asked here: this answers "is it an integer", the range gate answers "is it a status".
fn de_api_error_status<'de, D>(deserializer: D) -> Result<ApiErrorStatus, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(match value.as_i64() {
        Some(n) => ApiErrorStatus::Value(n),
        None => ApiErrorStatus::Unreadable,
    })
}

/// Outer JSON envelope from the Claude CLI tool.
#[derive(Debug, Deserialize)]
struct CliOutput {
    is_error: bool,
    result: String,
    /// Why the backend stopped, when it said. Absent on envelopes that do not
    /// carry it, which is a `None` that means "this one did not say" rather than
    /// "this backend cannot".
    #[serde(default)]
    stop_reason: Option<String>,
    /// The upstream HTTP status the CLI passed through, when it did.
    ///
    /// Read by [`classify_envelope_error`], which decides whether it is a status this
    /// provider will hand to [`ProviderError::Http`].
    #[serde(default, deserialize_with = "de_api_error_status")]
    api_error_status: ApiErrorStatus,
    /// Token counts, when the CLI envelope reports them. `#[serde(default)]`:
    /// the envelope does not guarantee this field across CLI versions, and
    /// `is_error`/`result` extraction must not depend on it.
    #[serde(default)]
    usage: Option<CliUsage>,
}

/// Token counts from the CLI envelope's `usage` object.
///
/// # The envelope reports both sides
///
/// Verified against CAPTURED envelopes rather than against `CliOutput`, which is this
/// crate's own view of the wire and would only ever confirm itself: thirteen of them,
/// twelve successes and one failure, produced by the same
/// `claude --print --output-format json` this provider invokes. The envelope carries a
/// field literally named `stop_reason`, and `usage` carries `output_tokens` alongside
/// `input_tokens`. Both are read, and
/// [`crate::provider::FinishReason::from_wire`] does the translation rather than a
/// second table.
///
/// The captures are tracked, redacted to the fields a requirement consumes, under
/// `src/providers/fixtures/envelopes/`.
#[derive(Debug, Default, Deserialize)]
struct CliUsage {
    #[serde(default)]
    input_tokens: Option<u32>,
    #[serde(default)]
    output_tokens: Option<u32>,
}

/// The two ways writing the prompt can fail, kept apart because collapsing them
/// produced a false statement.
///
/// * `Truncated` -- `write_all` failed, so the child may have seen a partial prompt.
/// * `Flush` -- `write_all` COMPLETED and the teardown failed, so the bytes were
///   delivered and only the pipe's shutdown did not.
///
/// With a bare `io::Error`, a shutdown failure was reported as "prompt write did not
/// complete", which invents a truncation that did not happen -- the same class of lie
/// the labels exist to prevent.
///
/// It deliberately does NOT implement `Display`: the `label_*` helpers take
/// `&dyn Display`, so the call site has to match first and hand over the `io::Error`
/// inside. Passing the enum whole would not compile, which is the point.
#[derive(Debug)]
pub(crate) enum WriteFailure {
    Truncated(std::io::Error),
    Flush(std::io::Error),
}

/// Label put on the envelope's `result` when it travels as a process diagnosis.
///
/// A field named `stderr` carrying something that is not stderr is the defect class
/// this release corrects elsewhere, so it says what it carries. Its content is fixed
/// here rather than at the moment the code is written: two things match it -- the
/// unit test that pins it and the mutation operator that empties it -- and choosing
/// it later leaves both agreeing with whatever the code says.
///
/// `Http.body` is NOT labelled. There the field is called `body` and what it carries
/// IS the body, so a prefix would only spend diagnosis budget.
pub(crate) const ENVELOPE_DIAGNOSIS_PREFIX: &str = "cli envelope: ";

/// Labels the envelope's `result` for the `Process.stderr` path.
pub(crate) fn label_envelope_diagnosis(result: &dyn std::fmt::Display) -> String {
    label_with(ENVELOPE_DIAGNOSIS_PREFIX, result)
}

/// Label put on the write error when the prompt did not reach the child.
///
/// Its content is fixed here rather than at the moment the code is written, because
/// TWO things match it: the unit test that pins it and the mutation operator that
/// empties it. Choosing it later leaves both agreeing with whatever the code says.
pub(crate) const PROMPT_WRITE_DIAGNOSIS_PREFIX: &str = "prompt write did not complete: ";

/// Label put on the error when reaping the child failed.
///
/// A third label rather than reusing the write one, because reusing it asserted a
/// failure that did not happen: reaping a child is not writing a prompt, and a
/// `stderr` reading "prompt write did not complete" over an operating-system
/// failure is the same lie the labels exist to prevent.
pub(crate) const REAP_DIAGNOSIS_PREFIX: &str = "child reap failed: ";

/// The one place that caps and labels; every `label_*` delegates here.
///
/// The cap bounds the FINAL string, label included -- the consumer receives the
/// labelled one, so that is what must fit. Hence the budget passed to
/// `mark_within_cap` is the cap minus the prefix; handing it the whole cap yields
/// `cap + prefix.len()` bytes, which overruns the very constant that exists to
/// bound what reaches an error body.
///
/// It caps only when the text EXCEEDS the budget. Otherwise every unit test that
/// pins a literal would be asserting against a marked string and could never pass.
fn label_with(prefix: &str, text: &dyn std::fmt::Display) -> String {
    let rendered = text.to_string();
    let budget = crate::error::MAX_ERROR_BODY_PREFIX_BYTES.saturating_sub(prefix.len());
    if rendered.len() <= budget {
        // Only caps when the text EXCEEDS. Otherwise every sibling test asserting a
        // literal would be asserting against a marked string and could never pass.
        return format!("{prefix}{rendered}");
    }
    // `saturating_sub` above because a prefix longer than the cap would underflow;
    // `mark_within_cap` already returns the truncated marker in that case.
    format!(
        "{prefix}{}",
        crate::error::mark_within_cap(&rendered, budget)
    )
}

/// Labels the error from a failed prompt write.
pub(crate) fn label_prompt_write_diagnosis(err: &dyn std::fmt::Display) -> String {
    label_with(PROMPT_WRITE_DIAGNOSIS_PREFIX, err)
}

/// Labels the error from a failed child reap.
pub(crate) fn label_reap_diagnosis(err: &dyn std::fmt::Display) -> String {
    label_with(REAP_DIAGNOSIS_PREFIX, err)
}

/// Parses the CLI output envelope into a [`Completion`], carrying whatever
/// telemetry the envelope holds.
///
/// # Parameters
/// * `raw` — the subprocess's raw stdout.
/// * `reasoning` — the caller's [`ReasoningControl`], read only to decide
///   whether to DECLARE that this provider cannot honour
///   [`ReasoningControl::Disabled`] (C-8) — `claude --print` exposes no flag
///   for its reasoning channel at all, and `ClaudeCliProvider` does **not**
///   delegate to [`crate::providers::claude::ClaudeProvider`]: it is an
///   independent struct with its own `model_id`, so omitting this call would
///   leave a provider declaring nothing — the exact silent no-op C-8 exists to
///   stop.
///
/// # Returns
/// A [`Completion`] whose text has its code fences stripped, and whose telemetry
/// carries what the envelope reported: `prompt_tokens` from `usage.input_tokens`,
/// `completion_tokens` from `usage.output_tokens`, and `finish` translated from
/// `stop_reason` by [`crate::provider::FinishReason::from_wire`]. Each stays `None`
/// when its field is absent — that says "this envelope did not report it", never a
/// zero, which would claim a measurement that did not happen.
///
/// # Errors
/// Shares its error path with [`parse_envelope`], which owns the envelope's convention.
fn parse_completion(raw: &str, reasoning: ReasoningControl) -> Result<Completion, ProviderError> {
    let output = parse_envelope(raw)?;
    let text = strip_code_fences(&output.result).to_string();

    let usage = output.usage.unwrap_or_default();

    let mut telemetry = CompletionTelemetry::unmeasured();
    if let Some(n) = usage.input_tokens {
        telemetry = telemetry.with_prompt_tokens(n);
    }
    if let Some(n) = usage.output_tokens {
        telemetry = telemetry.with_completion_tokens(n);
    }
    // `from_wire` and NOT a second table: the vocabulary is published once, in
    // `provider.rs`, and a provider that reimplements it is how two wires come to
    // disagree about the same word.
    if let Some(reason) = output.stop_reason.as_deref() {
        telemetry = telemetry.with_finish(crate::provider::FinishReason::from_wire(reason));
    }
    telemetry = telemetry.with_reasoning(match reasoning {
        ReasoningControl::Disabled => ReasoningState::Unsupported {
            backend: "anthropic-cli".to_string(),
            // This wire exposes no separate reasoning channel at all, so there was nothing to
            // read — which is `None`, not a zero. A zero would say the channel was read and
            // found empty, a stronger claim than anything this provider can make.
            chars: None,
            text: None,
        },
        ReasoningControl::Default => ReasoningState::NotMeasured,
    });

    Ok(Completion::new(text).with_telemetry(telemetry))
}

#[async_trait::async_trait]
impl LlmProvider for ClaudeCliProvider {
    /// Sends a completion request by launching a `claude` subprocess.
    ///
    /// The subprocess is launched directly, without a shell, so there is no
    /// shell injection risk on either channel.
    ///
    /// **The two prompts travel differently, and only one of them escapes the
    /// command-line length limit.** The user prompt goes through stdin, so it
    /// is not subject to it. The system prompt is passed on argv
    /// (`--system-prompt`, built by this provider's private `build_args`), and argv *is* — the exact
    /// limit the user prompt avoids. The three embedded prompts
    /// measure 8344 / 8435 / 9342 bytes (LF line endings); the largest is
    /// about 28.5% of the 32,767-character budget `CreateProcess` allows on
    /// Windows, and well under the roughly 131,072-byte-per-argument limit
    /// Linux enforces via `MAX_ARG_STRLEN`.
    ///
    /// The crate does not impose its own cap on the system prompt, on
    /// purpose: there is no single correct number, since the two platform
    /// limits differ by a factor of about four — a cap sized for Linux would
    /// not protect Windows, and one sized for Windows would reject
    /// legitimate input on Linux. The case this matters for is a custom
    /// prompt loaded through `from_directory`: if it is large enough to
    /// exhaust the remaining argv budget, the failure surfaces as an opaque
    /// process-spawn error, not as a message naming the prompt as too long.
    ///
    /// The timeout is NOT applied here — the orchestrator wraps the
    /// entire agent task in `tokio::time::timeout`.
    ///
    /// **Note:** `config.max_tokens` / `config.temperature` are ignored because
    /// the `claude --print` CLI does not expose those flags — it uses its own
    /// server-side defaults. `config.reasoning` IS read, only to declare
    /// [`crate::provider::ReasoningState::Unsupported`] when the caller asked
    /// to disable it (C-8): the CLI exposes no such flag either, and the
    /// forbidden thing was never carrying on, it was carrying on in silence.
    /// Users who need fine-grained control should use
    /// [`ClaudeProvider`](crate::providers::claude::ClaudeProvider) (HTTP API)
    /// instead.
    async fn complete(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        config: &CompletionConfig,
    ) -> Result<Completion, ProviderError> {
        let args = self.build_args(system_prompt);

        let mut child = Command::new(&self.binary)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| ProviderError::Process {
                exit_code: None,
                // Capped too. An OS spawn error is short in practice, so this one is
                // not the leak -- it is the SIBLING SITE of the one that was, and
                // fixing the site a finding names while leaving its twin is the
                // failure this campaign has now repeated seven times. A guarantee
                // that holds for the producers somebody remembered is not a
                // guarantee.
                stderr: cap_to_error_budget(format!("failed to spawn claude process: {e}")),
            })?;

        // THE FIX. The write and the reap run CONCURRENTLY. `wait_with_output`
        // already drains stdout and stderr -- the defect was never that it did not
        // drain, it was that it ran AFTER the write had completed. With a large
        // prompt and a child that fills its stderr pipe before reading stdin, both
        // sides block and neither can move.
        //
        // Taken BEFORE the join: `wait_with_output()` consumes the child, so the
        // write half cannot be holding a borrow of it.
        let stdin = child.stdin.take();
        let write = async move {
            let Some(mut stdin) = stdin else {
                return Ok(());
            };
            if let Err(e) = stdin.write_all(user_prompt.as_bytes()).await {
                return Err(WriteFailure::Truncated(e));
            }
            let flush = stdin.shutdown().await;
            // EXPLICIT drop, never implicit at the end of the block. The child's EOF
            // comes from this drop, and the whole deadlock hangs off it -- an
            // implicit one is what a refactor moves without noticing.
            drop(stdin);
            flush.map_err(WriteFailure::Flush)
        };

        // POLLED BY HAND, and NOT with `tokio::join!`. That macro lives behind
        // tokio's `macros` feature, which this crate enables only as a
        // dev-dependency -- measured: `cargo tree -e normal` shows no `tokio-macros`
        // in the runtime graph. Turning it on would hand every consumer a new
        // proc-macro crate, and this release adds no dependencies.
        //
        // The semantics are the ones the fix needs and the ones `try_join!` would
        // NOT give: both futures are polled to completion and NEITHER is cancelled
        // when the other resolves. When the child dies mid-write there is a broken
        // pipe AND an exit code waiting, and the precedence below needs both --
        // cancelling on the first error would deliver the pipe error and lose the
        // exit code, which is the datum that wins.
        //
        // No `spawn` either, so there is no detached task to leak and no guard to
        // add: both halves live in this task and are dropped together.
        let mut write = std::pin::pin!(write);
        let mut wait = std::pin::pin!(child.wait_with_output());
        let mut write_done = None;
        let mut wait_done = None;
        let (write_outcome, reaped) = std::future::poll_fn(|cx| {
            use std::future::Future;
            if write_done.is_none()
                && let std::task::Poll::Ready(v) = write.as_mut().poll(cx)
            {
                write_done = Some(v);
            }
            if wait_done.is_none()
                && let std::task::Poll::Ready(v) = wait.as_mut().poll(cx)
            {
                wait_done = Some(v);
            }
            // Destructured rather than unwrapped: `unwrap` is forbidden outside
            // `cfg(test)`, and putting the halves back is what keeps this a plain
            // re-poll instead of a panic waiting for a scheduling order nobody
            // planned.
            match (write_done.take(), wait_done.take()) {
                (Some(w), Some(r)) => std::task::Poll::Ready((w, r)),
                (w, r) => {
                    write_done = w;
                    wait_done = r;
                    std::task::Poll::Pending
                }
            }
        })
        .await;

        // The reap can fail, and that branch exists. `wait_with_output()` returns
        // `io::Result<Output>`: an OS failure, not a model one, so the exit code is
        // unknown and the diagnostic gets its OWN label -- reusing the write one
        // would assert a write failure that did not happen.
        let output = match reaped {
            Ok(output) => output,
            Err(e) => {
                return Err(ProviderError::Process {
                    exit_code: None,
                    stderr: label_reap_diagnosis(&e),
                });
            }
        };

        if !output.status.success() {
            // Row (3): the child died for its own reason, and THAT code is the
            // diagnosis. A broken pipe the parent noticed is secondary and goes to
            // tracing, never into the error type.
            if let Err(WriteFailure::Truncated(e)) = &write_outcome {
                tracing::warn!(
                    error = %e,
                    "the prompt write did not complete, but the child's own exit is the diagnosis"
                );
            }
            // STDOUT FIRST. The discriminator arrives there precisely in the case
            // where this crate used to stop reading stdout, so an envelope that parses
            // classifies the failure; stderr is the FALLBACK, never the other way
            // round. An empty stdout counts as a parse failure -- it is the normal
            // case when the binary did not start, and the real reason is on stderr.
            let stdout = String::from_utf8_lossy(&output.stdout);
            match serde_json::from_str::<CliOutput>(&stdout) {
                // The envelope declares the failure, so it classifies it.
                Ok(envelope) if envelope.is_error => {
                    return Err(classify_envelope_error(envelope, output.status.code()));
                }
                // The two sources CONTRADICT each other: the envelope describes the
                // conversation, the exit code describes the process, and a process
                // that died did not complete whatever its last message says. The
                // process wins, and the envelope's `result` travels as the diagnosis
                // -- LABELLED, because a field named `stderr` has to say when what it
                // carries is not stderr.
                Ok(envelope) => {
                    return Err(ProviderError::Process {
                        exit_code: output.status.code(),
                        stderr: label_envelope_diagnosis(&envelope.result),
                    });
                }
                Err(_) => {
                    // CAPPED like everything else that reaches this field. The child's
                    // stderr is arbitrary and `wait_with_output` reads it to EOF with
                    // no bound of its own -- this milestone's own scenario writes 4 MiB
                    // there -- while the model-authored `result`, which is inherently
                    // modest, was already bounded. Capping two of three producers and
                    // leaving the unbounded one is the wrong two.
                    return Err(ProviderError::Process {
                        exit_code: output.status.code(),
                        stderr: cap_to_error_budget(
                            String::from_utf8_lossy(&output.stderr).to_string(),
                        ),
                    });
                }
            }
        }

        // Exit 0, so the write outcome is consulted BEFORE the parsing path. The
        // match is exhaustive and carries no `_`: a new variant must break the build
        // until somebody decides its consequence.
        match write_outcome {
            Ok(()) => {}
            // Rows (2) and (2b): the write did not complete, so the result is NEVER
            // `Ok`, whatever the exit code. The child may have read part of the
            // prompt and produced something from it, and returning that would be a
            // verdict on mutilated input.
            //
            // `exit_code: None` and not `Some(0)`: the process succeeded and the
            // WRITE failed, so a `Some(0)` would be a failure variant declaring
            // success.
            Err(WriteFailure::Truncated(e)) => {
                return Err(ProviderError::Process {
                    exit_code: None,
                    stderr: label_prompt_write_diagnosis(&e),
                });
            }
            // The bytes were delivered; only the teardown failed. Reporting that as a
            // truncation invents one.
            Err(WriteFailure::Flush(e)) => {
                tracing::warn!(error = %e, "the prompt was delivered; closing its pipe failed");
            }
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_completion(&stdout, config.reasoning)
    }

    fn name(&self) -> &str {
        ClaudeCliProvider::name(self)
    }

    fn model(&self) -> &str {
        ClaudeCliProvider::model(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::MAX_ERROR_BODY_PREFIX_BYTES;
    use crate::provider::FinishReason;
    use crate::provider::is_retryable;
    use crate::test_support::{
        CAPTURED_404, FAILURE_KEEP_LIST, SUCCESS_KEEP_LIST, captured_envelope_with_stop_reason,
        captured_failure_with_api_error_status, captured_failure_with_oversized_result,
        captured_failure_with_unusable_api_error_status, captured_failure_without_api_error_status,
        envelope_with_is_error_false_and_status, result_of,
    };
    use crate::test_support::{with_claudecode, without_claudecode};
    use serial_test::serial;

    #[test]
    fn the_cli_status_is_classified_like_the_http_path() {
        // E-3. Synthetic is ONLY the field the scenario varies; the shape stays
        // measured.
        let out = captured_failure_with_api_error_status(429);
        match parse_envelope(&out) {
            // ALL FOUR FIELDS, not just the status: the conversion decides every one
            // of them, and with `..` three quarters would go unpinned. `body` matters
            // most -- it is the only diagnosis the CLI gives.
            Err(ProviderError::Http {
                status,
                body,
                retry_after_raw,
                received_at,
            }) => {
                assert_eq!(status, 429);
                // Without this, an `assert_eq!` between two empty strings would pass,
                // and losing the `result` is precisely the regression.
                assert!(!body.is_empty(), "the body cannot arrive empty");
                // SHORT case: a `result` under the cap travels whole and RAW.
                // `Http.body` is NOT labelled -- the label lives on the
                // `Process.stderr` path, where a field named `stderr` has to say that
                // what it carries is not stderr. Here the field is called `body` and
                // what it carries IS the body.
                assert_eq!(
                    body,
                    result_of(&out),
                    "a `result` under the cap travels intact and raw"
                );
                assert!(body.len() <= MAX_ERROR_BODY_PREFIX_BYTES);
                assert!(
                    retry_after_raw.is_empty(),
                    "the envelope carries no equivalent header"
                );
                assert!(received_at.is_none(), "with no header there is no instant");
                assert!(is_retryable(&ProviderError::Http {
                    status,
                    body: String::new(),
                    retry_after_raw: vec![],
                    received_at: None,
                }));
            }
            other => panic!("expected Http {{ status: 429 }}, got {other:?}"),
        }
        // 404 travels the same path and is NOT retryable -- one table, both providers.
        let out_404 = captured_failure_with_api_error_status(404);
        assert!(!is_retryable(&parse_envelope(&out_404).unwrap_err()));
        // Edge (a): a status on an envelope that does not claim failure is not a
        // failure -- `is_error` governs and the status is not read.
        assert!(parse_envelope(&envelope_with_is_error_false_and_status()).is_ok());
        // A LOCAL CLI failure never reached the API, so Process is the right answer.
        assert!(matches!(
            parse_envelope(&captured_failure_without_api_error_status()),
            Err(ProviderError::Process { .. })
        ));
        // PRESENT BUT UNUSABLE is a THIRD case and not the same as absent. BOTH
        // shapes, because `null` is valid JSON for the field and a string is not, so a
        // parser can treat them differently without anyone noticing.
        for unusable in ["\"429\"", "null"] {
            assert!(
                matches!(
                    parse_envelope(&captured_failure_with_unusable_api_error_status(unusable)),
                    Err(ProviderError::Process { .. })
                ),
                "an unreadable status must not become an Http: {unusable}"
            );
        }
    }

    #[test]
    fn an_envelope_without_is_error_is_a_failure_not_a_success() {
        // Pins an invariant that is an ABSENCE: `is_error` must NOT carry
        // `#[serde(default)]`. With the attribute, an envelope that never said whether
        // it failed would deserialize as a SUCCESS -- the safe direction is the other
        // one, because a missing field is a wire this crate does not understand.
        //
        // Mutation: adding `#[serde(default)]` to `is_error` must turn this red.
        let without = r#"{"result":"something happened"}"#;
        assert!(
            serde_json::from_str::<CliOutput>(without).is_err(),
            "an envelope that does not say whether it failed must not parse as success"
        );
    }

    #[test]
    fn an_out_of_range_api_error_status_is_not_an_http_error() {
        // THE BOUNDARIES, not a far-away value. A `99999` passes with `100..=599`,
        // with `99..=600` and with `0..=99999`, so it cannot detect an off-by-one --
        // the same vacuity the oversized-result test corrects next door.
        //
        // The range is a decision of this plan rather than of the spec, and it was
        // escalated and kept: `Http.status` is `u16` and GOVERNS lineage condemnation,
        // so putting a negative or a `99999` in there is not "an odd status" -- it is
        // fabricating one no server returned and letting it decide which lineage is
        // condemned.
        //
        // Declared consequence: a real status outside `100..=599`, if one ever
        // existed, would fall to `Process`. That is the safe direction -- `Process` is
        // mage-local and condemns no lineage, so the error is paid in diagnosis.
        //
        // Mutation: widening to `99..=600` must redden TWO rows and leave the rest
        // green. If only one falls, the test is not covering both ends.
        for (value, expect_http) in [
            (99i64, false),
            (100, true),
            (599, true),
            (600, false),
            (-1, false),
            (99999, false),
        ] {
            let raw = captured_failure_with_api_error_status(value);
            let err = parse_envelope(&raw).expect_err("is_error is true, so this fails");
            if expect_http {
                match err {
                    ProviderError::Http { status, .. } => assert_eq!(
                        i64::from(status),
                        value,
                        "an in-range status must arrive unchanged"
                    ),
                    other => panic!("{value} is a usable status; got {other:?}"),
                }
            } else {
                assert!(
                    matches!(err, ProviderError::Process { .. }),
                    "{value} cannot be an HTTP status, so it must stay Process"
                );
            }
        }
    }

    #[test]
    fn an_oversized_envelope_result_is_capped_and_says_so() {
        // The cap assertion is VACUOUS against the real fixture: its `result` is two
        // lines, so `len() <= 8 KiB` holds with the truncation deleted. Only a
        // `result` that EXCEEDS the cap tells a working cap from an absent one.
        //
        // Mutation: removing the truncation from the CLI path must redden THIS test
        // and leave the short case green. If the short case falls too, it was not
        // isolated.
        let raw = captured_failure_with_oversized_result(16 * 1024);
        let ProviderError::Http { body, .. } =
            parse_envelope(&raw).expect_err("is_error is true, so this fails")
        else {
            panic!("an envelope with a usable status becomes Http");
        };
        assert!(
            body.len() <= MAX_ERROR_BODY_PREFIX_BYTES,
            "the body must fit the cap: {} > {}",
            body.len(),
            MAX_ERROR_BODY_PREFIX_BYTES
        );
        assert!(
            body.ends_with(crate::error::TRUNCATION_MARKER),
            "a capped body says so"
        );
        let original = result_of(&raw);
        let kept = body
            .strip_suffix(crate::error::TRUNCATION_MARKER)
            .expect("the marker was just asserted");
        assert!(
            original.starts_with(kept),
            "what precedes the marker is a prefix of the original result"
        );
    }

    #[test]
    fn a_non_json_stdout_is_a_process_failure() {
        // One of the two shapes the capture spike provoked. Each captured shape needs
        // a consumer: a capture without one is wasted work and, worse, reads as
        // coverage.
        //
        // SEPARATE from its sibling below rather than a loop over both, and the reason
        // is the one this milestone learned twice: a loop aborts on the first failing
        // input, so the second shape would go unasserted exactly when the first
        // regresses -- which is the moment the second matters most.
        assert!(
            matches!(
                parse_envelope("just some plain text, not JSON at all"),
                Err(ProviderError::Process { .. })
            ),
            "output that is not JSON at all must fall to Process"
        );
    }

    #[test]
    fn a_json_object_without_the_result_field_is_a_process_failure() {
        // The other captured shape, from the same fixture directory as the envelopes
        // that DO parse. Same branch, different input -- a parser can treat the two
        // differently without anyone noticing, which is why they are asserted apart.
        assert!(
            matches!(
                parse_envelope(include_str!("fixtures/envelopes/empty_object.json")),
                Err(ProviderError::Process { .. })
            ),
            "valid JSON that is not an envelope must fall to Process"
        );
    }

    #[test]
    fn the_in_band_diagnosis_is_labelled_and_capped_like_its_sibling() {
        // The IN-BAND failure path -- an envelope that declares a failure with no
        // usable status -- is the dominant one: every exit-0 `is_error: true`
        // envelope without a status reaches it. It used to hand the envelope's
        // `result` to `Process.stderr` RAW and UNBOUNDED, while the exit != 0 path
        // labelled the same content and the sibling arm of the same `match` capped
        // it.
        //
        // Both halves matter and each has its own reason:
        //
        // * LABELLED, because `ProviderError::Process.stderr` now publishes -- in
        //   PUBLIC rustdoc, on docs.rs -- that this provider labels whatever is not
        //   really stderr. Leaving the dominant path unlabelled made that sentence
        //   false the day it shipped.
        // * CAPPED, because this error reaches `failed_agents` and the serialized
        //   report, and the cap was moved into `error.rs` by this very milestone so a
        //   `ProviderError` could bound what it carries.
        //
        // The existing tests on this path assert only the VARIANT, so none of them
        // would have noticed either.
        let raw = captured_failure_without_api_error_status();
        let ProviderError::Process { stderr, .. } =
            parse_envelope(&raw).expect_err("is_error is true, so this fails")
        else {
            panic!("an envelope with no usable status stays a Process failure");
        };
        assert!(
            stderr.starts_with("cli envelope: "),
            "the dominant in-band path must label too, or the published contract of \
             `Process.stderr` is false: {stderr}"
        );
        assert!(
            stderr.contains(result_of(&raw).as_str()),
            "the envelope's own diagnosis has to survive the labelling"
        );

        // And the cap, asserted with a `result` that EXCEEDS it -- against the real
        // fixture the length check holds with the cap deleted.
        //
        // The status is dropped HERE rather than by a second helper: the oversized
        // capture keeps its `api_error_status`, so it reaches the `Http` arm and not
        // this one. Composing the two conditions locally keeps the helper honest for
        // its own caller and adds no surface.
        let oversized = {
            let mut v: serde_json::Value =
                serde_json::from_str(&captured_failure_with_oversized_result(16 * 1024))
                    .expect("the oversized fixture parses");
            v.as_object_mut()
                .expect("the fixture is an object")
                .remove("api_error_status");
            v.to_string()
        };
        let ProviderError::Process { stderr, .. } =
            parse_envelope(&oversized).expect_err("is_error is true, so this fails")
        else {
            panic!("an envelope with no usable status stays a Process failure");
        };
        assert!(
            stderr.len() <= MAX_ERROR_BODY_PREFIX_BYTES,
            "the labelled diagnosis must fit the cap: {} > {}",
            stderr.len(),
            MAX_ERROR_BODY_PREFIX_BYTES
        );
        // AND that it says so. A cap that truncated without marking would satisfy the
        // length bound above and hand the consumer a silently shortened diagnosis --
        // which is the sibling assertion this test was missing.
        assert!(
            stderr.ends_with(crate::error::TRUNCATION_MARKER),
            "a capped diagnosis has to announce the cut: {stderr}"
        );
    }

    #[test]
    fn the_diagnosis_prefix_labels_the_envelope_result() {
        // THE LITERAL, never `starts_with(ENVELOPE_DIAGNOSIS_PREFIX)`: with the
        // constant on both sides, emptying it moves both and the test stays green over
        // a label that vanished.
        assert_eq!(
            label_envelope_diagnosis(&"boom"),
            "cli envelope: boom",
            "a field named `stderr` must say when what it carries is not stderr"
        );
    }

    #[test]
    fn the_ms1_surface_this_milestone_consumes_is_reachable() {
        // COMPILATION probe, not a behaviour test: what it verifies is that these two
        // items EXIST with the visibility this milestone needs, from THIS feature set.
        // A grep finds the declaration and cannot see the `#[cfg]` that leaves it out
        // -- which is exactly the trap this milestone hit with the error-body cap, so
        // repeating it inside the guard written to avoid it would be the whole joke.
        //
        // Under the stacked branch topology it is guaranteed by construction, so this
        // is the last-resort probe for someone branching from elsewhere. The
        // assertions are deliberately trivial: one that also asserted behaviour would
        // go red for a second reason and stop being a probe.
        assert!(!crate::provider::is_retryable(
            &ProviderError::NestedSession
        ));
        assert!(crate::error::mark_within_cap("", 0).is_empty());

        // The same probe over the API SHAPES the tests assume: `FinishReason` compared
        // with `assert_eq!` needs `PartialEq + Debug`, and the two telemetry fields are
        // read through `Completion.telemetry`, never `Completion.finish` -- `Completion`
        // has exactly `text` and `telemetry`. Discovering either by compiling the Red
        // sends the executor hunting for a typo instead of a missing derive.
        assert_eq!(FinishReason::from_wire("end_turn"), FinishReason::Stop);
        let probe = parse_completion(
            r#"{"is_error":false,"result":"hi"}"#,
            ReasoningControl::default(),
        )
        .expect("a minimal envelope completes");
        assert!(probe.telemetry.finish.is_none());
        assert!(probe.telemetry.completion_tokens.is_none());
    }

    #[test]
    fn the_reap_prefix_labels_its_error() {
        // THE LITERAL, never `starts_with(REAP_DIAGNOSIS_PREFIX)`. With the constant
        // on both sides, emptying it moves both and the test stays GREEN over a
        // label that vanished -- the vacuity 3.1.0 already paid for once. The
        // mutation operator is still "empty the constant"; what changes is that it
        // can now go red.
        assert_eq!(
            label_reap_diagnosis(&"boom"),
            "child reap failed: boom",
            "the reap label is its own, because reaping a child is not writing a prompt"
        );
    }

    #[test]
    fn the_prompt_write_prefix_labels_its_error() {
        // Its OWN test and not its sibling's: a shared assertion goes green on the
        // other constant and says nothing about this one.
        assert_eq!(
            label_prompt_write_diagnosis(&"boom"),
            "prompt write did not complete: boom",
            "the write label states what failed, so the field does not lie about what it carries"
        );
    }

    #[test]
    fn label_with_caps_only_when_the_text_exceeds() {
        use crate::error::{MAX_ERROR_BODY_PREFIX_BYTES, TRUNCATION_MARKER};

        // Half of the contract: a text that fits is NOT marked. Without this, every
        // sibling test asserting a literal would be asserting against a marked
        // string and could never pass.
        assert_eq!(label_with("p: ", &"short"), "p: short");

        // The other half, and the one the mutation targets: the cap bounds the
        // LABELLED result, so the budget handed to `mark_within_cap` is the cap
        // MINUS the prefix. Passing the whole cap produces `cap + prefix.len()`
        // bytes -- an overrun of the constant that exists to bound what reaches an
        // error body, and invisible unless the length is asserted.
        let prefix = "p: ";
        let long = "x".repeat(MAX_ERROR_BODY_PREFIX_BYTES * 2);
        let labelled = label_with(prefix, &long);
        assert!(
            labelled.len() <= MAX_ERROR_BODY_PREFIX_BYTES,
            "the labelled result must fit the cap, not the cap plus the prefix: {} > {}",
            labelled.len(),
            MAX_ERROR_BODY_PREFIX_BYTES
        );
        assert!(labelled.starts_with(prefix), "the prefix survives the cap");
        assert!(
            labelled.ends_with(TRUNCATION_MARKER),
            "a capped text says so"
        );
    }

    /// A prefix LONGER than the cap must not smuggle itself past it.
    ///
    /// Unreachable with the three shipped prefixes (15-31 bytes against 8192), and
    /// pinned anyway because the comment that used to justify the gap was FALSE
    /// about the code beside it: it said `mark_within_cap` "already returns the
    /// truncated marker in that case", and with a budget of zero that function
    /// returns the EMPTY string -- after which the whole prefix is concatenated in
    /// front regardless. BOTH branches returned the prefix entire, so the function
    /// broke the contract its own rustdoc states: the cap bounds the FINAL string.
    #[test]
    fn a_prefix_longer_than_the_cap_is_itself_capped() {
        use crate::error::MAX_ERROR_BODY_PREFIX_BYTES;

        let oversized = "p".repeat(MAX_ERROR_BODY_PREFIX_BYTES + 100);

        // BOTH branches of the function, because they reach the overflow by
        // different routes: an empty text takes the early return, a non-empty one
        // goes through `mark_within_cap` with a zero budget. The first is the one
        // the finding named; testing only it would leave the sibling open, which is
        // this project's most-repeated defect.
        for text in ["", "some text"] {
            let labelled = label_with(&oversized, &text);
            assert!(
                labelled.len() <= MAX_ERROR_BODY_PREFIX_BYTES,
                "text {text:?}: the labelled result must fit the cap: {} > {}",
                labelled.len(),
                MAX_ERROR_BODY_PREFIX_BYTES
            );
        }
    }

    #[test]
    #[serial]
    fn the_cannot_test_prefix_is_emitted_only_outside_ci() {
        use crate::test_support::cannot_test;

        // The self-test of the adjudication path. It asserts the message FORMAT in
        // both directions, which is what a closing report adjudicates on and what a
        // rewording would break in silence.
        //
        // What it CANNOT prove is that the panic happens at all -- that is decided
        // by the precondition probe on a real platform. It proves the format.
        fn message_with(magi_ci: Option<&str>) -> String {
            let original = std::env::var("MAGI_CI").ok();
            unsafe {
                match magi_ci {
                    Some(v) => std::env::set_var("MAGI_CI", v),
                    None => std::env::remove_var("MAGI_CI"),
                }
            }
            let caught = std::panic::catch_unwind(|| cannot_test("the reason"));
            unsafe {
                match original {
                    Some(v) => std::env::set_var("MAGI_CI", v),
                    None => std::env::remove_var("MAGI_CI"),
                }
            }
            let payload = caught.expect_err("cannot_test always panics");
            payload
                .downcast_ref::<String>()
                .cloned()
                .unwrap_or_else(|| "<not a string payload>".to_string())
        }

        let local = message_with(None);
        assert!(
            local.starts_with("CANNOT_TEST: "),
            "off a runner the prefix is what makes the skip adjudicable: {local}"
        );
        assert!(
            !local.contains("prefix suppressed"),
            "nothing was suppressed here: {local}"
        );

        let on_ci = message_with(Some("1"));
        assert!(
            !on_ci.starts_with("CANNOT_TEST: "),
            "on a runner there must be nothing to adjudicate: {on_ci}"
        );
        assert!(
            on_ci.contains("[prefix suppressed: MAGI_CI is set]"),
            "a suppression nobody can see is worse than none: {on_ci}"
        );

        // An EMPTY variable is not "set". Without this, a machine exporting it blank
        // would close the adjudication path with nobody deciding so.
        let blank = message_with(Some(""));
        assert!(
            blank.starts_with("CANNOT_TEST: "),
            "an empty MAGI_CI must not count as set: {blank}"
        );
    }

    #[test]
    fn the_cli_envelope_reports_what_the_backend_said() {
        // The fixture is CAPTURED and redacted, never hand-written: a hand-written one
        // reproduces the view the crate already has, which is how this defect survived.
        // `parse_completion` takes the RAW string and parses it itself -- it does NOT
        // take a `CliOutput`.
        let raw = include_str!("fixtures/envelopes/success_end_turn.json");

        let completion = parse_completion(raw, ReasoningControl::default())
            .expect("a successful envelope completes");
        // `.telemetry.finish`, NOT `.finish`: `Completion` has exactly two fields --
        // `text` and `telemetry` -- and `finish` lives on `CompletionTelemetry`.
        assert!(
            completion.telemetry.finish.is_some(),
            "the backend DID say why it stopped"
        );
        assert!(
            completion.telemetry.completion_tokens.is_some(),
            "and how much it wrote"
        );
    }

    #[test]
    fn the_failure_envelope_deserializes_its_api_status() {
        // The field is ADDED in this task -- R-3 consumes it in the next one -- so its
        // deserialization is asserted HERE. Without this, the task ships a struct field
        // whose only verification lives in another commit.
        let raw = include_str!("fixtures/envelopes/failure_404.json");
        let out: CliOutput = serde_json::from_str(raw).expect("the captured envelope parses");
        assert_eq!(out.api_error_status, ApiErrorStatus::Value(404));
    }

    #[test]
    fn an_absent_api_status_is_distinguishable_from_an_unreadable_one() {
        // THE WHOLE POINT OF THE THREE-STATE TYPE. With `Option<i64>` both cases were
        // `None`, so the `warn!` R-3 adds promised a distinction the type could not
        // make: "there was no status" and "there was something I could not read" are a
        // local CLI failure and a wire-format regression, and they must not share a
        // value.
        //
        // What is asserted is the DISCRIMINATOR, not the log line. What stays uncovered
        // is someone deleting the `warn!` and leaving the arm -- declared, not
        // simulated.
        let absent: CliOutput =
            serde_json::from_str(&captured_failure_without_api_error_status()).unwrap();
        assert_eq!(
            absent.api_error_status,
            ApiErrorStatus::Absent,
            "absent must stay absent"
        );
        // EVERY JSON shape that is not an integer, not two of them. The fail-soft
        // deserializer's whole job is to never return `Err`, so its risk is a shape
        // nobody tried. The last one is the OVERFLOW case, and it is the one nobody
        // writes: an integer that is a perfectly good JSON number and does not fit
        // `i64`. It is handled correctly BY CONSTRUCTION -- serde refuses it and the
        // `deserialize_with` catches the refusal -- but "correct by construction" is
        // exactly the claim this array exists to stop trusting.
        for unusable in [
            "\"429\"",
            "null",
            "[]",
            "{}",
            "true",
            "4.29",
            "99999999999999999999",
        ] {
            let out: CliOutput =
                serde_json::from_str(&captured_failure_with_unusable_api_error_status(unusable))
                    .expect("a weird field must NOT kill the envelope");
            assert_eq!(
                out.api_error_status,
                ApiErrorStatus::Unreadable,
                "present but unreadable: {unusable}"
            );
        }
    }

    #[test]
    fn an_unknown_stop_reason_is_not_mapped_to_stop() {
        // `end_turn` is the only value captured. The rest of the vocabulary is NOT
        // invented: what is not recognised falls in the variant that says so, never in
        // Stop by default -- inventing Stop is exactly what 4.0.0 rejected.
        let raw = captured_envelope_with_stop_reason("some_future_reason");
        let completion = parse_completion(&raw, ReasoningControl::default()).unwrap();
        // THE EXACT VARIANT, not `!= Stop`. A negative assertion passes ALSO when
        // `finish` is None -- which is a DIFFERENT bug: it would mean the backend said
        // why it stopped and we dropped it. `Other` keeps the raw string, so a value
        // measured later can be promoted without losing what arrived.
        assert_eq!(
            completion.telemetry.finish,
            Some(FinishReason::Other("some_future_reason".to_string())),
            "an unrecognised stop_reason keeps its raw value; it is not Stop and not None"
        );
    }

    #[test]
    fn the_captured_envelope_keeps_exactly_the_consumed_fields() {
        // TWO fixtures, because the consumed set DIFFERS by envelope type: the success
        // one carries `stop_reason` and `usage.output_tokens`; the failure one carries
        // `api_error_status` and neither of those.
        let cases: &[(&str, &[&str])] = &[
            (
                include_str!("fixtures/envelopes/success_end_turn.json"),
                &SUCCESS_KEEP_LIST,
            ),
            (CAPTURED_404, &FAILURE_KEEP_LIST),
        ];

        for (raw, expected) in cases {
            let v: serde_json::Value = serde_json::from_str(raw).unwrap();
            let obj = v.as_object().unwrap();

            // FOUR and THREE are different numbers and both are right: the raw failure
            // capture carries four fields exclusive to a failure and only ONE of them
            // is consumed by a REQ, so after redaction its expected list has three.
            //
            // EXACT count: one key too many is as much a defect as one too few.
            assert_eq!(obj.len(), expected.len(), "fixture keys: {:?}", obj.keys());
            for key in *expected {
                assert!(obj.contains_key(*key), "consumed field {key} missing");
            }

            // NESTED, not just the root: a forbidden field under `usage` survives a
            // root-level check -- which is exactly what R-35 is for.
            if let Some(usage) = obj.get("usage").and_then(|u| u.as_object()) {
                assert_eq!(usage.len(), 1, "usage keeps only output_tokens");
                assert!(usage.contains_key("output_tokens"));
            }
            for banned in ["session_id", "uuid", "total_cost_usd", "inference_geo"] {
                assert!(!raw.contains(banned), "{banned} must have been redacted");
            }
        }
    }

    // NOTE: These tests manipulate the CLAUDECODE environment variable, which is
    // process-global state. The #[serial] attribute ensures they never run in
    // parallel, making them safe under both `cargo nextest` and `cargo test`.

    // -- BDD Scenario 23: detects nested session --

    /// CLAUDECODE env var present returns Err(ProviderError::NestedSession) in constructor.
    #[test]
    #[serial]
    fn test_new_with_claudecode_env_returns_nested_session_error() {
        with_claudecode(|| {
            let result = ClaudeCliProvider::new("sonnet");
            assert!(result.is_err());
            let err = result.unwrap_err();
            assert!(
                matches!(err, ProviderError::NestedSession),
                "expected NestedSession, got: {err}"
            );
        });
    }

    // -- Model alias resolution --

    /// new("sonnet") maps to "claude-sonnet-4-6".
    #[test]
    #[serial]
    fn test_new_sonnet_maps_to_claude_sonnet_model() {
        without_claudecode(|| {
            let provider = ClaudeCliProvider::new("sonnet").unwrap();
            assert_eq!(provider.model(), "claude-sonnet-4-6");
        });
    }

    /// new("opus") maps to "claude-opus-4-7".
    #[test]
    #[serial]
    fn test_new_opus_maps_to_claude_opus_model() {
        without_claudecode(|| {
            let provider = ClaudeCliProvider::new("opus").unwrap();
            assert_eq!(provider.model(), "claude-opus-4-7");
        });
    }

    /// new("haiku") maps to "claude-haiku-4-5-20251001".
    #[test]
    #[serial]
    fn test_new_haiku_maps_to_claude_haiku_model() {
        without_claudecode(|| {
            let provider = ClaudeCliProvider::new("haiku").unwrap();
            assert_eq!(provider.model(), "claude-haiku-4-5-20251001");
        });
    }

    /// new("claude-custom-model") passes through (contains "claude-").
    #[test]
    #[serial]
    fn test_new_claude_prefix_passes_through() {
        without_claudecode(|| {
            let provider = ClaudeCliProvider::new("claude-custom-model").unwrap();
            assert_eq!(provider.model(), "claude-custom-model");
        });
    }

    /// new("invalid") returns ProviderError::Auth.
    #[test]
    #[serial]
    fn test_new_invalid_model_returns_auth_error() {
        without_claudecode(|| {
            let result = ClaudeCliProvider::new("invalid");
            assert!(result.is_err());
            let err = result.unwrap_err();
            assert!(
                matches!(err, ProviderError::Auth { .. }),
                "expected Auth, got: {err}"
            );
        });
    }

    /// Mixed case aliases (Sonnet, SONNET) rejected with ProviderError::Auth.
    #[test]
    #[serial]
    fn test_new_mixed_case_alias_returns_auth_error() {
        without_claudecode(|| {
            let result_upper = ClaudeCliProvider::new("Sonnet");
            let result_all_caps = ClaudeCliProvider::new("SONNET");
            assert!(
                matches!(result_upper.unwrap_err(), ProviderError::Auth { .. }),
                "expected Auth for 'Sonnet'"
            );
            assert!(
                matches!(result_all_caps.unwrap_err(), ProviderError::Auth { .. }),
                "expected Auth for 'SONNET'"
            );
        });
    }

    // -- Accessors --

    /// provider.name() returns "claude-cli".
    #[test]
    #[serial]
    fn test_provider_name_returns_claude_cli() {
        without_claudecode(|| {
            let provider = ClaudeCliProvider::new("sonnet").unwrap();
            assert_eq!(provider.name(), "claude-cli");
        });
    }

    /// provider.model() returns the resolved model_id.
    #[test]
    #[serial]
    fn test_provider_model_returns_resolved_model_id() {
        without_claudecode(|| {
            let provider = ClaudeCliProvider::new("opus").unwrap();
            assert_eq!(provider.model(), "claude-opus-4-7");
        });
    }

    // -- BDD Scenario 18: command arguments --

    /// build_args includes --print, --output-format json, --model, --system-prompt.
    #[test]
    #[serial]
    fn test_build_args_includes_required_cli_flags() {
        without_claudecode(|| {
            let provider = ClaudeCliProvider::new("sonnet").unwrap();
            let args = provider.build_args("You are an analyst.");

            assert!(args.contains(&"--print".to_string()));
            assert!(args.contains(&"--output-format".to_string()));
            assert!(args.contains(&"json".to_string()));
            assert!(args.contains(&"--model".to_string()));
            assert!(args.contains(&"claude-sonnet-4-6".to_string()));
            assert!(args.contains(&"--system-prompt".to_string()));
            assert!(args.contains(&"You are an analyst.".to_string()));
        });
    }

    /// User prompt is NOT in build_args (sent via stdin).
    #[test]
    #[serial]
    fn test_user_prompt_not_in_build_args() {
        without_claudecode(|| {
            let provider = ClaudeCliProvider::new("sonnet").unwrap();
            let user_prompt = "Analyze this code for security issues";
            let args = provider.build_args("System prompt");

            assert!(
                !args.contains(&user_prompt.to_string()),
                "user prompt should not be in CLI args"
            );
        });
    }

    // -- BDD Scenario 19: parses double-nested JSON --

    /// The envelope parser extracts the inner result from `{"is_error": false, "result": ...}`.
    #[test]
    fn test_parse_cli_output_extracts_inner_result() {
        let outer = r#"{"type":"result","subtype":"success","is_error":false,"result":"{\"agent\":\"melchior\",\"verdict\":\"approve\"}","usage":{"input_tokens":100}}"#;
        let result = parse_envelope(outer).unwrap().result;
        assert_eq!(result, r#"{"agent":"melchior","verdict":"approve"}"#);
    }

    // -- BDD Scenario 20: detects error in CLI response --

    /// A failing envelope with no usable status is a `Process` failure.
    ///
    /// The hand-written fixture this used to carry paired `is_error: true` with
    /// `subtype: "error"`, a combination the real CLI does NOT produce -- measured
    /// against captured envelopes, where a genuine failure carries
    /// `subtype: "success"`. An invented fixture confirms the view the crate already
    /// has instead of contrasting it, which is how the whole class of defect this
    /// milestone corrects survived.
    ///
    /// It now starts from the captured failure with its `api_error_status` removed,
    /// which is the same condition by a shape that was measured.
    #[test]
    fn test_parse_cli_output_error_flag_returns_process_error() {
        let result = parse_envelope(&captured_failure_without_api_error_status());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ProviderError::Process { .. }),
            "expected Process, got: {err}"
        );
    }

    /// Malformed JSON returns ProviderError::Process.
    #[test]
    fn test_parse_cli_output_malformed_json_returns_process_error() {
        let result = parse_envelope("not valid json");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ProviderError::Process { .. }),
            "expected Process, got: {err}"
        );
    }

    // -- Task 11: telemetry the CLI provider CAN report --

    /// The envelope in this test carries `usage.input_tokens` and no `stop_reason`,
    /// so `finish` stays `None` — which says "this envelope did not report it",
    /// never a `Stop` nobody sent.
    ///
    /// This doc block is the THIRD site that claimed the wire has no `stop_reason`
    /// "verified against the envelope shape", and the third that had been checked
    /// against `CliOutput` rather than against an envelope. The requirement names
    /// two; this one is their sibling, found by grepping for the word rather than
    /// by trusting the list. Fixing the sites a finding names and leaving their
    /// siblings is the recurring shape this release exists to correct.
    #[test]
    fn an_envelope_without_a_stop_reason_leaves_finish_none() {
        // The assertion is unchanged and still true; its NAME and its message were
        // not. They said "the CLI envelope has no stop_reason", which is a claim
        // about the wire, and the wire does carry one -- measured against captured
        // envelopes. What is true is narrower and is what this test now says: THIS
        // envelope does not carry the field, so `finish` stays `None`.
        let raw = r#"{"is_error":false,"result":"hi","usage":{"input_tokens":100}}"#;
        let out = super::parse_completion(raw, ReasoningControl::default())
            .expect("valid envelope parses");
        assert_eq!(out.telemetry.prompt_tokens, Some(100));
        assert_eq!(
            out.telemetry.finish, None,
            "this envelope carries no stop_reason, so there is nothing to translate"
        );
    }

    /// Task 11 / C-8: `ClaudeCliProvider` is an INDEPENDENT struct — it does not
    /// delegate to `ClaudeProvider` — so omitting it would leave a provider
    /// declaring nothing, exactly the silent no-op this task exists to stop.
    /// The `claude --print` CLI exposes no flag for its reasoning channel at
    /// all, so `Disabled` is DECLARED here too.
    #[test]
    fn the_cli_provider_declares_unsupported_when_the_caller_disables_reasoning() {
        use crate::provider::ReasoningState;

        let raw = r#"{"is_error":false,"result":"hi"}"#;
        let out = super::parse_completion(raw, ReasoningControl::Disabled)
            .expect("valid envelope parses");
        assert!(
            matches!(
                out.telemetry.reasoning,
                ReasoningState::Unsupported { ref backend, .. } if backend == "anthropic-cli"
            ),
            "expected Unsupported{{backend: \"anthropic-cli\"}}, got {:?}",
            out.telemetry.reasoning
        );
    }

    /// The other half of C-8: nothing asked, nothing declared.
    #[test]
    fn the_cli_provider_declares_nothing_when_reasoning_control_is_default() {
        use crate::provider::ReasoningState;

        let raw = r#"{"is_error":false,"result":"hi"}"#;
        let out =
            super::parse_completion(raw, ReasoningControl::Default).expect("valid envelope parses");
        assert_eq!(out.telemetry.reasoning, ReasoningState::NotMeasured);
    }

    // -- BDD Scenario 21: strips code fences --

    /// extract_json removes ```json ... ``` wrapping.
    #[test]
    fn test_strip_code_fences_removes_json_fence() {
        let input = "```json\n{\"key\": \"value\"}\n```";
        let result = strip_code_fences(input);
        assert_eq!(result, "{\"key\": \"value\"}");
    }

    /// extract_json removes plain ``` ... ``` wrapping.
    #[test]
    fn test_strip_code_fences_removes_plain_fence() {
        let input = "```\n{\"key\": \"value\"}\n```";
        let result = strip_code_fences(input);
        assert_eq!(result, "{\"key\": \"value\"}");
    }

    /// No fences returns text unchanged.
    #[test]
    fn test_strip_code_fences_no_fences_returns_unchanged() {
        let input = r#"{"key": "value"}"#;
        let result = strip_code_fences(input);
        assert_eq!(result, input);
    }

    /// The producer that is fed by text this crate did not author.
    ///
    /// `serde_json` embeds the offending value VERBATIM in its message, and the
    /// value here is the child's stdout -- arbitrary bytes with no bound of their
    /// own. So this path, and not the spawn one beside it, is what decides whether
    /// `Process.stderr`'s published guarantee is true. Measured before the fix:
    /// a 100 KB value produced a 100,095-byte diagnosis.
    #[test]
    fn a_parse_failure_does_not_carry_the_whole_offending_value() {
        let oversized = "A".repeat(MAX_ERROR_BODY_PREFIX_BYTES * 4);
        let raw = format!(r#"{{"is_error": "{oversized}", "result": "x"}}"#);

        let err = parse_envelope(&raw).expect_err("a string where a bool belongs cannot parse");
        let ProviderError::Process { stderr, .. } = err else {
            panic!("a parse failure is a Process error, got {err:?}");
        };
        assert!(
            stderr.len() <= MAX_ERROR_BODY_PREFIX_BYTES,
            "the parse diagnosis carries the offending value; it must fit the cap: {} > {}",
            stderr.len(),
            MAX_ERROR_BODY_PREFIX_BYTES
        );
        assert!(
            stderr.ends_with(crate::error::TRUNCATION_MARKER),
            "a capped diagnosis has to announce the cut: {stderr}"
        );
    }
}
