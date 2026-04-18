// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05

use std::sync::LazyLock;

use regex::Regex;

use crate::error::MagiError;
use crate::schema::{AgentOutput, Finding, ZERO_WIDTH_PATTERN};

/// Matches control whitespace characters that should be replaced with a space:
/// horizontal tab, newline, vertical tab, form feed, carriage return, and NEL (U+0085).
static CONTROL_WHITESPACE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[\t\n\x0B\x0C\r\x{85}]").expect("valid CONTROL_WHITESPACE_RE regex")
});

/// Matches invisible characters and Unicode separators that should be removed:
/// zero-width spaces, bidi marks, line/paragraph separators (U+2028..U+202F range),
/// extended formatting controls (U+2060..U+206F), BOM (U+FEFF), and soft hyphen (U+00AD).
static INVISIBLE_AND_SEPARATOR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[\u{200b}-\u{200f}\u{2028}-\u{202f}\u{2060}-\u{206f}\u{feff}\u{00ad}]")
        .expect("valid INVISIBLE_AND_SEPARATOR_RE regex")
});

/// Cleans a title by normalizing control whitespace, stripping invisible
/// characters and separators, and trimming edges.
///
/// # Pipeline
///
/// 1. Replace control whitespace (`\t`, `\n`, `\x0B`, `\x0C`, `\r`, U+0085) with ASCII space.
/// 2. Remove invisible characters and selected Unicode separators
///    (zero-width, bidi marks, line/paragraph separators in the U+2028..U+202F range,
///    word joiner and related U+2060..U+206F controls, BOM, soft hyphen).
/// 3. Trim leading/trailing whitespace.
///
/// Note: interior whitespace is NOT collapsed — an input `"foo\t\tbar"` becomes
/// `"foo  bar"` (two spaces). This matches the Python reference implementation.
///
/// # Examples
///
/// ```
/// use magi_core::validate::clean_title;
///
/// assert_eq!(clean_title("  hello\nworld  "), "hello world");
/// assert_eq!(clean_title("text\u{200b}with\u{feff}invisibles"), "textwithinvisibles");
/// ```
pub fn clean_title(input: &str) -> String {
    let step1 = CONTROL_WHITESPACE_RE.replace_all(input, " ");
    let step2 = INVISIBLE_AND_SEPARATOR_RE.replace_all(&step1, "");
    step2.trim().to_string()
}

/// Configuration thresholds for agent output validation.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct ValidationLimits {
    /// Maximum number of findings per agent output.
    pub max_findings: usize,
    /// Maximum character count for finding titles (Unicode scalar values, not bytes).
    pub max_title_len: usize,
    /// Maximum character count for finding details (Unicode scalar values, not bytes).
    pub max_detail_len: usize,
    /// Maximum character count for text fields — summary, reasoning, recommendation
    /// (Unicode scalar values, not bytes).
    pub max_text_len: usize,
    /// Minimum valid confidence value, inclusive.
    pub confidence_min: f64,
    /// Maximum valid confidence value, inclusive.
    pub confidence_max: f64,
}

impl Default for ValidationLimits {
    fn default() -> Self {
        Self {
            max_findings: 100,
            max_title_len: 500,
            max_detail_len: 10_000,
            max_text_len: 50_000,
            confidence_min: 0.0,
            confidence_max: 1.0,
        }
    }
}

/// Validates `AgentOutput` fields against configurable limits.
///
/// Uses [`ZERO_WIDTH_PATTERN`] from `schema` for stripping zero-width Unicode
/// characters, and configurable limits for field lengths and counts.
pub struct Validator {
    /// Active validation limits.
    pub limits: ValidationLimits,
}

impl Default for Validator {
    fn default() -> Self {
        Self::new()
    }
}

impl Validator {
    /// Creates a validator with default limits.
    pub fn new() -> Self {
        Self::with_limits(ValidationLimits::default())
    }

    /// Creates a validator with custom limits.
    pub fn with_limits(limits: ValidationLimits) -> Self {
        Self { limits }
    }

    /// Validates an `AgentOutput`, returning on first failure.
    ///
    /// Checks in order: confidence, summary, reasoning, recommendation, findings.
    /// Returns `MagiError::Validation` with a descriptive message on failure.
    pub fn validate(&self, output: &AgentOutput) -> Result<(), MagiError> {
        self.validate_confidence(output.confidence)?;
        self.validate_text_field("summary", &output.summary)?;
        self.validate_text_field("reasoning", &output.reasoning)?;
        self.validate_text_field("recommendation", &output.recommendation)?;
        self.validate_findings(&output.findings)?;
        Ok(())
    }

    /// Validates `output` in place, replacing each finding's title with its
    /// cleaned form (see [`clean_title`]) before length validation.
    ///
    /// This is the preferred entry point for pipelines that parse LLM responses,
    /// because it ensures downstream code sees titles in the canonical cleaned
    /// form used by the consensus engine.
    ///
    /// # Errors
    ///
    /// Returns [`MagiError::Validation`] on the first field that fails validation.
    /// Validation order: confidence → summary → reasoning → recommendation →
    /// findings (count, then each title/detail after cleaning).
    pub fn validate_mut(&self, output: &mut AgentOutput) -> Result<(), MagiError> {
        self.validate_confidence(output.confidence)?;
        self.validate_text_field("summary", &output.summary)?;
        self.validate_text_field("reasoning", &output.reasoning)?;
        self.validate_text_field("recommendation", &output.recommendation)?;
        if output.findings.len() > self.limits.max_findings {
            return Err(MagiError::Validation(format!(
                "findings count {} exceeds maximum of {}",
                output.findings.len(),
                self.limits.max_findings
            )));
        }
        for finding in &mut output.findings {
            finding.title = clean_title(&finding.title);
            self.validate_finding_cleaned(finding)?;
        }
        Ok(())
    }

    fn validate_confidence(&self, confidence: f64) -> Result<(), MagiError> {
        if !(confidence >= self.limits.confidence_min && confidence <= self.limits.confidence_max) {
            return Err(MagiError::Validation(format!(
                "confidence {} is out of range [{}, {}]",
                confidence, self.limits.confidence_min, self.limits.confidence_max
            )));
        }
        Ok(())
    }

    fn validate_text_field(&self, field_name: &str, value: &str) -> Result<(), MagiError> {
        if value.chars().count() > self.limits.max_text_len {
            return Err(MagiError::Validation(format!(
                "{field_name} exceeds maximum length of {} characters",
                self.limits.max_text_len
            )));
        }
        Ok(())
    }

    fn validate_findings(&self, findings: &[Finding]) -> Result<(), MagiError> {
        if findings.len() > self.limits.max_findings {
            return Err(MagiError::Validation(format!(
                "findings count {} exceeds maximum of {}",
                findings.len(),
                self.limits.max_findings
            )));
        }
        for finding in findings {
            self.validate_finding(finding)?;
        }
        Ok(())
    }

    /// Validates a finding whose title has already been cleaned (no stripping needed).
    fn validate_finding_cleaned(&self, finding: &Finding) -> Result<(), MagiError> {
        if finding.title.is_empty() {
            return Err(MagiError::Validation(
                "finding title is empty after removing zero-width characters".to_string(),
            ));
        }
        if finding.title.chars().count() > self.limits.max_title_len {
            return Err(MagiError::Validation(format!(
                "finding title exceeds maximum length of {} characters",
                self.limits.max_title_len
            )));
        }
        if finding.detail.chars().count() > self.limits.max_detail_len {
            return Err(MagiError::Validation(format!(
                "finding detail exceeds maximum length of {} characters",
                self.limits.max_detail_len
            )));
        }
        Ok(())
    }

    fn validate_finding(&self, finding: &Finding) -> Result<(), MagiError> {
        let stripped = self.strip_zero_width(&finding.title);
        if stripped.is_empty() {
            return Err(MagiError::Validation(
                "finding title is empty after removing zero-width characters".to_string(),
            ));
        }
        if stripped.chars().count() > self.limits.max_title_len {
            return Err(MagiError::Validation(format!(
                "finding title exceeds maximum length of {} characters",
                self.limits.max_title_len
            )));
        }
        if finding.detail.chars().count() > self.limits.max_detail_len {
            return Err(MagiError::Validation(format!(
                "finding detail exceeds maximum length of {} characters",
                self.limits.max_detail_len
            )));
        }
        Ok(())
    }

    fn strip_zero_width(&self, text: &str) -> String {
        ZERO_WIDTH_PATTERN.replace_all(text, "").into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::*;

    fn valid_agent_output() -> AgentOutput {
        AgentOutput {
            agent: AgentName::Melchior,
            verdict: Verdict::Approve,
            confidence: 0.9,
            summary: "Good code".to_string(),
            reasoning: "Well structured".to_string(),
            findings: vec![],
            recommendation: "Approve as-is".to_string(),
        }
    }

    fn output_with_confidence(confidence: f64) -> AgentOutput {
        AgentOutput {
            confidence,
            ..valid_agent_output()
        }
    }

    fn output_with_findings(findings: Vec<Finding>) -> AgentOutput {
        AgentOutput {
            findings,
            ..valid_agent_output()
        }
    }

    // -- Constructor tests --

    #[test]
    fn test_validator_new_creates_with_default_limits() {
        let v = Validator::new();
        assert_eq!(v.limits.max_findings, 100);
        assert_eq!(v.limits.max_title_len, 500);
        assert_eq!(v.limits.max_detail_len, 10_000);
        assert_eq!(v.limits.max_text_len, 50_000);
        assert!((v.limits.confidence_min - 0.0).abs() < f64::EPSILON);
        assert!((v.limits.confidence_max - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_validator_with_limits_uses_custom_limits() {
        let custom = ValidationLimits {
            max_findings: 5,
            ..ValidationLimits::default()
        };
        let v = Validator::with_limits(custom);
        assert_eq!(v.limits.max_findings, 5);
    }

    // -- BDD-10: confidence out of range --

    #[test]
    fn test_validate_rejects_confidence_above_one() {
        let v = Validator::new();
        let output = output_with_confidence(1.5);
        let err = v.validate(&output).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("confidence"),
            "error should mention confidence: {msg}"
        );
    }

    #[test]
    fn test_validate_rejects_confidence_below_zero() {
        let v = Validator::new();
        let output = output_with_confidence(-0.1);
        let err = v.validate(&output).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("confidence"),
            "error should mention confidence: {msg}"
        );
    }

    #[test]
    fn test_validate_accepts_confidence_at_boundaries() {
        let v = Validator::new();
        assert!(v.validate(&output_with_confidence(0.0)).is_ok());
        assert!(v.validate(&output_with_confidence(1.0)).is_ok());
    }

    #[test]
    fn test_validate_rejects_nan_confidence() {
        let v = Validator::new();
        let output = output_with_confidence(f64::NAN);
        assert!(v.validate(&output).is_err());
    }

    #[test]
    fn test_validate_rejects_infinity_confidence() {
        let v = Validator::new();
        assert!(v.validate(&output_with_confidence(f64::INFINITY)).is_err());
        assert!(
            v.validate(&output_with_confidence(f64::NEG_INFINITY))
                .is_err()
        );
    }

    // -- BDD-11: empty title after strip zero-width --

    #[test]
    fn test_validate_rejects_finding_with_only_zero_width_title() {
        let v = Validator::new();
        let output = output_with_findings(vec![Finding {
            severity: Severity::Warning,
            title: "\u{200B}\u{FEFF}\u{200C}".to_string(),
            detail: "detail".to_string(),
        }]);
        let err = v.validate(&output).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("title"), "error should mention title: {msg}");
    }

    #[test]
    fn test_validate_accepts_finding_with_normal_title() {
        let v = Validator::new();
        let output = output_with_findings(vec![Finding {
            severity: Severity::Info,
            title: "Security vulnerability".to_string(),
            detail: "detail".to_string(),
        }]);
        assert!(v.validate(&output).is_ok());
    }

    // -- BDD-12: text field exceeds max_text_len --

    #[test]
    fn test_validate_rejects_reasoning_exceeding_max_text_len() {
        let v = Validator::new();
        let mut output = valid_agent_output();
        output.reasoning = "x".repeat(50_001);
        let err = v.validate(&output).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("reasoning"),
            "error should mention reasoning: {msg}"
        );
    }

    #[test]
    fn test_validate_rejects_summary_exceeding_max_text_len() {
        let v = Validator::new();
        let mut output = valid_agent_output();
        output.summary = "x".repeat(50_001);
        let err = v.validate(&output).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("summary"),
            "error should mention summary: {msg}"
        );
    }

    #[test]
    fn test_validate_rejects_recommendation_exceeding_max_text_len() {
        let v = Validator::new();
        let mut output = valid_agent_output();
        output.recommendation = "x".repeat(50_001);
        let err = v.validate(&output).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("recommendation"),
            "error should mention recommendation: {msg}"
        );
    }

    // -- Findings count and field limits --

    #[test]
    fn test_validate_rejects_findings_count_exceeding_max_findings() {
        let v = Validator::new();
        let findings: Vec<Finding> = (0..101)
            .map(|i| Finding {
                severity: Severity::Info,
                title: format!("Finding {i}"),
                detail: "detail".to_string(),
            })
            .collect();
        let output = output_with_findings(findings);
        let err = v.validate(&output).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("findings"),
            "error should mention findings: {msg}"
        );
    }

    #[test]
    fn test_validate_rejects_finding_title_exceeding_max_title_len() {
        let v = Validator::new();
        let output = output_with_findings(vec![Finding {
            severity: Severity::Warning,
            title: "x".repeat(501),
            detail: "detail".to_string(),
        }]);
        let err = v.validate(&output).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("title"), "error should mention title: {msg}");
    }

    #[test]
    fn test_validate_rejects_finding_detail_exceeding_max_detail_len() {
        let v = Validator::new();
        let output = output_with_findings(vec![Finding {
            severity: Severity::Info,
            title: "Valid title".to_string(),
            detail: "x".repeat(10_001),
        }]);
        let err = v.validate(&output).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("detail"), "error should mention detail: {msg}");
    }

    // -- Happy path --

    #[test]
    fn test_validate_accepts_valid_agent_output() {
        let v = Validator::new();
        assert!(v.validate(&valid_agent_output()).is_ok());
    }

    // -- strip_zero_width --

    #[test]
    fn test_strip_zero_width_removes_cf_category_characters() {
        let v = Validator::new();
        let input = "Hello\u{200B}World\u{FEFF}Test\u{200C}End";
        let result = v.strip_zero_width(input);
        assert_eq!(result, "HelloWorldTestEnd");
    }

    // -- Validation order --

    #[test]
    fn test_validation_order_confidence_checked_before_text_fields() {
        let v = Validator::new();
        let mut output = valid_agent_output();
        output.confidence = 2.0;
        output.summary = "x".repeat(50_001);
        let err = v.validate(&output).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("confidence"),
            "confidence should be checked first, got: {msg}"
        );
    }

    #[test]
    fn test_validation_order_summary_checked_before_reasoning() {
        let v = Validator::new();
        let mut output = valid_agent_output();
        output.summary = "x".repeat(50_001);
        output.reasoning = "x".repeat(50_001);
        let err = v.validate(&output).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("summary"),
            "summary should be checked before reasoning, got: {msg}"
        );
    }

    #[test]
    fn test_validation_order_recommendation_checked_before_findings() {
        let v = Validator::new();
        let mut output = valid_agent_output();
        output.recommendation = "x".repeat(50_001);
        output.findings = (0..101)
            .map(|i| Finding {
                severity: Severity::Info,
                title: format!("Finding {i}"),
                detail: "detail".to_string(),
            })
            .collect();
        let err = v.validate(&output).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("recommendation"),
            "recommendation should be checked before findings, got: {msg}"
        );
    }

    // -- Title length checked after strip --

    #[test]
    fn test_title_length_checked_after_strip_zero_width() {
        let limits = ValidationLimits {
            max_title_len: 5,
            ..ValidationLimits::default()
        };
        let v = Validator::with_limits(limits);
        // Title is 8 chars raw but 5 after stripping 3 zero-width chars => should pass
        let output = output_with_findings(vec![Finding {
            severity: Severity::Info,
            title: "He\u{200B}l\u{FEFF}lo\u{200C}".to_string(),
            detail: "detail".to_string(),
        }]);
        assert!(v.validate(&output).is_ok());
    }

    // -- validate_mut tests --

    fn finding_with_title(title: &str) -> Finding {
        Finding {
            severity: Severity::Info,
            title: title.to_string(),
            detail: "some detail".to_string(),
        }
    }

    #[test]
    fn test_validate_mut_replaces_title_with_cleaned_form() {
        let v = Validator::new();
        let mut output = output_with_findings(vec![finding_with_title("  Issue\t\u{200b}Title  ")]);
        v.validate_mut(&mut output).unwrap();
        assert_eq!(output.findings[0].title, "Issue Title");
    }

    #[test]
    fn test_validate_mut_strips_zero_width_from_titles() {
        let v = Validator::new();
        let mut output = output_with_findings(vec![finding_with_title("Good\u{200b}Title")]);
        v.validate_mut(&mut output).unwrap();
        assert_eq!(output.findings[0].title, "GoodTitle");
    }

    #[test]
    fn test_validate_mut_collapses_control_whitespace_in_titles() {
        let v = Validator::new();
        let mut output = output_with_findings(vec![finding_with_title("Bad\tTitle")]);
        v.validate_mut(&mut output).unwrap();
        assert_eq!(output.findings[0].title, "Bad Title");
    }

    #[test]
    fn test_validate_mut_preserves_order_of_findings() {
        let v = Validator::new();
        let titles = ["Alpha\u{200b}One", "Beta\tTwo", "  Gamma Three  "];
        let mut output =
            output_with_findings(titles.iter().map(|t| finding_with_title(t)).collect());
        v.validate_mut(&mut output).unwrap();
        assert_eq!(output.findings[0].title, "AlphaOne");
        assert_eq!(output.findings[1].title, "Beta Two");
        assert_eq!(output.findings[2].title, "Gamma Three");
    }

    #[test]
    fn test_validate_retains_original_behavior_on_immutable_slice() {
        let v = Validator::new();
        let output = output_with_findings(vec![finding_with_title("Normal Title")]);
        // Immutable validate does not mutate.
        let original_title = output.findings[0].title.clone();
        v.validate(&output).unwrap();
        assert_eq!(output.findings[0].title, original_title);
    }

    // -- clean_title tests --

    #[test]
    fn test_clean_title_replaces_tab_with_space() {
        assert_eq!(clean_title("foo\tbar"), "foo bar");
    }

    #[test]
    fn test_clean_title_replaces_newline_with_space() {
        assert_eq!(clean_title("foo\nbar"), "foo bar");
    }

    #[test]
    fn test_clean_title_replaces_vertical_tab_with_space() {
        assert_eq!(clean_title("foo\x0Bbar"), "foo bar");
    }

    #[test]
    fn test_clean_title_replaces_carriage_return_with_space() {
        assert_eq!(clean_title("foo\rbar"), "foo bar");
    }

    #[test]
    fn test_clean_title_replaces_nel_u0085_with_space() {
        assert_eq!(clean_title("foo\u{85}bar"), "foo bar");
    }

    #[test]
    fn test_clean_title_strips_zero_width_space_u200b() {
        assert_eq!(clean_title("a\u{200b}b"), "ab");
    }

    #[test]
    fn test_clean_title_strips_zwnj_u200c() {
        assert_eq!(clean_title("a\u{200c}b"), "ab");
    }

    #[test]
    fn test_clean_title_strips_zwj_u200d() {
        assert_eq!(clean_title("a\u{200d}b"), "ab");
    }

    #[test]
    fn test_clean_title_strips_lrm_rlm_u200e_u200f() {
        assert_eq!(clean_title("a\u{200e}b\u{200f}c"), "abc");
    }

    #[test]
    fn test_clean_title_strips_line_separator_u2028() {
        assert_eq!(clean_title("a\u{2028}b"), "ab");
    }

    #[test]
    fn test_clean_title_strips_paragraph_separator_u2029() {
        assert_eq!(clean_title("a\u{2029}b"), "ab");
    }

    #[test]
    fn test_clean_title_strips_narrow_nbsp_u202f() {
        assert_eq!(clean_title("a\u{202f}b"), "ab");
    }

    #[test]
    fn test_clean_title_strips_bidi_override_u202a_through_u202e() {
        for cp in ['\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}', '\u{202e}'] {
            let input = format!("a{cp}b");
            assert_eq!(clean_title(&input), "ab", "failed for U+{:04X}", cp as u32);
        }
    }

    #[test]
    fn test_clean_title_strips_word_joiner_u2060() {
        assert_eq!(clean_title("a\u{2060}b"), "ab");
    }

    #[test]
    fn test_clean_title_strips_bom_ufeff() {
        assert_eq!(clean_title("\u{feff}hello"), "hello");
    }

    #[test]
    fn test_clean_title_strips_soft_hyphen_u00ad() {
        assert_eq!(clean_title("soft\u{00ad}hyphen"), "softhyphen");
    }

    #[test]
    fn test_clean_title_trims_leading_trailing_spaces() {
        assert_eq!(clean_title("  hello  "), "hello");
    }

    #[test]
    fn test_clean_title_trims_leading_trailing_tabs_after_replacement() {
        // Leading/trailing \t are replaced to spaces in step 1, then trimmed in step 3
        assert_eq!(clean_title("\thello\t"), "hello");
    }

    #[test]
    fn test_clean_title_preserves_interior_single_spaces() {
        assert_eq!(clean_title("hello world"), "hello world");
    }

    #[test]
    fn test_clean_title_does_not_collapse_double_spaces_interior() {
        // Interior whitespace is NOT collapsed — this is intentional Python parity
        assert_eq!(clean_title("foo  bar"), "foo  bar");
    }

    #[test]
    fn test_clean_title_preserves_unicode_letters() {
        assert_eq!(clean_title("café"), "café");
    }

    #[test]
    fn test_clean_title_empty_string_returns_empty() {
        assert_eq!(clean_title(""), "");
    }

    #[test]
    fn test_clean_title_all_whitespace_returns_empty() {
        assert_eq!(clean_title("   \t\n  "), "");
    }

    #[test]
    fn test_clean_title_is_idempotent() {
        let inputs = [
            "hello\nworld",
            "  \u{200b}spaces\u{feff}  ",
            "café\u{2060}",
            "normal text",
            "",
        ];
        for input in inputs {
            let once = clean_title(input);
            let twice = clean_title(&once);
            assert_eq!(once, twice, "not idempotent for input: {input:?}");
        }
    }
}
