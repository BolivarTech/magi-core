// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-07-26

//! MS2 rotation integration tests (S2, S4, S6, S7, S10 + W17).
//!
//! Gated on `test-utils` so the file is skipped when the feature is off; the
//! §0.1 gate runs `--all-features` and `run-tests.py` builds with it.
#![cfg(feature = "test-utils")]

use std::sync::Arc;

use magi_core::prelude::*;
use magi_core::test_support::{
    Beh, ScriptProvider, build_schema_local_case, build_trio_with_caspar,
    build_two_5xx_with_local_fallbacks, build_two_failing_with_single_free_fallback,
    build_two_network_failing_no_fallback, report_run_failed,
};

/// Wraps a provider in a `RetryProvider` that exhausts INSTANTLY (zero delay,
/// one retry), so retry-then-rotate composition is exercised without slow sleeps.
/// `RetryConfig` is `#[non_exhaustive]`, so an external crate builds it via
/// `default()` + public-field mutation, not a struct literal.
#[allow(clippy::field_reassign_with_default)]
fn retry0(p: Arc<dyn LlmProvider>) -> Arc<dyn LlmProvider> {
    let mut cfg = RetryConfig::default();
    cfg.max_retries = 1;
    cfg.base_delay = std::time::Duration::ZERO;
    Arc::new(RetryProvider::with_config(p, cfg))
}

#[tokio::test]
async fn test_rotates_on_transport_to_next_lineage() {
    // S2 — Caspar's primary always Network-fails (RetryProvider exhausts) and a
    // fallback lineage is available → rotate; the run is NOT degraded.
    let caspar_primary = retry0(ScriptProvider::new("deepseek", vec![Beh::Network]));
    let fallback_ok = ScriptProvider::new("glm", vec![Beh::Ok]);
    let magi = MagiBuilder::new(ScriptProvider::new("m", vec![Beh::Ok]) as Arc<dyn LlmProvider>)
        .with_agent(
            AgentName::Melchior,
            ScriptProvider::new("q", vec![Beh::Ok]),
            Lineage::new("alibaba"),
        )
        .with_agent(
            AgentName::Balthasar,
            ScriptProvider::new("k", vec![Beh::Ok]),
            Lineage::new("moonshot"),
        )
        .with_agent(AgentName::Caspar, caspar_primary, Lineage::new("deepseek"))
        .with_fallback_pool(
            FallbackPool::builder()
                .push(fallback_ok, Lineage::new("zhipu"))
                .max_rotations(2)
                .build(),
        )
        .build()
        .unwrap();
    let report = magi
        .analyze(&Mode::CodeReview, "content long enough")
        .await
        .unwrap();
    assert!(
        !report.degraded,
        "rotation to fallback should yield a non-degraded run"
    );
    let cas = &report.rotations[&AgentName::Caspar];
    assert_eq!(cas.chain.len(), 1);
    assert_eq!(cas.chain[0].kind(), RotationKind::Transport);
    assert_eq!(*cas.chain[0].to(), Lineage::new("zhipu"));
}

#[tokio::test]
async fn test_retry_then_rotate_on_fallback() {
    // S2 + W17 — Caspar's primary transport-fails (Http 5xx, NON-connection →
    // surfaces immediately) → rotate to fallback A. Fallback A is
    // RetryProvider-wrapped and Network-fails (RETRYABLE → its OWN retry exhausts)
    // → rotate to fallback B (succeeds). Proves the FSM composes with a wrapped
    // fallback and that A's retry actually fired. The primary is 5xx (not Network)
    // so only ONE connection lineage (zhipu) is condemned — below the endpoint-down
    // threshold — isolating the retry-then-rotate property from the abort path
    // (S13 covers 2 distinct connection failures → endpoint-down).
    let caspar_primary = ScriptProvider::new("deepseek", vec![Beh::Http5xx]);
    let fb_a_inner = ScriptProvider::new("glm", vec![Beh::Network]);
    let fb_a = retry0(fb_a_inner.clone());
    let fb_b = ScriptProvider::new("minimax", vec![Beh::Ok]);
    let magi = MagiBuilder::new(ScriptProvider::new("m", vec![Beh::Ok]) as Arc<dyn LlmProvider>)
        .with_agent(
            AgentName::Melchior,
            ScriptProvider::new("q", vec![Beh::Ok]),
            Lineage::new("alibaba"),
        )
        .with_agent(
            AgentName::Balthasar,
            ScriptProvider::new("k", vec![Beh::Ok]),
            Lineage::new("moonshot"),
        )
        .with_agent(AgentName::Caspar, caspar_primary, Lineage::new("deepseek"))
        .with_fallback_pool(
            FallbackPool::builder()
                .push(fb_a, Lineage::new("zhipu"))
                .push(fb_b, Lineage::new("minimax"))
                .max_rotations(2)
                .build(),
        )
        .build()
        .unwrap();
    let report = magi
        .analyze(&Mode::CodeReview, "content long enough")
        .await
        .unwrap();
    assert!(!report.degraded);
    let cas = &report.rotations[&AgentName::Caspar];
    assert_eq!(
        cas.chain.len(),
        2,
        "two transport rotations: primary→A, A→B"
    );
    assert_eq!(*cas.chain[0].to(), Lineage::new("zhipu"));
    assert_eq!(*cas.chain[1].to(), Lineage::new("minimax"));
    assert_eq!(
        cas.chain
            .iter()
            .filter(|e| e.kind() == RotationKind::Transport)
            .count(),
        2
    );
    assert!(
        fb_a_inner.calls() >= 2,
        "fallback A's RetryProvider must have retried before rotation"
    );
    assert_eq!(cas.model_used, "minimax");
}

#[tokio::test]
async fn test_schema_fail_is_mage_local_not_run_wide() {
    // S4 — Caspar schema-fails deepseek then rotates; deepseek is NOT condemned
    // run-wide (schema is mage-local), so it never appears in the transport-hop
    // set derived from telemetry.
    let magi = build_schema_local_case();
    let report = magi.analyze(&Mode::CodeReview, "content").await.unwrap();
    assert_eq!(
        report.rotations[&AgentName::Caspar].chain[0].kind(),
        RotationKind::Schema
    );
    assert!(!report_run_failed(&report).contains(&Lineage::new("deepseek")));
}

#[tokio::test]
async fn test_panic_never_rotates() {
    // S6 — a panicking primary never rotates and surfaces as a failure.
    let caspar = ScriptProvider::new("deepseek", vec![Beh::Panic]);
    let magi = build_trio_with_caspar(caspar, vec![("glm", "zhipu")]);
    let report = magi.analyze(&Mode::CodeReview, "content").await.unwrap();
    assert!(report.rotations[&AgentName::Caspar].chain.is_empty());
    assert!(report.failed_agents.contains_key(&AgentName::Caspar));
}

#[tokio::test]
async fn test_successful_retry_avoids_rotation() {
    // S7 — first attempt BadJson, corrective retry Ok → no rotation.
    let caspar = ScriptProvider::new("deepseek", vec![Beh::BadJson, Beh::Ok]);
    let magi = build_trio_with_caspar(caspar, vec![("glm", "zhipu")]);
    let report = magi.analyze(&Mode::CodeReview, "content").await.unwrap();
    assert!(report.rotations[&AgentName::Caspar].chain.is_empty());
    assert_eq!(report.rotations[&AgentName::Caspar].model_used, "deepseek");
    assert!(report.retried_agents.contains(&AgentName::Caspar));
}

#[tokio::test]
async fn test_short_pool_yields_insufficient_agents_not_collapse() {
    // S10 (W3) — two mages need to rotate, only ONE free fallback lineage → one
    // reserves it, the other fails with `no_fitting_candidate`; never a duplicated
    // lineage / collapsed ensemble.
    let magi = build_two_failing_with_single_free_fallback();
    let result = magi.analyze(&Mode::CodeReview, "content").await;
    match result {
        Err(MagiError::InsufficientAgents { .. }) => {}
        Ok(report) => {
            let losers: Vec<_> = report
                .failed_agents
                .values()
                .filter(|r| r.contains("no_fitting_candidate"))
                .collect();
            assert_eq!(
                losers.len(),
                1,
                "exactly one mage fails to find a free lineage; the other reserved it"
            );
        }
        other => panic!("unexpected: {other:?}"),
    }
}

#[tokio::test]
async fn test_two_connection_failures_abort_with_endpoint_down() {
    // S13 — two DISTINCT lineages Network-fail (connection) with no free fallback
    // → the endpoint-down latch fires → the run aborts pre-consensus.
    let magi = build_two_network_failing_no_fallback();
    let result = magi.analyze(&Mode::CodeReview, "content").await;
    assert!(
        matches!(result, Err(MagiError::EndpointDown { .. })),
        "two connection failures must abort pre-consensus, got: {result:?}"
    );
}

#[tokio::test]
async fn test_5xx_condemns_but_does_not_abort_endpoint_down() {
    // S14 — two lineages Http 5xx (NOT connection) + local fallbacks → NO
    // endpoint-down; the mages rotate and the run proceeds.
    let magi = build_two_5xx_with_local_fallbacks();
    let report = magi.analyze(&Mode::CodeReview, "content").await.unwrap();
    assert!(
        report.rotations.values().any(|r| !r.chain.is_empty()),
        "5xx condemns lineages but the run proceeds by rotating"
    );
}
