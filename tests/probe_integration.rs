// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-07-26

//! MS2 probe/verify integration tests (G2 trio diversity warning, R19 honesty).
//!
//! Gated on `test-utils`; the §0.1 gate runs `--all-features`.
#![cfg(feature = "test-utils")]

use std::sync::Arc;

use magi_core::prelude::*;
use magi_core::test_support::{Beh, MockProbe, ScriptProvider};

#[tokio::test]
async fn test_trio_digest_collision_warns_not_errors() {
    // G2 — two probing primaries resolve to the SAME weights digest. The preflight
    // WARNS (reduced diversity) but analyze() still succeeds: diversity never blocks
    // the run.
    let same = "sha256:identical";
    let magi =
        MagiBuilder::new(ScriptProvider::new("default", vec![Beh::Ok]) as Arc<dyn LlmProvider>)
            .with_probing_agent(
                AgentName::Melchior,
                MockProbe::with_digest("m1", same),
                Lineage::new("alibaba"),
            )
            .with_probing_agent(
                AgentName::Balthasar,
                MockProbe::with_digest("m2", same),
                Lineage::new("moonshot"),
            )
            .with_agent(
                AgentName::Caspar,
                ScriptProvider::new("c", vec![Beh::Ok]),
                Lineage::new("deepseek"),
            )
            .build()
            .unwrap();
    let result = magi.analyze(&Mode::CodeReview, "content").await;
    assert!(
        result.is_ok(),
        "trio digest collision must WARN, not error — diversity never blocks the run (G2)"
    );
}

#[tokio::test]
async fn test_ran_unmeasured_marks_estimated() {
    // S22 — Caspar's committed model has an UNKNOWN window (probe returns None) →
    // `ran_unmeasured` is set and the run-level report discloses "estimated".
    let caspar = MockProbe::with_window("deepseek", None);
    let magi =
        MagiBuilder::new(ScriptProvider::new("default", vec![Beh::Ok]) as Arc<dyn LlmProvider>)
            .with_agent(
                AgentName::Melchior,
                ScriptProvider::new("m", vec![Beh::Ok]),
                Lineage::new("alibaba"),
            )
            .with_agent(
                AgentName::Balthasar,
                ScriptProvider::new("b", vec![Beh::Ok]),
                Lineage::new("moonshot"),
            )
            .with_probing_agent(AgentName::Caspar, caspar, Lineage::new("deepseek"))
            .build()
            .unwrap();
    let report = magi.analyze(&Mode::CodeReview, "content").await.unwrap();
    assert!(
        report.rotations[&AgentName::Caspar].ran_unmeasured,
        "an unmeasured window must set ran_unmeasured"
    );
    assert!(
        report.report.contains("estimated"),
        "run-level report must disclose 'estimated'"
    );
}
