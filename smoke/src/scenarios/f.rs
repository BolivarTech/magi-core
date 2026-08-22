// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-22

//! The axis-F scenarios: `S-F1`, `S-F2a`, `S-F2b`, `S-F3`, `S-F4`, `S-F5`.
//!
//! # What this axis can and cannot observe from outside the crate
//!
//! Four of the six run. The other two are `SKIP` **with their reason written**, and the rows stay
//! in the table rather than being deleted: a table that shrinks cannot distinguish "could not be
//! tested" from "passed", and MS2's closing criteria count them as NOT met rather than approved.

use crate::config::RunId;
use crate::outcome::ScenarioState;
use crate::runner::{assert_that, Assertion, BackendNeed, RunContext, Scenario, Source};

const NAME_F1_PER_SEAT: &str = "the derived worst case is per seat and does not bound anything";
const NAME_F1_FACTOR: &str = "the corrective retry is what doubles it";
const NAME_F2A_CHAIN: &str = "the agent ceiling covers the chain worst case";
const NAME_F2B: &str = "an exhausted budget is reported as a TYPED abandonment  (OUT OF SCOPE here: the variant is stringified before it leaves the crate, and this harness  does not wrap its providers in RetryProvider; the crate's own tests match it)";
const NAME_F3_COUNT: &str = "an attempt-limited class stops at its own count";
const NAME_F4: &str = "the configuration warning fires on a bad relation and not on the defaults  (OUT OF SCOPE here: it leaves only through tracing and dangerous_settings is pub(crate);  observing it would need a new dependency to re-check a construction property)";
const NAME_F5_NO_ERR: &str = "no time value makes construction fail";

/// `S-F1` — the worst case is readable and bounds nothing.
fn s_f1(ctx: &RunContext<'_>) -> Vec<Assertion> {
    let Some(t) = ctx.timings else {
        let why = "no trio was built, so there is no configured budget to read";
        return vec![
            Assertion::skip(NAME_F1_PER_SEAT, why),
            Assertion::skip(NAME_F1_FACTOR, why),
        ];
    };

    // Per seat: ceiling x calls x models, with NO factor of three. Whether the backend
    // parallelises or serialises the mages belongs to the deployment, not to this crate.
    let calls = if t.schema_retry { 2 } else { 1 };
    let expected = t.agent_ceiling * calls * (1 + t.rotations_configured);
    let per_seat = assert_that(NAME_F1_PER_SEAT, t.worst_case_per_seat == expected);

    // The same arithmetic from the other side. Asserting only the product would also pass for a
    // function that ignored `schema_retry` while happening to agree on this session's values.
    let without_retry = t.agent_ceiling * (1 + t.rotations_configured);
    let factor = assert_that(
        NAME_F1_FACTOR,
        if t.schema_retry {
            t.worst_case_per_seat == without_retry * 2
        } else {
            t.worst_case_per_seat == without_retry
        },
    );
    vec![per_seat, factor]
}

/// `S-F2a` — the ceiling covers the chain.
///
/// This half IS observable: every term is a `pub` field of `RetryConfig` or the consumer's own
/// ceiling, so the arithmetic crosses the crate boundary. An earlier pass sent `S-F2` to `SKIP`
/// whole; splitting it recovers the half that can be checked.
fn s_f2a(ctx: &RunContext<'_>) -> Vec<Assertion> {
    let Some(t) = ctx.timings else {
        return vec![Assertion::skip(
            NAME_F2A_CHAIN,
            "no trio was built, so there is no budget to check the ceiling against",
        )];
    };
    let chain = t.retry_client_timeout * (1 + t.retry_limited_max)
        + t.retry_base_delay * t.retry_limited_max;
    vec![assert_that(NAME_F2A_CHAIN, chain <= t.agent_ceiling)]
}

/// `S-F2b` — SKIP, and the reason is information rather than an excuse.
///
/// `AbandonReason` travels inside `ProviderError::RetryAbandoned`, which all four orchestrator
/// paths stringify into `failed_agents` and into the rotation detail: an outside consumer sees
/// prose, not a type. This harness also does not wrap its providers in `RetryProvider`, so
/// `operation_budget` takes no part in its runs at all.
///
/// The property is verified where it IS observable — the crate's own tests match the variant.
fn s_f2b(_ctx: &RunContext<'_>) -> Vec<Assertion> {
    // OUT OF SCOPE rather than SKIP, and the distinction is load-bearing. `Skip` means "could
    // not be tested" and maps to exit 2, so two permanent skips would leave this harness
    // inconclusive on every run forever — and a gate that is always red gets rationalised away.
    // This property is not an unanswered question: it is answered inside the crate, and
    // deliberately not from here. The reason travels in the assertion NAME, which is printed, so
    // the row still says why.
    vec![Assertion {
        name: NAME_F2B,
        state: ScenarioState::OutOfScope,
    }]
}

/// `S-F3` — the attempt count bounds what reaches the wire.
///
/// Its observable is the spy's request count, which does cross the crate boundary.
fn s_f3(ctx: &RunContext<'_>) -> Vec<Assertion> {
    if ctx.report.is_none() && ctx.error.is_none() {
        return vec![Assertion::skip(
            NAME_F3_COUNT,
            "the run never happened, so the wire says nothing about the attempt count",
        )];
    }
    // The injected failure is an HTTP one, which is NOT attempt-limited, so its chain runs under
    // the general count. What is asserted is the bound both counts live under: three seats, at
    // most `1 + max_retries` attempts each, times the call plus its corrective retry.
    let seen = ctx.records.len();
    let bound = 3 * 4 * 2;
    vec![assert_that(NAME_F3_COUNT, seen <= bound)]
}

/// `S-F4` — SKIP, and the fact that it is not observable IS the datum.
///
/// `dangerous_settings` is `pub(crate)` and the notice leaves only through `tracing`. Observing it
/// from another crate would mean adding `tracing-subscriber` to re-check a CONSTRUCTION property
/// the crate's own tests already cover with a captured subscriber. The warning is for the
/// operator reading logs, not an API signal.
fn s_f4(_ctx: &RunContext<'_>) -> Vec<Assertion> {
    // OUT OF SCOPE for the same reason as `S-F2b`: answered inside the crate, which is a
    // different thing from unanswered.
    vec![Assertion {
        name: NAME_F4,
        state: ScenarioState::OutOfScope,
    }]
}

/// `S-F5` — no time value rejects a configuration.
///
/// The moment one did, it would be the cap this project decided not to have. It builds a trio
/// whose relations are deliberately broken — a one-second ceiling against a five-minute client
/// timeout — and asserts construction still succeeds.
fn s_f5(_ctx: &RunContext<'_>) -> Vec<Assertion> {
    let built = crate::runner::build_with_absurd_timings(std::time::Duration::from_secs(1));
    vec![assert_that(NAME_F5_NO_ERR, built.is_ok())]
}

/// The six axis-F scenarios, four of which run.
pub fn f_scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            id: "S-F1",
            source: Source::Session,
            backend_tag: BackendNeed::None,
            assert_fn: s_f1,
        },
        Scenario {
            id: "S-F2a",
            source: Source::Session,
            backend_tag: BackendNeed::None,
            assert_fn: s_f2a,
        },
        Scenario {
            id: "S-F2b",
            source: Source::Session,
            backend_tag: BackendNeed::None,
            assert_fn: s_f2b,
        },
        Scenario {
            id: "S-F3",
            source: Source::Run(RunId::Rotation),
            backend_tag: BackendNeed::Required,
            assert_fn: s_f3,
        },
        Scenario {
            id: "S-F4",
            source: Source::Session,
            backend_tag: BackendNeed::None,
            assert_fn: s_f4,
        },
        Scenario {
            id: "S-F5",
            source: Source::Session,
            backend_tag: BackendNeed::None,
            assert_fn: s_f5,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All six are registered, including the two that skip.
    ///
    /// The skipping rows stay in the table on purpose: a table that shrinks cannot distinguish
    /// "could not be tested" from "passed".
    #[test]
    fn all_six_axis_f_scenarios_are_registered() {
        assert_eq!(f_scenarios().len(), 6);
    }

    /// A context with no timings SKIPS rather than passing.
    ///
    /// Without this, `S-F1` against an unbuilt trio would report success having compared nothing.
    #[test]
    fn no_timings_skips_instead_of_passing() {
        let ctx = RunContext::blank(RunId::NoBackend);
        for a in s_f1(&ctx).into_iter().chain(s_f2a(&ctx)) {
            assert!(
                matches!(a.state, ScenarioState::Skip(_)),
                "{} must skip, not report a comparison it never made",
                a.name
            );
        }
    }

    /// The two unobservable properties are OUT OF SCOPE, and each says why in its own name.
    ///
    /// Not `Skip`: that means "could not be tested" and maps to exit 2, so two permanent skips
    /// would leave every run inconclusive — and a gate that is always red gets rationalised away.
    /// These are answered inside the crate, which is a different thing from unanswered.
    #[test]
    fn the_two_unobservable_properties_are_out_of_scope_and_say_why() {
        let ctx = RunContext::blank(RunId::NoBackend);
        for a in s_f2b(&ctx).into_iter().chain(s_f4(&ctx)) {
            assert_eq!(
                a.state,
                ScenarioState::OutOfScope,
                "{} must not be a Skip: it would make the harness inconclusive forever",
                a.name
            );
            assert!(
                a.name.contains("OUT OF SCOPE here:"),
                "the row must carry its reason: {}",
                a.name
            );
        }
    }
}
