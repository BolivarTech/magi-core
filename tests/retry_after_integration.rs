// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-07-25
#![cfg(feature = "openai-compat")]

//! End-to-end `Retry-After` tests: a real 429 with the header, through the
//! provider (which populates `retry_after_raw`/`received_at`) and the retry loop
//! (which interprets it). This is the scenario that justifies the 2.0 major.

mod support;
use magi_core::prelude::*;
use std::sync::Arc;
use std::time::{Duration, Instant};
use support::mock_server;

/// S11: a real 429 carrying `Retry-After: 2` makes the loop wait ~2 s (the
/// server's value), not the ~1 s formula, then succeed on the retry.
#[tokio::test]
async fn test_real_429_with_header_waits_what_the_server_asked() {
    let (url, handle) = mock_server::spawn_429_with_retry_after("2").await;
    let inner = Arc::new(OpenAiCompatibleProvider::new(url, "test-model", None).expect("provider"));
    let retry = RetryProvider::new(inner);

    let start = Instant::now();
    let out = retry
        .complete("system", "user", &CompletionConfig::default())
        .await
        .expect("the second attempt must succeed");
    let elapsed = start.elapsed();

    assert!(out.contains("ok"));
    // >= 1800ms proves the ~2 s server header was honored (the formula, base 1 s,
    // would wait <= ~1 s). Not `== 2 s`: the C3.1 discount shaves a few ms and the
    // jitter adds up to 1 s, so a lower bound below 2 s is the robust check.
    assert!(
        elapsed >= Duration::from_millis(1800),
        "must honor the server's ~2s, waited {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(6),
        "should not wait much more (2s + jitter <=1s + runtime): {elapsed:?}"
    );
    handle.abort();
}

/// S21: a `Retry-After` in date form (out of scope) ABANDONS the retry with a
/// typed reason — it does not fall to the formula (that would retry sooner than
/// the server asked).
#[tokio::test]
async fn test_unintelligible_header_abandons_end_to_end() {
    let (url, handle) =
        mock_server::spawn_429_with_retry_after("Sun, 06 Nov 1994 08:49:37 GMT").await;
    let inner = Arc::new(OpenAiCompatibleProvider::new(url, "test-model", None).expect("provider"));
    let retry = RetryProvider::new(inner);

    let err = retry
        .complete("system", "user", &CompletionConfig::default())
        .await
        .expect_err("a date-form header must abandon");

    assert!(
        matches!(
            err,
            ProviderError::RetryAbandoned {
                reason: AbandonReason::RetryAfterUnintelligible { .. },
                ..
            }
        ),
        "expected unintelligible abandonment, got: {err}"
    );
    handle.abort();
}

/// S14: `Retry-After: 0` is treated as absent and falls to the jittered formula
/// (never a synchronized flood at instant zero); the retry then succeeds.
#[tokio::test]
async fn test_retry_after_zero_falls_back_to_jittered_formula() {
    let (url, handle) = mock_server::spawn_429_with_retry_after("0").await;
    let inner = Arc::new(OpenAiCompatibleProvider::new(url, "test-model", None).expect("provider"));
    let retry = RetryProvider::new(inner);

    let start = Instant::now();
    let out = retry
        .complete("system", "user", &CompletionConfig::default())
        .await
        .expect("the retry must succeed");
    let elapsed = start.elapsed();

    assert!(out.contains("ok"));
    // The formula wait (base 1 s, jittered <= 1 s) is short — clearly not the
    // honored path and not an instant-zero flood.
    assert!(
        elapsed < Duration::from_secs(3),
        "expected the short formula wait, got {elapsed:?}"
    );
    handle.abort();
}
