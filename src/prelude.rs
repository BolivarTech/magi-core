// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05

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
pub use crate::provider::{CompletionConfig, LlmProvider, RetryProvider, resolve_claude_alias};

// Agents
pub use crate::agent::{Agent, AgentFactory};

// Orchestrator
pub use crate::orchestrator::{Magi, MagiBuilder, MagiConfig};

// Feature-gated providers
#[cfg(feature = "claude-api")]
pub use crate::providers::claude::ClaudeProvider;

#[cfg(feature = "claude-cli")]
pub use crate::providers::claude_cli::ClaudeCliProvider;

#[cfg(test)]
mod tests {
    /// Prelude re-exports Magi, Mode, MagiReport, LlmProvider, CompletionConfig, etc.
    /// This test passes if it compiles — the types must be reachable via prelude.
    #[test]
    fn test_prelude_reexports_core_types() {
        use crate::prelude::*;

        // Error types
        let _: fn() -> MagiError = || MagiError::Validation("test".into());
        let _: fn() -> ProviderError = || ProviderError::Network {
            message: "test".into(),
        };

        // Domain schema
        let _mode = Mode::CodeReview;
        let _verdict = Verdict::Approve;
        let _severity = Severity::Info;
        let _agent_name = AgentName::Melchior;

        // Verify Finding, AgentOutput are in scope (compile-time check)
        fn _takes_finding(_f: &Finding) {}
        fn _takes_agent_output(_o: &AgentOutput) {}

        // Provider trait and config
        let _config = CompletionConfig::default();
        fn _takes_provider(_p: &dyn LlmProvider) {}

        // Consensus
        let _config = ConsensusConfig::default();
        fn _takes_consensus_result(_r: &ConsensusResult) {}

        // Orchestrator types compile-time check
        fn _takes_magi_config(_c: &MagiConfig) {}
        fn _takes_magi_report(_r: &MagiReport) {}

        // Validation
        let _limits = ValidationLimits::default();
        let _validator = Validator::default();
    }

    /// Crate compiles with no features enabled (core only).
    #[test]
    fn test_crate_compiles_with_no_features() {
        // This test exists in the default test suite (no features).
        // It passes if the crate compiles — structural verification.
        // Core types must be accessible.
        use crate::prelude::*;
        let _mode = Mode::Analysis;
        let _err = MagiError::Validation("test".into());
    }

    /// Feature flags conditionally compile provider modules.
    #[test]
    fn test_feature_flags_gate_provider_modules() {
        // When no features are enabled, the providers module exists
        // but contains no sub-modules. This is a compile-time check.
        // The test passes by not failing to compile.

        #[cfg(not(feature = "claude-api"))]
        {
            // ClaudeProvider should NOT be accessible via prelude
            // (compile-time verification — if this compiled with the type, it would be wrong)
        }

        #[cfg(not(feature = "claude-cli"))]
        {
            // ClaudeCliProvider should NOT be accessible via prelude
        }
    }

    /// Crate compiles with claude-api feature.
    #[cfg(feature = "claude-api")]
    #[test]
    fn test_crate_compiles_with_claude_api_feature() {
        use crate::prelude::ClaudeProvider;
        fn _takes_provider(_p: &ClaudeProvider) {}
    }

    /// Crate compiles with claude-cli feature.
    #[cfg(feature = "claude-cli")]
    #[test]
    fn test_crate_compiles_with_claude_cli_feature() {
        use crate::prelude::ClaudeCliProvider;
        fn _takes_provider(_p: &ClaudeCliProvider) {}
    }

    /// Prelude re-exports RetryProvider.
    #[test]
    fn test_prelude_reexports_retry_provider() {
        use crate::prelude::RetryProvider;
        fn _takes_provider(_p: &RetryProvider) {}
    }

    /// Prelude re-exports Agent and AgentFactory.
    #[test]
    fn test_prelude_reexports_agent_types() {
        use crate::prelude::{Agent, AgentFactory};
        fn _takes_agent(_a: &Agent) {}
        fn _takes_factory(_f: &AgentFactory) {}
    }

    /// Prelude re-exports ReportConfig and ReportFormatter.
    #[test]
    fn test_prelude_reexports_report_types() {
        use crate::prelude::{ReportConfig, ReportFormatter};
        let _config = ReportConfig::default();
        let _formatter = ReportFormatter::new();
    }

    /// Prelude re-exports ConsensusEngine.
    #[test]
    fn test_prelude_reexports_consensus_engine() {
        use crate::prelude::ConsensusEngine;
        let _engine = ConsensusEngine::default();
    }

    /// Prelude re-exports MagiBuilder.
    #[test]
    fn test_prelude_reexports_builder() {
        use crate::prelude::MagiBuilder;
        fn _takes_builder(_b: MagiBuilder) {}
    }
}
