// Author: Julian Bolivar
// Version: 4.0.0
// Date: 2026-08-23

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
    /// Test-only, and gated so it never reaches a consumer. It exists for one
    /// reason: the deadlock this provider can hit is between two processes, and
    /// reproducing it needs a real child whose behaviour the test controls.
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
        stderr: format!("failed to parse CLI output: {e}"),
    })?;

    if output.is_error {
        return Err(ProviderError::Process {
            exit_code: None,
            stderr: output.result,
        });
    }

    Ok(output)
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
    /// Read only by tests until R-3 classifies it, which is why the lint below is
    /// `expect` and not `allow`: `expect` goes RED the moment the condition it
    /// describes stops holding, so the day a production path reads this field the
    /// attribute forces its own removal. An `allow` would outlive its reason in
    /// silence, which is the thing the project's rule against silencing a linter is
    /// actually about.
    /// `cfg_attr(not(test), ...)` and not a bare `expect`, and the difference is
    /// measured: under `cfg(test)` the field IS read -- by the tests in this file --
    /// so an unconditional expectation is UNFULFILLED there and `-D warnings` turns
    /// that into an error of its own. The expectation belongs to the target where the
    /// field is genuinely dead, which is the library without its test module.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "deserialized here (R-26); the production reader arrives with R-3"
        )
    )]
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
/// # The envelope reports both sides, and it always did
///
/// Two claims stood here and both were false. They said the envelope carried no
/// `stop_reason`-equivalent field and no output count, each "verified" — and what
/// they had been checked against was `CliOutput`, this crate's own view of the wire,
/// which held those fields only because nobody had added them. The check confirmed
/// itself.
///
/// Verified against CAPTURED envelopes this time, not against `CliOutput`: thirteen
/// of them, twelve successes and one failure, produced by the same
/// `claude --print --output-format json` this provider invokes. The envelope carries
/// a field literally named `stop_reason`, and `usage` carries `output_tokens`
/// alongside `input_tokens`. Both are read now, and
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
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "labelled here; the production call site arrives with R-3's implementation"
    )
)]
pub(crate) const ENVELOPE_DIAGNOSIS_PREFIX: &str = "cli envelope: ";

/// Labels the envelope's `result` for the `Process.stderr` path.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "labelled here; the production call site arrives with R-3's implementation"
    )
)]
pub(crate) fn label_envelope_diagnosis(result: &str) -> String {
    // STUB: the behaviour lands in this task's implementation step.
    let _ = result;
    String::new()
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
    /// The user prompt is sent via stdin to avoid shell injection and
    /// command-line length limits. The subprocess is launched directly
    /// without invoking a shell.
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
                stderr: format!("failed to spawn claude process: {e}"),
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
            return Err(ProviderError::Process {
                exit_code: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
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
    fn the_diagnosis_prefix_labels_the_envelope_result() {
        // THE LITERAL, never `starts_with(ENVELOPE_DIAGNOSIS_PREFIX)`: with the
        // constant on both sides, emptying it moves both and the test stays green over
        // a label that vanished.
        assert_eq!(
            label_envelope_diagnosis("boom"),
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

    /// is_error=true returns ProviderError::Process.
    #[test]
    fn test_parse_cli_output_error_flag_returns_process_error() {
        let outer =
            r#"{"type":"result","subtype":"error","is_error":true,"result":"Rate limit exceeded"}"#;
        let result = parse_envelope(outer);
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
}
