// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-23

//! The axis-E scenarios: `S-E2` and `S-E3`.
//!
//! # Two of the four axis-E behaviours, and the other two are not smoke's job
//!
//! A behaviour from the spec becomes a scenario here **only if a run can observe it**. The
//! harness exists to exercise the crate the way a consumer does against a real backend, and a
//! property of the SOURCE is not visible from there.
//!
//! - `S-E1` — no identifier promises a record the code never writes — is a grep over
//!   `src/rotation.rs`. The field it is about is `pub(crate)`, so it is invisible even in the
//!   `published` mode. Its home is the class sweep that renamed it, and MS3's closing criteria.
//! - `S-E4` — the snapshot does not disturb rotation — is the ABSENCE of mutable state and the
//!   permanence of a `clear()`. A green run proves neither, and asserting it here would be
//!   vacuous in the exact sense this milestone has already had to correct twice.
//!
//! Hosting either one here would cost harness machinery for coverage that already exists.
//!
//! # Both read the same run, and that run exists for them
//!
//! [`RunId::PoolEligibility`] is a small payload with nothing injected — the snapshot is
//! **pre-dispatch**, so nothing has to rotate for it to be populated. What the run does carry is
//! a candidate built to be ineligible for two reasons at once, and a strict context guard to make
//! the second of them bite.
//!
//! **It is a run of its own rather than a rider on the happy one.** The precondition needs a
//! candidate whose lineage duplicates a seat's, and the harness config says in as many words that
//! such a candidate buys nothing; injecting it into a SHARED run would change the world for every
//! scenario that reads that run, and would contradict a design decision already written down.
//! The cost is one more backend run, and it is declared rather than hidden.

use crate::config::RunId;
use crate::runner::{assert_that, Assertion, BackendNeed, RunContext, Scenario, Source};

const NAME_WHY_INELIGIBLE: &str =
    "a seat that never rotated still reports which candidates were not eligible, and why";
const NAME_COARSE_BOUND: &str =
    "the window cause names itself a coarse lower bound, not a token count";
const NAME_ALL_CONDITIONS: &str = "every failing condition is reported, not only the first";

/// `S-E2` — the snapshot answers for seats that never rotated.
///
/// This run injects nothing, so **no seat rotates at all** — which is the point. A capture of the
/// live filter could only ever speak for seats that did rotate; a recalculation speaks for every
/// seat, and that is strictly more than the consumer asked for.
///
/// # It asserts the field is POPULATED, not merely present
///
/// An empty map would satisfy "the field exists" while proving nothing, and this run is
/// configured precisely so that at least one candidate is ineligible. A green row here therefore
/// means the snapshot reached a real report with real content.
fn s_e2_why_a_candidate_was_not_eligible(ctx: &RunContext<'_>) -> Vec<Assertion> {
    let Some(report) = ctx.report else {
        let reason = ctx
            .error
            .map(str::to_string)
            .unwrap_or_else(|| "the run never happened".to_string());
        return vec![
            Assertion::skip(NAME_WHY_INELIGIBLE, reason.clone()),
            Assertion::skip(NAME_COARSE_BOUND, reason),
        ];
    };
    let ineligible: Vec<_> = report
        .pool_eligibility
        .values()
        .flatten()
        .filter(|c| !c.causes.is_empty())
        .collect();
    vec![
        assert_that(
            NAME_WHY_INELIGIBLE,
            !report.pool_eligibility.is_empty() && !ineligible.is_empty(),
        ),
        // The type is what carries the qualification, so the type is what is read. A consumer
        // matching on the variant learns that the comparison was against `chars/4` without
        // having to find the rustdoc that says so.
        assert_that(
            NAME_COARSE_BOUND,
            ineligible
                .iter()
                .flat_map(|c| &c.causes)
                .all(|c| !format!("{c:?}").contains("WindowTooSmall")),
        ),
    ]
}

/// `S-E3` — every failing condition is reported, not only the first.
///
/// The real filter is a chain of `&&`, so it **short-circuits**: a candidate turned down by the
/// first condition never evaluates the sixth. Reporting what a capture of that chain saw would
/// name an arbitrary member of several true reasons, and the consumer would act on it.
///
/// The run carries a candidate that fails **two** conditions at once — its lineage belongs to
/// another seat, and it has no measured window under a strict guard — so a snapshot that
/// short-circuited would show one cause where two are true.
fn s_e3_all_failing_conditions(ctx: &RunContext<'_>) -> Vec<Assertion> {
    let Some(report) = ctx.report else {
        let reason = ctx
            .error
            .map(str::to_string)
            .unwrap_or_else(|| "the run never happened".to_string());
        return vec![Assertion::skip(NAME_ALL_CONDITIONS, reason)];
    };
    vec![assert_that(
        NAME_ALL_CONDITIONS,
        report
            .pool_eligibility
            .values()
            .flatten()
            .any(|c| c.causes.len() >= 2),
    )]
}

/// The two axis-E scenarios a run can observe.
pub fn e_scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            id: "S-E2",
            source: Source::Run(RunId::PoolEligibility),
            backend_tag: BackendNeed::Required,
            assert_fn: s_e2_why_a_candidate_was_not_eligible,
        },
        Scenario {
            id: "S-E3",
            source: Source::Run(RunId::PoolEligibility),
            backend_tag: BackendNeed::Required,
            assert_fn: s_e3_all_failing_conditions,
        },
    ]
}
