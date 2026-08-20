// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-20

//! The E2 scenarios: the ones that verify what `4.0.0` changed.
//!
//! They are written with the milestone that introduces the property, not before it, and not
//! after. Written before, they sit red for weeks and a permanently red harness gets rationalised
//! away — which is the `3.0.2` failure wearing different clothes. Written after, the property
//! ships uncertified. Written alongside, the scenario IS the test.

use crate::alias::magi_core::prelude::ReasoningState;
use crate::config::RunId;
use crate::proxy::RequestRecord;
use crate::runner::{
    assert_that, Assertion, BackendNeed, RunContext, Scenario, Source, COMPLETIONS_PATH,
};

/// The compatibility path the crate spoke until `4.0.0`. Named here rather than inlined so the
/// assertion below reads as "this specific endpoint is gone", not as "some string is absent".
const LEGACY_COMPAT_PATH: &str = "/v1/chat/completions";

const NAME_NATIVE_ONLY: &str = "every completion request goes to the native endpoint";
const NAME_NO_LEGACY: &str = "no request reaches the OpenAI-compatible completions path";

/// `S8` — the native endpoint is the ONLY completions path, and there is no way back to `/v1`.
///
/// # Why this needs a live backend rather than a mock
///
/// A unit test with a mock server proves the provider *builds* the right request. This proves
/// the whole configured stack — builder, provider, rotation, retry — never produces a request
/// on the old path, against a backend that would happily serve both. Ollama answers on `/v1`
/// too, so a conditional route left in by accident would work perfectly and be invisible
/// everywhere except here.
fn s8_completions_are_native_only(ctx: &RunContext<'_>) -> Vec<Assertion> {
    if ctx.proxy_degraded {
        let why = "the proxy registry degraded during this run; a partial record could fail an \
                   assertion the crate satisfied perfectly";
        return vec![
            Assertion::skip(NAME_NATIVE_ONLY, why),
            Assertion::skip(NAME_NO_LEGACY, why),
        ];
    }

    let completions: Vec<&RequestRecord> = ctx
        .records
        .iter()
        .filter(|r| r.path == COMPLETIONS_PATH)
        .collect();

    // The non-empty guard is the whole scenario, not a formality. "No request used the legacy
    // path" is vacuously true over an empty record set, so without this a run whose traffic
    // never reached the proxy at all would certify the property it never observed. This harness
    // exists because a green that means "nothing was looked at" already cost this project a
    // release.
    let native_only = assert_that(NAME_NATIVE_ONLY, !completions.is_empty());

    let no_legacy = assert_that(
        NAME_NO_LEGACY,
        !ctx.records.iter().any(|r| r.path == LEGACY_COMPAT_PATH),
    );

    vec![native_only, no_legacy]
}

// ---------------------------------------------------------------------------
// S13 — the telemetry records EVERY completion
// ---------------------------------------------------------------------------

const NAME_ONE_PER_SEAT: &str = "every seat that answered left a completion record";
const NAME_MODEL_AND_CAP: &str = "every record names its model and the budget it ran under";
const NAME_TERMINATION: &str = "every record carries the termination reason the backend reported";
const NAME_TRACE_LENGTH_ONLY: &str = "with the flag off, no record carries the trace text";

/// `S13` — a clean run still records one entry per completion
/// (`sbtdd/smoke-harness-spec.md`, "S13").
///
/// # Why a CLEAN run is the interesting case, not a cut one
///
/// Recording only the attempts that were cut leaves a consumer blind until the first cut, which
/// is the blindness this release exists to end: the old 4096-token default did not fail all at
/// once, it had been scraping by for a while. A unit test can prove the field is populated; only
/// a real backend proves the numbers in it came from a real response rather than from a mock that
/// was told what to say.
///
/// # What it deliberately does NOT assert
///
/// It does not check the token counters for a specific value, nor that they are present at all.
/// This run uses a MIXED trio on purpose, and a compatible backend that omits `usage` is a
/// legitimate deployment — the crate's contract is that an absent counter is reported as absent,
/// never as a zero, and a scenario demanding presence would be asserting a property of the
/// backend rather than of the crate.
fn s13_every_completion_is_recorded(ctx: &RunContext<'_>) -> Vec<Assertion> {
    let Some(report) = ctx.report else {
        let reason = ctx
            .error
            .map(str::to_string)
            .unwrap_or_else(|| "the run never happened".to_string());
        return vec![
            Assertion::skip(NAME_ONE_PER_SEAT, reason.clone()),
            Assertion::skip(NAME_MODEL_AND_CAP, reason.clone()),
            Assertion::skip(NAME_TERMINATION, reason.clone()),
            Assertion::skip(NAME_TRACE_LENGTH_ONLY, reason),
        ];
    };

    let records: Vec<_> = report.completions.values().flatten().collect();

    // The non-empty half is the scenario, not a formality: every assertion below quantifies over
    // this set, and all of them are vacuously true over an empty one. A report in hand with no
    // records at all is exactly the regression this scenario exists to catch, and a green that
    // means "nothing was looked at" has already cost this project a release.
    let one_per_seat = assert_that(
        NAME_ONE_PER_SEAT,
        !records.is_empty() && report.completions.len() == report.agents.len(),
    );

    let model_and_cap = assert_that(
        NAME_MODEL_AND_CAP,
        !records.is_empty() && records.iter().all(|r| !r.model.is_empty() && r.cap > 0),
    );

    // Both live backends in this run report a termination reason on the wire, so an absent one
    // here means the crate dropped it between the response and the report — which is the exact
    // omission that made "the model burned its budget" and "the server sent nothing" the same
    // opaque error until `4.0.0`.
    let termination = assert_that(
        NAME_TERMINATION,
        !records.is_empty() && records.iter().all(|r| r.finish.is_some()),
    );

    // The OFF half of `S11`'s comparison. This run uses the default, so a trace that came back
    // must report its LENGTH and withhold its TEXT — additive means the flag adds the text, not
    // that it decides whether anything is measured at all.
    let trace_is_length_only = assert_that(
        NAME_TRACE_LENGTH_ONLY,
        // Non-empty for the same reason as its three siblings: `all` over an empty set is true,
        // so without it a report that recorded nothing would certify the flag it never observed.
        !records.is_empty()
            && records
                .iter()
                .all(|r| !matches!(r.reasoning, ReasoningState::Measured { text: Some(_), .. })),
    );

    vec![
        one_per_seat,
        model_and_cap,
        termination,
        trace_is_length_only,
    ]
}

// ---------------------------------------------------------------------------
// S10 — the 62 k bundle stops costing a seat
// ---------------------------------------------------------------------------

const NAME_LARGE_OBSERVED: &str = "the large-payload run reached the wire and was recorded";
const NAME_NO_SEAT_LOST: &str = "no seat was lost to an empty completion on the large payload";
const NAME_NOT_DEGRADED: &str = "the large-payload run is not degraded";

/// `S10` — the raised output budget stops the 62 k bundle from costing a seat
/// (`sbtdd/smoke-harness-spec.md`, "S10").
///
/// # The large payload is the whole scenario, not a bigger version of a small one
///
/// Evidence run H passes clean against the same model and the same budget that run C fails; the
/// only difference is the size of the input. A harness that exercises only small payloads
/// certifies exactly what never breaks, and that blindness is what let this defect reach four
/// consumer reports before anyone saw it.
///
/// # What it does NOT assert, and why the obvious assertion is wrong
///
/// It does not require that no completion terminated on `length`. A completion that hit the
/// budget and **still produced a valid verdict** is a success, and recording it is the entire
/// point of the telemetry this milestone added — asserting its absence would make the harness
/// contradict the field it certifies. What must not happen is a seat being **lost**.
fn s10_the_large_payload_costs_no_seat(ctx: &RunContext<'_>) -> Vec<Assertion> {
    let Some(report) = ctx.report else {
        let reason = ctx
            .error
            .map(str::to_string)
            .unwrap_or_else(|| "the run never happened".to_string());
        return vec![
            Assertion::skip(NAME_LARGE_OBSERVED, reason.clone()),
            Assertion::skip(NAME_NO_SEAT_LOST, reason.clone()),
            Assertion::skip(NAME_NOT_DEGRADED, reason),
        ];
    };

    // Without this the two assertions below are true of a run that analysed nothing, and a green
    // meaning "nothing was looked at" is the failure mode this whole harness exists to prevent.
    let observed = assert_that(
        NAME_LARGE_OBSERVED,
        !report.completions.is_empty() && report.agents.len() + report.failed_agents.len() >= 3,
    );

    // The failure this release is named for, read from the outside: a seat whose model burned its
    // budget reasoning and returned nothing. The reason text is the crate's own rendering, which
    // `4.0.0` made name the budget instead of saying "http error 0".
    let no_seat_lost = assert_that(
        NAME_NO_SEAT_LOST,
        !report
            .failed_agents
            .values()
            .any(|reason| reason.contains("empty completion")),
    );

    let not_degraded = assert_that(NAME_NOT_DEGRADED, !report.degraded);

    vec![observed, no_seat_lost, not_degraded]
}

// ---------------------------------------------------------------------------
// S11 — `reasoning_trace` ADDS the text; the length is there either way
// ---------------------------------------------------------------------------

const NAME_TRACE_MEASURED: &str = "a reasoning trace was measured on the large-payload run";
const NAME_TRACE_TEXT: &str = "with the flag on, the trace carries its text as well as its length";

/// `S11` — the opt-in flag is ADDITIVE (`sbtdd/smoke-harness-spec.md`, "S11").
///
/// # The comparison is split across two scenarios, on purpose
///
/// A scenario reads ONE run, and the property is a comparison: length-only with the flag off,
/// length-and-text with it on. So this one certifies the **on** half against the large-payload
/// run — the only run that asks for the text — and `S13` certifies the **off** half against the
/// small happy run, which uses the default. Each asserts what its own run actually observed
/// rather than one of them speaking for a run it never saw.
///
/// # Why the large run is where the text is worth having
///
/// A model burning a large budget is the case somebody turns this flag on to understand. On a
/// small payload the assertion would only show the field is not empty.
fn s11_the_trace_flag_adds_the_text(ctx: &RunContext<'_>) -> Vec<Assertion> {
    let Some(report) = ctx.report else {
        let reason = ctx
            .error
            .map(str::to_string)
            .unwrap_or_else(|| "the run never happened".to_string());
        return vec![
            Assertion::skip(NAME_TRACE_MEASURED, reason.clone()),
            Assertion::skip(NAME_TRACE_TEXT, reason),
        ];
    };

    let measured: Vec<(usize, bool)> = report
        .completions
        .values()
        .flatten()
        .filter_map(|r| match &r.reasoning {
            ReasoningState::Measured { chars, text } => Some((*chars, text.is_some())),
            _ => None,
        })
        .collect();

    // Without a measured trace the assertion below is vacuous, and a green meaning "no model
    // reasoned, so nothing contradicted us" certifies nothing about the flag.
    let any_measured = assert_that(
        NAME_TRACE_MEASURED,
        measured.iter().any(|(chars, _)| *chars > 0),
    );

    // Additive, never substitutive: every measured trace carries its length AND, because this
    // run asked for it, its text.
    let carries_text = assert_that(
        NAME_TRACE_TEXT,
        !measured.is_empty()
            && measured
                .iter()
                .all(|(chars, has_text)| *chars > 0 && *has_text),
    );

    vec![any_measured, carries_text]
}

/// The E2 scenario table.
pub fn e2_scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            id: "S8",
            // The small happy run is enough: routing is a property of every completion, not of a
            // large payload. Reading it here also keeps S8 off the slow run's critical path.
            source: Source::Run(RunId::HappySmall),
            backend_tag: BackendNeed::Required,
            assert_fn: s8_completions_are_native_only,
        },
        Scenario {
            id: "S10",
            // The ONLY scenario that reads the large-payload run, and the reason that run
            // exists: the property is invisible at small sizes.
            source: Source::Run(RunId::Large62k),
            backend_tag: BackendNeed::Required,
            assert_fn: s10_the_large_payload_costs_no_seat,
        },
        Scenario {
            id: "S11",
            // The only run that asks for the trace text; `S13` reads the off half against the
            // small run, which uses the default.
            source: Source::Run(RunId::Large62k),
            backend_tag: BackendNeed::Required,
            assert_fn: s11_the_trace_flag_adds_the_text,
        },
        Scenario {
            id: "S13",
            // Same run as S8, and for the same reason: recording is a property of every
            // completion, so the cheap run observes it as well as the expensive one would.
            source: Source::Run(RunId::HappySmall),
            backend_tag: BackendNeed::Required,
            assert_fn: s13_every_completion_is_recorded,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alias::magi_core::prelude::MagiReport;
    use crate::outcome::ScenarioState;

    fn report_from(json: &str) -> MagiReport {
        serde_json::from_str(json).expect("test fixture JSON must deserialize into MagiReport")
    }

    #[test]
    fn the_table_carries_exactly_the_scenarios_this_stage_implements() {
        let ids: Vec<&str> = e2_scenarios().iter().map(|s| s.id).collect();
        assert_eq!(ids, vec!["S8", "S10", "S11", "S13"]);
    }

    #[test]
    fn the_legacy_path_named_here_is_not_the_one_the_crate_now_uses() {
        // If someone ever points `COMPLETIONS_PATH` back at `/v1`, the two assertions of this
        // scenario would contradict each other silently — one requiring records on that path,
        // the other requiring none. Pinning them as distinct makes that a test failure instead.
        assert_ne!(COMPLETIONS_PATH, LEGACY_COMPAT_PATH);
        assert_eq!(COMPLETIONS_PATH, "/api/chat");
    }
    // -- S13 --

    /// A report with one clean seat and one completion record for it. Written as JSON rather
    /// than built field by field because that is what a consumer actually receives, and it
    /// exercises the serde path the crate ships alongside the assertion.
    const ONE_CLEAN_SEAT: &str = r#"{
      "agents": [
        {"agent":"caspar","verdict":"approve","confidence":0.9,"summary":"s","reasoning":"r","findings":[],"recommendation":"go"}
      ],
      "consensus": {
        "consensus":"GO (1-0)","consensus_verdict":"approve","confidence":0.9,"score":1.0,
        "agent_count":1,"votes":{},"majority_summary":"","dissent":[],"findings":[],"conditions":[],"recommendations":{}
      },
      "banner":"","report":"","degraded":false,"failed_agents":{},
      "completions":{"caspar":[{"model":"glm-5.2","cap":16384,"finish":"stop",
        "completion_tokens":1280,"prompt_tokens":569,"reasoning":"NotMeasured"}]}
    }"#;

    /// The same report with the field absent entirely — which is what a `3.2.0` document looks
    /// like, and what a regression that stopped populating it would produce.
    const NO_RECORDS_AT_ALL: &str = r#"{
      "agents": [
        {"agent":"caspar","verdict":"approve","confidence":0.9,"summary":"s","reasoning":"r","findings":[],"recommendation":"go"}
      ],
      "consensus": {
        "consensus":"GO (1-0)","consensus_verdict":"approve","confidence":0.9,"score":1.0,
        "agent_count":1,"votes":{},"majority_summary":"","dissent":[],"findings":[],"conditions":[],"recommendations":{}
      },
      "banner":"","report":"","degraded":false,"failed_agents":{}
    }"#;

    #[test]
    fn s13_passes_when_every_seat_left_a_record() {
        let report = report_from(ONE_CLEAN_SEAT);
        let ctx = RunContext {
            report: Some(&report),
            ..RunContext::blank(RunId::HappySmall)
        };
        for a in s13_every_completion_is_recorded(&ctx) {
            assert_eq!(a.state, ScenarioState::Pass, "{}", a.name);
        }
    }

    #[test]
    fn s13_fails_on_a_report_that_recorded_nothing() {
        // The direction that matters. Every assertion in this scenario quantifies over the
        // record set, so all three are vacuously true over an empty one — and a scenario that
        // cannot go red on the regression it exists to catch is worse than no scenario.
        let report = report_from(NO_RECORDS_AT_ALL);
        assert!(
            report.completions.is_empty(),
            "the fixture must really be empty"
        );
        let ctx = RunContext {
            report: Some(&report),
            ..RunContext::blank(RunId::HappySmall)
        };
        for a in s13_every_completion_is_recorded(&ctx) {
            assert_eq!(a.state, ScenarioState::Fail, "{}", a.name);
        }
    }

    #[test]
    fn s13_fails_when_the_termination_reason_was_dropped() {
        // The omission that made "the model burned its budget" and "the server sent nothing"
        // the same opaque error until `4.0.0`, observed from the outside.
        let report = report_from(&ONE_CLEAN_SEAT.replace(r#""finish":"stop","#, ""));
        let ctx = RunContext {
            report: Some(&report),
            ..RunContext::blank(RunId::HappySmall)
        };
        let states: Vec<_> = s13_every_completion_is_recorded(&ctx)
            .into_iter()
            .map(|a| (a.name, a.state))
            .collect();
        assert!(
            states.contains(&(NAME_TERMINATION, ScenarioState::Fail)),
            "the termination row must be the one that goes red: {states:?}"
        );
    }

    #[test]
    fn s13_skips_rather_than_fails_when_the_run_produced_no_report() {
        // A run that never happened says nothing about the crate, and a red row here would
        // point an operator at code that was never reached.
        let ctx = RunContext::blank(RunId::HappySmall);
        for a in s13_every_completion_is_recorded(&ctx) {
            assert!(
                matches!(a.state, ScenarioState::Skip(_)),
                "{} must skip, got {:?}",
                a.name,
                a.state
            );
        }
    }
}
