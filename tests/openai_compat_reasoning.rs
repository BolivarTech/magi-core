// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-20
#![cfg(feature = "openai-compat")]

//! C-8: a provider that cannot honour `ReasoningControl::Disabled` must
//! DECLARE that in its telemetry instead of ignoring the request in silence.
//! `think: false` was measured accepted-with-HTTP-200-and-no-effect on this
//! wire (evidence run G) — the forbidden thing was never "carry on", it was
//! carrying on in silence, which is exactly the silent no-op this crate
//! already named in its own backlog.
//!
//! Lives here rather than in `src/providers/openai_compat.rs` because it
//! exercises `complete()` end to end against a real socket — the mock server
//! lives under `tests/support/`, which `src/` cannot reach.

mod support;
use magi_core::prelude::*;
use support::mock_server;

/// A minimal, valid OpenAI Chat Completions success body.
const OK_BODY: &str = r#"{"choices":[{"message":{"content":"a verdict"}}]}"#;

/// C-8: `Disabled` cannot be honoured on this wire, so it is DECLARED.
#[tokio::test]
async fn a_provider_that_cannot_honour_the_control_says_so_and_does_not_break_the_run() {
    let (url, _captured, handle) = mock_server::spawn_capturing(200, OK_BODY).await;
    let provider = OpenAiCompatibleProvider::new(url, "m", None).expect("constructs");

    let cfg = CompletionConfig::default().with_reasoning(ReasoningControl::Disabled);
    let out = provider
        .complete("s", "u", &cfg)
        .await
        .expect("the canned body parses");

    assert!(
        matches!(
            out.telemetry.reasoning,
            ReasoningState::Unsupported { ref backend } if backend == "openai-compatible"
        ),
        "expected Unsupported{{backend: \"openai-compatible\"}}, got {:?}",
        out.telemetry.reasoning
    );
    handle.abort();
}

/// The other half of C-8: with nothing asked, nothing is declared, and there
/// is no trace channel this provider reads regardless — so this must be
/// `NotMeasured`, never `Measured { chars: 0 }`.
#[tokio::test]
async fn the_default_control_asks_for_nothing_and_declares_nothing() {
    let (url, _captured, handle) = mock_server::spawn_capturing(200, OK_BODY).await;
    let provider = OpenAiCompatibleProvider::new(url, "m", None).expect("constructs");

    let out = provider
        .complete("s", "u", &CompletionConfig::default())
        .await
        .expect("the canned body parses");

    assert_eq!(
        out.telemetry.reasoning,
        ReasoningState::NotMeasured,
        "no control was asked, so nothing is declared either"
    );
    handle.abort();
}

/// The declaration is a TYPE the caller matches on, never prose it must grep
/// — the fragility this project rejected when `magi-rs` proposed detecting
/// `EMPTY_CONTENT_BODY` by its message text.
#[tokio::test]
async fn the_declaration_is_a_type_not_a_string_a_consumer_must_grep() {
    let (url, _captured, handle) = mock_server::spawn_capturing(200, OK_BODY).await;
    let provider = OpenAiCompatibleProvider::new(url, "m", None).expect("constructs");

    let cfg = CompletionConfig::default().with_reasoning(ReasoningControl::Disabled);
    let out = provider
        .complete("s", "u", &cfg)
        .await
        .expect("the canned body parses");

    let backend: String = match out.telemetry.reasoning {
        ReasoningState::Unsupported { backend } => backend,
        other => panic!("expected Unsupported, got {other:?}"),
    };
    assert_eq!(backend, "openai-compatible");
    handle.abort();
}
