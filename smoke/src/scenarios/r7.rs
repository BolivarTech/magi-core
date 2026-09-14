// Author: Julian Bolivar
// Version: 4.1.0
// Date: 2026-09-13

//! The two request-dialect scenarios: `S-R7a` and `S-R7b`.
//!
//! # The property, and why the obvious assertion cannot fail
//!
//! `OpenAiCompatibleProvider` sends its output cap under one of two spellings: `max_tokens`,
//! the default, or `max_completion_tokens`, the one OpenAI documents for its current models.
//! Against Ollama's `/v1` only the first is honoured; the second is **discarded in silence** —
//! measured: 692 tokens generated against 16 requested, `finish_reason: stop`, no error. The
//! default rests on that measurement.
//!
//! So a scenario that sent the modern spelling and asserted *"the cap was respected"* could
//! not fail: it would pass when the crate is right AND when the backend ignores the request.
//! An assertion that depends on the model rather than on the product is not an assertion.
//! What IS observable is the **contrast** between the two spellings under the same cap.
//!
//! # Two runs, one direct completion each
//!
//! Both runs send the small payload through one `OpenAiCompatibleProvider` with a cap low
//! enough that any answer exceeds it ([`DIALECT_PROBE_CAP`]), and differ in nothing but the
//! dialect. Neither runs a trio: the builder always dispatches three mages, a cap that cuts
//! every answer leaves no verdict for any of them, and `analyze()` then returns
//! `InsufficientAgents` with the completion telemetry gone. One direct `complete()` keeps the
//! crate's own reading of the answer, which is what both scenarios read — never the wire.
//!
//! # What each row is
//!
//! **`S-R7a` verifies.** Under the default dialect the completion must come back **cut at the
//! cap** — `finish: Length`, and no more completion tokens than the cap. That is the product's
//! property: the crate sent the spelling this backend honours. A completion that is not cut
//! is red.
//!
//! **`S-R7b` records.** Under the modern dialect the same request comes back **not cut**, and
//! that is a fact about the backend, not about the crate: a gate cannot go red for something
//! the product does not control. The row is an observation, with the numbers in it, and it
//! counts apart from the passes — the certificate declares ONE verified property and ONE
//! recorded observation, never two passes. It goes red on exactly one day: the day this
//! backend starts honouring the field. That is not a regression of the crate, it is the
//! measurement the default rests on changing under it, and it is the moment the default
//! deserves revisiting — so it is the moment to be told.
//!
//! # An honest red that is not a regression, and how to tell it apart
//!
//! `S-R7a` needs the model to want MORE than the cap. A model that answers shorter on its own
//! produces a red that is honest — the property is unknown, so it fails closed — but is not a
//! defect of the crate. When the verdict row is red the scenario adds the reading beside it:
//! the finish reason and the token counts. *Finish `stop` with fewer tokens than the cap*
//! means there was nothing to cut and the cap should be lowered before this is treated as a
//! finding; *finish `stop` with more tokens than the cap* means the cap never reached the
//! backend, which is the regression.
//!
//! # The seed comes by another path than the mechanism under test
//!
//! Nothing here touches the dialect from the outside: the run constructs the provider through
//! the public constructor with the dialect it names, and the scenario reads the telemetry the
//! crate produced. A mock backend respects the field by construction, which is why this lives
//! in the smoke harness and not in the crate's suite.

use crate::alias::magi_core::provider::FinishReason;
use crate::config::RunId;
use crate::runner::{
    assert_that, Assertion, BackendNeed, CompletionEvidence, RunContext, Scenario, Source,
    DIALECT_PROBE_CAP,
};

const NAME_DEFAULT_DIALECT_CUTS: &str =
    "the default dialect's cap reaches the backend: the completion comes back cut at the cap";
const NAME_DEFAULT_DIALECT_READING: &str =
    "what the backend reported for the capped completion, beside the verdict";
const NAME_MODERN_DIALECT_RECORDED: &str = "the modern dialect's cap against this backend is \
     recorded, not verified: a cut here means the backend now honours the field and the \
     default's justification has changed";

/// The skip reason for a run that produced neither a completion nor an error.
const NEVER_HAPPENED: &str = "the run never happened";

/// `S-R7a` — the dialect the backend honours cuts the completion at the cap.
///
/// One row when the property holds; two when it does not, the second carrying the reading
/// that tells an honest red from a regression. A run that reached no completion skips naming
/// the error; a run that never happened skips saying so.
fn s_r7a_default_dialect_is_cut_at_the_cap(ctx: &RunContext<'_>) -> Vec<Assertion> {
    let _ = ctx;
    Vec::new()
}

/// `S-R7b` — the modern dialect against this backend: recorded, never verified.
///
/// One row. `Observed`, with the reading, when the backend discarded the cap; `Fail` when it
/// honoured it — the measurement the default rests on has changed; a skip when the completion
/// could not be measured.
fn s_r7b_modern_dialect_is_recorded(ctx: &RunContext<'_>) -> Vec<Assertion> {
    let _ = ctx;
    Vec::new()
}

/// Whether the crate read the completion as cut by its output budget.
///
/// `Length` alone decides; the token count, when the backend counted it, must not exceed the
/// cap — a `Length` with more tokens than the cap would be a cut at some OTHER budget.
///
/// # Parameters
///
/// * `evidence` — what the run measured.
fn is_cut_at_the_cap(evidence: &CompletionEvidence) -> bool {
    let _ = evidence;
    false
}

/// The reading, rendered for a row: the cap, the finish reason and the token counts.
///
/// # Parameters
///
/// * `evidence` — what the run measured.
fn render_reading(evidence: &CompletionEvidence) -> String {
    let _ = evidence;
    String::new()
}

/// The finish reason as a word, or `unreported` when the backend gave none.
///
/// # Parameters
///
/// * `finish` — the crate's reading of why the model stopped.
fn finish_label(finish: Option<&FinishReason>) -> String {
    let _ = finish;
    String::new()
}

/// The two dialect scenarios, each over the run that exists for it.
pub fn r7_scenarios() -> Vec<Scenario> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alias::magi_core::provider::CompletionTelemetry;
    use crate::outcome::ScenarioState;
    use crate::runner::ErrorClass;

    /// Evidence with the probe cap, a finish reason and a completion count.
    fn evidence(
        finish: Option<FinishReason>,
        completion_tokens: Option<u32>,
    ) -> CompletionEvidence {
        let mut telemetry = CompletionTelemetry::unmeasured().with_prompt_tokens(2_811);
        if let Some(f) = finish {
            telemetry = telemetry.with_finish(f);
        }
        if let Some(n) = completion_tokens {
            telemetry = telemetry.with_completion_tokens(n);
        }
        CompletionEvidence {
            cap: DIALECT_PROBE_CAP,
            telemetry,
        }
    }

    /// The reading the honoured dialect produces on a backend that respects the cap.
    fn cut_at_the_cap() -> CompletionEvidence {
        evidence(Some(FinishReason::Length), Some(DIALECT_PROBE_CAP))
    }

    /// The reading the discarded dialect produces: the model ran to its own end, far past
    /// the cap.
    fn ran_to_the_end() -> CompletionEvidence {
        evidence(Some(FinishReason::Stop), Some(692))
    }

    /// Both scenarios are registered, each over its own run, and both need a backend.
    #[test]
    fn both_dialect_scenarios_are_registered() {
        let scenarios = r7_scenarios();
        assert_eq!(scenarios.len(), 2);
        assert_eq!(scenarios[0].id, "S-R7a");
        assert_eq!(scenarios[0].source, Source::Run(RunId::DialectMaxTokens));
        assert_eq!(scenarios[1].id, "S-R7b");
        assert_eq!(
            scenarios[1].source,
            Source::Run(RunId::DialectMaxCompletionTokens)
        );
        for s in &scenarios {
            assert_eq!(
                s.backend_tag,
                BackendNeed::Required,
                "{} reads a completion a backend produced",
                s.id
            );
        }
    }

    /// A context with no completion and no error SKIPS rather than passing — and the row
    /// counts are exact, so a scenario returning nothing cannot satisfy this vacuously.
    #[test]
    fn no_completion_skips_instead_of_passing() {
        let a =
            s_r7a_default_dialect_is_cut_at_the_cap(&RunContext::blank(RunId::DialectMaxTokens));
        assert_eq!(
            a.len(),
            1,
            "S-R7a has one row when there is nothing to read"
        );
        let b =
            s_r7b_modern_dialect_is_recorded(&RunContext::blank(RunId::DialectMaxCompletionTokens));
        assert_eq!(b.len(), 1, "S-R7b has one row");
        for row in a.into_iter().chain(b) {
            match &row.state {
                ScenarioState::Skip(reason) => assert_eq!(reason, NEVER_HAPPENED),
                other => panic!("{} must skip, got {other:?}", row.name),
            }
        }
    }

    /// A completion that failed before anything was measured skips NAMING the failure —
    /// never fails, since a transport or contract failure says nothing about the dialect.
    #[test]
    fn a_failed_completion_skips_naming_the_error() {
        let err = "network error: connection refused".to_string();
        for run in [RunId::DialectMaxTokens, RunId::DialectMaxCompletionTokens] {
            let ctx = RunContext {
                error: Some(&err),
                error_class: Some(ErrorClass::Environment),
                ..RunContext::blank(run)
            };
            let rows = if run == RunId::DialectMaxTokens {
                s_r7a_default_dialect_is_cut_at_the_cap(&ctx)
            } else {
                s_r7b_modern_dialect_is_recorded(&ctx)
            };
            assert_eq!(rows.len(), 1);
            match &rows[0].state {
                ScenarioState::Skip(reason) => assert!(
                    reason.contains("connection refused"),
                    "the skip must carry the error the completion failed with: {reason}"
                ),
                other => panic!("an unmeasured completion cannot be judged: {other:?}"),
            }
        }
    }

    /// `S-R7a` is green — one row, a pass — when the crate read the completion as cut at
    /// the cap.
    #[test]
    fn s_r7a_passes_on_a_completion_cut_at_the_cap() {
        let ev = cut_at_the_cap();
        let ctx = RunContext {
            completion: Some(&ev),
            ..RunContext::blank(RunId::DialectMaxTokens)
        };
        let rows = s_r7a_default_dialect_is_cut_at_the_cap(&ctx);
        assert_eq!(
            rows.len(),
            1,
            "a pass carries no reading beside it: {rows:?}"
        );
        assert_eq!(rows[0].name, NAME_DEFAULT_DIALECT_CUTS);
        assert_eq!(rows[0].state, ScenarioState::Pass, "{:?}", rows[0]);
    }

    /// A cut whose token count the backend did not report is still a cut: the finish reason
    /// decides, and the count only has to agree when it is there.
    #[test]
    fn s_r7a_passes_on_a_cut_whose_count_was_not_reported() {
        let ev = evidence(Some(FinishReason::Length), None);
        let ctx = RunContext {
            completion: Some(&ev),
            ..RunContext::blank(RunId::DialectMaxTokens)
        };
        let rows = s_r7a_default_dialect_is_cut_at_the_cap(&ctx);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].state, ScenarioState::Pass, "{:?}", rows[0]);
    }

    /// `S-R7a` goes RED on a completion that ran to its own end — the reading the crate
    /// produces when the cap never reached the backend — and adds the reading beside the
    /// verdict, so the red says WHICH kind of not-cut it was.
    ///
    /// This is the mutation proof for the dialect: send the discarded spelling where the
    /// honoured one belongs and this is the row that says so.
    #[test]
    fn s_r7a_fails_on_a_completion_that_was_not_cut_and_shows_the_reading() {
        let ev = ran_to_the_end();
        let ctx = RunContext {
            completion: Some(&ev),
            ..RunContext::blank(RunId::DialectMaxTokens)
        };
        let rows = s_r7a_default_dialect_is_cut_at_the_cap(&ctx);
        assert_eq!(rows.len(), 2, "a red carries its reading: {rows:?}");
        assert_eq!(rows[0].name, NAME_DEFAULT_DIALECT_CUTS);
        assert_eq!(rows[0].state, ScenarioState::Fail, "{:?}", rows[0]);
        assert_eq!(rows[1].name, NAME_DEFAULT_DIALECT_READING);
        match &rows[1].state {
            ScenarioState::Observed(reading) => {
                assert!(reading.contains("cap 16"), "{reading}");
                assert!(reading.contains("finish stop"), "{reading}");
                assert!(reading.contains("692 completion tokens"), "{reading}");
                assert!(reading.contains("2811 prompt tokens"), "{reading}");
            }
            other => panic!("the reading beside a red is an observation, got {other:?}"),
        }
    }

    /// The honest red: the model answered SHORTER than the cap on its own. Still red — the
    /// property is unknown, so it fails closed — but the reading beside it shows fewer tokens
    /// than the cap, which is what tells an operator to lower the cap rather than file a
    /// regression.
    #[test]
    fn s_r7a_honest_red_shows_fewer_tokens_than_the_cap() {
        let ev = evidence(Some(FinishReason::Stop), Some(9));
        let ctx = RunContext {
            completion: Some(&ev),
            ..RunContext::blank(RunId::DialectMaxTokens)
        };
        let rows = s_r7a_default_dialect_is_cut_at_the_cap(&ctx);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].state, ScenarioState::Fail);
        match &rows[1].state {
            ScenarioState::Observed(reading) => assert!(
                reading.contains("9 completion tokens") && reading.contains("cap 16"),
                "the count must sit beside the cap so the shortfall is visible: {reading}"
            ),
            other => panic!("expected the reading, got {other:?}"),
        }
    }

    /// A `Length` with MORE tokens than the cap is a cut at some other budget, not at ours:
    /// red, with the reading.
    #[test]
    fn s_r7a_fails_on_a_cut_past_the_cap() {
        let ev = evidence(Some(FinishReason::Length), Some(4_096));
        let ctx = RunContext {
            completion: Some(&ev),
            ..RunContext::blank(RunId::DialectMaxTokens)
        };
        let rows = s_r7a_default_dialect_is_cut_at_the_cap(&ctx);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].state, ScenarioState::Fail, "{:?}", rows[0]);
        assert!(matches!(rows[1].state, ScenarioState::Observed(_)));
    }

    /// A completion whose finish reason the backend never reported cannot be judged either
    /// way: skip, saying so — not a pass on a count alone, not a red on an absence.
    #[test]
    fn s_r7a_skips_when_the_backend_reported_no_finish_reason() {
        let ev = evidence(None, Some(DIALECT_PROBE_CAP));
        let ctx = RunContext {
            completion: Some(&ev),
            ..RunContext::blank(RunId::DialectMaxTokens)
        };
        let rows = s_r7a_default_dialect_is_cut_at_the_cap(&ctx);
        assert_eq!(rows.len(), 1);
        match &rows[0].state {
            ScenarioState::Skip(reason) => assert!(
                reason.contains("finish reason"),
                "the skip must say what could not be read: {reason}"
            ),
            other => panic!("an unreported finish cannot be judged: {other:?}"),
        }
    }

    /// `S-R7b` RECORDS a completion the backend ran to its own end: an observation with the
    /// numbers in it, never a pass — and the row says the cap was discarded.
    #[test]
    fn s_r7b_records_the_discarded_cap_as_an_observation_with_its_numbers() {
        let ev = ran_to_the_end();
        let ctx = RunContext {
            completion: Some(&ev),
            ..RunContext::blank(RunId::DialectMaxCompletionTokens)
        };
        let rows = s_r7b_modern_dialect_is_recorded(&ctx);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, NAME_MODERN_DIALECT_RECORDED);
        match &rows[0].state {
            ScenarioState::Observed(reading) => {
                assert!(reading.contains("discarded"), "{reading}");
                assert!(reading.contains("cap 16"), "{reading}");
                assert!(reading.contains("finish stop"), "{reading}");
                assert!(reading.contains("692 completion tokens"), "{reading}");
            }
            other => panic!("a backend property is recorded, never judged: {other:?}"),
        }
    }

    /// `S-R7b` goes RED the day the backend honours the field: the measurement the default
    /// rests on has changed, and that is the moment to be told.
    #[test]
    fn s_r7b_fails_the_day_the_backend_honours_the_field() {
        let ev = cut_at_the_cap();
        let ctx = RunContext {
            completion: Some(&ev),
            ..RunContext::blank(RunId::DialectMaxCompletionTokens)
        };
        let rows = s_r7b_modern_dialect_is_recorded(&ctx);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].state,
            ScenarioState::Fail,
            "a cut under the discarded spelling is news about the backend, and red is how \
             the news arrives: {:?}",
            rows[0]
        );
    }

    /// The reading renders every field the operator needs, and says when one is missing
    /// rather than inventing a number.
    #[test]
    fn the_reading_names_the_cap_the_finish_and_both_counts() {
        assert_eq!(
            render_reading(&cut_at_the_cap()),
            "cap 16: finish length, 16 completion tokens, 2811 prompt tokens"
        );
        let uncounted = CompletionEvidence {
            cap: DIALECT_PROBE_CAP,
            telemetry: CompletionTelemetry::unmeasured(),
        };
        assert_eq!(
            render_reading(&uncounted),
            "cap 16: finish unreported, completion tokens not counted, prompt tokens not counted"
        );
    }

    /// The cut predicate on its boundaries: `Length` at the cap and under it is a cut, over
    /// it is not, and no other finish reason is.
    #[test]
    fn the_cut_predicate_holds_only_for_length_within_the_cap() {
        assert!(is_cut_at_the_cap(&cut_at_the_cap()));
        assert!(is_cut_at_the_cap(&evidence(
            Some(FinishReason::Length),
            Some(DIALECT_PROBE_CAP - 1)
        )));
        assert!(is_cut_at_the_cap(&evidence(
            Some(FinishReason::Length),
            None
        )));
        assert!(!is_cut_at_the_cap(&evidence(
            Some(FinishReason::Length),
            Some(DIALECT_PROBE_CAP + 1)
        )));
        assert!(!is_cut_at_the_cap(&evidence(
            Some(FinishReason::Stop),
            Some(DIALECT_PROBE_CAP)
        )));
        assert!(!is_cut_at_the_cap(&evidence(None, Some(DIALECT_PROBE_CAP))));
    }

    /// Every finish reason renders as its wire word, and an absent one says so.
    #[test]
    fn the_finish_label_is_the_wire_word_or_unreported() {
        assert_eq!(finish_label(Some(&FinishReason::Stop)), "stop");
        assert_eq!(finish_label(Some(&FinishReason::Length)), "length");
        assert_eq!(finish_label(Some(&FinishReason::Load)), "load");
        assert_eq!(
            finish_label(Some(&FinishReason::Other("content_filter".into()))),
            "content_filter"
        );
        assert_eq!(finish_label(None), "unreported");
    }
}
