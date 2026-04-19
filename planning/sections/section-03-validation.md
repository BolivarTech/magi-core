# Section 03: Validation (`validate.rs`)

## Overview

This section implements the `Validator` struct and `ValidationLimits` config, responsible for validating `AgentOutput` fields before they reach the consensus engine. The validator checks confidence ranges, text field lengths, finding counts, and strips zero-width Unicode characters from finding titles. All validation failures return `MagiError::Validation` with descriptive messages including the field name for diagnostics. The validator is stateless per call -- it holds only precompiled state (regex, limits).

## Dependencies

- **External crates**: `regex = "1"` (must be in `Cargo.toml` under `[dependencies]`)
- **Internal sections**:
  - Section 01 (`error.rs`) -- `MagiError::Validation` for error returns
  - Section 02 (`schema.rs`) -- `AgentOutput`, `Finding` types being validated
- **Standard library**: None beyond what is already used

## Files to Create or Modify

| File | Action |
|------|--------|
| `magi-core/Cargo.toml` | Ensure `regex = "1"` is in `[dependencies]` |
| `magi-core/src/validate.rs` | Create -- contains `Validator` and `ValidationLimits` |
| `magi-core/src/lib.rs` | Add `pub mod validate;` |

## File Header

Every new source file in this project must start with:

```rust
// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05
```

## Tests (Write First -- Red Phase)

All tests go in `src/validate.rs` inside a `#[cfg(test)] mod tests` block. Write these tests before any implementation. They must fail (or not compile) until the Green phase.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::*;

    // Helper: build a valid AgentOutput for baseline tests
    // fn valid_agent_output() -> AgentOutput { ... }

    /// Validator::new creates instance with default limits and compiled regex.
    #[test]
    fn test_validator_new_creates_with_default_limits() {
        // Construct Validator::new()
        // Assert limits match ValidationLimits::default()
    }

    /// Validator::with_limits uses custom limits.
    #[test]
    fn test_validator_with_limits_uses_custom_limits() {
        // Construct custom ValidationLimits with max_findings=5
        // Create Validator::with_limits(custom)
        // Assert limits.max_findings == 5
    }

    // -- BDD Scenario 10: confidence out of range --

    /// Validate rejects confidence > 1.0 with MagiError::Validation.
    #[test]
    fn test_validate_rejects_confidence_above_one() {
        // Build AgentOutput with confidence=1.5
        // Assert validator.validate(&output) is Err(MagiError::Validation(_))
        // Assert error message contains "confidence"
    }

    /// Validate rejects confidence < 0.0 with MagiError::Validation.
    #[test]
    fn test_validate_rejects_confidence_below_zero() {
        // Build AgentOutput with confidence=-0.1
        // Assert validator.validate(&output) is Err(MagiError::Validation(_))
    }

    /// Validate accepts confidence at boundaries (0.0 and 1.0).
    #[test]
    fn test_validate_accepts_confidence_at_boundaries() {
        // Build AgentOutput with confidence=0.0, validate, assert Ok
        // Build AgentOutput with confidence=1.0, validate, assert Ok
    }

    // -- BDD Scenario 11: empty title after strip zero-width --

    /// Validate rejects finding with title composed entirely of zero-width chars.
    #[test]
    fn test_validate_rejects_finding_with_only_zero_width_title() {
        // Build Finding with title = "\u{200B}\u{FEFF}\u{200C}"
        // Build AgentOutput with that finding
        // Assert validator.validate(&output) is Err(MagiError::Validation(_))
        // Assert error message contains "title"
    }

    /// Validate accepts finding with normal title.
    #[test]
    fn test_validate_accepts_finding_with_normal_title() {
        // Build Finding with title = "Security vulnerability"
        // Build AgentOutput with that finding
        // Assert validator.validate(&output) is Ok(())
    }

    // -- BDD Scenario 12: text field exceeds max_text_len --

    /// Validate rejects reasoning exceeding max_text_len.
    #[test]
    fn test_validate_rejects_reasoning_exceeding_max_text_len() {
        // Build AgentOutput with reasoning = "x".repeat(50_001) (default max is 50_000)
        // Assert validator.validate(&output) is Err(MagiError::Validation(_))
        // Assert error message contains "reasoning"
    }

    /// Validate rejects summary exceeding max_text_len.
    #[test]
    fn test_validate_rejects_summary_exceeding_max_text_len() {
        // Build AgentOutput with summary = "x".repeat(50_001)
        // Assert Err with message containing "summary"
    }

    /// Validate rejects recommendation exceeding max_text_len.
    #[test]
    fn test_validate_rejects_recommendation_exceeding_max_text_len() {
        // Build AgentOutput with recommendation = "x".repeat(50_001)
        // Assert Err with message containing "recommendation"
    }

    // -- Additional validation tests --

    /// Validate rejects findings count exceeding max_findings.
    #[test]
    fn test_validate_rejects_findings_count_exceeding_max_findings() {
        // Build AgentOutput with 101 findings (default max is 100)
        // Assert Err with message containing "findings"
    }

    /// Validate rejects finding title exceeding max_title_len.
    #[test]
    fn test_validate_rejects_finding_title_exceeding_max_title_len() {
        // Build Finding with title = "x".repeat(501) (default max is 500)
        // Assert Err
    }

    /// Validate rejects finding detail exceeding max_detail_len.
    #[test]
    fn test_validate_rejects_finding_detail_exceeding_max_detail_len() {
        // Build Finding with detail = "x".repeat(10_001) (default max is 10_000)
        // Assert Err
    }

    /// Validate accepts valid AgentOutput with all fields within limits.
    #[test]
    fn test_validate_accepts_valid_agent_output() {
        // Build valid AgentOutput with reasonable values for all fields
        // Assert validator.validate(&output) is Ok(())
    }

    /// strip_zero_width removes Unicode category Cf characters.
    #[test]
    fn test_strip_zero_width_removes_cf_category_characters() {
        // Create Validator, call strip_zero_width on string with mixed content
        // Assert zero-width chars removed, normal chars preserved
    }
}
```

## Implementation Details (Green Phase)

### `ValidationLimits` Struct

A `#[non_exhaustive]` configuration struct with default values for all validation thresholds.

- **Fields**:
  - `max_findings: usize` -- maximum number of findings per agent output (default: `100`)
  - `max_title_len: usize` -- maximum character length for finding titles (default: `500`)
  - `max_detail_len: usize` -- maximum character length for finding details (default: `10_000`)
  - `max_text_len: usize` -- maximum character length for text fields (summary, reasoning, recommendation) (default: `50_000`)
  - `confidence_min: f64` -- minimum valid confidence value, inclusive (default: `0.0`)
  - `confidence_max: f64` -- maximum valid confidence value, inclusive (default: `1.0`)
- **Derives**: `Debug`, `Clone`
- **Implements**: `Default` with the values specified above

### `Validator` Struct

Holds validation state that is expensive to construct (precompiled regex) and reused across calls.

- **Fields**:
  - `limits: ValidationLimits` -- the active validation limits
  - `zero_width_pattern: Regex` -- precompiled regex for Unicode category Cf characters
- **Constructors**:
  - `new() -> Self` -- creates with `ValidationLimits::default()` and compiles the zero-width regex once
  - `with_limits(limits: ValidationLimits) -> Self` -- creates with custom limits and compiles the regex
- **Public method**:
  - `validate(&self, output: &AgentOutput) -> Result<(), MagiError>` -- calls sub-validators in order: confidence, summary, reasoning, recommendation, findings. Returns on first failure.
- **Private methods**:
  - `validate_confidence(&self, confidence: f64) -> Result<(), MagiError>` -- checks `confidence_min <= confidence <= confidence_max`
  - `validate_text_field(&self, field_name: &str, value: &str) -> Result<(), MagiError>` -- checks `value.len() <= limits.max_text_len`
  - `validate_findings(&self, findings: &[Finding]) -> Result<(), MagiError>` -- checks count <= max_findings, then validates each finding
  - `validate_finding(&self, finding: &Finding) -> Result<(), MagiError>` -- checks title length, detail length, and that stripped title is not empty
  - `strip_zero_width(&self, text: &str) -> String` -- uses `self.zero_width_pattern.replace_all(text, "")` to remove zero-width characters

### Regex Pattern

The zero-width character regex should match Unicode category Cf (format) characters. The pattern covers at minimum:

- U+00AD (soft hyphen)
- U+0600-U+0605, U+061C, U+06DD, U+070F, U+08E2 (Arabic format chars)
- U+180E (Mongolian vowel separator)
- U+200B-U+200F (zero-width space, ZWNJ, ZWJ, directional marks)
- U+202A-U+202E (directional formatting)
- U+2060-U+2064, U+2066-U+206F (invisible operators)
- U+FEFF (byte order mark / zero-width no-break space)
- U+FFF9-U+FFFB (interlinear annotation anchors)

### Validation Order and Error Messages

The `validate` method calls sub-validators in this order:
1. `validate_confidence(output.confidence)` -- error: `"confidence {value} is out of range [{min}, {max}]"`
2. `validate_text_field("summary", &output.summary)` -- error: `"summary exceeds maximum length of {max} characters"`
3. `validate_text_field("reasoning", &output.reasoning)` -- same pattern
4. `validate_text_field("recommendation", &output.recommendation)` -- same pattern
5. `validate_findings(&output.findings)` -- error for count: `"findings count {count} exceeds maximum of {max}"`, for title length: `"finding title exceeds maximum length of {max} characters"`, for empty stripped title: `"finding title is empty after removing zero-width characters"`, for detail length: `"finding detail exceeds maximum length of {max} characters"`

The method returns on the first validation failure (fail-fast). Each error message includes the field name to aid diagnostics.

### `lib.rs` Module Declaration

Add `pub mod validate;` to `src/lib.rs`.

## Constraints Checklist

- No `panic!`, `unwrap()`, `expect()` outside `#[cfg(test)]`
- No `unsafe`
- All public types and methods have `///` Rustdoc
- Error messages include field name and actual/max values for diagnostics
- `Regex` is compiled once in the constructor, not per validation call
- `#[non_exhaustive]` on `ValidationLimits`
- `rustfmt` and `clippy --tests -- -D warnings` clean
- File header present on all new files

## Refactor Phase Notes

After Green phase passes all tests:

- Verify error messages are consistent in style (lowercase after field name, include actual and limit values)
- Consider whether `strip_zero_width` should be public (currently private -- only needed by `validate_finding`)
- Add Rustdoc `///` on the struct, constructor, and validate method
- Confirm `cargo doc --no-deps` generates clean documentation
- Verify that the regex pattern is comprehensive for Unicode Cf category
