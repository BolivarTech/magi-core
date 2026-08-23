// Author: Julian Bolivar
// Version: 4.0.0
// Date: 2026-08-23

//! # magi-core
//!
//! Multi-perspective analysis using three independent LLM agents
//! (Melchior/Scientist, Balthasar/Pragmatist, Caspar/Critic).
//!
//! Each agent analyzes content from a different perspective, then a
//! consensus engine synthesizes their verdicts into a unified report.
//!
//! ## Retry, backoff and how long a run can take
//!
//! The opt-in [`RetryProvider`](crate::provider::RetryProvider) wraps any
//! provider with capped, jittered backoff and honors `Retry-After` (since 2.0).
//!
//! **Ask the crate for the worst case; do not derive it.**
//! [`Magi::worst_case_per_seat`](crate::orchestrator::Magi::worst_case_per_seat)
//! computes it from the configuration you actually built. Two figures get
//! quoted and only one of them is the default: with no fallback pool there is a
//! single model, so it is **22 minutes per seat**; declaring a pool brings
//! rotation in and the same defaults give 66. Whether a run costs that once or
//! three times over depends on whether your backend serves the three mages in
//! parallel, which this crate cannot know. Wrap the call in
//! `tokio::time::timeout` if you need a harder bound.
//!
//! Since `4.0.0` the retry chain is bounded by an attempt **count** rather than
//! by elapsed time, so `operation_budget + client_timeout` is no longer how the
//! per-call bound is computed — that relation is deliberately unsatisfied by
//! the shipped defaults, and `operation_budget` is now a backstop. See the
//! [`RetryConfig`](crate::provider::RetryConfig) rustdoc for the current form.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use magi_core::prelude::*;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), MagiError> {
//! // let provider: Arc<dyn LlmProvider> = /* your provider */;
//! // let magi = Magi::new(provider);
//! // let report = magi.analyze(&Mode::CodeReview, "fn main() {}").await?;
//! // println!("{}", report.report);
//! # Ok(())
//! # }
//! ```
//!
//! ## Custom prompts: check the contract before you deploy
//!
//! Since `3.0.0` an agent's verdict is read **only** from between
//! [`VERDICT_OPEN`](crate::verdict_markers::VERDICT_OPEN) and
//! [`VERDICT_CLOSE`](crate::verdict_markers::VERDICT_CLOSE) — nothing outside the markers
//! is ever parsed. That closes the case where a model echoes the worked example from its
//! own instructions and fabricates a verdict nobody formed.
//!
//! The consequence for you: **`MagiBuilder::build()` rejects any custom prompt that does
//! not carry the marker block.** Assert it in your own test suite instead of finding out
//! at build time — [`prompts::validate_prompt`] is the very function `build()` runs, so
//! what it accepts is exactly what `build()` accepts:
//!
//! ```rust
//! use magi_core::prompts::{caspar_prompt, validate_prompt};
//!
//! // The built-in prompt IS the canonical shape: start from it, or copy its
//! // `## Output format` section verbatim into your own.
//! validate_prompt(caspar_prompt()).expect("the shipped prompt satisfies the contract");
//!
//! // A pre-3.0 prompt has no marker block, so `build()` would refuse it.
//! assert!(validate_prompt("You are Caspar. Reply with only a JSON object.").is_err());
//! ```
//!
//! There is deliberately **no** automatic fixer, and the reason is worth stating here
//! rather than elsewhere: appending the marker section to a legacy prompt produces one
//! that **contradicts itself** — half of it forbidding any text outside the JSON, half
//! inviting the model to reason freely before the markers. That prompt passes the guard
//! and performs worse than either half. Migrating means *reading* your prompt and
//! removing the old "no text outside the JSON" instruction, not wrapping it.
//!
//! One case has no prompt-side fix: if your provider forces `response_format`/structured
//! outputs, the model **cannot** wrap its JSON in markers, so it is incompatible with the
//! sentinel. Staying on `2.x` is not the answer either — that is the version with the
//! fabrication hole still open. Stop forcing structured output on that provider.

pub mod agent;
pub mod backoff;
pub mod consensus;
pub mod error;
pub mod finding_id;
pub mod orchestrator;
pub mod prelude;
// NO outer doc comment here on purpose: the module documents itself in `prompts/mod.rs`.
// An outer `///` at the declaration merges with the module's own `//!`, and the merged
// docs resolve intra-doc links in the CRATE ROOT's scope — where `caspar_prompt` is not in
// scope, so every link inside the module breaks. Rustdoc even reports those failures at
// this line rather than at the real one, which makes it a slow thing to diagnose.
pub mod prompts;
pub mod provider;
pub mod providers;
pub mod reporting;
pub mod rotation;
pub mod schema;
/// Test-only support (RoutingMockProvider). Gated by `test-utils` feature
/// for downstream integration tests; always available in-tree under `cfg(test)`.
#[cfg(any(test, feature = "test-utils"))]
pub mod test_support;
mod user_prompt;
pub mod validate;
/// Public because a consumer needs to be able to call [`verdict_markers::extract`]: the
/// crate defines exactly one notion of "this line is a marker", and one that cannot be
/// called from outside gets reimplemented — most likely with `str::lines()`, which does
/// not split on a lone `\r`. Drift across the crate boundary is as bad as drift within it.
pub mod verdict_markers;
