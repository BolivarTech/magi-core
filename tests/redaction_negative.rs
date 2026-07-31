// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-07-30

//! The negative test that defines this milestone.
//!
//! A test that only checks the clean case proves nothing about a leak: it must build a provider
//! **with** credentials, provoke each class of failure, and assert the secrets appear in **none**
//! of the channels a report travels through.
//!
//! Three channels are covered — `Debug`, the error's `Display`, and the **serialized report**, which
//! is the one that matters most because it is what people paste into tickets and chats.
//!
//! No network: port 1 on loopback refuses immediately.

//! Requires the OpenAI-compatible provider: it is the one that accepts a caller-supplied
//! `base_url`, so it is the only place a credential can enter a URL at all.
#![cfg(feature = "openai-compat")]

use magi_core::prelude::*;
#[cfg(feature = "ollama")]
use magi_core::rotation::ProviderProbe;

const USER: &str = "aliceUser";
const PASS: &str = "s3cretPass";
const QKEY: &str = "q3rySecret";

/// Asserts that none of the three secrets appear in `haystack`.
fn assert_clean(haystack: &str, channel: &str) {
    for needle in [USER, PASS, QKEY] {
        assert!(
            !haystack.contains(needle),
            "{channel} leaked {needle}:\n{haystack}"
        );
    }
}

fn provider_with(url: String) -> OpenAiCompatibleProvider {
    OpenAiCompatibleProvider::new(url, "some-model", None).expect("constructs")
}

#[test]
fn userinfo_never_appears_in_debug() {
    let p = provider_with(format!("http://{USER}:{PASS}@127.0.0.1:1/v1"));
    let dbg = format!("{p:?}");
    assert_clean(&dbg, "Debug");
    assert!(dbg.contains("127.0.0.1"), "host must survive: {dbg}");
}

#[test]
fn a_query_secret_never_appears_in_debug_but_its_name_does() {
    let p = provider_with(format!("http://127.0.0.1:1/v1?key={QKEY}"));
    let dbg = format!("{p:?}");
    assert_clean(&dbg, "Debug");
    assert!(
        dbg.contains("key"),
        "the parameter NAME is diagnostic context, not a secret: {dbg}"
    );
}

#[test]
fn both_forms_in_one_url_are_both_redacted_in_debug() {
    // The realistic shape behind an authenticated proxy in front of a query-authenticated endpoint.
    let p = provider_with(format!("http://{USER}:{PASS}@127.0.0.1:1/v1?key={QKEY}"));
    assert_clean(&format!("{p:?}"), "Debug (combined)");
}

#[tokio::test]
async fn credentials_never_appear_in_a_transport_error() {
    let p = provider_with(format!("http://{USER}:{PASS}@127.0.0.1:1/v1?key={QKEY}"));
    let err = p
        .complete("sys", "usr", &CompletionConfig::default())
        .await
        .expect_err("connection refused on port 1");
    let msg = err.to_string();
    assert_clean(&msg, "error Display");
    assert!(
        msg.contains("127.0.0.1"),
        "the endpoint must still be identifiable: {msg}"
    );
}

#[tokio::test]
async fn credentials_never_reach_the_serialized_report() {
    // The channel that matters most: the error travels into `failed_agents` and from there into
    // whatever gets shared. Stopping at the error string would never exercise this path.
    let provider = std::sync::Arc::new(provider_with(format!(
        "http://{USER}:{PASS}@127.0.0.1:1/v1?key={QKEY}"
    )));
    let magi = Magi::builder(provider).build().expect("builds");

    let serialized = match magi.analyze(&Mode::CodeReview, "contenido de prueba").await {
        Ok(report) => serde_json::to_string(&report).expect("serializes"),
        Err(e) => e.to_string(),
    };
    assert_clean(&serialized, "serialized report");
}

// -- the Ollama provider --
//
// It composes TWO more endpoints than its sibling (`/api/show`, `/api/tags`) and derives its
// completions authority from the same base, so every one of those is a place a credential could
// escape. It is also the provider whose construction was silently replacing credentials with the
// redaction placeholder until review caught it.

#[cfg(feature = "ollama")]
fn ollama_with(url: String) -> magi_core::providers::ollama::OllamaProvider {
    magi_core::providers::ollama::OllamaProvider::new(url, "qwen3:8b").expect("constructs")
}

#[cfg(feature = "ollama")]
#[tokio::test]
async fn ollama_credentials_never_appear_in_a_completion_error() {
    let p = ollama_with(format!("http://{USER}:{PASS}@127.0.0.1:1?key={QKEY}"));
    let err = p
        .complete("sys", "usr", &CompletionConfig::default())
        .await
        .expect_err("connection refused on port 1");
    let msg = err.to_string();
    assert_clean(&msg, "ollama completion error");
    assert!(
        msg.contains("127.0.0.1"),
        "the endpoint must stay identifiable: {msg}"
    );
}

#[cfg(feature = "ollama")]
#[tokio::test]
async fn ollama_credentials_never_appear_in_a_probe_error() {
    // The probe endpoints are built from the base authority directly, so they are a separate
    // channel from the completion path and need their own assertion.
    let p = ollama_with(format!("http://{USER}:{PASS}@127.0.0.1:1?key={QKEY}"));

    for (label, rendered) in [
        ("window", format!("{:?}", p.window().await)),
        ("digest", format!("{:?}", p.digest().await)),
    ] {
        assert_clean(&rendered, &format!("ollama {label} probe"));
    }
}

#[cfg(feature = "ollama")]
#[tokio::test]
async fn ollama_credentials_never_reach_the_serialized_report() {
    // The channel that matters most, and the one the Ollama coverage was missing: the error
    // travels into `failed_agents` and from there into whatever gets pasted into a ticket.
    // Stopping at the provider's own error string never exercises that path.
    let provider = std::sync::Arc::new(ollama_with(format!(
        "http://{USER}:{PASS}@127.0.0.1:1?key={QKEY}"
    )));
    let magi = Magi::builder(provider).build().expect("builds");

    let serialized = match magi.analyze(&Mode::CodeReview, "contenido de prueba").await {
        Ok(report) => serde_json::to_string(&report).expect("serializes"),
        Err(e) => e.to_string(),
    };
    assert_clean(&serialized, "ollama serialized report");
}
