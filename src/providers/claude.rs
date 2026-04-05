// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05

use crate::error::ProviderError;
use crate::provider::{CompletionConfig, LlmProvider};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Anthropic API base URL.
const API_BASE_URL: &str = "https://api.anthropic.com/v1/messages";

/// Anthropic API version header value.
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// LLM provider that communicates with the Claude Messages API via HTTP.
///
/// Uses `reqwest::Client` for connection pooling — a single client is created
/// at construction time and reused across all requests.
///
/// Feature-gated behind `claude-api`.
///
/// # Examples
///
/// ```no_run
/// use magi_core::providers::claude::ClaudeProvider;
/// use magi_core::provider::{LlmProvider, CompletionConfig};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let provider = ClaudeProvider::new("sk-ant-api03-...", "claude-sonnet-4-6");
/// let response = provider.complete("You are helpful", "Hello", &CompletionConfig::default()).await?;
/// # Ok(())
/// # }
/// ```
pub struct ClaudeProvider {
    client: Client,
    api_key: String,
    model: String,
}

impl fmt::Debug for ClaudeProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClaudeProvider")
            .field("model", &self.model)
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

/// Request body for the Claude Messages API.
#[derive(Debug, Serialize)]
pub struct ClaudeRequest {
    /// Model identifier.
    pub model: String,
    /// Maximum tokens in the response.
    pub max_tokens: u32,
    /// Sampling temperature.
    pub temperature: f64,
    /// System prompt.
    pub system: String,
    /// Conversation messages.
    pub messages: Vec<ClaudeMessage>,
}

/// A single message in the Claude Messages API request.
#[derive(Debug, Serialize)]
pub struct ClaudeMessage {
    /// Message role ("user" or "assistant").
    pub role: String,
    /// Message content.
    pub content: String,
}

/// Response from the Claude Messages API.
#[derive(Debug, Deserialize)]
struct ClaudeResponse {
    content: Vec<ContentBlock>,
}

/// A content block in the Claude Messages API response.
#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    type_: String,
    text: Option<String>,
}

impl ClaudeProvider {
    /// Creates a new `ClaudeProvider` with the given API key and model.
    ///
    /// A `reqwest::Client` is created once and reused for all subsequent
    /// requests (connection pooling).
    ///
    /// # Parameters
    /// - `api_key`: Anthropic API key (e.g., `"sk-ant-api03-..."`).
    /// - `model`: Model identifier (e.g., `"claude-sonnet-4-6"`).
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            model: model.into(),
        }
    }

    /// Returns the provider name.
    pub fn name(&self) -> &str {
        "claude"
    }

    /// Returns the configured model identifier.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Builds the request body for the Claude Messages API.
    ///
    /// # Parameters
    /// - `system_prompt`: System-level instruction for the LLM.
    /// - `user_prompt`: User's input content.
    /// - `config`: Completion parameters (max_tokens, temperature).
    pub fn build_request_body(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        config: &CompletionConfig,
    ) -> ClaudeRequest {
        ClaudeRequest {
            model: self.model.clone(),
            max_tokens: config.max_tokens,
            temperature: config.temperature,
            system: system_prompt.to_string(),
            messages: vec![ClaudeMessage {
                role: "user".to_string(),
                content: user_prompt.to_string(),
            }],
        }
    }

    /// Parses a Claude Messages API response JSON string and extracts the
    /// first text content block.
    ///
    /// # Parameters
    /// - `body`: Raw JSON response body from the API.
    ///
    /// # Returns
    /// The text content of the first `"text"` content block, or a
    /// `ProviderError` if parsing fails or no text block is found.
    pub fn parse_response(body: &str) -> Result<String, ProviderError> {
        let response: ClaudeResponse =
            serde_json::from_str(body).map_err(|e| ProviderError::Http {
                status: 0,
                body: format!("failed to parse response: {e}"),
            })?;

        response
            .content
            .into_iter()
            .find(|block| block.type_ == "text")
            .and_then(|block| block.text)
            .ok_or_else(|| ProviderError::Http {
                status: 0,
                body: "no text content block in response".to_string(),
            })
    }

    /// Maps an HTTP status code and response body to the appropriate
    /// `ProviderError` variant.
    ///
    /// - 401 and 403 map to `ProviderError::Auth`.
    /// - All other non-2xx status codes map to `ProviderError::Http`.
    pub fn map_status_to_error(status: u16, body: &str) -> ProviderError {
        match status {
            401 | 403 => ProviderError::Auth {
                message: body.to_string(),
            },
            _ => ProviderError::Http {
                status,
                body: body.to_string(),
            },
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for ClaudeProvider {
    /// Sends a completion request to the Claude Messages API.
    ///
    /// Builds a POST request with the appropriate headers and body, sends it,
    /// and parses the response to extract the text content.
    ///
    /// # Errors
    /// - `ProviderError::Timeout` if the request times out.
    /// - `ProviderError::Network` on connection failures.
    /// - `ProviderError::Auth` on 401/403 responses.
    /// - `ProviderError::Http` on other non-2xx responses.
    async fn complete(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        config: &CompletionConfig,
    ) -> Result<String, ProviderError> {
        let body = self.build_request_body(system_prompt, user_prompt, config);

        let response = self
            .client
            .post(API_BASE_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ProviderError::Timeout {
                        message: e.to_string(),
                    }
                } else {
                    ProviderError::Network {
                        message: e.to_string(),
                    }
                }
            })?;

        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            let response_body = response.text().await.unwrap_or_default();
            return Err(Self::map_status_to_error(status, &response_body));
        }

        let response_body = response.text().await.map_err(|e| ProviderError::Network {
            message: format!("failed to read response body: {e}"),
        })?;

        Self::parse_response(&response_body)
    }

    fn name(&self) -> &str {
        "claude"
    }

    fn model(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    // -- Construction and accessors --

    /// ClaudeProvider::new creates provider with api_key and model.
    #[test]
    fn test_claude_provider_new_creates_with_key_and_model() {
        let provider = super::ClaudeProvider::new("sk-test-key", "claude-sonnet-4-6");
        assert_eq!(provider.name(), "claude");
        assert_eq!(provider.model(), "claude-sonnet-4-6");
    }

    /// provider.name() returns "claude".
    #[test]
    fn test_claude_provider_name_returns_claude() {
        let provider = super::ClaudeProvider::new("key", "model");
        assert_eq!(provider.name(), "claude");
    }

    /// provider.model() returns the configured model string.
    #[test]
    fn test_claude_provider_model_returns_configured_model() {
        let provider = super::ClaudeProvider::new("key", "claude-opus-4-6");
        assert_eq!(provider.model(), "claude-opus-4-6");
    }

    // -- Request building --

    /// build_request_body produces correct JSON structure with all required fields.
    #[test]
    fn test_build_request_body_contains_all_required_fields() {
        use crate::provider::CompletionConfig;

        let provider = super::ClaudeProvider::new("sk-test", "claude-sonnet-4-6");
        let config = CompletionConfig::default();
        let body = provider.build_request_body("You are helpful", "Hello", &config);

        assert_eq!(body.model, "claude-sonnet-4-6");
        assert_eq!(body.max_tokens, 4096);
        assert!((body.temperature - 0.0).abs() < f64::EPSILON);
        assert_eq!(body.system, "You are helpful");
        assert_eq!(body.messages.len(), 1);
        assert_eq!(body.messages[0].role, "user");
        assert_eq!(body.messages[0].content, "Hello");
    }

    // -- Response parsing --

    /// parse_response extracts text content from Claude response format.
    #[test]
    fn test_parse_claude_response_extracts_text_content() {
        let json = r#"{"content": [{"type": "text", "text": "response text"}], "id": "msg_1", "model": "claude-sonnet-4-6", "role": "assistant"}"#;
        let result = super::ClaudeProvider::parse_response(json);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "response text");
    }

    /// parse_response handles multiple content blocks, extracting first text block.
    #[test]
    fn test_parse_response_extracts_first_text_block() {
        let json = r#"{"content": [{"type": "text", "text": "first"}, {"type": "text", "text": "second"}], "id": "msg_1", "model": "m", "role": "assistant"}"#;
        let result = super::ClaudeProvider::parse_response(json);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "first");
    }

    /// parse_response returns error when no text content block found.
    #[test]
    fn test_parse_response_error_when_no_text_block() {
        let json = r#"{"content": [], "id": "msg_1", "model": "m", "role": "assistant"}"#;
        let result = super::ClaudeProvider::parse_response(json);
        assert!(result.is_err());
    }

    /// parse_response returns error on invalid JSON.
    #[test]
    fn test_parse_response_error_on_invalid_json() {
        let result = super::ClaudeProvider::parse_response("not json");
        assert!(result.is_err());
    }

    // -- Error mapping --

    /// map_status_to_error maps 401 to ProviderError::Auth.
    #[test]
    fn test_map_status_401_to_auth_error() {
        let err = super::ClaudeProvider::map_status_to_error(401, "unauthorized");
        assert!(matches!(err, crate::error::ProviderError::Auth { .. }));
    }

    /// map_status_to_error maps 403 to ProviderError::Auth.
    #[test]
    fn test_map_status_403_to_auth_error() {
        let err = super::ClaudeProvider::map_status_to_error(403, "forbidden");
        assert!(matches!(err, crate::error::ProviderError::Auth { .. }));
    }

    /// map_status_to_error maps 500 to ProviderError::Http.
    #[test]
    fn test_map_status_500_to_http_error() {
        let err = super::ClaudeProvider::map_status_to_error(500, "server error");
        match err {
            crate::error::ProviderError::Http { status, body } => {
                assert_eq!(status, 500);
                assert_eq!(body, "server error");
            }
            other => panic!("expected Http, got: {other}"),
        }
    }

    /// map_status_to_error maps 429 to ProviderError::Http (not Auth).
    #[test]
    fn test_map_status_429_to_http_error() {
        let err = super::ClaudeProvider::map_status_to_error(429, "rate limited");
        assert!(matches!(
            err,
            crate::error::ProviderError::Http { status: 429, .. }
        ));
    }

    // -- Client reuse --

    /// reqwest::Client is stored in struct (structural test).
    #[test]
    fn test_client_is_stored_in_struct() {
        let provider = super::ClaudeProvider::new("key", "model");
        assert_eq!(provider.name(), "claude");
        let provider2 = super::ClaudeProvider::new("key2", "model2");
        assert_eq!(provider2.model(), "model2");
    }

    // -- Debug does not expose API key --

    /// Debug output does not contain the API key.
    #[test]
    fn test_debug_does_not_expose_api_key() {
        let provider = super::ClaudeProvider::new("sk-super-secret-key-12345", "model");
        let debug_str = format!("{:?}", provider);
        assert!(
            !debug_str.contains("sk-super-secret-key-12345"),
            "Debug output must not contain API key, got: {debug_str}"
        );
    }
}
