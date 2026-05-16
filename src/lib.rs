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
pub mod consensus;
pub mod error;
pub mod orchestrator;
pub mod prelude;
mod prompts;
pub mod provider;
pub mod providers;
pub mod reporting;
pub mod schema;
/// Test-only support (RoutingMockProvider). Gated by `test-utils` feature
/// for downstream integration tests; always available in-tree under `cfg(test)`.
/// See `docs/migration-v0.4.md` for stability contract.
#[cfg(any(test, feature = "test-utils"))]
pub mod test_support;
mod user_prompt;
pub mod validate;
