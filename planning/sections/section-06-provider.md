# Section 06: LlmProvider Trait and RetryProvider (`provider.rs`)

## Overview

This section defines the `LlmProvider` async trait, the `CompletionConfig` struct, and the `RetryProvider` opt-in wrapper. The `LlmProvider` trait is the abstraction that makes magi-core LLM-agnostic -- any backend (Claude, Gemini, OpenAI, local models) implements this trait. The trait uses `async-trait` because native async traits in Rust do not yet support `dyn Trait` dispatch, which is required for `Arc<dyn LlmProvider>` with `tokio::spawn`. The `RetryProvider` wraps any provider with configurable retry logic for transient errors, keeping retry out of the orchestrator (spec-compliant).

## Dependencies

- **External crates**:
  - `async-trait = "0.1"` (must be added to `Cargo.toml`)
  - `tokio = { version = "1", features = ["time"] }` (for `RetryProvider` sleep between retries)
- **Internal sections**:
  - Section 01 (`error.rs`) -- `ProviderError` for the trait's error type
- **Standard library**: `std::sync::Arc`, `std::time::Duration`

## Files to Create or Modify

| File | Action |
|------|--------|
| `magi-core/Cargo.toml` | Add `async-trait = "0.1"`, ensure `tokio` has `time` feature |
| `magi-core/src/provider.rs` | Create -- contains `LlmProvider` trait, `CompletionConfig`, `RetryProvider` |
| `magi-core/src/lib.rs` | Add `pub mod provider;` |

## File Header

Every new source file in this project must start with:

```rust
// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05
```

## Tests (Write First -- Red Phase)

All tests go in `src/provider.rs` inside a `#[cfg(test)] mod tests` block. Async tests require `#[tokio::test]`. For mocking, create a simple manual mock struct implementing `LlmProvider` (or use `mockall` if already available).

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Helper: manual mock provider for testing
    // struct MockProvider { ... }
    // impl LlmProvider for MockProvider { ... }

    // -- CompletionConfig tests --

    /// CompletionConfig::default has max_tokens=4096, temperature=0.0.
    #[test]
    fn test_completion_config_default_values() {
        // let config = CompletionConfig::default();
        // assert_eq!(config.max_tokens, 4096);
        // assert!((config.temperature - 0.0).abs() < f64::EPSILON);
    }

    /// CompletionConfig is #[non_exhaustive] (structural property, compile-time check).
    #[test]
    fn test_completion_config_is_non_exhaustive() {
        // This test verifies the struct cannot be constructed with struct literal
        // outside this crate. For in-crate tests, just verify Default works.
        // The #[non_exhaustive] attribute is a compile-time guarantee.
        // Construct via Default and verify fields are accessible.
    }

    // -- RetryProvider delegation tests --

    /// RetryProvider wraps inner provider and delegates name().
    #[tokio::test]
    async fn test_retry_provider_delegates_name() {
        // Create mock provider with name "test-provider"
        // Wrap in RetryProvider
        // Assert retry_provider.name() == "test-provider"
    }

    /// RetryProvider wraps inner provider and delegates model().
    #[tokio::test]
    async fn test_retry_provider_delegates_model() {
        // Create mock with model "test-model"
        // Wrap in RetryProvider
        // Assert retry_provider.model() == "test-model"
    }

    // -- RetryProvider retry behavior --

    /// RetryProvider retries on ProviderError::Timeout up to max_retries.
    #[tokio::test]
    async fn test_retry_provider_retries_on_timeout() {
        // Create mock that fails with Timeout twice, then succeeds
        // Set max_retries=3, base_delay very short for test speed
        // Assert complete() returns Ok
    }

    /// RetryProvider retries on ProviderError::Http with status 500.
    #[tokio::test]
    async fn test_retry_provider_retries_on_http_500() {
        // Create mock that fails with Http { status: 500 } then succeeds
        // Assert complete() returns Ok
    }

    /// RetryProvider retries on ProviderError::Http with status 429.
    #[tokio::test]
    async fn test_retry_provider_retries_on_http_429() {
        // Create mock that fails with Http { status: 429 } then succeeds
        // Assert complete() returns Ok
    }

    /// RetryProvider does NOT retry on ProviderError::Auth.
    #[tokio::test]
    async fn test_retry_provider_does_not_retry_on_auth() {
        // Create mock that fails with Auth
        // Assert complete() returns Err immediately (only 1 call to inner)
    }

    /// RetryProvider does NOT retry on ProviderError::Process.
    #[tokio::test]
    async fn test_retry_provider_does_not_retry_on_process() {
        // Create mock that fails with Process
        // Assert complete() returns Err immediately
    }

    /// RetryProvider returns last error after exhausting retries.
    #[tokio::test]
    async fn test_retry_provider_returns_last_error_after_exhausting_retries() {
        // Create mock that always fails with Timeout
        // Set max_retries=2
        // Assert complete() returns Err(Timeout) after 3 total attempts (1 + 2 retries)
    }

    /// RetryProvider returns success on first successful retry.
    #[tokio::test]
    async fn test_retry_provider_returns_success_on_first_retry() {
        // Create mock that fails once, then succeeds
        // Assert complete() returns Ok with the success value
    }

    /// RetryProvider default config: 3 retries, 1s delay.
    #[test]
    fn test_retry_provider_default_config() {
        // Create RetryProvider with defaults
        // Assert max_retries == 3
        // Assert base_delay == Duration::from_secs(1)
    }
}
```

## Implementation Details (Green Phase)

### `LlmProvider` Trait

The core abstraction for LLM backends. Uses `async-trait` for dynamic dispatch compatibility.

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Sends a completion request to the LLM provider.
    ///
    /// # Parameters
    /// - `system_prompt`: The system-level instruction for the LLM
    /// - `user_prompt`: The user's input content
    /// - `config`: Completion parameters (max_tokens, temperature)
    ///
    /// # Returns
    /// The LLM's text response, or a `ProviderError` on failure.
    async fn complete(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        config: &CompletionConfig,
    ) -> Result<String, ProviderError>;

    /// Returns the provider's name (e.g., "claude", "claude-cli", "openai").
    fn name(&self) -> &str;

    /// Returns the model identifier (e.g., "claude-sonnet-4-6").
    fn model(&self) -> &str;
}
```

The `Send + Sync` bounds are required because `Arc<dyn LlmProvider>` is shared across `tokio::spawn` tasks. Without these bounds, the trait object cannot be sent between threads.

### `CompletionConfig` Struct

A `#[non_exhaustive]` configuration struct for completion requests.

- **Fields**:
  - `max_tokens: u32` -- maximum tokens in the LLM response (default: `4096`)
  - `temperature: f64` -- sampling temperature (default: `0.0` for deterministic output)
- **Derives**: `Debug`, `Clone`
- **Implements**: `Default`
- The `#[non_exhaustive]` attribute prevents external crates from constructing via struct literal, allowing new fields to be added without breaking changes.

### `RetryProvider` Struct

An opt-in wrapper that adds retry logic to any `LlmProvider`. Implements `LlmProvider` itself, making it transparent to consumers. This design keeps retry out of the orchestrator (spec-compliant: "No retry automatico a nivel de orquestador") while giving users explicit control.

- **Fields**:
  - `inner: Arc<dyn LlmProvider>` -- the wrapped provider
  - `max_retries: u32` -- maximum number of retry attempts after the first failure (default: `3`)
  - `base_delay: Duration` -- delay between retries (default: `1 second`)
- **Derives**: None (cannot derive `Debug` or `Clone` due to `Arc<dyn LlmProvider>`)

- **Constructor**:
  - `new(inner: Arc<dyn LlmProvider>) -> Self` -- creates with default retry settings
  - `with_config(inner: Arc<dyn LlmProvider>, max_retries: u32, base_delay: Duration) -> Self` -- creates with custom retry settings

- **`LlmProvider` implementation**:

  - `name(&self) -> &str` -- delegates to `self.inner.name()`
  - `model(&self) -> &str` -- delegates to `self.inner.model()`
  - `complete(...)` -- retry logic:
    1. Attempt `self.inner.complete(...)`
    2. On success, return immediately
    3. On failure, check if the error is retryable:
       - **Retryable**: `ProviderError::Timeout`, `ProviderError::Network`, `ProviderError::Http` with status `500` or `429`
       - **Not retryable**: `ProviderError::Auth`, `ProviderError::Process`, `ProviderError::NestedSession`, `ProviderError::Http` with other status codes
    4. If retryable and retries remaining, sleep for `base_delay` then retry
    5. If retries exhausted, return the last error
    6. If not retryable, return immediately

### Retryable Error Classification

The `is_retryable` check is a private function or method:

```rust
fn is_retryable(error: &ProviderError) -> bool {
    matches!(error,
        ProviderError::Timeout { .. }
        | ProviderError::Network { .. }
        | ProviderError::Http { status, .. } if *status == 500 || *status == 429
    )
}
```

### `lib.rs` Module Declaration

Add `pub mod provider;` to `src/lib.rs`.

## Constraints Checklist

- No `panic!`, `unwrap()`, `expect()` outside `#[cfg(test)]`
- No `unsafe`
- All public types, trait methods, and struct methods have `///` Rustdoc
- `#[non_exhaustive]` on `CompletionConfig`
- `async-trait` used for `dyn Trait` compatibility
- `Send + Sync` bounds on `LlmProvider` for `Arc<dyn LlmProvider>` usage with `tokio::spawn`
- Retry logic only retries transient errors, not auth or process failures
- `rustfmt` and `clippy --tests -- -D warnings` clean
- File header present on all new files

## Refactor Phase Notes

After Green phase passes all tests:

- Verify `is_retryable` covers all intended transient error cases
- Consider whether `base_delay` should use exponential backoff (current spec says fixed delay -- keep it simple)
- Add Rustdoc `///` explaining the retry strategy and which errors are retryable
- Document the `Send + Sync` requirement rationale in trait-level Rustdoc
- Confirm `cargo doc --no-deps` generates clean documentation
- Verify that `RetryProvider` is fully transparent -- consumers should not need to know retry is active
