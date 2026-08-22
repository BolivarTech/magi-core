// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-21

//! The MS1 x MS2 cross-milestone check (`21-ter`).
//!
//! # Why it lives in a file of its own rather than in either milestone's suite
//!
//! A cross-milestone criterion belongs to neither: MS1 could not run it because MS2 did not
//! exist, and MS2's own list does not carry it. That is precisely how one gets to a release tag
//! without ever having been run.
//!
//! # What it actually covers, stated honestly
//!
//! In its corrected form it verifies a property of **MS1** — completion telemetry records per
//! ATTEMPT, not per model — and none of MS2's. It is kept because the risk it covers belongs to
//! the INTERACTION: that MS2 changes the retry count and nobody notices MS1's telemetry stopped
//! reflecting it.

#![cfg(feature = "test-utils")]

mod common;

use magi_core::schema::AgentName;

/// Two orchestrator calls against the SAME model leave TWO records.
///
/// # The statement was corrected, and the original cannot be verified at all
///
/// It first spoke of a HANG with `limited_max_retries = 1`. That retry is a TRANSPORT one and
/// happens INSIDE `RetryProvider`, which hands a single result back to the orchestrator — from
/// outside there is exactly one completion, however many times it retried internally.
///
/// What does produce two records of the same model is the CORRECTIVE schema retry: two calls
/// made by the orchestrator, both visible. It verifies the same property and is the only one of
/// the two that can be observed.
#[tokio::test]
async fn the_corrective_retry_produces_two_records_against_the_same_model() {
    let r = common::run_where_the_first_response_fails_schema_and_the_retry_succeeds()
        .await
        .expect("the corrective retry succeeds, so the run completes");
    let recs = r
        .completions
        .get(&AgentName::Caspar)
        .expect("the seat that retried is recorded");
    assert_eq!(
        recs.len(),
        2,
        "two ATTEMPTS, not two models: telemetry records per attempt"
    );
    assert_eq!(
        recs[0].model, recs[1].model,
        "the same model both times: it was a corrective retry, not a rotation"
    );
}

/// The counter-example, spelled out so nobody "fixes" the test above by expecting two here.
///
/// A transport hang produces ONE record, and that is correct: `RetryProvider` absorbs its own
/// retries and returns a single result.
#[tokio::test]
#[cfg(feature = "openai-compat")]
async fn a_transport_hang_produces_one_record_and_that_is_correct() {
    let started = std::time::Instant::now();
    let r = common::run_against_a_hanging_backend()
        .await
        .expect("one hung seat degrades the run; it does not abort it");
    let elapsed = started.elapsed();
    let recs = r
        .completions
        .get(&AgentName::Caspar)
        .expect("the hung seat is recorded");
    assert_eq!(
        recs.len(),
        1,
        "the RetryProvider absorbs its own retries: one call from the orchestrator's side"
    );
    // Pins that the absorption ACTUALLY HAPPENED rather than being asserted. The helper wraps a
    // 300 ms client timeout with `limited_max_retries = 1`, so two internal attempts cost more
    // than one. Without this, reclassifying the hang as non-retryable would leave this test green
    // with its own claim unearned — which is how it read before review.
    // DERIVED from the helper's own client timeout rather than hardcoded: two attempts have a
    // hard floor of twice it, and the slack covers scheduling. A literal here would decouple the
    // day someone lowered that timeout, and this test would go red against a correct crate.
    let floor = common::HANGING_SEAT_CLIENT_TIMEOUT * 2 - std::time::Duration::from_millis(100);
    assert!(
        elapsed >= floor,
        "only one attempt was made ({elapsed:?} < {floor:?}), so nothing was absorbed and the claim is unearned"
    );
}
