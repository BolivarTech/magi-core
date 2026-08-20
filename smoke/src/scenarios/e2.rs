// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-20

//! The E2 scenarios: the ones that verify what `4.0.0` changed.
//!
//! They are written with the milestone that introduces the property, not before it, and not
//! after. Written before, they sit red for weeks and a permanently red harness gets rationalised
//! away — which is the `3.0.2` failure wearing different clothes. Written after, the property
//! ships uncertified. Written alongside, the scenario IS the test.

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

/// The E2 scenario table.
pub fn e2_scenarios() -> Vec<Scenario> {
    vec![Scenario {
        id: "S8",
        // The small happy run is enough: routing is a property of every completion, not of a
        // large payload. Reading it here also keeps S8 off the slow run's critical path.
        source: Source::Run(RunId::HappySmall),
        backend_tag: BackendNeed::Required,
        assert_fn: s8_completions_are_native_only,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_carries_exactly_the_scenarios_this_stage_implements() {
        let s = e2_scenarios();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].id, "S8");
    }

    #[test]
    fn the_legacy_path_named_here_is_not_the_one_the_crate_now_uses() {
        // If someone ever points `COMPLETIONS_PATH` back at `/v1`, the two assertions of this
        // scenario would contradict each other silently — one requiring records on that path,
        // the other requiring none. Pinning them as distinct makes that a test failure instead.
        assert_ne!(COMPLETIONS_PATH, LEGACY_COMPAT_PATH);
        assert_eq!(COMPLETIONS_PATH, "/api/chat");
    }
}
