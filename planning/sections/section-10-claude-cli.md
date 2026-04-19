# Section 10: ClaudeCliProvider -- CLI Subprocess (`providers/claude_cli.rs`)

## Overview

This section implements `ClaudeCliProvider`, a concrete `LlmProvider` implementation that communicates with Claude via the `claude` CLI tool as a subprocess using `tokio::process`. The provider supports model alias resolution (e.g., "sonnet" maps to "claude-sonnet-4-6"), detects nested session environments via the `CLAUDECODE` env var, handles double-nested JSON parsing from the CLI output envelope, and strips code fences from LLM responses. It is feature-gated behind `claude-cli` and requires no additional external crates beyond `tokio`.

## Dependencies

- **External crates**:
  - `tokio` (already in dependencies) -- `process::Command`, `time::timeout`
  - `serde` and `serde_json` (already in dependencies) -- parsing CLI output envelope
- **Internal sections**:
  - Section 01 (`error.rs`) -- `ProviderError` (Process, Timeout, Auth, NestedSession)
  - Section 06 (`provider.rs`) -- `LlmProvider` trait, `CompletionConfig`
- **Standard library**: `std::env` for `CLAUDECODE` detection
- **Feature flag**: `claude-cli` -- must be added to `Cargo.toml` under `[features]` and gate the module in `providers/mod.rs`

## Files to Create or Modify

| File | Action |
|------|--------|
| `magi-core/src/providers/claude_cli.rs` | Create -- contains `ClaudeCliProvider` and private `CliOutput` |
| `magi-core/src/providers/mod.rs` | Add `#[cfg(feature = "claude-cli")] pub mod claude_cli;` |
| `magi-core/Cargo.toml` | Add `claude-cli` feature flag (no extra deps needed) |

## File Header

Every new source file in this project must start with:

```rust
// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05
```

## Tests (Write First -- Red Phase)

All tests go in `src/providers/claude_cli.rs` inside a `#[cfg(test)] mod tests` block. Since these tests should not launch real subprocesses, focus on unit-testing construction logic, model alias resolution, output parsing, and env var detection.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // -- BDD Scenario 23: detects nested session --

    /// CLAUDECODE env var present returns Err(ProviderError::NestedSession) in constructor.
    #[test]
    fn test_new_with_claudecode_env_returns_nested_session_error() {
        // Set CLAUDECODE env var temporarily
        // Call ClaudeCliProvider::new("sonnet")
        // Assert Err(ProviderError::NestedSession)
        // Restore env var
    }

    // -- Model alias resolution --

    /// new("sonnet") maps to "claude-sonnet-4-6".
    #[test]
    fn test_new_sonnet_maps_to_claude_sonnet_model() {
        // let provider = ClaudeCliProvider::new("sonnet").unwrap();
        // Assert provider.model() == "claude-sonnet-4-6"
    }

    /// new("opus") maps to "claude-opus-4-6".
    #[test]
    fn test_new_opus_maps_to_claude_opus_model() {
        // let provider = ClaudeCliProvider::new("opus").unwrap();
        // Assert provider.model() == "claude-opus-4-6"
    }

    /// new("haiku") maps to "claude-haiku-4-5-20251001".
    #[test]
    fn test_new_haiku_maps_to_claude_haiku_model() {
        // let provider = ClaudeCliProvider::new("haiku").unwrap();
        // Assert provider.model() == "claude-haiku-4-5-20251001"
    }

    /// new("claude-custom-model") passes through (contains "claude-").
    #[test]
    fn test_new_claude_prefix_passes_through() {
        // let provider = ClaudeCliProvider::new("claude-custom-model").unwrap();
        // Assert provider.model() == "claude-custom-model"
    }

    /// new("invalid") returns ProviderError::Auth.
    #[test]
    fn test_new_invalid_model_returns_auth_error() {
        // let result = ClaudeCliProvider::new("invalid");
        // Assert Err(ProviderError::Auth { .. })
    }

    // -- Accessors --

    /// provider.name() returns "claude-cli".
    #[test]
    fn test_provider_name_returns_claude_cli() {
        // let provider = ClaudeCliProvider::new("sonnet").unwrap();
        // Assert provider.name() == "claude-cli"
    }

    /// provider.model() returns the resolved model_id.
    #[test]
    fn test_provider_model_returns_resolved_model_id() {
        // let provider = ClaudeCliProvider::new("opus").unwrap();
        // Assert provider.model() == "claude-opus-4-6"
    }

    // -- BDD Scenario 18: command arguments --

    /// build_args includes --print, --output-format json, --model, --system-prompt.
    #[test]
    fn test_build_args_includes_required_cli_flags() {
        // Verify the argument list built for the child process includes:
        //   "--print", "--output-format", "json", "--model", model_id, "--system-prompt", system_prompt
    }

    // -- BDD Scenario 19: parses double-nested JSON --

    /// parse_cli_output extracts inner JSON from {"result": "..."} envelope.
    #[test]
    fn test_parse_cli_output_extracts_inner_result() {
        // let outer = r#"{"is_error": false, "result": "{\"agent\":\"melchior\",...}"}"#;
        // Assert parse_cli_output(outer) returns the inner JSON string
    }

    // -- BDD Scenario 20: detects error in CLI response --

    /// is_error=true returns ProviderError::Process.
    #[test]
    fn test_parse_cli_output_error_flag_returns_process_error() {
        // let outer = r#"{"is_error": true, "result": "Something went wrong"}"#;
        // Assert parse_cli_output(outer) returns Err(ProviderError::Process { .. })
    }

    // -- BDD Scenario 21: strips code fences --

    /// extract_json removes ```json ... ``` wrapping.
    #[test]
    fn test_extract_json_strips_code_fences() {
        // let input = "```json\n{\"key\": \"value\"}\n```";
        // Assert extract_json(input) == "{\"key\": \"value\"}"
    }

    // -- stdin usage --

    /// complete sends user prompt via stdin, not as CLI arg.
    #[test]
    fn test_user_prompt_sent_via_stdin_not_cli_arg() {
        // Verify the command construction does NOT include user_prompt in args
        // User prompt should be written to child.stdin
    }
}
```

## Implementation Details (Green Phase)

### Cargo.toml Changes

Add to `[features]`:

```toml
claude-cli = []
```

No additional dependencies -- `tokio` with `process` feature is already present.

### `ClaudeCliProvider` Struct

- **Fields**:
  - `model: String` -- the display alias (e.g., "sonnet", "opus", or pass-through)
  - `model_id: String` -- the resolved model identifier sent to the CLI

- **Constructor**:
  - `new(model: impl Into<String>) -> Result<Self, ProviderError>` -- performs two checks:
    1. **Nested session detection**: checks `std::env::var("CLAUDECODE")`. If the variable is set (regardless of value), returns `ProviderError::NestedSession` immediately. This prevents recursive agent spawning when running inside Claude Code.
    2. **Model alias resolution**: maps known aliases to full model identifiers:
       - `"opus"` -> `"claude-opus-4-6"`
       - `"sonnet"` -> `"claude-sonnet-4-6"`
       - `"haiku"` -> `"claude-haiku-4-5-20251001"`
       - Any string containing `"claude-"` -> passed through as-is
       - Anything else -> `ProviderError::Auth` with message indicating unknown model

### `LlmProvider` Implementation

```rust
#[async_trait]
impl LlmProvider for ClaudeCliProvider {
    async fn complete(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        config: &CompletionConfig,
    ) -> Result<String, ProviderError>;

    fn name(&self) -> &str;  // returns "claude-cli"
    fn model(&self) -> &str; // returns &self.model_id
}
```

#### `complete()` Method

1. **Build command**: `tokio::process::Command::new("claude")` with arguments:
   - `["--print", "--output-format", "json", "--model", &self.model_id, "--system-prompt", system_prompt]`
   - All stdio piped: `.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())`
   - `.kill_on_drop(true)` -- ensures child process is killed if the future is dropped

2. **Spawn and write stdin**: spawn the child process, write `user_prompt` to stdin, then `drop(stdin)` to signal EOF. The user prompt is sent via stdin (not as a CLI argument) to avoid shell injection and command-line length limits.

3. **Wait with timeout**: wrap `child.wait_with_output()` in `tokio::time::timeout(Duration)`. The timeout duration comes from the orchestrator's `MagiConfig.timeout`, not from `CompletionConfig`. However, since `complete()` does not have access to `MagiConfig`, the orchestrator wraps the entire agent task in `tokio::time::timeout`. The provider itself does not apply a separate timeout.

4. **Handle exit status**: if the child exits with non-zero status, return `ProviderError::Process { exit_code: status.code(), stderr: String::from_utf8_lossy(&output.stderr).to_string() }`.

5. **Parse CLI output**: the CLI returns a JSON envelope:
   ```json
   {"is_error": false, "result": "<inner content as string>"}
   ```
   - Deserialize stdout as `CliOutput`
   - If `is_error == true`, return `ProviderError::Process` with the `result` field as stderr
   - Extract the `result` string

6. **Strip code fences**: if the result string starts with `` ```json `` (or `` ``` ``), strip the opening fence line and closing `` ``` `` line.

7. **Return**: the cleaned result string

### `CliOutput` Helper Struct (Private)

```rust
#[derive(Deserialize)]
struct CliOutput {
    is_error: bool,
    result: String,
}
```

Handles the outer JSON envelope from the Claude CLI tool.

### Helper Functions (Private)

- `parse_cli_output(raw: &str) -> Result<String, ProviderError>` -- deserializes `CliOutput`, checks `is_error`, returns `result` string
- `strip_code_fences(text: &str) -> &str` (or returns `String`) -- removes `` ```json\n `` prefix and `` \n``` `` suffix if present

### `providers/mod.rs`

Add alongside the `claude-api` gate:

```rust
#[cfg(feature = "claude-cli")]
pub mod claude_cli;
```

### Security Considerations

- `tokio::process::Command::new("claude")` calls the executable directly without invoking a shell, preventing shell injection even with user-controlled prompts. Document this in Rustdoc.
- User prompts are sent via stdin, never as command-line arguments (which could be visible in process listings).

### Windows Limitation

`child.kill()` on Windows uses `TerminateProcess`, which does not propagate to grandchild processes. If the `claude` CLI spawns subprocesses, those may survive a timeout kill. Document this as a known limitation in Rustdoc.

## Constraints Checklist

- No `panic!`, `unwrap()`, `expect()` outside `#[cfg(test)]`
- No `unsafe`
- All public types and methods have `///` Rustdoc
- Feature-gated: entire module only compiles with `claude-cli` feature
- `CLAUDECODE` env var check is fail-fast in the constructor
- Model aliases are validated at construction time, not at `complete()` time
- `kill_on_drop(true)` set on child process for resource safety
- User prompt sent via stdin, not CLI arguments
- No shell invocation -- direct executable call via `Command::new`
- `rustfmt` and `clippy --tests -- -D warnings` clean
- File header present on all new files

## Refactor Phase Notes

After Green phase passes all tests:

- Verify the `CLAUDECODE` env var test properly restores the environment (consider using a test mutex or serial test runner)
- Ensure code fence stripping handles edge cases: no fences, only opening fence, multiple fence blocks
- Add `tracing::debug!` for subprocess spawn and completion events
- Add Rustdoc `///` examples on `ClaudeCliProvider::new` showing alias usage
- Document the Windows `TerminateProcess` limitation in the struct-level Rustdoc
- Document the shell injection mitigation in the `complete()` method Rustdoc
- Confirm `cargo doc --no-deps` generates clean documentation
- Verify the module compiles cleanly with `cargo build --features claude-cli`
