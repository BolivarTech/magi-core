# Section 04: Consensus Engine (`consensus.rs`)

## Overview

This section implements the core algorithm of magi-core: the `ConsensusEngine`. It takes agent outputs and synthesizes them into a unified consensus verdict with a label, confidence score, deduplicated findings, dissent tracking, and condition extraction. The engine is **stateless** -- `determine(&self)` takes inputs and returns everything in `ConsensusResult`. No mutable state persists between calls. This makes the engine thread-safe and each `analyze()` call fully independent.

## Dependencies

- **External crates**: None beyond what is already in `Cargo.toml` (`serde`)
- **Internal sections**:
  - Section 01 (`error.rs`) -- `MagiError::Validation`, `MagiError::InsufficientAgents` for error returns
  - Section 02 (`schema.rs`) -- `Verdict`, `Severity`, `AgentName`, `Finding`, `AgentOutput` types
- **Standard library**: `std::collections::BTreeMap`, `std::collections::HashMap`

## Files to Create or Modify

| File | Action |
|------|--------|
| `magi-core/src/consensus.rs` | Create -- contains `ConsensusEngine`, `ConsensusConfig`, `ConsensusResult`, and supporting structs |
| `magi-core/src/lib.rs` | Add `pub mod consensus;` |

## File Header

Every new source file in this project must start with:

```rust
// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05
```

## Tests (Write First -- Red Phase)

All tests go in `src/consensus.rs` inside a `#[cfg(test)] mod tests` block. Write these tests before any implementation.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::*;

    // Helper: build AgentOutput with specified agent, verdict, confidence
    // fn make_output(agent: AgentName, verdict: Verdict, confidence: f64) -> AgentOutput { ... }

    // -- BDD Scenario 1: unanimous approve --

    /// Three approve agents produce STRONG GO with score=1.0.
    #[test]
    fn test_unanimous_approve_produces_strong_go() {
        // 3 agents all Approve with confidence ~0.9
        // Assert label="STRONG GO", verdict=Approve, score≈1.0
    }

    // -- BDD Scenario 2: mixed 2 approve + 1 reject --

    /// Two approve + one reject produces GO (2-1) with positive score.
    #[test]
    fn test_two_approve_one_reject_produces_go_2_1() {
        // Melchior=Approve, Balthasar=Approve, Caspar=Reject
        // Assert label="GO (2-1)", verdict=Approve, score≈0.333
        // Assert dissent contains Caspar
    }

    // -- BDD Scenario 3: approve + conditional + reject --

    /// Approve + conditional + reject produces GO WITH CAVEATS.
    #[test]
    fn test_approve_conditional_reject_produces_go_with_caveats() {
        // Melchior=Approve, Balthasar=Conditional, Caspar=Reject
        // Assert label="GO WITH CAVEATS", verdict=Approve
        // Assert conditions present (from Balthasar)
    }

    // -- BDD Scenario 4: unanimous reject --

    /// Three reject agents produce STRONG NO-GO with score=-1.0.
    #[test]
    fn test_unanimous_reject_produces_strong_no_go() {
        // 3 agents all Reject
        // Assert label="STRONG NO-GO", verdict=Reject, score≈-1.0
    }

    // -- BDD Scenario 5: tie with 2 agents --

    /// One approve + one reject (2 agents) produces HOLD -- TIE.
    #[test]
    fn test_tie_with_two_agents_produces_hold_tie() {
        // 2 agents: Approve + Reject → score=0
        // Assert label="HOLD -- TIE", verdict=Reject
    }

    // -- BDD Scenario 13: finding deduplication --

    /// Same title different case merges into single finding with severity promoted.
    #[test]
    fn test_duplicate_findings_merged_with_severity_promoted() {
        // Melchior: Finding(Warning, "Security Issue", detail_a)
        // Balthasar: Finding(Critical, "security issue", detail_b)
        // Assert deduplicated findings has 1 entry with severity=Critical
    }

    /// Merged finding sources include both contributing agents.
    #[test]
    fn test_merged_finding_sources_include_both_agents() {
        // Same setup as above
        // Assert sources contains both AgentName::Melchior and AgentName::Balthasar
    }

    /// Detail preserved from highest-severity finding.
    #[test]
    fn test_merged_finding_detail_from_highest_severity() {
        // Melchior: Finding(Warning, "Issue", "detail_warning")
        // Balthasar: Finding(Critical, "issue", "detail_critical")
        // Assert merged detail == "detail_critical"
    }

    /// On same severity, detail comes from first agent by AgentName ordering.
    #[test]
    fn test_merged_finding_detail_from_first_agent_on_same_severity() {
        // Balthasar: Finding(Warning, "Issue", "detail_b")
        // Melchior: Finding(Warning, "issue", "detail_m")
        // Both Warning → first by AgentName::Ord (Balthasar < Melchior)
        // Assert merged detail == "detail_b"
    }

    // -- BDD Scenario 33: degraded mode caps STRONG labels --

    /// Two approve agents (degraded) produce GO (2-0) not STRONG GO.
    #[test]
    fn test_degraded_mode_caps_strong_go_to_go() {
        // 2 approve agents (agent_count < 3)
        // Assert label="GO (2-0)", NOT "STRONG GO"
    }

    /// Two reject agents (degraded) produce HOLD (2-0) not STRONG NO-GO.
    #[test]
    fn test_degraded_mode_caps_strong_no_go_to_hold() {
        // 2 reject agents (agent_count < 3)
        // Assert label="HOLD (2-0)", NOT "STRONG NO-GO"
    }

    // -- Error cases --

    /// Determine rejects fewer than min_agents with InsufficientAgents.
    #[test]
    fn test_determine_rejects_fewer_than_min_agents() {
        // ConsensusConfig with min_agents=2, provide 1 agent
        // Assert Err(MagiError::InsufficientAgents { succeeded: 1, required: 2 })
    }

    /// Determine rejects duplicate agent names with Validation error.
    #[test]
    fn test_determine_rejects_duplicate_agent_names() {
        // Two agents both with AgentName::Melchior
        // Assert Err(MagiError::Validation(_))
    }

    // -- Score and confidence calculations --

    /// Epsilon-aware classification near score boundaries.
    #[test]
    fn test_epsilon_aware_classification_near_boundaries() {
        // Test score very close to 0, 1.0, -1.0 within epsilon
        // Verify correct classification
    }

    /// Confidence formula: base * weight_factor, clamped [0,1], rounded 2 decimals.
    #[test]
    fn test_confidence_formula_clamped_and_rounded() {
        // Construct agents with known confidence values
        // Compute expected: base = sum(majority_conf) / num_agents
        // weight_factor = (abs(score) + 1) / 2
        // confidence = base * weight_factor, clamped, rounded
        // Assert result matches expected
    }

    /// Majority summary joins majority agent summaries with " | ".
    #[test]
    fn test_majority_summary_joins_with_pipe() {
        // 2 approve agents with known summaries
        // Assert majority_summary contains both summaries separated by " | "
    }

    /// Conditions extracted from agents with Conditional verdict.
    #[test]
    fn test_conditions_extracted_from_conditional_agents() {
        // Agent with Conditional verdict and recommendation text
        // Assert conditions vec contains entry with that agent and recommendation
    }

    /// Recommendations map includes all agents.
    #[test]
    fn test_recommendations_includes_all_agents() {
        // 3 agents with different recommendations
        // Assert recommendations map has 3 entries
    }

    /// ConsensusConfig enforces min_agents >= 1.
    #[test]
    fn test_consensus_config_enforces_min_agents_at_least_one() {
        // Attempt to create ConsensusConfig with min_agents=0
        // Assert it is corrected to 1 or returns error
    }
}
```

## Implementation Details (Green Phase)

### `ConsensusConfig` Struct

A `#[non_exhaustive]` configuration struct for the consensus engine.

- **Fields**:
  - `min_agents: usize` -- minimum number of successful agent outputs required (default: `2`). Enforced to be >= 1 to prevent division by zero. If set to 0, clamp to 1.
  - `epsilon: f64` -- tolerance for floating-point comparisons (default: `1e-9`)
- **Derives**: `Debug`, `Clone`
- **Implements**: `Default`

### `ConsensusEngine` Struct

Holds `config: ConsensusConfig`. The engine is constructed once and reused across calls.

- **Constructor**: `new(config: ConsensusConfig) -> Self`
- **Main method**: `determine(&self, agents: &[AgentOutput]) -> Result<ConsensusResult, MagiError>`

### Consensus Algorithm (inside `determine`)

The algorithm proceeds in these steps:

1. **Validate input count**: if `agents.len() < config.min_agents`, return `MagiError::InsufficientAgents { succeeded: agents.len(), required: config.min_agents }`.

2. **Reject duplicates**: collect agent names into a set. If any name appears more than once, return `MagiError::Validation("duplicate agent name: {name}")`.

3. **Compute normalized score**: `score = sum(agent.verdict.weight() for agent in agents) / agents.len() as f64`. Range is [-1.0, +1.0].

4. **Determine majority verdict**: count effective verdicts (using `verdict.effective()`). The side with more votes wins. Binary result: either `Approve` or `Reject`. On tie, break by comparing the first agent on each side using `AgentName::cmp()` (alphabetically first agent's side wins -- this means ties are deterministic).

5. **Classify score to label + consensus verdict** using epsilon-aware comparisons:
   - `|score - 1.0| < epsilon` → label = `"STRONG GO"`, verdict = `Approve`
   - `|score - (-1.0)| < epsilon` → label = `"STRONG NO-GO"`, verdict = `Reject`
   - `score > epsilon` and any agent has `Conditional` verdict → label = `"GO WITH CAVEATS"`, verdict = `Approve`
   - `score > epsilon` and no conditionals → label = `"GO ({approve_count}-{reject_count})"`, verdict = `Approve`
   - `|score| < epsilon` → label = `"HOLD -- TIE"`, verdict = `Reject`
   - `score < -epsilon` → label = `"HOLD ({reject_count}-{approve_count})"`, verdict = `Reject`

6. **Degraded mode cap**: if `agents.len() < 3`, replace `"STRONG GO"` with `"GO ({n}-0)"` and `"STRONG NO-GO"` with `"HOLD ({n}-0)"` where `n = agents.len()`.

7. **Compute confidence**:
   - `base_confidence = sum(confidence for agents on majority side) / agents.len() as f64`
   - `weight_factor = (score.abs() + 1.0) / 2.0`
   - `confidence = base_confidence * weight_factor`
   - Clamp to `[0.0, 1.0]`
   - Round to 2 decimal places: `(confidence * 100.0).round() / 100.0`

8. **Deduplicate findings**: group all findings from all agents by `stripped_title().to_lowercase()`. For each group:
   - Title: use the title from the finding with highest severity (or first agent by `AgentName::Ord` on tie)
   - Severity: promote to highest in the group
   - Detail: use detail from the finding with highest severity (or first agent on tie)
   - Sources: collect all agent names that contributed this finding

9. **Identify dissent**: agents whose `effective_verdict() != majority_verdict`. For each dissenter, create a `Dissent { agent, summary, reasoning }`.

10. **Extract conditions**: agents with `Verdict::Conditional`. For each, create a `Condition { agent, condition: agent.recommendation.clone() }`.

11. **Build votes map**: `BTreeMap<AgentName, Verdict>` from each agent.

12. **Build majority summary**: join summaries of agents on the majority side with `" | "`.

13. **Build recommendations map**: `BTreeMap<AgentName, String>` from each agent's recommendation.

14. **Construct and return `ConsensusResult`**.

### `ConsensusResult` Struct

- **Fields**:
  - `consensus: String` -- the classification label (e.g., "STRONG GO", "GO WITH CAVEATS")
  - `consensus_verdict: Verdict` -- the final verdict enum
  - `confidence: f64` -- computed confidence, rounded to 2 decimals
  - `score: f64` -- raw normalized score
  - `agent_count: usize` -- number of agents that contributed
  - `votes: BTreeMap<AgentName, Verdict>` -- per-agent verdicts
  - `majority_summary: String` -- joined summaries from majority side
  - `dissent: Vec<Dissent>` -- dissenting agent details
  - `findings: Vec<DedupFinding>` -- deduplicated findings sorted by severity (Critical first)
  - `conditions: Vec<Condition>` -- conditions from Conditional agents
  - `recommendations: BTreeMap<AgentName, String>` -- per-agent recommendations
- **Derives**: `Debug`, `Clone`, `Serialize`, `Deserialize`

### Supporting Structs

#### `DedupFinding`

- **Fields**: `severity: Severity`, `title: String`, `detail: String`, `sources: Vec<AgentName>`
- **Derives**: `Debug`, `Clone`, `Serialize`, `Deserialize`, `PartialEq`

#### `Dissent`

- **Fields**: `agent: AgentName`, `summary: String`, `reasoning: String`
- **Derives**: `Debug`, `Clone`, `Serialize`, `Deserialize`, `PartialEq`

#### `Condition`

- **Fields**: `agent: AgentName`, `condition: String`
- **Derives**: `Debug`, `Clone`, `Serialize`, `Deserialize`, `PartialEq`

### `lib.rs` Module Declaration

Add `pub mod consensus;` to `src/lib.rs`.

## Constraints Checklist

- No `panic!`, `unwrap()`, `expect()` outside `#[cfg(test)]`
- No `unsafe`
- All public types, fields, and methods have `///` Rustdoc
- All float comparisons use epsilon (never direct `==` on `f64`)
- `BTreeMap` used instead of `HashMap` for deterministic ordering in serialized output
- `rustfmt` and `clippy --tests -- -D warnings` clean
- File header present on all new files
- Stateless design -- no `&mut self` on `determine`

## Refactor Phase Notes

After Green phase passes all tests:

- Extract classification logic into a private helper method for readability
- Extract deduplication logic into a private helper method
- Verify all float operations handle edge cases (NaN, infinity -- though these should not occur with valid confidence values)
- Add Rustdoc `///` explaining the algorithm at the `determine` method level
- Confirm `cargo doc --no-deps` generates clean documentation
- Consider named constants for classification labels ("STRONG GO", etc.) to avoid string literals scattered through the code
