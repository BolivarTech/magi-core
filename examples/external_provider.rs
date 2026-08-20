// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-07-30

//! Implementing [`LlmProvider`] **outside** this crate, and failing in a typed way.
//!
//! # This example is a test, and it can only be one from here
//!
//! An example compiles as a **separate crate**, so `#[non_exhaustive]` applies to it exactly as it
//! applies to a real consumer. Inside `src/` every variant is constructible, so an equivalent test
//! there would pass while the public API stayed broken — which is what happened: the variants were
//! closed in 2.0.0 with no constructor to replace them, and nothing in the suite noticed.
//!
//! Run it with:
//!
//! ```text
//! cargo run --all-features --example external_provider
//! ```

use async_trait::async_trait;
use magi_core::prelude::*;

/// A stand-in for a real backend — an inference crate, another vendor's API, anything living
/// outside `magi-core`.
struct MyBackend;

#[async_trait]
impl LlmProvider for MyBackend {
    async fn complete(
        &self,
        _system_prompt: &str,
        _user_prompt: &str,
        _config: &CompletionConfig,
    ) -> Result<Completion, ProviderError> {
        // THE POINT OF THIS FILE.
        //
        // `ProviderError::external` is the only constructor an outside crate can reach, and
        // without it this line has no honest form: every variant is `#[non_exhaustive]`, so
        // `ProviderError::Network { message }` fails to compile with E0639. The alternatives left
        // were all worse — claim `NestedSession` (a lie), return `Ok` with garbage (a lie the
        // parser then has to catch), or panic (which takes the whole run down).
        Err(ProviderError::external(
            "backend unreachable",
            ExternalErrorKind::Network,
        ))
    }

    fn name(&self) -> &str {
        "my-backend"
    }

    fn model(&self) -> &str {
        "my-model-v1"
    }
}

/// The whole migration cost of the 4.0.0 break, in one type.
///
/// An implementor that measures nothing changes exactly two things: the SIGNATURE
/// (`Result<String, _>` becomes `Result<Completion, _>`) and the RETURN (`Ok(text)` becomes
/// `Ok(text.into())`). Nothing else. What it gets back for free is telemetry that says
/// **not measured** rather than reporting zeros, because a zero that means "nobody looked"
/// is indistinguishable from a measurement that came back zero.
struct MinimalProvider;

#[async_trait]
impl LlmProvider for MinimalProvider {
    async fn complete(
        &self,
        _system_prompt: &str,
        _user_prompt: &str,
        _config: &CompletionConfig,
    ) -> Result<Completion, ProviderError> {
        Ok("verdict text".to_string().into())
    }

    fn name(&self) -> &str {
        "minimal"
    }

    fn model(&self) -> &str {
        "none"
    }
}

/// Renders a failure the way a consumer would when logging it.
///
/// The `_` arm is **required**: `ProviderError` is `#[non_exhaustive]`, so an outside crate cannot
/// match it exhaustively, and removing the arm below does not compile. That asymmetry is the
/// design and not an oversight — 3.1.0 opens a door for **construction** while leaving
/// **matching** closed, so this crate can keep adding variants without breaking anyone.
fn describe(err: &ProviderError) -> String {
    match err {
        ProviderError::External { kind, .. } => format!("external backend failed: {kind:?}"),
        ProviderError::Timeout { .. } => "timed out".to_string(),
        _ => "some other failure".to_string(),
    }
}

#[tokio::main]
async fn main() {
    let provider = MyBackend;
    // Matched rather than unwrapped. An example is the first code a new implementor copies, so it
    // has to model the handling this crate asks for everywhere else — `expect_err` would teach the
    // opposite in the one file written to be imitated.
    match provider
        .complete("system", "user", &CompletionConfig::default())
        .await
    {
        Err(err) => {
            println!("{}", describe(&err));
            println!("rendered: {err}");
        }
        Ok(completion) => println!("unexpected success: {}", completion.text),
    }

    // Criterion 11, and it is only observable from OUT HERE: an external implementor migrates
    // with two changes AND its telemetry reports not-measured, never zeros. Compiling the example
    // does not show the second half — these asserts do, and they only run under `cargo run`.
    let completion = MinimalProvider
        .complete("system", "user", &CompletionConfig::default())
        .await
        .expect("the minimal provider cannot fail");
    assert_eq!(completion.text, "verdict text");
    assert!(
        completion.telemetry.finish.is_none(),
        "an unmeasured completion must not claim a termination reason"
    );
    assert!(
        completion.telemetry.completion_tokens.is_none(),
        "reporting 0 tokens would assert a measurement nobody took"
    );
    assert!(
        completion.telemetry.prompt_tokens.is_none(),
        "reporting 0 tokens would assert a measurement nobody took"
    );
    assert!(
        matches!(completion.telemetry.reasoning, ReasoningState::NotMeasured),
        "NotMeasured is a third state, distinct from Measured with a length of 0"
    );
    println!("external provider: telemetry reports NOT MEASURED, not zeros");

    // The shape is declared by the third party; the CONSEQUENCE stays with magi-core. This crate
    // decides whether that shape is retried and whether it condemns a lineage — an external
    // provider names its failure, it does not choose what happens next.
    println!("magi-core decides retryability from the shape, not from this message");
}
