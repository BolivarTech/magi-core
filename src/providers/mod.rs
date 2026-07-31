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

// Before adding a provider here: does it authenticate via URL **path** or **fragment**?
// If so, the redaction boundary must be revisited — it covers userinfo and query values only,
// and path/fragment are deliberately out of scope (no known LLM API authenticates there).
// Gated on EITHER HTTP provider: the bounded body readers and the error mapper live here, and
// scoping them to one feature is how the Claude provider ended up without a body cap while the
// changelog said every response was bounded.
#[cfg(any(feature = "claude-api", feature = "openai-compat"))]
pub(crate) mod provider_url;

#[cfg(feature = "openai-compat")]
pub mod openai_compat;

#[cfg(feature = "ollama")]
pub mod ollama;
