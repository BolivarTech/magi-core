# Section 11: Prelude and Crate Root (`prelude.rs` + `lib.rs`)

## Overview

This section implements the `prelude` module and finalizes the crate root (`lib.rs`). The prelude re-exports the most commonly needed types so users can write `use magi_core::prelude::*` for a convenient one-line import. The crate root (`lib.rs`) declares all modules, conditionally compiles provider modules based on feature flags, and re-exports key types at crate root level. This section is the final wiring step that makes the entire library usable from external code.

## Dependencies

- **Internal sections**: All sections 01-10 must be complete before this section, since it re-exports types from every module.
  - Section 01 (`error.rs`) -- `MagiError`, `ProviderError`
  - Section 02 (`schema.rs`) -- `Verdict`, `Severity`, `Mode`, `AgentName`, `Finding`, `AgentOutput`
  - Section 03 (`validate.rs`) -- `Validator`, `ValidationLimits`
  - Section 04 (`consensus.rs`) -- `ConsensusEngine`, `ConsensusConfig`, `ConsensusResult`
  - Section 05 (`reporting.rs`) -- `ReportFormatter`, `ReportConfig`, `MagiReport`
  - Section 06 (`provider.rs`) -- `LlmProvider`, `CompletionConfig`, `RetryProvider`
  - Section 07 (`agent.rs`) -- `Agent`, `AgentFactory`
  - Section 08 (`orchestrator.rs`) -- `Magi`, `MagiBuilder`, `MagiConfig`
  - Section 09 (`providers/claude.rs`) -- `ClaudeProvider` (feature-gated)
  - Section 10 (`providers/claude_cli.rs`) -- `ClaudeCliProvider` (feature-gated)
- **External crates**: None new.

## Files to Create or Modify

| File | Action |
|------|--------|
| `magi-core/src/prelude.rs` | Create -- re-exports of common types |
| `magi-core/src/lib.rs` | Finalize -- all module declarations, feature gates, crate-level re-exports, crate-level Rustdoc |

## File Header

Every new source file in this project must start with:

```rust
// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05
```

## Tests (Write First -- Red Phase)

Tests for this section verify that the prelude and feature-gated compilation work correctly. These are primarily compile-time verification tests.

```rust
#[cfg(test)]
mod tests {
    // -- Prelude re-exports --

    /// prelude re-exports Magi, Mode, MagiReport, LlmProvider, CompletionConfig, etc.
    #[test]
    fn test_prelude_reexports_core_types() {
        // use crate::prelude::*;
        // Verify that the following types are accessible:
        //   Magi, MagiBuilder, MagiConfig
        //   Mode, Verdict, Severity, AgentName
        //   MagiReport, AgentOutput, Finding
        //   LlmProvider, CompletionConfig
        //   MagiError, ProviderError
        //   ConsensusResult
        // This test passes if it compiles -- the types must be reachable via prelude
    }

    /// crate compiles with no features enabled (core only).
    #[test]
    fn test_crate_compiles_with_no_features() {
        // This test exists in the default test suite (no features)
        // It passes if the crate compiles -- structural verification
        // Assert that providers module exists but ClaudeProvider is not accessible
    }

    /// feature flags conditionally compile provider modules.
    #[test]
    fn test_feature_flags_gate_provider_modules() {
        // When claude-api is not enabled:
        //   Verify ClaudeProvider is not in scope
        // When claude-cli is not enabled:
        //   Verify ClaudeCliProvider is not in scope
        // (Compile-time tests -- these pass by not failing to compile)
    }
}
```

Additionally, create compilation tests in separate feature-gated test files or use `cfg` attributes:

```rust
/// crate compiles with claude-api feature.
#[cfg(feature = "claude-api")]
#[test]
fn test_crate_compiles_with_claude_api_feature() {
    // use crate::providers::claude::ClaudeProvider;
    // Assert ClaudeProvider type is accessible
}

/// crate compiles with claude-cli feature.
#[cfg(feature = "claude-cli")]
#[test]
fn test_crate_compiles_with_claude_cli_feature() {
    // use crate::providers::claude_cli::ClaudeCliProvider;
    // Assert ClaudeCliProvider type is accessible
}
```

## Implementation Details (Green Phase)

### `prelude.rs`

The prelude module re-exports the types that most users will need. It provides a convenient single import for common usage patterns.

```rust
//! Convenience re-exports for common magi-core types.
//!
//! # Usage
//!
//! ```rust
//! use magi_core::prelude::*;
//! ```

// Error types
pub use crate::error::{MagiError, ProviderError};

// Domain schema
pub use crate::schema::{AgentName, AgentOutput, Finding, Mode, Severity, Verdict};

// Validation
pub use crate::validate::{ValidationLimits, Validator};

// Consensus
pub use crate::consensus::{ConsensusConfig, ConsensusEngine, ConsensusResult};

// Reporting
pub use crate::reporting::{MagiReport, ReportConfig, ReportFormatter};

// Provider trait and config
pub use crate::provider::{CompletionConfig, LlmProvider, RetryProvider};

// Agents
pub use crate::agent::{Agent, AgentFactory};

// Orchestrator
pub use crate::orchestrator::{Magi, MagiBuilder, MagiConfig};

// Feature-gated providers
#[cfg(feature = "claude-api")]
pub use crate::providers::claude::ClaudeProvider;

#[cfg(feature = "claude-cli")]
pub use crate::providers::claude_cli::ClaudeCliProvider;
```

### `lib.rs` -- Final Structure

The crate root declares all modules, applies feature gates, provides crate-level Rustdoc, and re-exports key types.

```rust
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
//! let provider: Arc<dyn LlmProvider> = /* your provider */;
//! let magi = Magi::new(provider);
//! let report = magi.analyze(&Mode::CodeReview, "fn main() {}").await?;
//! println!("{}", report.report);
//! # Ok(())
//! # }
//! ```

// Core modules (always compiled)
pub mod error;
pub mod schema;
pub mod validate;
pub mod consensus;
pub mod reporting;
pub mod provider;
pub mod agent;
pub mod orchestrator;
pub mod prelude;

// Private prompt module (used internally by agent.rs)
mod prompts;

// Feature-gated provider modules
pub mod providers;
```

### `providers/mod.rs` -- Final Structure

```rust
//! LLM provider implementations.
//!
//! Each provider is feature-gated to minimize dependencies.

#[cfg(feature = "claude-api")]
pub mod claude;

#[cfg(feature = "claude-cli")]
pub mod claude_cli;
```

### Feature Flags in `Cargo.toml`

The `[features]` section should look like:

```toml
[features]
default = []
claude-api = ["dep:reqwest"]
claude-cli = []
```

Nothing is enabled by default. Users opt into specific providers.

### Module Visibility

- All core modules (`error`, `schema`, `validate`, `consensus`, `reporting`, `provider`, `agent`, `orchestrator`) are `pub mod` for direct access
- `prompts` is `mod` (private) -- only used internally by `agent.rs`
- `providers` is `pub mod` but its contents are feature-gated
- `prelude` is `pub mod` for `use magi_core::prelude::*`

## Constraints Checklist

- No `panic!`, `unwrap()`, `expect()` outside `#[cfg(test)]`
- No `unsafe`
- Crate-level `//!` Rustdoc on `lib.rs` with usage example
- Module-level `//!` Rustdoc on `prelude.rs`
- Feature flags: nothing enabled by default
- `providers` module compiles with zero, one, or both features enabled
- All re-exports in prelude match the actual public API surface
- `rustfmt` and `clippy --tests -- -D warnings` clean
- File header present on all new files

## Refactor Phase Notes

After Green phase passes all tests:

- Verify `cargo build` succeeds with no features, `--features claude-api`, `--features claude-cli`, and `--features claude-api,claude-cli`
- Run `cargo doc --no-deps` and verify all re-exported types appear in the generated documentation
- Verify the prelude does not re-export internal/private types
- Ensure crate-level Rustdoc example compiles (use `no_run` or `ignore` if it requires a real provider)
- Consider adding a `full` feature that enables all providers for convenience
- Confirm the `providers` module is empty but compiles cleanly when no features are enabled
