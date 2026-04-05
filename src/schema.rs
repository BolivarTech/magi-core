// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;

/// An agent's judgment on the analyzed content.
///
/// Serializes as lowercase (`"approve"`, `"reject"`, `"conditional"`).
/// Display outputs uppercase (`"APPROVE"`, `"REJECT"`, `"CONDITIONAL"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    /// The agent approves the content.
    Approve,
    /// The agent rejects the content.
    Reject,
    /// The agent approves with conditions; counts as approval for majority.
    Conditional,
}

impl Verdict {
    /// Returns the numeric weight for consensus score computation.
    ///
    /// - `Approve` => `+1.0`
    /// - `Reject` => `-1.0`
    /// - `Conditional` => `+0.5`
    pub fn weight(&self) -> f64 {
        match self {
            Verdict::Approve => 1.0,
            Verdict::Reject => -1.0,
            Verdict::Conditional => 0.5,
        }
    }

    /// Maps the verdict to its effective binary form for majority counting.
    ///
    /// `Conditional` maps to `Approve`; others are identity.
    pub fn effective(&self) -> Verdict {
        match self {
            Verdict::Approve | Verdict::Conditional => Verdict::Approve,
            Verdict::Reject => Verdict::Reject,
        }
    }
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Verdict::Approve => write!(f, "APPROVE"),
            Verdict::Reject => write!(f, "REJECT"),
            Verdict::Conditional => write!(f, "CONDITIONAL"),
        }
    }
}

/// Severity level of a finding reported by an agent.
///
/// Ordering: `Critical > Warning > Info`.
/// Serializes as lowercase (`"critical"`, `"warning"`, `"info"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Highest severity — blocks approval.
    Critical,
    /// Medium severity — warrants attention.
    Warning,
    /// Low severity — informational only.
    Info,
}

impl Severity {
    /// Returns a short icon string for report formatting.
    ///
    /// - `Critical` => `"[!!!]"`
    /// - `Warning` => `"[!!]"`
    /// - `Info` => `"[i]"`
    pub fn icon(&self) -> &'static str {
        match self {
            Severity::Critical => "[!!!]",
            Severity::Warning => "[!!]",
            Severity::Info => "[i]",
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Critical => write!(f, "CRITICAL"),
            Severity::Warning => write!(f, "WARNING"),
            Severity::Info => write!(f, "INFO"),
        }
    }
}

impl PartialOrd for Severity {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Severity {
    fn cmp(&self, other: &Self) -> Ordering {
        fn rank(s: &Severity) -> u8 {
            match s {
                Severity::Info => 0,
                Severity::Warning => 1,
                Severity::Critical => 2,
            }
        }
        rank(self).cmp(&rank(other))
    }
}

/// Analysis mode that determines agent perspectives.
///
/// Serializes as kebab-case (`"code-review"`, `"design"`, `"analysis"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    /// Source code review perspective.
    CodeReview,
    /// Architecture and design perspective.
    Design,
    /// General analysis perspective.
    Analysis,
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Mode::CodeReview => write!(f, "code-review"),
            Mode::Design => write!(f, "design"),
            Mode::Analysis => write!(f, "analysis"),
        }
    }
}

/// Identifies one of the three MAGI agents.
///
/// Ordering is alphabetical (`Balthasar < Caspar < Melchior`) for
/// deterministic tiebreaking in consensus.
/// Serializes as lowercase (`"melchior"`, `"balthasar"`, `"caspar"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentName {
    /// The Scientist — innovative, research-oriented perspective.
    Melchior,
    /// The Pragmatist — practical, engineering-oriented perspective.
    Balthasar,
    /// The Critic — skeptical, risk-oriented perspective.
    Caspar,
}

impl AgentName {
    /// Returns the agent's analytical role title.
    ///
    /// - `Melchior` => `"Scientist"`
    /// - `Balthasar` => `"Pragmatist"`
    /// - `Caspar` => `"Critic"`
    pub fn title(&self) -> &'static str {
        match self {
            AgentName::Melchior => "Scientist",
            AgentName::Balthasar => "Pragmatist",
            AgentName::Caspar => "Critic",
        }
    }

    /// Returns the agent's proper name as a string.
    pub fn display_name(&self) -> &'static str {
        match self {
            AgentName::Melchior => "Melchior",
            AgentName::Balthasar => "Balthasar",
            AgentName::Caspar => "Caspar",
        }
    }
}

impl PartialOrd for AgentName {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for AgentName {
    fn cmp(&self, other: &Self) -> Ordering {
        self.display_name().cmp(other.display_name())
    }
}

/// A single finding reported by an agent during analysis.
///
/// Findings have a severity, title, and detail explanation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Finding {
    /// Severity level of this finding.
    pub severity: Severity,
    /// Short title describing the finding.
    pub title: String,
    /// Detailed explanation of the finding.
    pub detail: String,
}

impl Finding {
    /// Returns the title with Unicode format characters (category Cf) removed.
    ///
    /// Strips zero-width spaces, byte order marks, and other invisible
    /// formatting characters that could interfere with deduplication.
    pub fn stripped_title(&self) -> String {
        let re = Regex::new(
            "[\u{00AD}\u{0600}-\u{0605}\u{061C}\u{06DD}\u{070F}\u{08E2}\u{180E}\
             \u{200B}-\u{200F}\u{202A}-\u{202E}\u{2060}-\u{2064}\u{2066}-\u{206F}\
             \u{FEFF}\u{FFF9}-\u{FFFB}]",
        )
        .expect("zero-width regex is valid");
        re.replace_all(&self.title, "").into_owned()
    }
}

/// Deserialized output from a single LLM agent.
///
/// Contains the agent's verdict, confidence, reasoning, and findings.
/// Unknown JSON fields are silently ignored during deserialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentOutput {
    /// Which agent produced this output.
    pub agent: AgentName,
    /// The agent's judgment verdict.
    pub verdict: Verdict,
    /// Confidence level between 0.0 and 1.0 (validated externally).
    pub confidence: f64,
    /// Brief summary of the agent's analysis.
    pub summary: String,
    /// Detailed reasoning behind the verdict.
    pub reasoning: String,
    /// Specific findings discovered during analysis.
    pub findings: Vec<Finding>,
    /// The agent's actionable recommendation.
    pub recommendation: String,
}

impl AgentOutput {
    /// Returns `true` if the verdict is `Approve` or `Conditional`.
    pub fn is_approving(&self) -> bool {
        matches!(self.verdict, Verdict::Approve | Verdict::Conditional)
    }

    /// Returns `true` if this agent's effective verdict differs from the majority.
    pub fn is_dissenting(&self, majority: Verdict) -> bool {
        self.effective_verdict() != majority
    }

    /// Returns the effective binary verdict (delegates to [`Verdict::effective`]).
    pub fn effective_verdict(&self) -> Verdict {
        self.verdict.effective()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    // -- Verdict tests --

    #[test]
    fn test_verdict_approve_weight_is_positive_one() {
        assert_eq!(Verdict::Approve.weight(), 1.0);
    }

    #[test]
    fn test_verdict_reject_weight_is_negative_one() {
        assert_eq!(Verdict::Reject.weight(), -1.0);
    }

    #[test]
    fn test_verdict_conditional_weight_is_half() {
        assert_eq!(Verdict::Conditional.weight(), 0.5);
    }

    #[test]
    fn test_verdict_conditional_effective_maps_to_approve() {
        assert_eq!(Verdict::Conditional.effective(), Verdict::Approve);
    }

    #[test]
    fn test_verdict_approve_effective_is_identity() {
        assert_eq!(Verdict::Approve.effective(), Verdict::Approve);
    }

    #[test]
    fn test_verdict_reject_effective_is_identity() {
        assert_eq!(Verdict::Reject.effective(), Verdict::Reject);
    }

    #[test]
    fn test_verdict_display_outputs_uppercase() {
        assert_eq!(format!("{}", Verdict::Approve), "APPROVE");
        assert_eq!(format!("{}", Verdict::Reject), "REJECT");
        assert_eq!(format!("{}", Verdict::Conditional), "CONDITIONAL");
    }

    #[test]
    fn test_verdict_serializes_as_lowercase() {
        assert_eq!(
            serde_json::to_string(&Verdict::Approve).unwrap(),
            "\"approve\""
        );
        assert_eq!(
            serde_json::to_string(&Verdict::Reject).unwrap(),
            "\"reject\""
        );
        assert_eq!(
            serde_json::to_string(&Verdict::Conditional).unwrap(),
            "\"conditional\""
        );
    }

    #[test]
    fn test_verdict_deserializes_from_lowercase() {
        assert_eq!(
            serde_json::from_str::<Verdict>("\"approve\"").unwrap(),
            Verdict::Approve
        );
        assert_eq!(
            serde_json::from_str::<Verdict>("\"reject\"").unwrap(),
            Verdict::Reject
        );
        assert_eq!(
            serde_json::from_str::<Verdict>("\"conditional\"").unwrap(),
            Verdict::Conditional
        );
    }

    #[test]
    fn test_verdict_deserialization_rejects_invalid() {
        assert!(serde_json::from_str::<Verdict>("\"invalid\"").is_err());
    }

    // -- Severity tests --

    #[test]
    fn test_severity_ordering_critical_greater_than_warning_greater_than_info() {
        assert!(Severity::Critical > Severity::Warning);
        assert!(Severity::Warning > Severity::Info);
        assert!(Severity::Critical > Severity::Info);
    }

    #[test]
    fn test_severity_icon_returns_correct_strings() {
        assert_eq!(Severity::Critical.icon(), "[!!!]");
        assert_eq!(Severity::Warning.icon(), "[!!]");
        assert_eq!(Severity::Info.icon(), "[i]");
    }

    #[test]
    fn test_severity_display_outputs_uppercase() {
        assert_eq!(format!("{}", Severity::Critical), "CRITICAL");
        assert_eq!(format!("{}", Severity::Warning), "WARNING");
        assert_eq!(format!("{}", Severity::Info), "INFO");
    }

    #[test]
    fn test_severity_serializes_as_lowercase() {
        assert_eq!(
            serde_json::to_string(&Severity::Critical).unwrap(),
            "\"critical\""
        );
        assert_eq!(
            serde_json::to_string(&Severity::Warning).unwrap(),
            "\"warning\""
        );
        assert_eq!(serde_json::to_string(&Severity::Info).unwrap(), "\"info\"");
    }

    #[test]
    fn test_severity_deserializes_from_lowercase() {
        assert_eq!(
            serde_json::from_str::<Severity>("\"critical\"").unwrap(),
            Severity::Critical
        );
    }

    #[test]
    fn test_severity_deserialization_rejects_invalid() {
        assert!(serde_json::from_str::<Severity>("\"invalid\"").is_err());
    }

    // -- Mode tests --

    #[test]
    fn test_mode_display_outputs_hyphenated_lowercase() {
        assert_eq!(format!("{}", Mode::CodeReview), "code-review");
        assert_eq!(format!("{}", Mode::Design), "design");
        assert_eq!(format!("{}", Mode::Analysis), "analysis");
    }

    #[test]
    fn test_mode_serializes_as_lowercase_with_hyphens() {
        assert_eq!(
            serde_json::to_string(&Mode::CodeReview).unwrap(),
            "\"code-review\""
        );
        assert_eq!(serde_json::to_string(&Mode::Design).unwrap(), "\"design\"");
        assert_eq!(
            serde_json::to_string(&Mode::Analysis).unwrap(),
            "\"analysis\""
        );
    }

    #[test]
    fn test_mode_deserializes_from_lowercase_with_hyphens() {
        assert_eq!(
            serde_json::from_str::<Mode>("\"code-review\"").unwrap(),
            Mode::CodeReview
        );
        assert_eq!(
            serde_json::from_str::<Mode>("\"design\"").unwrap(),
            Mode::Design
        );
        assert_eq!(
            serde_json::from_str::<Mode>("\"analysis\"").unwrap(),
            Mode::Analysis
        );
    }

    #[test]
    fn test_mode_deserialization_rejects_invalid() {
        assert!(serde_json::from_str::<Mode>("\"invalid\"").is_err());
    }

    // -- AgentName tests --

    #[test]
    fn test_agent_name_title_returns_role() {
        assert_eq!(AgentName::Melchior.title(), "Scientist");
        assert_eq!(AgentName::Balthasar.title(), "Pragmatist");
        assert_eq!(AgentName::Caspar.title(), "Critic");
    }

    #[test]
    fn test_agent_name_display_name_returns_name() {
        assert_eq!(AgentName::Melchior.display_name(), "Melchior");
        assert_eq!(AgentName::Balthasar.display_name(), "Balthasar");
        assert_eq!(AgentName::Caspar.display_name(), "Caspar");
    }

    #[test]
    fn test_agent_name_ord_is_alphabetical() {
        assert!(AgentName::Balthasar < AgentName::Caspar);
        assert!(AgentName::Caspar < AgentName::Melchior);
        assert!(AgentName::Balthasar < AgentName::Melchior);
    }

    #[test]
    fn test_agent_name_serializes_as_lowercase() {
        assert_eq!(
            serde_json::to_string(&AgentName::Melchior).unwrap(),
            "\"melchior\""
        );
        assert_eq!(
            serde_json::to_string(&AgentName::Balthasar).unwrap(),
            "\"balthasar\""
        );
        assert_eq!(
            serde_json::to_string(&AgentName::Caspar).unwrap(),
            "\"caspar\""
        );
    }

    #[test]
    fn test_agent_name_deserializes_from_lowercase() {
        assert_eq!(
            serde_json::from_str::<AgentName>("\"melchior\"").unwrap(),
            AgentName::Melchior
        );
    }

    #[test]
    fn test_agent_name_usable_as_btreemap_key() {
        let mut map = BTreeMap::new();
        map.insert(AgentName::Melchior, "scientist");
        map.insert(AgentName::Balthasar, "pragmatist");
        map.insert(AgentName::Caspar, "critic");
        assert_eq!(map.get(&AgentName::Melchior), Some(&"scientist"));
        assert_eq!(map.get(&AgentName::Balthasar), Some(&"pragmatist"));
        assert_eq!(map.get(&AgentName::Caspar), Some(&"critic"));
    }

    // -- Finding tests --

    #[test]
    fn test_finding_stripped_title_removes_zero_width_characters() {
        let finding = Finding {
            severity: Severity::Warning,
            title: "Hello\u{200B}World\u{FEFF}Test\u{200C}End".to_string(),
            detail: "detail".to_string(),
        };
        assert_eq!(finding.stripped_title(), "HelloWorldTestEnd");
    }

    #[test]
    fn test_finding_stripped_title_preserves_normal_text() {
        let finding = Finding {
            severity: Severity::Info,
            title: "Normal title".to_string(),
            detail: "detail".to_string(),
        };
        assert_eq!(finding.stripped_title(), "Normal title");
    }

    #[test]
    fn test_finding_serde_roundtrip() {
        let finding = Finding {
            severity: Severity::Critical,
            title: "Security issue".to_string(),
            detail: "SQL injection vulnerability".to_string(),
        };
        let json = serde_json::to_string(&finding).unwrap();
        let deserialized: Finding = serde_json::from_str(&json).unwrap();
        assert_eq!(finding, deserialized);
    }

    // -- AgentOutput tests --

    fn make_output(verdict: Verdict) -> AgentOutput {
        AgentOutput {
            agent: AgentName::Melchior,
            verdict,
            confidence: 0.9,
            summary: "summary".to_string(),
            reasoning: "reasoning".to_string(),
            findings: vec![],
            recommendation: "recommendation".to_string(),
        }
    }

    #[test]
    fn test_agent_output_is_approving_true_for_approve() {
        assert!(make_output(Verdict::Approve).is_approving());
    }

    #[test]
    fn test_agent_output_is_approving_true_for_conditional() {
        assert!(make_output(Verdict::Conditional).is_approving());
    }

    #[test]
    fn test_agent_output_is_approving_false_for_reject() {
        assert!(!make_output(Verdict::Reject).is_approving());
    }

    #[test]
    fn test_agent_output_is_dissenting_when_verdict_differs_from_majority() {
        let output = make_output(Verdict::Reject);
        assert!(output.is_dissenting(Verdict::Approve));
    }

    #[test]
    fn test_agent_output_is_not_dissenting_when_verdict_matches_majority() {
        let output = make_output(Verdict::Approve);
        assert!(!output.is_dissenting(Verdict::Approve));
    }

    #[test]
    fn test_agent_output_conditional_is_not_dissenting_from_approve_majority() {
        let output = make_output(Verdict::Conditional);
        assert!(!output.is_dissenting(Verdict::Approve));
    }

    #[test]
    fn test_agent_output_effective_verdict_maps_conditional_to_approve() {
        let output = make_output(Verdict::Conditional);
        assert_eq!(output.effective_verdict(), Verdict::Approve);
    }

    #[test]
    fn test_agent_output_serde_roundtrip() {
        let output = AgentOutput {
            agent: AgentName::Balthasar,
            verdict: Verdict::Conditional,
            confidence: 0.75,
            summary: "looks okay".to_string(),
            reasoning: "mostly good".to_string(),
            findings: vec![Finding {
                severity: Severity::Warning,
                title: "Minor issue".to_string(),
                detail: "Could improve naming".to_string(),
            }],
            recommendation: "approve with changes".to_string(),
        };
        let json = serde_json::to_string(&output).unwrap();
        let deserialized: AgentOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(output, deserialized);
    }

    #[test]
    fn test_agent_output_empty_findings_valid() {
        let output = make_output(Verdict::Approve);
        assert!(output.findings.is_empty());
        let json = serde_json::to_string(&output).unwrap();
        let deserialized: AgentOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(output, deserialized);
    }

    #[test]
    fn test_agent_output_ignores_unknown_fields() {
        let json = r#"{
            "agent": "caspar",
            "verdict": "reject",
            "confidence": 0.3,
            "summary": "bad",
            "reasoning": "terrible",
            "findings": [],
            "recommendation": "reject",
            "unknown_field": "should be ignored"
        }"#;
        let output: AgentOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.agent, AgentName::Caspar);
        assert_eq!(output.verdict, Verdict::Reject);
    }
}
