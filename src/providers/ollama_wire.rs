// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-20

//! The native Ollama wire format (`POST {base}/api/chat`).
//!
//! This module is private on purpose: nothing here is part of the public
//! contract. The crate's public vocabulary is `max_tokens` /
//! [`crate::provider::ReasoningControl`], never the wire's `num_predict` /
//! `think` — that translation happens entirely inside this file (T-5.3), and
//! nothing typed in terms of it escapes to a caller outside `providers/`.
//!
//! Four things live here:
//!
//! - [`is_crate_defect`] — the three-signal footprint that tells a genuine
//!   empty completion apart from a malformed request of ours.
//! - The **request** shapes ([`NativeRequest`], [`NativeOptions`],
//!   [`NativeMessage`]) — what this crate sends.
//! - The **success response** shapes ([`NativeResponse`], [`NativeRespMessage`])
//!   and [`NativeResponse::into_completion`] — what a `200` carries and how it
//!   becomes a [`Completion`] or a typed failure.
//! - [`NativeError`] — the one non-success body shape this crate has captured
//!   (a model-not-found `404`).
//!
//! # Why `is_crate_defect` lives here and not in a shared module
//!
//! Its discriminant — `eval_count` / `prompt_eval_count` **absent** together
//! with `done_reason: "load"` — is native vocabulary. The OpenAI-compatible
//! wire has no equivalent fields, so the footprint is only ever observable on
//! this path; a shared module would be an abstraction for a second consumer
//! that does not exist (KISS).

use serde::{Deserialize, Serialize};

use crate::error::ProviderError;
use crate::provider::{
    Completion, CompletionTelemetry, FinishReason, ReasoningControl, ReasoningState,
};

/// Whether this response is the fingerprint of a **defect in this crate**,
/// never of the model.
///
/// The trigger is the **full footprint, all three signals together**: the
/// token counters `eval_count` and `prompt_eval_count` are **absent** (not
/// zero — see below), `done_reason` is [`FinishReason::Load`], and `content`
/// is empty (after trimming whitespace). Any one signal alone is a far more
/// likely ordinary failure, and the consequence of a positive result here is
/// an **abort** of the whole run
/// ([`ProviderError::NoGeneration`](crate::error::ProviderError::NoGeneration)),
/// so the trigger stays narrow on purpose: every case that does not match this
/// exact fingerprint takes the reversible, mage-local
/// [`EmptyCompletion`](crate::error::ProviderError::EmptyCompletion) route
/// instead.
///
/// # Parameters
///
/// * `eval_count` — completion-token counter the backend reported, if any.
/// * `prompt_eval_count` — prompt-token counter the backend reported, if any.
/// * `done` — the termination reason the backend reported, if any.
/// * `content` — the completion text, possibly empty.
///
/// # Returns
///
/// `true` only when all three signals hold at once.
///
/// # Why `None` is the discriminant, never zero
///
/// A backend that starts sending `eval_count: 0` instead of omitting the
/// field makes the footprint stop matching, and the case falls back to
/// `EmptyCompletion` — mage-local, reversible. The protection is lost;
/// nothing breaks. That is the right direction to fail, and it is why this
/// function takes `Option<u32>` rather than treating an absent counter and a
/// zero counter as the same thing.
///
/// # Known false positive: a cold start
///
/// `done_reason: "load"` literally means the model was loading, so a
/// transient cold start is the most plausible way to see this footprint
/// without anyone having written a malformed request. A cold start **was**
/// captured (`native-cold-start.json`, 14.5 s of `load_duration`) and does
/// **not** match this footprint: the daemon answered `stop`, with content and
/// with both counters present. That is one observation, not a proof that the
/// footprint can never fire on a cold start under different conditions — it
/// is evidence, not exclusivity.
///
/// # Complexity
///
/// `O(1)` beyond the cost of `content.trim()`, which is `O(n)` in the length
/// of `content` and touches no allocation.
pub(crate) fn is_crate_defect(
    eval_count: Option<u32>,
    prompt_eval_count: Option<u32>,
    done: Option<&FinishReason>,
    content: &str,
) -> bool {
    eval_count.is_none()
        && prompt_eval_count.is_none()
        && matches!(done, Some(FinishReason::Load))
        && content.trim().is_empty()
}

/// One message on the native wire — `{"role": ..., "content": ...}`.
///
/// Used for the **request** side. The response side has its own shape,
/// [`NativeRespMessage`], because it carries different optional fields
/// (`thinking`) and is deserialized rather than serialized.
#[derive(Debug, Serialize)]
pub(crate) struct NativeMessage {
    pub(crate) role: String,
    pub(crate) content: String,
}

impl NativeMessage {
    /// Builds a `system` message.
    ///
    /// # Parameters
    ///
    /// * `content` — the system prompt text.
    pub(crate) fn system(content: &str) -> Self {
        Self {
            role: "system".to_string(),
            content: content.to_string(),
        }
    }

    /// Builds a `user` message.
    ///
    /// # Parameters
    ///
    /// * `content` — the user prompt text.
    pub(crate) fn user(content: &str) -> Self {
        Self {
            role: "user".to_string(),
            content: content.to_string(),
        }
    }
}

/// The `options` object on a native request — where sampling parameters live.
///
/// Wire vocabulary (`num_predict`), not the crate's public one (`max_tokens`):
/// this is the translation boundary itself, so the wire name is exactly
/// right here.
#[derive(Debug, Serialize)]
pub(crate) struct NativeOptions {
    pub(crate) num_predict: u32,
    pub(crate) temperature: f64,
}

/// A request to `POST {base}/api/chat`.
///
/// # Fields worth reading twice
///
/// * `stream` is **always explicit**, never omitted: `/api/chat` streams by
///   **default** when the field is absent, and this crate reads a single
///   buffered body (`read_verdict_body`), not a stream.
/// * `think` is only present on the wire when the caller asked for
///   [`ReasoningControl::Disabled`]. [`ReasoningControl::Default`] means "say
///   nothing", never "say `true`" — a crate that started asserting a control
///   nobody asked for would change behaviour for every existing consumer who
///   never mentioned reasoning. `#[serde(skip_serializing_if = "Option::is_none")]`
///   is what keeps the field off the wire in that case.
#[derive(Debug, Serialize)]
pub(crate) struct NativeRequest {
    pub(crate) model: String,
    pub(crate) messages: Vec<NativeMessage>,
    /// MUST be explicit: `/api/chat` streams by DEFAULT when the field is absent.
    pub(crate) stream: bool,
    pub(crate) options: NativeOptions,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) think: Option<bool>,
}

impl NativeRequest {
    /// Builds a native chat request.
    ///
    /// # Parameters
    ///
    /// * `model` — the model tag, passed through unchanged.
    /// * `messages` — the conversation, in order.
    /// * `max_tokens` — this crate's public budget, translated to
    ///   `options.num_predict` at this boundary.
    /// * `temperature` — sampling temperature, translated to
    ///   `options.temperature`.
    /// * `reasoning` — whether to ask the backend to skip its reasoning
    ///   channel. Only [`ReasoningControl::Disabled`] puts a field on the
    ///   wire; [`ReasoningControl::Default`] puts nothing.
    ///
    /// # Returns
    ///
    /// A request with `stream: false` set explicitly.
    pub(crate) fn new(
        model: &str,
        messages: Vec<NativeMessage>,
        max_tokens: u32,
        temperature: f64,
        reasoning: ReasoningControl,
    ) -> Self {
        Self {
            model: model.to_string(),
            messages,
            // NEVER omitted: /api/chat streams by default when absent.
            stream: false,
            options: NativeOptions {
                num_predict: max_tokens,
                temperature,
            },
            // `Default` says NOTHING on the wire; only `Disabled` does.
            think: matches!(reasoning, ReasoningControl::Disabled).then_some(false),
        }
    }
}

/// The `message` object on a **native success response**.
///
/// `#[serde(default)]` on every field: a backend that omits one must not fail
/// the parse (A-3). `thinking` in particular is **not always present** —
/// verified against the captured corpus: it appears only when there was
/// reasoning to report (`native-N2.json`), and is entirely absent when there
/// was none (`native-N1.json`, whose keys are exactly `{content, role}`).
/// Assuming it is always there fails on that fixture.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct NativeRespMessage {
    #[serde(default)]
    pub(crate) content: Option<String>,
    #[serde(default)]
    pub(crate) thinking: Option<String>,
}

/// A response body from `POST {base}/api/chat`, success shape.
///
/// # `done_reason` is not closed
///
/// The captured corpus observed three values (`stop`, `length`, `load`), and
/// `load` appeared in no documentation the project had before capture — the
/// space is open. [`FinishReason`] is `#[non_exhaustive]` with an `Other`
/// fallback for exactly this reason, so a fourth value deserializes instead
/// of failing the parse.
///
/// # Token counters are `Option`, because D-8 saw them **absent**
///
/// Not zero — genuinely missing from the body. That absence, together with
/// `done_reason: "load"` and empty content, is the whole discriminant of
/// [`is_crate_defect`].
#[derive(Debug, Deserialize)]
pub(crate) struct NativeResponse {
    #[serde(default)]
    pub(crate) message: NativeRespMessage,
    #[serde(default)]
    pub(crate) done_reason: Option<FinishReason>,
    #[serde(default)]
    pub(crate) eval_count: Option<u32>,
    #[serde(default)]
    pub(crate) prompt_eval_count: Option<u32>,
}

impl NativeResponse {
    /// Converts a parsed native response into a [`Completion`] or a typed
    /// failure.
    ///
    /// # Parameters
    ///
    /// * `cap` — the `max_tokens` budget that was in force for this request,
    ///   carried into [`ProviderError::EmptyCompletion`] so its `Display`
    ///   names the cap that produced an empty answer.
    /// * `trace` — whether the caller opted into carrying the reasoning
    ///   trace's **text**, not just its length
    ///   ([`CompletionConfig::reasoning_trace`](crate::provider::CompletionConfig::reasoning_trace)).
    ///
    /// # Returns
    ///
    /// `Ok(Completion)` when `content` is non-empty, with whatever telemetry
    /// the backend reported attached.
    ///
    /// # Errors
    ///
    /// * [`ProviderError::NoGeneration`] when `content` is empty **and**
    ///   [`is_crate_defect`] matches the full three-signal footprint — this
    ///   is checked **first**, because it is the strictly narrower case;
    ///   checking `EmptyCompletion` first would make this branch
    ///   unreachable.
    /// * [`ProviderError::EmptyCompletion`] when `content` is empty and the
    ///   footprint does not match — the safe, mage-local, reversible default
    ///   for "the model said nothing."
    ///
    /// # Complexity
    ///
    /// `O(n)` in the length of `content` plus the reasoning trace, when
    /// present — a single pass over each to trim and to count characters, no
    /// re-scanning of the parsed body.
    pub(crate) fn into_completion(
        self,
        cap: u32,
        trace: bool,
    ) -> Result<Completion, ProviderError> {
        let NativeResponse {
            message,
            done_reason,
            eval_count,
            prompt_eval_count,
        } = self;
        let NativeRespMessage { content, thinking } = message;
        let content = content.unwrap_or_default();

        if content.trim().is_empty() {
            // ORDER MATTERS: the full fingerprint is checked FIRST, because it
            // is the strictly narrower case. Checking `EmptyCompletion` first
            // would make the crate-defect branch unreachable.
            if is_crate_defect(
                eval_count,
                prompt_eval_count,
                done_reason.as_ref(),
                &content,
            ) {
                return Err(ProviderError::NoGeneration { done_reason });
            }
            return Err(ProviderError::EmptyCompletion {
                finish: done_reason,
                cap,
            });
        }

        let reasoning = match thinking {
            Some(s) => ReasoningState::Measured {
                chars: s.chars().count(),
                text: trace.then_some(s),
            },
            None => ReasoningState::NotMeasured,
        };
        let mut telemetry = CompletionTelemetry::unmeasured().with_reasoning(reasoning);
        if let Some(f) = done_reason {
            telemetry = telemetry.with_finish(f);
        }
        if let Some(n) = eval_count {
            telemetry = telemetry.with_completion_tokens(n);
        }
        if let Some(n) = prompt_eval_count {
            telemetry = telemetry.with_prompt_tokens(n);
        }
        Ok(Completion::new(content).with_telemetry(telemetry))
    }
}

/// The one native non-success body shape this crate has captured: a
/// model-not-found error, returned with HTTP `404` and a single `error` key.
///
/// # This does not build a [`ProviderError`] itself
///
/// A real HTTP status (`404` included) maps to
/// [`ProviderError::Http`](crate::error::ProviderError::Http) — after
/// `PARSE_FAILURE_STATUS` is removed, `Http` transports only real statuses,
/// and a `404` is one. That mapping needs the response's status code and
/// headers, which this type deliberately does not hold: it exists only to
/// give the caller a typed read of the body, once the caller already knows
/// the response was not a success.
///
/// # The `error` message is not an identifier
///
/// Verified against the captured corpus: asking for a `:cloud`-suffixed tag
/// (e.g. `some-model:cloud`) reports the name **without** the suffix. The
/// name this type carries is not necessarily the one that was requested, so
/// it must not be used to decide *which* candidate failed — the model comes
/// from the request that was sent, not from this body.
#[derive(Debug, Deserialize)]
pub(crate) struct NativeError {
    pub(crate) error: String,
}

/// Maps a **non-success** native HTTP status and its already-read body into a
/// [`ProviderError`].
///
/// # Parameters
///
/// * `status` — the HTTP status code, already known to be outside `200..300`
///   by the caller.
/// * `body` — the response body, already read through the same
///   `read_verdict_body` cap every other body on this path goes through, so
///   this function does no I/O of its own.
///
/// # Returns
///
/// `Auth` on `401`/`403`, mirroring the OpenAI-compatible path's
/// `map_status_to_error` for the same statuses. Every other status becomes
/// `Http { status, .. }` — after `PARSE_FAILURE_STATUS` is removed, `Http`
/// carries only real statuses, and this one is real (B-3). The body attached
/// prefers the single `error` key from [`NativeError`] when the body parses
/// as one — the shape captured for a model-not-found `404` — and falls back
/// to the raw text otherwise, so a body this crate cannot parse still reaches
/// the caller instead of being silently dropped.
///
/// # What this function does NOT do
///
/// It does not carry `Retry-After` or a receipt timestamp: the caller reads
/// the body through `read_verdict_body`, which consumes the response and
/// leaves neither available at the call site. Ollama's native API is not
/// documented to send `Retry-After`, so this is not a currently-known gap —
/// but a caller that later needs it must capture both **before** reading the
/// body, the same way the OpenAI-compatible path does.
pub(crate) fn native_error(status: u16, body: &str) -> ProviderError {
    match status {
        401 | 403 => ProviderError::Auth {
            message: body.to_string(),
        },
        _ => {
            let message = serde_json::from_str::<NativeError>(body)
                .map(|e| e.error)
                .unwrap_or_else(|_| body.to_string());
            ProviderError::Http {
                status,
                body: message,
                retry_after_raw: vec![],
                received_at: None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIX_N1: &str = include_str!("../../tests/fixtures/ec/native-N1.json");
    const FIX_N2: &str = include_str!("../../tests/fixtures/ec/native-N2.json");
    const FIX_LOAD: &str = include_str!("../../tests/fixtures/ec/native-E-malformed.json");
    const FIX_E_404: &str = include_str!("../../tests/fixtures/ec/native-E-model-not-found.json");

    /// Builds a minimal native success body carrying only `done_reason`, empty
    /// content and no counters — enough to exercise `done_reason` parsing in
    /// isolation without depending on a captured fixture for a value the
    /// corpus does not contain (e.g. a made-up fourth `done_reason`).
    fn done_reason_body(done_reason: &str) -> String {
        serde_json::json!({
            "model": "m",
            "message": { "role": "assistant", "content": "" },
            "done": true,
            "done_reason": done_reason,
        })
        .to_string()
    }

    /// Builds a native success body with empty content but the token counters
    /// PRESENT — the safe direction: without the fixture, `is_crate_defect`
    /// must not fire because the counters are not absent.
    fn native_empty_with_counters(eval_count: u32, prompt_eval_count: u32) -> String {
        serde_json::json!({
            "model": "m",
            "message": { "role": "assistant", "content": "" },
            "done": true,
            "done_reason": "length",
            "eval_count": eval_count,
            "prompt_eval_count": prompt_eval_count,
        })
        .to_string()
    }

    // -- Task 3c: is_crate_defect -------------------------------------------

    #[test]
    fn the_trigger_is_the_full_footprint_not_a_single_symptom() {
        // Counters ABSENT (not zero) AND done_reason "load" AND empty content. Any
        // one of them alone is a much likelier ordinary failure, and the
        // consequence here is an ABORT — so the trigger is narrowed to the exact
        // footprint that was measured, and everything else takes the reversible
        // mage-local route.
        //
        // ABSENT (`None`) is the discriminant, NOT zero: a backend that starts
        // sending `eval_count: 0` instead of omitting it makes the footprint stop
        // matching, and the case falls back to `EmptyCompletion`. The protection
        // is lost; nothing breaks. That is the right direction to fail.
        assert!(is_crate_defect(None, None, Some(&FinishReason::Load), ""));
        assert!(!is_crate_defect(
            Some(0),
            Some(0),
            Some(&FinishReason::Load),
            ""
        ));

        // HALF the counters is not the footprint either, and these two cases are the only
        // thing that pins the counter conditions INDEPENDENTLY. With `Some(0), Some(0)` as
        // the only counters-present case, each condition is covered by its sibling: dropping
        // either one on its own left all eighteen tests green, verified by mutation. The
        // measured shape is BOTH absent; anything else falls to `EmptyCompletion`, which is
        // mage-local and reversible — the safe direction for a trigger that aborts the run.
        assert!(!is_crate_defect(
            Some(7),
            None,
            Some(&FinishReason::Load),
            ""
        ));
        assert!(!is_crate_defect(
            None,
            Some(11),
            Some(&FinishReason::Load),
            ""
        ));
        assert!(!is_crate_defect(None, None, Some(&FinishReason::Stop), ""));
        assert!(!is_crate_defect(
            None,
            None,
            Some(&FinishReason::Load),
            "some text"
        ));
    }

    #[test]
    fn a_measured_cold_start_does_not_match_the_footprint() {
        // `done_reason: "load"` literally means the model was loading, so a cold
        // start is the most plausible false positive for a trigger that ABORTS the
        // run. It was captured rather than assumed (`native-cold-start.json`,
        // 14.5 s of `load_duration`): the daemon answered `stop`, with content and
        // with counters PRESENT.
        assert!(!is_crate_defect(
            Some(17),
            Some(82),
            Some(&FinishReason::Stop),
            "a verdict"
        ));
    }

    #[test]
    fn an_unload_response_satisfies_two_of_the_three_signals_and_is_not_a_defect() {
        // `native-unload-empty-messages.json`: counters absent AND empty content,
        // but `done_reason: "unload"`. This is what a caller passing
        // `keep_alive: 0` gets. With a two-signal footprint it would have been
        // read as a defect of THIS crate and would have aborted the run, so the
        // third condition is load-bearing, not belt-and-braces.
        assert!(!is_crate_defect(
            None,
            None,
            Some(&FinishReason::Other("unload".to_string())),
            ""
        ));
    }

    #[test]
    fn an_absent_done_reason_is_not_enough_either() {
        // Nothing observed about termination cannot support a claim that WE are at
        // fault. Same reason `ReasoningState::NotMeasured` exists.
        assert!(!is_crate_defect(None, None, None, ""));
    }

    // -- Task 6: the request types -------------------------------------------

    #[test]
    fn the_native_request_serialises_the_four_fields_the_endpoint_needs() {
        let req = NativeRequest::new(
            "m",
            vec![NativeMessage::user("hi")],
            16_384,
            0.0,
            ReasoningControl::Disabled,
        );
        let v: serde_json::Value = serde_json::to_value(&req).unwrap();
        assert_eq!(
            v["stream"], false,
            "an omitted stream field defaults to TRUE on /api/chat"
        );
        assert_eq!(v["options"]["num_predict"], 16_384);
        assert_eq!(v["options"]["temperature"], 0.0);
        assert_eq!(v["think"], false);
    }

    #[test]
    fn the_default_control_omits_think_entirely() {
        let req = NativeRequest::new("m", vec![], 4, 0.0, ReasoningControl::Default);
        let v: serde_json::Value = serde_json::to_value(&req).unwrap();
        assert!(
            v.get("think").is_none(),
            "Default means say nothing, not say true"
        );
    }

    // -- Task 7: the success response ----------------------------------------

    #[test]
    fn thinking_is_absent_in_n1_and_present_in_n2_and_both_parse() {
        // VERIFIED against the captured corpus: message.thinking appears ONLY when
        // there was reasoning to report. Assuming it is always there fails on N1,
        // whose keys are exactly {content, role}.
        let n1: NativeResponse = serde_json::from_str(FIX_N1).unwrap();
        assert!(n1.message.thinking.is_none());
        let n2: NativeResponse = serde_json::from_str(FIX_N2).unwrap();
        assert!(
            n2.message
                .thinking
                .as_deref()
                .is_some_and(|t| !t.is_empty())
        );
    }

    #[test]
    fn the_three_observed_done_reasons_all_deserialise() {
        for (raw, want) in [
            ("stop", FinishReason::Stop),
            ("length", FinishReason::Length),
            ("load", FinishReason::Load),
        ] {
            let r: NativeResponse = serde_json::from_str(&done_reason_body(raw)).unwrap();
            assert_eq!(r.done_reason, Some(want));
        }
        // A fourth value must NOT break the parse: the space is open (D-8 found
        // `load`, which no documentation the project had mentioned).
        let r: NativeResponse = serde_json::from_str(&done_reason_body("brand_new")).unwrap();
        assert_eq!(r.done_reason, Some(FinishReason::Other("brand_new".into())));
    }

    #[test]
    fn into_completion_carries_the_telemetry_of_a_real_success() {
        // CORRECTED against the fixture, not the plan: `native-N2.json` is a
        // TRUNCATED success (empty content, done_reason "length"), so it cannot
        // exercise the non-empty-content branch — asserting `!c.text.is_empty()`
        // against it would never pass. `native-N1.json` is the real, complete
        // success (done_reason "stop", content present, no `thinking`), so it is
        // this fixture, not N2, that exercises the telemetry-carrying branch of
        // `into_completion`.
        let n1: NativeResponse = serde_json::from_str(FIX_N1).unwrap();
        let c = n1.into_completion(16_384, false).unwrap();
        assert!(!c.text.is_empty());
        assert_eq!(c.telemetry.finish, Some(FinishReason::Stop));
        assert_eq!(c.telemetry.completion_tokens, Some(6800));
        assert_eq!(c.telemetry.prompt_tokens, Some(63_924));
        // N1 carries no `thinking` field at all, so nothing was measured — NOT
        // `Measured { chars: 0 }`, which would assert a look that never happened.
        assert_eq!(c.telemetry.reasoning, ReasoningState::NotMeasured);
    }

    #[test]
    fn into_completion_carries_a_measured_reasoning_trace_when_present() {
        // No captured fixture pairs non-empty content with a present `thinking`
        // field (N1 has content and no thinking; N2 has thinking and no content
        // — see the correction above and `tests/fixtures/ec/mod.rs`'s own note
        // that an untested combination belongs in a JSON literal here, not a
        // fabricated "capture"). This tests OUR deserialiser's contract for that
        // combination, not a claim about backend behaviour.
        let body = serde_json::json!({
            "model": "m",
            "message": {
                "role": "assistant",
                "content": "a verdict",
                "thinking": "reasoning text",
            },
            "done": true,
            "done_reason": "stop",
            "eval_count": 10,
            "prompt_eval_count": 20,
        })
        .to_string();

        let without_trace: NativeResponse = serde_json::from_str(&body).unwrap();
        let c = without_trace.into_completion(16_384, false).unwrap();
        assert!(
            matches!(c.telemetry.reasoning, ReasoningState::Measured { chars, text: None } if chars == "reasoning text".chars().count())
        );

        let with_trace: NativeResponse = serde_json::from_str(&body).unwrap();
        let c = with_trace.into_completion(16_384, true).unwrap();
        assert!(matches!(
            c.telemetry.reasoning,
            ReasoningState::Measured { text: Some(t), .. } if t == "reasoning text"
        ));
    }

    #[test]
    fn a_native_empty_without_the_footprint_falls_back_to_empty_completion() {
        // The safe direction, and it needs its own test: content is empty but the
        // counters are PRESENT, so it is not our defect — it is the model saying
        // nothing. Mage-local and reversible, never an abort.
        let r: NativeResponse = serde_json::from_str(&native_empty_with_counters(0, 512)).unwrap();
        match r.into_completion(16_384, false) {
            Err(ProviderError::EmptyCompletion { cap, .. }) => assert_eq!(cap, 16_384),
            other => panic!("expected EmptyCompletion, got {other:?}"),
        }
    }

    #[test]
    fn into_completion_maps_an_empty_content_to_the_typed_error_not_to_an_empty_string() {
        // If it returned `Ok("")` the whole eje B would be unreachable: the empty
        // completion has to become `NoGeneration` HERE or never, since this
        // fixture is the fingerprint's positive case.
        let load: NativeResponse = serde_json::from_str(FIX_LOAD).unwrap();
        assert!(matches!(
            load.into_completion(16_384, false),
            Err(ProviderError::NoGeneration { .. })
        ));
    }

    #[test]
    fn the_token_counters_are_optional_because_d8_saw_them_absent() {
        // Absent, not zero — and that difference is the whole discriminant of the
        // family-3 footprint in is_crate_defect.
        let r: NativeResponse = serde_json::from_str(FIX_LOAD).unwrap();
        assert!(r.eval_count.is_none());
        assert!(r.prompt_eval_count.is_none());
    }

    // -- Task 8: the two native error forms ----------------------------------

    #[test]
    fn a_missing_model_is_a_404_with_a_single_error_key() {
        // NOTHING like the OpenAI error shape. Mapping it through the compat parser
        // would have produced an opaque failure for a perfectly legible 404.
        let e: NativeError = serde_json::from_str(FIX_E_404).unwrap();
        assert!(e.error.contains("not found"));
    }

    #[test]
    fn native_error_maps_a_404_with_a_parseable_body_to_http_carrying_the_message() {
        match native_error(404, FIX_E_404) {
            ProviderError::Http { status, body, .. } => {
                assert_eq!(status, 404);
                assert!(body.contains("not found"));
            }
            other => panic!("expected Http, got {other:?}"),
        }
    }

    #[test]
    fn native_error_falls_back_to_the_raw_body_when_it_does_not_parse_as_native_error() {
        // A reverse proxy in front of Ollama can answer with an HTML error page, not
        // the {"error": "..."} shape. The raw text must still reach the caller.
        match native_error(502, "<html>bad gateway</html>") {
            ProviderError::Http { status, body, .. } => {
                assert_eq!(status, 502);
                assert_eq!(body, "<html>bad gateway</html>");
            }
            other => panic!("expected Http, got {other:?}"),
        }
    }

    #[test]
    fn native_error_maps_401_and_403_to_auth() {
        assert!(matches!(
            native_error(401, "unauthorized"),
            ProviderError::Auth { .. }
        ));
        assert!(matches!(
            native_error(403, "forbidden"),
            ProviderError::Auth { .. }
        ));
    }

    #[test]
    fn the_error_message_must_not_be_used_to_identify_the_candidate() {
        // Sanity check, not proof: THIS fixture's request was not a `:cloud`-tagged
        // model (its `error` names "this-model-does-not-exist" verbatim), so this
        // assertion is vacuously true against it. The suffix-stripping claim in the
        // rustdoc above comes from the base spec's evidence (D-8), captured outside
        // the corpus that shipped to `tests/fixtures/ec/` — not from this artifact.
        // The assertion is kept because the CONTRACT it encodes still holds
        // regardless: the model comes from the request that was sent, never from
        // this body.
        let e: NativeError = serde_json::from_str(FIX_E_404).unwrap();
        assert!(!e.error.contains(":cloud"));
    }
}
