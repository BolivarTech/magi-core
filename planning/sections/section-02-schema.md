# Section 02: Domain Schema Types (`schema.rs`)

## Overview

This section implements the core domain types that every other module depends on: four enums (`Verdict`, `Severity`, `Mode`, `AgentName`) and two structs (`Finding`, `AgentOutput`). All types are pure data with encapsulated behavior methods -- no async, no I/O, no external dependencies beyond `serde` and `regex`. This module is the vocabulary of the entire crate.

## Dependencies

- **External crates**: `serde = { version = "1", features = ["derive"] }`, `serde_json = "1"`, `regex = "1"` (already in `Cargo.toml` or added now)
- **Internal sections**: Section 01 (`error.rs`) must be complete -- `Finding::stripped_title()` uses a compiled `Regex`, and downstream consumers rely on `MagiError`.
- **Standard library**: `std::fmt`, `std::cmp::Ordering`

## Files to Create or Modify

| File | Action |
|------|--------|
| `magi-core/Cargo.toml` | Ensure `regex = "1"` is in `[dependencies]` |
| `magi-core/src/schema.rs` | Create -- contains all enums and structs |
| `magi-core/src/lib.rs` | Add `pub mod schema;` |

## File Header

Every new source file in this project must start with:

```rust
// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05
```

## Tests (Write First -- Red Phase)

All tests go in `src/schema.rs` inside a `#[cfg(test)] mod tests` block. Write these tests before any implementation. They must fail (or not compile) until the Green phase.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // -- Verdict tests --

    /// Verdict::Approve weight is 1.0.
    #[test]
    fn test_verdict_approve_weight_is_positive_one() {
        // Construct Verdict::Approve, call weight(), assert_eq 1.0
    }

    /// Verdict::Reject weight is -1.0.
    #[test]
    fn test_verdict_reject_weight_is_negative_one() {
        // Construct Verdict::Reject, call weight(), assert_eq -1.0
    }

    /// Verdict::Conditional weight is 0.5.
    #[test]
    fn test_verdict_conditional_weight_is_half() {
        // Construct Verdict::Conditional, call weight(), assert_eq 0.5
    }

    /// Verdict::Conditional effective maps to Approve.
    #[test]
    fn test_verdict_conditional_effective_maps_to_approve() {
        // assert_eq!(Verdict::Conditional.effective(), Verdict::Approve)
    }

    /// Verdict::Approve effective maps to Approve (identity).
    #[test]
    fn test_verdict_approve_effective_is_identity() {
        // assert_eq!(Verdict::Approve.effective(), Verdict::Approve)
    }

    /// Verdict::Reject effective maps to Reject (identity).
    #[test]
    fn test_verdict_reject_effective_is_identity() {
        // assert_eq!(Verdict::Reject.effective(), Verdict::Reject)
    }

    /// Verdict Display outputs uppercase: "APPROVE", "REJECT", "CONDITIONAL".
    #[test]
    fn test_verdict_display_outputs_uppercase() {
        // assert_eq!(format!("{}", Verdict::Approve), "APPROVE")
        // assert_eq!(format!("{}", Verdict::Reject), "REJECT")
        // assert_eq!(format!("{}", Verdict::Conditional), "CONDITIONAL")
    }

    /// Verdict serializes as lowercase ("approve", "reject", "conditional").
    #[test]
    fn test_verdict_serializes_as_lowercase() {
        // serde_json::to_string(&Verdict::Approve) == "\"approve\""
    }

    /// Verdict deserializes from lowercase strings.
    #[test]
    fn test_verdict_deserializes_from_lowercase() {
        // serde_json::from_str::<Verdict>("\"approve\"") == Ok(Verdict::Approve)
    }

    // -- Severity tests --

    /// Severity ordering: Critical > Warning > Info.
    #[test]
    fn test_severity_ordering_critical_greater_than_warning_greater_than_info() {
        // assert!(Severity::Critical > Severity::Warning);
        // assert!(Severity::Warning > Severity::Info);
    }

    /// Severity icon returns "[!!!]", "[!!]", "[i]".
    #[test]
    fn test_severity_icon_returns_correct_strings() {
        // assert_eq!(Severity::Critical.icon(), "[!!!]")
        // assert_eq!(Severity::Warning.icon(), "[!!]")
        // assert_eq!(Severity::Info.icon(), "[i]")
    }

    /// Severity Display outputs "CRITICAL", "WARNING", "INFO".
    #[test]
    fn test_severity_display_outputs_uppercase() {
        // assert_eq!(format!("{}", Severity::Critical), "CRITICAL")
    }

    /// Severity serializes as lowercase.
    #[test]
    fn test_severity_serializes_as_lowercase() {
        // serde_json::to_string(&Severity::Critical) == "\"critical\""
    }

    // -- Mode tests --

    /// Mode Display outputs "code-review", "design", "analysis".
    #[test]
    fn test_mode_display_outputs_hyphenated_lowercase() {
        // assert_eq!(format!("{}", Mode::CodeReview), "code-review")
        // assert_eq!(format!("{}", Mode::Design), "design")
        // assert_eq!(format!("{}", Mode::Analysis), "analysis")
    }

    /// Mode serializes as lowercase with hyphens.
    #[test]
    fn test_mode_serializes_as_lowercase_with_hyphens() {
        // serde_json::to_string(&Mode::CodeReview) == "\"code-review\""
    }

    // -- AgentName tests --

    /// AgentName title returns "Scientist", "Pragmatist", "Critic".
    #[test]
    fn test_agent_name_title_returns_role() {
        // assert_eq!(AgentName::Melchior.title(), "Scientist")
        // assert_eq!(AgentName::Balthasar.title(), "Pragmatist")
        // assert_eq!(AgentName::Caspar.title(), "Critic")
    }

    /// AgentName display_name returns "Melchior", "Balthasar", "Caspar".
    #[test]
    fn test_agent_name_display_name_returns_name() {
        // assert_eq!(AgentName::Melchior.display_name(), "Melchior")
    }

    /// AgentName Ord follows alphabetical: Balthasar < Caspar < Melchior.
    #[test]
    fn test_agent_name_ord_is_alphabetical() {
        // assert!(AgentName::Balthasar < AgentName::Caspar);
        // assert!(AgentName::Caspar < AgentName::Melchior);
    }

    /// AgentName serializes as lowercase.
    #[test]
    fn test_agent_name_serializes_as_lowercase() {
        // serde_json::to_string(&AgentName::Melchior) == "\"melchior\""
    }

    /// AgentName implements Eq and Hash (usable as BTreeMap key).
    #[test]
    fn test_agent_name_usable_as_btreemap_key() {
        // let mut map = std::collections::BTreeMap::new();
        // map.insert(AgentName::Melchior, "value");
        // assert_eq!(map.get(&AgentName::Melchior), Some(&"value"));
    }

    // -- Finding tests --

    /// Finding stripped_title removes zero-width characters (U+200B, U+FEFF, U+200C).
    #[test]
    fn test_finding_stripped_title_removes_zero_width_characters() {
        // Construct Finding with title containing \u{200B}, \u{FEFF}, \u{200C}
        // Assert stripped_title() returns string without those characters
    }

    /// Finding stripped_title preserves normal text.
    #[test]
    fn test_finding_stripped_title_preserves_normal_text() {
        // Construct Finding with title "Normal title"
        // Assert stripped_title() returns "Normal title"
    }

    /// Finding serializes/deserializes roundtrip.
    #[test]
    fn test_finding_serde_roundtrip() {
        // Construct Finding, serialize to JSON, deserialize back, assert equal
    }

    // -- AgentOutput tests --

    /// AgentOutput is_approving true for Approve.
    #[test]
    fn test_agent_output_is_approving_true_for_approve() {
        // Build AgentOutput with verdict=Approve, assert is_approving() == true
    }

    /// AgentOutput is_approving true for Conditional.
    #[test]
    fn test_agent_output_is_approving_true_for_conditional() {
        // Build AgentOutput with verdict=Conditional, assert is_approving() == true
    }

    /// AgentOutput is_approving false for Reject.
    #[test]
    fn test_agent_output_is_approving_false_for_reject() {
        // Build AgentOutput with verdict=Reject, assert is_approving() == false
    }

    /// AgentOutput is_dissenting true when effective verdict differs from majority.
    #[test]
    fn test_agent_output_is_dissenting_when_verdict_differs_from_majority() {
        // Build AgentOutput with verdict=Reject, call is_dissenting(Verdict::Approve)
        // Assert true
    }

    /// AgentOutput is_dissenting false when effective verdict matches majority.
    #[test]
    fn test_agent_output_is_not_dissenting_when_verdict_matches_majority() {
        // Build AgentOutput with verdict=Approve, call is_dissenting(Verdict::Approve)
        // Assert false
    }

    /// AgentOutput effective_verdict maps Conditional to Approve.
    #[test]
    fn test_agent_output_effective_verdict_maps_conditional_to_approve() {
        // Build AgentOutput with verdict=Conditional
        // assert_eq!(output.effective_verdict(), Verdict::Approve)
    }

    /// AgentOutput serializes/deserializes roundtrip with all fields.
    #[test]
    fn test_agent_output_serde_roundtrip() {
        // Construct full AgentOutput, serialize to JSON, deserialize back, assert equal
    }
}
```

## Implementation Details (Green Phase)

### `Verdict` Enum

An enum with three variants representing an agent's judgment.

- **Variants**: `Approve`, `Reject`, `Conditional`
- **Derives**: `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`, `Serialize`, `Deserialize`
- **Serde**: Use `#[serde(rename_all = "lowercase")]` so serialization produces `"approve"`, `"reject"`, `"conditional"`
- **Methods**:
  - `weight(&self) -> f64` -- returns `+1.0` for Approve, `-1.0` for Reject, `+0.5` for Conditional
  - `effective(&self) -> Verdict` -- returns `Approve` for both Approve and Conditional, `Reject` for Reject. Used for majority counting where Conditional counts as an approval.
- **`Display` impl**: outputs uppercase strings: `"APPROVE"`, `"REJECT"`, `"CONDITIONAL"`

### `Severity` Enum

Represents the severity level of a finding.

- **Variants**: `Critical`, `Warning`, `Info`
- **Derives**: `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`, `Serialize`, `Deserialize`
- **Serde**: `#[serde(rename_all = "lowercase")]`
- **`Ord` / `PartialOrd`**: Critical > Warning > Info. Implement manually or derive with variant ordering (Critical first in enum definition if using derived Ord, but verify direction -- derived `Ord` on enums uses discriminant order, so define variants as `Critical`, `Warning`, `Info` and then derived `Ord` gives `Critical < Warning < Info` which is wrong. Therefore implement `Ord` manually or reverse the definition order).
- **Methods**:
  - `icon(&self) -> &'static str` -- returns `"[!!!]"` for Critical, `"[!!]"` for Warning, `"[i]"` for Info
- **`Display` impl**: outputs uppercase strings: `"CRITICAL"`, `"WARNING"`, `"INFO"`

### `Mode` Enum

Represents the analysis mode.

- **Variants**: `CodeReview`, `Design`, `Analysis`
- **Derives**: `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`, `Serialize`, `Deserialize`
- **Serde**: Custom serialization needed because `CodeReview` must serialize as `"code-review"` (with hyphen). Use `#[serde(rename_all = "kebab-case")]` or manual rename attributes.
- **`Display` impl**: outputs `"code-review"`, `"design"`, `"analysis"`

### `AgentName` Enum

Identifies the three MAGI agents.

- **Variants**: `Melchior`, `Balthasar`, `Caspar`
- **Derives**: `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`, `Serialize`, `Deserialize`
- **Serde**: `#[serde(rename_all = "lowercase")]`
- **`Ord` / `PartialOrd`**: Alphabetical ordering (Balthasar < Caspar < Melchior). This is used for deterministic tiebreaking in consensus. Implement manually to guarantee alphabetical order regardless of variant definition order.
- **Methods**:
  - `title(&self) -> &'static str` -- returns the agent's analytical role: `"Scientist"` for Melchior, `"Pragmatist"` for Balthasar, `"Critic"` for Caspar
  - `display_name(&self) -> &'static str` -- returns the agent's name as a string: `"Melchior"`, `"Balthasar"`, `"Caspar"`

### `Finding` Struct

Represents a single finding reported by an agent.

- **Fields**: `severity: Severity`, `title: String`, `detail: String`
- **Derives**: `Debug`, `Clone`, `PartialEq`, `Eq`, `Hash`, `Serialize`, `Deserialize`
- **Methods**:
  - `stripped_title(&self) -> String` -- removes Unicode category Cf (format) characters from the title using a precompiled `Regex`. Characters to strip include zero-width space (U+200B), zero-width non-joiner (U+200C), zero-width joiner (U+200D), byte order mark (U+FEFF), and other Cf category chars. The regex pattern is `[\u{00AD}\u{0600}-\u{0605}\u{061C}\u{06DD}\u{070F}\u{08E2}\u{180E}\u{200B}-\u{200F}\u{202A}-\u{202E}\u{2060}-\u{2064}\u{2066}-\u{206F}\u{FEFF}\u{FFF9}-\u{FFFB}]` or equivalent. The method must compile the regex each call, or accept an externally-provided compiled regex. In this section, use `regex::Regex::new()` internally (the `Validator` in section 03 will precompile and reuse).

### `AgentOutput` Struct

The deserialized output from a single LLM agent.

- **Fields**:
  - `agent: AgentName`
  - `verdict: Verdict`
  - `confidence: f64` -- value between 0.0 and 1.0 (validation enforced by `Validator`, not here)
  - `summary: String`
  - `reasoning: String`
  - `findings: Vec<Finding>`
  - `recommendation: String`
- **Derives**: `Debug`, `Clone`, `PartialEq`, `Serialize`, `Deserialize` (no `Eq` or `Hash` because `f64` does not implement them)
- **Methods**:
  - `is_approving(&self) -> bool` -- returns `true` if verdict is `Approve` or `Conditional`
  - `is_dissenting(&self, majority: Verdict) -> bool` -- returns `true` if `self.effective_verdict() != majority`
  - `effective_verdict(&self) -> Verdict` -- delegates to `self.verdict.effective()`

### `lib.rs` Module Declaration

Add `pub mod schema;` to `src/lib.rs` (after `pub mod error;`).

## Constraints Checklist

- No `panic!`, `unwrap()`, `expect()` outside `#[cfg(test)]`
- No `unsafe`
- All public types, variants, and methods have `///` Rustdoc
- `#[serde(rename_all = ...)]` attributes for consistent JSON serialization
- `rustfmt` and `clippy --tests -- -D warnings` clean
- File header present on all new files
- All enums used as map keys implement `Ord` where needed

## Refactor Phase Notes

After Green phase passes all tests:

- Verify `Display` output is consistent in style across all enums (all uppercase for enum Display)
- Ensure serde roundtrip works for every type (serialize then deserialize yields equal value)
- Add Rustdoc `///` on each type, variant, and public method explaining its purpose
- Confirm `cargo doc --no-deps` generates clean documentation
- Verify `AgentName::Ord` is truly alphabetical by testing all pair orderings
