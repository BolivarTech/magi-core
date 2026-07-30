// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05

//! # magi-core
//!
//! Multi-perspective analysis using three independent LLM agents
//! (Melchior/Scientist, Balthasar/Pragmatist, Caspar/Critic).
//!
//! Each agent analyzes content from a different perspective, then a
//! consensus engine synthesizes their verdicts into a unified report.
//!
//! ## Retry & backoff (2.0)
//!
//! The opt-in [`RetryProvider`](crate::provider::RetryProvider) wraps any
//! provider with capped, jittered backoff and honors `Retry-After`. **Worst-case
//! latency with the defaults is ~15 minutes** per `complete()` call (a 10-minute
//! `operation_budget` plus one 5-minute request timeout). Wrap the call in
//! `tokio::time::timeout` if you need a harder bound.
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
/// Public since MS3 (R19): a consumer writing a custom prompt needs to see the canonical
/// shape now that `build()` enforces it — without this the guard is a wall with no door.
/// It is also the migration path: the built-in prompt IS the source of truth, always in
/// sync via `include_str!`. `validate_prompt` must be real public API for the strict
/// marker predicate to have a production consumer at all.
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
/// Public from birth (MS3 R1): the doctest for `extract` (T3) cannot compile
/// against a private module — a doctest compiles as an external crate — and
/// `§0.1` runs doctests, so deferring visibility would break the gate mid-plan.
pub mod verdict_markers;
