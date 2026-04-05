// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05

use crate::error::MagiError;
use crate::schema::{AgentOutput, Finding, ZERO_WIDTH_PATTERN};

/// Configuration thresholds for agent output validation.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct ValidationLimits {
    /// Maximum number of findings per agent output.
    pub max_findings: usize,
    /// Maximum character length for finding titles.
    pub max_title_len: usize,
    /// Maximum character length for finding details.
    pub max_detail_len: usize,
    /// Maximum character length for text fields (summary, reasoning, recommendation).
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
        if value.len() > self.limits.max_text_len {
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

    fn validate_finding(&self, finding: &Finding) -> Result<(), MagiError> {
        let stripped = self.strip_zero_width(&finding.title);
        if stripped.is_empty() {
            return Err(MagiError::Validation(
                "finding title is empty after removing zero-width characters".to_string(),
            ));
        }
        if stripped.len() > self.limits.max_title_len {
            return Err(MagiError::Validation(format!(
                "finding title exceeds maximum length of {} characters",
                self.limits.max_title_len
            )));
        }
        if finding.detail.len() > self.limits.max_detail_len {
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
}
