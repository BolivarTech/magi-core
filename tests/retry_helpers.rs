// Author: Julian Bolivar
// Version: 4.0.0
// Date: 2026-08-23

//! Tests for the shared retry-budget test helpers (MS2, Task 1a).
//!
//! Only `captured_warnings` is tested here, and that is deliberate: it is the one helper of the
//! five that can fail SILENTLY. Returning an empty `Vec` reads as "no warnings were emitted", so
//! every test built on it would pass while it captured nothing. The other four either return a
//! `Magi` (which fails to build if wrong) or a `Result` the caller asserts on directly.
//!
//! The second test is the one that matters most: `dangerous_settings()` can be perfectly correct
//! and called by nobody. It joins the two halves.

#![cfg(feature = "test-utils")]

mod common;

use magi_core::provider::{RetryConfig, RetryProvider};
use magi_core::test_support::ScriptProvider;
use std::sync::Arc;
use std::time::Duration;

use common::captured_warnings;

#[test]
fn captured_warnings_sees_a_warning_that_was_emitted() {
    // A helper that always returns empty makes every test that uses it pass. It is verified
    // against a `warn!` we know was emitted.
    let w = captured_warnings(|| tracing::warn!("canary"));
    assert!(
        w.iter().any(|s| s.contains("canary")),
        "captured_warnings returned {w:?}, which does not contain the canary it was given"
    );
}

#[test]
fn captured_warnings_stays_empty_when_nothing_warns() {
    // The mirror of the test above. Without it, a helper that returned a fixed non-empty vector
    // would also pass, and "sees a warning" would be true for the wrong reason.
    let w = captured_warnings(|| {});
    assert!(w.is_empty(), "expected no warnings, got {w:?}");
}

#[test]
fn with_config_actually_emits_the_dangerous_settings_warnings() {
    // `dangerous_settings()` can be flawless and called by nobody. This test is the one that
    // joins the two halves.
    //
    // The call site is `RetryProvider::with_config`, NOT `MagiBuilder::build`: the retry
    // configuration is validated where it enters, and grepping `build()` finds nothing.
    let w = captured_warnings(|| {
        // `RetryConfig` has no builder methods, by design — its own rustdoc documents adjusting
        // fields over `Default`.
        let mut c = RetryConfig::default();
        c.base_delay = Duration::ZERO;
        let inner: Arc<dyn magi_core::provider::LlmProvider> =
            ScriptProvider::new("canary-model", vec![magi_core::test_support::Beh::Ok]);
        let _ = RetryProvider::with_config(inner, c);
    });
    assert!(
        w.iter().any(|s| s.contains("base_delay")),
        "dangerous_settings() exists but nothing calls it; captured: {w:?}"
    );
}
