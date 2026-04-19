# Section 05: Reporting (`reporting.rs`)

## Overview

This section implements `ReportFormatter`, `ReportConfig`, and `MagiReport`. The formatter generates fixed-width ASCII banners (exactly 52 characters wide per line) and full markdown reports from agent outputs and consensus results. `MagiReport` is the final output struct returned by the orchestrator, containing all analysis data plus the formatted report string. The reporting module is pure string formatting -- no async, no I/O.

## Dependencies

- **External crates**: `serde` (for `MagiReport` serialization)
- **Internal sections**:
  - Section 01 (`error.rs`) -- `MagiError` (not directly used, but part of crate structure)
  - Section 02 (`schema.rs`) -- `Verdict`, `Severity`, `AgentName`, `AgentOutput`, `Mode`
  - Section 04 (`consensus.rs`) -- `ConsensusResult`, `DedupFinding`, `Dissent`, `Condition`
- **Standard library**: `std::fmt::Write`, `std::collections::BTreeMap`

## Files to Create or Modify

| File | Action |
|------|--------|
| `magi-core/src/reporting.rs` | Create -- contains `ReportFormatter`, `ReportConfig`, `MagiReport` |
| `magi-core/src/lib.rs` | Add `pub mod reporting;` |

## File Header

Every new source file in this project must start with:

```rust
// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05
```

## Tests (Write First -- Red Phase)

All tests go in `src/reporting.rs` inside a `#[cfg(test)] mod tests` block.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::*;
    use crate::consensus::*;

    // Helper: build minimal ConsensusResult for testing
    // Helper: build minimal AgentOutput list for testing

    // -- BDD Scenario 15: banner width --

    /// All banner lines are exactly 52 characters wide.
    #[test]
    fn test_banner_lines_are_exactly_52_chars_wide() {
        // Generate banner from 3 agents + consensus result
        // Split by newline, assert each non-empty line is exactly 52 chars
    }

    /// Banner with long agent names still fits 52 chars via padding/truncation.
    #[test]
    fn test_banner_with_long_content_fits_52_chars() {
        // Generate banner with long consensus label
        // Assert all lines remain 52 chars
    }

    // -- BDD Scenario 16: report sections --

    /// Report with mixed consensus contains all 5 markdown headers.
    #[test]
    fn test_report_with_mixed_consensus_contains_all_headers() {
        // Build consensus with dissent, conditions, findings
        // Assert report contains "## Consensus Summary", "## Key Findings",
        // "## Dissenting Opinion", "## Conditions for Approval", "## Recommended Actions"
    }

    /// Report without dissent omits "## Dissenting Opinion".
    #[test]
    fn test_report_without_dissent_omits_dissent_section() {
        // Build unanimous consensus (no dissent)
        // Assert report does NOT contain "## Dissenting Opinion"
    }

    /// Report without conditions omits "## Conditions for Approval".
    #[test]
    fn test_report_without_conditions_omits_conditions_section() {
        // Build consensus without Conditional verdicts
        // Assert report does NOT contain "## Conditions for Approval"
    }

    /// Report without findings omits "## Key Findings".
    #[test]
    fn test_report_without_findings_omits_findings_section() {
        // Build consensus with empty findings vec
        // Assert report does NOT contain "## Key Findings"
    }

    // -- Banner formatting --

    /// format_banner generates correct ASCII art structure.
    #[test]
    fn test_format_banner_has_correct_structure() {
        // Assert banner starts with separator line
        // Assert banner contains "MAGI SYSTEM -- VERDICT"
        // Assert banner contains agent lines
        // Assert banner contains "CONSENSUS:" line
        // Assert banner ends with separator line
    }

    /// format_init_banner shows mode, model, timeout.
    #[test]
    fn test_format_init_banner_shows_mode_model_timeout() {
        // Call format_init_banner with mode=CodeReview, model="claude-sonnet", timeout=300
        // Assert output contains "code-review", "claude-sonnet", "300"
    }

    /// Separator line is "+" + "=" * 50 + "+".
    #[test]
    fn test_separator_format() {
        // Assert separator is "+==...==+" with exactly 50 '=' between '+' chars
        // Total: 52 chars
    }

    /// Agent line shows "Name (Title):  VERDICT (NN%)" format.
    #[test]
    fn test_agent_line_format() {
        // Generate banner, extract agent line
        // Assert contains "Melchior (Scientist):" and "APPROVE" and percentage
    }

    // -- Report content sections --

    /// Findings section shows icon + severity + title + sources + detail.
    #[test]
    fn test_findings_section_format() {
        // Build consensus with DedupFinding
        // Assert report contains icon, "[SEVERITY]", title, "(from agents)"
    }

    /// Dissent section shows agent name, summary, full reasoning.
    #[test]
    fn test_dissent_section_format() {
        // Build consensus with Dissent entry
        // Assert report contains agent display name, summary text, reasoning text
    }

    /// Conditions section shows bulleted list with agent names.
    #[test]
    fn test_conditions_section_format() {
        // Build consensus with Condition entry
        // Assert report contains "- " bullet, agent name, condition text
    }

    /// Recommendations section shows per-agent recommendations.
    #[test]
    fn test_recommendations_section_format() {
        // Build consensus with recommendations map
        // Assert report contains each agent name and recommendation
    }

    /// Agent display falls back to AgentName methods when not in config.
    #[test]
    fn test_agent_display_fallback_to_agent_name_methods() {
        // Create ReportConfig with empty agent_titles
        // Assert formatter uses AgentName::display_name() and AgentName::title()
    }

    // -- MagiReport tests --

    /// MagiReport serializes to JSON.
    #[test]
    fn test_magi_report_serializes_to_json() {
        // Build MagiReport, serialize to JSON string
        // Assert valid JSON, contains expected keys
    }

    /// degraded=false when all 3 agents succeed.
    #[test]
    fn test_magi_report_not_degraded_with_three_agents() {
        // Build MagiReport with 3 agents, degraded=false
        // Assert degraded field is false
    }

    /// degraded=true with failed_agents populated when agent fails.
    #[test]
    fn test_magi_report_degraded_with_failed_agents() {
        // Build MagiReport with degraded=true, failed_agents=[Caspar]
        // Assert degraded is true and failed_agents contains Caspar
    }

    /// Agent names in JSON are lowercase.
    #[test]
    fn test_magi_report_json_agent_names_lowercase() {
        // Serialize MagiReport to JSON string
        // Assert JSON contains "melchior" not "Melchior"
    }

    /// consensus.confidence is rounded to 2 decimals.
    #[test]
    fn test_magi_report_confidence_rounded() {
        // Build with confidence = 0.8567
        // Assert serialized confidence is 0.86 (or check the field directly)
    }
}
```

## Implementation Details (Green Phase)

### `ReportConfig` Struct

A `#[non_exhaustive]` configuration struct for the report formatter.

- **Fields**:
  - `banner_width: usize` -- total width of the ASCII banner, including border chars (default: `52`)
  - `agent_titles: BTreeMap<AgentName, (String, String)>` -- maps agent name to `(display_name, title)` for override. Default: populated from `AgentName::display_name()` and `AgentName::title()` for all three agents.
- **Derives**: `Debug`, `Clone`
- **Implements**: `Default`

### `ReportFormatter` Struct

Holds `config: ReportConfig` and `banner_inner: usize` (calculated as `config.banner_width - 2`, representing the usable space between border characters).

- **Constructor**: `new(config: ReportConfig) -> Self` -- computes `banner_inner`

- **Public methods**:

  - `format_banner(&self, agents: &[AgentOutput], consensus: &ConsensusResult) -> String`

    Generates the fixed-width ASCII verdict box. Every line is exactly `banner_width` (52) characters. Structure:
    ```
    +==================================================+
    |          MAGI SYSTEM -- VERDICT                  |
    +==================================================+
    |  Melchior (Scientist):  APPROVE (90%)            |
    |  Balthasar (Pragmatist):  CONDITIONAL (85%)      |
    |  Caspar (Critic):  REJECT (78%)                  |
    +==================================================+
    |  CONSENSUS: GO WITH CAVEATS                      |
    +==================================================+
    ```

    The separator line is `+` followed by `banner_inner` `=` characters followed by `+`. Content lines are `|` followed by content padded to `banner_inner` characters followed by `|`. Content is left-aligned with 2-space indent. Confidence is formatted as integer percentage: `(confidence * 100.0).round() as u32`.

  - `format_init_banner(&self, mode: &Mode, model: &str, timeout_secs: u64) -> String`

    Generates a pre-analysis initialization box with mode, model, and timeout info. Same 52-char width. Content includes:
    ```
    +==================================================+
    |          MAGI SYSTEM -- INIT                     |
    +==================================================+
    |  Mode:     code-review                           |
    |  Model:    claude-sonnet-4-6                     |
    |  Timeout:  300s                                  |
    +==================================================+
    ```

  - `format_report(&self, agents: &[AgentOutput], consensus: &ConsensusResult) -> String`

    Generates the full markdown report. Concatenates sections in order:
    1. Banner (from `format_banner`)
    2. `## Consensus Summary` -- `consensus.majority_summary`
    3. `## Key Findings` (only if `consensus.findings` is non-empty) -- each finding formatted as: `{icon} **[{SEVERITY}]** {title} _(from {sources joined by ", "})_` followed by detail on the next line
    4. `## Dissenting Opinion` (only if `consensus.dissent` is non-empty) -- each dissent formatted with agent display name as header, summary, then full reasoning
    5. `## Conditions for Approval` (only if `consensus.conditions` is non-empty) -- bulleted list: `- **{agent_display_name}**: {condition}`
    6. `## Recommended Actions` -- per-agent recommendations from `consensus.recommendations`

- **Private helper methods**:
  - `format_separator(&self) -> String` -- generates `+{"=" * banner_inner}+`
  - `format_line(&self, content: &str) -> String` -- generates `|{content padded to banner_inner}|`, truncating if needed
  - `agent_display(&self, agent: &AgentName) -> (&str, &str)` -- returns `(display_name, title)` from config, falling back to `AgentName` methods

### `MagiReport` Struct

The final output struct returned by the orchestrator's `analyze()` method.

- **Fields**:
  - `agents: Vec<AgentOutput>` -- the successful agent outputs used in analysis
  - `consensus: ConsensusResult` -- the computed consensus
  - `banner: String` -- the ASCII banner string
  - `report: String` -- the full markdown report string
  - `degraded: bool` -- `true` if fewer than 3 agents succeeded
  - `failed_agents: Vec<AgentName>` -- names of agents that failed (timeout, parse error, etc.)
- **Derives**: `Debug`, `Clone`, `Serialize`
- **Note**: `MagiReport` implements `Serialize` for JSON output. Agent names serialize as lowercase (handled by `AgentName`'s serde config). Confidence values should already be rounded to 2 decimals by the consensus engine.

### `lib.rs` Module Declaration

Add `pub mod reporting;` to `src/lib.rs`.

## Constraints Checklist

- No `panic!`, `unwrap()`, `expect()` outside `#[cfg(test)]`
- No `unsafe`
- All public types and methods have `///` Rustdoc
- Banner width is exactly 52 characters on every line -- no exceptions
- Conditional sections omitted entirely (not just empty) when data is absent
- `BTreeMap` for `agent_titles` to ensure deterministic iteration order
- `rustfmt` and `clippy --tests -- -D warnings` clean
- File header present on all new files

## Refactor Phase Notes

After Green phase passes all tests:

- Verify banner width invariant with edge cases (very long consensus labels, very long agent names)
- Ensure content truncation in banner lines preserves readability
- Add Rustdoc `///` on all public types and methods
- Confirm `cargo doc --no-deps` generates clean documentation
- Consider extracting banner constants (width, title text) into named constants
- Verify `MagiReport` JSON output matches the expected format from the Python original
