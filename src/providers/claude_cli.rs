// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05

use crate::error::ProviderError;
use crate::provider::{CompletionConfig, LlmProvider, resolve_claude_alias};
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
    /// - `"opus"` maps to `"claude-opus-4-6"`
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

/// Parses the CLI output envelope from the `claude` subprocess.
///
/// The CLI returns a JSON envelope:
/// ```json
/// {"is_error": false, "result": "<inner content as string>"}
/// ```
///
/// If `is_error` is `true`, returns `ProviderError::Process`.
/// Otherwise, returns the `result` string.
///
/// # Errors
///
/// - `ProviderError::Process` if `is_error` is `true` or JSON is malformed
fn parse_cli_output(raw: &str) -> Result<String, ProviderError> {
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

    Ok(output.result)
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
#[derive(Deserialize)]
struct CliOutput {
    is_error: bool,
    result: String,
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
    /// **Note:** `config` (max_tokens, temperature) is ignored because the
    /// `claude --print` CLI does not expose those flags. The CLI uses its
    /// own server-side defaults. Users who need fine-grained control should
    /// use [`ClaudeProvider`](crate::providers::claude::ClaudeProvider) (HTTP API) instead.
    async fn complete(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        _config: &CompletionConfig,
    ) -> Result<String, ProviderError> {
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
        let result = parse_cli_output(&stdout)?;
        Ok(strip_code_fences(&result).to_string())
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

    /// new("opus") maps to "claude-opus-4-6".
    #[test]
    #[serial]
    fn test_new_opus_maps_to_claude_opus_model() {
        without_claudecode(|| {
            let provider = ClaudeCliProvider::new("opus").unwrap();
            assert_eq!(provider.model(), "claude-opus-4-6");
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
            assert_eq!(provider.model(), "claude-opus-4-6");
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

    /// parse_cli_output extracts inner JSON from {"is_error": false, "result": "..."} envelope.
    #[test]
    fn test_parse_cli_output_extracts_inner_result() {
        let outer = r#"{"type":"result","subtype":"success","is_error":false,"result":"{\"agent\":\"melchior\",\"verdict\":\"approve\"}","usage":{"input_tokens":100}}"#;
        let result = parse_cli_output(outer).unwrap();
        assert_eq!(result, r#"{"agent":"melchior","verdict":"approve"}"#);
    }

    // -- BDD Scenario 20: detects error in CLI response --

    /// is_error=true returns ProviderError::Process.
    #[test]
    fn test_parse_cli_output_error_flag_returns_process_error() {
        let outer =
            r#"{"type":"result","subtype":"error","is_error":true,"result":"Rate limit exceeded"}"#;
        let result = parse_cli_output(outer);
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
        let result = parse_cli_output("not valid json");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ProviderError::Process { .. }),
            "expected Process, got: {err}"
        );
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
