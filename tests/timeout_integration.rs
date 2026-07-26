// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-07-25
#![cfg(feature = "openai-compat")]

mod support;
use magi_core::prelude::*;
use std::time::Duration;
use support::mock_server;

/// S16: a model that returns headers and then hangs on the body must produce a
/// total-timeout error — a connect-timeout would never fire here.
#[tokio::test]
async fn test_hanging_body_produces_timeout_error() {
    let (url, handle) = mock_server::spawn_hanging_headers().await;
    let provider =
        OpenAiCompatibleProvider::with_timeout(url, "test-model", None, Duration::from_millis(300))
            .expect("provider");

    let err = provider
        .complete("system", "user", &CompletionConfig::default())
        .await
        .expect_err("must time out");

    assert!(
        matches!(err, ProviderError::Timeout { .. }),
        "a connect-timeout would not have fired; expected Timeout, got: {err}"
    );
    handle.abort();
}
