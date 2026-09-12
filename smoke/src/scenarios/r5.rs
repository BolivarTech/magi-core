// Author: Julian Bolivar
// Version: 4.1.0
// Date: 2026-09-11

//! The two endpoint-down scenarios of R-5: `S-R5a` and `S-R5b`.
//!
//! # What R-5 changed, and why a unit test can only approximate it
//!
//! The endpoint-down latch is armed when two seats of distinct lineages fail at the
//! connection level. Before R-5 the join loop consulted it after every outcome and aborted the
//! run the moment it was set — **after the first successful join**, cancelling the two seats
//! still in flight, even when those seats had already rotated past the blip and were about to
//! answer. R-5 conditions the abort on the run being irrecoverable: the latch is set AND the
//! seats already succeeded plus the ones still pending cannot reach the consensus quorum.
//!
//! That criterion lives in the interaction of the join loop with the abort guard and with the
//! order in which the seat handles resolve. A unit test approximates it; a run against a real
//! backend, with the failure injected on the wire, exercises it. That is why these two are the
//! only smoke scenarios this milestone declares **non-waivable**.
//!
//! # The two runs, and what each asserts
//!
//! [`RunId::EndpointBlip`] cuts the **first** connection of two seats of distinct lineages and
//! forwards everything after it. Both seats see `ProviderError::Network`, the latch arms, both
//! rotate to a candidate of yet another lineage, and the third seat answers first time.
//! `S-R5a` asserts that `analyze()` returned a report with **three** verdicts and no
//! degradation — the pre-fix crate returned `EndpointDown` here, and that is the red this row
//! exists to produce — plus one piece of evidence that the blip really happened: **exactly as
//! many seats as were cut** carry a rotation hop classified as transport. Without that second
//! row a run whose injection silently never fired would pass the first on a trivially healthy
//! trio.
//!
//! [`RunId::EndpointDown`] cuts **every** connection of every model the run can dispatch —
//! seats and fallbacks alike, with an unbounded budget — so no seat can ever succeed and the
//! quorum is unreachable at the second join. `S-R5b` asserts the run aborted with the **typed**
//! `MagiError::EndpointDown`. A report is a red row, and so is any other error: a regression
//! that stopped aborting early would wait every seat out and leave with `InsufficientAgents`,
//! which is exactly the "different error" this row must not accept.
//!
//! # What is NOT asserted, and why
//!
//! Neither scenario asserts **which model** answered a rotated seat. That is decided by the
//! backend and the config's fallback order, not by the crate, and pinning it would turn a
//! config change into a red row about the product.
//!
//! `S-R5b` does not assert that the abort came **before** the remaining seats were joined.
//! From `analyze()`'s outcome that is unobservable — with every connection cut, every seat
//! fails within milliseconds, so there is nothing left to wait for by the time the criterion
//! fires. The crate's own suite pins the join at which the abort lands; this row pins that the
//! abort still happens at all, typed, against a real wire.
//!
//! # The seed comes by another path than the mechanism under test
//!
//! Both failures enter through the proxy cutting the socket, never by touching the latch. A
//! scenario that armed `endpoint_down_signalled` itself would move its setup and its assertion
//! together and could not fail. And the budget is a **counter, never a timer**: the proxy cuts
//! the first `n` completions naming a model and forwards from there, so `S-R5a` neither sleeps
//! nor watches the product's progress — the two things that make a scenario intermittent.
//!
//! # A dropped attempt leaves no row on the wire
//!
//! The proxy records only exchanges that had a response, and a cut connection had none. So
//! neither scenario reads `records`: the evidence is `analyze()`'s outcome — the report, or the
//! typed error — and the rotation telemetry inside the report.
//!
//! # The precondition `S-R5a` needs from the CONFIG, and where it is enforced
//!
//! Two seats rotate **concurrently**, and the crate lets one lineage be held by one live mage
//! at a time, so the two dropped seats need **two** fallbacks of lineages distinct from every
//! seat's and from each other's. A config with a single fallback lets one seat rotate and
//! leaves the other with nowhere to go: the run completes **degraded with two verdicts**, and
//! the two rows here would go red naming the CRATE for a section short in a TOML file — the
//! 1-versus-2 confusion the harness exists to eliminate. So the **preflight refuses** such a
//! config in its config step, beside the check that refuses an empty pool for the rotation
//! run: exit 2, naming this run and the count it needs, before anything is spent. The rows
//! here still read the report honestly — a degraded report is red, not skipped — so a config
//! that somehow reached a run with too shallow a pool would still not pass by omission.

use crate::alias::magi_core::reporting::MagiReport;
use crate::alias::magi_core::rotation::RotationKind;
use crate::config::RunId;
use crate::runner::{
    assert_that, Assertion, BackendNeed, RunContext, Scenario, Source, BLIP_SEATS,
};

const NAME_RECOVERED_RUN_COMPLETES: &str =
    "a run whose seats rotated past a connection blip completes with three verdicts";
const NAME_BLIP_REALLY_HAPPENED: &str = "every cut seat rotated away from its connection";
const NAME_DEAD_ENDPOINT_ABORTS: &str = "a dead endpoint aborts the run with a typed EndpointDown";

/// How many verdicts a full, non-degraded trio produces.
const FULL_TRIO: usize = 3;

/// The skip reason for a run that produced neither a report nor an error.
const NEVER_HAPPENED: &str = "the run never happened";

/// `S-R5a` — a run that recovered from a connection blip is not aborted.
///
/// Two rows. The first is the property R-5 exists for: a report with a full trio and no
/// degradation, where the pre-fix crate returned `EndpointDown`. The second is the evidence the
/// blip was real — exactly [`BLIP_SEATS`] seats whose first rotation hop is
/// [`RotationKind::Transport`] — so the first cannot pass on a run where the injection never
/// fired.
///
/// A typed error fails the first row and leaves the second unreadable: the run aborted before
/// any rotation telemetry existed, so that row skips naming the error rather than inventing a
/// count. A run that never happened skips both.
fn s_r5a_recovered_run_is_not_aborted(ctx: &RunContext<'_>) -> Vec<Assertion> {
    match (ctx.report, ctx.error) {
        (Some(report), _) => vec![
            assert_that(
                NAME_RECOVERED_RUN_COMPLETES,
                report.agents.len() == FULL_TRIO && !report.degraded,
            ),
            assert_that(
                NAME_BLIP_REALLY_HAPPENED,
                seats_that_left_on_transport(report) == BLIP_SEATS,
            ),
        ],
        // A typed failure where a report belongs: the pre-fix outcome, and a red row.
        (None, Some(error)) => vec![
            assert_that(NAME_RECOVERED_RUN_COMPLETES, false),
            Assertion::skip(
                NAME_BLIP_REALLY_HAPPENED,
                format!("the run aborted before any rotation could be read: {error}"),
            ),
        ],
        (None, None) => vec![
            Assertion::skip(NAME_RECOVERED_RUN_COMPLETES, NEVER_HAPPENED),
            Assertion::skip(NAME_BLIP_REALLY_HAPPENED, NEVER_HAPPENED),
        ],
    }
}

/// How many seats' FIRST rotation hop was a transport failure — the footprint a cut
/// connection leaves in the report.
///
/// The first hop and not any hop: a seat cut once rotates once for that reason, and a later
/// hop for another cause is that seat's story, not the blip's. Counted over the seats rather
/// than over the hops for the same reason.
///
/// # Complexity
///
/// `O(a)` in the number of agents, reading one hop each.
fn seats_that_left_on_transport(report: &MagiReport) -> usize {
    report
        .rotations
        .values()
        .filter(|r| {
            r.chain
                .first()
                .is_some_and(|hop| hop.kind() == RotationKind::Transport)
        })
        .count()
}

/// `S-R5b` — an endpoint that is really down still aborts the run, typed.
///
/// One row: `analyze()` returned `MagiError::EndpointDown`. It is read from the typed carrier
/// [`RunContext::reported_endpoint_down`], never from the rendered text. A report fails it — the
/// crate carried on past an unreachable quorum — and so does any other error, since a run that
/// merely waited every seat out leaves with `InsufficientAgents`. A run that never happened
/// skips.
fn s_r5b_dead_endpoint_still_aborts(ctx: &RunContext<'_>) -> Vec<Assertion> {
    if ctx.report.is_none() && ctx.error.is_none() {
        return vec![Assertion::skip(NAME_DEAD_ENDPOINT_ABORTS, NEVER_HAPPENED)];
    }
    // A report, or any error but the endpoint-down abort, is the run carrying on past an
    // unreachable quorum — the regression, not a reading that could not be made.
    vec![assert_that(
        NAME_DEAD_ENDPOINT_ABORTS,
        ctx.reported_endpoint_down,
    )]
}

/// The two R-5 scenarios, each over the run that exists for it.
pub fn r5_scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            id: "S-R5a",
            source: Source::Run(RunId::EndpointBlip),
            backend_tag: BackendNeed::Required,
            assert_fn: s_r5a_recovered_run_is_not_aborted,
        },
        Scenario {
            id: "S-R5b",
            source: Source::Run(RunId::EndpointDown),
            backend_tag: BackendNeed::Required,
            assert_fn: s_r5b_dead_endpoint_still_aborts,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::ScenarioState;
    use crate::runner::ErrorClass;

    const AGENT_MELCHIOR: &str = r#"{"agent":"melchior","verdict":"approve","confidence":0.9,"summary":"s","reasoning":"r","findings":[],"recommendation":"go"}"#;
    const AGENT_BALTHASAR: &str = r#"{"agent":"balthasar","verdict":"approve","confidence":0.9,"summary":"s","reasoning":"r","findings":[],"recommendation":"go"}"#;
    const AGENT_CASPAR: &str = r#"{"agent":"caspar","verdict":"approve","confidence":0.9,"summary":"s","reasoning":"r","findings":[],"recommendation":"go"}"#;

    /// A rotation entry whose single hop left `from` for `to` because of a cut connection.
    fn transport_hop(from: &str, to: &str) -> String {
        format!(
            r#"{{"model_configured":"m-{from}","model_used":"m-{to}","chain":[{{"from":"{from}","to":"{to}","model_resolved":"m-{to}","kind":"transport","detail":"connection cut"}}],"ran_unmeasured":false}}"#
        )
    }

    /// A rotation entry for a seat that never left its primary.
    fn no_hop(lineage: &str) -> String {
        format!(
            r#"{{"model_configured":"m-{lineage}","model_used":"m-{lineage}","chain":[],"ran_unmeasured":false}}"#
        )
    }

    /// A report with the given agents, degradation flag and rotation map.
    ///
    /// Deserialized from a literal rather than constructed: `MagiReport` is
    /// `#[non_exhaustive]`, so an outside crate cannot build one with a literal.
    fn report_with(agents: &[&str], degraded: bool, rotations: &[(&str, String)]) -> MagiReport {
        let agents = agents.join(",");
        let rotations = rotations
            .iter()
            .map(|(agent, entry)| format!(r#""{agent}":{entry}"#))
            .collect::<Vec<_>>()
            .join(",");
        let json = format!(
            r#"{{
              "agents":[{agents}],
              "consensus":{{
                "consensus":"GO","consensus_verdict":"approve","confidence":0.9,"score":1.0,
                "agent_count":0,"votes":{{}},"majority_summary":"","dissent":[],
                "findings":[],"conditions":[],"recommendations":{{}}
              }},
              "banner":"","report":"","degraded":{degraded},"failed_agents":{{}},
              "rotations":{{{rotations}}}
            }}"#
        );
        serde_json::from_str(&json).expect("test fixture JSON must deserialize into MagiReport")
    }

    /// The report the blip run produces on the fixed crate: three verdicts, not degraded,
    /// and the two cut seats each carrying one transport hop.
    fn recovered_report() -> MagiReport {
        report_with(
            &[AGENT_MELCHIOR, AGENT_BALTHASAR, AGENT_CASPAR],
            false,
            &[
                ("melchior", transport_hop("alibaba", "deepseek")),
                ("balthasar", transport_hop("moonshot", "openai")),
                ("caspar", no_hop("zhipu")),
            ],
        )
    }

    /// Both scenarios are registered, each over its own run.
    #[test]
    fn both_endpoint_scenarios_are_registered() {
        let scenarios = r5_scenarios();
        assert_eq!(scenarios.len(), 2);
        assert_eq!(scenarios[0].id, "S-R5a");
        assert_eq!(scenarios[0].source, Source::Run(RunId::EndpointBlip));
        assert_eq!(scenarios[1].id, "S-R5b");
        assert_eq!(scenarios[1].source, Source::Run(RunId::EndpointDown));
        for s in &scenarios {
            assert_eq!(
                s.backend_tag,
                BackendNeed::Required,
                "{} reads a run that talks to a backend",
                s.id
            );
        }
    }

    /// A context with no report and no error SKIPS every row rather than passing any.
    ///
    /// The row counts are exact: a scenario that returned no rows at all would satisfy a
    /// "every row skips" loop vacuously, which is green by omission one level up.
    #[test]
    fn no_report_skips_instead_of_passing() {
        let blip = s_r5a_recovered_run_is_not_aborted(&RunContext::blank(RunId::EndpointBlip));
        assert_eq!(blip.len(), 2, "S-R5a has two rows");
        let down = s_r5b_dead_endpoint_still_aborts(&RunContext::blank(RunId::EndpointDown));
        assert_eq!(down.len(), 1, "S-R5b has one row");
        for a in blip.into_iter().chain(down) {
            assert!(
                matches!(a.state, ScenarioState::Skip(_)),
                "{} must skip, not report a reading it never made",
                a.name
            );
        }
    }

    /// `S-R5a` goes RED on the pre-fix outcome: a typed `EndpointDown` where a report belongs.
    ///
    /// This is the mutation proof for the fix in `src/`: revert R-5 and the blip run returns
    /// exactly this error, and this is the row that says so. The evidence row cannot be read
    /// from an aborted run, so it skips naming the error instead of inventing a count.
    #[test]
    fn s_r5a_fails_on_the_pre_fix_endpoint_down_abort() {
        let err = "endpoint down: no lineage reachable (alibaba, moonshot)".to_string();
        let ctx = RunContext {
            error: Some(&err),
            error_class: Some(ErrorClass::Environment),
            reported_endpoint_down: true,
            ..RunContext::blank(RunId::EndpointBlip)
        };
        let rows = s_r5a_recovered_run_is_not_aborted(&ctx);
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0].state,
            ScenarioState::Fail,
            "an aborted run is the defect R-5 removed; it must be a red row"
        );
        match &rows[1].state {
            ScenarioState::Skip(reason) => assert!(
                reason.contains("endpoint down"),
                "the skip must carry the error the run aborted with: {reason}"
            ),
            other => panic!("rotation evidence cannot be read from an aborted run: {other:?}"),
        }
    }

    /// `S-R5a` is green on the fixed outcome: three verdicts, no degradation, two transport
    /// hops.
    #[test]
    fn s_r5a_passes_on_three_verdicts_with_two_transport_rotations() {
        let report = recovered_report();
        let ctx = RunContext {
            report: Some(&report),
            ..RunContext::blank(RunId::EndpointBlip)
        };
        let rows = s_r5a_recovered_run_is_not_aborted(&ctx);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].state, ScenarioState::Pass, "{:?}", rows[0]);
        assert_eq!(rows[1].state, ScenarioState::Pass, "{:?}", rows[1]);
    }

    /// `S-R5a` goes RED when the run completed degraded — the shape a pool one candidate
    /// deep produces, where one cut seat rotates and the other has nowhere to go.
    ///
    /// The preflight refuses that config before any run, so this is the row's honesty on a
    /// report it should never see: both rows red, because two verdicts is not a full trio and
    /// one transport hop is not two, and never a skip that reads as green by omission.
    #[test]
    fn s_r5a_fails_on_a_degraded_report() {
        let report = report_with(
            &[AGENT_MELCHIOR, AGENT_CASPAR],
            true,
            &[
                ("melchior", transport_hop("alibaba", "deepseek")),
                ("balthasar", no_hop("moonshot")),
                ("caspar", no_hop("zhipu")),
            ],
        );
        let ctx = RunContext {
            report: Some(&report),
            ..RunContext::blank(RunId::EndpointBlip)
        };
        let rows = s_r5a_recovered_run_is_not_aborted(&ctx);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].state, ScenarioState::Fail, "{:?}", rows[0]);
        assert_eq!(rows[1].state, ScenarioState::Fail, "{:?}", rows[1]);
    }

    /// `S-R5a`'s first row needs BOTH the trio count and the degradation flag, each on its
    /// own.
    ///
    /// The degraded fixture above fails on the flag alone, so a mutation that relaxed the
    /// count to `>= 2` survived it. Two verdicts under a clean flag is a report that contradicts
    /// itself — a seat is missing and nothing says so — and three verdicts under a raised
    /// flag is one that admits a failure the count hides; the row must be red on either.
    #[test]
    fn s_r5a_first_row_needs_both_the_count_and_the_flag() {
        let two_hops = [
            ("melchior", transport_hop("alibaba", "deepseek")),
            ("balthasar", transport_hop("moonshot", "openai")),
            ("caspar", no_hop("zhipu")),
        ];

        let two_under_a_clean_flag = report_with(&[AGENT_MELCHIOR, AGENT_CASPAR], false, &two_hops);
        let ctx = RunContext {
            report: Some(&two_under_a_clean_flag),
            ..RunContext::blank(RunId::EndpointBlip)
        };
        let rows = s_r5a_recovered_run_is_not_aborted(&ctx);
        assert_eq!(
            rows[0].state,
            ScenarioState::Fail,
            "two verdicts is not a full trio, whatever the flag says"
        );

        let three_under_a_raised_flag = report_with(
            &[AGENT_MELCHIOR, AGENT_BALTHASAR, AGENT_CASPAR],
            true,
            &two_hops,
        );
        let ctx = RunContext {
            report: Some(&three_under_a_raised_flag),
            ..RunContext::blank(RunId::EndpointBlip)
        };
        let rows = s_r5a_recovered_run_is_not_aborted(&ctx);
        assert_eq!(
            rows[0].state,
            ScenarioState::Fail,
            "a report that calls itself degraded is not a recovered run, whatever the count"
        );
    }

    /// `S-R5a`'s evidence row goes RED on a healthy trio that never rotated.
    ///
    /// Three verdicts with no transport hop is what a run whose injection silently never fired
    /// looks like. The first row is green on it — the trio really did complete — and that is
    /// precisely why the second row exists.
    #[test]
    fn s_r5a_evidence_row_fails_when_no_seat_rotated() {
        let report = report_with(
            &[AGENT_MELCHIOR, AGENT_BALTHASAR, AGENT_CASPAR],
            false,
            &[
                ("melchior", no_hop("alibaba")),
                ("balthasar", no_hop("moonshot")),
                ("caspar", no_hop("zhipu")),
            ],
        );
        let ctx = RunContext {
            report: Some(&report),
            ..RunContext::blank(RunId::EndpointBlip)
        };
        let rows = s_r5a_recovered_run_is_not_aborted(&ctx);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].state, ScenarioState::Pass, "{:?}", rows[0]);
        assert_eq!(
            rows[1].state,
            ScenarioState::Fail,
            "a trio that never rotated proves nothing about the latch"
        );
    }

    /// `S-R5b` is green on a typed `EndpointDown` and RED on the two things a regression would
    /// produce instead: a report, or a different environment error.
    ///
    /// `InsufficientAgents` is the sharper of the two — it carries the same `ErrorClass`, so
    /// a row that read the class alone would pass on it, and it is exactly what a crate that
    /// stopped aborting early returns after waiting every seat out.
    #[test]
    fn s_r5b_passes_only_on_a_typed_endpoint_down() {
        let down = "endpoint down: no lineage reachable (alibaba, moonshot)".to_string();
        let ctx = RunContext {
            error: Some(&down),
            error_class: Some(ErrorClass::Environment),
            reported_endpoint_down: true,
            ..RunContext::blank(RunId::EndpointDown)
        };
        let rows = s_r5b_dead_endpoint_still_aborts(&ctx);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].state, ScenarioState::Pass, "{:?}", rows[0]);

        let report = recovered_report();
        let ctx = RunContext {
            report: Some(&report),
            ..RunContext::blank(RunId::EndpointDown)
        };
        let rows = s_r5b_dead_endpoint_still_aborts(&ctx);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].state,
            ScenarioState::Fail,
            "a report from a dead endpoint means the crate carried on past an unreachable quorum"
        );

        let insufficient = "insufficient agents: 0 succeeded, 2 required".to_string();
        let ctx = RunContext {
            error: Some(&insufficient),
            error_class: Some(ErrorClass::Environment),
            reported_endpoint_down: false,
            ..RunContext::blank(RunId::EndpointDown)
        };
        let rows = s_r5b_dead_endpoint_still_aborts(&ctx);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].state,
            ScenarioState::Fail,
            "waiting every seat out and leaving with InsufficientAgents is the regression"
        );
    }
}
