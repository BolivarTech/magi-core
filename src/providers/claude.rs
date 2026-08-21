// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05

use crate::error::{ProviderError, ResponseContractCause};
use crate::provider::{
    Completion, CompletionConfig, CompletionTelemetry, DEFAULT_CLIENT_TIMEOUT, FinishReason,
    LlmProvider, ReasoningControl, ReasoningState, resolve_claude_alias,
};
use crate::providers::provider_url::ProviderUrl;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{Duration, Instant};

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
/// let provider = ClaudeProvider::new("sk-ant-api03-...", "claude-sonnet-4-6")?;
/// let response = provider.complete("You are helpful", "Hello", &CompletionConfig::default()).await?;
/// # Ok(())
/// # }
/// ```
pub struct ClaudeProvider {
    client: Client,
    /// The endpoint authority. A constant with no secret in it, yet still held as the authority
    /// type rather than a `&str`: it is what gives this provider the bounded body readers, and a
    /// rule that holds "except here" is a rule someone eventually applies wrong.
    endpoint: ProviderUrl,
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
///
/// This type is internal to the crate; it is HTTP request-shaping plumbing
/// for the `claude-api` feature and is not part of the analysis contract.
#[derive(Debug, Serialize)]
pub(crate) struct ClaudeRequest {
    /// Model identifier.
    pub(crate) model: String,
    /// Maximum tokens in the response.
    pub(crate) max_tokens: u32,
    /// Sampling temperature.
    pub(crate) temperature: f64,
    /// System prompt.
    pub(crate) system: String,
    /// Conversation messages.
    pub(crate) messages: Vec<ClaudeMessage>,
}

/// A single message in the Claude Messages API request.
///
/// This type is internal to the crate; it is HTTP request-shaping plumbing
/// for the `claude-api` feature and is not part of the analysis contract.
#[derive(Debug, Serialize)]
pub(crate) struct ClaudeMessage {
    /// Message role ("user" or "assistant").
    pub(crate) role: String,
    /// Message content.
    pub(crate) content: String,
}

/// Response from the Claude Messages API.
///
/// `stop_reason` and `usage` are `#[serde(default)]`: a body that omits either
/// (or both) must still parse (A-3) — the text extraction in
/// [`ClaudeProvider::parse_response`] never depended on them.
#[derive(Debug, Deserialize)]
struct ClaudeResponse {
    content: Vec<ContentBlock>,
    #[serde(default)]
    stop_reason: Option<String>,
    #[serde(default)]
    usage: Option<ClaudeUsage>,
}

/// A content block in the Claude Messages API response.
#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    type_: String,
    text: Option<String>,
    /// The payload of an extended-thinking block, which carries its text under `thinking`
    /// rather than `text`. Read so the reported reasoning state is a MEASUREMENT: without it
    /// this provider announced `chars: 0` for a channel it had never looked at, which is the
    /// zero-that-means-unread that `NotMeasured` exists to keep out of the report.
    #[serde(default)]
    thinking: Option<String>,
}

/// Token counts from the Claude Messages API's `usage` object.
///
/// Both fields `#[serde(default)]`: a partial `usage` object (Anthropic has
/// sent one missing `input_tokens` before) must not fail the whole parse.
#[derive(Debug, Default, Deserialize)]
struct ClaudeUsage {
    #[serde(default)]
    output_tokens: Option<u32>,
    #[serde(default)]
    input_tokens: Option<u32>,
}

/// Translates Anthropic's `stop_reason` vocabulary to this crate's
/// [`FinishReason`] at the provider boundary (T-5.3) — the same translation
/// [`crate::providers::ollama_wire`] performs for the native wire's
/// `done_reason`. Public API vocabulary stays vendor-neutral; the translation
/// lives entirely inside the provider that speaks the vendor's wire.
///
/// # Parameters
/// * `raw` — the value Anthropic sent.
///
/// # Returns
/// [`FinishReason::Length`] for `"max_tokens"` — the output-budget cut this
/// crate's diagnosis axis exists to surface — [`FinishReason::Stop`] for the
/// three values Anthropic documents as the model finishing on its own terms,
/// and [`FinishReason::Other`] for anything else, via
/// [`FinishReason::from_wire`] (capped at 64 characters, open space: Anthropic
/// could add a fourth value at any time).
fn map_stop_reason(raw: &str) -> FinishReason {
    match raw {
        "end_turn" | "stop_sequence" | "tool_use" => FinishReason::Stop,
        "max_tokens" => FinishReason::Length,
        other => FinishReason::from_wire(other),
    }
}

impl ClaudeProvider {
    /// Creates a new `ClaudeProvider` with the given API key and model.
    ///
    /// Supports model aliases: `"sonnet"`, `"opus"`, `"haiku"`, or any
    /// full model identifier containing `"claude-"`.
    ///
    /// A `reqwest::Client` is created once and reused for all subsequent
    /// requests (connection pooling).
    ///
    /// # Parameters
    /// - `api_key`: Anthropic API key (e.g., `"sk-ant-api03-..."`).
    /// - `model`: Model alias or full identifier (e.g., `"opus"` or `"claude-opus-4-7"`).
    ///
    /// # Errors
    ///
    /// Returns `ProviderError::Auth` if the model alias is unknown.
    pub fn new(
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self, ProviderError> {
        Self::with_timeout(api_key, model, DEFAULT_CLIENT_TIMEOUT)
    }

    /// Same as [`Self::new`] with an explicit **total** request timeout.
    ///
    /// The timeout covers the entire request, from send to the last body byte
    /// ([`reqwest::ClientBuilder::timeout`]) — this is what makes
    /// `ProviderError::Timeout` reachable against a model that hangs while
    /// generating. Pass `Duration::MAX` for "no timeout" (dangerous).
    pub fn with_timeout(
        api_key: impl Into<String>,
        model: impl Into<String>,
        timeout: Duration,
    ) -> Result<Self, ProviderError> {
        let model_id = resolve_claude_alias(&model.into())?;
        let client = Client::builder()
            .timeout(timeout)
            // Referer OFF — see the note in the OpenAI-compatible provider: on a redirect the
            // client would send the ORIGINAL url, query string included, to the target origin.
            .referer(false)
            .build()
            .map_err(|e| crate::provider::client_build_error(&e))?;
        Ok(Self {
            client,
            endpoint: ProviderUrl::parse(API_BASE_URL)?,
            api_key: api_key.into(),
            model: model_id,
        })
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
    /// This is a crate-internal helper used by the `LlmProvider` implementation.
    /// Its return type (`ClaudeRequest`) is also `pub(crate)`, keeping HTTP
    /// request-shaping details out of the public API surface.
    ///
    /// # Parameters
    /// - `system_prompt`: System-level instruction for the LLM.
    /// - `user_prompt`: User's input content.
    /// - `config`: Completion parameters (max_tokens, temperature).
    pub(crate) fn build_request_body(
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
        // Keeps its `Result` contract: this entry point returns only the text, so it has no
        // telemetry with which to tell a budget cut from a broken shape. `parse_completion`,
        // which does, draws that distinction instead.
        Self::text_of(Self::deserialize_body(body)?.content).ok_or(
            ProviderError::ResponseContract {
                reason: crate::error::ResponseContractCause::NoMessage,
                detail: String::new(),
            },
        )
    }

    /// Deserializes the body ONCE, so a caller that also wants telemetry does not pay for a
    /// second full parse of the same bytes into the same type. The body is bounded by the cap
    /// derived from `max_tokens`, so "parse it twice" is not free: it is one extra pass over up
    /// to a megabyte, on every completion, of every seat, of every rotation hop.
    fn deserialize_body(body: &str) -> Result<ClaudeResponse, ProviderError> {
        // The serde detail is dropped rather than rendered into text. It used to go through a
        // helper whose whole job was making a parse message safe to embed; with the cause TYPED,
        // that text became a second and untyped statement of the same fact. What a reader needs
        // — which half of the contract was not met — is in the variant.
        serde_json::from_str(body).map_err(|_| ProviderError::ResponseContract {
            reason: ResponseContractCause::Unreadable,
            detail: String::new(),
        })
    }

    /// Picks the first text block out of already-parsed content, if there is one.
    ///
    /// Returns `Option` rather than `Result` because "no text block" is not always a broken
    /// contract: under extended thinking, a response that hits `max_tokens` legitimately comes
    /// back carrying only a thinking block. The caller decides which it is, and it needs the
    /// telemetry to tell -- which is why this no longer throws that decision away.
    fn text_of(content: Vec<ContentBlock>) -> Option<String> {
        content
            .into_iter()
            .find(|block| block.type_ == "text")
            .and_then(|block| block.text)
    }

    /// Parses a Claude Messages API response body into a [`Completion`],
    /// carrying whatever telemetry Anthropic reported alongside the text
    /// [`Self::parse_response`] already extracts.
    ///
    /// # Parameters
    /// * `body` — the raw JSON response body.
    /// * `reasoning` — the caller's [`ReasoningControl`], read only to decide
    ///   whether to DECLARE that this provider cannot honour
    ///   [`ReasoningControl::Disabled`] (C-8) — the Messages API has no
    ///   per-request switch for its reasoning channel at all.
    ///
    /// # Returns
    /// A [`Completion`] whose telemetry carries `stop_reason` (mapped through
    /// [`map_stop_reason`]) and `usage`'s token counts when Anthropic sent
    /// them, and [`ReasoningState::Unsupported`] when the caller asked to
    /// disable reasoning. With [`ReasoningControl::Default`] the reasoning
    /// state is [`ReasoningState::NotMeasured`] — nothing was asked, so
    /// nothing is declared, and there is no trace channel this provider reads
    /// regardless.
    ///
    /// # Reads the response ONCE
    ///
    /// An earlier version parsed `body` here and then again, in the same function, to reach the
    /// text — a full extra parse per completion, per seat, per rotation hop, over a body bounded
    /// only by the cap derived from `max_tokens`. Its stated reason was that removing the helper
    /// would leave it dead, and that was not true: the helper had a caller, one line above. The
    /// text now comes from the same parse the telemetry does.
    ///
    /// # Errors
    /// Same as [`Self::parse_response`].
    pub(crate) fn parse_completion(
        body: &str,
        reasoning: ReasoningControl,
        cap: u32,
        trace: bool,
    ) -> Result<Completion, ProviderError> {
        let response = Self::deserialize_body(body)?;

        let mut telemetry = CompletionTelemetry::unmeasured();
        if let Some(raw) = response.stop_reason.as_deref() {
            telemetry = telemetry.with_finish(map_stop_reason(raw));
        }
        if let Some(usage) = response.usage {
            if let Some(n) = usage.output_tokens {
                telemetry = telemetry.with_completion_tokens(n);
            }
            if let Some(n) = usage.input_tokens {
                telemetry = telemetry.with_prompt_tokens(n);
            }
        }
        // Taken from the SAME parse: reading the text through `parse_response` here would
        // deserialize these very bytes a second time into this very type.
        let had_content = !response.content.is_empty();
        // Counted BEFORE the content is consumed, and counted at all because Anthropic does
        // have a reasoning channel: extended thinking returns `thinking` blocks. The earlier
        // claim that this wire exposed none was simply wrong.
        // The kinds present, for the diagnostic below. Collected before `content` is consumed.
        let block_types: Vec<String> = response.content.iter().map(|b| b.type_.clone()).collect();
        let thought_text: String = response
            .content
            .iter()
            .filter(|b| b.type_ == "thinking")
            .filter_map(|b| b.thinking.as_deref())
            .collect();
        // READABLE is the condition, not merely PRESENT. A `redacted_thinking` block, or a
        // `thinking` block whose payload is absent, proves the channel fired but carries
        // nothing to count -- and reporting `Measured { chars: 0 }` for it would claim a look
        // that found nothing where the truth is a look that could not read. `NotMeasured` is
        // what the crate uses for exactly that, everywhere else.
        //
        // ONE value, not a flag beside a string: a separate `saw_thinking` boolean let `chars`
        // and `text` be decided independently, and they promptly disagreed.
        let readable: Option<&str> = (!thought_text.is_empty()).then_some(thought_text.as_str());
        let text = Self::text_of(response.content);
        telemetry = telemetry.with_reasoning(match (reasoning, readable) {
            // The control cannot be honoured here: this provider sends no switch that turns
            // extended thinking off. What it CAN do is say how much came back anyway, which is
            // the proof the control had no effect -- the same thing the compat path reports.
            (ReasoningControl::Disabled, readable) => ReasoningState::Unsupported {
                backend: "anthropic".to_string(),
                // The SAME readability rule as the arm below, swept across both instead of one:
                // a redacted block fired the channel and left nothing to count, and reporting
                // `0` here says no reasoning came back — which reads as the control having
                // worked, on the variant whose whole job is to say it did not.
                // BOTH fields, from ONE decision. Sweeping `chars` and leaving `text` beside it
                // produced `Some("")` where `chars` was `None` -- an empty trace offered as
                // though it were the trace, which is the same zero-that-lies one field over. A
                // sweep that stops at the sibling in the same struct literal is not a sweep.
                chars: readable.map(|t| t.chars().count()),
                // Carried only when the consumer asked AND there is something to carry. Reading
                // the channel and then discarding an opted-in payload is the silent drop the
                // reasoning contract forbids; offering an empty one is the same drop wearing a
                // value.
                text: readable.filter(|_| trace).map(str::to_string),
            },
            // Measured only when a thinking block was actually present. With none, nothing was
            // seen -- and `NotMeasured` says that, where `Measured { chars: 0 }` would claim a
            // look that found nothing.
            // The value comes from the BINDING, so there is no default to reinstate. Guarding
            // with `is_some()` and then reaching for the value through `map_or(0, ..)` left the
            // zero-that-lies sitting in the expression as an unreachable default -- one edit
            // from being reachable again, in the arm whose whole subject is that zero.
            (ReasoningControl::Default, Some(t)) => ReasoningState::Measured {
                chars: t.chars().count(),
                text: trace.then(|| t.to_string()),
            },
            (ReasoningControl::Default, None) => ReasoningState::NotMeasured,
        });

        // The THIRD wire to need this, and the reason it is worth stating once more: under
        // extended thinking an Anthropic response that exhausts `max_tokens` comes back with a
        // thinking block and no text one. Propagating that with `?` discarded the telemetry
        // assembled just above -- including the `max_tokens` stop reason that names the cut --
        // and reported a broken contract instead of the budget cut it was.
        //
        // An empty `content` array is still a contract failure: nothing was sent at all, and no
        // termination reason makes that legitimate.
        match text {
            Some(t) if !t.trim().is_empty() => Ok(Completion::new(t).with_telemetry(telemetry)),
            // NARROWED to a termination the budget could actually explain. Widened to "any
            // content with no text block", this arm told an operator to raise `max_tokens`
            // for a `tool_use` or `redacted_thinking` response the budget never cut -- which
            // is the misdiagnosis this whole release exists to end, re-created one level
            // down by the fix for it. An absent reason still qualifies: unknown is not the
            // same as known-to-be-something-else.
            Some(_) | None
                if had_content && matches!(telemetry.finish, None | Some(FinishReason::Length)) =>
            {
                Err(ProviderError::EmptyCompletion { telemetry, cap })
            }
            // The observed SHAPE travels, because this arm now receives a class the narrowing
            // moved into it — a `tool_use` or redacted response — and rendering that
            // identically to a literally empty `content: []` throws away the only thing that
            // tells the two apart. Discarding telemetry on an error path is the defect this
            // release fixed for `EmptyCompletion`; the fix for THAT re-created it here.
            //
            // Through the bounding constructor, so the text is capped like every other
            // outside-influenced string that reaches the serialized report.
            _ => Err(ProviderError::response_contract(
                crate::error::ResponseContractCause::NoMessage,
                format!(
                    "no text block; termination {:?}, blocks [{}]",
                    telemetry.finish,
                    block_types.join(", ")
                ),
            )),
        }
    }

    /// Maps an HTTP status code and response body to the appropriate
    /// `ProviderError` variant.
    ///
    /// - 401 and 403 map to `ProviderError::Auth`.
    /// - All other non-2xx status codes map to `ProviderError::Http`, carrying the
    ///   raw `Retry-After` header for the retry policy to interpret.
    ///
    /// **2.0 breaking change:** gains `retry_after_raw` and `received_at`
    /// parameters. Visibility stays `pub`. A 1.x caller preserves the old
    /// behaviour by passing `vec![], None` — the empty vector means "the server
    /// sent no `Retry-After`", and `None` means "no receipt instant to discount
    /// a honored delay against", which together reproduce the pre-2.0 semantics
    /// exactly.
    pub fn map_status_to_error(
        status: u16,
        body: &str,
        retry_after_raw: Vec<String>,
        received_at: Option<Instant>,
    ) -> ProviderError {
        match status {
            // 401/403 -> Auth: no header carried.
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
    /// - `ProviderError::Http` on other non-2xx responses — and since `4.0.0` `Http`
    ///   carries **only real HTTP statuses**.
    /// - [`ProviderError::ResponseTooLarge`] when the body exceeds the cap derived from
    ///   `max_tokens`. It fails rather than truncating: a cut body loses its closing marker.
    /// - [`ProviderError::ResponseContract`] when the response did not meet the contract —
    ///   `Unreadable`, `NoMessage`, or `RedirectRefused`. It is **mage-local**: no lineage
    ///   is condemned run-wide.
    async fn complete(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        config: &CompletionConfig,
    ) -> Result<Completion, ProviderError> {
        let body = self.build_request_body(system_prompt, user_prompt, config);

        // Routed through the URL authority like every other HTTP provider. The endpoint has no
        // secret to hide, so this is not about redaction here — it is what supplies the BOUNDED
        // body readers below, and it keeps the transport-error mapping in the one place that owns
        // it. An exception for the provider that happens not to need redaction is how this
        // provider silently kept reading unbounded bodies.
        let response = self
            .endpoint
            .request(&self.client, reqwest::Method::POST, &[])
            .header("x-api-key", self.api_key.clone())
            .header("anthropic-version", ANTHROPIC_VERSION.to_string())
            .json(&body)
            .send()
            .await?;

        // Capture the receipt instant (headers arrived) and the raw `Retry-After` header in the
        // same place the status is read.
        let received_at = Instant::now();
        let status = response.status();
        let retry_after_raw: Vec<String> = response.retry_after_raw();
        if !(200..300).contains(&status) {
            // Truncates and says so: an error body is diagnostic text, so losing its tail costs
            // detail, while dropping it whole costs the reason the error is read at all.
            let response_body = response.read_diagnostic_body().await;
            return Err(Self::map_status_to_error(
                status,
                &response_body,
                retry_after_raw,
                Some(received_at),
            ));
        }

        // FAILS over the cap rather than truncating: this body carries the verdict, and a cut one
        // loses its closing marker, which would make the parser blame the model for our cut.
        let response_body = response.read_verdict_body(config.max_tokens).await?;

        Self::parse_completion(
            &response_body,
            config.reasoning,
            config.max_tokens,
            config.reasoning_trace,
        )
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
        let provider = super::ClaudeProvider::new("sk-test-key", "claude-sonnet-4-6").unwrap();
        assert_eq!(provider.name(), "claude");
        assert_eq!(provider.model(), "claude-sonnet-4-6");
    }

    /// provider.name() returns "claude".
    #[test]
    fn test_claude_provider_name_returns_claude() {
        let provider = super::ClaudeProvider::new("key", "claude-sonnet-4-6").unwrap();
        assert_eq!(provider.name(), "claude");
    }

    /// provider.model() returns the configured model string.
    #[test]
    fn test_claude_provider_model_returns_configured_model() {
        let provider = super::ClaudeProvider::new("key", "claude-opus-4-6").unwrap();
        assert_eq!(provider.model(), "claude-opus-4-6");
    }

    /// new("opus") resolves alias to full model identifier.
    #[test]
    fn test_claude_provider_new_resolves_alias() {
        let provider = super::ClaudeProvider::new("key", "opus").unwrap();
        assert_eq!(provider.model(), "claude-opus-4-7");
    }

    /// new("unknown") returns ProviderError::Auth.
    #[test]
    fn test_claude_provider_new_rejects_unknown_model() {
        let result = super::ClaudeProvider::new("key", "unknown");
        assert!(result.is_err());
    }

    // -- Request building --

    /// build_request_body produces correct JSON structure with all required fields.
    #[test]
    fn test_build_request_body_contains_all_required_fields() {
        use crate::provider::CompletionConfig;

        let provider = super::ClaudeProvider::new("sk-test", "claude-sonnet-4-6").unwrap();
        let config = CompletionConfig::default();
        let body = provider.build_request_body("You are helpful", "Hello", &config);

        assert_eq!(body.model, "claude-sonnet-4-6");
        assert_eq!(body.max_tokens, 16_384);
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

    // -- Task 11: telemetry the HTTP provider CAN report --

    /// Step 3b: `stop_reason` and `usage` are read into the completion's
    /// telemetry. `"max_tokens"` maps to `FinishReason::Length` — the output
    /// budget cut, which is the signal the diagnosis axis exists to surface.
    /// An extended-thinking response that exhausts `max_tokens` comes back with a thinking
    /// block and NO text block. Propagating that with `?` used to discard the telemetry
    /// assembled one line above -- including the `max_tokens` stop reason -- and report a
    /// broken contract instead of the budget cut it actually was. Third wire, same defect.
    #[test]
    fn an_anthropic_budget_cut_with_no_text_block_names_the_budget_not_a_broken_contract() {
        let json = r#"{"content":[{"type":"thinking","thinking":"a long deliberation"}],
                       "stop_reason":"max_tokens",
                       "usage":{"input_tokens":63926,"output_tokens":16384}}"#;
        match super::ClaudeProvider::parse_completion(
            json,
            super::ReasoningControl::Default,
            16_384,
            false,
        ) {
            Err(crate::error::ProviderError::EmptyCompletion { telemetry, cap }) => {
                assert_eq!(cap, 16_384);
                // The channel was READ, not assumed absent: the block's own text is counted.
                assert!(
                    matches!(
                        telemetry.reasoning,
                        crate::provider::ReasoningState::Measured { chars, .. }
                            if chars == "a long deliberation".chars().count()
                    ),
                    "the thinking block must be measured, got {:?}",
                    telemetry.reasoning
                );
                assert_eq!(
                    telemetry.finish,
                    Some(crate::provider::FinishReason::Length)
                );
                assert_eq!(telemetry.completion_tokens, Some(16_384));
                assert_eq!(telemetry.prompt_tokens, Some(63_926));
            }
            other => panic!("expected EmptyCompletion naming the budget, got {other:?}"),
        }
    }

    /// The safe direction keeps its own meaning: nothing was sent at all, so no termination
    /// reason makes it legitimate and it stays a contract failure.
    /// A non-text block that the BUDGET did not cut is not a budget cut.
    ///
    /// The fix for the budget-cut case first fired on ANY content carrying no text block, so a
    /// `tool_use` response told the operator to raise `max_tokens` for a completion nothing had
    /// truncated -- the same misdiagnosis this release exists to end, re-created one level down
    /// by the correction for it. The termination reason is what separates them.
    #[test]
    fn a_non_text_block_with_a_normal_ending_is_not_reported_as_a_budget_cut() {
        let json = r#"{"content":[{"type":"tool_use","id":"t1"}],
                       "stop_reason":"tool_use",
                       "usage":{"input_tokens":10,"output_tokens":20}}"#;
        match super::ClaudeProvider::parse_completion(
            json,
            super::ReasoningControl::Default,
            4096,
            false,
        ) {
            Err(crate::error::ProviderError::ResponseContract { .. }) => {}
            other => panic!("a tool_use response is a contract miss, not a cut: {other:?}"),
        }
    }

    /// `reasoning_trace` is honoured on this wire too, on both reasoning arms.
    ///
    /// It reads the thinking channel since this milestone, and reading a channel then throwing
    /// away the payload someone opted into is the silent drop the reasoning contract forbids.
    #[test]
    fn the_opted_in_reasoning_trace_is_carried_on_the_anthropic_wire() {
        let json = r#"{"content":[{"type":"thinking","thinking":"deliberating"},
                                  {"type":"text","text":"answer"}],
                       "stop_reason":"end_turn"}"#;
        for control in [
            super::ReasoningControl::Default,
            super::ReasoningControl::Disabled,
        ] {
            let off = super::ClaudeProvider::parse_completion(json, control, 4096, false)
                .expect("parses");
            let on =
                super::ClaudeProvider::parse_completion(json, control, 4096, true).expect("parses");
            let (chars_off, text_off) = reasoning_parts(&off.telemetry.reasoning);
            let (chars_on, text_on) = reasoning_parts(&on.telemetry.reasoning);
            assert_eq!(
                chars_off, chars_on,
                "{control:?}: the length is always there"
            );
            assert!(
                text_off.is_none(),
                "{control:?}: opt-in means off by default"
            );
            assert_eq!(
                text_on.as_deref(),
                Some("deliberating"),
                "{control:?}: the opted-in payload must survive"
            );
        }
    }

    /// Pulls `(chars, text)` out of either measured shape, so the test above can assert the
    /// SAME property across both arms instead of duplicating itself per variant.
    fn reasoning_parts(state: &crate::provider::ReasoningState) -> (Option<usize>, Option<String>) {
        match state {
            crate::provider::ReasoningState::Measured { chars, text } => {
                (Some(*chars), text.clone())
            }
            crate::provider::ReasoningState::Unsupported { chars, text, .. } => {
                (*chars, text.clone())
            }
            other => panic!("expected a measured shape, got {other:?}"),
        }
    }

    /// A non-text response says WHAT it was, so it is not confused with an empty one.
    ///
    /// The narrowing that stopped calling a `tool_use` a budget cut sent it to the contract arm,
    /// which discarded the telemetry — rendering it identically to a literally empty
    /// `content: []`. Discarding telemetry on an error path is precisely the defect this release
    /// fixed for `EmptyCompletion`, re-created by the fix for it.
    #[test]
    fn a_non_text_contract_failure_names_the_shape_it_saw() {
        let json = r#"{"content":[{"type":"tool_use","id":"t1"}],"stop_reason":"tool_use"}"#;
        let err = super::ClaudeProvider::parse_completion(
            json,
            super::ReasoningControl::Default,
            4096,
            false,
        )
        .expect_err("a response with no text is a failure");
        let msg = err.to_string();
        assert!(
            msg.contains("tool_use"),
            "the block kind must survive: {msg}"
        );

        // The genuinely empty case is DIFFERENT, which is the whole point.
        let empty = super::ClaudeProvider::parse_completion(
            r#"{"content":[],"stop_reason":"end_turn"}"#,
            super::ReasoningControl::Default,
            4096,
            false,
        )
        .expect_err("an empty content array is a failure");
        assert_ne!(
            msg,
            empty.to_string(),
            "a tool_use response and an empty one must not render identically"
        );
    }

    /// A thinking channel that fired but could not be READ is not a measured zero.
    ///
    /// `redacted_thinking`, and a `thinking` block whose payload is absent, both prove the
    /// channel ran and carry nothing to count. Reporting `Measured { chars: 0 }` would claim a
    /// look that found nothing where the truth is a look that could not read.
    #[test]
    fn an_unreadable_thinking_block_is_not_reported_as_a_measured_zero() {
        for blocks in [
            r#"{"type":"redacted_thinking","data":"enc"}"#,
            r#"{"type":"thinking"}"#,
        ] {
            let json = format!(r#"{{"content":[{blocks},{{"type":"text","text":"a"}}]}}"#);

            // BOTH arms, because the first version of this fix repaired one and left the other
            // saying `chars: 0` — which on `Unsupported` reads as "no reasoning came back", and
            // therefore as the control having worked, on the variant that exists to declare it
            // did not. A test scoped to one arm is how half a sweep looks like a whole one.
            let default = super::ClaudeProvider::parse_completion(
                &json,
                super::ReasoningControl::Default,
                4096,
                false,
            )
            .expect("the text block makes this a success");
            assert!(
                matches!(
                    default.telemetry.reasoning,
                    crate::provider::ReasoningState::NotMeasured
                ),
                "{blocks} (Default): expected NotMeasured, got {:?}",
                default.telemetry.reasoning
            );

            let disabled = super::ClaudeProvider::parse_completion(
                &json,
                super::ReasoningControl::Disabled,
                4096,
                false,
            )
            .expect("the text block makes this a success");
            assert!(
                matches!(
                    disabled.telemetry.reasoning,
                    crate::provider::ReasoningState::Unsupported { chars: None, .. }
                ),
                "{blocks} (Disabled): the declaration must survive with NO count, got {:?}",
                disabled.telemetry.reasoning
            );

            // THE SIBLING FIELD, in the same struct literal. Sweeping `chars` and leaving
            // `text` beside it produced `Some("")` where `chars` was `None` — an empty trace
            // offered as though it were the trace. Both come from one decision now, and this
            // asserts they cannot disagree again.
            let opted_in = super::ClaudeProvider::parse_completion(
                &json,
                super::ReasoningControl::Disabled,
                4096,
                true,
            )
            .expect("the text block makes this a success");
            let crate::provider::ReasoningState::Unsupported { chars, text, .. } =
                &opted_in.telemetry.reasoning
            else {
                panic!(
                    "expected Unsupported, got {:?}",
                    opted_in.telemetry.reasoning
                );
            };
            assert!(chars.is_none(), "{blocks}: nothing readable to count");
            assert!(
                text.is_none(),
                "{blocks}: an unreadable channel has no trace to offer, not an empty one: {text:?}"
            );
        }

        // And a READABLE channel still counts, on both arms, so the rule is not a wall.
        let readable = r#"{"content":[{"type":"thinking","thinking":"abc"},
                                      {"type":"text","text":"a"}]}"#;
        for (control, expect_some) in [
            (super::ReasoningControl::Default, true),
            (super::ReasoningControl::Disabled, true),
        ] {
            let out = super::ClaudeProvider::parse_completion(readable, control, 4096, false)
                .expect("parses");
            let (chars, _) = reasoning_parts(&out.telemetry.reasoning);
            assert_eq!(chars.is_some(), expect_some, "{control:?}: {chars:?}");
            assert_eq!(chars, Some(3), "{control:?}");
        }
    }

    #[test]
    fn an_anthropic_response_with_no_content_at_all_is_still_a_contract_failure() {
        let json = r#"{"content":[],"stop_reason":"max_tokens"}"#;
        assert!(matches!(
            super::ClaudeProvider::parse_completion(
                json,
                super::ReasoningControl::Default,
                4096,
                false
            ),
            Err(crate::error::ProviderError::ResponseContract {
                reason: crate::error::ResponseContractCause::NoMessage,
                ..
            })
        ));
    }

    #[test]
    fn the_anthropic_http_provider_reports_its_stop_reason_and_usage() {
        use crate::provider::FinishReason;

        let json = r#"{"content":[{"type":"text","text":"hi"}],
            "stop_reason":"max_tokens",
            "usage":{"input_tokens":100,"output_tokens":50}}"#;
        let out = super::ClaudeProvider::parse_completion(
            json,
            super::ReasoningControl::default(),
            4096,
            false,
        )
        .expect("valid body parses");
        assert_eq!(out.telemetry.finish, Some(FinishReason::Length));
        assert_eq!(out.telemetry.completion_tokens, Some(50));
        assert_eq!(out.telemetry.prompt_tokens, Some(100));
    }

    /// Task 11 / C-8: the Messages API has no per-request switch for its
    /// reasoning channel, so `Disabled` is DECLARED rather than silently
    /// ignored — the forbidden thing was never "carry on", it was carrying on
    /// in silence.
    #[test]
    fn the_provider_declares_unsupported_when_the_caller_disables_reasoning() {
        use crate::provider::ReasoningState;

        let json = r#"{"content":[{"type":"text","text":"hi"}]}"#;
        let out = super::ClaudeProvider::parse_completion(
            json,
            super::ReasoningControl::Disabled,
            4096,
            false,
        )
        .expect("valid body parses");
        assert!(
            matches!(
                out.telemetry.reasoning,
                ReasoningState::Unsupported { ref backend, .. } if backend == "anthropic"
            ),
            "expected Unsupported{{backend: \"anthropic\"}}, got {:?}",
            out.telemetry.reasoning
        );
    }

    /// The other half of C-8: with nothing asked, nothing is declared — and
    /// there is no trace channel this provider reads regardless, so this must
    /// be `NotMeasured`, never `Measured {{ chars: 0 }}`.
    #[test]
    fn the_provider_declares_nothing_when_reasoning_control_is_default() {
        use crate::provider::ReasoningState;

        let json = r#"{"content":[{"type":"text","text":"hi"}]}"#;
        let out = super::ClaudeProvider::parse_completion(
            json,
            super::ReasoningControl::Default,
            4096,
            false,
        )
        .expect("valid body parses");
        assert_eq!(out.telemetry.reasoning, ReasoningState::NotMeasured);
    }

    // -- Error mapping --

    /// map_status_to_error maps 401 to ProviderError::Auth.
    #[test]
    fn test_map_status_401_to_auth_error() {
        let err = super::ClaudeProvider::map_status_to_error(401, "unauthorized", vec![], None);
        assert!(matches!(err, crate::error::ProviderError::Auth { .. }));
    }

    /// map_status_to_error maps 403 to ProviderError::Auth.
    #[test]
    fn test_map_status_403_to_auth_error() {
        let err = super::ClaudeProvider::map_status_to_error(403, "forbidden", vec![], None);
        assert!(matches!(err, crate::error::ProviderError::Auth { .. }));
    }

    /// map_status_to_error maps 500 to ProviderError::Http.
    #[test]
    fn test_map_status_500_to_http_error() {
        let err = super::ClaudeProvider::map_status_to_error(500, "server error", vec![], None);
        match err {
            crate::error::ProviderError::Http { status, body, .. } => {
                assert_eq!(status, 500);
                assert_eq!(body, "server error");
            }
            other => panic!("expected Http, got: {other}"),
        }
    }

    /// map_status_to_error maps 429 to ProviderError::Http (not Auth).
    #[test]
    fn test_map_status_429_to_http_error() {
        let err = super::ClaudeProvider::map_status_to_error(429, "rate limited", vec![], None);
        assert!(matches!(
            err,
            crate::error::ProviderError::Http { status: 429, .. }
        ));
    }

    /// map_status_to_error carries the raw Retry-After header into the Http error.
    #[test]
    fn test_claude_map_status_429_carries_retry_after_header() {
        let err = super::ClaudeProvider::map_status_to_error(
            429,
            "rate limited",
            vec!["12".to_string()],
            None,
        );
        match err {
            crate::error::ProviderError::Http {
                status,
                retry_after_raw,
                ..
            } => {
                assert_eq!(status, 429);
                assert_eq!(retry_after_raw, vec!["12".to_string()]);
            }
            other => panic!("expected Http with header, got: {other:?}"),
        }
    }

    // -- Client reuse --

    /// reqwest::Client is stored in struct (structural test).
    #[test]
    fn test_client_is_stored_in_struct() {
        let provider = super::ClaudeProvider::new("key", "sonnet").unwrap();
        assert_eq!(provider.name(), "claude");
        let provider2 = super::ClaudeProvider::new("key2", "opus").unwrap();
        assert_eq!(provider2.model(), "claude-opus-4-7");
    }

    // -- Debug does not expose API key --

    /// Debug output does not contain the API key.
    #[test]
    fn test_debug_does_not_expose_api_key() {
        let provider = super::ClaudeProvider::new("sk-super-secret-key-12345", "sonnet").unwrap();
        let debug_str = format!("{:?}", provider);
        assert!(
            !debug_str.contains("sk-super-secret-key-12345"),
            "Debug output must not contain API key, got: {debug_str}"
        );
    }

    // -- pub(crate) visibility guard --

    /// ClaudeProvider is fully usable after ClaudeRequest/ClaudeMessage became
    /// pub(crate). This test is a compile-time guard: if the visibility change
    /// accidentally broke crate-internal access it would fail to compile.
    #[cfg(feature = "claude-api")]
    #[test]
    fn test_claude_provider_usable_without_public_request_types() {
        // ClaudeRequest and ClaudeMessage are now pub(crate); ClaudeProvider still works.
        let provider = super::ClaudeProvider::new("sk-test", "claude-sonnet-4-6")
            .expect("ClaudeProvider should construct with a valid model");
        assert_eq!(provider.name(), "claude");
        assert_eq!(provider.model(), "claude-sonnet-4-6");
    }
}
