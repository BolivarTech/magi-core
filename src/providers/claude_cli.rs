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
}

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

        Ok(Self { model_id })
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

        let mut child = Command::new("claude")
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

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(user_prompt.as_bytes())
                .await
                .map_err(|e| ProviderError::Process {
                    exit_code: None,
                    stderr: format!("failed to write to stdin: {e}"),
                })?;
        }

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| ProviderError::Process {
                exit_code: None,
                stderr: format!("failed to wait for claude process: {e}"),
            })?;

        if !output.status.success() {
            return Err(ProviderError::Process {
                exit_code: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
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
    use crate::provider::FinishReason;
    use crate::test_support::{
        CAPTURED_404, FAILURE_KEEP_LIST, SUCCESS_KEEP_LIST, captured_envelope_with_stop_reason,
        captured_failure_with_unusable_api_error_status, captured_failure_without_api_error_status,
    };
    use serial_test::serial;

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

    /// Saves the current CLAUDECODE env var, clears it, runs the closure,
    /// then restores the original value. All env mutations are in unsafe blocks
    /// (required by Rust 2024 edition).
    fn without_claudecode<F: FnOnce()>(f: F) {
        let original = std::env::var("CLAUDECODE").ok();
        unsafe {
            std::env::remove_var("CLAUDECODE");
        }
        f();
        if let Some(val) = original {
            unsafe {
                std::env::set_var("CLAUDECODE", val);
            }
        }
    }

    /// Sets CLAUDECODE env var, runs the closure, then restores original value.
    fn with_claudecode<F: FnOnce()>(f: F) {
        let original = std::env::var("CLAUDECODE").ok();
        unsafe {
            std::env::set_var("CLAUDECODE", "1");
        }
        f();
        if let Some(val) = original {
            unsafe {
                std::env::set_var("CLAUDECODE", val);
            }
        } else {
            unsafe {
                std::env::remove_var("CLAUDECODE");
            }
        }
    }

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
