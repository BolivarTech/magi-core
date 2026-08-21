// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05

use crate::backoff::RetryClass;
use crate::error::{AbandonReason, ExternalErrorKind, ProviderError, ResponseContractCause};
use crate::schema::Mode;
use std::sync::Arc;
use std::time::Duration;

/// What the caller wants done with a backend's separate reasoning channel.
///
/// `#[non_exhaustive]` because a token budget for that channel is the obvious
/// next variant, and it must not cost another major.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReasoningControl {
    /// Say nothing on the wire — the backend's own default applies.
    ///
    /// This must mean exactly that, never "send `think: true`": a crate that
    /// started asserting a control nobody asked for would change behaviour for
    /// every existing consumer who never mentioned reasoning.
    #[default]
    Default,
    /// Ask the backend to skip its reasoning channel.
    ///
    /// **Measured inert on the OpenAI-compatible endpoint** (evidence run G:
    /// accepted with HTTP 200, no effect) and effective on the native one. That
    /// asymmetry is why `OllamaProvider` completes on the native path
    /// unconditionally rather than shipping a knob that appears to work.
    Disabled,
}

/// Configuration for LLM completion requests.
///
/// Controls parameters like token limits and sampling temperature.
/// Marked `#[non_exhaustive]` to allow adding fields in future versions
/// without breaking downstream crates.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct CompletionConfig {
    /// Maximum number of tokens in the LLM response.
    ///
    /// Defaults to `16_384` since `4.0.0`, raised from `4096` because that number was cutting
    /// verdicts a model had genuinely produced. With the real system prompt on a 62 k-token
    /// bundle, `glm-5.2` demanded **10 686** completion tokens — so the old default truncated a
    /// legitimate answer from a model that converges, not merely from a pathological one.
    ///
    /// # What the raise does NOT buy
    ///
    /// A model that spends its whole budget in a reasoning channel is not rescued by a bigger
    /// budget: `deepseek-v4-pro` was measured returning nothing at `16_384` **and** at `32_768`.
    /// That is what [`ReasoningControl`] is for. Reading this number as the fix repeats the first
    /// hypothesis the reporters measured until it broke.
    ///
    /// # One backend takes this value verbatim, and can reject it
    ///
    /// [`ClaudeProvider`](crate::providers::claude::ClaudeProvider) passes `max_tokens` straight
    /// through without comparing it against the model's own output ceiling, and asking for more
    /// than that ceiling is a **400 from Anthropic**, not a degraded answer. It is deliberately
    /// not clamped: a silent clamp would be exactly the quiet no-op this crate refuses elsewhere.
    ///
    /// The three aliases this crate resolves — `sonnet`, `opus`, `haiku` — are all 4.x models
    /// whose ceilings are far above `16_384`, so a consumer on the default configuration cannot
    /// hit this. What can: pinning a **literal pre-4.x model id** through the passthrough and
    /// never setting `max_tokens`. Set it explicitly if you do that.
    pub max_tokens: u32,
    /// Sampling temperature (0.0 = deterministic).
    pub temperature: f64,
    /// What to do with the backend's reasoning channel, if it has one.
    ///
    /// Defaults to [`ReasoningControl::Default`] — say nothing on the wire.
    pub reasoning: ReasoningControl,
    /// Whether the report should carry the reasoning trace's TEXT, not just its length.
    ///
    /// Opt-in, `false` by default. **Additive, never a replacement**: with `false` the report
    /// still carries the trace's length; `true` adds the text on top. The length never
    /// disappears, so no consumer loses information by leaving this off — and no report grows by
    /// surprise, since the default changes nothing.
    ///
    /// Named `reasoning_trace` and not `reasoning`, because
    /// [`AgentOutput::reasoning`](crate::schema::AgentOutput::reasoning) is one of the seven
    /// verdict keys and is **always** present. That is the model reasoning *inside* its verdict;
    /// this is the provider's own channel, *before* it. Two different things under one name in
    /// the same output would be a defect, not a shortcut.
    ///
    /// # Turning this on accepts four things, and they are named here rather than pointed at
    ///
    /// 1. **The text is the model's**, not this crate's. Nothing here authored it and nothing
    ///    here vouches for it.
    /// 2. **It does not pass the `Validator`.** The verdict and the findings do — length limits,
    ///    invisible-codepoint stripping, NaN rejection. This does not.
    /// 3. **It is not redacted.** It is a third channel of the open backlog item *"text this
    ///    crate did not author is NOT redacted"*, alongside an external provider's message and a
    ///    server's echoed error body. A secret a model repeats back lands in the report verbatim.
    /// 4. **It has no cap, and how big it gets is a formula, not a number.** Every completion is
    ///    recorded, so a seat that rotates accumulates one trace per model: worst case
    ///    `(1 + max_rotations) x calls_per_model` traces **per agent**, times three agents —
    ///    **up to 18 per run** with the shipped defaults. Traces of **~141 000 characters per
    ///    agent** have been measured, which is a **measured reference and not a ceiling**:
    ///    another model reasons more and the number grows.
    ///
    /// The absence of a cap is deliberate. A truncated trace reads as a complete shorter one,
    /// and whoever turns this on to find out why a model spent its budget needs the **end** of
    /// it, which is where convergence shows. Truncating would hand over the half that does not
    /// help. If a cap is ever needed it is additive and costs no major.
    ///
    /// # Which providers can fill it
    ///
    /// | provider | trace channel |
    /// |---|---|
    /// | [`OllamaProvider`](crate::providers::ollama::OllamaProvider) | `message.thinking` |
    /// | [`OpenAiCompatibleProvider`](crate::providers::openai_compat::OpenAiCompatibleProvider) | `message.reasoning` |
    /// | the Claude providers | none — they report [`ReasoningState::NotMeasured`] |
    ///
    /// `NotMeasured` there, never `Measured { chars: 0 }`, which would claim a look that never
    /// happened. It is a different question from [`ReasoningState::Unsupported`], which is about
    /// the [`ReasoningControl`]: a backend that cannot switch reasoning off still reasons, so
    /// that variant reports the length it saw anyway — the trace coming back is the proof the
    /// control had no effect, and its size is what a consumer wants from that proof.
    pub reasoning_trace: bool,
}

impl Default for CompletionConfig {
    fn default() -> Self {
        Self {
            max_tokens: 16_384,
            temperature: 0.0,
            reasoning: ReasoningControl::default(),
            reasoning_trace: false,
        }
    }
}

impl CompletionConfig {
    /// Sets how the backend's reasoning channel should be handled.
    ///
    /// # Parameters
    ///
    /// * `r` — the control to request.
    ///
    /// # Returns
    ///
    /// `self`, so calls chain. A builder because [`CompletionConfig`] is
    /// `#[non_exhaustive]` and cannot be constructed with a struct literal from
    /// outside this crate.
    pub fn with_reasoning(mut self, r: ReasoningControl) -> Self {
        self.reasoning = r;
        self
    }

    /// Opts into carrying the reasoning trace's text in the report, in addition
    /// to its length.
    ///
    /// # Parameters
    ///
    /// * `on` — `true` to include the text; `false` (the default) to keep only
    ///   the length.
    ///
    /// # Returns
    ///
    /// `self`, so calls chain.
    pub fn with_reasoning_trace(mut self, on: bool) -> Self {
        self.reasoning_trace = on;
        self
    }
}

/// Maximum number of **characters** kept in [`FinishReason::Other`].
///
/// Characters, not bytes: the cut lands on a character boundary so a multi-byte
/// value cannot panic or produce invalid UTF-8.
const MAX_FINISH_REASON_CHARS: usize = 64;

/// Why the model stopped generating.
///
/// `#[non_exhaustive]` **and** carrying [`FinishReason::Other`]: the captured
/// corpus observed three values — `stop`, `length` and `load` — and `load`
/// appeared in no documentation the project had, so the space is not closed. A
/// plain `String` would force consumers to compare text; a closed enum would
/// break on the next value a backend invents.
///
/// # Wire format
///
/// Serialized as a plain string and deserialized through [`FinishReason::from_wire`],
/// so a report round-trip and a wire parse take the same code path and cannot
/// drift.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinishReason {
    /// The model finished on its own.
    Stop,
    /// The output budget ran out before the model finished.
    Length,
    /// The backend answered without generating, while loading the model.
    Load,
    /// A value this crate does not know, kept verbatim up to 64 characters.
    ///
    /// The cap is `MAX_FINISH_REASON_CHARS`, which stays private: it is a bound
    /// on wire-sourced text, not a knob, and nothing outside the crate consumes
    /// it. The number is written out here because a doc link to a private item
    /// does not resolve, and CI denies rustdoc warnings.
    Other(String),
}

// Serde is written by hand, and this is not gold-plating: a plain
// `#[derive(Deserialize)]` would be wrong in a way that only shows up on the
// wire. With `rename_all`, the unit variants read `"stop"` fine, but the newtype
// variant would expect `{"Other": "brand_new"}`. A bare unknown string — exactly
// the case `Other` exists for, since the capture campaign found `load` in no
// documentation the project had — would FAIL to deserialize instead of landing
// in `Other`.
impl serde::Serialize for FinishReason {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(match self {
            Self::Stop => "stop",
            Self::Length => "length",
            Self::Load => "load",
            Self::Other(o) => o,
        })
    }
}

impl<'de> serde::Deserialize<'de> for FinishReason {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        Ok(Self::from_wire(&raw))
    }
}

impl FinishReason {
    /// Maps a wire value to a reason.
    ///
    /// # Parameters
    ///
    /// * `raw` — the value the backend sent, in either wire format.
    ///
    /// # Returns
    ///
    /// The matching known variant, or [`FinishReason::Other`] holding `raw`
    /// **truncated at 64 characters** (`MAX_FINISH_REASON_CHARS`) — characters, not
    /// bytes, cut on a character boundary, so it never panics on multi-byte
    /// input.
    ///
    /// # Complexity
    ///
    /// `O(n)` in the length of `raw`, single pass, no allocation for a known
    /// value.
    ///
    /// # Why capped at all, and why 64
    ///
    /// This is text **from the wire** landing in public, serialized telemetry,
    /// and this crate caps everything else that comes from outside. 64 is
    /// generous for any real label — the three known ones are 4 to 6 characters —
    /// and far below what a hostile backend could send.
    ///
    /// # Why truncating is right here and wrong for a reasoning trace
    ///
    /// A clipped label still identifies the reason; a clipped trace loses its
    /// END, which is exactly where convergence shows. Same operation, opposite
    /// verdict, because what survives the cut is different.
    ///
    /// # What a consumer sees
    ///
    /// A truncated value is **not marked**: adding an ellipsis would make the
    /// string differ from the wire's for reasons of ours. A consumer comparing
    /// against a known label of 64 characters or fewer is unaffected.
    ///
    /// # Errors
    ///
    /// None — the mapping is total.
    ///
    /// ```
    /// # use magi_core::provider::FinishReason;
    /// assert_eq!(FinishReason::from_wire("length"), FinishReason::Length);
    /// assert_eq!(
    ///     FinishReason::from_wire("brand_new"),
    ///     FinishReason::Other("brand_new".to_string())
    /// );
    /// ```
    pub fn from_wire(raw: &str) -> Self {
        match raw {
            "stop" => Self::Stop,
            "length" => Self::Length,
            "load" => Self::Load,
            other => {
                let cut = other
                    .char_indices()
                    .nth(MAX_FINISH_REASON_CHARS)
                    .map_or(other.len(), |(i, _)| i);
                Self::Other(other[..cut].to_string())
            }
        }
    }
}

/// Whether the provider could honour the reasoning control, and what it measured.
///
/// A typed state, not an `Option`: `None` would be ambiguous between "the backend
/// cannot do this" and "it can and the model did not reason", and a consumer who
/// needs to branch would be left matching on text.
///
/// # The variants are constructible on purpose, and that has a price
///
/// The enum carries `#[non_exhaustive]` but its **variants deliberately do not**, unlike every
/// struct-like variant of [`ProviderError`]. The attribute would forbid construction from
/// another crate, and an external [`LlmProvider`] has to be able to report what it measured —
/// which is the whole reason `complete` returns a [`Completion`] rather than a `String`. Closing
/// them would re-create, one level down, the `E0639` trap that left `ProviderError`
/// unconstructible from outside this crate across six published releases before a consumer
/// reported it.
///
/// The price is real and is stated rather than hidden: **adding a field to a variant here is a
/// breaking change**, where adding one to a `ProviderError` variant is not. `4.0.0` already paid
/// it once — `Unsupported` gained `chars` and `text` mid-milestone — so treat the shape as not
/// yet settled, and prefer a new variant over a new field where the two would serve equally.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ReasoningState {
    /// **Nobody measured.** NOT `Measured { chars: 0 }`, which would assert that
    /// something looked and saw zero. It is the same criterion that makes the
    /// completion counters `Option`, and that made input telemetry an `Option`
    /// instead of a struct of zeros — a zero meaning "could not measure" is
    /// indistinguishable from a real zero.
    NotMeasured,
    /// The backend cannot honour the reasoning control at all, and says so
    /// instead of ignoring it in silence.
    ///
    /// `String`, **not `&'static str`** — and the reason is mechanical, not
    /// stylistic: this type travels inside the serialized report, which is
    /// **deserialized**, and no `Deserialize` impl can produce a `&'static str`
    /// from borrowed input.
    ///
    /// # It carries what came back anyway, and that is the point
    ///
    /// A backend that cannot switch reasoning off still reasons, and the trace it returns is
    /// the **proof** the control had no effect. Discarding its length would throw away the
    /// operator's actual question in a mixed trio — *how much is the seat that cannot honour
    /// this still spending?* — and would quietly break the contract that the length travels
    /// either way.
    Unsupported {
        /// The backend that cannot honour it, so a human reading the report knows
        /// which seat is unaffected by the control it set.
        backend: String,
        /// Length of the reasoning trace that came back regardless, in characters — when it
        /// could be measured at all.
        ///
        /// `Option` because the three backends that report this state can each reach it without
        /// a number to give: a wire with no separate reasoning channel has nothing to read, a
        /// compatibility body can omit the field entirely, and an Anthropic response can carry a
        /// `redacted_thinking` block that proves the channel FIRED while carrying nothing
        /// countable. All three used to report `0`, and the sentence here used to say that zero
        /// was a real zero — which turned "we could not read it" into "the model did not
        /// reason", and on this variant that reads as though the control had worked, when the
        /// variant exists to declare that it did not.
        ///
        /// `Some(0)` therefore keeps its meaning and is worth having: the channel was read and
        /// was empty. It is the same distinction [`crate::reporting::MagiReport::input_size`]
        /// draws with its own `Option`, for the same reason.
        chars: Option<usize>,
        /// The trace itself, present only when the consumer opted in — the same rule
        /// [`ReasoningState::Measured`] follows.
        text: Option<String>,
    },
    /// The backend honoured the control and measured the reasoning channel.
    Measured {
        /// Length of the reasoning trace in characters. Zero here is a real zero:
        /// the backend looked and the model did not reason.
        chars: usize,
        /// The trace itself, present only when the consumer opted in.
        text: Option<String>,
    },
}

/// One completion, with whatever the provider could measure about producing it.
///
/// Returned instead of a bare `String` because a completion's diagnosis does not
/// fit in its text: how the model stopped, what it spent, and whether the
/// reasoning channel was even observable are all facts a caller needs and a
/// `String` throws away.
///
/// `#[non_exhaustive]` plus a builder, not a fixed-arity constructor: the
/// attribute forbids literal construction from another crate, so a `new` taking
/// every field would break every external implementor the first time a field is
/// added — which is the trap the builder exists to avoid.
///
/// ```
/// # use magi_core::provider::Completion;
/// // The one-line migration for an external provider that measures nothing.
/// let c: Completion = "verdict text".to_string().into();
/// assert_eq!(c.telemetry.completion_tokens, None);
/// ```
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct Completion {
    /// The text the model produced.
    pub text: String,
    /// What the provider could measure. See [`CompletionTelemetry::unmeasured`]
    /// for the honest default.
    pub telemetry: CompletionTelemetry,
}

impl Completion {
    /// Builds a completion from its text alone, with telemetry set to
    /// **unmeasured**.
    ///
    /// # Parameters
    ///
    /// * `text` — the completion text.
    ///
    /// # Returns
    ///
    /// A completion whose telemetry asserts nothing, since nothing was measured.
    /// Add measurements with [`Completion::with_telemetry`].
    pub fn new(text: String) -> Self {
        Self {
            text,
            telemetry: CompletionTelemetry::unmeasured(),
        }
    }

    /// Attaches measured telemetry, replacing whatever was there.
    ///
    /// # Parameters
    ///
    /// * `t` — the telemetry the provider measured.
    ///
    /// # Returns
    ///
    /// `self`, so calls chain.
    pub fn with_telemetry(mut self, t: CompletionTelemetry) -> Self {
        self.telemetry = t;
        self
    }
}

impl From<String> for Completion {
    /// The one-line migration path for an external provider: `Ok(text.into())`.
    ///
    /// Equivalent to [`Completion::new`] — telemetry says **not measured**, never
    /// zeros.
    fn from(t: String) -> Self {
        Self::new(t)
    }
}

/// What a provider could measure about one completion.
///
/// Every field is optional or a typed state, and that is the point: a `0` meaning
/// "could not measure" is indistinguishable from a real zero, so this type never
/// reports one. An external provider that measures nothing produces
/// [`CompletionTelemetry::unmeasured`], which says exactly that.
///
/// `#[non_exhaustive]` plus builders, for the same reason as [`Completion`]: a
/// constructor taking every field would break external implementors the first
/// time one is added, leaving them unable to report telemetry they hold.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct CompletionTelemetry {
    /// Why the model stopped, when the backend said.
    pub finish: Option<FinishReason>,
    /// Tokens the completion itself consumed, when the backend counted them.
    pub completion_tokens: Option<u32>,
    /// Tokens the prompt consumed, when the backend counted them.
    pub prompt_tokens: Option<u32>,
    /// Whether the reasoning control was honoured, and what was measured.
    pub reasoning: ReasoningState,
}

impl CompletionTelemetry {
    /// The honest starting point: **nothing was measured**.
    ///
    /// # Returns
    ///
    /// Telemetry whose every field declares absence of measurement, including the
    /// reasoning one. Putting `Measured { chars: 0 }` there would have made the
    /// one type that exists to distinguish the three reasoning states report the
    /// wrong one by default.
    pub fn unmeasured() -> Self {
        Self {
            finish: None,
            completion_tokens: None,
            prompt_tokens: None,
            reasoning: ReasoningState::NotMeasured,
        }
    }

    /// Records why the model stopped.
    ///
    /// # Parameters
    ///
    /// * `r` — the reason the backend reported.
    ///
    /// # Returns
    ///
    /// `self`, so calls chain.
    pub fn with_finish(mut self, r: FinishReason) -> Self {
        self.finish = Some(r);
        self
    }

    /// Records the tokens the completion consumed.
    ///
    /// # Parameters
    ///
    /// * `n` — the count the backend reported.
    ///
    /// # Returns
    ///
    /// `self`, so calls chain.
    pub fn with_completion_tokens(mut self, n: u32) -> Self {
        self.completion_tokens = Some(n);
        self
    }

    /// Records the tokens the prompt consumed.
    ///
    /// # Parameters
    ///
    /// * `n` — the count the backend reported.
    ///
    /// # Returns
    ///
    /// `self`, so calls chain.
    pub fn with_prompt_tokens(mut self, n: u32) -> Self {
        self.prompt_tokens = Some(n);
        self
    }

    /// Records what happened to the reasoning channel.
    ///
    /// # Parameters
    ///
    /// * `s` — the state observed, including
    ///   [`ReasoningState::Unsupported`] when the backend cannot honour the
    ///   control at all.
    ///
    /// # Returns
    ///
    /// `self`, so calls chain.
    pub fn with_reasoning(mut self, s: ReasoningState) -> Self {
        self.reasoning = s;
        self
    }

    /// Whether the output budget could be what left the completion empty.
    ///
    /// The predicate is deliberately the same one the Anthropic provider branches
    /// on, and for the same reason: an ABSENT termination is not evidence that the
    /// budget was untouched, while any termination the backend did name — a normal
    /// stop, a load, or a value this crate does not recognise — is. `map_stop_reason`
    /// turns `max_tokens` into [`FinishReason::Length`] and everything else into
    /// something that is not it, so a named reason other than `Length` says the
    /// budget is not the explanation.
    ///
    /// It exists so an error message can name the cap as the FIX only where the fix
    /// applies. Prescribing "raise `max_tokens`" for a refusal is the misdiagnosis
    /// this release was written to end, wearing different clothes.
    ///
    /// # Returns
    ///
    /// `true` when the termination is unknown or was the budget running out.
    pub(crate) fn budget_may_explain_empty(&self) -> bool {
        matches!(self.finish, None | Some(FinishReason::Length))
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
///
/// # Implementing this outside `magi-core` — a supported extension point
///
/// ## Report failures with [`ProviderError::external`]
///
/// It is the **only** constructor reachable from another crate, and that is deliberate rather
/// than an accident of visibility. Every variant of [`ProviderError`] is `#[non_exhaustive]`, so
/// none can be built with a struct expression from outside; the transport variants stay closed
/// because their fields decide which model **lineages get condemned**, which is a run-level
/// consequence an external crate should not be able to reach for.
///
/// ```
/// # use magi_core::prelude::*;
/// let err = ProviderError::external("backend unreachable", ExternalErrorKind::Network);
/// assert!(err.to_string().contains("unreachable"));
/// ```
///
/// ## The `kind` names a shape; it does not choose a consequence
///
/// You declare *what kind* of failure happened. Whether it is retried, and how far its
/// condemnation reaches, is decided here — see [`ExternalErrorKind`]. In particular
/// `ExternalErrorKind::Network` never feeds the endpoint-down latch: this crate has no way to
/// verify what a third-party backend's outage implies about the lineages the **other** agents are
/// using, and aborting a whole run on that inference is not recoverable.
///
/// ## Your message is not redacted
///
/// It travels into the report like any other failure text. It is size-capped, but this crate
/// cannot clean it — recognising a secret inside arbitrary prose is not something a library can
/// do. **Do not put credentials in it.**
///
/// A complete implementation lives in `examples/external_provider.rs`.
///
/// [`ProviderError::external`]: crate::error::ProviderError::external
/// [`ExternalErrorKind`]: crate::error::ExternalErrorKind
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
    /// The LLM's text response.
    ///
    /// # Errors
    ///
    /// Any [`ProviderError`]. Which variant depends on who implements this:
    ///
    /// - **Implementations in this crate** use the transport variants — `Network`, `Timeout`,
    ///   `Auth`, `Http`, `ResponseTooLarge`, `Process`, `NestedSession` — whose fields drive retry
    ///   classification and lineage condemnation.
    /// - **Implementations outside this crate** use [`ProviderError::external`], the only
    ///   constructor reachable from another crate. It names the SHAPE of the failure; the
    ///   consequences stay here.
    ///
    /// [`ProviderError::external`]: crate::error::ProviderError::external
    ///
    /// # Migrating from 3.x
    ///
    /// Two changes, and only two: the **signature** (`Result<String, _>` becomes
    /// `Result<Completion, _>`) and the **return** (`Ok(text)` becomes `Ok(text.into())`) —
    /// plus any helper of your own that returns the old type.
    ///
    /// An implementor that measures nothing gets [`CompletionTelemetry::unmeasured`], which
    /// **says so** rather than reporting zeros. That distinction is the point of the break: a
    /// zero token count that means "nobody looked" is indistinguishable from one that means
    /// "the model emitted nothing", and this crate spent a release learning the difference.
    async fn complete(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        config: &CompletionConfig,
    ) -> Result<Completion, ProviderError>;

    /// Returns the provider's name (e.g., "claude", "claude-cli", "openai").
    fn name(&self) -> &str;

    /// Returns the model identifier (e.g., "claude-sonnet-4-6").
    fn model(&self) -> &str;
}

/// Resolves a short Claude model alias to a full model identifier.
///
/// Used by both `ClaudeProvider` (HTTP) and `ClaudeCliProvider` (subprocess).
/// Other providers (Gemini, OpenAI) should implement their own alias resolvers.
///
/// # Aliases
///
/// - `"sonnet"` → `"claude-sonnet-4-6"`
/// - `"opus"` → `"claude-opus-4-7"`
/// - `"haiku"` → `"claude-haiku-4-5-20251001"`
/// - Any string containing `"claude-"` passes through as-is
///
/// # Errors
///
/// Returns `ProviderError::Auth` if the alias is unknown.
///
/// # Examples
///
/// ```
/// use magi_core::provider::resolve_claude_alias;
///
/// assert_eq!(resolve_claude_alias("opus").unwrap(), "claude-opus-4-7");
/// assert_eq!(resolve_claude_alias("claude-custom").unwrap(), "claude-custom");
/// assert!(resolve_claude_alias("unknown").is_err());
/// ```
pub fn resolve_claude_alias(model: &str) -> Result<String, ProviderError> {
    match model {
        "sonnet" => Ok("claude-sonnet-4-6".to_string()),
        "opus" => Ok("claude-opus-4-7".to_string()),
        "haiku" => Ok("claude-haiku-4-5-20251001".to_string()),
        m if m.contains("claude-") => Ok(m.to_string()),
        _ => Err(ProviderError::Auth {
            message: format!("unknown model alias: {model}"),
        }),
    }
}

/// Resolves the default model short-name (`"opus"`, `"sonnet"`, `"haiku"`)
/// recommended for the given analysis mode.
///
/// Mirrors Python's `MODE_DEFAULT_MODELS` (MAGI@v2.2.8 `models.py:58-62`).
/// As of v0.4.0 all three modes default to `"opus"` per Python parity.
/// Pair with [`resolve_claude_alias`] to obtain the full model id:
///
/// ```
/// use magi_core::provider::{default_model_for_mode, resolve_claude_alias};
/// use magi_core::schema::Mode;
///
/// let alias = default_model_for_mode(Mode::Analysis);
/// let model_id = resolve_claude_alias(alias).unwrap();
/// assert_eq!(model_id, "claude-opus-4-7");
/// ```
///
/// # Arguments
///
/// * `mode` — The analysis mode whose default model alias to return.
///
/// # Returns
///
/// The short alias name (always `"opus"` in v0.4.0). Future versions may
/// route different modes to different defaults without breaking this API.
pub fn default_model_for_mode(mode: Mode) -> &'static str {
    match mode {
        Mode::CodeReview => "opus",
        Mode::Design => "opus",
        Mode::Analysis => "opus",
    }
}

/// Named default values (no magic numbers).
const DEFAULT_MAX_RETRIES: u32 = 3;
const DEFAULT_BASE_DELAY: Duration = Duration::from_secs(1);
const DEFAULT_CAP: Duration = Duration::from_secs(60);
const DEFAULT_RETRY_AFTER_CAP: Duration = Duration::from_secs(300);
const DEFAULT_OPERATION_BUDGET: Duration = Duration::from_secs(600);

/// Default **total** request timeout for the HTTP providers.
///
/// Total, not connect or read: the failure mode that matters — a model that
/// accepts the connection, returns headers, and then **hangs generating** — is
/// caught by no connect-timeout, and a read-timeout resets on each byte. 300 s is
/// generous for an LLM completion; who needs more uses the provider's
/// `with_timeout` constructor. `Duration::MAX` means "no timeout".
pub const DEFAULT_CLIENT_TIMEOUT: Duration = Duration::from_secs(300);

/// Retry configuration for [`RetryProvider`].
///
/// Set **only at construction** and then immutable: there is no public way to
/// mutate it afterwards, so that the dangerous-configuration warnings cannot be
/// evaded.
///
/// `RetryConfig` is `#[non_exhaustive]`: build it from [`Default`] and then
/// adjust fields (the struct-literal `RetryConfig { .. }` does not compile
/// outside the crate — that is the 2.0 migration pattern).
///
/// # Layering against the per-agent timeout — the defaults do NOT satisfy it
///
/// A retry chain costs `operation_budget + client_timeout`, and the client timeout applies **per
/// attempt**. For this budget to be reachable when a provider hangs:
///
/// ```text
/// operation_budget + client_timeout <= MagiConfig::timeout
/// ```
///
/// The defaults give `600 + 300 = 900` against a 300 s agent timeout, so on a hang the first
/// attempt consumes the whole agent budget and **none of the retries below ever run**. Every knob
/// here is then inert for that failure mode — which is worth knowing before tuning them.
///
/// Fixing the numbers is a latency trade-off, not a bug fix. **It is tracked for 3.3.0**, starting
/// from a configuration that puts this budget *below* the agent ceiling so that abandonment is
/// typed and diagnosable rather than an opaque cut. Said here
/// as well as on [`MagiConfig::timeout`] deliberately: whoever tunes retries does not necessarily
/// read the orchestrator's config, and a layering rule documented on one side only is a rule that
/// gets broken from the other.
///
/// [`MagiConfig::timeout`]: crate::orchestrator::MagiConfig::timeout
///
/// ```
/// use magi_core::prelude::*;
/// use std::time::Duration;
///
/// let mut cfg = RetryConfig::default();
/// cfg.max_retries = 5;
/// cfg.cap = Duration::from_secs(30);
///
/// assert_eq!(cfg.max_retries, 5);
/// assert_eq!(cfg.cap, Duration::from_secs(30));
/// // The rest of the defaults still apply.
/// assert_eq!(cfg.base_delay, Duration::from_secs(1));
/// ```
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct RetryConfig {
    /// Maximum retries after the first failure.
    pub max_retries: u32,
    /// Base wait. `ZERO` disables **both the pause and the jitter** — see warnings.
    pub base_delay: Duration,
    /// Ceiling for our own backoff. Mandatory: there is no "no ceiling".
    pub cap: Duration,
    /// Maximum `Retry-After` we accept obeying; exceeding it abandons.
    ///
    /// The `Retry-After` header is parsed in **whole seconds** (RFC 7231
    /// delta-seconds), so this cap is compared at whole-second granularity: a
    /// **sub-second** `retry_after_cap` (e.g. 500 ms) rounds down to 0 s, which
    /// makes **every** positive `Retry-After` (>= 1 s) exceed it and abandon.
    /// Use whole-second values; `ZERO` is the explicit "ignore the header" opt-out.
    pub retry_after_cap: Duration,
    /// Hard cap on the total retry time. `Duration::MAX` disables it.
    ///
    /// `Duration::ZERO` is **not** the opt-out: since the check is reactive
    /// (`elapsed >= budget` before each attempt), a zero budget is already met
    /// on the first check and yields **zero retries** — it behaves like
    /// `max_retries = 0`, not "start now". It is legitimate but almost always a
    /// mistake, and unlike the other zero-valued settings it emits no warning of
    /// its own — the symptom shows up at runtime. For "no cap" use `Duration::MAX`.
    pub operation_budget: Duration,
    /// Classes that use **flat** backoff instead of exponential.
    pub flat_classes: Vec<RetryClass>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: DEFAULT_MAX_RETRIES,
            base_delay: DEFAULT_BASE_DELAY,
            cap: DEFAULT_CAP,
            retry_after_cap: DEFAULT_RETRY_AFTER_CAP,
            operation_budget: DEFAULT_OPERATION_BUDGET,
            flat_classes: vec![RetryClass::Timeout, RetryClass::Network],
        }
    }
}

impl RetryConfig {
    /// Returns the detected dangerous combinations, in human-readable text.
    ///
    /// **We warn, we do not correct**: bounding the consumer's configuration
    /// "for their own good" would change behavior silently — exactly the problem
    /// we want to avoid.
    pub(crate) fn dangerous_settings(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.base_delay.is_zero() {
            out.push(format!(
                "base_delay = 0: {} consecutive retries with no pause and NO JITTER",
                self.max_retries
            ));
        }
        if self.retry_after_cap.is_zero() {
            // Same principle as the other two zeros: it SILENTLY disables honoring
            // `Retry-After` entirely — not even an unintelligible header aborts.
            // Legitimate (it is the explicit opt-out) but almost always a mistake.
            out.push(
                "retry_after_cap = 0: the `Retry-After` header is ignored entirely".to_string(),
            );
        }
        if self.cap.is_zero() {
            out.push("cap = 0: every wait is zero, no pause and NO JITTER".to_string());
        }
        if self.retry_after_cap > self.operation_budget {
            // The budget is checked reactively (before each attempt), so a honored
            // `Retry-After` sleep of up to `retry_after_cap` is NOT interrupted. If
            // that cap exceeds the whole budget, a single honored wait can overrun
            // the budget by the difference. Legitimate but almost always a mistake.
            out.push(format!(
                "retry_after_cap ({:?}) > operation_budget ({:?}): a single honored Retry-After can overrun the budget",
                self.retry_after_cap, self.operation_budget
            ));
        }
        out
    }

    // NOTE: there is no `dangerous_settings_with_timeout`. See the retry loop:
    // the "budget < timeout" condition is detected by its **runtime symptom**,
    // not by comparing config (the trait does not expose the wrapped timeout).
}

/// Opt-in retry wrapper for any `LlmProvider`.
///
/// # Concurrency
///
/// Shared across tasks via `Arc` and `complete` takes `&self`: it has **no
/// interior mutable state** — no RNG, no counters. All per-attempt state lives
/// on the stack. *Putting the RNG in a field would force a `Mutex` and serialize
/// concurrent callers exactly where the jitter exists to make them independent.*
///
/// Implements `LlmProvider` itself, making it transparent to consumers.
pub struct RetryProvider {
    inner: Arc<dyn LlmProvider>,
    config: RetryConfig,
}

impl RetryProvider {
    /// Creates a `RetryProvider` with the default configuration.
    ///
    /// # Parameters
    /// - `inner`: The provider to wrap with retry logic.
    pub fn new(inner: Arc<dyn LlmProvider>) -> Self {
        Self::with_config(inner, RetryConfig::default())
    }

    /// Creates a `RetryProvider` with explicit configuration.
    ///
    /// Emits a `tracing` warning if the combination is dangerous (an internal
    /// `dangerous_settings` check). The warning is emitted **once per
    /// constructed provider**: since the config is immutable, no `AtomicBool` or
    /// rate-limit is needed.
    ///
    /// # Parameters
    /// - `inner`: The provider to wrap with retry logic.
    /// - `config`: The retry configuration.
    pub fn with_config(inner: Arc<dyn LlmProvider>, config: RetryConfig) -> Self {
        for warning in config.dangerous_settings() {
            tracing::warn!(target: "magi_core::retry", "{warning}");
        }
        Self { inner, config }
    }

    /// Read-only access to the effective configuration.
    pub fn config(&self) -> &RetryConfig {
        &self.config
    }
}

/// Determines whether a `ProviderError` is transient and should be retried.
///
/// Retryable errors:
/// - `Timeout`: Provider did not respond in time.
/// - `Network`: DNS, connection refused, etc.
/// - `Http` with a transient status (408, 429, 500, 502, 503, 504). The three
///   new 5xx cover local server cold-start; 408 is a server-side request timeout.
///
/// Non-retryable errors:
/// - `Auth`: Invalid credentials won't become valid on retry.
/// - `Process`: CLI subprocess failure.
/// - `NestedSession`: Structural environment issue.
/// - `Http` with any other status code (e.g., 400, 403, 404).
fn is_retryable(error: &ProviderError) -> bool {
    match error {
        ProviderError::Timeout { .. } | ProviderError::Network { .. } => true,
        ProviderError::Http { status, .. } => TRANSIENT_STATUSES.contains(status),
        // Exhaustive on purpose — the catch-all this replaced would have silently classified any
        // new variant as "never retry", which for a transient shape is the wrong answer and would
        // fail no test. A new variant must now break the build until someone decides.
        ProviderError::Auth { .. }
        | ProviderError::Process { .. }
        | ProviderError::NestedSession
        | ProviderError::RetryAbandoned { .. }
        // A server that sent an oversized body will send it again: retrying spends budget the
        // rotation needs to try a DIFFERENT lineage.
        | ProviderError::ResponseTooLarge { .. } => false,
        // The third party named a SHAPE; the decision is made here. `Other` is deliberately not
        // retryable: the escape hatch must not become a way to buy retries by declining to
        // classify.
        //
        // An explicit match with NO catch-all, for the same reason the outer one has none. A
        // `matches!` reads as exhaustive and is not: it is an `_ => false` wearing better
        // clothes, so a new shape would arrive classified as non-retryable in silence — the
        // precise defect this release removed one level up. Inside the crate that defines it,
        // `#[non_exhaustive]` does not block exhaustiveness, so this costs nothing and makes the
        // compiler demand a decision.
        ProviderError::External { kind, .. } => match kind {
            ExternalErrorKind::Network
            | ExternalErrorKind::Timeout
            | ExternalErrorKind::RateLimit
            | ExternalErrorKind::ServerError => true,
            ExternalErrorKind::Auth | ExternalErrorKind::Other => false,
        },
        // Retrying an empty completion reproduces it by construction — same request, same
        // budget, same nothing — and that is measured, not assumed. `NoGeneration` is OUR bad
        // request: four identical attempts would only delay the diagnosis while burning the
        // chain the rotation needs.
        ProviderError::EmptyCompletion { .. } | ProviderError::NoGeneration { .. } => false,
        // The two contract causes split, and they split on whether asking again can return
        // something different. It can for a body that arrived unreadable — a cut connection, a
        // proxy that clipped the response — and it cannot for a body that parsed fine and
        // deliberately carried no message.
        ProviderError::ResponseContract { reason, .. } => match reason {
            ResponseContractCause::Unreadable => true,
            // Neither of these changes on a second try: one carried no message on purpose, and
            // the other follows the same redirect chain to the same refusal.
            ResponseContractCause::NoMessage | ResponseContractCause::RedirectRefused => false,
        },
    }
}

/// Upper bound for a composed transport error message, in bytes.
// Serves the HTTP providers only. Gated with them rather than left ungated: with no HTTP
// provider compiled in there is nothing to compose an error for, and an item that is dead
// under a supported feature set is a warning the default gate run reports.
#[cfg(any(feature = "claude-api", feature = "openai-compat"))]
pub(crate) const MAX_TRANSPORT_MESSAGE_BYTES: usize = 2000;

/// Builds a provider error message from parts this crate controls.
///
/// # Why compose instead of interpolating the client's error
///
/// An HTTP client's error text embeds the URL it was given, which may carry credentials. Composing
/// from an already-redacted rendering plus the *causes* keeps every bit of diagnostic value and
/// drops exactly the part that can leak.
///
/// # Why the order matters
///
/// Operation and endpoint come first, so when the cap bites it eats the tail of the cause chain —
/// the least critical part — and never the endpoint, which is the first thing a reader needs.
#[cfg(any(feature = "claude-api", feature = "openai-compat"))]
pub(crate) fn compose_transport_message(op: &str, redacted_url: &str, cause_chain: &str) -> String {
    let head = format!("{op} for {redacted_url}");
    // ONE cap, applied to whatever was composed. Returning the head early when there is no cause
    // chain skipped the bound entirely, so a pathological URL produced an unbounded message — the
    // cap held only on the path that happened to have a suffix.
    let full = if cause_chain.is_empty() {
        head
    } else {
        format!("{head}: {cause_chain}")
    };
    if full.len() <= MAX_TRANSPORT_MESSAGE_BYTES {
        return full;
    }
    // Reserve the marker inside the budget: a cap its own suffix can exceed lies about its name.
    let budget = MAX_TRANSPORT_MESSAGE_BYTES.saturating_sub(crate::error::TRUNCATION_MARKER.len());
    let cut = full.floor_char_boundary(budget);
    format!("{}{}", &full[..cut], crate::error::TRUNCATION_MARKER)
}

/// Joins an error's `source()` chain, **starting at the first source**.
///
/// The top-level error is skipped on purpose: an HTTP client's `Display` interpolates the URL. Its
/// causes (transport, I/O) describe the failure without it.
///
/// Lives here rather than in the provider-URL module because it takes `&dyn Error` — it touches no
/// HTTP type — and it must be reachable when only the Claude feature is enabled, which does not
/// compile that module. Duplicating it would put two definitions on the one path that produces
/// error text.
#[cfg(any(feature = "claude-api", feature = "openai-compat"))]
pub(crate) fn cause_chain(e: &dyn std::error::Error) -> String {
    let mut parts = Vec::new();
    let mut cur = e.source();
    while let Some(c) = cur {
        parts.push(c.to_string());
        cur = c.source();
    }
    parts.join(": ")
}

/// Builds the error for a client that could not be constructed.
///
/// Separate from [`to_provider_error`] because no request exists yet — there is no URL to redact
/// and nothing to compose against. It lives here, with the other constructors, so that **no
/// provider file builds a transport variant at all**: that is what lets the CI check be a flat
/// prohibition instead of a rule with exceptions.
#[cfg(any(feature = "claude-api", feature = "openai-compat"))]
pub(crate) fn client_build_error(e: &reqwest::Error) -> ProviderError {
    ProviderError::Network {
        message: format!("failed to build HTTP client: {}", cause_chain(e)),
    }
}

/// Turns a transport failure into this crate's error type.
///
/// **This is the only place a transport [`ProviderError`] is built from a client error.** That is
/// what lets the rule "provider files never construct transport errors" be unconditional — including
/// for a provider whose URL is a constant with no secret. A conditional rule is one someone applies
/// wrong.
#[cfg(any(feature = "claude-api", feature = "openai-compat"))]
pub(crate) fn to_provider_error(op: &str, redacted_url: &str, e: &reqwest::Error) -> ProviderError {
    let message = compose_transport_message(op, redacted_url, &cause_chain(e));
    if e.is_timeout() {
        ProviderError::Timeout { message }
    } else if e.is_redirect() {
        // A redirect-policy failure is NOT a connection failure, and the difference has teeth.
        // `Network` is the connection class: two of them trip the endpoint-down latch and abort
        // the whole run, which would blame a reachable endpoint for a misconfigured redirect
        // chain. It is also not transient — the same request follows the same chain and fails the
        // same way — so retrying only spends budget.
        //
        // It used to borrow the synthetic zero status for three properties — never a real HTTP
        // status, non-retryable, mage-local — of which only the first came from the zero itself.
        // With the disguise gone it says what it is, and `Http.status` is left holding only real
        // statuses. The message still names which of the two causes applied.
        // Through the constructor, so the bound is the type's and not this call site's. The
        // message names the operation and the REDACTED endpoint: this is the one case routed
        // here whose whole diagnostic value is which endpoint refused, and it is why the
        // variant carries a detail at all.
        ProviderError::response_contract(ResponseContractCause::RedirectRefused, message)
    } else {
        ProviderError::Network { message }
    }
}

/// HTTP statuses considered transient (worth retrying).
const TRANSIENT_STATUSES: &[u16] = &[408, 429, 500, 502, 503, 504];

/// Maps a [`ProviderError`] to its [`RetryClass`].
///
/// # Compiler-enforced synchronization
///
/// The `match` is exhaustive on purpose. `ProviderError` is `#[non_exhaustive]`
/// for external consumers, but **not within the crate**: adding a variant
/// **breaks compilation here** until it is mapped. Without this, a new class
/// would silently fall into the wrong backoff path.
pub(crate) fn classify(err: &ProviderError) -> RetryClass {
    match err {
        ProviderError::Timeout { .. } => RetryClass::Timeout,
        ProviderError::Network { .. } => RetryClass::Network,
        ProviderError::Http { .. } => RetryClass::Http,
        ProviderError::Auth { .. } => RetryClass::Auth,
        ProviderError::Process { .. } => RetryClass::Process,
        ProviderError::NestedSession => RetryClass::NestedSession,
        ProviderError::RetryAbandoned { .. } => RetryClass::RetryAbandoned,
        ProviderError::ResponseTooLarge { .. } => RetryClass::ResponseTooLarge,
        // One class per variant, as this enum's contract says — NOT a mapping of `kind` onto the
        // existing classes. Folding `ExternalErrorKind::Network` into `RetryClass::Network` would
        // let a third party inherit whatever backoff policy is configured for this crate's own
        // network failures, which is exactly the ownership the design keeps here.
        ProviderError::External { .. } => RetryClass::External,
        ProviderError::ResponseContract { .. } => RetryClass::ResponseContract,
        ProviderError::EmptyCompletion { .. } => RetryClass::EmptyCompletion,
        ProviderError::NoGeneration { .. } => RetryClass::NoGeneration,
    }
}

#[async_trait::async_trait]
impl LlmProvider for RetryProvider {
    async fn complete(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        config: &CompletionConfig,
    ) -> Result<Completion, ProviderError> {
        let started = std::time::Instant::now();
        let mut last_error: Option<ProviderError> = None;

        for attempt in 0..=self.config.max_retries {
            // Reactive budget check, before each new attempt.
            if attempt > 0 {
                let elapsed = started.elapsed();
                if elapsed >= self.config.operation_budget {
                    // If the budget is exhausted already on the first check, a
                    // single attempt consumed it whole: almost always
                    // `operation_budget < provider timeout`. Detected by SYMPTOM,
                    // not by comparing config: the `LlmProvider` trait does not
                    // expose the wrapped timeout, so a construction-time comparison
                    // would be unreachable code.
                    if attempt == 1 {
                        tracing::warn!(
                            target: "magi_core::retry",
                            ?elapsed,
                            budget = ?self.config.operation_budget,
                            "operation_budget exhausted by a SINGLE attempt: no retry will ever happen. Is the budget smaller than one attempt plus its backoff (e.g. the provider timeout)?"
                        );
                    }
                    tracing::warn!(
                        target: "magi_core::retry",
                        ?elapsed,
                        budget = ?self.config.operation_budget,
                        attempts = attempt,
                        "operation budget exhausted; abandoning retries"
                    );
                    return Err(ProviderError::RetryAbandoned {
                        reason: AbandonReason::OperationBudgetExhausted {
                            elapsed,
                            budget: self.config.operation_budget,
                        },
                        attempts: attempt,
                    });
                }
            }

            let err = match self
                .inner
                .complete(system_prompt, user_prompt, config)
                .await
            {
                Ok(response) => return Ok(response),
                Err(e) => e,
            };

            if !is_retryable(&err) || attempt == self.config.max_retries {
                return Err(err);
            }

            // Interpret the Retry-After HERE: the only point that knows the
            // configured cap. A present but non-honorable header ABANDONS.
            let retry_after = match &err {
                ProviderError::Http {
                    retry_after_raw,
                    received_at,
                    ..
                } => match crate::backoff::parse_retry_after(
                    retry_after_raw.as_slice(),
                    self.config.retry_after_cap,
                ) {
                    crate::backoff::RetryAfter::Absent => None,
                    crate::backoff::RetryAfter::Honor(asked) => {
                        // C3.1: discount the time elapsed since the headers were
                        // received, with SATURATING subtraction (never negative).
                        Some(match received_at {
                            Some(t) => asked.saturating_sub(t.elapsed()),
                            None => asked,
                        })
                    }
                    crate::backoff::RetryAfter::TooLong { requested } => {
                        tracing::warn!(
                            target: "magi_core::retry",
                            ?requested,
                            cap = ?self.config.retry_after_cap,
                            "server asked to wait longer than retry_after_cap; abandoning"
                        );
                        return Err(ProviderError::RetryAbandoned {
                            reason: AbandonReason::RetryAfterTooLong {
                                requested,
                                cap: self.config.retry_after_cap,
                            },
                            attempts: attempt + 1,
                        });
                    }
                    crate::backoff::RetryAfter::Unintelligible { raw } => {
                        tracing::warn!(
                            target: "magi_core::retry",
                            raw = %raw,
                            "Retry-After present but uninterpretable; abandoning"
                        );
                        return Err(ProviderError::RetryAbandoned {
                            reason: AbandonReason::RetryAfterUnintelligible { raw },
                            attempts: attempt + 1,
                        });
                    }
                },
                _ => None,
            };

            let mut rand = || fastrand::f64();
            let wait = crate::backoff::next_backoff(
                attempt,
                classify(&err),
                self.config.base_delay,
                self.config.cap,
                &self.config.flat_classes,
                retry_after,
                &mut rand,
            );

            tracing::debug!(
                target: "magi_core::retry",
                attempt,
                ?wait,
                honored_retry_after = retry_after.is_some(),
                "transient error; backing off before retry"
            );
            last_error = Some(err);
            tokio::time::sleep(wait).await;
        }

        // Unreachable by construction: the loop is only left AFTER at least one
        // failed attempt, and every failure assigns `last_error`. The
        // `debug_assert!` makes it visible in dev if someone restructures the
        // loop; in release it degrades to an honest error rather than panicking,
        // because a library must not tear down the consumer's process for its own
        // bug (§Error handling: `panic!` only for the unrecoverable).
        debug_assert!(
            last_error.is_some(),
            "the loop exited without recording any error: check the exit condition"
        );
        Err(last_error.unwrap_or(ProviderError::Network {
            message: "retry loop ended without an attempt".to_string(),
        }))
    }

    fn name(&self) -> &str {
        self.inner.name()
    }

    fn model(&self) -> &str {
        self.inner.model()
    }
}

#[cfg(test)]
mod message_composition_tests {
    use super::*;

    #[test]
    fn an_oversized_response_is_not_retryable_and_has_its_own_class() {
        let e = ProviderError::ResponseTooLarge { limit: 1024 };
        assert!(
            !is_retryable(&e),
            "a server that sent 1 MiB will send it again"
        );
        assert_eq!(classify(&e), RetryClass::ResponseTooLarge);
    }

    #[test]
    #[cfg(any(feature = "claude-api", feature = "openai-compat"))]
    fn compose_puts_operation_and_url_first_and_truncates_the_cause_tail() {
        let long_cause = "x".repeat(MAX_TRANSPORT_MESSAGE_BYTES * 2);
        let msg = compose_transport_message("request failed", "http://h/v1", &long_cause);
        assert!(
            msg.len() <= MAX_TRANSPORT_MESSAGE_BYTES,
            "capped: {}",
            msg.len()
        );
        assert!(msg.starts_with("request failed"), "operation first: {msg}");
        assert!(
            msg.contains("http://h/v1"),
            "endpoint survives truncation: {msg}"
        );
        assert!(msg.contains("truncated"), "the cut is announced: {msg}");
    }

    /// The path that had no test, and therefore no cap: with an empty cause chain the composer
    /// returned early and skipped the bound entirely.
    #[test]
    #[cfg(any(feature = "claude-api", feature = "openai-compat"))]
    fn compose_caps_the_head_even_with_no_cause_chain() {
        let long_url = format!("http://{}/v1", "h".repeat(MAX_TRANSPORT_MESSAGE_BYTES));
        let msg = compose_transport_message("request failed", &long_url, "");
        assert!(
            msg.len() <= MAX_TRANSPORT_MESSAGE_BYTES,
            "an empty cause chain must not exempt the message from its cap: {} bytes",
            msg.len()
        );
        assert!(
            msg.contains("request failed"),
            "the operation survives the cut"
        );
    }

    #[test]
    #[cfg(any(feature = "claude-api", feature = "openai-compat"))]
    fn compose_does_not_truncate_when_under_the_cap() {
        let msg = compose_transport_message("request failed", "http://h/v1", "connection refused");
        assert!(msg.contains("connection refused"));
        assert!(
            !msg.contains("truncated"),
            "no marker when nothing was cut: {msg}"
        );
    }

    #[test]
    #[cfg(any(feature = "claude-api", feature = "openai-compat"))]
    fn compose_never_panics_on_multibyte_boundaries() {
        let cause = "ñ".repeat(MAX_TRANSPORT_MESSAGE_BYTES * 2);
        let msg = compose_transport_message("op", "http://h", &cause);
        assert!(msg.len() <= MAX_TRANSPORT_MESSAGE_BYTES);
    }

    #[test]
    #[cfg(any(feature = "claude-api", feature = "openai-compat"))]
    fn cause_chain_skips_the_top_level_error() {
        use std::fmt;

        #[derive(Debug)]
        struct Top;
        #[derive(Debug)]
        struct Inner;
        impl fmt::Display for Top {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "for url (http://u:p@h)")
            }
        }
        impl fmt::Display for Inner {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "connection refused")
            }
        }
        impl std::error::Error for Inner {}
        impl std::error::Error for Top {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&Inner)
            }
        }

        let chain = cause_chain(&Top);
        assert!(
            chain.contains("connection refused"),
            "sources kept: {chain}"
        );
        assert!(
            !chain.contains("http://u:p@h"),
            "top-level Display excluded: {chain}"
        );
    }
}

#[cfg(test)]
mod tests {

    #[tokio::test]
    async fn the_retry_wrapper_passes_the_inner_telemetry_through_untouched() {
        // The contract is NON-ENRICHMENT. Telemetry belongs to the provider that spoke to the
        // backend; a wrapper that added anything would be asserting it measured what only the
        // inner one could see, and a wrapper that dropped anything would hide it. Both are
        // failures of the same kind, so the value is pinned on BOTH sides rather than checking
        // that "something" came back.
        //
        // This never had a Red: the property was already true when the break changed the
        // signature, so it is a PIN against regression, not a cycle. Verified by mutation
        // instead — rebuilding the returned value as `Completion::new(response.text)` turns it
        // red, which is what says the assertions can fail at all.
        struct MeasuringProvider;

        #[async_trait::async_trait]
        impl LlmProvider for MeasuringProvider {
            async fn complete(
                &self,
                _s: &str,
                _u: &str,
                _c: &CompletionConfig,
            ) -> Result<Completion, ProviderError> {
                Ok(Completion::new("inner".to_string()).with_telemetry(
                    CompletionTelemetry::unmeasured()
                        .with_finish(FinishReason::Length)
                        .with_completion_tokens(4096)
                        .with_prompt_tokens(11)
                        .with_reasoning(ReasoningState::Measured {
                            chars: 15_409,
                            text: None,
                        }),
                ))
            }
            fn name(&self) -> &str {
                "measuring"
            }
            fn model(&self) -> &str {
                "m"
            }
        }

        let retry = RetryProvider::new(Arc::new(MeasuringProvider));
        let out = retry
            .complete("sys", "usr", &CompletionConfig::default())
            .await
            .expect("the inner provider always succeeds");

        assert_eq!(out.text, "inner");
        assert_eq!(out.telemetry.finish, Some(FinishReason::Length));
        assert_eq!(out.telemetry.completion_tokens, Some(4096));
        assert_eq!(out.telemetry.prompt_tokens, Some(11));
        assert_eq!(
            out.telemetry.reasoning,
            ReasoningState::Measured {
                chars: 15_409,
                text: None
            }
        );
    }

    // ---- Task 3b: retryability of the three contract variants ----

    #[test]
    fn three_of_the_four_contract_cases_are_not_retryable() {
        // Without this, `RetryProvider` would retry OUR OWN bad request four times and the abort
        // would land after the whole chain had been burned.
        assert!(!is_retryable(&ProviderError::EmptyCompletion {
            telemetry: crate::provider::CompletionTelemetry::unmeasured()
                .with_finish(FinishReason::Length),
            cap: 4096,
        }));
        assert!(!is_retryable(&ProviderError::NoGeneration {
            done_reason: Some(FinishReason::Load),
        }));
        assert!(!is_retryable(&ProviderError::ResponseContract {
            reason: ResponseContractCause::NoMessage,
            detail: String::new(),
        }));

        // The one that IS retryable, and the reason it differs: an unreadable body can be a
        // TRUNCATION IN TRANSIT — a cut connection, a proxy that clipped the response — so asking
        // again can genuinely return something else. The other three cannot: same request, same
        // budget, same answer.
        assert!(is_retryable(&ProviderError::ResponseContract {
            reason: ResponseContractCause::Unreadable,
            detail: String::new(),
        }));
    }

    #[test]
    fn every_contract_variant_has_its_own_retry_class() {
        // `RetryClass` mirrors `ProviderError`'s discriminants one-to-one — that is the enum's
        // stated contract — so that `flat_classes` and the limited-retry list can name any of
        // them without a variant inheriting another's backoff policy.
        let classes = [
            classify(&ProviderError::ResponseContract {
                reason: ResponseContractCause::Unreadable,
                detail: String::new(),
            }),
            classify(&ProviderError::EmptyCompletion {
                telemetry: crate::provider::CompletionTelemetry::unmeasured(),
                cap: 16_384,
            }),
            classify(&ProviderError::NoGeneration { done_reason: None }),
        ];
        assert_eq!(classes[0], RetryClass::ResponseContract);
        assert_eq!(classes[1], RetryClass::EmptyCompletion);
        assert_eq!(classes[2], RetryClass::NoGeneration);
        // Distinct from each other AND from the transport classes they must never inherit.
        assert_ne!(classes[0], RetryClass::Http);
        assert_ne!(classes[1], RetryClass::Http);
    }
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    // -- Test providers for the retry loop (Task 7) --

    /// Always fails with the given error. Counts invocations.
    struct FailingProvider {
        error: ProviderError,
        calls: AtomicUsize,
    }

    impl FailingProvider {
        fn new(error: ProviderError) -> Self {
            Self {
                error,
                calls: AtomicUsize::new(0),
            }
        }
        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for FailingProvider {
        async fn complete(
            &self,
            _s: &str,
            _u: &str,
            _c: &CompletionConfig,
        ) -> Result<Completion, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Err(self.error.clone())
        }
        fn name(&self) -> &str {
            "failing"
        }
        fn model(&self) -> &str {
            "failing"
        }
    }

    /// Like `FailingProvider`, but **delays** before failing: used to exhaust the
    /// `operation_budget` with a controlled number of attempts. Also records the
    /// peak concurrency observed (deterministic, no timing asserts).
    struct SlowFailingProvider {
        delay: Duration,
        calls: AtomicUsize,
        in_flight: AtomicUsize,
        peak_in_flight: AtomicUsize,
    }

    impl SlowFailingProvider {
        fn new(delay: Duration) -> Self {
            Self {
                delay,
                calls: AtomicUsize::new(0),
                in_flight: AtomicUsize::new(0),
                peak_in_flight: AtomicUsize::new(0),
            }
        }
        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
        fn peak_in_flight(&self) -> usize {
            self.peak_in_flight.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for SlowFailingProvider {
        async fn complete(
            &self,
            _s: &str,
            _u: &str,
            _c: &CompletionConfig,
        ) -> Result<Completion, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak_in_flight.fetch_max(now, Ordering::SeqCst);
            tokio::time::sleep(self.delay).await;
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            Err(ProviderError::Network {
                message: "slow fail".to_string(),
            })
        }
        fn name(&self) -> &str {
            "slow-failing"
        }
        fn model(&self) -> &str {
            "slow-failing"
        }
    }

    /// Returns a 429 with the given `Retry-After` headers and, after `fail_times`
    /// failures, responds with success. The only mock that exercises the
    /// `Retry-After` honouring path end to end.
    struct RetryAfterProvider {
        headers: Vec<String>,
        fail_times: usize,
        calls: AtomicUsize,
    }

    impl RetryAfterProvider {
        fn new(headers: Vec<String>, fail_times: usize) -> Self {
            Self {
                headers,
                fail_times,
                calls: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for RetryAfterProvider {
        async fn complete(
            &self,
            _s: &str,
            _u: &str,
            _c: &CompletionConfig,
        ) -> Result<Completion, ProviderError> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            if n >= self.fail_times {
                return Ok(Completion::new("ok".to_string()));
            }
            Err(ProviderError::Http {
                status: 429,
                body: String::new(),
                retry_after_raw: self.headers.clone(),
                received_at: Some(Instant::now()),
            })
        }
        fn name(&self) -> &str {
            "retry-after"
        }
        fn model(&self) -> &str {
            "retry-after"
        }
    }

    #[test]
    fn test_is_retryable_covers_the_eight_transient_cases() {
        for status in [408u16, 429, 500, 502, 503, 504] {
            assert!(
                is_retryable(&ProviderError::Http {
                    status,
                    body: String::new(),
                    retry_after_raw: vec![],
                    received_at: None
                }),
                "status {status} must be transient"
            );
        }
        assert!(is_retryable(&ProviderError::Timeout {
            message: String::new()
        }));
        assert!(is_retryable(&ProviderError::Network {
            message: String::new()
        }));
    }

    #[test]
    fn test_is_retryable_rejects_non_transient() {
        for status in [400u16, 403, 404] {
            assert!(!is_retryable(&ProviderError::Http {
                status,
                body: String::new(),
                retry_after_raw: vec![],
                received_at: None
            }));
        }
        assert!(!is_retryable(&ProviderError::Auth {
            message: String::new()
        }));
    }

    /// The core decides retryability; the third party only names a shape. Every variant is
    /// listed, so adding one to `ExternalErrorKind` without deciding its consequence leaves a
    /// visible hole here.
    #[test]
    fn the_core_decides_retryability_of_external_failures_not_the_third_party() {
        for kind in [
            ExternalErrorKind::Network,
            ExternalErrorKind::Timeout,
            ExternalErrorKind::RateLimit,
            ExternalErrorKind::ServerError,
        ] {
            assert!(
                is_retryable(&ProviderError::external("x", kind)),
                "{kind:?} is a transient shape"
            );
        }
        for kind in [ExternalErrorKind::Auth, ExternalErrorKind::Other] {
            assert!(
                !is_retryable(&ProviderError::external("x", kind)),
                "{kind:?} must not buy a retry"
            );
        }
    }

    #[test]
    fn every_external_failure_shares_one_retry_class() {
        // The class mirrors the VARIANT, not the declared kind: backoff policy is configured in
        // this crate's vocabulary, and an external crate must not be able to select among the
        // policies meant for our own failures.
        for kind in [ExternalErrorKind::Network, ExternalErrorKind::Auth] {
            assert_eq!(
                classify(&ProviderError::external("x", kind)),
                RetryClass::External
            );
        }
    }

    #[tokio::test]
    async fn test_max_retries_zero_does_not_retry() {
        let inner = Arc::new(FailingProvider::new(ProviderError::Network {
            message: "fail".into(),
        }));
        let p = RetryProvider::with_config(
            inner.clone(),
            RetryConfig {
                max_retries: 0,
                ..Default::default()
            },
        );
        let _ = p.complete("s", "u", &CompletionConfig::default()).await;
        assert_eq!(inner.calls(), 1, "only the initial request");
    }

    #[tokio::test]
    async fn test_base_zero_with_three_retries_emits_exactly_four_requests() {
        // S2 / B7 end-to-end: `base_delay = 0` does not sleep, but the burst is
        // BOUNDED to `max_retries + 1`.
        let inner = Arc::new(FailingProvider::new(ProviderError::Network {
            message: "fail".into(),
        }));
        let p = RetryProvider::with_config(
            inner.clone(),
            RetryConfig {
                base_delay: Duration::ZERO,
                max_retries: 3,
                ..Default::default()
            },
        );
        let _ = p.complete("s", "u", &CompletionConfig::default()).await;
        assert_eq!(inner.calls(), 4, "1 initial + 3 retries, no infinite loop");
    }

    #[tokio::test]
    async fn test_budget_exhaustion_abandons_with_typed_reason() {
        let inner = Arc::new(SlowFailingProvider::new(Duration::from_millis(50)));
        let p = RetryProvider::with_config(
            inner,
            RetryConfig {
                operation_budget: Duration::from_millis(10),
                base_delay: Duration::ZERO,
                ..Default::default()
            },
        );
        let err = p
            .complete("s", "u", &CompletionConfig::default())
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                ProviderError::RetryAbandoned {
                    reason: AbandonReason::OperationBudgetExhausted { .. },
                    ..
                }
            ),
            "expected budget abandonment, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_retry_after_beyond_cap_abandons_with_typed_reason() {
        let inner = Arc::new(RetryAfterProvider::new(vec!["600".to_string()], 1));
        let p = RetryProvider::new(inner);
        let err = p
            .complete("s", "u", &CompletionConfig::default())
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                ProviderError::RetryAbandoned {
                    reason: AbandonReason::RetryAfterTooLong { .. },
                    ..
                }
            ),
            "{err}"
        );
    }

    #[tokio::test]
    async fn test_single_attempt_budget_exhaustion_is_announced() {
        // Budget smaller than one attempt's duration: NEVER a retry.
        let inner = Arc::new(SlowFailingProvider::new(Duration::from_millis(80)));
        let provider = RetryProvider::with_config(
            inner.clone(),
            RetryConfig {
                operation_budget: Duration::from_millis(10),
                ..Default::default()
            },
        );
        let err = provider
            .complete("s", "u", &CompletionConfig::default())
            .await
            .unwrap_err();
        assert_eq!(
            inner.calls(),
            1,
            "one attempt: the budget cuts before the second"
        );
        assert!(
            matches!(
                err,
                ProviderError::RetryAbandoned {
                    reason: AbandonReason::OperationBudgetExhausted { .. },
                    attempts: 1,
                }
            ),
            "{err}"
        );
    }

    #[tokio::test]
    async fn test_operation_budget_zero_yields_single_attempt() {
        // E3.1 exact edge: `operation_budget = ZERO` -> `elapsed >= 0` is met on
        // the first check -> ZERO retries, behaves like `max_retries = 0`.
        let inner = Arc::new(FailingProvider::new(ProviderError::Network {
            message: "x".into(),
        }));
        let provider = RetryProvider::with_config(
            inner.clone(),
            RetryConfig {
                operation_budget: Duration::ZERO,
                ..Default::default()
            },
        );
        let _ = provider
            .complete("s", "u", &CompletionConfig::default())
            .await
            .unwrap_err();
        assert_eq!(inner.calls(), 1, "budget ZERO: one attempt, no retries");
    }

    #[tokio::test]
    async fn test_honored_retry_after_can_overrun_a_small_budget() {
        // retry_after_cap (5s) > operation_budget (50ms): the reactive budget does
        // NOT clamp the honored `Retry-After` sleep, so a single honored wait
        // overruns the budget and the NEXT reactive check abandons. Pins the
        // documented "reactive, not a hard cap" behavior (and the config that the
        // new dangerous_settings warning flags).
        let inner = Arc::new(RetryAfterProvider::new(vec!["1".to_string()], 1));
        let cfg = RetryConfig {
            operation_budget: Duration::from_millis(50),
            retry_after_cap: Duration::from_secs(5),
            ..Default::default()
        };
        let p = RetryProvider::with_config(inner, cfg);
        let start = Instant::now();
        let err = p
            .complete("s", "u", &CompletionConfig::default())
            .await
            .unwrap_err();
        let elapsed = start.elapsed();
        assert!(
            matches!(
                err,
                ProviderError::RetryAbandoned {
                    reason: AbandonReason::OperationBudgetExhausted { .. },
                    ..
                }
            ),
            "{err}"
        );
        assert!(
            elapsed >= Duration::from_secs(1),
            "the ~1s honored wait overran the 50ms budget: {elapsed:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_shared_provider_does_not_serialize_callers() {
        // Three concurrent tasks over ONE shared RetryProvider behind Arc. A peak
        // `>= 2` proves it did NOT serialize callers (an internal mutex would give
        // peak 1). Not a formal proof of no interior mutability (R10 is structural,
        // held by type review); asserted `>= 2` (robust) not `== 3`.
        let inner_probe = Arc::new(SlowFailingProvider::new(Duration::from_millis(50)));
        let provider = Arc::new(RetryProvider::with_config(
            Arc::clone(&inner_probe) as Arc<dyn LlmProvider>,
            RetryConfig {
                max_retries: 0,
                ..Default::default()
            },
        ));

        let mut handles = Vec::new();
        for _ in 0..3 {
            let p = Arc::clone(&provider);
            handles.push(tokio::spawn(async move {
                let _ = p.complete("s", "u", &CompletionConfig::default()).await;
            }));
        }
        for h in handles {
            h.await.expect("task joined");
        }

        assert!(
            inner_probe.peak_in_flight() >= 2,
            "the shared provider serialized callers: concurrency peak = {}",
            inner_probe.peak_in_flight()
        );
    }

    #[test]
    fn test_retry_config_defaults() {
        let c = RetryConfig::default();
        assert_eq!(c.max_retries, 3);
        assert_eq!(c.base_delay, Duration::from_secs(1));
        assert_eq!(c.cap, Duration::from_secs(60));
        assert_eq!(c.retry_after_cap, Duration::from_secs(300));
        assert_eq!(c.operation_budget, Duration::from_secs(600));
        assert_eq!(
            c.flat_classes,
            vec![RetryClass::Timeout, RetryClass::Network]
        );
    }

    #[test]
    fn test_dangerous_config_is_announced_for_zero_base_delay() {
        let cfg = RetryConfig {
            base_delay: Duration::ZERO,
            ..Default::default()
        };
        let warnings = cfg.dangerous_settings();
        assert!(
            warnings.iter().any(|w| w.contains("base_delay")),
            "{warnings:?}"
        );
    }

    #[test]
    fn test_dangerous_config_is_announced_for_zero_cap() {
        let cfg = RetryConfig {
            cap: Duration::ZERO,
            ..Default::default()
        };
        let warnings = cfg.dangerous_settings();
        assert!(warnings.iter().any(|w| w.contains("cap")), "{warnings:?}");
    }

    #[test]
    fn test_dangerous_config_is_announced_for_zero_retry_after_cap() {
        // F2: the THREE zeros that silently disable a protection all warn.
        let cfg = RetryConfig {
            retry_after_cap: Duration::ZERO,
            ..Default::default()
        };
        let warnings = cfg.dangerous_settings();
        assert!(
            warnings.iter().any(|w| w.contains("retry_after_cap")),
            "{warnings:?}"
        );
    }

    #[test]
    fn test_dangerous_config_is_announced_for_retry_after_cap_over_budget() {
        // default retry_after_cap (300s) > operation_budget here (100s): a honored
        // Retry-After can overrun the budget (reactive check does not clamp sleeps).
        let cfg = RetryConfig {
            operation_budget: Duration::from_secs(100),
            ..Default::default()
        };
        let warnings = cfg.dangerous_settings();
        assert!(
            warnings.iter().any(|w| w.contains("overrun")),
            "{warnings:?}"
        );
    }

    #[test]
    fn test_default_config_is_silent() {
        assert!(RetryConfig::default().dangerous_settings().is_empty());
    }

    /// classify maps every ProviderError variant to its RetryClass.
    #[test]
    fn test_classify_maps_every_variant() {
        assert_eq!(
            classify(&ProviderError::Timeout {
                message: "t".into()
            }),
            RetryClass::Timeout
        );
        assert_eq!(
            classify(&ProviderError::Network {
                message: "n".into()
            }),
            RetryClass::Network
        );
        assert_eq!(
            classify(&ProviderError::Http {
                status: 503,
                body: String::new(),
                retry_after_raw: vec![],
                received_at: None,
            }),
            RetryClass::Http
        );
        assert_eq!(
            classify(&ProviderError::Auth {
                message: "a".into()
            }),
            RetryClass::Auth
        );
        assert_eq!(
            classify(&ProviderError::Process {
                exit_code: Some(1),
                stderr: "p".into(),
            }),
            RetryClass::Process
        );
        assert_eq!(
            classify(&ProviderError::NestedSession),
            RetryClass::NestedSession
        );
        assert_eq!(
            classify(&ProviderError::RetryAbandoned {
                reason: crate::error::AbandonReason::OperationBudgetExhausted {
                    elapsed: Duration::ZERO,
                    budget: Duration::ZERO,
                },
                attempts: 0,
            }),
            RetryClass::RetryAbandoned
        );
        // The two variants this release adds. The name of this test says "every variant", and it
        // stopped being true the moment they landed — a stale exhaustiveness test is a claim that
        // outlives its evidence.
        assert_eq!(
            classify(&ProviderError::ResponseTooLarge { limit: 1 << 20 }),
            RetryClass::ResponseTooLarge
        );
        assert_eq!(
            classify(&ProviderError::external("x", ExternalErrorKind::Network)),
            RetryClass::External
        );
        // `classify`'s own match is exhaustive in-crate, so a NEW variant breaks the build there
        // before it can reach this test. This pins the mapping; the compiler pins the coverage.
    }

    // -- default_model_for_mode tests (T02) --

    /// Default model for code-review mode is opus per Python v2.2.8 MODE_DEFAULT_MODELS.
    #[test]
    fn test_default_model_for_mode_code_review_is_opus() {
        assert_eq!(default_model_for_mode(Mode::CodeReview), "opus");
    }

    /// Default model for design mode is opus per Python v2.2.8 MODE_DEFAULT_MODELS.
    #[test]
    fn test_default_model_for_mode_design_is_opus() {
        assert_eq!(default_model_for_mode(Mode::Design), "opus");
    }

    /// Default model for analysis mode is opus per Python v2.2.8 MODE_DEFAULT_MODELS.
    /// Note: Python v2.2.3 had sonnet here briefly, reverted to opus in v2.2.5
    /// due to output-length structural failures. See `models.py:39-50` in upstream.
    #[test]
    fn test_default_model_for_mode_analysis_is_opus() {
        assert_eq!(default_model_for_mode(Mode::Analysis), "opus");
    }

    /// Pairing with resolve_claude_alias yields the full model id.
    #[test]
    fn test_default_model_for_mode_composes_with_resolve_claude_alias() {
        let alias = default_model_for_mode(Mode::Analysis);
        let id = resolve_claude_alias(alias).expect("opus alias resolves");
        assert_eq!(id, "claude-opus-4-7");
    }

    /// Manual mock provider for testing.
    struct MockProvider {
        provider_name: String,
        provider_model: String,
        responses: std::sync::Mutex<Vec<Result<Completion, ProviderError>>>,
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
            responses: Vec<Result<Completion, ProviderError>>,
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
        ) -> Result<Completion, ProviderError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            let mut responses = self.responses.lock().unwrap();
            if let Some(result) = responses.pop() {
                result
            } else {
                Ok(Completion::new("default response".to_string()))
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

    /// CompletionConfig::default has max_tokens=16_384, temperature=0.0.
    #[test]
    fn test_completion_config_default_values() {
        let config = CompletionConfig::default();
        assert_eq!(config.max_tokens, 16_384);
        assert!((config.temperature - 0.0).abs() < f64::EPSILON);
    }

    /// CompletionConfig is #[non_exhaustive] — verify Default works and fields accessible.
    #[test]
    fn test_completion_config_is_non_exhaustive() {
        let config = CompletionConfig::default();
        assert_eq!(config.max_tokens, 16_384);
        assert!((config.temperature).abs() < f64::EPSILON);
    }

    // ---- Task 5: ReasoningControl and reasoning_trace ----

    #[test]
    fn the_default_reasoning_control_leaves_the_wire_untouched() {
        // Default must mean "say nothing", not "say think:true": a crate that starts
        // asserting a control it was never asked for would change behaviour for every
        // consumer that never mentioned reasoning.
        let cfg = CompletionConfig::default();
        assert_eq!(cfg.reasoning, ReasoningControl::Default);
    }

    #[test]
    fn the_control_is_set_through_a_builder_because_the_struct_is_non_exhaustive() {
        let cfg = CompletionConfig::default().with_reasoning(ReasoningControl::Disabled);
        assert_eq!(cfg.reasoning, ReasoningControl::Disabled);
    }

    #[test]
    fn reasoning_trace_defaults_to_false_and_is_set_through_its_own_builder() {
        // Default `false`: with it, behaviour does not change and no report grows by
        // surprise. It is ADDITIVE — the flag adds the text, never replaces the length.
        assert!(!CompletionConfig::default().reasoning_trace);
        assert!(
            CompletionConfig::default()
                .with_reasoning_trace(true)
                .reasoning_trace
        );
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
                Ok(Completion::new("success".to_string())),
            ],
        ));
        let retry = RetryProvider::with_config(
            mock.clone(),
            RetryConfig {
                max_retries: 3,
                base_delay: Duration::from_millis(1),
                ..Default::default()
            },
        );
        let config = CompletionConfig::default();
        let result = retry.complete("sys", "usr", &config).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().text, "success");
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
                    retry_after_raw: vec![],
                    received_at: None,
                }),
                Ok(Completion::new("ok".to_string())),
            ],
        ));
        let retry = RetryProvider::with_config(
            mock.clone(),
            RetryConfig {
                max_retries: 3,
                base_delay: Duration::from_millis(1),
                ..Default::default()
            },
        );
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
                    retry_after_raw: vec![],
                    received_at: None,
                }),
                Ok(Completion::new("ok".to_string())),
            ],
        ));
        let retry = RetryProvider::with_config(
            mock.clone(),
            RetryConfig {
                max_retries: 3,
                base_delay: Duration::from_millis(1),
                ..Default::default()
            },
        );
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
                Ok(Completion::new("ok".to_string())),
            ],
        ));
        let retry = RetryProvider::with_config(
            mock.clone(),
            RetryConfig {
                max_retries: 3,
                base_delay: Duration::from_millis(1),
                ..Default::default()
            },
        );
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
        let retry = RetryProvider::with_config(
            mock.clone(),
            RetryConfig {
                max_retries: 3,
                base_delay: Duration::from_millis(1),
                ..Default::default()
            },
        );
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
        let retry = RetryProvider::with_config(
            mock.clone(),
            RetryConfig {
                max_retries: 3,
                base_delay: Duration::from_millis(1),
                ..Default::default()
            },
        );
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
        let retry = RetryProvider::with_config(
            mock.clone(),
            RetryConfig {
                max_retries: 3,
                base_delay: Duration::from_millis(1),
                ..Default::default()
            },
        );
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
                retry_after_raw: vec![],
                received_at: None,
            })],
        ));
        let retry = RetryProvider::with_config(
            mock.clone(),
            RetryConfig {
                max_retries: 3,
                base_delay: Duration::from_millis(1),
                ..Default::default()
            },
        );
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
        let retry = RetryProvider::with_config(
            mock.clone(),
            RetryConfig {
                max_retries: 2,
                base_delay: Duration::from_millis(1),
                ..Default::default()
            },
        );
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
                Ok(Completion::new("recovered".to_string())),
            ],
        ));
        let retry = RetryProvider::with_config(
            mock.clone(),
            RetryConfig {
                max_retries: 3,
                base_delay: Duration::from_millis(1),
                ..Default::default()
            },
        );
        let config = CompletionConfig::default();
        let result = retry.complete("sys", "usr", &config).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().text, "recovered");
        assert_eq!(mock.call_count(), 2);
    }

    /// RetryProvider default config: 3 retries, 1s delay.
    #[test]
    fn test_retry_provider_default_config() {
        let mock = Arc::new(MockProvider::new("p", "m"));
        let retry = RetryProvider::new(mock);
        assert_eq!(retry.config().max_retries, 3);
        assert_eq!(retry.config().base_delay, Duration::from_secs(1));
    }

    #[test]
    fn test_resolve_claude_alias_opus_returns_claude_opus_4_7() {
        let result = resolve_claude_alias("opus").unwrap();
        assert_eq!(result, "claude-opus-4-7");
    }

    #[test]
    fn test_resolve_claude_alias_sonnet_returns_claude_sonnet_4_6() {
        let result = resolve_claude_alias("sonnet").unwrap();
        assert_eq!(result, "claude-sonnet-4-6");
    }

    #[test]
    fn test_resolve_claude_alias_haiku_returns_claude_haiku_4_5_20251001() {
        let result = resolve_claude_alias("haiku").unwrap();
        assert_eq!(result, "claude-haiku-4-5-20251001");
    }

    /// Consumers who pinned "claude-opus-4-6" from v0.1.x get the string passed through
    /// unchanged — backward compatibility for callers that already resolved the alias.
    #[test]
    fn test_resolve_claude_alias_accepts_literal_claude_opus_4_6_passthrough() {
        // Consumers may have pinned the string "claude-opus-4-6" from v0.1.x;
        // the resolver must pass any string containing "claude-" through unchanged.
        assert_eq!(
            resolve_claude_alias("claude-opus-4-6").unwrap(),
            "claude-opus-4-6"
        );
    }

    #[test]
    fn finish_reason_keeps_an_unknown_wire_value_instead_of_dropping_it() {
        // The capture campaign returned THREE observed values — stop, length and
        // `load` — and `load` appeared in no documentation the project had. The
        // space of wire values is NOT closed.
        let r = FinishReason::from_wire("something_new_2027");
        assert_eq!(r, FinishReason::Other("something_new_2027".into()));
    }

    #[test]
    fn finish_reason_other_is_capped_at_64_chars_on_a_char_boundary() {
        // Wire text entering PUBLIC, SERIALIZED telemetry. The crate caps
        // everything else that comes from outside; a reason is a word.
        let long = "á".repeat(200);
        let FinishReason::Other(s) = FinishReason::from_wire(&long) else {
            panic!("an unknown value must land in Other")
        };
        assert!(s.chars().count() <= 64);
    }

    #[test]
    fn an_external_implementor_can_report_real_telemetry_not_only_unmeasured() {
        // `#[non_exhaustive]` on a STRUCT forbids literal construction from
        // another crate. Without builders, an external provider could only ever
        // say "unmeasured" — even holding the numbers. That would contradict the
        // very argument that made complete() return a richer type.
        let t = CompletionTelemetry::unmeasured()
            .with_finish(FinishReason::Stop)
            .with_completion_tokens(602);
        assert_eq!(t.finish, Some(FinishReason::Stop));
        assert_eq!(t.completion_tokens, Some(602));
    }

    #[test]
    fn from_string_yields_unmeasured_never_zeros() {
        let c: Completion = "hello".to_string().into();
        assert_eq!(
            c.telemetry.completion_tokens, None,
            "a 0 meaning 'could not measure' is the same lie as a zeroed input size"
        );
    }

    #[test]
    fn completion_new_takes_only_the_mandatory_field() {
        // A fixed-arity `new` on a #[non_exhaustive] type breaks with the first
        // new field — exactly the trap the sibling builder exists to avoid.
        let c = Completion::new("text".into());
        assert_eq!(c.text, "text");
    }

    #[test]
    fn unsupported_is_distinguishable_from_measured_zero() {
        // Three states, not two: "the backend cannot", "it can and the model did
        // not reason", "it can and it reasoned". An Option would collapse the
        // first two.
        assert_ne!(
            ReasoningState::Unsupported {
                backend: "anthropic".into(),
                chars: Some(0),
                text: None,
            },
            ReasoningState::Measured {
                chars: 0,
                text: None
            }
        );
    }

    #[test]
    fn the_default_covers_the_largest_measured_demand() {
        // A-7. `glm-5.2` demanded 10 686 completion tokens with the REAL system prompt on the
        // 62k bundle, so 8 192 cuts a demand that was measured, not imagined. And in the
        // degraded run `eb-run-1800.json` Caspar's SECOND candidate is measured converging at
        // both 8 192 and 16 384 — with this default that `degraded 2/3` would have been 3/3.
        //
        // What it does NOT buy is worth pinning in the same breath: `deepseek-v4-pro` converges
        // at no value tried, 32 768 included. The budget was never the whole problem; the
        // reasoning channel was, and that is the C axis. Reading this raise as "fixed" repeats
        // the reporter's own first hypothesis, which they measured until it broke.
        assert_eq!(CompletionConfig::default().max_tokens, 16_384);
    }
    // ---------------------------------------------------------------------
    // Task 16 — `reasoning_trace`: the four things activating it accepts.
    // ---------------------------------------------------------------------

    /// The four warnings A-9 makes a REQUIREMENT of this flag, each keyed by a phrase that
    /// cannot survive a rewrite that drops the point.
    ///
    /// Guarded mechanically because the promise already broke once: the flag's own rustdoc said
    /// the four were "documented where the report field it feeds is defined", and that field's
    /// rustdoc did not carry them. A pointer to a document that does not say the thing is worse
    /// than no pointer — the reader follows it and comes back believing they read the warning.
    const TRACE_WARNINGS: [(&str, &str); 4] = [
        ("whose text it is", "model"),
        ("it skips the Validator", "Validator"),
        ("it is not redacted", "redact"),
        ("how big it can get", "max_rotations"),
    ];

    #[test]
    fn the_trace_flag_names_everything_activating_it_accepts() {
        // A flag that transfers responsibility without naming it does not transfer it: whoever
        // turns it on without knowing what it drags along did not choose, they inherited.
        let src = include_str!("provider.rs");
        let start = src
            .find("pub reasoning_trace: bool,")
            .expect("the flag must exist");
        // The rustdoc sits ABOVE the field, so read back to the previous field's declaration.
        let doc_start = src[..start]
            .rfind("pub reasoning: ReasoningControl,")
            .expect("the preceding field anchors the block");
        let doc = &src[doc_start..start];
        for (what, needle) in TRACE_WARNINGS {
            assert!(
                doc.contains(needle),
                "the rustdoc must say {what} (looked for {needle:?})"
            );
        }
    }

    #[test]
    fn the_provider_trace_and_the_verdict_reasoning_are_different_fields() {
        // T-5.2. `AgentOutput::reasoning` is one of the seven verdict keys and exists ALWAYS;
        // this flag captures the PROVIDER's channel, which is another thing entirely. Naming
        // this one `reasoning` would have put two different things under one name in the same
        // output — which is why it is `reasoning_trace`.
        //
        // Asserted on the TYPES rather than on a run: what must never happen is one field
        // overwriting the other, and that is a structural property, not a runtime one.
        let cfg = CompletionConfig::default().with_reasoning_trace(true);
        assert!(cfg.reasoning_trace);
        // The verdict's own field is untouched by the flag, and carries text of its own.
        let out = crate::schema::AgentOutput {
            agent: crate::schema::AgentName::Caspar,
            verdict: crate::schema::Verdict::Approve,
            confidence: 0.9,
            summary: "s".to_string(),
            reasoning: "the model's own reasoning, inside the verdict".to_string(),
            findings: Vec::new(),
            recommendation: "go".to_string(),
        };
        assert!(!out.reasoning.is_empty());
    }
    // ---------------------------------------------------------------------
    // Task 19 — the synthetic HTTP status is gone from the tree.
    // ---------------------------------------------------------------------

    #[test]
    fn no_synthetic_http_status_survives_anywhere_in_the_crate() {
        // B-1/B-3, as a MECHANICAL check rather than a reading. The sentinel this looks for was
        // a contract failure wearing an HTTP error's clothes, and that disguise is the root
        // cause of the whole milestone: it inherited run-wide semantics by carrying the wrong
        // type.
        //
        // The needle is BUILT from parts, so this test's own source does not contain it. The
        // same self-reference already made a sibling check pass while guarding nothing.
        let needle = concat!("PARSE_", "FAILURE_STATUS");
        //
        // With it gone, `Http.status` only ever holds a real status -- which makes lineage
        // condemnation honest BY CONSTRUCTION rather than by comment.
        //
        // WALKED, never enumerated. The list used to name four files and omitted
        // `providers/openai_compat.rs` -- the one the sentinel was actually CONSTRUCTED in.
        // All three reviewers found that independently, and it is the same failure the
        // record-site guard had: a hand-maintained allowlist reports success over whatever
        // nobody remembered to add, and this one backs an acceptance criterion.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files: Vec<std::path::PathBuf> = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("src/ must be readable") {
                let path = entry.expect("a readable entry").path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    files.push(path);
                }
            }
        }
        files.sort();
        assert!(
            files.len() >= 10,
            "the walk found {} files; it is not reaching src/",
            files.len()
        );

        // The SHAPE as well as the name. Deleting the constant and writing the literal back
        // would restore the defect while leaving a name-only check green, so the disguise is
        // searched for as it would actually be worn.
        let shape = concat!("status: ", "0");
        for path in files {
            let file = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            let src = std::fs::read_to_string(&path).expect("a readable source file");
            assert!(
                !src.contains(needle),
                "the disguise is still alive in {file}"
            );
            // CODE only. Documenting where the sentinel went is exactly what a reader coming
            // from `3.2.0` needs, and a guard that forbids naming the thing it removed makes
            // the removal undocumentable — the same correction `check_r0.sh` already took.
            let code: String = src
                .lines()
                .filter(|l| {
                    let t = l.trim_start();
                    !t.starts_with("//")
                })
                .collect::<Vec<_>>()
                .join(
                    "
",
                );
            assert!(
                !code.contains(shape),
                "a synthetic zero status was written back by hand in {file}"
            );
        }
    }

    #[test]
    fn a_refused_redirect_is_mage_local_and_never_retried() {
        // The one site that was NOT a content failure and still used the sentinel. It kept the
        // zero for three properties — never a real status, non-retryable, mage-local — and only
        // the first of those came from the zero itself.
        //
        // It lands in `ResponseContract` because that is where its CONSEQUENCE lives, which is
        // the unit of separation in this error type. What it must never be: `Network`, whose
        // connection class trips the endpoint-down latch and would abort a whole run over a
        // misconfigured redirect chain on one seat.
        let e = ProviderError::ResponseContract {
            reason: ResponseContractCause::RedirectRefused,
            detail: String::new(),
        };
        assert!(!is_retryable(&e));
        // Same chain, same failure, every time: retrying only spends budget.
        assert!(e.to_string().contains("redirect"));
    }
}
