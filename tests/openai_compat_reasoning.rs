// Author: Julian Bolivar
// Version: 4.0.0
// Date: 2026-08-23
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
            ReasoningState::Unsupported { ref backend, .. } if backend == "openai-compatible"
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
/// — the fragility this project rejected when `magi-rs` proposed detecting the
/// empty-content failure by the text of its message.
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
        ReasoningState::Unsupported { backend, .. } => backend,
        other => panic!("expected Unsupported, got {other:?}"),
    };
    assert_eq!(backend, "openai-compatible");
    handle.abort();
}

/// A real `/v1` capture: the model converged, so content, `finish_reason`,
/// `usage` and the reasoning channel are all present.
const FIX_H: &str = include_str!("fixtures/ec/resp-H.json");

/// The same model against the 62k payload: it burned its whole budget
/// reasoning and emitted nothing — HTTP 200, `finish_reason: "length"`,
/// empty content. The shape this whole release exists for.
const FIX_C: &str = include_str!("fixtures/ec/resp-C.json");

/// Deserializing the fields is worth nothing if `complete()` does not carry
/// them out. This is the end-to-end half: a captured body in, telemetry out.
#[tokio::test]
async fn the_telemetry_of_a_captured_success_survives_the_whole_call() {
    let (url, _captured, handle) = mock_server::spawn_capturing(200, FIX_H).await;
    let provider = OpenAiCompatibleProvider::new(url, "m", None).expect("constructs");

    let out = provider
        .complete("s", "u", &CompletionConfig::default())
        .await
        .expect("the captured body parses");

    assert!(!out.text.is_empty());
    assert_eq!(out.telemetry.finish, Some(FinishReason::Stop));
    assert_eq!(out.telemetry.completion_tokens, Some(1280));
    assert_eq!(out.telemetry.prompt_tokens, Some(569));
    assert!(matches!(
        out.telemetry.reasoning,
        ReasoningState::Measured {
            chars: 3535,
            text: None
        }
    ));
    handle.abort();
}

/// B-1/B-2: the head of the chain, observed from the outside. A budget-exhausted
/// completion used to become `Http { status: 0 }` — a contract failure wearing an
/// HTTP error's clothes, and that disguise is how it inherited the run-wide
/// condemnation that pulled a healthy lineage out of the other two mages' pools.
#[tokio::test]
async fn a_budget_exhausted_completion_is_named_for_what_it_is_and_carries_no_http_status() {
    let (url, _captured, handle) = mock_server::spawn_capturing(200, FIX_C).await;
    let provider = OpenAiCompatibleProvider::new(url, "m", None).expect("constructs");

    let err = provider
        .complete("s", "u", &CompletionConfig::default())
        .await
        .expect_err("an empty completion is a failure");

    let ProviderError::EmptyCompletion { telemetry, .. } = &err else {
        panic!("expected EmptyCompletion carrying the termination, got {err:?}");
    };
    assert_eq!(telemetry.finish, Some(FinishReason::Length));
    // Observed from OUTSIDE the crate, through `complete()`, which is the only view a
    // consumer has: the measurement that explains the cut must survive all the way out,
    // not merely exist inside the parser.
    assert_eq!(telemetry.completion_tokens, Some(4096));
    assert!(
        matches!(telemetry.reasoning, ReasoningState::Measured { chars, .. } if chars > 10_000),
        "the burned reasoning must reach the caller, got {:?}",
        telemetry.reasoning
    );
    assert!(
        !matches!(err, ProviderError::Http { .. }),
        "a contract failure must never wear an HTTP error's clothes: {err:?}"
    );
    handle.abort();
}

/// An unreadable body is the ONE contract failure that is retryable, because a
/// body can be unreadable for having been truncated in transit. It is still not
/// an `Http` error: nothing about it carries a real status.
#[tokio::test]
async fn an_unreadable_body_is_a_contract_failure_not_a_synthetic_http_status() {
    let (url, _captured, handle) = mock_server::spawn_capturing(200, "not json").await;
    let provider = OpenAiCompatibleProvider::new(url, "m", None).expect("constructs");

    let err = provider
        .complete("s", "u", &CompletionConfig::default())
        .await
        .expect_err("an unreadable body is a failure");

    assert!(
        matches!(
            err,
            ProviderError::ResponseContract {
                reason: ResponseContractCause::Unreadable,
                ..
            }
        ),
        "expected ResponseContract{{Unreadable}}, got {err:?}"
    );
    handle.abort();
}
