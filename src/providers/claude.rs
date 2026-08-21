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
/// [`ClaudeProvider::parse_completion`] reads them, and the text extraction
/// never depended on them.
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
/// [`FinishReason::Length`] for the two values that mean the response ran out of
/// room, [`FinishReason::Stop`] for the values that end a turn for a reason that
/// is **not** the budget, and [`FinishReason::Other`] for anything else, via
/// [`FinishReason::from_wire`] (capped at 64 characters).
///
/// # Every value Anthropic documents is translated, and that is the point
///
/// What lands in [`FinishReason::Other`] decides what the empty-completion message
/// is allowed to claim: an untranslated reason means crate::error::REMEDY_UNKNOWN, so a reason
/// left there by oversight turns a knowable case into an unknowable one. Three
/// values used to fall through here and each was a live defect:
///
/// * `model_context_window_exceeded` is Anthropic's out-of-room response. It reads
///   as [`FinishReason::Length`] because that is what it is — the same condition the
///   OpenAI-compatible wire reports as `"length"`, so the two wires now agree. Its
///   remedy is not always "raise the cap", which is why the message for `Length`
///   names the prompt as the other possibility instead of prescribing blindly.
/// * `refusal` and `pause_turn` end a turn on terms the backend named, and neither
///   is the output budget. They join `end_turn`, `stop_sequence` and `tool_use`.
///
/// # What the fold costs, said plainly
///
/// [`FinishReason::Stop`] does not carry the word that produced it, so a report of a
/// refusal now reads `Stop` and the fact that the model REFUSED is gone from it. That
/// is a real loss and it is chosen, not overlooked. The alternatives were a variant per
/// vendor word — which does not scale past two vendors — or a payload on `Stop`, which
/// would touch every construction and match of a type nothing outside the crate needs
/// the payload from. **Every consumer of this value consumes exactly one property**: the
/// reply is not short because it ran out of room. The day a consumer needs the word, the
/// payload is the fix, and it is additive for `Other` and a major for `Stop`.
///
/// The same trade decides `model_context_window_exceeded`. Reading it as `Length` folds
/// away Anthropic's own disambiguation between an undersized budget and an oversized
/// prompt — a distinction the OpenAI-compatible wire does not make at all. It is folded
/// so that ONE condition does not read as two different things depending on which
/// backend answered, and the message pays for it by naming both causes and printing
/// `prompt_tokens` instead of prescribing one. If that disambiguation is ever needed
/// programmatically, the answer is a dedicated variant, not un-folding this one.
fn map_stop_reason(raw: &str) -> FinishReason {
    match raw {
        // Only where Anthropic DIFFERS from the shared table. Its not-the-budget words --
        // `end_turn`, `stop_sequence`, `tool_use`, `refusal`, `pause_turn` -- live in
        // `from_wire` beside the compat wire's, so that `Other` means "no vendor publishes
        // this" on both wires rather than per-wire, which is what four rustdocs claim.
        "max_tokens" | "model_context_window_exceeded" => FinishReason::Length,
        other => FinishReason::from_wire(other),
    }
}

/// Whether the budget question alone routes a contentless reply to
/// [`ProviderError::EmptyCompletion`].
///
/// **An exhaustive `match`, not a comparison.** `== MayExplain` would let a fourth
/// [`crate::provider::BudgetBearing`] state be added and silently join the `false`
/// side — the identical hazard the `budget_bearing` rustdoc argues against one
/// module over, re-created at the call site by an operator that does not force a
/// decision. This way a new state stops the build here and someone chooses.
///
/// # Parameters
/// * `t` — the telemetry read from the same response the message will describe.
///
/// # Returns
/// `true` only for [`crate::provider::BudgetBearing::MayExplain`].
///
/// # `Unknown` is `false`, and the guard and the message DO answer different questions
///
/// The message says "whether the budget was reached cannot be told"; the guard has to
/// route anyway, and it declines to overrule the block shape on a maybe. That is not a
/// contradiction — a reply carrying a `tool_use` block and no text produced *something*,
/// which is a fact about the shape, not about the budget. Since every value Anthropic and
/// the OpenAI-compatible wire document is now translated, `Unknown` means a value neither
/// vendor has published, and filing that as a broken contract is the conservative read.
fn budget_routes_here(t: &CompletionTelemetry) -> bool {
    match t.budget_bearing() {
        crate::provider::BudgetBearing::MayExplain => true,
        crate::provider::BudgetBearing::RuledOut | crate::provider::BudgetBearing::Unknown => false,
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
        // EVERY text block, not the first one's payload. `find` took the first block of that
        // type and then read its `text`, so a first block carrying `null` -- or an empty string
        // -- DISCARDED a verdict sitting in a later one, and the empty case additionally told
        // the operator to raise a budget that had cut nothing. Anthropic interleaves text with
        // thinking and tool_use blocks, so "the first one" was never the whole answer.
        let mut seen_text_block = false;
        let mut out = String::new();
        for block in content {
            if block.type_ != "text" {
                continue;
            }
            seen_text_block = true;
            if let Some(t) = block.text {
                out.push_str(&t);
            }
        }
        seen_text_block.then_some(out)
    }

    /// Parses a Claude Messages API response body into a [`Completion`],
    /// carrying whatever telemetry Anthropic reported alongside the text
    /// [`Self::text_of`] already extracts.
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
    /// Same as [`Self::deserialize_body`].
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
        // Taken from the SAME parse: deserializing the body a second time here would
        // deserialize these very bytes a second time into this very type.
        let had_content = !response.content.is_empty();
        // Counted BEFORE the content is consumed, and counted at all because Anthropic does
        // have a reasoning channel: extended thinking returns `thinking` blocks. The earlier
        // claim that this wire exposed none was simply wrong.
        // The kinds present, for the diagnostic below. Collected before `content` is consumed.
        let block_types: Vec<String> = response.content.iter().map(|b| b.type_.clone()).collect();
        // Whether the turn produced anything OTHER than text. It is what separates "the model
        // returned nothing", which the compat wire also sees, from "the model did something
        // else", which only this wire can express.
        let had_non_text = response.content.iter().any(|b| b.type_ != "text");
        // Three states, kept apart, because collapsing any two of them is the conflation this
        // whole release exists to remove:
        //
        //   * a payload we could READ, even an empty one -> a measurement (`Some`, maybe "")
        //   * a payload we could NOT read -- `redacted_thinking`, or a `thinking` block with no
        //     payload -- so the channel fired and its size is UNKNOWN
        //   * no thinking block at all -> nothing to measure
        //
        // The first two were merged by testing emptiness, which made `thinking: ""` report as
        // "nobody looked" while the compat wire called the same body a measured zero. The two
        // wires then contradicted each other, each with a test pinning its own answer.
        //
        // An unreadable block ALONGSIDE a readable one still makes the total unknown: counting
        // only what parsed would report a partial read as the channel's full size.
        //
        // KNOWN LIMIT, decided rather than overlooked: "fired but unreadable" and "never fired"
        // both come out as `NotMeasured`, because `ReasoningState` has no state between them and
        // adding one is public surface. It is not added because the distinction changes NO
        // decision: either way the operator cannot see the reasoning, and the remedy — raise the
        // budget, or turn the channel off where a provider honours it — is identical. What DOES
        // change their decision is where the budget went, and `completion_tokens` on the same
        // record answers that.
        //
        // The cost is real and belongs in the open: an opted-in trace is dropped when any block
        // is unreadable, since `NotMeasured` carries no text. If a consumer ever needs the
        // readable part of a partially-redacted channel, that is the evidence for a fourth
        // state — and this crate adds surface on evidence, not on anticipation.
        let thinking_blocks = response
            .content
            .iter()
            .filter(|b| b.type_ == "thinking" || b.type_ == "redacted_thinking");
        let mut any_unreadable = false;
        let mut readable_text = String::new();
        let mut any_block = false;
        for b in thinking_blocks {
            any_block = true;
            match b.thinking.as_deref() {
                Some(t) => readable_text.push_str(t),
                None => any_unreadable = true,
            }
        }
        let thought_text = readable_text;
        // READABLE is the condition, not merely PRESENT. A `redacted_thinking` block, or a
        // `thinking` block whose payload is absent, proves the channel fired but carries
        // nothing to count -- and reporting `Measured { chars: 0 }` for it would claim a look
        // that found nothing where the truth is a look that could not read. `NotMeasured` is
        // what the crate uses for exactly that, everywhere else.
        //
        // ONE value, not a flag beside a string: a separate `saw_thinking` boolean let `chars`
        // and `text` be decided independently, and they promptly disagreed.
        let readable: Option<&str> =
            (any_block && !any_unreadable).then_some(thought_text.as_str());
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
            // ASYMMETRIC with the compat wire ON PURPOSE, and it is not half a sweep: there,
            // `content: ""` means the model produced nothing and the budget is the only
            // explanation worth naming. Here a response can carry `tool_use` or
            // `redacted_thinking` blocks — content that is not text and was never cut — so the
            // termination reason is the only thing that separates a budget cut from a shape
            // this arm has no business calling one.
            //
            // NARROWED to a termination the budget could actually explain. Widened to "any
            // content with no text block", this arm told an operator to raise `max_tokens`
            // for a `tool_use` or `redacted_thinking` response the budget never cut -- which
            // is the misdiagnosis this whole release exists to end, re-created one level
            // down by the fix for it. An absent reason still qualifies: unknown is not the
            // same as known-to-be-something-else.
            // ONE condition over the whole table, not two ordered arms. Splitting it into
            // "empty text" and "no text" put them in an order, and the order was wrong twice in
            // opposite directions: first the narrowing swallowed an empty text block that the
            // compat wire calls an empty completion, then the fix for that preempted the
            // narrowing and turned a `tool_use` turn into a budget cut, and the fix for THAT
            // left the canonical case of this whole release -- an empty text block beside a
            // `thinking` block on a `max_tokens` cut -- reported as a broken contract.
            //
            // Enumerated instead, over what this wire can send once no usable text came back:
            //
            //   no blocks at all            -> contract: nothing was sent
            //   only text, all empty        -> EMPTY: the compat wire's `content: ""`, exactly
            //   text + non-text, cut        -> EMPTY: the budget explains it
            //   text + non-text, not cut    -> contract: the turn produced something else
            //   non-text only, cut          -> EMPTY
            //   non-text only, not cut      -> contract
            //
            // Which collapses to: the budget could explain it, OR nothing but text came back.
            // The budget question is asked of ONE function, the same one the message
            // branches on, so the guard and the wording cannot drift apart.
            Some(_) | None if had_content && (budget_routes_here(&telemetry) || !had_non_text) => {
                Err(ProviderError::EmptyCompletion { telemetry, cap })
            }
            _ => Err(ProviderError::response_contract(
                crate::error::ResponseContractCause::NoMessage,
                // Says what was OBSERVED. It read "no text block" and listed a text block in
                // the same sentence, because an empty one reaches here too — a message that
                // contradicts its own evidence sends the reader looking for the wrong thing.
                // Carries the MEASUREMENT too, not only the reason. A turn that produced a
                // 15 000-character thinking block and no text is a different problem from one
                // that produced nothing at all, and the shape list alone does not say which.
                format!(
                    "no usable text; termination {:?}, blocks [{}], completion_tokens {:?}, reasoning {}",
                    telemetry.finish,
                    block_types.join(", "),
                    telemetry.completion_tokens,
                    // The MEASUREMENT, never the trace. `{:?}` on the state would have
                    // embedded the opt-in reasoning TEXT -- model-authored, never through the
                    // `Validator`, never redacted -- into a field whose own rustdoc says it
                    // carries none of that. The fix for a leak is not to cap it, it is not to
                    // open the channel.
                    telemetry.reasoning.measurement()
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

    /// Drives the shipping entry point with the defaults these tests do not vary.
    fn parse_any(body: &str) -> Result<crate::provider::Completion, crate::error::ProviderError> {
        super::ClaudeProvider::parse_completion(
            body,
            crate::provider::ReasoningControl::Default,
            16_384,
            false,
        )
    }

    /// `parse_any` for the cases that must succeed.
    fn parse_ok(body: &str) -> crate::provider::Completion {
        parse_any(body).expect("parses")
    }

    /// Text extraction from the Claude response format.
    ///
    /// Against `parse_completion`, the entry point that ships. It used to be asserted through
    /// a second public parser that returned only the text -- which gave the two of them
    /// OPPOSITE answers for a reply whose only text block was empty, so the one that could
    /// not tell a budget cut from a broken shape was removed rather than reconciled.
    #[test]
    fn test_parse_claude_response_extracts_text_content() {
        let json = r#"{"content": [{"type": "text", "text": "response text"}], "id": "msg_1", "model": "claude-sonnet-4-6", "role": "assistant"}"#;
        let out = parse_ok(json);
        assert_eq!(out.text, "response text");
    }

    /// `parse_response` joins EVERY text block, because the message is the sequence of them.
    ///
    /// It used to return the first one and this test pinned that, which made the data loss look
    /// deliberate. Anthropic interleaves text with thinking and tool_use blocks, so a reply
    /// split across two text blocks came back truncated at the first — and with a `null` or
    /// empty first block, came back as nothing at all.
    #[test]
    fn test_parse_response_joins_every_text_block() {
        let json = r#"{"content": [{"type": "text", "text": "first"}, {"type": "text", "text": " second"}], "id": "msg_1", "model": "m", "role": "assistant"}"#;
        assert_eq!(parse_ok(json).text, "first second");
    }

    /// parse_response returns error when no text content block found.
    #[test]
    fn test_parse_response_error_when_no_text_block() {
        let json = r#"{"content": [], "id": "msg_1", "model": "m", "role": "assistant"}"#;
        assert!(matches!(
            parse_any(json),
            Err(crate::error::ProviderError::ResponseContract { .. })
        ));
    }

    /// parse_response returns error on invalid JSON.
    /// A verdict in a LATER text block is not lost to an earlier empty or null one.
    ///
    /// `text_of` took the first block of type `text` and then read its payload, so a first block
    /// carrying `null` — or `""` — discarded real content sitting behind it. The empty case was
    /// worse than losing the answer: it reported a budget cut, telling the operator to raise a
    /// ceiling that had cut nothing while the verdict was right there.
    #[test]
    fn a_verdict_in_a_later_text_block_survives_an_empty_or_null_earlier_one() {
        for first in [r#"{"type":"text","text":""}"#, r#"{"type":"text"}"#] {
            let json = format!(
                r#"{{"content":[{first},{{"type":"text","text":"the verdict"}}],
                     "stop_reason":"end_turn"}}"#
            );
            let out = super::ClaudeProvider::parse_completion(
                &json,
                super::ReasoningControl::Default,
                4096,
                false,
            )
            .unwrap_or_else(|e| panic!("{first}: the verdict is there, got {e:?}"));
            assert_eq!(out.text, "the verdict", "{first}");
        }
    }

    #[test]
    fn the_budget_question_is_asked_in_exactly_one_place_in_this_file() {
        // The claim that guard and message share ONE implementation is what makes the
        // agreement between them structural rather than remembered. Nothing enforced it:
        // re-inlining a copy of the rule here is behaviour-identical today, so no
        // behavioural test can see it, and the two copies then drift on the next edit --
        // which is exactly how this decision arrived at its third round.
        //
        // Scans source, which is coarse. It is the honest tool for the property, because
        // the property IS about the source: a second reader of `telemetry.finish` in this
        // file is the defect, whatever it computes.
        let src = include_str!("claude.rs");
        let body = src
            .split("mod tests {")
            .next()
            .expect("the non-test half is what ships");
        assert_eq!(
            body.matches("budget_bearing()").count(),
            1,
            "the budget rule is consulted from ONE place in this file -- `budget_routes_here`. A second caller is a second decision site even when it calls the same function, because the next edit only has to change one of them."
        );
        // `.finish` and not `telemetry.finish`: binding the telemetry to any other name --
        // `let t = &telemetry; t.finish` -- walked straight past the narrower spelling. The
        // `.finish(` subtraction is the `Debug` builder's method, which is not a read of the
        // field and must not be counted as one.
        let field_reads = body.matches(".finish").count() - body.matches(".finish(").count();
        assert_eq!(
            field_reads, 1,
            "`telemetry.finish` is read once outside the tests, to render the observed reason into the contract detail. A second read is how the budget question gets re-decided locally, which is the drift that took this decision to a fourth round."
        );
    }

    #[test]
    fn the_contract_detail_carries_the_measurement_and_never_the_trace() {
        // The detail gained the token counts and the reasoning state so an operator could
        // tell "reasoned 15 000 characters and emitted nothing" from "produced nothing at
        // all". Rendering the STATE with `{:?}` would have carried the opt-in trace with it
        // -- model-authored text, never through the `Validator`, never redacted -- into a
        // field whose rustdoc promises none of that. The measurement is the whole point;
        // the text is the leak.
        let secret = "deliberating about something private";
        let body = format!(
            r#"{{"content":[{{"type":"thinking","thinking":"{secret}"}},{{"type":"tool_use","id":"t"}}],"stop_reason":"end_turn"}}"#
        );
        let detail = match super::ClaudeProvider::parse_completion(
            &body,
            crate::provider::ReasoningControl::Default,
            16_384,
            // trace ON: the consumer opted in, which is exactly when the leak is possible
            true,
        ) {
            Err(crate::error::ProviderError::ResponseContract { detail, .. }) => detail,
            other => panic!("a tool_use turn with no text is a contract failure: {other:?}"),
        };
        assert!(
            detail.contains("chars"),
            "the measurement is why the field was widened: {detail}"
        );
        assert!(
            !detail.contains(secret),
            "the reasoning TRACE reached the contract detail: {detail}"
        );
    }

    #[test]
    fn the_wire_string_decides_the_advice_the_operator_reads() {
        // END TO END, from the raw `stop_reason` to the rendered sentence, because the
        // table above asserts VARIANTS and throws the telemetry away -- so nothing checked
        // that the reason reaching the guard is the reason reaching the message. Round C
        // rewrote the advice and the whole suite stayed green through a one-line mutation
        // of `map_stop_reason`'s fallback, which is how the two rounds before it got in.
        //
        // The shape is text-only-and-empty on purpose: it reaches `EmptyCompletion` for
        // every termination, so what the message says is decided by the termination alone.
        let cases: [(&str, &str); 10] = [
            // the budget may be the explanation -> the message carries its own fix
            (
                r#""stop_reason":"max_tokens""#,
                crate::error::REMEDY_PRESCRIBES,
            ),
            // Anthropic's out-of-room value. It is `Length` because it IS running out of
            // room; the hedge in that sentence is what keeps the advice honest for it.
            (
                r#""stop_reason":"model_context_window_exceeded""#,
                crate::error::REMEDY_PRESCRIBES,
            ),
            // absent: unknown is not evidence that the budget was untouched
            (r#""id":"msg_1""#, crate::error::REMEDY_PRESCRIBES),
            // reasons the backend named, none of them the budget
            (
                r#""stop_reason":"end_turn""#,
                crate::error::REMEDY_RULED_OUT,
            ),
            (
                r#""stop_reason":"stop_sequence""#,
                crate::error::REMEDY_RULED_OUT,
            ),
            (
                r#""stop_reason":"tool_use""#,
                crate::error::REMEDY_RULED_OUT,
            ),
            (r#""stop_reason":"refusal""#, crate::error::REMEDY_RULED_OUT),
            (
                r#""stop_reason":"pause_turn""#,
                crate::error::REMEDY_RULED_OUT,
            ),
            (r#""stop_reason":"load""#, crate::error::REMEDY_RULED_OUT),
            // a value no vendor publishes: neither direction is supportable
            (
                r#""stop_reason":"brand_new_reason""#,
                crate::error::REMEDY_UNKNOWN,
            ),
        ];

        for (stop, expected) in cases {
            let body = format!(r#"{{"content":[{{"type":"text","text":""}}],{stop}}}"#);
            let rendered = match super::ClaudeProvider::parse_completion(
                &body,
                crate::provider::ReasoningControl::Default,
                16_384,
                false,
            ) {
                Err(e) => e.to_string(),
                other => panic!("{stop} should be an empty completion -> {other:?}"),
            };
            assert!(
                rendered.contains(expected),
                "{stop} must render advice containing {expected:?} -> {rendered}"
            );
        }
    }

    /// Every shape this wire can send once no usable text came back, across every
    /// termination it can report. Asserting them together is what makes a later edit that
    /// satisfies one cell and breaks another fail here rather than three reviews downstream.
    #[test]
    fn every_shape_with_no_usable_text_lands_where_the_table_says() {
        let cut = r#""stop_reason":"max_tokens""#;
        let ended = r#""stop_reason":"end_turn""#;
        // The THIRD termination value, which the discriminator treats like a cut because
        // unknown is not the same as known-to-be-something-else. Enumerating only the two
        // present ones left that branch unasserted: deleting `None |` from the guard kept every
        // row green, which is a table that certifies an axis it does not cover.
        let absent = r#""id":"msg_1""#;
        // A reason the backend NAMED that is not the budget. It reads as `Stop` since the
        // translator was completed — the comment here used to say it landed in `Other`,
        // which stopped being true in the same commit that added these rows and left the
        // `Unknown` arm of the routing guard unexercised behind a name that claimed to
        // cover it.
        let named_other = r#""stop_reason":"refusal""#;
        // A value NO vendor publishes. This is what reaches `BudgetBearing::Unknown`, and
        // nothing else in this file does.
        let novel = r#""stop_reason":"brand_new_reason""#;
        // The fifth cell. `map_stop_reason` hands anything it does not know to
        // `from_wire`, which DOES interpret "load" — so this reaches `FinishReason::Load`
        // and not `Other`, and the axis has five states, not four.
        let loading = r#""stop_reason":"load""#;
        // A BLANK payload, not merely an empty one. The compat wire trims before deciding, so
        // "all empty" is really "all blank" and only half of it was asserted.
        let text_blank = r#"{"type":"text","text":"     "}"#;
        let text_empty = r#"{"type":"text","text":""}"#;
        let thinking = r#"{"type":"thinking","thinking":"deliberating"}"#;
        let tool = r#"{"type":"tool_use","id":"t"}"#;

        // (blocks, termination, expect_empty_completion)
        let table: [(&str, &str, bool); 21] = [
            // nothing was sent at all -> contract, whatever ended the turn
            ("", cut, false),
            // only text, and it came back empty -> the compat wire's `content: ""`, exactly
            (text_empty, ended, true),
            // THE CASE THIS RELEASE IS ABOUT: reasoned to the ceiling, emitted nothing
            (&format!("{thinking},{text_empty}"), cut, true),
            // the turn produced a tool call, not an exhausted budget
            (&format!("{tool},{text_empty}"), ended, false),
            // non-text only, cut by the budget
            (thinking, cut, true),
            // non-text only, ended normally -> the turn did something else
            (tool, ended, false),
            // ---- the third termination value: ABSENT, treated like a cut ----
            ("", absent, false),
            (&format!("{thinking},{text_empty}"), absent, true),
            (&format!("{tool},{text_empty}"), absent, true),
            (thinking, absent, true),
            // ---- blank, not merely empty: the compat wire trims before deciding ----
            (text_blank, ended, true),
            // ---- a reason the backend named, and it is not the budget ----
            // Text-only stays `EmptyCompletion`: the observation is that the reply was
            // empty, which is true whatever ended the turn, and that variant is the one
            // carrying the telemetry that names the refusal. What must not follow is the
            // PRESCRIPTION, and that is pinned in `error.rs`, not here.
            (text_empty, named_other, true),
            (thinking, named_other, false),
            (&format!("{thinking},{text_empty}"), named_other, false),
            ("", named_other, false),
            // ---- the FIFTH cell: `load`, which `from_wire` interprets even from this
            // wire, so the axis is now total over `Option<FinishReason>` and not merely
            // over the values Anthropic is documented to send ----
            (text_empty, loading, true),
            (thinking, loading, false),
            (&format!("{thinking},{text_empty}"), loading, false),
            // ---- the SIXTH cell: a reason no vendor publishes, the only one that reaches
            // `BudgetBearing::Unknown`. Without these the guard's `Unknown` arm was dead
            // code that could be flipped to `true` with the whole suite green ----
            (text_empty, novel, true),
            (thinking, novel, false),
            (&format!("{thinking},{text_empty}"), novel, false),
        ];

        for (blocks, stop, expect_empty) in table {
            let json = format!(r#"{{"content":[{blocks}],{stop}}}"#);
            let got = super::ClaudeProvider::parse_completion(
                &json,
                super::ReasoningControl::Default,
                4096,
                false,
            );
            // Asserts the OUTCOME, not a bit. `!is_empty` was satisfied by a success, a
            // panic-free `Ok`, or any other error — so a row could stop meaning what it says
            // and stay green, which is the whole defect this table exists to prevent.
            match (&got, expect_empty) {
                (Err(crate::error::ProviderError::EmptyCompletion { .. }), true) => {}
                (Err(crate::error::ProviderError::ResponseContract { .. }), false) => {}
                _ => panic!(
                    "blocks=[{blocks}] {stop} expected {} -> {got:?}",
                    if expect_empty {
                        "EmptyCompletion"
                    } else {
                        "ResponseContract"
                    }
                ),
            }
        }
    }

    #[test]
    fn an_empty_text_block_beside_a_tool_use_is_not_a_budget_cut() {
        let json = r#"{"content":[{"type":"tool_use","id":"t"},{"type":"text","text":""}],
                       "stop_reason":"tool_use"}"#;
        match super::ClaudeProvider::parse_completion(
            json,
            super::ReasoningControl::Default,
            4096,
            false,
        ) {
            Err(crate::error::ProviderError::ResponseContract { .. }) => {}
            other => panic!("the turn produced a tool call, not an exhausted budget: {other:?}"),
        }
    }

    #[test]
    fn test_parse_response_error_on_invalid_json() {
        assert!(matches!(
            parse_any("not json"),
            Err(crate::error::ProviderError::ResponseContract {
                reason: crate::error::ResponseContractCause::Unreadable,
                ..
            })
        ));
    }

    // -- Task 11: telemetry the HTTP provider CAN report --

    /// Step 3b: `stop_reason` and `usage` are read into the completion's
    /// telemetry. `"max_tokens"` maps to `FinishReason::Length` — the output
    /// budget cut, which is the signal the diagnosis axis exists to surface.
    /// An extended-thinking response that exhausts `max_tokens` comes back with a thinking
    /// block and NO text block. Propagating that with `?` used to discard the telemetry
    /// assembled one line above -- including the `max_tokens` stop reason -- and report a
    /// broken contract instead of the budget cut it actually was. Third wire, same defect.
    /// An EMPTY text block is an empty completion on both wires, whatever ended the turn.
    ///
    /// The narrowing that stopped calling a `tool_use` response a budget cut also caught this,
    /// and it should not have: the contract shape DID come back, the content is simply empty, so
    /// the model returned nothing exactly as it does on the compat wire — which classifies the
    /// same shape as `EmptyCompletion` regardless of termination. One shape both wires receive,
    /// answered two ways, is the cross-wire disagreement this round closed one level up.
    #[test]
    fn an_empty_text_block_is_an_empty_completion_like_the_other_wire() {
        for stop in ["end_turn", "stop_sequence"] {
            let json =
                format!(r#"{{"content":[{{"type":"text","text":""}}],"stop_reason":"{stop}"}}"#);
            match super::ClaudeProvider::parse_completion(
                &json,
                super::ReasoningControl::Default,
                4096,
                false,
            ) {
                Err(crate::error::ProviderError::EmptyCompletion { .. }) => {}
                other => panic!("{stop}: expected EmptyCompletion, got {other:?}"),
            }
        }

        // And the narrowing still holds for what it was FOR: a non-text block the budget never
        // cut stays a contract failure.
        match super::ClaudeProvider::parse_completion(
            r#"{"content":[{"type":"tool_use","id":"t"}],"stop_reason":"tool_use"}"#,
            super::ReasoningControl::Default,
            4096,
            false,
        ) {
            Err(crate::error::ProviderError::ResponseContract { .. }) => {}
            other => panic!("a tool_use response is not a budget cut: {other:?}"),
        }
    }

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
    /// The two wires must give the SAME answer for a present-but-empty reasoning payload.
    ///
    /// They did not: testing emptiness made `thinking: ""` report as "nobody looked" here while
    /// the compat wire called the identical body a measured zero — two wires contradicting each
    /// other, each with a test pinning its own answer. `""` is a payload that was READ and was
    /// empty, which is exactly what `Some(0)` means and exactly what `NotMeasured` does not.
    #[test]
    fn a_present_but_empty_thinking_payload_is_a_measured_zero_like_the_other_wire() {
        let json = r#"{"content":[{"type":"thinking","thinking":""},
                                  {"type":"text","text":"a"}]}"#;
        // BOTH arms, like the sibling test in this file. Pinning one and covering the other
        // "by construction" is how the previous round's half-sweep looked whole.
        let default = super::ClaudeProvider::parse_completion(
            json,
            super::ReasoningControl::Default,
            4096,
            false,
        )
        .expect("parses");
        assert!(
            matches!(
                default.telemetry.reasoning,
                crate::provider::ReasoningState::Measured { chars: 0, .. }
            ),
            "an empty payload was READ and was empty, got {:?}",
            default.telemetry.reasoning
        );

        let disabled = super::ClaudeProvider::parse_completion(
            json,
            super::ReasoningControl::Disabled,
            4096,
            false,
        )
        .expect("parses");
        assert!(
            matches!(
                disabled.telemetry.reasoning,
                crate::provider::ReasoningState::Unsupported { chars: Some(0), .. }
            ),
            "the declaration carries the same measured zero, got {:?}",
            disabled.telemetry.reasoning
        );
    }

    /// An unreadable block beside a readable one makes the channel's size UNKNOWN.
    ///
    /// Counting only what parsed would report a partial read as the channel's full size — a
    /// number that looks like a measurement of the whole and is a measurement of a part.
    #[test]
    fn a_redacted_block_beside_a_readable_one_is_not_a_complete_measurement() {
        let json = r#"{"content":[{"type":"thinking","thinking":"abc"},
                                  {"type":"redacted_thinking","data":"enc"},
                                  {"type":"text","text":"a"}]}"#;
        let out = super::ClaudeProvider::parse_completion(
            json,
            super::ReasoningControl::Default,
            4096,
            false,
        )
        .expect("parses");
        assert!(
            matches!(
                out.telemetry.reasoning,
                crate::provider::ReasoningState::NotMeasured
            ),
            "a partial read is not the channel's size, got {:?}",
            out.telemetry.reasoning
        );
    }

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
