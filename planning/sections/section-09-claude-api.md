# Section 09: ClaudeProvider -- HTTP API (`providers/claude.rs`)

## Overview

This section implements `ClaudeProvider`, a concrete `LlmProvider` implementation that communicates with the Claude Messages API via HTTP using `reqwest`. The provider is feature-gated behind `claude-api` so users who only need CLI-based providers do not pull in the `reqwest` dependency. `ClaudeProvider` manages a reusable `reqwest::Client` for connection pooling, sends POST requests to the Anthropic API, and maps HTTP errors to `ProviderError` variants.

## Dependencies

- **External crates**:
  - `reqwest = { version = "0.12", features = ["json"], optional = true }` -- HTTP client, only compiled with `claude-api` feature
  - `serde` and `serde_json` (already in dependencies) -- request/response serialization
- **Internal sections**:
  - Section 01 (`error.rs`) -- `ProviderError` for error mapping (Http, Network, Timeout, Auth)
  - Section 06 (`provider.rs`) -- `LlmProvider` trait, `CompletionConfig`
- **Standard library**: none beyond what's already used
- **Feature flag**: `claude-api` -- must be added to `Cargo.toml` under `[features]` and gate the module in `providers/mod.rs`

## Files to Create or Modify

| File | Action |
|------|--------|
| `magi-core/src/providers/claude.rs` | Create -- contains `ClaudeProvider` |
| `magi-core/src/providers/mod.rs` | Create or modify -- add `#[cfg(feature = "claude-api")] pub mod claude;` |
| `magi-core/Cargo.toml` | Add `reqwest` as optional dependency, add `claude-api` feature flag |
| `magi-core/src/lib.rs` | Add `pub mod providers;` if not already present |

## File Header

Every new source file in this project must start with:

```rust
// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05
```

## Tests (Write First -- Red Phase)

All tests go in `src/providers/claude.rs` inside a `#[cfg(test)] mod tests` block. Since these tests should not make real HTTP calls, use unit tests that verify construction, accessor methods, and request/response formatting logic. Integration tests against the real API are out of scope.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // -- Construction and accessors --

    /// ClaudeProvider::new creates provider with api_key and model.
    #[test]
    fn test_claude_provider_new_creates_with_key_and_model() {
        // let provider = ClaudeProvider::new("sk-test-key", "claude-sonnet-4-6");
        // Assert provider is constructed without error
    }

    /// provider.name() returns "claude".
    #[test]
    fn test_claude_provider_name_returns_claude() {
        // let provider = ClaudeProvider::new("key", "model");
        // Assert provider.name() == "claude"
    }

    /// provider.model() returns the configured model string.
    #[test]
    fn test_claude_provider_model_returns_configured_model() {
        // let provider = ClaudeProvider::new("key", "claude-opus-4-6");
        // Assert provider.model() == "claude-opus-4-6"
    }

    // -- Request building --

    /// complete sends POST to /v1/messages with correct headers.
    #[test]
    fn test_build_request_includes_correct_headers() {
        // Verify request construction includes:
        //   x-api-key header
        //   anthropic-version: 2023-06-01
        //   content-type: application/json
    }

    /// complete builds request body with model, max_tokens, temperature, system, messages.
    #[test]
    fn test_build_request_body_contains_all_required_fields() {
        // Build request body for system_prompt="You are helpful", user_prompt="Hello"
        // Assert body JSON contains "model", "max_tokens", "temperature", "system"
        // Assert body.messages[0].role == "user"
        // Assert body.messages[0].content == "Hello"
    }

    // -- Response parsing --

    /// complete extracts text content from Claude response format.
    #[test]
    fn test_parse_claude_response_extracts_text_content() {
        // Simulate Claude API response JSON:
        // {"content": [{"type": "text", "text": "response text"}], ...}
        // Assert extracted text == "response text"
    }

    /// complete maps non-2xx response to ProviderError::Http.
    #[test]
    fn test_non_2xx_response_maps_to_provider_error_http() {
        // Simulate a 401 response with error body
        // Assert maps to ProviderError::Http { status: 401, body: "..." }
    }

    // -- Client reuse --

    /// reqwest::Client is reused across calls (not created per call).
    #[test]
    fn test_client_is_stored_in_struct() {
        // Verify ClaudeProvider stores client as a field
        // (structural test -- the client field exists and is set at construction)
    }
}
```

## Implementation Details (Green Phase)

### Cargo.toml Changes

Add to `[dependencies]`:

```toml
reqwest = { version = "0.12", features = ["json"], optional = true }
```

Add to `[features]`:

```toml
claude-api = ["dep:reqwest"]
```

### `ClaudeProvider` Struct

- **Fields**:
  - `client: reqwest::Client` -- reusable HTTP client with connection pooling
  - `api_key: String` -- Anthropic API key
  - `model: String` -- model identifier (e.g., `"claude-sonnet-4-6"`)

- **Constructor**:
  - `new(api_key: impl Into<String>, model: impl Into<String>) -> Self` -- creates a `reqwest::Client` with default settings. The API key is stored but not set as a default header on the client (it is added per-request for flexibility).

### `LlmProvider` Implementation

```rust
#[async_trait]
impl LlmProvider for ClaudeProvider {
    async fn complete(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        config: &CompletionConfig,
    ) -> Result<String, ProviderError>;

    fn name(&self) -> &str;  // returns "claude"
    fn model(&self) -> &str; // returns &self.model
}
```

#### `complete()` Method

1. **Build request**: POST to `https://api.anthropic.com/v1/messages`
2. **Headers**:
   - `x-api-key: {api_key}` -- set via `.header()` with `set_sensitive(true)` on the header value to prevent logging
   - `anthropic-version: 2023-06-01`
   - `content-type: application/json`
3. **Request body** (as JSON):
   ```json
   {
     "model": "<self.model>",
     "max_tokens": "<config.max_tokens>",
     "temperature": "<config.temperature>",
     "system": "<system_prompt>",
     "messages": [
       {"role": "user", "content": "<user_prompt>"}
     ]
   }
   ```
4. **Send request**: `self.client.post(url).headers(headers).json(&body).send().await`
5. **Handle errors**:
   - `reqwest::Error` with timeout → `ProviderError::Timeout`
   - `reqwest::Error` with connection failure → `ProviderError::Network`
   - Non-2xx status → `ProviderError::Http { status, body }` (read response body for error details)
   - 401/403 status → `ProviderError::Auth` (more specific than generic Http)
6. **Parse response**: extract the text content from the Claude Messages API response format:
   ```json
   {
     "content": [
       {"type": "text", "text": "the actual response text"}
     ],
     ...
   }
   ```
   Find the first content block with `type == "text"` and return its `text` field.
7. **Return**: the extracted text string

### Helper Structs (Private)

Private structs for request/response serialization (derive `Serialize`/`Deserialize` as needed):

- `ClaudeRequest` -- model, max_tokens, temperature, system, messages
- `ClaudeMessage` -- role, content
- `ClaudeResponse` -- content (Vec of content blocks), plus any other fields needed
- `ContentBlock` -- type_, text (use `#[serde(rename = "type")]` for the `type` field)

### `providers/mod.rs`

```rust
#[cfg(feature = "claude-api")]
pub mod claude;
```

### `lib.rs` Module Declaration

Add `pub mod providers;` to `src/lib.rs` if not already present.

## Constraints Checklist

- No `panic!`, `unwrap()`, `expect()` outside `#[cfg(test)]`
- No `unsafe`
- All public types and methods have `///` Rustdoc
- Feature-gated: entire module only compiles with `claude-api` feature
- API key is never logged or included in error messages
- `reqwest::Client` is created once and reused (connection pooling)
- All HTTP errors mapped to appropriate `ProviderError` variants
- `#[serde(rename = "type")]` used for the `type` field in `ContentBlock` since `type` is a Rust keyword
- `rustfmt` and `clippy --tests -- -D warnings` clean
- File header present on all new files

## Refactor Phase Notes

After Green phase passes all tests:

- Verify API key is marked sensitive in request headers (`set_sensitive(true)`)
- Ensure error messages from non-2xx responses include the response body for diagnostics
- Add Rustdoc examples on `ClaudeProvider::new` showing typical usage
- Consider adding `tracing::debug!` for request/response metadata (never log the API key or full response body)
- Confirm `cargo doc --no-deps` generates clean documentation
- Verify the module compiles cleanly with `cargo build --features claude-api` and does NOT compile when the feature is disabled
