// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-20

//! The E2 scenarios: the ones that verify what `4.0.0` changed.
//!
//! They are written with the milestone that introduces the property, not before it, and not
//! after. Written before, they sit red for weeks and a permanently red harness gets rationalised
//! away — which is the `3.0.2` failure wearing different clothes. Written after, the property
//! ships uncertified. Written alongside, the scenario IS the test.

use crate::alias::magi_core::prelude::{CompletionRecord, ReasoningState};
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
const NAME_NEW_DEFAULT_APPLIED: &str =
    "every completion ran under the raised output budget, not the old one";
const NAME_PROMPT_LARGE: &str =
    "the large payload reached the model large IN TOKENS, not just in bytes";

/// The budget this release raised TO. Named rather than inlined because the assertion below
/// is about this number: on a tree where the raise never happened, every record would carry
/// the old one instead.
const RAISED_MAX_TOKENS: u32 = 16_384;

/// `R17` — the large-payload run really is large **in tokens**.
///
/// # Why bytes are not enough, and why this could not be written until now
///
/// Everything upstream of the wire is measured in BYTES: the generator fills to
/// `payload_target_bytes` and `config` refuses anything under
/// [`crate::config::MIN_PAYLOAD_TARGET_BYTES`]. But the failure this stage reproduces is a
/// function of TOKENS — evidence run H passes on the same model and the same budget that run C
/// fails, and the only difference is how much the model had to read. Three scenarios (`S3`,
/// `S10`, `S11`) read `RunId::Large62k`, and a fourth (`S8b`) reads `RunId::Large62kNoReasoning`,
/// sized from the same `payload_target_bytes`. **Not one of them would notice a payload that
/// silently shrank**, because not one of them looks at size at all.
///
/// This guard was specified with E1 and left unimplemented, because until `A-5` there was
/// nothing on the report to read: the backend's own `prompt_eval_count` now arrives as
/// [`CompletionRecord::prompt_tokens`]. It is measured by the BACKEND, which is strictly better
/// evidence than any byte proxy this harness could compute.
///
/// # The floor is derived, never hardcoded
///
/// A fixed floor would be wrong for a legally configured smaller payload: the default target is
/// 250 000 bytes (~63 900 tokens measured) but the accepted MINIMUM is 100 000. So the bound is
/// derived from that minimum at **half** the crate's own `chars/4` estimate — 8 chars per token
/// rather than 4 — so it is a lower bound on a lower bound, deliberately loose, because
/// tokenisers vary and this asserts that the payload ARRIVED large, not that any particular
/// ratio holds. It still discriminates by ~22x: the small-run payload is 2 048 bytes, which
/// measured 569 prompt tokens.
fn r17_the_prompt_is_large_in_tokens(records: &[&CompletionRecord]) -> Assertion {
    const FLOOR_TOKENS: usize = crate::config::MIN_PAYLOAD_TARGET_BYTES / 8;

    // Only the records that actually carry a measurement. A provider that does not report
    // `prompt_tokens` says nothing about the payload, and folding its `None` in either
    // direction would be an opinion the harness has no basis for.
    let measured: Vec<u32> = records.iter().filter_map(|c| c.prompt_tokens).collect();

    // The non-empty companion is the whole point: `all` over an empty set is TRUE, so without
    // this the row would go green on a run where nobody measured anything -- which is exactly
    // the shape of the defect this guard exists to catch, reproduced inside the guard.
    assert_that(
        NAME_PROMPT_LARGE,
        !measured.is_empty() && measured.iter().all(|t| *t as usize >= FLOOR_TOKENS),
    )
}

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
            Assertion::skip(NAME_NOT_DEGRADED, reason.clone()),
            Assertion::skip(NAME_NEW_DEFAULT_APPLIED, reason.clone()),
            Assertion::skip(NAME_PROMPT_LARGE, reason),
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

    let records: Vec<_> = report.completions.values().flatten().collect();

    // Without this the scenario ALSO passes on a tree where the raise never happened: three
    // healthy seats on a large payload satisfy every assertion above whether the budget was
    // 4 096 or 16 384. What ties it to the raise is the budget each attempt actually ran under.
    //
    // NOT "some completion spent more than 4 096", which is what this asserted first and what
    // the plan proposed. That is a property of the MODEL, not of the crate, and it flaked
    // between two consecutive live runs of identical code — green in the first, red in the
    // second. A gate that flickers teaches nothing, and this one would have been read as a
    // regression in the raise. The cap is what the crate controls, and a tree without the raise
    // could not produce it.
    let new_default_applied = assert_that(
        NAME_NEW_DEFAULT_APPLIED,
        // Its own non-empty companion, like every other row in this module. The scenario would
        // still redden through `NAME_LARGE_OBSERVED`, but a row that can pass over nothing is
        // the pattern this harness relies on not having.
        !records.is_empty() && records.iter().all(|c| c.cap == RAISED_MAX_TOKENS),
    );

    vec![
        observed,
        no_seat_lost,
        not_degraded,
        new_default_applied,
        r17_the_prompt_is_large_in_tokens(&records),
    ]
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

// ---------------------------------------------------------------------------
// S3 — the large payload, which is the case EC would have caught
// ---------------------------------------------------------------------------

const NAME_NO_EMPTY_MISCLASSIFIED: &str =
    "no seat was lost to an empty completion classified as transport";
const NAME_CUT_NAMES_BUDGET: &str = "a completion cut by the budget names the budget in its error";
const NAME_NO_RUN_WIDE: &str = "no content failure condemned a lineage run-wide";

/// `S3` — the failure this release is named for, observed from the outside
/// (`sbtdd/smoke-harness-spec.md`, "S3").
///
/// # Three assertions, because three separate things had to be true and only one was
///
/// The chain was: an empty completion became a synthetic `Http { status: 0 }`, which became
/// `Transport`, which condemned the lineage **run-wide** — taking it away from the other two
/// seats over what one seat observed. Each assertion below reads one link, so a red row says
/// which one came back rather than "the large payload is unhappy".
///
/// # Why the rotation telemetry is where the run-wide claim is read
///
/// A run-wide condemnation is not directly visible in a report field; what IS visible is the
/// **kind** each rotation event reports. `Transport` is the kind that means the run was
/// condemned, so a content failure reporting it is the mislabelling this milestone deleted.
fn s3_the_large_payload_loses_no_seat_to_misclassification(ctx: &RunContext<'_>) -> Vec<Assertion> {
    let Some(report) = ctx.report else {
        let reason = ctx
            .error
            .map(str::to_string)
            .unwrap_or_else(|| "the run never happened".to_string());
        return vec![
            Assertion::skip(NAME_NO_EMPTY_MISCLASSIFIED, reason.clone()),
            Assertion::skip(NAME_CUT_NAMES_BUDGET, reason.clone()),
            Assertion::skip(NAME_NO_RUN_WIDE, reason),
        ];
    };

    // NON-VACUITY FIRST, and it is what review found missing. All three assertions below are
    // `!any(...)` or `all(filter(...))`, so every one of them is satisfied by a run that
    // observed nothing at all — and this is the row that would be cited as evidence the fix
    // works. Its siblings `S8`, `S8b` and `S10` all pair with a non-empty check; this one was
    // the outlier.
    //
    // The precondition is the RECORD SET, not the presence of a cut: what has to have happened
    // for these to mean anything is that completions were observed at all.
    //
    // It does NOT make the rotation row unconditional, and saying otherwise would repeat the
    // defect in a smaller place: a healthy large-payload run rotates nowhere, so
    // `NAME_NO_RUN_WIDE` still quantifies over an empty chain. What the guard buys is that the
    // rows speak about a run that HAPPENED; what the rotation row asserts is "no hop, if any,
    // misreports its scope". Both are true statements and only one of them is unconditional.
    if report.completions.values().flatten().count() == 0 {
        let why = "the run recorded no completion at all, so there was nothing to classify — a \
                   green here would certify a run this scenario never saw";
        return vec![
            Assertion::skip(NAME_NO_EMPTY_MISCLASSIFIED, why),
            Assertion::skip(NAME_CUT_NAMES_BUDGET, why),
            Assertion::skip(NAME_NO_RUN_WIDE, why),
        ];
    }

    // A seat lost to an empty completion shows up in `failed_agents`, and until `4.0.0` its
    // reason read as a transport fault — sending the operator to look at a network that had
    // answered HTTP 200 perfectly.
    let no_misclassified = assert_that(
        NAME_NO_EMPTY_MISCLASSIFIED,
        !report
            .failed_agents
            .values()
            .any(|r| r.contains("empty completion") && r.contains("transport")),
    );

    // Quantified over the cuts that HAPPENED, so it certifies the shape of the message rather
    // than the absence of the case. A run where nothing was cut satisfies it truthfully —
    // there was no error to name a budget in — and the guard above is what makes that
    // truthfulness rest on an observed run rather than on an empty one.
    let cut_names_budget = assert_that(
        NAME_CUT_NAMES_BUDGET,
        report
            .failed_agents
            .values()
            .filter(|r| r.contains("empty completion"))
            .all(|r| r.contains("output budget")),
    );

    // The link that matters most, and the one nothing else in this harness reads: a content
    // failure must never report the kind that means the whole run was condemned.
    let no_run_wide = assert_that(
        NAME_NO_RUN_WIDE,
        report
            .rotations
            .values()
            .flat_map(|r| r.chain.iter())
            .all(|hop| hop.kind().is_mage_local() || !content_failure_detail(hop.detail())),
    );

    vec![no_misclassified, cut_names_budget, no_run_wide]
}

/// Whether a rotation hop's detail describes a CONTENT failure rather than a transport one.
///
/// Reads the text because that is all a hop carries besides its kind, and the whole point of the
/// assertion is to catch a hop whose kind and detail disagree. It is deliberately narrow: it
/// looks for the two phrases this crate's own content errors render, not for a general notion of
/// content, so a transport hop cannot match by accident.
fn content_failure_detail(detail: &str) -> bool {
    detail.contains("empty completion") || detail.contains("response contract")
}

// ---------------------------------------------------------------------------
// S9 — a malformed request of OURS aborts; it does not masquerade as a dead mage
// ---------------------------------------------------------------------------

const NAME_ABORTS: &str = "the run ends with an error that names a defect of this crate";
const NAME_NOT_A_SEAT: &str = "no report was produced, so no seat was blamed for it";

/// `S9` — the abort, read from the outside (`sbtdd/smoke-harness-spec.md`, "S9").
///
/// # Why the assertion is about the ERROR and not about a report field
///
/// There is no report. That is the whole point: `failed_agents` is where model failures land
/// every day, so a defect of ours filed there would be invisible in the noise of the normal and
/// the operator would go and look at the model. This project has already paid that bill once, on
/// a bug that masqueraded as a provider error.
///
/// # What it does NOT do, and could not
///
/// It cannot tell a backend's own `load` from the replayed one — nothing on the wire
/// distinguishes them. It does not need to: this run configures the replay itself, so the
/// footprint can only be the one it injected.
fn s9_a_defect_of_ours_aborts_the_run(ctx: &RunContext<'_>) -> Vec<Assertion> {
    // Deliberately NOT the `report.is_none()` skip the other scenarios use: here an absent
    // report is the PASS condition, so borrowing that guard would skip the run this scenario
    // exists to read.
    let Some(error) = ctx.error else {
        let why = if ctx.report.is_some() {
            "the run produced a report, so the defect was filed against a seat instead of \
             aborting — which is the failure this scenario exists to catch"
                .to_string()
        } else {
            "the run produced neither a report nor an error, so it never happened".to_string()
        };
        // A report in hand is a FAIL, not a skip: the crate did something, and it did the wrong
        // thing. Only "the run never happened" is unanswerable.
        return if ctx.report.is_some() {
            vec![
                assert_that(NAME_ABORTS, false),
                assert_that(NAME_NOT_A_SEAT, false),
            ]
        } else {
            vec![
                Assertion::skip(NAME_ABORTS, why.clone()),
                Assertion::skip(NAME_NOT_A_SEAT, why),
            ]
        };
    };

    // The name has to carry the category, or the bug hides in the noise. The rendered error says
    // "defect in magi-core", and it separates what was OBSERVED from the hypothesis about why —
    // because the discriminant rests on a single captured case.
    let aborts = assert_that(
        NAME_ABORTS,
        error.contains("magi-core") && error.contains("no generation"),
    );

    let not_a_seat = assert_that(NAME_NOT_A_SEAT, ctx.report.is_none());

    vec![aborts, not_a_seat]
}

// ---------------------------------------------------------------------------
// S9b — the NoGeneration footprint still matches against the REAL backend
// ---------------------------------------------------------------------------

const NAME_COUNTERS_ABSENT: &str = "the live backend still OMITS the token counters";
const NAME_REASON_LOAD: &str = "the live backend still reports the load termination";
const NAME_CONTENT_EMPTY: &str = "the live backend still returns empty content";

/// The status a native answer carries. A probe that got anything else did not obtain the
/// response this scenario is about.
const NATIVE_OK: u16 = 200;

/// `S9b` — erosion detection (`sbtdd/smoke-harness-spec.md`, "S9b").
///
/// # Not a duplicate of `S9`, and the difference is the whole reason it exists
///
/// `S9` replays a CAPTURED body, so it certifies that our classification is right — and it would
/// stay green forever even if the backend changed its API, because a fixture does not change.
/// This asks the LIVE backend, and it is the only thing that detects erosion: the day the backend
/// reports `eval_count: 0` instead of omitting it, the footprint stops matching, the run-aborting
/// protection degrades to the reversible route, and nobody is told.
///
/// # Three assertions, because the footprint is a CONJUNCTION
///
/// The trigger requires all three signals together, deliberately, so that everything else takes
/// the reversible route. A scenario asserting them as one would say "the footprint eroded"
/// without saying which signal moved — and which one moved is the fix.
///
/// # Declared scope: a WARM backend
///
/// It does not cover a cold start, which is the most plausible way to see this footprint without
/// anyone having written a bad request — `done_reason: "load"` literally means the model was
/// loading. That gap is named rather than papered over: claiming coverage this does not have
/// would be worse than the gap itself.
fn s9b_the_footprint_still_matches_the_live_backend(ctx: &RunContext<'_>) -> Vec<Assertion> {
    let (Some(body), Some(status)) = (ctx.erosion_probe_body, ctx.erosion_probe_status) else {
        let why = "the erosion probe did not run, so nothing was asked of the live backend";
        return vec![
            Assertion::skip(NAME_COUNTERS_ABSENT, why),
            Assertion::skip(NAME_REASON_LOAD, why),
            Assertion::skip(NAME_CONTENT_EMPTY, why),
        ];
    };

    // The STATUS is a gate, not a decoration, and leaving it unread cost a whole smoke round.
    // The probe once asked about a model that does not exist and got a `404` error envelope
    // back; two assertions went red and the third — "the counters are absent" — passed
    // VACUOUSLY, because an error body has no counters either. The result read as "the backend
    // half-eroded" when the truth was "we asked the wrong question".
    //
    // Anything but a `200` means the probe did not obtain the answer this scenario is about, so
    // it SKIPS naming the status rather than reporting erosion it never observed.
    if status != NATIVE_OK {
        let why = format!(
            "the backend answered {status}, so this is not the response whose footprint is \
             being checked — the probe asked the wrong question, which says nothing about erosion"
        );
        return vec![
            Assertion::skip(NAME_COUNTERS_ABSENT, why.clone()),
            Assertion::skip(NAME_REASON_LOAD, why.clone()),
            Assertion::skip(NAME_CONTENT_EMPTY, why),
        ];
    }

    let Ok(parsed) = serde_json::from_slice::<serde_json::Value>(body) else {
        let why = format!("the backend answered {status} with a body that is not JSON");
        return vec![
            Assertion::skip(NAME_COUNTERS_ABSENT, why.clone()),
            Assertion::skip(NAME_REASON_LOAD, why.clone()),
            Assertion::skip(NAME_CONTENT_EMPTY, why),
        ];
    };

    // And the counters assertion is no longer allowed to answer on its own. Absence is only
    // meaningful in a body that IS a native answer; in anything else it is the absence of the
    // whole shape, which is a different statement.
    let is_native_answer = parsed.get("message").is_some() && parsed.get("done").is_some();

    // ABSENT, not zero, and the difference is the whole discriminant: a backend that starts
    // sending `eval_count: 0` would satisfy any check written as "counters are zero or missing",
    // and the protection would be gone with the test still green.
    let counters_absent = assert_that(
        NAME_COUNTERS_ABSENT,
        is_native_answer
            && parsed.get("eval_count").is_none()
            && parsed.get("prompt_eval_count").is_none(),
    );

    let reason_load = assert_that(
        NAME_REASON_LOAD,
        parsed.get("done_reason").and_then(|v| v.as_str()) == Some("load"),
    );

    let content_empty = assert_that(
        NAME_CONTENT_EMPTY,
        parsed
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .is_some_and(str::is_empty),
    );

    vec![counters_absent, reason_load, content_empty]
}

// ---------------------------------------------------------------------------
// S12 — a mixed trio: one honours, one declares, nobody breaks
// ---------------------------------------------------------------------------

const NAME_RUN_SURVIVES: &str = "the mixed trio completes without breaking or degrading";
const NAME_SOMEONE_DECLARES: &str =
    "the seat that cannot honour the control declares it, naming its backend";
const NAME_DISTINGUISHABLE: &str =
    "the declaration is distinguishable from a measured absence of reasoning";

/// `S12` — one seat honours the reasoning control, another cannot, and the run survives
/// (`sbtdd/smoke-harness-spec.md`, "S12").
///
/// # Why this needs a heterogeneous trio and could not be split into two runs
///
/// The control is set ONCE, for the whole run. A homogeneous trio can only ever show one half of
/// the contract, and two runs would show the halves separately — while what has to hold is that
/// the two **coexist**. Failing the run when a provider cannot honour the control was the obvious
/// first design, and it is exactly what this scenario exists to prove was not shipped: a consumer
/// who wants reasoning off *where it can be* would get a broken run instead.
///
/// # What it does NOT assert
///
/// It does not require that the honouring seat measured zero reasoning. A model may simply not
/// have reasoned, and demanding otherwise would be asserting a property of the model rather than
/// of the crate.
fn s12_a_mixed_trio_honours_and_declares(ctx: &RunContext<'_>) -> Vec<Assertion> {
    let Some(report) = ctx.report else {
        let reason = ctx
            .error
            .map(str::to_string)
            .unwrap_or_else(|| "the run never happened".to_string());
        return vec![
            Assertion::skip(NAME_RUN_SURVIVES, reason.clone()),
            Assertion::skip(NAME_SOMEONE_DECLARES, reason.clone()),
            Assertion::skip(NAME_DISTINGUISHABLE, reason),
        ];
    };

    // The half that the "fail loudly" design would have broken.
    let survives = assert_that(
        NAME_RUN_SURVIVES,
        !report.degraded && report.agents.len() == 3,
    );

    let states: Vec<&ReasoningState> = report
        .completions
        .values()
        .flatten()
        .map(|r| &r.reasoning)
        .collect();

    // Non-empty, or both assertions below quantify over nothing and certify a run they never saw.
    let declares = assert_that(
        NAME_SOMEONE_DECLARES,
        states.iter().any(
            |s| matches!(s, ReasoningState::Unsupported { backend, .. } if !backend.is_empty()),
        ),
    );

    // The point of a typed state rather than an `Option`: "this backend cannot do it" must not
    // read as "it can and the model did not reason". Both appear in this very run.
    let distinguishable = assert_that(
        NAME_DISTINGUISHABLE,
        !states.is_empty()
            && states
                .iter()
                .any(|s| !matches!(s, ReasoningState::Unsupported { .. })),
    );

    vec![survives, declares, distinguishable]
}

// ---------------------------------------------------------------------------
// S8b — switching the reasoning channel off TAKES EFFECT on the 62k payload
// ---------------------------------------------------------------------------

const NAME_CONVERGES: &str = "every seat produced a verdict on the large payload";
const NAME_NOT_REASONING: &str = "no seat spent the run reasoning";
const NAME_BUDGET_INTACT: &str = "and no completion came near the output budget";

/// The budget a converging completion must stay well under for the control to have taken effect.
///
/// Not the cap itself: a completion at the cap is one that was CUT, and this scenario is about a
/// completion that finished early because it did not reason. Run F spent 602 tokens where run G
/// spent 32 768 — the gap is orders of magnitude, so the threshold does not need to be tight.
const CONVERGED_TOKEN_CEILING: u32 = 8_000;

/// `S8b` — the reproduction of the one measurement that proves the C axis works.
///
/// # Why this cannot be a fixture, and why it needed its own run
///
/// A captured body proves that something happened **once**; it cannot show that the control
/// **causes** the change. That needs a live backend, the same payload, and the flag switched.
///
/// It also could not share the other large-payload run. That one needs the reasoning channel
/// **on** — `S10` reads a completion that spent more than the old default, `S11` reads a trace —
/// and this one needs it **off**. Two opposite properties on the same payload, so the payload is
/// paid for twice. Naming that cost is better than quietly weakening one of them.
///
/// # A sibling of `S8` rather than a new id
///
/// `S8` certifies that completions go to the native endpoint; this certifies that the control
/// only that endpoint honours actually works. Same axis, one measurement apart — the same
/// relationship `S9b` has with `S9`.
fn s8b_disabling_reasoning_makes_the_large_payload_converge(
    ctx: &RunContext<'_>,
) -> Vec<Assertion> {
    let Some(report) = ctx.report else {
        let reason = ctx
            .error
            .map(str::to_string)
            .unwrap_or_else(|| "the run never happened".to_string());
        return vec![
            Assertion::skip(NAME_CONVERGES, reason.clone()),
            Assertion::skip(NAME_NOT_REASONING, reason.clone()),
            Assertion::skip(NAME_BUDGET_INTACT, reason),
        ];
    };

    // The point of the whole axis: with the channel off, the payload that used to cost a seat
    // produces a verdict.
    let converges = assert_that(NAME_CONVERGES, !report.degraded && report.agents.len() == 3);

    let records: Vec<_> = report.completions.values().flatten().collect();

    // The control took effect, read from what came BACK rather than from what was asked. A seat
    // whose provider cannot honour it declares `Unsupported`, which is honest and also not this
    // run: every seat here is on the native path.
    let not_reasoning = assert_that(
        NAME_NOT_REASONING,
        !records.is_empty()
            && records.iter().all(
                |r| !matches!(r.reasoning, ReasoningState::Measured { chars, .. } if chars > 0),
            ),
    );

    // And the budget survived it. Without this the scenario passes on a run where every seat
    // burned its whole allowance and happened to emit a verdict anyway — which is the outcome
    // the control exists to prevent, not the one it produces.
    let budget_intact = assert_that(
        NAME_BUDGET_INTACT,
        !records.is_empty()
            && records.iter().all(|r| {
                r.completion_tokens
                    .is_none_or(|n| n < CONVERGED_TOKEN_CEILING)
            }),
    );

    vec![converges, not_reasoning, budget_intact]
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
            id: "S8b",
            // Its own large-payload run, with the reasoning channel OFF. It cannot share the
            // other one, which needs it on.
            source: Source::Run(RunId::Large62kNoReasoning),
            backend_tag: BackendNeed::Required,
            assert_fn: s8b_disabling_reasoning_makes_the_large_payload_converge,
        },
        Scenario {
            id: "S3",
            // The large payload, for the same reason as `S10`: the misclassification this
            // scenario reads needs a model that can actually exhaust its budget.
            source: Source::Run(RunId::Large62k),
            backend_tag: BackendNeed::Required,
            assert_fn: s3_the_large_payload_loses_no_seat_to_misclassification,
        },
        Scenario {
            id: "S9",
            // Its own run, because the injected footprint ABORTS: sharing a run with any other
            // scenario would deny that scenario the report it reads.
            source: Source::Run(RunId::CrateDefect),
            backend_tag: BackendNeed::Required,
            assert_fn: s9_a_defect_of_ours_aborts_the_run,
        },
        Scenario {
            id: "S9b",
            // Preflight-sourced: the probe runs once, before anything, and belongs to no run.
            source: Source::Preflight,
            backend_tag: BackendNeed::Required,
            assert_fn: s9b_the_footprint_still_matches_the_live_backend,
        },
        Scenario {
            id: "S10",
            // The scenario that SIZES the large-payload run, and the reason that run
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
            id: "S12",
            // Its own run, because the trio itself is the fixture: no other run mixes providers.
            source: Source::Run(RunId::MixedTrio),
            backend_tag: BackendNeed::Required,
            assert_fn: s12_a_mixed_trio_honours_and_declares,
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
    /// The name the R17 token guard is declared under. Held as a constant so the tripwire and
    /// the guard cannot drift apart silently.
    const R17_GUARD_FN: &str = "r17_the_prompt_is_large_in_tokens";

    /// # This tripwire earned its keep, which is why it outlived the stub it was written for
    ///
    /// R17 spent E1 as an `unimplemented!()` behind an `e2` feature whose own comment promised
    /// MS1 would enable it. MS1 did not, and the feature gated nothing else, so the guard was
    /// never compiled — the boundary written specifically so it could not be forgotten was
    /// itself forgotten, while four scenarios kept reading the payload it was meant to size.
    /// The guard is real and unconditional now, and its tripwire follows it here.
    #[test]
    fn the_r17_token_guard_is_still_present_in_the_source() {
        assert!(
            crate::testkit::source_declares_fn(include_str!("e2.rs"), R17_GUARD_FN),
            "the R17 guard was removed or commented out; four scenarios read the large payload              and not one of the others looks at its size"
        );
    }

    #[test]
    fn a_mention_of_the_r17_guard_does_not_satisfy_the_tripwire() {
        // The tripwire matched a SUBSTRING once, so two slashes in front of the guard left it
        // green while the guard was gone. Every input here contains the name; none declares it.
        let mentions = [
            format!("// fn {R17_GUARD_FN}() {{"),
            format!("    //fn {R17_GUARD_FN}() {{"),
            format!("    let name = \"fn {R17_GUARD_FN}\";"),
            format!("/*\nfn {R17_GUARD_FN}() {{}}\n*/"),
            format!("/* fn {R17_GUARD_FN}() {{}} */"),
            format!("/// See [`fn {R17_GUARD_FN}`] for the token bound."),
        ];
        for src in mentions {
            assert!(
                !crate::testkit::source_declares_fn(&src, R17_GUARD_FN),
                "a mention must not satisfy the tripwire: {src:?}"
            );
        }
    }

    #[test]
    fn a_real_r17_declaration_satisfies_the_tripwire_however_it_is_formatted() {
        // The other direction, and it matters just as much: a tripwire that goes red when
        // someone re-indents the guard is a tripwire that gets deleted.
        let declarations = [
            format!("fn {R17_GUARD_FN}() {{}}"),
            format!("                fn {R17_GUARD_FN}() {{}}"),
            format!("\t\tfn {R17_GUARD_FN}(\n) {{}}"),
            format!("#[allow(dead_code)]\n    fn {R17_GUARD_FN}() {{}}"),
            // A block comment that OPENED and CLOSED earlier must not leave the scanner stuck.
            format!("/* an earlier note */\nfn {R17_GUARD_FN}() {{}}"),
            // Nor must a `/*` that only ever appears inside a line comment.
            format!("// see /* the note */\nfn {R17_GUARD_FN}() {{}}"),
        ];
        for src in declarations {
            assert!(
                crate::testkit::source_declares_fn(&src, R17_GUARD_FN),
                "a real declaration must satisfy the tripwire: {src:?}"
            );
        }
    }
    use super::*;
    use crate::alias::magi_core::prelude::MagiReport;
    use crate::outcome::ScenarioState;
    use crate::runner::ErrorClass;

    fn report_from(json: &str) -> MagiReport {
        serde_json::from_str(json).expect("test fixture JSON must deserialize into MagiReport")
    }

    #[test]
    fn the_table_carries_exactly_the_scenarios_this_stage_implements() {
        let ids: Vec<&str> = e2_scenarios().iter().map(|s| s.id).collect();
        assert_eq!(
            ids,
            vec!["S8", "S8b", "S3", "S9", "S9b", "S10", "S11", "S12", "S13"]
        );
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

    // -- R17 --

    fn rec(prompt_tokens: Option<u32>) -> CompletionRecord {
        let r = CompletionRecord::new("glm-5.2".to_string(), 16_384);
        match prompt_tokens {
            Some(n) => r.with_prompt_tokens(n),
            None => r,
        }
    }

    #[test]
    fn r17_passes_when_the_payload_arrived_large() {
        let recs = [rec(Some(63_924))];
        let refs: Vec<&CompletionRecord> = recs.iter().collect();
        assert_eq!(
            r17_the_prompt_is_large_in_tokens(&refs).state,
            ScenarioState::Pass
        );
    }

    #[test]
    fn r17_fails_when_the_payload_silently_shrank() {
        // The small-run payload is 2 048 bytes, which measured 569 prompt tokens against the
        // live backend. That is the substitution this guard exists to catch: every other row
        // of `S10` is satisfied by three healthy seats whatever they were asked to read.
        let recs = [rec(Some(569))];
        let refs: Vec<&CompletionRecord> = recs.iter().collect();
        assert_eq!(
            r17_the_prompt_is_large_in_tokens(&refs).state,
            ScenarioState::Fail
        );
    }

    #[test]
    fn r17_fails_rather_than_passing_vacuously_when_nothing_was_measured() {
        // THE case the row is written around. `all` over an empty set is TRUE, so a guard
        // without its non-empty companion goes green precisely when it learned nothing —
        // reproducing, inside the guard, the defect the guard was added to prevent.
        let recs = [rec(None), rec(None)];
        let refs: Vec<&CompletionRecord> = recs.iter().collect();
        assert_eq!(
            r17_the_prompt_is_large_in_tokens(&refs).state,
            ScenarioState::Fail
        );
    }

    #[test]
    fn r17_fails_when_only_some_completions_saw_the_large_payload() {
        // THE case that separates `all` from `any`, and without it the quantifier is not
        // tested at all: in every other fixture here the two agree, so `.all(` -> `.any(`
        // survived the whole set. A run where one seat read the 62 k bundle and another read
        // something tiny would have passed under the weaker guard with nothing to notice.
        let recs = [rec(Some(63_924)), rec(Some(569))];
        let refs: Vec<&CompletionRecord> = recs.iter().collect();
        assert_eq!(
            r17_the_prompt_is_large_in_tokens(&refs).state,
            ScenarioState::Fail
        );
    }

    #[test]
    fn r17_reads_the_measured_records_and_ignores_the_silent_ones() {
        // A provider that does not report `prompt_tokens` says nothing about the payload.
        // Folding its `None` in either direction would be an opinion with no basis: counted as
        // a failure it would redden a healthy run, counted as a pass it would be vacuity again.
        let recs = [rec(None), rec(Some(63_924))];
        let refs: Vec<&CompletionRecord> = recs.iter().collect();
        assert_eq!(
            r17_the_prompt_is_large_in_tokens(&refs).state,
            ScenarioState::Pass
        );
    }

    // -- S3 --

    /// A report carrying ONE rotation hop for Caspar, whose `chain` is exactly `chain_json`, and
    /// whose `failed_agents` is exactly `failed_json`.
    ///
    /// Shared by the three tests below so each differs only in the thing it is about, never in
    /// the surrounding shape — the same discipline `S4`'s fixture follows in `e1`.
    ///
    /// It carries a completion record because `S3`'s non-vacuity guard requires one: a report
    /// that recorded nothing is SKIPPED, and these fixtures exist to exercise the assertions
    /// themselves rather than the guard in front of them.
    fn report_with_chain_and_failures(chain_json: &str, failed_json: &str) -> MagiReport {
        report_from(&format!(
            r#"{{
              "agents": [],
              "consensus": {{
                "consensus":"GO","consensus_verdict":"approve","confidence":0.5,"score":0.5,
                "agent_count":0,"votes":{{}},"majority_summary":"","dissent":[],"findings":[],"conditions":[],"recommendations":{{}}
              }},
              "banner":"","report":"","degraded":false,
              "failed_agents": {failed_json},
              "completions": {{"caspar":[{{"model":"glm-5.2","cap":16384,"reasoning":"NotMeasured"}}]}},
              "rotations": {{
                "caspar": {{
                  "model_configured":"glm-5.2:cloud","model_used":"deepseek-v4-pro:cloud",
                  "chain": {chain_json},
                  "ran_unmeasured": false
                }}
              }}
            }}"#
        ))
    }

    #[test]
    fn s3_passes_when_a_content_failure_reports_a_mage_local_kind() {
        // The shape `4.0.0` produces: a completion the budget cut, rotated away from with the
        // kind that says so, and a failure reason that names the budget.
        let report = report_with_chain_and_failures(
            r#"[{"from":"zhipu","to":"deepseek","model_resolved":"deepseek-v4-pro:cloud","kind":"empty_completion","detail":"empty completion: the model returned no content. The output budget in force was 16384 tokens"}]"#,
            "{}",
        );
        let ctx = RunContext {
            report: Some(&report),
            ..RunContext::blank(RunId::Large62k)
        };
        for a in s3_the_large_payload_loses_no_seat_to_misclassification(&ctx) {
            assert_eq!(a.state, ScenarioState::Pass, "{}", a.name);
        }
    }

    #[test]
    fn s3_fails_when_a_content_failure_still_reports_the_run_wide_kind() {
        // The exact defect: an empty completion riding `transport`, which everywhere else in
        // this system means the run was condemned. It is what took a healthy lineage away from
        // the other two seats.
        let report = report_with_chain_and_failures(
            r#"[{"from":"zhipu","to":"deepseek","model_resolved":"deepseek-v4-pro:cloud","kind":"transport","detail":"empty completion: the model returned no content"}]"#,
            "{}",
        );
        let ctx = RunContext {
            report: Some(&report),
            ..RunContext::blank(RunId::Large62k)
        };
        let states: Vec<_> = s3_the_large_payload_loses_no_seat_to_misclassification(&ctx)
            .into_iter()
            .map(|a| (a.name, a.state))
            .collect();
        assert!(
            states.contains(&(NAME_NO_RUN_WIDE, ScenarioState::Fail)),
            "the run-wide row must be the one that goes red: {states:?}"
        );
    }

    #[test]
    fn s3_fails_when_a_lost_seat_reads_as_a_transport_fault() {
        // The operator-facing half of the same defect: the seat is gone and the reason sends
        // them to look at a network that answered HTTP 200 perfectly.
        let report = report_with_chain_and_failures(
            "[]",
            r#"{"caspar":"transport: empty completion after rotation"}"#,
        );
        let ctx = RunContext {
            report: Some(&report),
            ..RunContext::blank(RunId::Large62k)
        };
        let states: Vec<_> = s3_the_large_payload_loses_no_seat_to_misclassification(&ctx)
            .into_iter()
            .map(|a| (a.name, a.state))
            .collect();
        assert!(
            states.contains(&(NAME_NO_EMPTY_MISCLASSIFIED, ScenarioState::Fail)),
            "{states:?}"
        );
        // And the budget row too: a lost seat whose message names no budget is exactly the
        // error that used to read "http error 0".
        assert!(
            states.contains(&(NAME_CUT_NAMES_BUDGET, ScenarioState::Fail)),
            "{states:?}"
        );
    }

    #[test]
    fn s3_skips_on_a_report_that_recorded_no_completion_at_all() {
        // The non-vacuity guard, exercised. Every assertion in this scenario is satisfied by a
        // run that observed nothing — and this is the row that would be cited as evidence the
        // milestone's headline fix works, so a green meaning "nothing was looked at" is the
        // worst possible outcome here.
        let report = report_from(NO_RECORDS_AT_ALL);
        let ctx = RunContext {
            report: Some(&report),
            ..RunContext::blank(RunId::Large62k)
        };
        for a in s3_the_large_payload_loses_no_seat_to_misclassification(&ctx) {
            assert!(
                matches!(a.state, ScenarioState::Skip(_)),
                "{} must skip over an empty record set, got {:?}",
                a.name,
                a.state
            );
        }
    }

    #[test]
    fn s3_skips_rather_than_fails_when_the_run_produced_no_report() {
        let ctx = RunContext::blank(RunId::Large62k);
        for a in s3_the_large_payload_loses_no_seat_to_misclassification(&ctx) {
            assert!(
                matches!(a.state, ScenarioState::Skip(_)),
                "{} must skip, got {:?}",
                a.name,
                a.state
            );
        }
    }
    // -- S9 --

    #[test]
    fn s9_passes_when_the_run_aborted_naming_the_crate() {
        let err = "defect in magi-core, run aborted: no generation - token counters absent \
                   (termination: Some(Load)); the known cause is a request without `messages`";
        let ctx = RunContext {
            error: Some(err),
            error_class: Some(ErrorClass::CrateFailure),
            ..RunContext::blank(RunId::CrateDefect)
        };
        for a in s9_a_defect_of_ours_aborts_the_run(&ctx) {
            assert_eq!(a.state, ScenarioState::Pass, "{}", a.name);
        }
    }

    #[test]
    fn s9_fails_when_the_defect_was_filed_against_a_seat_instead_of_aborting() {
        // The regression this scenario exists for, and the reason a report in hand is a FAIL
        // rather than a skip: the crate DID something, and what it did was hide a bug of ours in
        // the place where model failures land every day.
        let report = report_from(NO_RECORDS_AT_ALL);
        let ctx = RunContext {
            report: Some(&report),
            ..RunContext::blank(RunId::CrateDefect)
        };
        for a in s9_a_defect_of_ours_aborts_the_run(&ctx) {
            assert_eq!(a.state, ScenarioState::Fail, "{}", a.name);
        }
    }

    #[test]
    fn s9_fails_when_the_run_aborted_for_some_other_reason() {
        // An abort alone is not the property: `EndpointDown` also aborts, and reading it as this
        // defect would certify a classification that never happened.
        let ctx = RunContext {
            error: Some("endpoint down: 2 lineages connection-failed"),
            error_class: Some(ErrorClass::CrateFailure),
            ..RunContext::blank(RunId::CrateDefect)
        };
        let states: Vec<_> = s9_a_defect_of_ours_aborts_the_run(&ctx)
            .into_iter()
            .map(|a| (a.name, a.state))
            .collect();
        assert!(
            states.contains(&(NAME_ABORTS, ScenarioState::Fail)),
            "{states:?}"
        );
    }

    #[test]
    fn s9_skips_only_when_the_run_never_happened_at_all() {
        let ctx = RunContext::blank(RunId::CrateDefect);
        for a in s9_a_defect_of_ours_aborts_the_run(&ctx) {
            assert!(
                matches!(a.state, ScenarioState::Skip(_)),
                "{} must skip, got {:?}",
                a.name,
                a.state
            );
        }
    }
    // -- S9b --

    /// The captured footprint, verbatim: `200`, `done_reason: "load"`, empty content, and the
    /// token counters **absent** rather than zero.
    const LIVE_FOOTPRINT: &[u8] = br#"{"model":"m","created_at":"t","message":{"role":"assistant","content":""},"done_reason":"load","done":true}"#;

    fn erosion_ctx(body: &'static [u8]) -> RunContext<'static> {
        RunContext {
            erosion_probe_body: Some(body),
            erosion_probe_status: Some(200),
            ..RunContext::blank(RunId::HappySmall)
        }
    }

    #[test]
    fn s9b_passes_while_the_footprint_still_matches() {
        for a in s9b_the_footprint_still_matches_the_live_backend(&erosion_ctx(LIVE_FOOTPRINT)) {
            assert_eq!(a.state, ScenarioState::Pass, "{}", a.name);
        }
    }

    #[test]
    fn s9b_fails_the_moment_the_counters_stop_being_absent() {
        // THE erosion case, and the one a looser check would miss: a backend that starts sending
        // `eval_count: 0` satisfies "zero or missing" perfectly, and the run-aborting protection
        // would be gone with the scenario still green.
        const WITH_ZEROS: &[u8] = br#"{"model":"m","message":{"role":"assistant","content":""},"done_reason":"load","eval_count":0,"prompt_eval_count":0,"done":true}"#;
        let states: Vec<_> =
            s9b_the_footprint_still_matches_the_live_backend(&erosion_ctx(WITH_ZEROS))
                .into_iter()
                .map(|a| (a.name, a.state))
                .collect();
        assert!(
            states.contains(&(NAME_COUNTERS_ABSENT, ScenarioState::Fail)),
            "the counters row must be the one that names the erosion: {states:?}"
        );
    }

    #[test]
    fn s9b_fails_when_the_termination_reason_changes() {
        const OTHER_REASON: &[u8] = br#"{"model":"m","message":{"role":"assistant","content":""},"done_reason":"stop","done":true}"#;
        let states: Vec<_> =
            s9b_the_footprint_still_matches_the_live_backend(&erosion_ctx(OTHER_REASON))
                .into_iter()
                .map(|a| (a.name, a.state))
                .collect();
        assert!(
            states.contains(&(NAME_REASON_LOAD, ScenarioState::Fail)),
            "{states:?}"
        );
    }

    #[test]
    fn s9b_skips_when_the_probe_did_not_run() {
        // Under `--no-backend`, or when the request could not be sent. A probe that did not run
        // says nothing about whether the footprint eroded, and a red row here would send a
        // reader to look at a backend nobody asked.
        let ctx = RunContext::blank(RunId::HappySmall);
        for a in s9b_the_footprint_still_matches_the_live_backend(&ctx) {
            assert!(
                matches!(a.state, ScenarioState::Skip(_)),
                "{} must skip, got {:?}",
                a.name,
                a.state
            );
        }
    }
    // -- S12 --

    /// A three-seat report whose completions carry exactly `reasoning_json` per seat, in order.
    fn report_with_reasoning_states(states: [&str; 3]) -> MagiReport {
        let [a, b, c] = states;
        report_from(&format!(
            r#"{{
              "agents": [
                {{"agent":"melchior","verdict":"approve","confidence":0.9,"summary":"s","reasoning":"r","findings":[],"recommendation":"go"}},
                {{"agent":"balthasar","verdict":"approve","confidence":0.9,"summary":"s","reasoning":"r","findings":[],"recommendation":"go"}},
                {{"agent":"caspar","verdict":"approve","confidence":0.9,"summary":"s","reasoning":"r","findings":[],"recommendation":"go"}}
              ],
              "consensus": {{
                "consensus":"STRONG GO","consensus_verdict":"approve","confidence":0.95,"score":1.0,
                "agent_count":3,"votes":{{}},"majority_summary":"","dissent":[],"findings":[],"conditions":[],"recommendations":{{}}
              }},
              "banner":"","report":"","degraded":false,"failed_agents":{{}},
              "completions": {{
                "melchior":[{{"model":"m","cap":16384,"reasoning":{a}}}],
                "balthasar":[{{"model":"b","cap":16384,"reasoning":{b}}}],
                "caspar":[{{"model":"c","cap":16384,"reasoning":{c}}}]
              }}
            }}"#
        ))
    }

    /// What a seat on a backend that cannot honour the control reports.
    const UNSUPPORTED: &str =
        r#"{"Unsupported":{"backend":"openai-compatible","chars":3535,"text":null}}"#;
    /// What a seat that CAN honour it reports when the model did not reason.
    const MEASURED_ZERO: &str = r#"{"Measured":{"chars":0,"text":null}}"#;

    #[test]
    fn s12_passes_on_a_genuinely_mixed_trio() {
        let report = report_with_reasoning_states([MEASURED_ZERO, MEASURED_ZERO, UNSUPPORTED]);
        let ctx = RunContext {
            report: Some(&report),
            ..RunContext::blank(RunId::MixedTrio)
        };
        for a in s12_a_mixed_trio_honours_and_declares(&ctx) {
            assert_eq!(a.state, ScenarioState::Pass, "{}", a.name);
        }
    }

    #[test]
    fn s12_fails_when_nobody_declared_the_control_unsupported() {
        // The silent no-op: every seat reports as if the control took, so a consumer who set it
        // once believes all three honoured it. That is the failure C-8 exists to prevent, and
        // it is invisible to every other scenario.
        let report = report_with_reasoning_states([MEASURED_ZERO, MEASURED_ZERO, MEASURED_ZERO]);
        let ctx = RunContext {
            report: Some(&report),
            ..RunContext::blank(RunId::MixedTrio)
        };
        let states: Vec<_> = s12_a_mixed_trio_honours_and_declares(&ctx)
            .into_iter()
            .map(|a| (a.name, a.state))
            .collect();
        assert!(
            states.contains(&(NAME_SOMEONE_DECLARES, ScenarioState::Fail)),
            "{states:?}"
        );
    }

    #[test]
    fn s12_fails_when_the_declaration_is_indistinguishable_from_a_measurement() {
        // Every seat unsupported is not a mixed trio: the run would prove nothing about the two
        // states coexisting, which is the entire property.
        let report = report_with_reasoning_states([UNSUPPORTED, UNSUPPORTED, UNSUPPORTED]);
        let ctx = RunContext {
            report: Some(&report),
            ..RunContext::blank(RunId::MixedTrio)
        };
        let states: Vec<_> = s12_a_mixed_trio_honours_and_declares(&ctx)
            .into_iter()
            .map(|a| (a.name, a.state))
            .collect();
        assert!(
            states.contains(&(NAME_DISTINGUISHABLE, ScenarioState::Fail)),
            "{states:?}"
        );
    }

    #[test]
    fn s12_skips_rather_than_fails_when_the_run_produced_no_report() {
        let ctx = RunContext::blank(RunId::MixedTrio);
        for a in s12_a_mixed_trio_honours_and_declares(&ctx) {
            assert!(
                matches!(a.state, ScenarioState::Skip(_)),
                "{} must skip, got {:?}",
                a.name,
                a.state
            );
        }
    }
    /// The `404` envelope the backend returns for a model that does not exist.
    ///
    /// This is the body the erosion probe actually received on its first live run, because it
    /// reused a model name chosen precisely because it does not exist.
    const NOT_FOUND_ENVELOPE: &[u8] = br#"{"error":"model \"x\" not found"}"#;

    #[test]
    fn s9b_skips_rather_than_reporting_erosion_when_the_probe_asked_the_wrong_question() {
        // The defect this closes, from the first live SMOKE round: a `404` made two rows go red
        // and the third pass VACUOUSLY — an error body has no counters either — so the result
        // read as "the backend half-eroded" when nothing about erosion had been observed.
        let ctx = RunContext {
            erosion_probe_body: Some(NOT_FOUND_ENVELOPE),
            erosion_probe_status: Some(404),
            ..RunContext::blank(RunId::HappySmall)
        };
        for a in s9b_the_footprint_still_matches_the_live_backend(&ctx) {
            assert!(
                matches!(a.state, ScenarioState::Skip(_)),
                "{} must skip on a non-200, got {:?}",
                a.name,
                a.state
            );
        }
    }

    #[test]
    fn s9b_counters_row_cannot_pass_on_a_body_that_is_not_a_native_answer() {
        // Belt to the status gate's braces, and a different failure: a `200` carrying something
        // that is not a native answer at all. Absence of the counters means nothing there — it
        // is the absence of the whole shape, which is a different statement.
        let ctx = RunContext {
            erosion_probe_body: Some(br#"{"unexpected":"shape"}"#),
            erosion_probe_status: Some(200),
            ..RunContext::blank(RunId::HappySmall)
        };
        let states: Vec<_> = s9b_the_footprint_still_matches_the_live_backend(&ctx)
            .into_iter()
            .map(|a| (a.name, a.state))
            .collect();
        assert!(
            states.contains(&(NAME_COUNTERS_ABSENT, ScenarioState::Fail)),
            "the counters row must not answer on a body that is not a native answer: {states:?}"
        );
    }

    #[test]
    fn the_erosion_probe_does_not_reuse_the_deliberately_missing_transparency_model() {
        // The root cause, pinned at its source. `PROBE_MODEL` is a name chosen BECAUSE it does
        // not exist — right for a probe that compares two halves against each other, wrong for
        // one that needs the backend's real answer to a malformed request.
        let src = include_str!("../runner.rs").replace("\r\n", "\n");
        let start = src
            .find("pub async fn prime_erosion_probe")
            .expect("the erosion probe must exist");
        let end = src[start..]
            .find("\n    }\n")
            .map(|i| start + i)
            .expect("the probe must be a complete function");
        // COMMENT LINES STRIPPED before scanning: the probe's own comment names the constant in
        // order to explain why reusing it was wrong, and a scan that reads prose as code would
        // fail on the very explanation of the fix. Same trap as two sibling checks in the crate.
        let code: String = src[start..end]
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join(
                "
",
            );
        assert!(
            !code.contains("PROBE_MODEL"),
            "the erosion probe must ask about a model that EXISTS"
        );
    }
}
