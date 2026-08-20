// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-20
#![cfg(feature = "ollama")]

//! The native `/api/chat` completion path, end to end against a real socket.
//!
//! These live here rather than in `src/providers/ollama.rs` because what they assert is what
//! went ON THE WIRE — the path and the request body — and that is only observable from a server
//! that records it. The mock server lives under `tests/support/`, which `src/` cannot reach.

mod support;
use magi_core::prelude::*;
use support::mock_server;

/// A minimal native success body: the shape `/api/chat` actually returns.
const NATIVE_OK: &str = r#"{"model":"m","created_at":"2026-08-20T00:00:00Z",
"message":{"role":"assistant","content":"a verdict"},
"done":true,"done_reason":"stop","prompt_eval_count":11,"eval_count":7}"#;

/// C-1: every completion goes to `/api/chat`, and `stream` is explicit.
///
/// `stream` is asserted because omitting it is not neutral — `/api/chat` streams by DEFAULT when
/// the field is absent, and a streamed body would arrive as newline-delimited chunks that the
/// parser is not written for. It is the kind of default that fails as a parse error three layers
/// away from its cause.
#[tokio::test]
async fn every_completion_goes_to_api_chat_with_stream_false() {
    let (url, captured, handle) = mock_server::spawn_capturing(200, NATIVE_OK).await;
    let provider = OllamaProvider::new(&url, "m").expect("constructs");

    let completion = provider
        .complete("sys", "user", &CompletionConfig::default())
        .await
        .expect("the canned native body parses");
    assert_eq!(completion.text, "a verdict");

    let req = captured.lock().expect("not poisoned");
    let req = req.as_ref().expect("the server recorded a request");
    assert_eq!(
        req.path, "/api/chat",
        "no conditional routing: the /v1 compatibility path is gone"
    );
    assert_eq!(req.body["stream"], false);
    assert_eq!(req.body["options"]["num_predict"], 16_384);
    handle.abort();
}

/// C-7: `Default` means SAY NOTHING, not `think: true`.
///
/// A crate that started asserting a control nobody asked for would change behaviour for every
/// consumer who never mentioned reasoning — which is exactly the silent behaviour change this
/// milestone exists to stop making.
#[tokio::test]
async fn the_default_reasoning_control_puts_no_think_field_on_the_wire() {
    let (url, captured, handle) = mock_server::spawn_capturing(200, NATIVE_OK).await;
    let provider = OllamaProvider::new(&url, "m").expect("constructs");

    let _ = provider
        .complete("sys", "user", &CompletionConfig::default())
        .await;

    let req = captured.lock().expect("not poisoned");
    let req = req.as_ref().expect("the server recorded a request");
    assert!(
        req.body.get("think").is_none(),
        "Default must omit the field entirely, got {}",
        req.body
    );
    handle.abort();
}

/// C-7 again, the other half: `Disabled` DOES put it on the wire.
///
/// Without this the previous test would pass against a provider that can never send the control
/// at all — an omission is indistinguishable from an absence unless the presence is also pinned.
#[tokio::test]
async fn disabling_reasoning_puts_think_false_on_the_wire() {
    let (url, captured, handle) = mock_server::spawn_capturing(200, NATIVE_OK).await;
    let provider = OllamaProvider::new(&url, "m").expect("constructs");

    let cfg = CompletionConfig::default().with_reasoning(ReasoningControl::Disabled);
    let _ = provider.complete("sys", "user", &cfg).await;

    let req = captured.lock().expect("not poisoned");
    let req = req.as_ref().expect("the server recorded a request");
    assert_eq!(req.body["think"], false);
    handle.abort();
}

/// The native path REUSES the hardening of 3.1.0 instead of re-implementing it.
///
/// The body cap is derived from `max_tokens` by the shared reader, so a provider that built its
/// own client and read its own body would silently lose it — which is how this crate once ended
/// up with one provider reading unbounded bodies while the changelog said all of them were
/// bounded. Asking for a body over the cap is the cheapest way to prove the shared reader is the
/// one in use.
#[tokio::test]
async fn an_oversized_native_body_is_refused_by_the_shared_reader() {
    let huge = format!(
        r#"{{"model":"m","message":{{"role":"assistant","content":"{}"}},"done":true,"done_reason":"stop"}}"#,
        "x".repeat(2 * 1024 * 1024)
    );
    let (url, _captured, handle) = mock_server::spawn_capturing(200, &huge).await;
    let provider = OllamaProvider::new(&url, "m").expect("constructs");

    let err = provider
        .complete("sys", "user", &CompletionConfig::default())
        .await
        .expect_err("over the cap");
    assert!(
        matches!(err, ProviderError::ResponseTooLarge { .. }),
        "expected the shared reader's refusal, got {err:?}"
    );
    handle.abort();
}
