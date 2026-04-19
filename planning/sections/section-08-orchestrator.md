# Section 08: Orchestrator -- Magi, MagiBuilder, MagiConfig (`orchestrator.rs`)

## Overview

This section implements the main entry point of the magi-core library: the `Magi` struct, `MagiBuilder`, and `MagiConfig`. The `Magi` struct composes all other modules (agents, validation, consensus, reporting) and orchestrates the full analysis flow. The `analyze()` method launches three agents in parallel using `tokio::task::JoinSet`, parses their JSON responses, validates outputs, computes consensus, and generates a formatted report. `MagiBuilder` provides a consuming builder pattern with sensible defaults. The orchestrator handles degradation gracefully: 2/3 agents succeeding produces a degraded report, while fewer than 2 returns `MagiError::InsufficientAgents`.

## Dependencies

- **External crates**:
  - `tokio` (already in dependencies) -- `task::JoinSet`, `time::timeout`
  - `serde_json` (already in dependencies) -- parsing agent JSON responses
  - `tracing` -- structured logging for init banner and agent status events
- **Internal sections**:
  - Section 01 (`error.rs`) -- `MagiError`, `ProviderError`
  - Section 02 (`schema.rs`) -- `AgentName`, `Mode`, `AgentOutput`, `Finding`, `Verdict`, `Severity`
  - Section 03 (`validate.rs`) -- `Validator`, `ValidationLimits`
  - Section 04 (`consensus.rs`) -- `ConsensusEngine`, `ConsensusConfig`, `ConsensusResult`
  - Section 05 (`reporting.rs`) -- `ReportFormatter`, `ReportConfig`, `MagiReport`
  - Section 06 (`provider.rs`) -- `LlmProvider`, `CompletionConfig`
  - Section 07 (`agent.rs`) -- `Agent`, `AgentFactory`
- **Standard library**: `std::sync::Arc`, `std::time::Duration`, `std::collections::BTreeMap`

## Files to Create or Modify

| File | Action |
|------|--------|
| `magi-core/src/orchestrator.rs` | Create -- contains `Magi`, `MagiBuilder`, `MagiConfig`, and `parse_agent_response` |
| `magi-core/src/lib.rs` | Add `pub mod orchestrator;` |

## File Header

Every new source file in this project must start with:

```rust
// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05
```

## Tests (Write First -- Red Phase)

All tests go in `src/orchestrator.rs` inside a `#[cfg(test)] mod tests` block. Use `mockall`-generated `MockLlmProvider` or a manual mock that returns predefined JSON strings.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::*;
    use std::sync::Arc;
    use std::time::Duration;

    // Helper: build a valid AgentOutput JSON string for a given agent name and verdict
    // fn mock_agent_json(agent: &str, verdict: &str, confidence: f64) -> String { ... }

    // Helper: create a mock provider that returns specific JSON per agent
    // struct MockProvider { responses: Vec<String>, ... }

    // -- BDD Scenario 1: successful analysis with 3 unanimous agents --

    /// analyze returns MagiReport with 3 outputs, consensus, banner, report, degraded=false.
    #[tokio::test]
    async fn test_analyze_unanimous_approve_returns_complete_report() {
        // Create mock provider returning valid approve JSON for all 3 agents
        // Build Magi with Magi::new(provider)
        // Call analyze(Mode::CodeReview, "some content")
        // Assert Ok(report) with 3 agent outputs
        // Assert report.degraded == false
        // Assert report.failed_agents is empty
        // Assert report.consensus.consensus_verdict == Verdict::Approve
        // Assert report.banner is non-empty
        // Assert report.report is non-empty
    }

    // -- BDD Scenario 6: degradation - 1 agent timeout --

    /// 2 succeed + 1 timeout produces Ok(MagiReport), degraded=true, failed_agents=[timeout_agent].
    #[tokio::test]
    async fn test_analyze_one_agent_timeout_degrades_gracefully() {
        // Create mock where one agent's complete() returns ProviderError::Timeout
        // Build Magi with short timeout config
        // Call analyze
        // Assert Ok(report)
        // Assert report.degraded == true
        // Assert report.failed_agents contains the timed-out agent
        // Assert report has 2 agent outputs
    }

    // -- BDD Scenario 7: degradation - 1 agent invalid JSON --

    /// 2 succeed + 1 bad JSON produces Ok(MagiReport), degraded=true.
    #[tokio::test]
    async fn test_analyze_one_agent_bad_json_degrades_gracefully() {
        // Create mock where one agent returns "not valid json at all"
        // Build Magi
        // Call analyze
        // Assert Ok(report)
        // Assert report.degraded == true
    }

    // -- BDD Scenario 8: 2 agents fail --

    /// 1 succeed + 2 fail returns Err(InsufficientAgents { succeeded: 1, required: 2 }).
    #[tokio::test]
    async fn test_analyze_two_agents_fail_returns_insufficient_agents() {
        // Create mock where 2 agents return errors
        // Build Magi
        // Call analyze
        // Assert Err(MagiError::InsufficientAgents { succeeded: 1, required: 2 })
    }

    // -- BDD Scenario 9: all agents fail --

    /// 0 succeed returns Err(InsufficientAgents { succeeded: 0, required: 2 }).
    #[tokio::test]
    async fn test_analyze_all_agents_fail_returns_insufficient_agents() {
        // Create mock where all agents return errors
        // Call analyze
        // Assert Err(MagiError::InsufficientAgents { succeeded: 0, required: 2 })
    }

    // -- BDD Scenario 14: LLM returns non-JSON --

    /// Agent returns plain text, treated as failed, system continues with remaining.
    #[tokio::test]
    async fn test_analyze_plain_text_response_treated_as_failure() {
        // Create mock where one agent returns "I think the code is good"
        // Other 2 return valid JSON
        // Call analyze
        // Assert Ok with degraded=true
    }

    // -- BDD Scenario 28: Magi::new with single provider --

    /// new creates Magi with 3 agents sharing same provider, all defaults.
    #[tokio::test]
    async fn test_magi_new_creates_with_defaults() {
        // Create mock provider
        // Build Magi::new(provider)
        // Call analyze
        // Assert all 3 agents used the same provider (by name)
    }

    // -- BDD Scenario 29: builder with mixed providers and custom config --

    /// Builder sets per-agent providers and custom timeout.
    #[tokio::test]
    async fn test_builder_with_mixed_providers_and_custom_config() {
        // Create default mock and override mock
        // MagiBuilder::new(default)
        //   .with_provider(Caspar, override_mock)
        //   .with_timeout(Duration::from_secs(60))
        //   .build()
        // Call analyze
        // Assert Caspar used override provider
    }

    // -- BDD Scenario 32: input too large --

    /// Content exceeding max_input_len returns Err(InputTooLarge) without launching agents.
    #[tokio::test]
    async fn test_analyze_input_too_large_rejects_without_launching_agents() {
        // Create Magi with max_input_len = 100
        // Call analyze with content of 200 bytes
        // Assert Err(MagiError::InputTooLarge { size: 200, max: 100 })
        // Assert mock provider was NOT called (no agents launched)
    }

    // -- MagiConfig defaults --

    /// MagiConfig::default has timeout=300s, max_input_len=1MB.
    #[test]
    fn test_magi_config_default_values() {
        // let config = MagiConfig::default();
        // Assert config.timeout == Duration::from_secs(300)
        // Assert config.max_input_len == 1_048_576
    }

    // -- build_prompt formatting --

    /// build_prompt formats "MODE: {mode}\nCONTEXT:\n{content}".
    #[test]
    fn test_build_prompt_formats_mode_and_content() {
        // Assert build_prompt(Mode::CodeReview, "fn main() {}")
        //   == "MODE: code-review\nCONTEXT:\nfn main() {}"
    }

    // -- parse_agent_response --

    /// parse_agent_response strips code fences from JSON.
    #[test]
    fn test_parse_agent_response_strips_code_fences() {
        // let raw = "```json\n{\"agent\":\"melchior\",...}\n```"
        // Assert parse_agent_response(raw) is Ok with correct AgentOutput
    }

    /// parse_agent_response finds JSON object in preamble text.
    #[test]
    fn test_parse_agent_response_extracts_json_from_preamble() {
        // let raw = "Here is my analysis:\n{\"agent\":\"melchior\",...}"
        // Assert parse_agent_response(raw) is Ok
    }

    /// parse_agent_response fails on completely invalid input.
    #[test]
    fn test_parse_agent_response_fails_on_invalid_input() {
        // Assert parse_agent_response("no json here") is Err
    }

    // -- MagiBuilder --

    /// MagiBuilder::build returns Ok(Magi) with required provider.
    #[test]
    fn test_magi_builder_build_returns_result() {
        // let magi = MagiBuilder::new(mock_provider).build();
        // Assert magi.is_ok()
    }
}
```

## Implementation Details (Green Phase)

### `MagiConfig` Struct

A `#[non_exhaustive]` configuration struct for the orchestrator.

- **Fields**:
  - `timeout: Duration` -- maximum time to wait for each agent (default 300 seconds)
  - `max_input_len: usize` -- maximum content size in bytes (default 1_048_576 = 1MB, measured as UTF-8 encoded bytes via `str::len()`)
  - `completion: CompletionConfig` -- forwarded to each agent's `execute` call

Derives: `Debug`, `Clone`. Implements `Default`.

### `MagiBuilder` Struct

A consuming builder (methods take `mut self` and return `Self`) for constructing `Magi` instances. The only required field is `default_provider`, passed to the constructor.

- **Fields**:
  - `default_provider: Arc<dyn LlmProvider>` -- required, set at construction
  - `agent_providers: BTreeMap<AgentName, Arc<dyn LlmProvider>>` -- optional per-agent overrides
  - `custom_prompts: BTreeMap<AgentName, String>` -- optional per-agent prompt overrides
  - `prompts_dir: Option<PathBuf>` -- optional directory for filesystem-loaded prompts
  - `config: MagiConfig` -- defaults to `MagiConfig::default()`
  - `validation_limits: ValidationLimits` -- defaults to `ValidationLimits::default()`
  - `consensus_config: ConsensusConfig` -- defaults to `ConsensusConfig::default()`
  - `report_config: ReportConfig` -- defaults to `ReportConfig::default()`

- **Constructor**:
  - `new(default_provider: Arc<dyn LlmProvider>) -> Self`

- **Builder methods** (all consume `self`, return `Self`):
  - `with_provider(mut self, name: AgentName, provider: Arc<dyn LlmProvider>) -> Self`
  - `with_custom_prompt(mut self, name: AgentName, prompt: String) -> Self`
  - `with_prompts_dir(mut self, dir: PathBuf) -> Self`
  - `with_timeout(mut self, timeout: Duration) -> Self`
  - `with_max_input_len(mut self, max: usize) -> Self`
  - `with_completion_config(mut self, config: CompletionConfig) -> Self`
  - `with_validation_limits(mut self, limits: ValidationLimits) -> Self`
  - `with_consensus_config(mut self, config: ConsensusConfig) -> Self`
  - `with_report_config(mut self, config: ReportConfig) -> Self`

- **Build**:
  - `build(self) -> Result<Magi, MagiError>` -- assembles all components. Loads prompts from `prompts_dir` if set (may fail with `MagiError::Io`). Constructs: `AgentFactory`, `Validator`, `ConsensusEngine`, `ReportFormatter`. Currently succeeds if required provider is present.

### `Magi` Struct

The main entry point. Fields: `config: MagiConfig`, `agent_factory: AgentFactory`, `validator: Validator`, `consensus_engine: ConsensusEngine`, `formatter: ReportFormatter`.

- **Convenience constructor**:
  - `new(provider: Arc<dyn LlmProvider>) -> Self` -- equivalent to `MagiBuilder::new(provider).build().unwrap()` but returns `Self` directly (uses all defaults, cannot fail)

- **Main method**:
  - `analyze(&self, mode: &Mode, content: &str) -> Result<MagiReport, MagiError>` -- full orchestration flow:

    1. **Input validation**: check `content.len() <= config.max_input_len`, return `MagiError::InputTooLarge { size, max }` if exceeded
    2. **Create agents**: `agent_factory.create_agents(mode)` -- produces 3 agents
    3. **Log init banner**: `formatter.format_init_banner(mode, ...)` emitted via `tracing::info!`
    4. **Build prompt**: `build_prompt(mode, content)` -- formats `"MODE: {mode}\nCONTEXT:\n{content}"`
    5. **Launch agents**: uses `tokio::task::JoinSet` to spawn all 3 agents concurrently. Each task is wrapped in `tokio::time::timeout(config.timeout)`. Dropping the `JoinSet` automatically aborts all spawned tasks, providing cancellation safety and preventing wasted LLM API quota.
    6. **Parse responses**: for each completed agent, call `parse_agent_response(raw)` to extract `AgentOutput` from the raw LLM string
    7. **Validate**: `validator.validate(&output)` per successful parse
    8. **Process results**: separate successes and failures. If fewer than `min_agents` (default 2) succeeded, return `MagiError::InsufficientAgents { succeeded, required }`
    9. **Consensus**: `consensus_engine.determine(&successful)` -- compute consensus from validated outputs
    10. **Report**: `formatter.format_report(&successful, &consensus)` -- generate markdown report
    11. **Return**: construct `MagiReport` with agents, consensus, banner, report, degraded flag, and failed_agents list

  - **Degradation logic**: if 2/3 agents succeed, continue with `degraded: true`. If fewer than 2 succeed, return `InsufficientAgents` error.

### `parse_agent_response` Function

A private (or `pub(crate)`) function that extracts `AgentOutput` from raw LLM response text. Handles common LLM output quirks:

1. **Strip code fences**: remove leading ` ```json ` and trailing ` ``` ` if present
2. **Find JSON boundaries**: locate first `{` and last `}` in the string to extract the JSON object, skipping any preamble text the LLM may have added
3. **Deserialize**: `serde_json::from_str::<AgentOutput>(json_str)` -- serde ignores unknown fields by default
4. **Return**: `Result<AgentOutput, MagiError>` -- failures produce `MagiError::Deserialization`

### `build_prompt` Function

A private helper that formats the user prompt sent to each agent:

```rust
fn build_prompt(mode: &Mode, content: &str) -> String {
    format!("MODE: {mode}\nCONTEXT:\n{content}")
}
```

### `lib.rs` Module Declaration

Add `pub mod orchestrator;` to `src/lib.rs`.

## Constraints Checklist

- No `panic!`, `unwrap()`, `expect()` outside `#[cfg(test)]` -- `Magi::new` is the only exception where `unwrap()` is acceptable since it uses all defaults and cannot fail; document this invariant
- No `unsafe`
- All public types and methods have `///` Rustdoc
- `JoinSet` used instead of raw `tokio::spawn` for cancellation safety
- `tokio::time::timeout` wraps each agent task individually
- Input length checked before any agent work begins (fail-fast)
- `parse_agent_response` is defensive: strips fences, finds JSON boundaries, handles preamble
- `BTreeMap` used for per-agent providers and prompts for deterministic ordering
- `#[non_exhaustive]` on `MagiConfig`
- `rustfmt` and `clippy --tests -- -D warnings` clean
- File header present on all new files

## Refactor Phase Notes

After Green phase passes all tests:

- Verify that dropping a `Magi` future mid-flight actually aborts spawned agent tasks (JoinSet cancellation)
- Ensure `parse_agent_response` handles all edge cases: empty string, whitespace-only, nested JSON, multiple JSON objects
- Add `tracing::warn!` for failed agents with the failure reason
- Add `tracing::debug!` for successful agent parse results
- Confirm error messages in `MagiError::InputTooLarge` display both size and max for diagnostics
- Add Rustdoc examples on `Magi::new` and `MagiBuilder` showing basic usage
- Confirm `cargo doc --no-deps` generates clean documentation
