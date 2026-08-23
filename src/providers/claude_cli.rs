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

/// Outer JSON envelope from the Claude CLI tool.
#[derive(Debug, Deserialize)]
struct CliOutput {
    is_error: bool,
    result: String,
    /// Token counts, when the CLI envelope reports them. `#[serde(default)]`:
    /// the envelope does not guarantee this field across CLI versions, and
    /// `is_error`/`result` extraction must not depend on it.
    #[serde(default)]
    usage: Option<CliUsage>,
}

/// Token counts from the CLI envelope's `usage` object.
///
/// The CLI envelope carries **no `stop_reason`-equivalent field at all** —
/// verified against the envelope shape (`CliOutput`'s only fields are
/// `is_error`/`result`/`usage`) — so there is nothing here to translate into a
/// [`crate::provider::FinishReason`]. [`parse_completion`] leaves `finish` at
/// `None`, which says "this backend does not say" rather than inventing
/// [`crate::provider::FinishReason::Stop`].
/// # It carries no OUTPUT count either, and that is verified rather than assumed
///
/// The envelope's `usage` object reports `input_tokens` and nothing equivalent for the
/// completion side, so [`parse_completion`] leaves `completion_tokens` at `None`. That is the
/// same statement `finish` makes: this backend does not say. Inventing a zero would report a
/// measurement that never happened.
#[derive(Debug, Default, Deserialize)]
struct CliUsage {
    #[serde(default)]
    input_tokens: Option<u32>,
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
/// A [`Completion`] whose text has its code fences stripped, and whose
/// telemetry carries `prompt_tokens` when the envelope's `usage.input_tokens`
/// is present. `finish` stays `None` unconditionally (see [`CliUsage`]).
///
/// # Errors
/// Shares its error path with [`parse_envelope`], which owns the envelope's convention.
fn parse_completion(raw: &str, reasoning: ReasoningControl) -> Result<Completion, ProviderError> {
    let output = parse_envelope(raw)?;
    let text = strip_code_fences(&output.result).to_string();

    let prompt_tokens = output.usage.and_then(|u| u.input_tokens);

    let mut telemetry = CompletionTelemetry::unmeasured();
    if let Some(n) = prompt_tokens {
        telemetry = telemetry.with_prompt_tokens(n);
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
    use serial_test::serial;

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

    /// Step 1c/3: the envelope carries `usage.input_tokens` (visible on its own
    /// in `test_parse_cli_output_extracts_inner_result`'s fixture), but NO
    /// `stop_reason` field at all — verified against the envelope shape, not
    /// assumed. `finish` staying `None` says "this backend does not say";
    /// asserting `Stop` would invent a measurement.
    #[test]
    fn the_cli_provider_reports_usage_and_leaves_finish_none() {
        let raw = r#"{"is_error":false,"result":"hi","usage":{"input_tokens":100}}"#;
        let out = super::parse_completion(raw, ReasoningControl::default())
            .expect("valid envelope parses");
        assert_eq!(out.telemetry.prompt_tokens, Some(100));
        assert_eq!(
            out.telemetry.finish, None,
            "the CLI envelope has no stop_reason"
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
