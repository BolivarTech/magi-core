// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05

//! LLM provider implementations.
//!
//! Each provider is feature-gated to minimize dependencies.

#[cfg(feature = "claude-api")]
pub mod claude;

#[cfg(feature = "claude-cli")]
pub mod claude_cli;
