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
use std::fmt;

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
}
