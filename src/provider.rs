// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05

use crate::error::ProviderError;
use std::sync::Arc;
use std::time::Duration;

/// Configuration for LLM completion requests.
///
/// Controls parameters like token limits and sampling temperature.
/// Marked `#[non_exhaustive]` to allow adding fields in future versions
/// without breaking downstream crates.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct CompletionConfig {
    /// Maximum number of tokens in the LLM response.
    pub max_tokens: u32,
    /// Sampling temperature (0.0 = deterministic).
    pub temperature: f64,
}

impl Default for CompletionConfig {
    fn default() -> Self {
        Self {
            max_tokens: 4096,
            temperature: 0.0,
        }
    }
}

/// Abstraction for LLM backends.
///
/// Any LLM provider (Claude, Gemini, OpenAI, local models) implements this
/// trait. Uses `async-trait` because native async traits in Rust do not yet
/// support `dyn Trait` dispatch, which is required for `Arc<dyn LlmProvider>`
/// with `tokio::spawn`.
///
/// The `Send + Sync` bounds are required because `Arc<dyn LlmProvider>` is
/// shared across `tokio::spawn` tasks.
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    /// Sends a completion request to the LLM provider.
    ///
    /// # Parameters
    /// - `system_prompt`: The system-level instruction for the LLM.
    /// - `user_prompt`: The user's input content.
    /// - `config`: Completion parameters (max_tokens, temperature).
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

/// Opt-in retry wrapper for any `LlmProvider`.
///
/// Wraps an inner provider and retries transient errors (timeout, network,
/// HTTP 500/429) up to `max_retries` times with exponential backoff starting
/// from `base_delay`. Non-retryable errors (auth, process, nested session,
/// other HTTP status codes) are returned immediately.
///
/// Implements `LlmProvider` itself, making it transparent to consumers.
pub struct RetryProvider {
    inner: Arc<dyn LlmProvider>,
    /// Maximum number of retry attempts after the first failure.
    pub max_retries: u32,
    /// Delay between retry attempts.
    pub base_delay: Duration,
}

impl RetryProvider {
    /// Creates a new `RetryProvider` with default settings (3 retries, 1s delay).
    ///
    /// # Parameters
    /// - `inner`: The provider to wrap with retry logic.
    pub fn new(inner: Arc<dyn LlmProvider>) -> Self {
        Self {
            inner,
            max_retries: 3,
            base_delay: Duration::from_secs(1),
        }
    }

    /// Creates a new `RetryProvider` with custom retry settings.
    ///
    /// # Parameters
    /// - `inner`: The provider to wrap with retry logic.
    /// - `max_retries`: Maximum retry attempts after the initial failure.
    /// - `base_delay`: Initial delay between retries; doubles on each subsequent attempt.
    pub fn with_config(
        inner: Arc<dyn LlmProvider>,
        max_retries: u32,
        base_delay: Duration,
    ) -> Self {
        Self {
            inner,
            max_retries,
            base_delay,
        }
    }
}

/// Determines whether a `ProviderError` is transient and should be retried.
///
/// Retryable errors:
/// - `Timeout`: Provider did not respond in time.
/// - `Network`: DNS, connection refused, etc.
/// - `Http` with status 500 (server error) or 429 (rate limit).
///
/// Non-retryable errors:
/// - `Auth`: Invalid credentials won't become valid on retry.
/// - `Process`: CLI subprocess failure.
/// - `NestedSession`: Structural environment issue.
/// - `Http` with other status codes (e.g., 400, 403, 404).
fn is_retryable(error: &ProviderError) -> bool {
    match error {
        ProviderError::Timeout { .. } | ProviderError::Network { .. } => true,
        ProviderError::Http { status, .. } => *status == 500 || *status == 429,
        _ => false,
    }
}

#[async_trait::async_trait]
impl LlmProvider for RetryProvider {
    async fn complete(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        config: &CompletionConfig,
    ) -> Result<String, ProviderError> {
        let mut last_error = None;
        let mut delay = self.base_delay;
        for attempt in 0..=self.max_retries {
            match self
                .inner
                .complete(system_prompt, user_prompt, config)
                .await
            {
                Ok(response) => return Ok(response),
                Err(err) => {
                    if !is_retryable(&err) || attempt == self.max_retries {
                        return Err(err);
                    }
                    last_error = Some(err);
                    tokio::time::sleep(delay).await;
                    delay = delay.saturating_mul(2);
                }
            }
        }
        Err(last_error.expect("at least one attempt must have been made"))
    }

    fn name(&self) -> &str {
        self.inner.name()
    }

    fn model(&self) -> &str {
        self.inner.model()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    /// Manual mock provider for testing.
    struct MockProvider {
        provider_name: String,
        provider_model: String,
        responses: std::sync::Mutex<Vec<Result<String, ProviderError>>>,
        call_count: AtomicU32,
    }

    impl MockProvider {
        fn new(name: &str, model: &str) -> Self {
            Self {
                provider_name: name.to_string(),
                provider_model: model.to_string(),
                responses: std::sync::Mutex::new(Vec::new()),
                call_count: AtomicU32::new(0),
            }
        }

        fn with_responses(
            name: &str,
            model: &str,
            responses: Vec<Result<String, ProviderError>>,
        ) -> Self {
            // Reverse so we can pop from the end (FIFO order)
            let mut reversed = responses;
            reversed.reverse();
            Self {
                provider_name: name.to_string(),
                provider_model: model.to_string(),
                responses: std::sync::Mutex::new(reversed),
                call_count: AtomicU32::new(0),
            }
        }

        fn call_count(&self) -> u32 {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for MockProvider {
        async fn complete(
            &self,
            _system_prompt: &str,
            _user_prompt: &str,
            _config: &CompletionConfig,
        ) -> Result<String, ProviderError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            let mut responses = self.responses.lock().unwrap();
            if let Some(result) = responses.pop() {
                result
            } else {
                Ok("default response".to_string())
            }
        }

        fn name(&self) -> &str {
            &self.provider_name
        }

        fn model(&self) -> &str {
            &self.provider_model
        }
    }

    // -- CompletionConfig tests --

    /// CompletionConfig::default has max_tokens=4096, temperature=0.0.
    #[test]
    fn test_completion_config_default_values() {
        let config = CompletionConfig::default();
        assert_eq!(config.max_tokens, 4096);
        assert!((config.temperature - 0.0).abs() < f64::EPSILON);
    }

    /// CompletionConfig is #[non_exhaustive] — verify Default works and fields accessible.
    #[test]
    fn test_completion_config_is_non_exhaustive() {
        let config = CompletionConfig::default();
        assert_eq!(config.max_tokens, 4096);
        assert!((config.temperature).abs() < f64::EPSILON);
    }

    // -- RetryProvider delegation tests --

    /// RetryProvider wraps inner provider and delegates name().
    #[tokio::test]
    async fn test_retry_provider_delegates_name() {
        let mock = Arc::new(MockProvider::new("test-provider", "test-model"));
        let retry = RetryProvider::new(mock);
        assert_eq!(retry.name(), "test-provider");
    }

    /// RetryProvider wraps inner provider and delegates model().
    #[tokio::test]
    async fn test_retry_provider_delegates_model() {
        let mock = Arc::new(MockProvider::new("test-provider", "test-model"));
        let retry = RetryProvider::new(mock);
        assert_eq!(retry.model(), "test-model");
    }

    // -- RetryProvider retry behavior --

    /// RetryProvider retries on ProviderError::Timeout up to max_retries.
    #[tokio::test]
    async fn test_retry_provider_retries_on_timeout() {
        let mock = Arc::new(MockProvider::with_responses(
            "p",
            "m",
            vec![
                Err(ProviderError::Timeout {
                    message: "t1".into(),
                }),
                Err(ProviderError::Timeout {
                    message: "t2".into(),
                }),
                Ok("success".into()),
            ],
        ));
        let retry = RetryProvider::with_config(mock.clone(), 3, Duration::from_millis(1));
        let config = CompletionConfig::default();
        let result = retry.complete("sys", "usr", &config).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
        assert_eq!(mock.call_count(), 3);
    }

    /// RetryProvider retries on ProviderError::Http with status 500.
    #[tokio::test]
    async fn test_retry_provider_retries_on_http_500() {
        let mock = Arc::new(MockProvider::with_responses(
            "p",
            "m",
            vec![
                Err(ProviderError::Http {
                    status: 500,
                    body: "err".into(),
                }),
                Ok("ok".into()),
            ],
        ));
        let retry = RetryProvider::with_config(mock.clone(), 3, Duration::from_millis(1));
        let config = CompletionConfig::default();
        let result = retry.complete("sys", "usr", &config).await;
        assert!(result.is_ok());
        assert_eq!(mock.call_count(), 2);
    }

    /// RetryProvider retries on ProviderError::Http with status 429.
    #[tokio::test]
    async fn test_retry_provider_retries_on_http_429() {
        let mock = Arc::new(MockProvider::with_responses(
            "p",
            "m",
            vec![
                Err(ProviderError::Http {
                    status: 429,
                    body: "rate limit".into(),
                }),
                Ok("ok".into()),
            ],
        ));
        let retry = RetryProvider::with_config(mock.clone(), 3, Duration::from_millis(1));
        let config = CompletionConfig::default();
        let result = retry.complete("sys", "usr", &config).await;
        assert!(result.is_ok());
        assert_eq!(mock.call_count(), 2);
    }

    /// RetryProvider retries on ProviderError::Network.
    #[tokio::test]
    async fn test_retry_provider_retries_on_network() {
        let mock = Arc::new(MockProvider::with_responses(
            "p",
            "m",
            vec![
                Err(ProviderError::Network {
                    message: "dns".into(),
                }),
                Ok("ok".into()),
            ],
        ));
        let retry = RetryProvider::with_config(mock.clone(), 3, Duration::from_millis(1));
        let config = CompletionConfig::default();
        let result = retry.complete("sys", "usr", &config).await;
        assert!(result.is_ok());
        assert_eq!(mock.call_count(), 2);
    }

    /// RetryProvider does NOT retry on ProviderError::Auth.
    #[tokio::test]
    async fn test_retry_provider_does_not_retry_on_auth() {
        let mock = Arc::new(MockProvider::with_responses(
            "p",
            "m",
            vec![Err(ProviderError::Auth {
                message: "bad key".into(),
            })],
        ));
        let retry = RetryProvider::with_config(mock.clone(), 3, Duration::from_millis(1));
        let config = CompletionConfig::default();
        let result = retry.complete("sys", "usr", &config).await;
        assert!(result.is_err());
        assert_eq!(mock.call_count(), 1);
    }

    /// RetryProvider does NOT retry on ProviderError::Process.
    #[tokio::test]
    async fn test_retry_provider_does_not_retry_on_process() {
        let mock = Arc::new(MockProvider::with_responses(
            "p",
            "m",
            vec![Err(ProviderError::Process {
                exit_code: Some(1),
                stderr: "fail".into(),
            })],
        ));
        let retry = RetryProvider::with_config(mock.clone(), 3, Duration::from_millis(1));
        let config = CompletionConfig::default();
        let result = retry.complete("sys", "usr", &config).await;
        assert!(result.is_err());
        assert_eq!(mock.call_count(), 1);
    }

    /// RetryProvider does NOT retry on ProviderError::NestedSession.
    #[tokio::test]
    async fn test_retry_provider_does_not_retry_on_nested_session() {
        let mock = Arc::new(MockProvider::with_responses(
            "p",
            "m",
            vec![Err(ProviderError::NestedSession)],
        ));
        let retry = RetryProvider::with_config(mock.clone(), 3, Duration::from_millis(1));
        let config = CompletionConfig::default();
        let result = retry.complete("sys", "usr", &config).await;
        assert!(result.is_err());
        assert_eq!(mock.call_count(), 1);
    }

    /// RetryProvider does NOT retry on ProviderError::Http with 4xx (except 429).
    #[tokio::test]
    async fn test_retry_provider_does_not_retry_on_http_4xx() {
        let mock = Arc::new(MockProvider::with_responses(
            "p",
            "m",
            vec![Err(ProviderError::Http {
                status: 403,
                body: "forbidden".into(),
            })],
        ));
        let retry = RetryProvider::with_config(mock.clone(), 3, Duration::from_millis(1));
        let config = CompletionConfig::default();
        let result = retry.complete("sys", "usr", &config).await;
        assert!(result.is_err());
        assert_eq!(mock.call_count(), 1);
    }

    /// RetryProvider returns last error after exhausting retries.
    #[tokio::test]
    async fn test_retry_provider_returns_last_error_after_exhausting_retries() {
        let mock = Arc::new(MockProvider::with_responses(
            "p",
            "m",
            vec![
                Err(ProviderError::Timeout {
                    message: "t1".into(),
                }),
                Err(ProviderError::Timeout {
                    message: "t2".into(),
                }),
                Err(ProviderError::Timeout {
                    message: "t3".into(),
                }),
            ],
        ));
        // max_retries=2 means 1 initial + 2 retries = 3 total attempts
        let retry = RetryProvider::with_config(mock.clone(), 2, Duration::from_millis(1));
        let config = CompletionConfig::default();
        let result = retry.complete("sys", "usr", &config).await;
        assert!(result.is_err());
        assert_eq!(mock.call_count(), 3);
        match result.unwrap_err() {
            ProviderError::Timeout { message } => assert_eq!(message, "t3"),
            other => panic!("expected Timeout, got: {other}"),
        }
    }

    /// RetryProvider returns success on first successful retry.
    #[tokio::test]
    async fn test_retry_provider_returns_success_on_first_retry() {
        let mock = Arc::new(MockProvider::with_responses(
            "p",
            "m",
            vec![
                Err(ProviderError::Timeout {
                    message: "t1".into(),
                }),
                Ok("recovered".into()),
            ],
        ));
        let retry = RetryProvider::with_config(mock.clone(), 3, Duration::from_millis(1));
        let config = CompletionConfig::default();
        let result = retry.complete("sys", "usr", &config).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "recovered");
        assert_eq!(mock.call_count(), 2);
    }

    /// RetryProvider default config: 3 retries, 1s delay.
    #[test]
    fn test_retry_provider_default_config() {
        let mock = Arc::new(MockProvider::new("p", "m"));
        let retry = RetryProvider::new(mock);
        assert_eq!(retry.max_retries, 3);
        assert_eq!(retry.base_delay, Duration::from_secs(1));
    }
}
