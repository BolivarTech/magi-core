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

use crate::error::{ProviderError, ResponseContractCause};
use crate::provider::{
    Completion, CompletionConfig, CompletionTelemetry, DEFAULT_CLIENT_TIMEOUT, FinishReason,
    ReasoningControl, ReasoningState,
};
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
///
/// Every field a backend may omit is `#[serde(default)]`: this wire format is spoken by OpenAI
/// cloud, LocalAI, vLLM, LM Studio and llama.cpp-server, and they do not agree on which optional
/// members they send. A server that omits one must still parse.
#[derive(Debug, Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
}

/// A single completion choice returned by the endpoint.
#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiRespMessage,
    /// Why the model stopped. Until `4.0.0` this field was **not deserialized at all**, and that
    /// single omission is the head of the causal chain this release exists for: without it, "the
    /// model burned its whole budget reasoning" and "the server sent nothing" are the same opaque
    /// error, and the operator goes looking at a network that answered HTTP 200 perfectly.
    #[serde(default)]
    finish_reason: Option<FinishReason>,
}

/// The assistant message inside a completion choice.
///
/// `content` is optional so that an **absent** or **null** field deserializes instead of failing
/// the whole parse with an opaque message: those cases mean "the server sent no content", which is
/// a contract failure worth naming, not a malformed document.
#[derive(Debug, Deserialize)]
struct OpenAiRespMessage {
    #[serde(default)]
    content: Option<String>,
    /// The provider-side reasoning channel, under either spelling in the wild.
    ///
    /// `Option<String>`, not `String`: absent and empty are different claims, and collapsing them
    /// would report that the model did not reason when the backend simply did not say.
    #[serde(default, alias = "reasoning_content")]
    reasoning: Option<String>,
}

/// The token counters, when the backend keeps them.
///
/// Both `Option`: a compatible server that reports a partial `usage` object must not force a zero,
/// and a zero meaning "nobody counted" is indistinguishable from a real one.
#[derive(Debug, Deserialize, Default)]
struct OpenAiUsage {
    #[serde(default)]
    completion_tokens: Option<u32>,
    #[serde(default)]
    prompt_tokens: Option<u32>,
}

impl OpenAiResponse {
    /// Converts a parsed response into a [`Completion`], or into the contract failure it turned
    /// out to be.
    ///
    /// # Parameters
    ///
    /// - `cap` — the output budget in force, carried into the error so an empty completion names
    ///   the number that cut it instead of leaving the caller to guess.
    /// - `trace` — whether the consumer opted in to carrying the reasoning text itself. The
    ///   **length** travels either way; the flag is additive, never substitutive.
    /// - `control` — the reasoning control the consumer asked for. This wire has no way to honour
    ///   `Disabled`, so it is **declared**, never silently ignored.
    ///
    /// # Errors
    ///
    /// - [`ProviderError::ResponseContract`] with [`ResponseContractCause::NoMessage`] when the
    ///   body parsed but carried no choice to read.
    /// - [`ProviderError::EmptyCompletion`] when the choice carried no usable content.
    ///
    /// # Complexity
    ///
    /// O(n) in the length of the reasoning trace, which is counted once for its character length.
    fn into_completion(
        self,
        cap: u32,
        trace: bool,
        control: ReasoningControl,
    ) -> Result<Completion, ProviderError> {
        let usage = self.usage.unwrap_or_default();
        let choice = self
            .choices
            .into_iter()
            .next()
            .ok_or(ProviderError::ResponseContract {
                reason: ResponseContractCause::NoMessage,
                detail: String::new(),
            })?;
        let OpenAiChoice {
            message,
            finish_reason,
        } = choice;
        let OpenAiRespMessage { content, reasoning } = message;
        let content = content.unwrap_or_default();

        // Assembled BEFORE the empty-content branch, deliberately. `resp-C.json` -- the capture
        // this milestone is named after -- is an EMPTY completion whose `reasoning` field holds
        // the whole budget the model burned. Building the telemetry after the branch dropped
        // exactly that, so the report named the cut and lost its cause.

        // ORDER MATTERS: the CONTROL resolves before the trace. A provider that was asked for
        // `Disabled` and cannot honour it declares `Unsupported` even when the body carries
        // `reasoning` — in fact ESPECIALLY then, because reasoning coming back is the proof the
        // control had no effect. Resolving it the other way round overwrote that declaration in
        // silence, which is the whole failure mode C-8 exists to prevent.
        let reasoning = match (control, reasoning) {
            // The control could not be honoured — and the trace that came back anyway is the
            // PROOF of that, so its size travels with the declaration instead of being dropped.
            // Discarding it was the quiet half of the same failure the declaration exists to
            // prevent: the consumer learns the control did not take, and not how much it cost.
            (ReasoningControl::Disabled, seen) => ReasoningState::Unsupported {
                backend: COMPAT_BACKEND_NAME.to_string(),
                // Absent field means the body never reported the channel; only a
                // PRESENT one is a measurement, even when it is empty.
                chars: seen.as_ref().map(|s| s.chars().count()),
                text: trace.then_some(seen).flatten(),
            },
            (ReasoningControl::Default, Some(s)) => ReasoningState::Measured {
                chars: s.chars().count(),
                text: trace.then_some(s),
            },
            // Nothing came back on the channel, so nothing was measured — NOT
            // `Measured { chars: 0 }`, which would assert a look that did happen and saw zero.
            (ReasoningControl::Default, None) => ReasoningState::NotMeasured,
        };

        let mut telemetry = CompletionTelemetry::unmeasured().with_reasoning(reasoning);
        if let Some(f) = finish_reason.clone() {
            telemetry = telemetry.with_finish(f);
        }
        if let Some(n) = usage.completion_tokens {
            telemetry = telemetry.with_completion_tokens(n);
        }
        if let Some(n) = usage.prompt_tokens {
            telemetry = telemetry.with_prompt_tokens(n);
        }

        if content.trim().is_empty() {
            // Absent, null, empty and blank all mean the same thing: the server sent no content.
            //
            // `NoGeneration` is deliberately NOT considered here. Its discriminant is a set of
            // NATIVE wire fields that do not exist in this format, so there is no way to assert
            // the defect is ours — and this crate takes the reversible route when it cannot tell.
            // Aborting a run on a guess is the failure this release exists to stop making.
            return Err(ProviderError::EmptyCompletion { telemetry, cap });
        }
        Ok(Completion::new(content).with_telemetry(telemetry))
    }
}

/// The backend name this provider reports when it cannot honour a reasoning control.
const COMPAT_BACKEND_NAME: &str = "openai-compatible";

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
    /// - `Auth` on 401/403; `Http` on any other non-2xx — and `Http` now carries
    ///   **only real HTTP statuses**. The synthetic `Http { status: 0 }` this method
    ///   used to return for a malformed body is gone: a contract failure wearing an
    ///   HTTP error's clothes inherited run-wide lineage condemnation, which is the
    ///   defect `4.0.0` is named for.
    /// - [`ProviderError::ResponseTooLarge`] when the body exceeds the cap derived from
    ///   `max_tokens`. It fails rather than truncating: a cut body loses its closing marker.
    /// - [`ProviderError::ResponseContract`] when the response did not meet the contract —
    ///   `Unreadable` for a body serde cannot parse, `NoMessage` when no choice carries a
    ///   message, `RedirectRefused` when the transport gave up following redirects, in which
    ///   case no body was received at all.
    /// - [`ProviderError::EmptyCompletion`] when the model produced no usable content,
    ///   carrying the termination reason and the budget that was in force. Both are
    ///   **mage-local**: the endpoint answered, so no lineage is condemned run-wide.
    async fn complete(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        config: &CompletionConfig,
    ) -> Result<Completion, ProviderError> {
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
        // One parse, one place that decides. The conversion owns the whole reading of this wire —
        // the termination reason, the counters and the reasoning channel — so that nothing is
        // decided twice in two spots that can drift apart.
        let parsed: OpenAiResponse =
            serde_json::from_str(&response_body).map_err(|_| ProviderError::ResponseContract {
                reason: ResponseContractCause::Unreadable,
                detail: String::new(),
            })?;
        parsed.into_completion(config.max_tokens, config.reasoning_trace, config.reasoning)
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

    /// `chars` and `text` cannot disagree on this wire either.
    ///
    /// The Anthropic provider let them diverge because a boolean decided one and a string the
    /// other; the fix made both come from a single value. This wire already had that shape —
    /// both derive from `seen` — but "already correct" and "verified" are different claims, and
    /// a reviewer asking which one this was is asking the right question.
    #[test]
    fn the_two_reasoning_fields_cannot_disagree_on_the_compat_wire() {
        // `seen` present-and-empty: BOTH say present-and-empty.
        let body = r#"{"choices":[{"message":{"content":"x","reasoning":""},
                                   "finish_reason":"stop"}]}"#;
        let r: OpenAiResponse = serde_json::from_str(body).expect("parses");
        let out = r
            .into_completion(4096, true, ReasoningControl::Disabled)
            .expect("content makes this a success");
        match out.telemetry.reasoning {
            ReasoningState::Unsupported { chars, text, .. } => {
                assert_eq!(chars, Some(0), "an empty-but-present channel WAS measured");
                assert_eq!(
                    text.as_deref(),
                    Some(""),
                    "and its text agrees with its count"
                );
            }
            other => panic!("expected Unsupported, got {other:?}"),
        }

        // `seen` ABSENT: both say nothing was measured.
        let bare = r#"{"choices":[{"message":{"content":"x"},"finish_reason":"stop"}]}"#;
        let r: OpenAiResponse = serde_json::from_str(bare).expect("parses");
        let out = r
            .into_completion(4096, true, ReasoningControl::Disabled)
            .expect("content makes this a success");
        match out.telemetry.reasoning {
            ReasoningState::Unsupported { chars, text, .. } => {
                assert_eq!(chars, None, "an absent field is not a measurement");
                assert_eq!(text, None, "and there is no text to offer for it");
            }
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    /// C-9. This provider must never acquire the native routing that
    /// `OllamaProvider` uses — it stays the documented path for OpenAI cloud,
    /// LocalAI, vLLM, LM Studio and llama.cpp-server (ADR 006). The only thing
    /// Task 11 adds here is C-8's declaration.
    ///
    /// Scans only the PRODUCTION half of this file, split at the `#[cfg(test)]`
    /// marker that opens this very module: `include_str!` embeds the whole
    /// file, test source included, and this assertion's own literal would
    /// otherwise make the file "contain" the needle it is checking for.
    #[test]
    fn the_openai_compatible_provider_never_acquires_native_routing() {
        let production =
            crate::provider::source_scan::production_half(include_str!("openai_compat.rs"));
        // BOTH spellings, because the crate does not use the one this guard used to check.
        // `OllamaProvider` addresses the native endpoint as a SEGMENT LIST -- `&["api",
        // "chat"]` (ollama.rs) -- so the literal `/api/chat` never appears in routing code and
        // a copy of that idiom into this file would have walked straight past the assertion.
        // A guard that reports success while guarding nothing is the defect class this
        // milestone has already paid for nine times.
        assert!(
            !production.contains("/api/chat"),
            "the compat provider must keep speaking the OpenAI wire format only"
        );
        assert!(
            !production.contains("\"api\""),
            "no native path segment may appear here: the crate routes with segment lists, so this is the spelling a real regression would wear"
        );
    }

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
        assert_eq!(body.max_tokens, 16_384);
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

    // The reading of this wire moved out of a `parse_response` that returned a bare `String` and
    // into `OpenAiResponse::into_completion`, which returns the telemetry with it. The tests below
    // moved with it rather than being dropped: what they pin — first choice wins, text comes back
    // whole, an unusable body names its own cause — is unchanged, only the door is.

    #[test]
    fn the_named_cause_travels_with_an_unusable_body() {
        for body in [
            r#"{"choices":[{"message":{}}]}"#,
            r#"{"choices":[{"message":{"content":null}}]}"#,
            r#"{"choices":[{"message":{"content":""}}]}"#,
        ] {
            let r: OpenAiResponse = serde_json::from_str(body).expect("valid JSON");
            let err = r
                .into_completion(4096, false, ReasoningControl::Default)
                .expect_err("no content is a failure");
            assert!(
                err.to_string().contains("no content"),
                "it says WHAT was wrong, not 'failed to parse': {err}"
            );
            assert!(
                err.to_string().contains("4096"),
                "and it names the budget in force, which is the actionable half: {err}"
            );
        }
    }

    #[test]
    fn content_with_text_is_returned() {
        let body = r#"{"choices":[{"message":{"role":"assistant","content":"hello"}}]}"#;
        let r: OpenAiResponse = serde_json::from_str(body).expect("valid JSON");
        let c = r
            .into_completion(4096, false, ReasoningControl::Default)
            .expect("parses");
        assert_eq!(c.text, "hello");
    }

    // Endpoint construction moved to the URL authority, and its coverage GREW rather than
    // shrank: the two tests that lived here (append, trailing slash) are now three, adding
    // query and fragment preservation — cases the old string-concatenation implementation
    // could not even express, since it produced `…/v1?key=X/chat/completions`.
    // See `provider_url::tests::join_path_*`.

    #[test]
    fn the_first_choice_is_the_one_that_is_read() {
        let json = r#"{"choices":[{"message":{"content":"a"}},{"message":{"content":"b"}}]}"#;
        let r: OpenAiResponse = serde_json::from_str(json).expect("valid JSON");
        let c = r
            .into_completion(4096, false, ReasoningControl::Default)
            .expect("parses");
        assert_eq!(c.text, "a");
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
    // ---------------------------------------------------------------------
    // Task 12 — the compat wire read COMPLETE: `finish_reason`, the reasoning
    // channel and `usage`, plus the conversion that puts them in the telemetry.
    // ---------------------------------------------------------------------

    /// A `/v1` capture whose model burned its whole budget reasoning: HTTP 200,
    /// `finish_reason: "length"`, empty content, 15 409 chars of reasoning.
    /// This exact shape is the head of the causal chain this release exists for.
    const FIX_C: &str = include_str!("../../tests/fixtures/ec/resp-C.json");

    /// The same model against a small payload: it converges, so content is
    /// present, `finish_reason` is `stop`, and the reasoning channel is non-empty.
    const FIX_H: &str = include_str!("../../tests/fixtures/ec/resp-H.json");

    /// Every termination the OpenAI API publishes, and one it does not.
    ///
    /// The translation was widened to cover `content_filter`, `tool_calls` and
    /// `function_call` because leaving them untranslated made the empty-completion message
    /// claim the budget "cannot be told" about values the vendor documents. That widening
    /// shipped with **no test**: deleting the three literals left the whole suite green,
    /// which is the same state the Anthropic side of this decision was in for three rounds.
    #[test]
    fn the_published_openai_vocabulary_is_translated_and_the_rest_is_not() {
        use crate::provider::{BudgetBearing, CompletionTelemetry};

        // (wire value, what it reads as, what that says about the budget)
        let table: [(&str, FinishReason, BudgetBearing); 8] = [
            ("stop", FinishReason::Stop, BudgetBearing::RuledOut),
            (
                "content_filter",
                FinishReason::Stop,
                BudgetBearing::RuledOut,
            ),
            ("tool_calls", FinishReason::Stop, BudgetBearing::RuledOut),
            ("function_call", FinishReason::Stop, BudgetBearing::RuledOut),
            ("length", FinishReason::Length, BudgetBearing::MayExplain),
            ("load", FinishReason::Load, BudgetBearing::RuledOut),
            (
                "brand_new_reason",
                FinishReason::Other("brand_new_reason".to_string()),
                BudgetBearing::Unknown,
            ),
            (
                "another_unpublished_one",
                FinishReason::Other("another_unpublished_one".to_string()),
                BudgetBearing::Unknown,
            ),
        ];

        for (raw, expected, bearing) in table {
            let got = FinishReason::from_wire(raw);
            assert_eq!(got, expected, "{raw} must translate to {expected:?}");
            assert_eq!(
                CompletionTelemetry::unmeasured()
                    .with_finish(got)
                    .budget_bearing(),
                bearing,
                "{raw} must bear {bearing:?} on the budget question"
            );
        }
    }

    /// The advice an operator reads, driven from a whole compat-wire body.
    ///
    /// The table above pins the translation; this pins what the translation is FOR. A
    /// `content_filter` reply that came back empty must not be answered with "raise your
    /// output budget", which is what an untranslated value produced.
    #[test]
    fn a_filtered_reply_is_not_answered_with_raise_your_budget() {
        for raw in ["content_filter", "tool_calls", "function_call"] {
            let body = format!(
                r#"{{"choices":[{{"message":{{"content":""}},"finish_reason":"{raw}"}}]}}"#
            );
            let r: OpenAiResponse = serde_json::from_str(&body).expect("parses");
            let rendered = match r.into_completion(16_384, false, ReasoningControl::Default) {
                Err(e) => e.to_string(),
                other => panic!("{raw} with empty content must be an empty completion: {other:?}"),
            };
            assert!(
                rendered.contains(crate::error::REMEDY_RULED_OUT),
                "{raw} is a reason the vendor named and it is not the budget: {rendered}"
            );
            assert!(
                !rendered.contains(crate::error::REMEDY_UNKNOWN),
                "{raw} is published; claiming it is uninterpretable is the defect: {rendered}"
            );
        }
    }

    #[test]
    fn finish_reason_is_read_where_today_it_is_silently_dropped() {
        // The response side used to deserialize ONLY `message`. That single
        // omission is the head of the whole causal chain: without it, "the model
        // burned its budget reasoning" and "the server sent nothing" are the same
        // opaque error.
        let r: OpenAiResponse = serde_json::from_str(FIX_C).expect("fixture parses");
        assert_eq!(r.choices[0].finish_reason, Some(FinishReason::Length));
    }

    #[test]
    fn the_reasoning_channel_is_read_and_its_absence_is_not_an_error() {
        let with: OpenAiResponse = serde_json::from_str(FIX_H).expect("fixture parses");
        assert!(with.choices[0].message.reasoning.is_some());
        // Literal rather than a fixture: no captured body OMITS the field, and the
        // point of the assertion is precisely that a backend which never sends it
        // must still parse. `Option`, not `String`: absent and empty are different
        // claims, and collapsing them would say the model did not reason when the
        // backend merely did not report.
        let without: OpenAiResponse =
            serde_json::from_str(r#"{"choices":[{"message":{"content":"hi"}}]}"#)
                .expect("a body without the field still parses");
        assert!(without.choices[0].message.reasoning.is_none());
    }

    #[test]
    fn the_reasoning_channel_is_also_read_under_its_other_spelling() {
        // Some compatible servers call it `reasoning_content`. One alias, because
        // both spellings are in the wild and a body using the other one would
        // otherwise report `NotMeasured` — a measurement that never happened.
        let r: OpenAiResponse = serde_json::from_str(
            r#"{"choices":[{"message":{"content":"hi","reasoning_content":"abc"}}]}"#,
        )
        .expect("parses");
        assert_eq!(r.choices[0].message.reasoning.as_deref(), Some("abc"));
    }

    #[test]
    #[should_panic(expected = "the test-module marker was not found")]
    fn a_split_that_finds_nothing_fails_instead_of_scanning_the_whole_file() {
        // The assertion inside the helper was PRESENT but not EXERCISED: deleting it left
        // the suite green, so the durable form of the CRLF fix was itself revertible. This
        // is the whole point of the helper -- a guard that silently widens to the whole file
        // reports success while checking nothing, which is the class this milestone has now
        // met in three different disguises.
        let _ = crate::provider::source_scan::production_half(
            "fn ships() {}
",
        );
    }

    #[test]
    fn the_production_half_is_found_whatever_the_checkout_did_to_line_endings() {
        // Pins the property independently of how THIS repo happens to be cloned. Without
        // it the fix is one revert from disarming again, and CI -- which runs on LF --
        // could never see it.
        let crlf = "fn ships() {}\r\n#[cfg(test)]\r\nmod tests {\r\n    fn hidden() {}\r\n}\r\n";
        let half = crate::provider::source_scan::production_half(crlf);
        assert!(
            half.contains("ships"),
            "the production half must survive: {half}"
        );
        assert!(
            !half.contains("hidden"),
            "the test half must be cut off even on a CRLF checkout: {half}"
        );
    }

    #[test]
    fn the_response_structs_stay_private() {
        // A-3: they are wire plumbing. `OpenAiRequest`/`OpenAiMessage` are already
        // `pub(crate)`; the response side must not become public by accident, the
        // way `ClaudeRequest`/`ClaudeMessage` did before 1.0.0 had to demote them.
        //
        // Scans only the PRODUCTION half, split at the `#[cfg(test)]` that opens
        // this module: `include_str!` embeds the test source too, so this literal
        // would otherwise make the file "contain" the needle it checks for.
        let production =
            crate::provider::source_scan::production_half(include_str!("openai_compat.rs"));
        for needle in [
            "pub struct OpenAiResponse",
            "pub struct OpenAiChoice",
            "pub struct OpenAiRespMessage",
            "pub struct OpenAiUsage",
        ] {
            assert!(
                !production.contains(needle),
                "the response side is wire plumbing and stays private: {needle}"
            );
        }
    }

    #[test]
    fn into_completion_carries_the_telemetry_of_a_real_success() {
        let r: OpenAiResponse = serde_json::from_str(FIX_H).expect("fixture parses");
        let c = r
            .into_completion(16_384, false, ReasoningControl::Default)
            .expect("content is present");
        assert!(!c.text.is_empty());
        assert_eq!(c.telemetry.finish, Some(FinishReason::Stop));
        // `usage` was read by no task before this one, so both counters would have
        // stayed `None` for EVERY compat provider — half of the telemetry A-5
        // promises.
        assert_eq!(c.telemetry.completion_tokens, Some(1280));
        assert_eq!(c.telemetry.prompt_tokens, Some(569));
        assert_eq!(
            c.telemetry.reasoning,
            ReasoningState::Measured {
                chars: 3535,
                text: None
            }
        );
    }

    #[test]
    fn the_trace_is_carried_only_when_the_consumer_opted_in() {
        let r: OpenAiResponse = serde_json::from_str(FIX_H).expect("fixture parses");
        let c = r
            .into_completion(16_384, true, ReasoningControl::Default)
            .expect("content is present");
        match c.telemetry.reasoning {
            ReasoningState::Measured { chars, text } => {
                assert_eq!(chars, 3535);
                // Additive, never substitutive: the length is there in both modes.
                assert_eq!(text.map(|t| t.chars().count()), Some(3535));
            }
            other => panic!("expected a measured trace, got {other:?}"),
        }
    }

    #[test]
    fn an_empty_completion_names_the_budget_that_cut_it() {
        let r: OpenAiResponse = serde_json::from_str(FIX_C).expect("fixture parses");
        let err = r
            .into_completion(4096, false, ReasoningControl::Default)
            .expect_err("no content is a failure");
        let ProviderError::EmptyCompletion { telemetry, cap } = &err else {
            panic!("expected EmptyCompletion, got {err:?}");
        };
        assert_eq!(*cap, 4096, "the cap that cut it travels with the error");
        assert_eq!(telemetry.finish, Some(FinishReason::Length));
        // THE POINT OF THE FIXTURE. `resp-C` is an empty completion whose `reasoning` field
        // holds the entire budget the model burned; reporting the cut without it leaves the
        // operator with "empty, budget 4096" and no evidence of where the budget went. This
        // assertion fails if the measurement is ever dropped on the way out again.
        assert_eq!(telemetry.completion_tokens, Some(4096));
        match &telemetry.reasoning {
            ReasoningState::Measured { chars, .. } => assert!(
                *chars > 10_000,
                "the burned reasoning must survive the empty path, got {chars}"
            ),
            other => panic!("expected a measured trace on the empty path, got {other:?}"),
        }
    }

    #[test]
    fn a_body_without_choices_is_a_contract_failure_named_for_what_is_missing() {
        let r: OpenAiResponse = serde_json::from_str(r#"{"choices":[]}"#).expect("valid JSON");
        let err = r
            .into_completion(4096, false, ReasoningControl::Default)
            .expect_err("no choices is a failure");
        assert!(
            matches!(
                err,
                ProviderError::ResponseContract {
                    reason: ResponseContractCause::NoMessage,
                    ..
                }
            ),
            "valid JSON that does not carry what the contract promises: {err:?}"
        );
    }

    #[test]
    fn the_compat_path_never_claims_our_own_defect() {
        // `NoGeneration`'s discriminant is a set of NATIVE wire fields that do not
        // exist here. With no way to assert the defect is ours, the reversible
        // route is the only honest one — an aborted run on a guess is the failure
        // this release exists to stop making.
        for body in [
            r#"{"choices":[{"message":{}}]}"#,
            r#"{"choices":[{"message":{"content":null}}]}"#,
            r#"{"choices":[{"message":{"content":""}}]}"#,
            r#"{"choices":[{"message":{"content":"   "}}]}"#,
        ] {
            let r: OpenAiResponse = serde_json::from_str(body).expect("valid JSON");
            let err = r
                .into_completion(4096, false, ReasoningControl::Default)
                .expect_err("no usable content is a failure");
            assert!(
                matches!(err, ProviderError::EmptyCompletion { .. }),
                "mage-local and reversible, never NoGeneration: {err:?}"
            );
        }
    }

    #[test]
    fn a_control_that_cannot_be_honoured_is_declared_even_when_reasoning_came_back() {
        // ORDER: the control resolves BEFORE the trace. A provider asked for
        // `Disabled` that cannot honour it declares `Unsupported` even though the
        // body carries `message.reasoning` — in fact ESPECIALLY then, because the
        // reasoning being present is the proof the control had no effect.
        // Resolving it the other way round overwrote C-8's declaration in silence.
        let r: OpenAiResponse = serde_json::from_str(FIX_H).expect("fixture parses");
        let c = r
            .into_completion(16_384, false, ReasoningControl::Disabled)
            .expect("content is present");
        // And it carries the length of the trace that came back ANYWAY — 3 535 characters on
        // this fixture. That number IS the proof the control had no effect, and it is the
        // operator's question in a mixed trio: how much is the seat that cannot honour this
        // still spending? Reporting the declaration without it answers half.
        assert_eq!(
            c.telemetry.reasoning,
            ReasoningState::Unsupported {
                backend: "openai-compatible".to_string(),
                chars: Some(3535),
                text: None,
            }
        );
    }

    #[test]
    fn absent_usage_leaves_the_counters_unmeasured_rather_than_zero() {
        let r: OpenAiResponse =
            serde_json::from_str(r#"{"choices":[{"message":{"content":"hi"}}]}"#).expect("parses");
        let c = r
            .into_completion(4096, false, ReasoningControl::Default)
            .expect("content is present");
        // A zero meaning "nobody counted" is indistinguishable from a real zero.
        assert_eq!(c.telemetry.completion_tokens, None);
        assert_eq!(c.telemetry.prompt_tokens, None);
        assert_eq!(c.telemetry.finish, None);
        assert_eq!(c.telemetry.reasoning, ReasoningState::NotMeasured);
    }
}
