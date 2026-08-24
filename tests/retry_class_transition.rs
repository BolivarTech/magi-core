// Author: Julian Bolivar
// Version: 4.0.0
// Date: 2026-08-23

//! The attempt cap follows the class of the error that JUST happened (MS2, Task 1).
//!
//! The unit tests cover `attempts_for`; this covers the LOOP, which is where the field changes
//! behaviour. Resolving the class once on entry would pass every unit test and fail here.

#![cfg(feature = "openai-compat")]

mod support;

use magi_core::prelude::*;
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn the_limit_follows_the_class_of_the_error_that_just_happened() {
    // The chain opens with a `429` — not attempt-limited, four attempts — and meets a hang on
    // the second. The cap that governs is the one for what IS happening, so it abandons on the
    // second attempt rather than the fourth. Resolving the class once on entry would give four,
    // and that is the defect this test exists to catch.
    let (url, handle, served) = support::mock_server::spawn_429_then_hang().await;

    // 200 ms rather than the 300 s default: the hang is ended by the CLIENT, and this task does
    // not change that default.
    let inner = OpenAiCompatibleProvider::with_timeout(url, "m", None, Duration::from_millis(200))
        .expect("provider builds");

    let mut cfg = RetryConfig::default();
    // So the backoff cannot dominate the clock either.
    cfg.base_delay = Duration::from_millis(1);

    let p = RetryProvider::with_config(Arc::new(inner) as Arc<dyn LlmProvider>, cfg);
    let err = p
        .complete("s", "u", &CompletionConfig::default())
        .await
        .expect_err("every attempt failed");

    let log = served
        .lock()
        .expect("the server task never panics while holding it")
        .clone();

    // The TRANSITION, which the count alone cannot show: the chain opened on a 429 — a class that
    // is NOT attempt-limited — and met a hang on the second. Without this, two hangs would give
    // the same count and the same final error, so the test would pass against a chain that never
    // changed class: the very defect it is named for.
    assert_eq!(
        log,
        vec!["429", "hang"],
        "the chain must actually change class, not merely stop at two"
    );
    assert_eq!(
        log.len(),
        2,
        "abandons on the Timeout cap, not the general one"
    );
    assert!(
        matches!(err, ProviderError::Timeout { .. }),
        "the last error is the hang, not the 429 that started the chain: {err}"
    );
    handle.abort();
}
