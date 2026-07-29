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
