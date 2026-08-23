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
//! **The coarse-bound naming of `E-4` is not asserted here either, and that is deliberate.** This
//! run produces no window cause at all — its candidate is ineligible for lineage and model — so
//! any check phrased over one would read an empty set and pass. It is asserted positively where
//! the cause can be produced: a unit test that hands the snapshot a measured window and a bound
//! it does not meet.
//!
//! # Both read the same run, and that run exists for them
//!
//! [`RunId::PoolEligibility`] is a small payload with nothing injected — the snapshot is
//! **pre-dispatch**, so nothing has to rotate for it to be populated. What the run does carry is
//! a candidate built to be ineligible for two reasons at once: it takes the first seat's lineage
//! and the second seat's model, so the second seat sees both that another mage already holds the
//! lineage and that the model is one it already runs.
//!
//! **An earlier form of this run tried to use the "unmeasured window" cause instead, and it could
//! not fire.** The candidate reused a seat's model, and a seat's model is probed — so the
//! capability map already had a measured window for it and the cause was unreachable by
//! construction. `S-E3` went red on the first live run and said so. The unmeasured cause is
//! covered where it can be produced: a unit test that passes the capability map directly.
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
        return vec![Assertion::skip(NAME_WHY_INELIGIBLE, reason)];
    };
    let ineligible: Vec<_> = report
        .pool_eligibility
        .values()
        .flatten()
        .filter(|c| !c.causes.is_empty())
        .collect();
    vec![assert_that(
        NAME_WHY_INELIGIBLE,
        !report.pool_eligibility.is_empty() && !ineligible.is_empty(),
    )]
}

/// `S-E3` — every failing condition is reported, not only the first.
///
/// The real filter is a chain of `&&`, so it **short-circuits**: a candidate turned down by the
/// first condition never evaluates the sixth. Reporting what a capture of that chain saw would
/// name an arbitrary member of several true reasons, and the consumer would act on it.
///
/// The run carries a candidate that fails **two** conditions at once — its lineage belongs to
/// another seat, and its model is one that seat already runs — so a snapshot that
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alias::magi_core::reporting::MagiReport;
    use crate::outcome::ScenarioState;

    /// A report carrying one candidate with the given causes, and nothing else of
    /// interest.
    ///
    /// Deserialized from a literal rather than constructed: `MagiReport` is
    /// `#[non_exhaustive]`, so an outside crate cannot build one with a literal —
    /// which is the same door the harness proves is open for `ProviderError`.
    fn report_with(causes_json: &str) -> MagiReport {
        let json = format!(
            r#"{{
              "agents":[],
              "consensus":{{
                "consensus":"","consensus_verdict":"approve","confidence":1.0,"score":1.0,
                "agent_count":0,"votes":{{}},"majority_summary":"","dissent":[],
                "findings":[],"conditions":[],"recommendations":{{}}
              }},
              "banner":"","report":"","degraded":false,"failed_agents":{{}},
              "pool_eligibility":{{"caspar":[{{"model":"md","causes":{causes_json}}}]}}
            }}"#
        );
        serde_json::from_str(&json).expect("test fixture JSON must deserialize into MagiReport")
    }

    /// Both scenarios are registered.
    #[test]
    fn both_axis_e_scenarios_are_registered() {
        assert_eq!(e_scenarios().len(), 2);
    }

    /// A context with no report SKIPS rather than passing.
    ///
    /// Without this, both scenarios against a run that never happened would report
    /// success having read nothing — the outcome this harness exists to make
    /// impossible.
    #[test]
    fn no_report_skips_instead_of_passing() {
        let ctx = RunContext::blank(RunId::PoolEligibility);
        for a in s_e2_why_a_candidate_was_not_eligible(&ctx)
            .into_iter()
            .chain(s_e3_all_failing_conditions(&ctx))
        {
            assert!(
                matches!(a.state, ScenarioState::Skip(_)),
                "{} must skip, not report a reading it never made",
                a.name
            );
        }
    }

    /// `S-E3` goes RED when only one condition is reported, and green on two.
    ///
    /// The half that matters. A scenario that only ever sees a two-cause report
    /// cannot show it would notice a one-cause one — and a snapshot that
    /// short-circuited produces exactly that.
    #[test]
    fn s_e3_fails_when_only_one_condition_is_reported() {
        let one = report_with(r#"["LineageHeldByAnotherMage"]"#);
        let ctx = RunContext {
            report: Some(&one),
            ..RunContext::blank(RunId::PoolEligibility)
        };
        assert_eq!(
            s_e3_all_failing_conditions(&ctx)[0].state,
            ScenarioState::Fail,
            "one cause where two are true must be a red row"
        );

        let two = report_with(r#"["LineageHeldByAnotherMage","ModelAlreadyUsedByThisMage"]"#);
        let ctx = RunContext {
            report: Some(&two),
            ..RunContext::blank(RunId::PoolEligibility)
        };
        assert_eq!(
            s_e3_all_failing_conditions(&ctx)[0].state,
            ScenarioState::Pass
        );
    }

    /// `S-E2` goes RED when nothing was ruled out.
    ///
    /// An empty `causes` list is what a run whose precondition failed to materialise
    /// produces, and it is precisely the case that must not read as coverage.
    #[test]
    fn s_e2_fails_when_nothing_was_ruled_out() {
        let none = report_with("[]");
        let ctx = RunContext {
            report: Some(&none),
            ..RunContext::blank(RunId::PoolEligibility)
        };
        assert_eq!(
            s_e2_why_a_candidate_was_not_eligible(&ctx)[0].state,
            ScenarioState::Fail,
            "a report with nothing ruled out proves nothing about the field"
        );
    }
}
