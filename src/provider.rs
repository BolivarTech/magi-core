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
    /// The turn ended on terms the backend named, and the output budget was not
    /// one of them.
    ///
    /// Wider than "the model finished its answer", and it always was: `tool_use`
    /// has mapped here since the wire translation existed, and a turn that stops to
    /// call a tool has not finished anything. `refusal`, `pause_turn`,
    /// `content_filter`, `tool_calls` and `stop_sequence` join it. What they share
    /// is the only property anything downstream asks of them -- the reply is not
    /// short because it ran out of room -- so the remedy for an empty one is never
    /// to raise the cap.
    ///
    /// # What the fold costs, said plainly
    ///
    /// This variant does not carry the word that produced it, so a report of a
    /// refusal reads `Stop` and the fact that the model REFUSED is gone from it.
    /// A real loss, chosen rather than overlooked: the alternatives were a variant
    /// per vendor word -- which does not scale past two vendors -- or a payload
    /// here, which would touch every construction and match of a type nothing
    /// outside the crate needs the payload from. **Every consumer of this value
    /// consumes exactly one property.** The day one needs the word, the fix is a NEW
    /// variant -- additive, this enum being `#[non_exhaustive]` -- and **not** a payload
    /// here: adding a field to a unit variant is a break, so that route costs another
    /// major and this one costs none.
    Stop,
    /// The output budget ran out before the model finished.
    ///
    /// **A declared boundary: this value covers two causes with opposite remedies.**
    /// The OpenAI-compatible wire reports `finish_reason: "length"` both for hitting
    /// `max_tokens` and for running past the model's context window, carrying nothing
    /// that separates them. The Anthropic wire *does* separate them, and its
    /// `model_context_window_exceeded` is mapped here **on purpose**, so that one
    /// condition does not read as two different things depending on which backend
    /// answered. This crate does not guess between the two -- it reports what the
    /// backend said -- so a completion cut by an oversized PROMPT arrives here
    /// indistinguishable from one cut by an undersized budget.
    /// The remedies differ (shrink the input versus raise the cap), and telling them
    /// apart needs `prompt_tokens` against the measured window, which
    /// [`crate::reporting::CompletionRecord`] carries for exactly that comparison --
    /// **the report is where that distinction survives, not this value.** Folding
    /// `model_context_window_exceeded` in here means this variant no longer separates
    /// the two even on the wire that separates them, which is the price of one condition
    /// reading the same way whichever backend answered.
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
    /// * `raw` — the value the backend sent, on any wire this crate speaks: the
    ///   OpenAI-compatible `finish_reason`, Anthropic's `stop_reason`, or the native
    ///   Ollama `done_reason`.
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
            // The union of every wire's published not-the-budget vocabulary. It is ONE
            // table on purpose: `Other` is documented in four places as "a value no vendor
            // publishes", and that was only true per-wire while each provider kept half the
            // list -- an Anthropic word arriving on the compat wire fell through to `Other`
            // and the message then claimed the budget could not be told. The bleed cannot
            // change an answer, because every word here carries the same bearing on every
            // wire; it can only reach a correct answer by a route nobody planned.
            "stop" | "content_filter" | "tool_calls" | "function_call" | "end_turn"
            | "stop_sequence" | "tool_use" | "refusal" | "pause_turn" => Self::Stop,
            // The budget half of the same union. `max_tokens` is Anthropic's word for it
            // and `model_context_window_exceeded` is Anthropic's out-of-room response --
            // read as `Length` because that is what it is, and the same condition the
            // compat wire reports as `"length"`, so ONE condition does not read as two
            // things depending on which backend answered. Leaving these two behind while
            // moving the not-the-budget words was worse than not merging at all: it made
            // `Other` mean "unpublished" for the words nobody was arguing about and
            // "published but untranslated" for the two this release is named after.
            "length" | "max_tokens" | "model_context_window_exceeded" => Self::Length,
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
#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

    /// What the termination says about the output budget as an explanation for an
    /// empty completion.
    ///
    /// **Three states, not two, and the third is the point.** An earlier form of
    /// this returned a `bool`, which forced every reason the crate does not
    /// recognise onto the "not the budget" side -- asserting a negative from an
    /// uninterpreted string. That is the same defect as the message it was written
    /// to fix, with the sign flipped. A value neither vendor publishes could be
    /// anything at all, a new way of saying the room ran out included, and the crate
    /// has no basis to rule that out. *(The example that first showed this was
    /// `model_context_window_exceeded`, which used to fall through untranslated. It
    /// no longer does -- the published vocabularies were completed, which shrank this
    /// branch to what genuinely belongs in it. The branch is still needed: the
    /// vocabularies grow without asking us.)*
    ///
    /// The `match` is exhaustive on purpose rather than a `matches!`: a variant
    /// added to [`FinishReason`] later must not be able to join a branch silently,
    /// and in-crate exhaustiveness makes it a compile error instead. That is the
    /// same discipline `provider_err_outcome` keeps for its own consequences.
    ///
    /// # Returns
    ///
    /// Which of the three the reported termination supports. An ABSENT termination
    /// is [`BudgetBearing::MayExplain`], because unknown is not evidence that the
    /// budget was untouched.
    pub(crate) fn budget_bearing(&self) -> BudgetBearing {
        match self.finish {
            None | Some(FinishReason::Length) => BudgetBearing::MayExplain,
            Some(FinishReason::Stop) | Some(FinishReason::Load) => BudgetBearing::RuledOut,
            Some(FinishReason::Other(_)) => BudgetBearing::Unknown,
        }
    }
}

impl std::fmt::Debug for ReasoningState {
    /// Renders the state without the trace, marking the elision rather than hiding it.
    ///
    /// `#[derive(Debug)]` printed the opt-in trace: model-authored text that never passes
    /// the `Validator` and is never redacted. Anything composed with `{:?}` therefore
    /// carried it -- a diagnostic in this crate did, and a consumer logging a
    /// [`crate::error::ProviderError`] the same way still would, since that type derives
    /// `Debug` and can hold this one. Composing carefully at each site is a rule someone
    /// eventually applies wrong; at the type there is nothing to apply.
    ///
    /// The elision is **announced**, following the same shape `ClaudeProvider` uses for its
    /// API key: a `Debug` that silently drops a field misleads the developer reading it,
    /// while one that says a field was withheld tells them exactly where to look.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotMeasured => f.write_str("NotMeasured"),
            Self::Unsupported {
                backend,
                chars,
                text,
            } => f
                .debug_struct("Unsupported")
                .field("backend", backend)
                .field("chars", chars)
                .field("text", &elided(text))
                .finish(),
            Self::Measured { chars, text } => f
                .debug_struct("Measured")
                .field("chars", chars)
                .field("text", &elided(text))
                .finish(),
        }
    }
}

/// How a withheld trace is announced inside [`ReasoningState`]'s `Debug`.
///
/// # Parameters
/// * `text` — the trace, when the consumer opted into carrying it.
///
/// # Returns
/// A marker naming the length, never the content.
fn elided(text: &Option<String>) -> String {
    match text {
        Some(t) => format!("<{} chars withheld>", t.chars().count()),
        None => "None".to_string(),
    }
}

impl ReasoningState {
    /// The state rendered as a MEASUREMENT, with the trace text left out.
    ///
    /// `Debug` on this type prints the trace when a consumer opted into carrying it, so
    /// any diagnostic composed with `{:?}` would copy model-authored text -- text that
    /// never passes the `Validator` and is never redacted -- into whatever it composes.
    /// One such diagnostic reached [`crate::error::ProviderError::ResponseContract`],
    /// whose own rustdoc promises the opposite. Composing from this instead makes the
    /// promise structural: there is nothing to leak because the text is not rendered.
    ///
    /// # Returns
    ///
    /// A short description naming the state and, where one exists, the length -- never
    /// the trace itself.
    ///
    /// Gated: the Anthropic provider is the only composer of a diagnostic that carries the
    /// reasoning state, and a helper compiled where nothing calls it is dead weight the
    /// linter is right to name.
    #[cfg(feature = "claude-api")]
    pub(crate) fn measurement(&self) -> String {
        match self {
            Self::NotMeasured => "not measured".to_string(),
            Self::Unsupported { backend, chars, .. } => match chars {
                Some(n) => format!("unsupported by {backend}, {n} chars"),
                None => format!("unsupported by {backend}, unmeasured"),
            },
            Self::Measured { chars, .. } => format!("{chars} chars"),
        }
    }
}

/// Helpers shared by the source-scanning guards, which live in several modules.
///
/// They read their own file with `include_str!` to assert properties no behavioural test
/// can see -- that a rule is consulted from one place, that a wire type stayed private.
/// The reading is what needs the care, which is why it is written once.
///
/// Gated on the providers whose guards use it: with neither feature on, the module compiles
/// with no caller and the linter is right to name it. That is not a reason to widen it --
/// the two guards are where the scanning happens.
#[cfg(all(test, any(feature = "claude-api", feature = "openai-compat")))]
pub(crate) mod source_scan {
    /// The half of a source file that ships, with line endings normalized.
    ///
    /// # Two properties, and the second is why this is a function
    ///
    /// **Normalization**: `core.autocrlf` is on for this repo, so a Windows checkout has
    /// CRLF on disk. `include_str!` embeds those bytes verbatim while rustc normalizes a
    /// multi-line marker in source to LF, so a split on one silently found nothing and the
    /// "production half" became the WHOLE file. **The dangerous direction is not the one
    /// that surfaced**: guards whose needle appears in their own message failed loudly,
    /// but a guard whose needle lives only in the production half would have PASSED while
    /// guarding nothing.
    ///
    /// **The split is asserted.** Removing the normalization, renaming the module, or any
    /// other reason the marker stops matching fails HERE, by name, instead of degrading
    /// every caller into a guard over the whole file. A silent fallback is what put this
    /// class in the tree twice.
    ///
    /// # Parameters
    /// * `src` -- the file's own source, from `include_str!`.
    ///
    /// # Returns
    /// Everything before the `#[cfg(test)]` module that opens the test half.
    ///
    /// # Panics
    /// If the marker is absent, which means the split guarded nothing.
    pub(crate) fn production_half(src: &str) -> String {
        let normalized = src.replace("\r\n", "\n");
        let marker = "\n#[cfg(test)]\nmod ";
        assert!(
            normalized.contains(marker),
            "the test-module marker was not found, so every guard built on this would have scanned the whole file and reported success while checking nothing"
        );
        normalized
            .split(marker)
            .next()
            .expect("split always yields at least one part")
            .to_string()
    }
}

/// What a reported termination says about the output budget.
///
/// Internal: it exists so **one** rule decides both whether a provider classifies a
/// contentless reply as an empty completion and whether the resulting message
/// prescribes raising the cap. Two sites branching on two copies of one rule is how
/// they drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BudgetBearing {
    /// The budget could be the explanation: it ran out, or nothing was reported.
    MayExplain,
    /// The backend named a reason, and that reason is not the budget.
    RuledOut,
    /// The backend named a reason this crate does not interpret. Neither direction
    /// is supportable, and saying either would be inventing evidence.
    Unknown,
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

/// The range `operation_budget` must sit in to preserve BOTH of its properties.
///
/// Derived rather than hardcoded: `[302, 603)` is only what it gives for the shipped
/// `client_timeout = 300` and `base_delay = 1`. Writing those literals would make the guard lie
/// for anyone who moves either value — which is the defect this function exists to prevent.
///
/// - **Floor, `ct + bd + 1`:** the budget is checked AFTER the first attempt. That attempt costs
///   `ct`, and the flat backoff of `Timeout`/`Network` adds `bd`, so the check lands near
///   `ct + bd`. The `+1` makes the comparison strict rather than being a magic number. Below the
///   floor the check cuts there and the **second attempt never starts**, losing the determinism
///   the per-class count was chosen for.
/// - **Ceiling, `2*ct + 2*bd + 1`:** above it a `Retry-After` chain reaches its THIRD check
///   instead of being cut at its second, costing roughly five more minutes per seat.
///
/// It lives in production, not in the test module: reimplementing it beside the tests would give
/// two formulas that can diverge, and the tests would stay green while the guard did something
/// else.
pub(crate) fn budget_window(
    client_timeout: Duration,
    base_delay: Duration,
) -> std::ops::Range<Duration> {
    // Saturating: `base_delay` is a `Duration` a consumer sets directly, so both terms are
    // reachable from a public field. Panicking inside a WARNING path would take down the
    // consumer's process for our own arithmetic.
    let floor = client_timeout
        .saturating_add(base_delay)
        .saturating_add(WINDOW_STRICTNESS_MARGIN);
    let ceiling = client_timeout
        .saturating_mul(2)
        .saturating_add(base_delay.saturating_mul(2))
        .saturating_add(WINDOW_STRICTNESS_MARGIN);
    floor..ceiling.max(floor)
}

/// The `+ 1` of the window formula: what makes the floor comparison strict rather than exact.
///
/// A whole second on a scale of hundreds, so it never competes with the terms it guards.
const WINDOW_STRICTNESS_MARGIN: Duration = Duration::from_secs(1);

/// Named default values (no magic numbers).
const DEFAULT_MAX_RETRIES: u32 = 3;
/// Retries for a class in `limited_retry_classes`, giving two attempts in total.
///
/// One rather than zero: a `Timeout` condemns the lineage run-wide, so rotating on the first
/// hang takes that lineage from the other two mages over a transient spike. The second retry
/// buys almost nothing — an endpoint that hung twice is not having a spike.
const DEFAULT_LIMITED_MAX_RETRIES: u32 = 1;
const DEFAULT_BASE_DELAY: Duration = Duration::from_secs(1);
const DEFAULT_CAP: Duration = Duration::from_secs(60);
const DEFAULT_RETRY_AFTER_CAP: Duration = Duration::from_secs(300);
/// The retry chain's BACKSTOP, not its operating limit.
///
/// # Why 450 and not any other number
///
/// With the per-class attempt count binding (see `DEFAULT_LIMITED_MAX_RETRIES`), the count cuts
/// the chain before this budget does: its only check lands at roughly 301 s and does not fire on
/// the normal path. It exists so that if something escapes the count, the abandonment is TYPED
/// (`AbandonReason::OperationBudgetExhausted`) rather than an opaque timeout cut.
///
/// # It sits inside a window, and moving it out breaks something silently
///
/// `[302, 603)` is the ONLY range that preserves both properties at once:
///
/// - **Floor (302):** the check after the first attempt must NOT cut, so the second attempt of a
///   hang actually runs. That is the determinism the attempt count was chosen for.
/// - **Ceiling (603):** a `Retry-After` chain — whose waits count toward `elapsed` even though
///   the sleep is not interrupted — is cut at its SECOND check (~604 s) rather than its third
///   (~906 s). That is what bounds the `Retry-After` path without lowering `retry_after_cap`,
///   which would turn honoured waits into abandonments.
///
/// A value below the floor costs the second attempt; above the ceiling costs ~5 more minutes per
/// seat on the `Retry-After` path.
///
/// **Only the floor is reported.** Losing the second attempt is silent and irreversible within a
/// run; being above the ceiling costs wall clock and is exactly what raising `client_timeout`
/// produces — which the local-deployment guidance prescribes as step one. A notice that fires on
/// a configuration the crate's own documentation instructs gets filtered, taking the real case
/// with it.
const DEFAULT_OPERATION_BUDGET: Duration = Duration::from_secs(450);

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
/// # The COUNT is the operating limit; this budget is a backstop
///
/// This inverts what an earlier reading of these fields suggested, so it is said plainly rather
/// than left to be inferred.
///
/// [`limited_retry_classes`] caps the attempts for the classes that can each burn a whole client
/// timeout, and that count cuts the chain **before** [`operation_budget`] does: with the shipped
/// defaults the budget's only check lands at roughly 301 s and **does not fire** on the normal
/// path. The budget exists so that if something escapes the count, the abandonment is **typed**
/// (`AbandonReason::OperationBudgetExhausted`) instead of an opaque cut — not to be the thing
/// that ends an ordinary chain.
///
/// Reading it the other way round leads to tuning the budget expecting the attempt count to
/// follow, which it will not.
///
/// # Layering against the per-agent timeout — the defaults DO satisfy it
///
/// The ceiling must cover the worst case of one chain:
///
/// ```text
/// MagiConfig::timeout >= (1 + limited_max_retries) * client_timeout + backoffs
/// ```
///
/// Which the defaults meet for a HOMOGENEOUS limited-class chain: `2 * 300 + 1 = 601 s` (and
/// about `604 s` through the `Retry-After` path) against a `660 s` ceiling.
///
/// **A MIXED chain does not fit, and the ceiling cuts first.** A `429` is not attempt-limited, so
/// it keeps the general count and each honoured `Retry-After` runs in full. The bound is
///
/// ```text
/// operation_budget + max(client_timeout, retry_after_cap + jitter)
/// ```
///
/// = `751 s` with the shipped values. **The binding term is whichever of the two is larger**, and
/// with both at 300 s they coincide — so writing it as `budget + client_timeout` would name the
/// wrong parameter and mislead anyone who tunes only the cap. Above the ceiling either way, so
/// the outer timeout ends it and the abandonment is opaque rather than the typed
/// `AbandonReason::OperationBudgetExhausted`. That trade is deliberate — see
/// [`MagiConfig::timeout`] — and is stated rather than left as an implied guarantee.
///
/// **The older form — `operation_budget + client_timeout <= MagiConfig::timeout` — is
/// deliberately NOT satisfied** (`450 + 300 = 750 > 660`), and that is not an oversight. It was
/// written when this budget was the binding limit; adding a backstop to a worst case it cannot
/// reach would charge about 25 minutes per seat of ceiling the chain can never use.
///
/// Said here as well as on [`MagiConfig::timeout`] deliberately: whoever tunes retries does not
/// necessarily read the orchestrator's config, and a layering rule documented on one side only is
/// a rule that gets broken from the other.
///
/// [`MagiConfig::timeout`]: crate::orchestrator::MagiConfig::timeout
/// [`limited_retry_classes`]: RetryConfig::limited_retry_classes
/// [`operation_budget`]: RetryConfig::operation_budget
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
    /// mistake. It now DOES warn — the floor guard added in `4.0.0` names this field
    /// directly, since `0` is below any floor — where before the symptom only showed up
    /// at runtime. For "no cap" use `Duration::MAX`, which the guard skips.
    ///
    /// # The floor this value must respect
    ///
    /// ```text
    /// operation_budget >= client_timeout + base_delay + 1
    /// ```
    ///
    /// With the shipped `client_timeout` of 300 s and `base_delay` of 1 s that is **302 s**, and
    /// the shipped `450` clears it. Below the floor the budget's first check cuts before the
    /// second attempt of a hang starts, losing the determinism the per-class attempt count exists
    /// for — silently, which is why the crate warns.
    ///
    /// **The warning computes that floor from the SHIPPED client timeout.** It cannot see yours:
    /// `client_timeout` belongs to each provider's HTTP client, not to this type. If you raised
    /// it, substitute your own value into the formula above — your real floor is higher, and the
    /// guard will not tell you.
    ///
    /// There is no upper bound worth warning about: a budget above the window lets a `Retry-After`
    /// chain run one check longer, which costs wall clock — and, on a mixed chain, can push the
    /// total past the agent ceiling, where the abandonment stops being typed. That is a real
    /// consequence rather than "nothing else", but it is not a silent one: it shows up as a
    /// timeout, and it is what raising `client_timeout` produces, so warning on it would fire on
    /// the configuration this crate's own guidance prescribes.
    pub operation_budget: Duration,
    /// Classes that use **flat** backoff instead of exponential.
    pub flat_classes: Vec<RetryClass>,
    /// Classes whose attempt count is [`limited_max_retries`] instead of [`max_retries`].
    ///
    /// Default: `[Timeout, Network]` — the two classes that can consume a whole client timeout
    /// per attempt. `Timeout` does so by definition: the model accepted the connection and kept
    /// generating. `Network` does so in its pathological case — a packet dropped in silence
    /// (firewall drop, host down), as opposed to the immediate refusal — so it belongs here too.
    /// Leaving it out would leave an undeclared hybrid regime: deterministic for one, time-bound
    /// for the other.
    ///
    /// # Orthogonal to [`flat_classes`], which is why a class can be in both
    ///
    /// That one governs the SHAPE of the backoff; this one governs HOW MANY attempts happen.
    /// `Timeout` appearing in both is expected rather than a conflict.
    ///
    /// # A `Vec`, not a map
    ///
    /// [`RetryClass`] derives neither `Ord` nor `Hash`, so a `BTreeMap`/`HashMap` key would
    /// require adding derives to a public type for a granularity nobody has asked for. It is
    /// also `#[non_exhaustive]`: a list with a fallback to the general count absorbs future
    /// classes without a further rule.
    ///
    /// [`limited_max_retries`]: RetryConfig::limited_max_retries
    /// [`max_retries`]: RetryConfig::max_retries
    /// [`flat_classes`]: RetryConfig::flat_classes
    pub limited_retry_classes: Vec<RetryClass>,
    /// Retries allowed for a class in [`limited_retry_classes`]. Default `1`, giving 2 attempts.
    ///
    /// # Why one and not zero
    ///
    /// A `Timeout` condemns the lineage run-wide, so rotating on the first hang takes that
    /// lineage away from the OTHER two mages over a transient load spike. One retry buys that
    /// second chance for one client timeout; a second buys almost nothing, because an endpoint
    /// that hung twice in a row is not having a spike.
    ///
    /// # Zero is legitimate and is not rejected
    ///
    /// "Do not retry, rotate straight away" is a valid choice. This crate imposes no artificial
    /// ceilings on the consumer's deployment.
    ///
    /// # It cannot usefully exceed [`max_retries`]
    ///
    /// The retry loop is bounded by `0..=max_retries`, so a larger value here is capped there and
    /// the limited classes get `max_retries + 1` attempts rather than the number asked for.
    /// The crate reports that through its dangerous-configuration warnings rather than clamping
    /// it: silently correcting a consumer's configuration is what this type refuses to do.
    ///
    /// [`max_retries`]: RetryConfig::max_retries
    ///
    /// [`limited_retry_classes`]: RetryConfig::limited_retry_classes
    pub limited_max_retries: u32,
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
            limited_retry_classes: vec![RetryClass::Timeout, RetryClass::Network],
            limited_max_retries: DEFAULT_LIMITED_MAX_RETRIES,
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
        if self.retry_after_cap >= self.operation_budget {
            // ONE honoured wait consuming the whole budget is the misconfiguration; a CHAIN of
            // them being cut by the budget is the design (that is what the budget's window is
            // for), so comparing the accumulated chain here would fire on this crate's own
            // defaults and be silenced on day one.
            //
            // What actually happens, pinned by `test_honored_retry_after_can_overrun_a_small_budget`:
            // the budget is checked at the TOP of an iteration and the honoured sleep runs at the
            // BOTTOM, so the wait is never interrupted. A wait at least as long as the budget
            // therefore RUNS IN FULL, overruns the budget by up to `cap - budget`, and the chain
            // abandons at the NEXT check — so the chain gets at most one of them.
            //
            // `>=` rather than `>` because equality already produces that.
            out.push(format!(
                "retry_after_cap ({:?}) >= operation_budget ({:?}): one honored Retry-After runs in full and overruns the budget, so the chain gets at most one of them before abandoning",
                self.retry_after_cap, self.operation_budget
            ));
        }
        // The budget's window. Everything it needs is here EXCEPT the client timeout, which
        // belongs to each provider's HTTP client and is not visible from this struct, so the
        // shipped default is the reference.
        //
        // That assumption travels in the MESSAGE, not in this comment. A consumer who raised
        // their own client timeout — which the local-deployment guidance tells them to do FIRST
        // — would otherwise get a window computed for a timeout they no longer use: a false
        // negative for the genuinely broken case, and a false positive for a correct one. The
        // formula is spelled out for the same reason: `budget_window` is `pub(crate)`, so
        // pointing the reader at it would name an item they cannot reach.
        let ct = DEFAULT_CLIENT_TIMEOUT;
        let bd = self.base_delay;
        let window = budget_window(ct, bd);
        let budget = self.operation_budget;
        // Only the FLOOR is reported, and that asymmetry is the point.
        //
        // Below it, the budget cuts before the second attempt of a hang starts — a silent loss of
        // the determinism the per-class count exists for.
        //
        // ABOVE the ceiling costs wall clock — and on a mixed chain can push the total past the
        // agent ceiling, where the abandonment stops being typed. Real, but not silent: it shows
        // up as a timeout. And it is exactly what raising the
        // client timeout produces — which the local-deployment guidance tells the consumer to do
        // FIRST. Warning there fires on the configuration this crate's own rustdoc prescribes,
        // and a notice that is noisy on correct configurations gets filtered, taking the genuinely
        // broken case with it. That is the pattern F-10 was rewritten to remove; reintroducing it
        // one guard later would be the same mistake.
        if self.operation_budget != Duration::MAX && budget < window.start {
            out.push(format!(
                "operation_budget ({budget:?}) is below {:?}, the floor for the DEFAULT client timeout of {ct:?}: the budget cuts before the second attempt of a hang starts. If you set a different client timeout ct, your floor is ct + {bd:?} + {WINDOW_STRICTNESS_MARGIN:?}",
                window.start
            ));
        }
        if self.limited_max_retries > self.max_retries {
            // The retry loop is bounded by `0..=max_retries`, so a larger limited count is capped
            // there and the consumer gets fewer attempts than they asked for, with nothing saying
            // so. Reported rather than clamped: correcting a consumer's configuration silently is
            // what this type's own rustdoc refuses to do.
            out.push(format!(
                "limited_max_retries ({}) is above max_retries ({}): the retry loop is bounded by max_retries, so the limited classes get {} attempts rather than the {} asked for",
                self.limited_max_retries,
                self.max_retries,
                self.max_retries.saturating_add(1),
                self.limited_max_retries.saturating_add(1)
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
                    // WHAT THIS DETECTS, and just as importantly what it does NOT.
                    //
                    // DETECTS: a budget smaller than ONE attempt plus its backoff. If the budget
                    // is already exhausted at the first check, a single attempt consumed it
                    // whole -- usually `operation_budget < provider timeout`. Detected by
                    // SYMPTOM rather than by comparing config, because the `LlmProvider` trait
                    // does not expose the wrapped timeout, so a construction-time comparison
                    // would be unreachable code.
                    //
                    // DOES NOT DETECT: an agent ceiling smaller than `budget + client_timeout`.
                    // This fires at `attempt == 1`, so it needs the chain to REACH a second
                    // attempt. When the orchestrator's outer `tokio::timeout` cancels the call
                    // first -- which is the shape of the layering defect -- the loop never
                    // reaches `attempt == 1` and NOTHING is emitted here. That case is silent
                    // and is addressed elsewhere: `Magi::worst_case_per_seat` shows the number
                    // the consumer actually bought.
                    //
                    // A guard that appears to cover more than it does is worse than none,
                    // because nobody goes looking for the one that is missing.
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

            // The effective attempt cap depends on the class of the error that JUST happened,
            // not on the one that started the chain. A chain can open with a `429` — four
            // attempts — and meet a `Timeout` on the second; the cap that governs is the one for
            // what IS happening. Resolving the class once on entry would give four, and that is
            // the defect this check exists to prevent.
            let limit = if self.config.limited_retry_classes.contains(&classify(&err)) {
                self.config.limited_max_retries
            } else {
                self.config.max_retries
            };
            if attempt >= limit {
                // Leaves through the same path as exhausting `max_retries`: the last error is
                // recorded and the post-loop code turns it into the caller's result. Returning
                // a different error here would make an attempt-capped chain distinguishable
                // from an exhausted one for no reason the consumer can act on.
                //
                // VERIFIED, not asserted — `the_attempt_cap_and_an_exhausted_chain_agree` pins
                // it. Two reviewers read this as sitting BEFORE the `Retry-After` abandonment
                // decision and therefore changing the final error type. It does not: that match
                // is above, and its `TooLong`/`Unintelligible` arms return before reaching here.
                // With the SHIPPED `limited_retry_classes` the two cannot interact either: that
                // match is on `ProviderError::Http`, which is not in the default list, so no
                // error reaching this break carries a `Retry-After` to discard. That list is
                // CONFIGURABLE, though — a consumer who puts `Http` in it makes the two meet, and
                // `the_two_exits_agree_for_a_configured_http_class` pins that they still agree.
                last_error = Some(err);
                break;
            }

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
        //
        // The error class matters now and did not before: this asserts the count for a class
        // that is NOT attempt-limited. `Http` fails fast, so four attempts cost seconds — which
        // is exactly why it keeps the general count. Its limited counterpart is the test below.
        let inner = Arc::new(FailingProvider::new(ProviderError::Http {
            status: 503,
            body: "err".into(),
            retry_after_raw: vec![],
            received_at: None,
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
    async fn a_limited_class_stops_at_its_own_count_not_at_max_retries() {
        // The behaviour change of this milestone, pinned end to end rather than only through
        // the `attempts_for` helper: `Network` is attempt-limited, so `max_retries = 3` does NOT
        // buy four attempts.
        //
        // This test previously asserted 4 with this same error, which is what makes the change
        // observable: a consumer who counted on four requests for a hang gets two.
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
        assert_eq!(
            inner.calls(),
            2,
            "1 initial + limited_max_retries, not 1 + max_retries"
        );
    }

    #[tokio::test]
    async fn zero_limited_retries_makes_a_hang_rotate_on_the_first_attempt() {
        // The zero case exercised through the real loop, not just the helper: a consumer who
        // wants to reach a different lineage immediately gets exactly one request.
        let inner = Arc::new(FailingProvider::new(ProviderError::Timeout {
            message: "hang".into(),
        }));
        let p = RetryProvider::with_config(
            inner.clone(),
            RetryConfig {
                base_delay: Duration::ZERO,
                limited_max_retries: 0,
                ..Default::default()
            },
        );
        let _ = p.complete("s", "u", &CompletionConfig::default()).await;
        assert_eq!(inner.calls(), 1, "no retry at all, by configuration");
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
        // 600 -> 450: the budget became a backstop rather than the operating limit, and 450 is
        // the value that sits in the `[302, 603)` window. See the constant's rustdoc; the window
        // itself is asserted by `the_backstop_sits_in_the_window_that_preserves_both_properties`.
        assert_eq!(c.operation_budget, Duration::from_secs(450));
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
        // default retry_after_cap (300s) >= operation_budget here (100s).
        //
        // The wait DOES overrun the budget: the reactive check is at the top of an iteration
        // and the sleep at the bottom, so a honoured wait always runs in full. What the guard
        // reports is that only ONE of them fits before the chain abandons.
        let cfg = RetryConfig {
            operation_budget: Duration::from_secs(100),
            ..Default::default()
        };
        let warnings = cfg.dangerous_settings();
        assert!(
            warnings.iter().any(|w| w.contains("overruns the budget")),
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
                // `Timeout` is attempt-limited by default (two attempts), which would end this
                // chain before the third scripted response. Raised here because the property
                // under test is that a transient timeout is RETRIED AND RECOVERS, not how many
                // attempts the shipped policy grants it — that has its own tests.
                limited_max_retries: 3,
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
                // Matched to `max_retries` so the general count governs: this test is about the
                // LAST error surviving an exhausted chain, not about the per-class policy.
                limited_max_retries: 2,
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
    fn debug_never_prints_the_trace_however_it_is_composed() {
        // At the TYPE, not at each call site. The trace is model-authored text that never
        // passes the `Validator` and is never redacted; one diagnostic in this crate carried
        // it through `{:?}` before this impl existed, and a consumer logging a
        // `ProviderError` -- which derives `Debug` and can hold this -- still would.
        //
        // The elision is announced rather than silent, the same shape `ClaudeProvider` uses
        // for its API key: a `Debug` that drops a field misleads whoever reads it.
        let secret = "a private chain of thought";
        for state in [
            ReasoningState::Measured {
                chars: secret.chars().count(),
                text: Some(secret.to_string()),
            },
            ReasoningState::Unsupported {
                backend: "anthropic".to_string(),
                chars: Some(secret.chars().count()),
                text: Some(secret.to_string()),
            },
        ] {
            let rendered = format!("{state:?}");
            assert!(
                !rendered.contains(secret),
                "the trace reached a Debug rendering: {rendered}"
            );
            assert!(
                rendered.contains("withheld"),
                "the elision must be announced, not silent: {rendered}"
            );
            assert!(
                rendered.contains("26"),
                "the length survives, because that is the measurement: {rendered}"
            );
        }

        // And it survives one level up, which is the case that actually reaches a consumer.
        let wrapped = format!(
            "{:?}",
            CompletionTelemetry::unmeasured().with_reasoning(ReasoningState::Measured {
                chars: secret.chars().count(),
                text: Some(secret.to_string()),
            })
        );
        assert!(!wrapped.contains(secret), "{wrapped}");
    }

    #[test]
    fn the_published_wire_vocabularies_are_translated_and_nothing_else_is() {
        // Lives HERE, next to `from_wire`, and NOT only in the provider that motivated it:
        // `openai_compat.rs` is feature-gated, so the per-commit run on the default feature
        // set never compiled the assertion that guards these literals. Deleting them left
        // that gate green -- which is the same shape as every other round of this decision.
        //
        // What lands in `Other` decides what the empty-completion message may claim: an
        // untranslated reason renders as "cannot be told", so a value a vendor publishes
        // that falls through here turns a knowable case into an unknowable one.
        // THREE wires, not two: `from_wire` is also what the native Ollama path reads
        // `done_reason` with. Its published set -- `stop`, `length`, `load` -- is a subset
        // of what is listed here, so the claim "no vendor publishes it" holds for that wire
        // too rather than being scoped away from it.
        let published_and_not_the_budget = [
            // OpenAI-compatible
            "stop",
            "content_filter",
            "tool_calls",
            "function_call",
            // Anthropic
            "end_turn",
            "stop_sequence",
            "tool_use",
            "refusal",
            "pause_turn",
        ];
        for raw in published_and_not_the_budget {
            assert_eq!(
                FinishReason::from_wire(raw),
                FinishReason::Stop,
                "{raw} is published and is not the output budget"
            );
            assert_eq!(
                CompletionTelemetry::unmeasured()
                    .with_finish(FinishReason::from_wire(raw))
                    .budget_bearing(),
                BudgetBearing::RuledOut,
                "{raw} must rule the budget out, not leave it undecided"
            );
        }

        // The budget half: published on one wire or the other, and the message must
        // PRESCRIBE for these rather than say the budget cannot be told.
        for raw in ["length", "max_tokens", "model_context_window_exceeded"] {
            assert_eq!(
                FinishReason::from_wire(raw),
                FinishReason::Length,
                "{raw} is how a wire says the response ran out of room"
            );
            assert_eq!(
                CompletionTelemetry::unmeasured()
                    .with_finish(FinishReason::from_wire(raw))
                    .budget_bearing(),
                BudgetBearing::MayExplain,
                "{raw} is exactly the case the cap can explain"
            );
        }
        assert_eq!(FinishReason::from_wire("load"), FinishReason::Load);

        // And the complement: `Other` is what no vendor publishes, on either wire.
        for raw in ["brand_new_reason", "another_unpublished_one"] {
            assert_eq!(
                FinishReason::from_wire(raw),
                FinishReason::Other(raw.to_string())
            );
            assert_eq!(
                CompletionTelemetry::unmeasured()
                    .with_finish(FinishReason::from_wire(raw))
                    .budget_bearing(),
                BudgetBearing::Unknown
            );
        }
    }

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

    /// How many attempts a class is allowed: `1 + limited_max_retries` when it is on the limited
    /// list, `1 + max_retries` otherwise.
    ///
    /// Lives beside the field it reads so the two tasks that consume it do not each rewrite it.
    fn attempts_for(cfg: &RetryConfig, class: RetryClass) -> u32 {
        if cfg.limited_retry_classes.contains(&class) {
            1 + cfg.limited_max_retries
        } else {
            1 + cfg.max_retries
        }
    }

    /// The two classes that can burn a whole `client_timeout` get two attempts; everything else
    /// keeps four.
    ///
    /// `Http` covers 429 and the transient 5xx: those fail fast, so four of them cost seconds.
    /// A `Timeout` costs the client timeout each time, which is what the shorter count is for.
    #[test]
    fn a_limited_class_gets_two_attempts_not_four() {
        let cfg = RetryConfig::default();
        assert_eq!(attempts_for(&cfg, RetryClass::Timeout), 2);
        assert_eq!(attempts_for(&cfg, RetryClass::Network), 2);
        assert_eq!(attempts_for(&cfg, RetryClass::Http), 4);
    }

    /// "Do not retry, rotate straight away" is a legitimate choice and is not rejected.
    ///
    /// This crate imposes no artificial ceilings; a consumer who wants to reach a different
    /// lineage on the first hang is entitled to say so.
    #[test]
    fn zero_limited_retries_is_legitimate_and_not_rejected() {
        // `RetryConfig` has no builders by design — fields over `Default`. Inside the crate the
        // struct-update form is available and clippy prefers it; an outside consumer, for whom
        // `#[non_exhaustive]` forbids the literal, assigns the field instead.
        let cfg = RetryConfig {
            limited_max_retries: 0,
            ..Default::default()
        };
        assert_eq!(attempts_for(&cfg, RetryClass::Timeout), 1);
    }

    /// `flat_classes` and `limited_retry_classes` are ORTHOGONAL: one governs the backoff shape,
    /// the other the attempt count. A class appearing in both is expected, not a conflict.
    #[test]
    fn the_two_class_lists_are_orthogonal() {
        let cfg = RetryConfig::default();
        for class in [RetryClass::Timeout, RetryClass::Network] {
            assert!(
                cfg.flat_classes.contains(&class),
                "{class:?} should back off flat"
            );
            assert!(
                cfg.limited_retry_classes.contains(&class),
                "{class:?} should also be attempt-limited; the lists govern different things"
            );
        }
    }

    /// The seven values of the time budget, each pinned by its NUMBER.
    ///
    /// Assertions on the value rather than on a range: a range test passes before the work
    /// starts — the old values fall inside it too — and that is not a Red phase, it is a task
    /// without one.
    ///
    /// The three unchanged ones are asserted as well — `DEFAULT_CLIENT_TIMEOUT`,
    /// `retry_after_cap` and `max_retries`. Without them there is no way to tell "was not
    /// touched" from "was touched by accident" while the other four moved or arrived.
    ///
    /// Six of the seven are here; the seventh is the agent ceiling, which belongs to
    /// `MagiConfig` and is pinned by `the_agent_ceiling_covers_the_worst_case_of_the_chain`.
    #[test]
    fn the_seven_defaults_are_exactly_these() {
        let r = RetryConfig::default();
        // `client_timeout` is not a field of `RetryConfig`: it belongs to each provider's HTTP
        // client, and the shared default is this constant.
        assert_eq!(DEFAULT_CLIENT_TIMEOUT, Duration::from_secs(300)); // unchanged
        assert_eq!(r.retry_after_cap, Duration::from_secs(300)); // unchanged
        assert_eq!(r.max_retries, 3); // unchanged
        assert_eq!(
            r.limited_retry_classes,
            vec![RetryClass::Timeout, RetryClass::Network]
        );
        assert_eq!(r.limited_max_retries, 1);
        assert_eq!(r.operation_budget, Duration::from_secs(450)); // 600 -> 450
    }

    /// The backstop sits in the one window that preserves BOTH properties.
    ///
    /// `[302, 603)` is the only range where the budget lets the second attempt of a hang run
    /// (determinism) AND cuts a `Retry-After` chain at its second check rather than its third.
    /// Moving it outside breaks one of the two, silently.
    #[test]
    fn the_backstop_sits_in_the_window_that_preserves_both_properties() {
        let b = RetryConfig::default().operation_budget.as_secs();
        assert!(
            (302..603).contains(&b),
            "budget {b} fell outside [302, 603): one of the two properties is now broken"
        );
    }

    /// The re-derived invariant holds with the shipped defaults.
    ///
    /// Everything is derived rather than literal, so the test stays true if someone moves
    /// `base_delay` or the client timeout — which is the same reason the budget window is
    /// expressed as a formula rather than as `[302, 603)` alone.
    ///
    /// The OLD invariant is not asserted. A test demanding that something FAIL breaks the day
    /// someone picks values where it happens to hold — and nothing would be wrong with that — so
    /// it is a maintenance trap over a relation that no longer governs anything. It lives in the
    /// rustdoc, which is where a reader arriving from `3.1.0` will look for it.
    #[test]
    fn the_re_derived_invariant_holds_with_the_shipped_defaults() {
        let r = RetryConfig::default();
        let ceiling = crate::orchestrator::MagiConfig::default().timeout.as_secs();
        // The REAL backoff, not filler: `Timeout` and `Network` are in `flat_classes`, so their
        // wait is FLAT (`base_delay`) rather than exponential, and with one limited retry there
        // is exactly one of them between the two attempts.
        let backoffs = r.limited_max_retries as u64 * r.base_delay.as_secs();
        let chain =
            (1 + r.limited_max_retries as u64) * DEFAULT_CLIENT_TIMEOUT.as_secs() + backoffs;
        assert!(
            chain <= ceiling,
            "the chain's worst case ({chain}s) must fit under the agent ceiling ({ceiling}s)"
        );
    }

    /// One honoured wait that eats the whole budget is flagged.
    ///
    /// A wait at least as long as the budget runs IN FULL — the check is at the top of an
    /// iteration and the sleep at the bottom, so it is never interrupted — and overruns the
    /// budget, so the chain gets at most one of them before abandoning.
    #[test]
    fn a_single_honoured_wait_that_eats_the_whole_budget_is_flagged() {
        let c = RetryConfig {
            retry_after_cap: Duration::from_secs(500),
            operation_budget: Duration::from_secs(450),
            ..Default::default()
        };
        assert!(
            c.dangerous_settings()
                .iter()
                .any(|w| w.contains("retry_after_cap") && w.contains("operation_budget")),
            "a cap that overruns the whole budget must be reported"
        );
    }

    /// Equality already produces the condition, which is why the comparison is `>=`.
    #[test]
    fn a_wait_exactly_equal_to_the_budget_is_flagged_too() {
        let c = RetryConfig {
            retry_after_cap: Duration::from_secs(450),
            operation_budget: Duration::from_secs(450),
            ..Default::default()
        };
        assert!(
            c.dangerous_settings()
                .iter()
                .any(|w| w.contains("retry_after_cap")),
            "at equality one honoured wait already consumes the whole budget"
        );
    }

    /// The test that makes the one above honest.
    ///
    /// A guard that fires on its own shipped defaults is switched off on day one, and then it
    /// guards nothing. Note this is also why the comparison is not against the ACCUMULATED
    /// chain: `max_retries * retry_after_cap` is `900 s` against a `450 s` budget, so that form
    /// would fire on every default run — and it would be a false positive, because the budget
    /// cutting a Retry-After chain is the design.
    #[test]
    fn the_crate_defaults_produce_no_retry_after_warning() {
        let c = RetryConfig::default();
        assert!(
            !c.dangerous_settings()
                .iter()
                .any(|w| w.contains("retry_after_cap")),
            "the shipped defaults must not trip their own guard"
        );
    }

    /// The default lands inside its own window.
    ///
    /// An earlier form of this used `(1 + limited_max_retries) * ct + 2` as the floor, giving
    /// 602 — and with that the guard would have fired on the crate's own defaults. The check that
    /// must NOT cut is the FIRST one, not the second.
    #[test]
    fn the_default_budget_lands_inside_its_own_window() {
        let c = RetryConfig::default();
        assert!(budget_window(DEFAULT_CLIENT_TIMEOUT, c.base_delay).contains(&c.operation_budget));
    }

    /// The window MOVES with both of its inputs.
    ///
    /// The second half is what the previous docstring claimed and the body did not do: moving
    /// `base_delay` has to move the window, or the guard lies for anyone who changes it.
    #[test]
    fn the_window_moves_with_client_timeout_and_base_delay() {
        let secs = Duration::from_secs;
        assert_eq!(budget_window(secs(300), secs(1)), secs(302)..secs(603));
        assert_eq!(budget_window(secs(600), secs(1)), secs(602)..secs(1203));
        // With a 600 s client timeout the shipped 450 no longer serves, and the guard must say so.
        assert!(!budget_window(secs(600), secs(1)).contains(&secs(450)));
        assert_eq!(budget_window(secs(300), secs(5)), secs(306)..secs(611));
        // And the sub-second term survives instead of collapsing into the zero case.
        assert_eq!(
            budget_window(secs(300), Duration::from_millis(500)).start,
            Duration::from_millis(301_500)
        );
    }

    /// A sub-second `base_delay` MOVES the floor; the guard must not round it away.
    ///
    /// `budget_window` took whole seconds, so a `base_delay` of 500 ms collapsed into the same
    /// window as a `base_delay` of ZERO, and a budget genuinely below the floor drew no warning.
    /// The blind band is under a second wide on a 300 s scale — which is exactly why it would
    /// never be found by reading the output, only by asserting the property.
    #[test]
    fn the_floor_guard_does_not_round_a_sub_second_base_delay_away() {
        // Shipped `ct` of 300 s and a 500 ms base delay put the floor at 301.5 s, so 301 s is
        // below it and must warn. Truncation moves that floor to 301 s and the warning vanishes.
        let c = RetryConfig {
            base_delay: Duration::from_millis(500),
            operation_budget: Duration::from_secs(301),
            ..Default::default()
        };
        assert!(
            c.dangerous_settings()
                .iter()
                .any(|w| w.contains("second attempt")),
            "301s is below the 301.5s floor a 500ms base_delay implies, and must warn"
        );
    }

    /// A budget INSIDE the window says nothing — the crate must not warn about itself.
    #[test]
    fn a_budget_inside_the_window_says_nothing() {
        let c = RetryConfig::default();
        assert!(
            !c.dangerous_settings()
                .iter()
                .any(|w| w.contains("second attempt"))
        );
    }

    /// The guard is WIRED into `dangerous_settings`, not merely defined.
    ///
    /// `budget_window` can be flawless and called by nobody. This crosses the function with its
    /// consumer: without it the window would be a correct function that guards nothing.
    #[test]
    fn the_window_guard_is_wired_into_dangerous_settings() {
        // Both under the floor: one just below it, one far below. Over the CEILING no longer
        // warns, deliberately — that is what a raised client timeout produces, and the guard
        // stopped firing on the configuration its own documentation prescribes.
        for secs in [301u64, 10] {
            let c = RetryConfig {
                operation_budget: Duration::from_secs(secs),
                ..Default::default()
            };
            assert!(
                !c.dangerous_settings().is_empty(),
                "budget {secs}s is below the floor and should have warned"
            );
        }
        // The counterpart — a budget above the window staying silent — is
        // `the_documented_local_recipe_does_not_trip_the_window_guard`, which asserts the same
        // 900 s case with the reasoning attached. Repeating it here would be a third copy.
    }

    /// The message's printed formula agrees with `budget_window`.
    ///
    /// The message hands the consumer a formula to compute their OWN window with, since the
    /// guard can only assume the shipped client timeout. Nothing related the two before, so
    /// changing `budget_window` would let the printed formula drift silently — the same "text
    /// that outlived its code" class this round exists to remove.
    #[test]
    fn the_window_message_formula_agrees_with_budget_window() {
        // NOT the default `base_delay` of 1 s, and that is the whole point: with `bd = 1` every
        // number the message could hardcode equals the number the function computes, so a message
        // carrying the literals `302`/`603` would pass this test while calling `budget_window`
        // never — which is precisely the drift it exists to catch.
        let bd = Duration::from_secs(5);
        let c = RetryConfig {
            operation_budget: Duration::from_secs(200), // outside, so the guard fires
            base_delay: bd,
            ..Default::default()
        };
        let msg = c
            .dangerous_settings()
            .into_iter()
            .find(|w| w.contains("second attempt"))
            .expect("the guard fires for a budget under the floor");

        // The FLOOR it printed must be the one the function computes. The message stopped
        // carrying the ceiling when the guard stopped firing above it.
        let shipped = budget_window(DEFAULT_CLIENT_TIMEOUT, bd);
        assert!(
            msg.contains(&format!("below {:?}", shipped.start)),
            "the printed floor must be the one the function computes: {msg}"
        );

        // And the formula it printed must be the one a consumer substitutes their own client
        // timeout into. (That `budget_window` agrees with itself for another `ct` is covered by
        // `the_window_moves_with_client_timeout_and_base_delay`; repeating it here would be the
        // duplication this round removed elsewhere.)
        assert!(
            msg.contains(&format!("ct + {bd:?} + {WINDOW_STRICTNESS_MARGIN:?}")),
            "the message must carry the formula a consumer substitutes into: {msg}"
        );
    }

    /// Following the documented local-deployment recipe must NOT trip the guard.
    ///
    /// The guidance on `MagiConfig::timeout` says to raise `client_timeout` FIRST and then keep
    /// the budget inside ITS window. A consumer who does exactly that — `ct = 600`, budget `900`
    /// — was warned, because the guard compares against the shipped 300 s. A notice that fires on
    /// the configuration its own documentation prescribes is filtered on day one, taking the
    /// genuinely broken case with it. That is the pattern F-10 was rewritten to remove.
    #[test]
    fn the_documented_local_recipe_does_not_trip_the_window_guard() {
        let c = RetryConfig {
            operation_budget: Duration::from_secs(900),
            ..Default::default()
        };
        assert!(
            !c.dangerous_settings()
                .iter()
                .any(|w| w.contains("second attempt")),
            "a budget correct for a raised client timeout must not be reported as broken"
        );
    }

    /// A budget under the floor is still reported — that is the case worth having.
    ///
    /// Below the floor the budget cuts before the second attempt of a hang starts, losing the
    /// determinism the per-class count exists for. Above the ceiling costs wall clock and is what
    /// a legitimately raised client timeout produces, so only the floor is worth a notice.
    #[test]
    fn a_budget_under_the_floor_is_still_reported() {
        let c = RetryConfig {
            operation_budget: Duration::from_secs(200),
            ..Default::default()
        };
        assert!(
            c.dangerous_settings()
                .iter()
                .any(|w| w.contains("second attempt")),
            "losing the second attempt is silent, so it must be announced"
        );
    }

    /// `limited_max_retries` above `max_retries` is capped by the loop, so it must be reported.
    ///
    /// The loop is `0..=max_retries`, so a consumer asking for ten limited retries against three
    /// general ones gets four attempts, not eleven — silently. All three reviewers raised this
    /// independently.
    #[test]
    fn a_limited_count_above_the_general_one_is_reported() {
        let c = RetryConfig {
            max_retries: 3,
            limited_max_retries: 10,
            ..Default::default()
        };
        assert!(
            c.dangerous_settings()
                .iter()
                .any(|w| w.contains("limited_max_retries") && w.contains("max_retries")),
            "a knob that is silently ignored above a threshold must say so"
        );
    }

    /// `budget_window` cannot overflow, however absurd its inputs.
    ///
    /// `base_delay` is a `Duration` a consumer sets directly, so `2 * bd` is reachable from a
    /// public field. Panicking in a warning path would take down the consumer's process for our
    /// own arithmetic.
    #[test]
    fn the_window_saturates_instead_of_overflowing() {
        let w = budget_window(Duration::MAX, Duration::MAX);
        assert!(w.start <= w.end, "a saturated window must stay well-formed");
        // And the guard that calls it must not panic either.
        let c = RetryConfig {
            base_delay: Duration::from_secs(u64::MAX),
            ..Default::default()
        };
        let _ = c.dangerous_settings();
    }

    /// The mixed-class chain does NOT fit under the agent ceiling, and the rustdoc says so.
    ///
    /// A `429` keeps the general count and each honoured `Retry-After` runs in full, so such a
    /// chain is bounded by `operation_budget + client_timeout`, not by the limited-class formula.
    /// Pinned so the prose cannot drift: if someone raises the ceiling past the mixed bound, this
    /// test says the caveat about losing the typed abandonment can go.
    #[test]
    fn the_mixed_class_chain_exceeds_the_ceiling_which_the_rustdoc_states() {
        let r = RetryConfig::default();
        let ceiling = crate::orchestrator::MagiConfig::default().timeout.as_secs();
        let homogeneous = (1 + r.limited_max_retries as u64) * DEFAULT_CLIENT_TIMEOUT.as_secs()
            + r.limited_max_retries as u64 * r.base_delay.as_secs();
        assert!(
            homogeneous <= ceiling,
            "the limited-class chain must fit: {homogeneous} vs {ceiling}"
        );
        // The BINDING term is whichever of the two waits is larger, not `client_timeout`. With
        // the shipped values they coincide at 300 s, so deriving it from the client timeout alone
        // would pin a coincidence and stay green for a consumer who raised only the cap.
        let longest_wait = DEFAULT_CLIENT_TIMEOUT
            .as_secs()
            .max(r.retry_after_cap.as_secs() + 1); // +1: RETRY_AFTER_JITTER
        let mixed = r.operation_budget.as_secs() + longest_wait;
        assert!(
            mixed > ceiling,
            "if the mixed bound ({mixed}s) now fits under {ceiling}s, the rustdoc caveat about losing the typed abandonment is stale"
        );
        assert!(
            longest_wait >= r.retry_after_cap.as_secs(),
            "the bound must follow retry_after_cap when it exceeds the client timeout"
        );
    }

    /// The floor guard cannot see a raised client timeout, and the blind spot is pinned.
    ///
    /// A consumer who follows step one of the local recipe (`ct = 600`) and leaves the budget at
    /// 450 is genuinely below THEIR floor of 602 — and gets no warning, because the guard
    /// computes from the shipped 300. Asserted deliberately: it is the documented limit of a
    /// guard that cannot reach the provider's client, not an oversight to be found by a consumer.
    #[test]
    fn the_floor_guard_cannot_see_a_raised_client_timeout() {
        let c = RetryConfig::default(); // budget 450, correct for ct = 300
        let real_floor = budget_window(Duration::from_secs(600), c.base_delay).start;
        assert!(c.operation_budget < real_floor);
        assert!(
            !c.dangerous_settings()
                .iter()
                .any(|w| w.contains("second attempt")),
            "silent by construction: below the real floor of {real_floor:?} for ct = 600, and the guard cannot know"
        );
    }

    /// With no fallback pool the worst case is one model, which is what the rustdoc publishes.
    ///
    /// The no-rotation arm was asserted nowhere: every existing case declared a pool.
    #[test]
    fn the_worst_case_with_no_pool_is_one_model() {
        let magi = crate::orchestrator::MagiBuilder::new(crate::test_support::ScriptProvider::new(
            "m-default",
            vec![crate::test_support::Beh::Ok],
        )
            as std::sync::Arc<dyn LlmProvider>)
        .build()
        .expect("the default trio builds");
        assert_eq!(
            magi.worst_case_per_seat(),
            Duration::from_secs(1320),
            "660 x 2 x 1: rotation is not engaged without a pool, which MagiConfig::timeout publishes as 22 minutes"
        );
    }

    /// The attempt cap and an exhausted general chain produce the SAME error for the same class.
    ///
    /// The break's "same exit path" was a comment, and the value it governs is consequential:
    /// `ProviderError::Http`'s status drives lineage condemnation, so a divergence here would
    /// change when a run aborts. Two reviewers flagged it as unverified; this is the verification.
    #[tokio::test]
    async fn the_attempt_cap_and_an_exhausted_chain_agree() {
        // Same class, same failure, two routes out: once through the per-class cap, once through
        // the general `attempt == max_retries` return.
        let via_cap = {
            let inner = Arc::new(FailingProvider::new(ProviderError::Timeout {
                message: "hang".into(),
            }));
            let p = RetryProvider::with_config(
                inner,
                RetryConfig {
                    base_delay: Duration::ZERO,
                    max_retries: 5,
                    limited_max_retries: 1, // the cap is what ends it
                    ..Default::default()
                },
            );
            p.complete("s", "u", &CompletionConfig::default())
                .await
                .unwrap_err()
        };
        let via_exhaustion = {
            let inner = Arc::new(FailingProvider::new(ProviderError::Timeout {
                message: "hang".into(),
            }));
            let p = RetryProvider::with_config(
                inner,
                RetryConfig {
                    base_delay: Duration::ZERO,
                    max_retries: 1,
                    limited_max_retries: 5, // the general count is what ends it
                    ..Default::default()
                },
            );
            p.complete("s", "u", &CompletionConfig::default())
                .await
                .unwrap_err()
        };
        assert_eq!(
            classify(&via_cap),
            classify(&via_exhaustion),
            "the two exits must be indistinguishable to a consumer: {via_cap} vs {via_exhaustion}"
        );
        assert!(
            matches!(via_cap, ProviderError::Timeout { .. }),
            "both surface the last raw error, not a synthesised one: {via_cap}"
        );
    }

    /// The floor guard is silent for a LOWERED client timeout too, and the message says why.
    ///
    /// The raised direction is pinned by `the_floor_guard_cannot_see_a_raised_client_timeout`;
    /// this is its mirror. With `ct = 60` the real floor is 62, so the shipped 450 is comfortably
    /// above it and warning would be wrong — the guard cannot see either direction, which is the
    /// premise, and the message now states the assumption rather than implying a verdict.
    #[test]
    fn the_floor_guard_is_also_blind_to_a_lowered_client_timeout() {
        let c = RetryConfig::default();
        let real_floor = budget_window(Duration::from_secs(60), c.base_delay).start;
        assert!(c.operation_budget > real_floor);
        assert!(
            !c.dangerous_settings()
                .iter()
                .any(|w| w.contains("second attempt")),
            "correct for ct = 60 as well; the guard reports only what the shipped default implies"
        );
    }

    /// The two exits agree even when a consumer puts `Http` in the limited list.
    ///
    /// The inline comment said the attempt cap and the `Retry-After` abandonment "cannot
    /// interact" — true of the SHIPPED class list, and `limited_retry_classes` is configurable.
    /// A consumer who adds `Http` makes them meet, so the agreement is asserted rather than
    /// reasoned about, on the value that governs lineage condemnation.
    #[tokio::test]
    async fn the_two_exits_agree_for_a_configured_http_class() {
        let make = |limited: u32, general: u32| {
            let inner = Arc::new(FailingProvider::new(ProviderError::Http {
                status: 503,
                body: "err".into(),
                retry_after_raw: vec![],
                received_at: None,
            }));
            RetryProvider::with_config(
                inner,
                RetryConfig {
                    base_delay: Duration::ZERO,
                    max_retries: general,
                    limited_max_retries: limited,
                    // The consumer's choice, not the shipped one.
                    limited_retry_classes: vec![RetryClass::Http],
                    ..Default::default()
                },
            )
        };
        let via_cap = make(1, 5)
            .complete("s", "u", &CompletionConfig::default())
            .await
            .unwrap_err();
        let via_exhaustion = make(5, 1)
            .complete("s", "u", &CompletionConfig::default())
            .await
            .unwrap_err();
        assert_eq!(
            classify(&via_cap),
            classify(&via_exhaustion),
            "both exits must surface the same class: {via_cap} vs {via_exhaustion}"
        );
        assert!(
            matches!(
                (&via_cap, &via_exhaustion),
                (ProviderError::Http { .. }, ProviderError::Http { .. })
            ),
            "and the same shape, since Http.status drives lineage condemnation"
        );
    }
}
