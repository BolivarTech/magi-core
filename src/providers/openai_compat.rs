// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-05-25

//! OpenAI Chat Completions-compatible provider.
//!
//! Implements [`OpenAiCompatibleProvider`], which speaks the OpenAI Chat
//! Completions wire format (`POST /chat/completions`). A single configurable
//! `base_url` makes the provider work with:
//!
//! - **OpenAI cloud** (`https://api.openai.com/v1`) — pass `api_key`.
//! - **Ollama** (`http://localhost:11434/v1`) — `api_key = None`.
//! - **LocalAI / vLLM / LM Studio** — any http/https base URL.
//!
//! Feature-gated behind `openai-compat`; pulls in `reqwest` as an optional
//! dependency (shared with `claude-api`).

use crate::error::ProviderError;
use crate::provider::CompletionConfig;
use serde::Serialize;
use std::fmt;

/// HTTP request body for the OpenAI Chat Completions endpoint
/// (`POST /chat/completions`). Non-streaming; no `stream` field.
///
/// `pub(crate)` — internal HTTP plumbing, not part of the public contract.
#[derive(Debug, Serialize)]
pub(crate) struct OpenAiRequest {
    pub(crate) model: String,
    pub(crate) messages: Vec<OpenAiMessage>,
    pub(crate) max_tokens: u32,
    pub(crate) temperature: f64,
}

/// A single message in the OpenAI Chat Completions `messages` array.
///
/// `pub(crate)` — internal HTTP plumbing, not part of the public contract.
#[derive(Debug, Serialize)]
pub(crate) struct OpenAiMessage {
    pub(crate) role: String,
    pub(crate) content: String,
}

/// LLM provider for any endpoint that speaks the OpenAI Chat Completions wire
/// format.
///
/// Uses `reqwest::Client` for connection pooling — a single client is created
/// at construction time and reused across all requests.
///
/// Feature-gated behind `openai-compat`.
///
/// # Examples
///
/// ```no_run
/// use magi_core::providers::openai_compat::OpenAiCompatibleProvider;
///
/// // Local Ollama (no API key)
/// let local = OpenAiCompatibleProvider::new("http://localhost:11434/v1", "phi4-mini", None)
///     .expect("valid url");
///
/// // OpenAI cloud
/// let cloud = OpenAiCompatibleProvider::new(
///     "https://api.openai.com/v1",
///     "gpt-4o",
///     Some("sk-...".into()),
/// )
/// .expect("valid url");
/// ```
pub struct OpenAiCompatibleProvider {
    #[allow(dead_code)] // used by complete() in Task 6
    client: reqwest::Client,
    base_url: String,
    model: String,
    #[allow(dead_code)] // used by auth_header()/complete() in Tasks 3/6
    api_key: Option<String>,
}

impl fmt::Debug for OpenAiCompatibleProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenAiCompatibleProvider")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

impl OpenAiCompatibleProvider {
    /// Creates a provider for any OpenAI-compatible endpoint. `base_url` is
    /// validated eagerly (`reqwest::Url`, scheme restricted to http/https) and
    /// normalized (trailing `/` stripped); an invalid URL or scheme returns
    /// `ProviderError::Network`. `api_key = None` omits the `Authorization`
    /// header (e.g., Ollama).
    pub fn new(
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key: Option<String>,
    ) -> Result<Self, ProviderError> {
        let base_url = base_url.into();
        let parsed = reqwest::Url::parse(&base_url).map_err(|e| ProviderError::Network {
            message: format!("invalid base_url: {e}"),
        })?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(ProviderError::Network {
                message: format!(
                    "invalid base_url scheme: {} (expected http/https)",
                    parsed.scheme()
                ),
            });
        }
        let client = reqwest::Client::builder()
            .build()
            .map_err(|e| ProviderError::Network {
                message: format!("failed to build HTTP client: {e}"),
            })?;
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.into(),
            api_key,
        })
    }

    /// Provider name for diagnostics/telemetry.
    pub fn name(&self) -> &str {
        "openai-compat"
    }

    /// Configured model identifier (pass-through; no alias resolution).
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Builds the JSON request body for the Chat Completions endpoint.
    ///
    /// Constructs a non-streaming [`OpenAiRequest`] with a two-message
    /// conversation: a `system` message followed by a `user` message.
    /// Token limit and temperature are taken from `config`.
    ///
    /// `pub(crate)` — consumed by `complete()` in Task 6.
    #[allow(dead_code)] // consumed by complete() in Task 6
    pub(crate) fn build_request_body(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        config: &CompletionConfig,
    ) -> OpenAiRequest {
        OpenAiRequest {
            model: self.model.clone(),
            messages: vec![
                OpenAiMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                OpenAiMessage {
                    role: "user".to_string(),
                    content: user_prompt.to_string(),
                },
            ],
            max_tokens: config.max_tokens,
            temperature: config.temperature,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_valid_url_ok_and_model_passthrough() {
        let p = OpenAiCompatibleProvider::new("http://127.0.0.1:11434/v1", "phi4-mini", None)
            .expect("valid url constructs");
        assert_eq!(p.model(), "phi4-mini");
        assert_eq!(p.name(), "openai-compat");
    }

    #[test]
    fn test_new_invalid_url_returns_network_error() {
        let result = OpenAiCompatibleProvider::new("notaurl", "m", None);
        assert!(matches!(result, Err(ProviderError::Network { .. })));
    }

    #[test]
    fn test_new_non_http_scheme_returns_network_error() {
        let result = OpenAiCompatibleProvider::new("file:///etc/passwd", "m", None);
        assert!(matches!(result, Err(ProviderError::Network { .. })));
    }

    #[test]
    fn test_new_cloud_with_key_passthrough() {
        let p = OpenAiCompatibleProvider::new(
            "https://api.openai.com/v1",
            "gpt-4o",
            Some("sk-x".into()),
        )
        .expect("valid url constructs");
        assert_eq!(p.model(), "gpt-4o");
    }

    #[test]
    fn test_debug_redacts_api_key() {
        let p = OpenAiCompatibleProvider::new("http://h/v1", "m", Some("sk-super-secret".into()))
            .expect("constructs");
        let dbg = format!("{p:?}");
        assert!(
            !dbg.contains("sk-super-secret"),
            "Debug must not leak key, got: {dbg}"
        );
    }

    #[test]
    fn test_build_request_body_shape() {
        let p = OpenAiCompatibleProvider::new("http://h/v1", "phi4-mini", None).unwrap();
        let cfg = CompletionConfig::default();
        let body = p.build_request_body("S", "U", &cfg);
        assert_eq!(body.model, "phi4-mini");
        assert_eq!(body.max_tokens, 4096);
        assert!((body.temperature - 0.0).abs() < f64::EPSILON);
        assert_eq!(body.messages.len(), 2);
        assert_eq!(body.messages[0].role, "system");
        assert_eq!(body.messages[0].content, "S");
        assert_eq!(body.messages[1].role, "user");
        assert_eq!(body.messages[1].content, "U");
    }

    #[test]
    fn test_build_request_body_has_no_stream_field() {
        let p = OpenAiCompatibleProvider::new("http://h/v1", "m", None).unwrap();
        let body = p.build_request_body("S", "U", &CompletionConfig::default());
        let json = serde_json::to_string(&body).unwrap();
        assert!(
            !json.contains("stream"),
            "request must not carry a stream field"
        );
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)] // CompletionConfig is #[non_exhaustive]; struct literal unavailable
    fn test_build_request_body_carries_config_values() {
        let p = OpenAiCompatibleProvider::new("http://h/v1", "m", None).unwrap();
        let mut cfg = CompletionConfig::default();
        cfg.max_tokens = 256;
        cfg.temperature = 0.7;
        let body = p.build_request_body("S", "U", &cfg);
        assert_eq!(body.max_tokens, 256);
        assert!((body.temperature - 0.7).abs() < f64::EPSILON);
    }
}
