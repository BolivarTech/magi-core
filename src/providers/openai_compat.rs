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
use crate::provider::{CompletionConfig, DEFAULT_CLIENT_TIMEOUT, PARSE_FAILURE_STATUS};
use crate::providers::provider_url::ProviderUrl;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{Duration, Instant};

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

/// Top-level response from the OpenAI Chat Completions endpoint.
#[derive(Debug, Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
}

/// A single completion choice returned by the endpoint.
#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiRespMessage,
}

/// The assistant message inside a completion choice.
///
/// `content` is optional so that an **absent** or **null** field deserializes instead of failing
/// the whole parse with an opaque message: those cases mean "the server sent no content", which is
/// a schema failure worth naming, not a malformed document.
#[derive(Debug, Deserialize)]
struct OpenAiRespMessage {
    #[serde(default)]
    content: Option<String>,
}

/// Body text when the server sent no usable content.
const EMPTY_CONTENT_BODY: &str = "response content was empty or absent";

/// Body text when the response carried no choices at all.
const NO_CHOICES_BODY: &str = "no choices in response";

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
    client: reqwest::Client,
    /// The URL authority. **Never a `String`**: a secret must not be storable as plain text, so
    /// that no future `derive(Debug)`, `format!` or log statement can print it.
    base_url: ProviderUrl,
    model: String,
    api_key: Option<String>,
}

impl fmt::Debug for OpenAiCompatibleProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenAiCompatibleProvider")
            // `ProviderUrl`'s own Debug is redacted, so this field cannot leak.
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
        Self::with_timeout(base_url, model, api_key, DEFAULT_CLIENT_TIMEOUT)
    }

    /// Same as [`Self::new`] with an explicit **total** request timeout.
    ///
    /// The timeout covers the entire request, from send to the last body byte
    /// ([`reqwest::ClientBuilder::timeout`]) — this is what makes
    /// `ProviderError::Timeout` reachable against a model that hangs while
    /// generating. Pass `Duration::MAX` for "no timeout" (dangerous: a hung
    /// model would hang forever).
    pub fn with_timeout(
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key: Option<String>,
        timeout: Duration,
    ) -> Result<Self, ProviderError> {
        // Parsing, scheme validation and normalisation all live in the URL authority now.
        let base_url = ProviderUrl::parse(&base_url.into())?;
        Self::from_authority(base_url, model, api_key, timeout)
    }

    /// Builds a provider from an already-parsed URL authority.
    ///
    /// # Parameters
    /// - `base_url`: the authority, derived rather than re-parsed.
    /// - `model`, `api_key`, `timeout`: as in [`with_timeout`](Self::with_timeout).
    ///
    /// # Errors
    /// [`ProviderError::Network`] if the HTTP client cannot be built.
    ///
    /// # Why an in-crate provider must use this and not the string constructors
    ///
    /// Rendering an existing authority back to a string goes through `Display`, which is the
    /// **redacted** form: real credentials come back as the placeholder, and the normalising
    /// trailing slash doubles the separator. Passing the authority itself has neither failure mode
    /// available to it.
    pub(crate) fn from_authority(
        base_url: ProviderUrl,
        model: impl Into<String>,
        api_key: Option<String>,
        timeout: Duration,
    ) -> Result<Self, ProviderError> {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            // Referer OFF. The default puts the ORIGINAL url — query string included — in the
            // `Referer` header of a redirect, handing a query-authenticated credential to the
            // target origin, which is exactly the leak this module exists to prevent, and it
            // survives every other defense because it never touches our own rendering.
            // An LLM API client has no use for Referer.
            .referer(false)
            .build()
            .map_err(|e| crate::provider::client_build_error(&e))?;
        Ok(Self {
            client,
            base_url,
            model: model.into(),
            api_key,
        })
    }

    /// The URL authority this provider was built with.
    ///
    /// Test-only, and deliberately so. Its single purpose is letting a test **compare** the
    /// authority the Ollama provider handed over — which is how the credential-destroying
    /// `format!("{base}/v1")` was caught. Returning the authority rather than a string keeps the
    /// guarantee intact even here: a caller can compare or derive, never render.
    ///
    /// Not exposed outside tests because no production path needs to read a URL back; every use
    /// goes through the request builder.
    #[cfg(all(test, feature = "ollama"))]
    pub(crate) fn base(&self) -> &ProviderUrl {
        &self.base_url
    }

    /// Provider name for diagnostics/telemetry.
    pub fn name(&self) -> &str {
        "openai-compat"
    }

    /// Configured model identifier (pass-through; no alias resolution).
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Returns the `Authorization` header tuple when an API key is configured,
    /// or `None` for keyless endpoints (e.g., local Ollama).
    ///
    /// The first element is the static header name `"Authorization"`; the second
    /// is `"Bearer <key>"`.
    pub(crate) fn auth_header(&self) -> Option<(&'static str, String)> {
        self.api_key
            .as_ref()
            .map(|k| ("Authorization", format!("Bearer {k}")))
    }

    /// Extracts `choices[0].message.content` from the raw response body.
    ///
    /// A serde failure or an empty `choices` array maps to
    /// `ProviderError::Http { status: 0, .. }` — a deliberate sentinel:
    /// `status: 0` is never a real HTTP status, marks a parse/contract failure,
    /// and is non-retryable per `is_retryable`.
    pub(crate) fn parse_response(body: &str) -> Result<String, ProviderError> {
        let resp: OpenAiResponse = serde_json::from_str(body).map_err(|e| ProviderError::Http {
            status: PARSE_FAILURE_STATUS,
            body: format!(
                "failed to parse response: {}",
                crate::provider::describe_parse_error(&e)
            ),
            retry_after_raw: vec![],
            received_at: None,
        })?;
        let choice = resp
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| ProviderError::Http {
                status: PARSE_FAILURE_STATUS,
                body: NO_CHOICES_BODY.to_string(),
                retry_after_raw: vec![],
                received_at: None,
            })?;
        match choice.message.content.as_deref() {
            // Absent, null and empty all mean the same thing: the server sent no content. The
            // condition is written EXPLICITLY rather than letting the empty case fall through
            // some other path — otherwise the next refactor of this match breaks it in silence
            // and no test notices.
            None | Some("") => Err(ProviderError::Http {
                status: PARSE_FAILURE_STATUS,
                body: EMPTY_CONTENT_BODY.to_string(),
                retry_after_raw: vec![],
                received_at: None,
            }),
            Some(text) => Ok(text.to_owned()),
        }
    }

    /// Maps an HTTP status code to a [`ProviderError`].
    ///
    /// 401 / 403 → [`ProviderError::Auth`]; all other codes →
    /// [`ProviderError::Http`] (preserving `status`, `body`, and the raw
    /// `Retry-After` header for the retry policy to interpret).
    pub(crate) fn map_status_to_error(
        status: u16,
        body: &str,
        retry_after_raw: Vec<String>,
        received_at: Option<Instant>,
    ) -> ProviderError {
        match status {
            // 401/403 -> Auth: no header carried (not a rate-limit path).
            401 | 403 => ProviderError::Auth {
                message: body.to_string(),
            },
            _ => ProviderError::Http {
                status,
                body: body.to_string(),
                retry_after_raw,
                received_at,
            },
        }
    }

    /// Builds the JSON request body for the Chat Completions endpoint.
    ///
    /// Constructs a non-streaming [`OpenAiRequest`] with a two-message
    /// conversation: a `system` message followed by a `user` message.
    /// Token limit and temperature are taken from `config`.
    ///
    /// `pub(crate)` — consumed by `complete()`.
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

use crate::provider::LlmProvider;

#[async_trait::async_trait]
impl LlmProvider for OpenAiCompatibleProvider {
    /// Sends a chat completion request to the configured endpoint and returns
    /// the assistant's reply.
    ///
    /// # Errors
    /// - `Timeout` if the request exceeds the **total** client timeout (300 s by
    ///   default, or the value passed to [`Self::with_timeout`]) — it fires even
    ///   when the server returns headers and then hangs on the body.
    /// - `Network` on connection failures (and on a malformed `base_url`/client).
    /// - `Auth` on 401/403; `Http` on other non-2xx; `Http { status: 0 }` on a
    ///   malformed response body.
    async fn complete(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        config: &CompletionConfig,
    ) -> Result<String, ProviderError> {
        let body = self.build_request_body(system_prompt, user_prompt, config);
        let mut req = self
            .base_url
            .request(
                &self.client,
                reqwest::Method::POST,
                &["chat", "completions"],
            )
            .json(&body);
        if let Some((name, value)) = self.auth_header() {
            req = req.header(name, value);
        }
        // `send` returns an already-composed, redacted error: this file never builds one.
        let response = req.send().await?;
        // C3.1 epoch: capture the receipt instant when the HEADERS arrive (the
        // `send` future resolves on headers; the body is read below), and the raw
        // `Retry-After`, in the same place the status is read.
        let received_at = Instant::now();
        let status = response.status();
        let retry_after_raw = response.retry_after_raw();
        if !(200..300).contains(&status) {
            // Error branch: the body is DIAGNOSTIC text, so it truncates and says so. Dropping a
            // 500's body whole would discard the only reason that error gets read.
            let response_body = response.read_diagnostic_body().await;
            return Err(Self::map_status_to_error(
                status,
                &response_body,
                retry_after_raw,
                Some(received_at),
            ));
        }
        // Success branch: the body carries the VERDICT, so over the cap it fails rather than
        // truncating — a cut body loses its closing marker and the parser would blame the model
        // for a cut this reader made, with a retry that could never fix it.
        //
        // The total timeout can also fire while reading (headers arrive, then the server hangs);
        // the shared mapper classifies that as `Timeout`, not `Network`.
        let response_body = response.read_verdict_body(config.max_tokens).await?;
        Self::parse_response(&response_body)
    }

    fn name(&self) -> &str {
        "openai-compat"
    }

    fn model(&self) -> &str {
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

    #[test]
    fn test_auth_header_some_when_key_present() {
        let p = OpenAiCompatibleProvider::new("http://h/v1", "m", Some("sk-x".into())).unwrap();
        assert_eq!(
            p.auth_header(),
            Some(("Authorization", "Bearer sk-x".to_string()))
        );
    }

    #[test]
    fn test_auth_header_none_when_key_absent() {
        let p = OpenAiCompatibleProvider::new("http://h/v1", "m", None).unwrap();
        assert_eq!(p.auth_header(), None);
    }

    #[test]
    fn absent_null_and_empty_content_are_all_a_named_schema_failure() {
        for body in [
            r#"{"choices":[{"message":{}}]}"#,
            r#"{"choices":[{"message":{"content":null}}]}"#,
            r#"{"choices":[{"message":{"content":""}}]}"#,
        ] {
            let err = OpenAiCompatibleProvider::parse_response(body)
                .expect_err("no content is a failure");
            assert!(
                matches!(err, ProviderError::Http { status: 0, .. }),
                "same sentinel as other unusable responses: {err:?}"
            );
            assert!(
                err.to_string().contains("empty or absent"),
                "and it says WHAT was wrong, not 'failed to parse': {err}"
            );
        }
    }

    #[test]
    fn content_with_text_is_returned() {
        let body = r#"{"choices":[{"message":{"content":"hello"}}]}"#;
        assert_eq!(
            OpenAiCompatibleProvider::parse_response(body).expect("parses"),
            "hello"
        );
    }

    // Endpoint construction moved to the URL authority, and its coverage GREW rather than
    // shrank: the two tests that lived here (append, trailing slash) are now three, adding
    // query and fragment preservation — cases the old string-concatenation implementation
    // could not even express, since it produced `…/v1?key=X/chat/completions`.
    // See `provider_url::tests::join_path_*`.

    #[test]
    fn test_parse_response_extracts_content() {
        let json = r#"{"choices":[{"message":{"role":"assistant","content":"hola"}}]}"#;
        assert_eq!(
            OpenAiCompatibleProvider::parse_response(json).unwrap(),
            "hola"
        );
    }

    #[test]
    fn test_parse_response_takes_first_choice() {
        let json = r#"{"choices":[{"message":{"content":"a"}},{"message":{"content":"b"}}]}"#;
        assert_eq!(OpenAiCompatibleProvider::parse_response(json).unwrap(), "a");
    }

    #[test]
    fn test_parse_response_empty_choices_is_http_zero() {
        let json = r#"{"choices":[]}"#;
        assert!(matches!(
            OpenAiCompatibleProvider::parse_response(json),
            Err(ProviderError::Http { status: 0, .. })
        ));
    }

    #[test]
    fn test_parse_response_bad_json_is_http_zero() {
        assert!(matches!(
            OpenAiCompatibleProvider::parse_response("not json"),
            Err(ProviderError::Http { status: 0, .. })
        ));
    }

    #[test]
    fn test_map_status_401_403_to_auth() {
        assert!(matches!(
            OpenAiCompatibleProvider::map_status_to_error(401, "x", vec![], None),
            ProviderError::Auth { .. }
        ));
        assert!(matches!(
            OpenAiCompatibleProvider::map_status_to_error(403, "x", vec![], None),
            ProviderError::Auth { .. }
        ));
    }

    #[test]
    fn test_map_status_429_500_404_to_http() {
        for s in [429u16, 500, 404] {
            match OpenAiCompatibleProvider::map_status_to_error(s, "b", vec![], None) {
                ProviderError::Http { status, body, .. } => {
                    assert_eq!(status, s);
                    assert_eq!(body, "b");
                }
                other => panic!("expected Http for {s}, got {other}"),
            }
        }
    }

    #[tokio::test]
    async fn test_usable_as_dyn_llm_provider() {
        use crate::provider::LlmProvider;
        use std::sync::Arc;
        let p: Arc<dyn LlmProvider> =
            Arc::new(OpenAiCompatibleProvider::new("http://h/v1", "phi4-mini", None).unwrap());
        assert_eq!(p.name(), "openai-compat");
        assert_eq!(p.model(), "phi4-mini");
    }
}
