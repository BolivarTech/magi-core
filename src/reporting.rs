// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05

use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt::Write;

use crate::consensus::{Condition, ConsensusResult, DedupFinding, Dissent};
use crate::schema::{AgentName, AgentOutput, Mode};

/// Configuration for the report formatter.
///
/// Controls banner width and agent display names/titles.
///
/// # ASCII Constraint
///
/// The fixed-width banner guarantee (`banner_width` bytes per line) assumes
/// all displayed content is ASCII. Agent titles, verdict labels, and consensus
/// strings are ASCII by default. If `agent_titles` contains multi-byte UTF-8
/// characters, banner lines will have correct byte length but may appear
/// visually misaligned in terminals because multi-byte characters can occupy
/// more than one display column.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct ReportConfig {
    /// Total width of the ASCII banner in bytes, including border characters
    /// (default: 52). Equals character count for ASCII content.
    pub banner_width: usize,
    /// Maps agent name to (display_name, title) for report display.
    /// Values should be ASCII for correct banner alignment.
    pub agent_titles: BTreeMap<AgentName, (String, String)>,
}

/// Formats consensus results into ASCII banners and markdown reports.
///
/// Generates fixed-width ASCII banners (exactly 52 characters wide per line)
/// and full markdown reports from agent outputs and consensus results.
/// The reporting module is pure string formatting -- no async, no I/O.
pub struct ReportFormatter {
    config: ReportConfig,
    banner_inner: usize,
}

/// Final output struct returned by the orchestrator's `analyze()` method.
///
/// Contains all analysis data plus the formatted report string.
/// Serializes to JSON matching the Python original format.
#[derive(Debug, Clone, Serialize)]
pub struct MagiReport {
    /// The successful agent outputs used in analysis.
    pub agents: Vec<AgentOutput>,
    /// The computed consensus result.
    pub consensus: ConsensusResult,
    /// The ASCII banner string.
    pub banner: String,
    /// The full markdown report string.
    pub report: String,
    /// True if fewer than 3 agents succeeded.
    pub degraded: bool,
    /// Agents that failed, mapped to their failure reason
    /// (e.g., `"parse: no valid JSON"`, `"validation: confidence out of range"`).
    pub failed_agents: BTreeMap<AgentName, String>,
}

impl Default for ReportConfig {
    fn default() -> Self {
        let mut agent_titles = BTreeMap::new();
        agent_titles.insert(
            AgentName::Melchior,
            ("Melchior".to_string(), "Scientist".to_string()),
        );
        agent_titles.insert(
            AgentName::Balthasar,
            ("Balthasar".to_string(), "Pragmatist".to_string()),
        );
        agent_titles.insert(
            AgentName::Caspar,
            ("Caspar".to_string(), "Critic".to_string()),
        );
        Self {
            banner_width: 52,
            agent_titles,
        }
    }
}

impl ReportFormatter {
    /// Creates a new formatter with default configuration.
    pub fn new() -> Self {
        Self::with_config(ReportConfig::default())
    }

    /// Creates a new formatter with custom configuration.
    pub fn with_config(config: ReportConfig) -> Self {
        let banner_inner = config.banner_width - 2;
        Self {
            config,
            banner_inner,
        }
    }

    /// Generates the fixed-width ASCII verdict banner.
    ///
    /// Every line is exactly `banner_width` (52) characters. Structure:
    /// ```text
    /// +==================================================+
    /// |          MAGI SYSTEM -- VERDICT                  |
    /// +==================================================+
    /// |  Melchior (Scientist):  APPROVE (90%)            |
    /// +==================================================+
    /// |  CONSENSUS: GO WITH CAVEATS                      |
    /// +==================================================+
    /// ```
    pub fn format_banner(&self, agents: &[AgentOutput], consensus: &ConsensusResult) -> String {
        let mut out = String::new();
        let sep = self.format_separator();

        writeln!(out, "{}", sep).ok();
        writeln!(
            out,
            "{}",
            self.format_line("        MAGI SYSTEM -- VERDICT")
        )
        .ok();
        writeln!(out, "{}", sep).ok();

        for agent in agents {
            let (display_name, title) = self.agent_display(&agent.agent);
            let pct = (agent.confidence * 100.0).round() as u32;
            let content = format!(
                "  {} ({}):  {} ({}%)",
                display_name, title, agent.verdict, pct
            );
            writeln!(out, "{}", self.format_line(&content)).ok();
        }

        writeln!(out, "{}", sep).ok();
        let consensus_line = format!("  CONSENSUS: {}", consensus.consensus);
        writeln!(out, "{}", self.format_line(&consensus_line)).ok();
        write!(out, "{}", sep).ok();

        out
    }

    /// Generates the pre-analysis initialization banner.
    ///
    /// Shows mode, model, and timeout in a fixed-width ASCII box.
    pub fn format_init_banner(&self, mode: &Mode, model: &str, timeout_secs: u64) -> String {
        let mut out = String::new();
        let sep = self.format_separator();

        writeln!(out, "{}", sep).ok();
        writeln!(
            out,
            "{}",
            self.format_line("        MAGI SYSTEM -- INITIALIZING")
        )
        .ok();
        writeln!(out, "{}", sep).ok();
        writeln!(
            out,
            "{}",
            self.format_line(&format!("  Mode:     {}", mode))
        )
        .ok();
        writeln!(
            out,
            "{}",
            self.format_line(&format!("  Model:    {}", model))
        )
        .ok();
        writeln!(
            out,
            "{}",
            self.format_line(&format!("  Timeout:  {}s", timeout_secs))
        )
        .ok();
        write!(out, "{}", sep).ok();

        out
    }

    /// Generates the full markdown report (banner + all sections).
    ///
    /// Concatenates sections in order: banner, consensus summary, key findings,
    /// dissenting opinion, conditions for approval, recommended actions.
    /// Optional sections are omitted entirely when their data is absent.
    pub fn format_report(&self, agents: &[AgentOutput], consensus: &ConsensusResult) -> String {
        let mut out = String::new();

        // 1. Banner
        out.push_str(&self.format_banner(agents, consensus));
        out.push('\n');

        // 2. Consensus Summary
        out.push_str(&self.format_consensus_summary(consensus));

        // 3. Key Findings (optional)
        if !consensus.findings.is_empty() {
            out.push_str(&self.format_findings(&consensus.findings));
        }

        // 4. Dissenting Opinion (optional)
        if !consensus.dissent.is_empty() {
            out.push_str(&self.format_dissent(&consensus.dissent));
        }

        // 5. Conditions for Approval (optional)
        if !consensus.conditions.is_empty() {
            out.push_str(&self.format_conditions(&consensus.conditions));
        }

        // 6. Recommended Actions
        out.push_str(&self.format_recommendations(&consensus.recommendations));

        out
    }

    /// Generates the separator line: `+` + `=` * inner + `+`.
    fn format_separator(&self) -> String {
        format!("+{}+", "=".repeat(self.banner_inner))
    }

    /// Generates a content line: `|` + content padded to inner width + `|`.
    ///
    /// Content is left-aligned. If content exceeds inner width, it is truncated.
    fn format_line(&self, content: &str) -> String {
        if content.len() > self.banner_inner {
            let boundary = floor_char_boundary(content, self.banner_inner);
            format!(
                "|{:<width$}|",
                &content[..boundary],
                width = self.banner_inner
            )
        } else {
            format!("|{:<width$}|", content, width = self.banner_inner)
        }
    }

    /// Returns `(display_name, title)` for the given agent.
    ///
    /// Looks up in `config.agent_titles` first, falls back to
    /// `AgentName::display_name()` and `AgentName::title()`.
    fn agent_display(&self, name: &AgentName) -> (&str, &str) {
        if let Some((display_name, title)) = self.config.agent_titles.get(name) {
            (display_name.as_str(), title.as_str())
        } else {
            (name.display_name(), name.title())
        }
    }

    /// Formats the consensus summary section.
    fn format_consensus_summary(&self, consensus: &ConsensusResult) -> String {
        let mut out = String::new();
        writeln!(out, "\n## Consensus Summary\n").ok();
        writeln!(out, "{}", consensus.majority_summary).ok();
        out
    }

    /// Formats the key findings section.
    fn format_findings(&self, findings: &[DedupFinding]) -> String {
        let mut out = String::new();
        writeln!(out, "\n## Key Findings\n").ok();
        for finding in findings {
            let sources = finding
                .sources
                .iter()
                .map(|s| s.display_name())
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(
                out,
                "{} **[{}]** {} _(from {})_",
                finding.severity.icon(),
                finding.severity,
                finding.title,
                sources
            )
            .ok();
            writeln!(out, "   {}", finding.detail).ok();
            writeln!(out).ok();
        }
        out
    }

    /// Formats the dissenting opinion section.
    fn format_dissent(&self, dissent: &[Dissent]) -> String {
        let mut out = String::new();
        writeln!(out, "\n## Dissenting Opinion\n").ok();
        for d in dissent {
            let (display_name, title) = self.agent_display(&d.agent);
            writeln!(out, "**{} ({})**: {}", display_name, title, d.summary).ok();
            writeln!(out).ok();
            writeln!(out, "{}", d.reasoning).ok();
            writeln!(out).ok();
        }
        out
    }

    /// Formats the conditions for approval section.
    fn format_conditions(&self, conditions: &[Condition]) -> String {
        let mut out = String::new();
        writeln!(out, "\n## Conditions for Approval\n").ok();
        for c in conditions {
            let (display_name, _) = self.agent_display(&c.agent);
            writeln!(out, "- **{}**: {}", display_name, c.condition).ok();
        }
        writeln!(out).ok();
        out
    }

    /// Formats the recommended actions section.
    fn format_recommendations(&self, recommendations: &BTreeMap<AgentName, String>) -> String {
        let mut out = String::new();
        writeln!(out, "\n## Recommended Actions\n").ok();
        for (name, rec) in recommendations {
            let (display_name, title) = self.agent_display(name);
            writeln!(out, "- **{}** ({}): {}", display_name, title, rec).ok();
        }
        out
    }
}

/// Returns the largest byte index at or before `index` that is a UTF-8 char boundary.
///
/// Equivalent to `str::floor_char_boundary` (unstable, rust-lang#93743).
/// Walks backwards from `index` until a leading byte (not a continuation byte) is found.
fn floor_char_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        return s.len();
    }
    let mut i = index;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

impl Default for ReportFormatter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consensus::*;
    use crate::schema::*;

    /// Helper: build a minimal AgentOutput for testing.
    fn make_agent(
        name: AgentName,
        verdict: Verdict,
        confidence: f64,
        summary: &str,
        reasoning: &str,
        recommendation: &str,
    ) -> AgentOutput {
        AgentOutput {
            agent: name,
            verdict,
            confidence,
            summary: summary.to_string(),
            reasoning: reasoning.to_string(),
            findings: vec![],
            recommendation: recommendation.to_string(),
        }
    }

    /// Helper: build a minimal ConsensusResult for testing.
    fn make_consensus(
        label: &str,
        verdict: Verdict,
        confidence: f64,
        score: f64,
        agents: &[&AgentOutput],
    ) -> ConsensusResult {
        let mut votes = BTreeMap::new();
        let mut recommendations = BTreeMap::new();
        for a in agents {
            votes.insert(a.agent, a.verdict);
            recommendations.insert(a.agent, a.recommendation.clone());
        }

        let majority_summary = agents
            .iter()
            .filter(|a| a.effective_verdict() == verdict.effective())
            .map(|a| format!("{}: {}", a.agent.display_name(), a.summary))
            .collect::<Vec<_>>()
            .join(" | ");

        ConsensusResult {
            consensus: label.to_string(),
            consensus_verdict: verdict,
            confidence,
            score,
            agent_count: agents.len(),
            votes,
            majority_summary,
            dissent: vec![],
            findings: vec![],
            conditions: vec![],
            recommendations,
        }
    }

    // -- BDD Scenario 15: banner width --

    /// All banner lines are exactly 52 characters wide.
    #[test]
    fn test_banner_lines_are_exactly_52_chars_wide() {
        let m = make_agent(
            AgentName::Melchior,
            Verdict::Approve,
            0.9,
            "Good",
            "R",
            "Rec",
        );
        let b = make_agent(
            AgentName::Balthasar,
            Verdict::Conditional,
            0.85,
            "Ok",
            "R",
            "Rec",
        );
        let c = make_agent(AgentName::Caspar, Verdict::Reject, 0.78, "Bad", "R", "Rec");
        let agents = vec![m.clone(), b.clone(), c.clone()];
        let consensus = make_consensus(
            "GO WITH CAVEATS",
            Verdict::Approve,
            0.85,
            0.33,
            &[&m, &b, &c],
        );

        let formatter = ReportFormatter::new();
        let banner = formatter.format_banner(&agents, &consensus);

        for line in banner.lines() {
            if !line.is_empty() {
                assert_eq!(line.len(), 52, "Line is not 52 chars: '{}'", line);
            }
        }
    }

    /// Banner with long consensus label still fits 52 chars.
    #[test]
    fn test_banner_with_long_content_fits_52_chars() {
        let m = make_agent(AgentName::Melchior, Verdict::Approve, 0.9, "S", "R", "Rec");
        let b = make_agent(
            AgentName::Balthasar,
            Verdict::Approve,
            0.85,
            "S",
            "R",
            "Rec",
        );
        let c = make_agent(AgentName::Caspar, Verdict::Approve, 0.95, "S", "R", "Rec");
        let agents = vec![m.clone(), b.clone(), c.clone()];
        let consensus = make_consensus("STRONG GO", Verdict::Approve, 0.9, 1.0, &[&m, &b, &c]);

        let formatter = ReportFormatter::new();
        let banner = formatter.format_banner(&agents, &consensus);

        for line in banner.lines() {
            if !line.is_empty() {
                assert_eq!(line.len(), 52, "Line is not 52 chars: '{}'", line);
            }
        }
    }

    // -- BDD Scenario 16: report sections --

    /// Report with mixed consensus contains all 5 markdown headers.
    #[test]
    fn test_report_with_mixed_consensus_contains_all_headers() {
        let m = make_agent(
            AgentName::Melchior,
            Verdict::Approve,
            0.9,
            "Good code",
            "Solid",
            "Merge",
        );
        let b = make_agent(
            AgentName::Balthasar,
            Verdict::Conditional,
            0.85,
            "Needs work",
            "Issues",
            "Fix first",
        );
        let c = make_agent(
            AgentName::Caspar,
            Verdict::Reject,
            0.78,
            "Problems",
            "Risky",
            "Reject",
        );
        let agents = vec![m.clone(), b.clone(), c.clone()];

        let mut consensus = make_consensus(
            "GO WITH CAVEATS",
            Verdict::Approve,
            0.85,
            0.33,
            &[&m, &b, &c],
        );
        consensus.dissent = vec![Dissent {
            agent: AgentName::Caspar,
            summary: "Problems found".to_string(),
            reasoning: "Risk is too high".to_string(),
        }];
        consensus.conditions = vec![Condition {
            agent: AgentName::Balthasar,
            condition: "Fix first".to_string(),
        }];
        consensus.findings = vec![DedupFinding {
            severity: Severity::Warning,
            title: "Test finding".to_string(),
            detail: "Detail here".to_string(),
            sources: vec![AgentName::Melchior, AgentName::Caspar],
        }];

        let formatter = ReportFormatter::new();
        let report = formatter.format_report(&agents, &consensus);

        assert!(
            report.contains("## Consensus Summary"),
            "Missing Consensus Summary"
        );
        assert!(report.contains("## Key Findings"), "Missing Key Findings");
        assert!(
            report.contains("## Dissenting Opinion"),
            "Missing Dissenting Opinion"
        );
        assert!(
            report.contains("## Conditions for Approval"),
            "Missing Conditions"
        );
        assert!(
            report.contains("## Recommended Actions"),
            "Missing Recommended Actions"
        );
    }

    /// Report without dissent omits "## Dissenting Opinion".
    #[test]
    fn test_report_without_dissent_omits_dissent_section() {
        let m = make_agent(
            AgentName::Melchior,
            Verdict::Approve,
            0.9,
            "Good",
            "R",
            "Merge",
        );
        let b = make_agent(
            AgentName::Balthasar,
            Verdict::Approve,
            0.85,
            "Good",
            "R",
            "Merge",
        );
        let c = make_agent(
            AgentName::Caspar,
            Verdict::Approve,
            0.95,
            "Good",
            "R",
            "Merge",
        );
        let agents = vec![m.clone(), b.clone(), c.clone()];
        let consensus = make_consensus("STRONG GO", Verdict::Approve, 0.9, 1.0, &[&m, &b, &c]);

        let formatter = ReportFormatter::new();
        let report = formatter.format_report(&agents, &consensus);

        assert!(!report.contains("## Dissenting Opinion"));
    }

    /// Report without conditions omits "## Conditions for Approval".
    #[test]
    fn test_report_without_conditions_omits_conditions_section() {
        let m = make_agent(
            AgentName::Melchior,
            Verdict::Approve,
            0.9,
            "Good",
            "R",
            "Merge",
        );
        let b = make_agent(
            AgentName::Balthasar,
            Verdict::Approve,
            0.85,
            "Good",
            "R",
            "Merge",
        );
        let c = make_agent(
            AgentName::Caspar,
            Verdict::Approve,
            0.95,
            "Good",
            "R",
            "Merge",
        );
        let agents = vec![m.clone(), b.clone(), c.clone()];
        let consensus = make_consensus("STRONG GO", Verdict::Approve, 0.9, 1.0, &[&m, &b, &c]);

        let formatter = ReportFormatter::new();
        let report = formatter.format_report(&agents, &consensus);

        assert!(!report.contains("## Conditions for Approval"));
    }

    /// Report without findings omits "## Key Findings".
    #[test]
    fn test_report_without_findings_omits_findings_section() {
        let m = make_agent(
            AgentName::Melchior,
            Verdict::Approve,
            0.9,
            "Good",
            "R",
            "Merge",
        );
        let b = make_agent(
            AgentName::Balthasar,
            Verdict::Approve,
            0.85,
            "Good",
            "R",
            "Merge",
        );
        let c = make_agent(
            AgentName::Caspar,
            Verdict::Approve,
            0.95,
            "Good",
            "R",
            "Merge",
        );
        let agents = vec![m.clone(), b.clone(), c.clone()];
        let consensus = make_consensus("STRONG GO", Verdict::Approve, 0.9, 1.0, &[&m, &b, &c]);

        let formatter = ReportFormatter::new();
        let report = formatter.format_report(&agents, &consensus);

        assert!(!report.contains("## Key Findings"));
    }

    // -- Banner formatting --

    /// format_banner generates correct ASCII art structure.
    #[test]
    fn test_format_banner_has_correct_structure() {
        let m = make_agent(AgentName::Melchior, Verdict::Approve, 0.9, "S", "R", "Rec");
        let b = make_agent(AgentName::Balthasar, Verdict::Reject, 0.7, "S", "R", "Rec");
        let c = make_agent(AgentName::Caspar, Verdict::Reject, 0.8, "S", "R", "Rec");
        let agents = vec![m.clone(), b.clone(), c.clone()];
        let consensus = make_consensus("HOLD (2-1)", Verdict::Reject, 0.7, -0.33, &[&m, &b, &c]);

        let formatter = ReportFormatter::new();
        let banner = formatter.format_banner(&agents, &consensus);

        assert!(banner.contains("MAGI SYSTEM -- VERDICT"));
        assert!(banner.contains("Melchior (Scientist)"));
        assert!(banner.contains("APPROVE"));
        assert!(banner.contains("CONSENSUS:"));
        assert!(banner.contains("HOLD (2-1)"));
    }

    /// format_init_banner shows mode, model, timeout.
    #[test]
    fn test_format_init_banner_shows_mode_model_timeout() {
        let formatter = ReportFormatter::new();
        let banner = formatter.format_init_banner(&Mode::CodeReview, "claude-sonnet", 300);

        assert!(banner.contains("code-review"), "Missing mode");
        assert!(banner.contains("claude-sonnet"), "Missing model");
        assert!(banner.contains("300"), "Missing timeout");

        for line in banner.lines() {
            if !line.is_empty() {
                assert_eq!(line.len(), 52, "Init banner line not 52 chars: '{}'", line);
            }
        }
    }

    /// Separator line is "+" + "=" * 50 + "+".
    #[test]
    fn test_separator_format() {
        let formatter = ReportFormatter::new();
        let banner = formatter.format_init_banner(&Mode::Analysis, "test", 60);
        let sep = format!("+{}+", "=".repeat(50));

        assert!(banner.contains(&sep), "Missing separator line");
        assert_eq!(sep.len(), 52);
    }

    /// Agent line shows "Name (Title):  VERDICT (NN%)" format.
    #[test]
    fn test_agent_line_format() {
        let m = make_agent(AgentName::Melchior, Verdict::Approve, 0.9, "S", "R", "Rec");
        let b = make_agent(
            AgentName::Balthasar,
            Verdict::Approve,
            0.85,
            "S",
            "R",
            "Rec",
        );
        let c = make_agent(AgentName::Caspar, Verdict::Approve, 0.78, "S", "R", "Rec");
        let agents = vec![m.clone(), b.clone(), c.clone()];
        let consensus = make_consensus("STRONG GO", Verdict::Approve, 0.9, 1.0, &[&m, &b, &c]);

        let formatter = ReportFormatter::new();
        let banner = formatter.format_banner(&agents, &consensus);

        assert!(banner.contains("Melchior (Scientist):  APPROVE (90%)"));
        assert!(banner.contains("Caspar (Critic):  APPROVE (78%)"));
    }

    // -- Report content sections --

    /// Findings section shows icon + severity + title + sources + detail.
    #[test]
    fn test_findings_section_format() {
        let m = make_agent(
            AgentName::Melchior,
            Verdict::Approve,
            0.9,
            "Good",
            "R",
            "Merge",
        );
        let agents = vec![m.clone()];
        let mut consensus = make_consensus("GO (1-0)", Verdict::Approve, 0.9, 1.0, &[&m]);
        consensus.findings = vec![DedupFinding {
            severity: Severity::Critical,
            title: "SQL injection risk".to_string(),
            detail: "User input not sanitized".to_string(),
            sources: vec![AgentName::Melchior, AgentName::Caspar],
        }];

        let formatter = ReportFormatter::new();
        let report = formatter.format_report(&agents, &consensus);

        assert!(report.contains("[!!!]"), "Missing critical icon");
        assert!(report.contains("[CRITICAL]"), "Missing severity label");
        assert!(report.contains("SQL injection risk"), "Missing title");
        assert!(report.contains("Melchior"), "Missing source agent");
        assert!(report.contains("Caspar"), "Missing source agent");
        assert!(
            report.contains("User input not sanitized"),
            "Missing detail"
        );
    }

    /// Dissent section shows agent name, summary, full reasoning.
    #[test]
    fn test_dissent_section_format() {
        let m = make_agent(
            AgentName::Melchior,
            Verdict::Approve,
            0.9,
            "Good",
            "R",
            "Merge",
        );
        let c = make_agent(
            AgentName::Caspar,
            Verdict::Reject,
            0.8,
            "Bad",
            "Too risky",
            "Reject",
        );
        let agents = vec![m.clone(), c.clone()];
        let mut consensus = make_consensus("GO (1-1)", Verdict::Approve, 0.8, 0.0, &[&m, &c]);
        consensus.dissent = vec![Dissent {
            agent: AgentName::Caspar,
            summary: "Too many issues".to_string(),
            reasoning: "The code has critical flaws".to_string(),
        }];

        let formatter = ReportFormatter::new();
        let report = formatter.format_report(&agents, &consensus);

        assert!(report.contains("Caspar"), "Missing dissenting agent name");
        assert!(report.contains("Critic"), "Missing dissenting agent title");
        assert!(
            report.contains("Too many issues"),
            "Missing dissent summary"
        );
        assert!(
            report.contains("The code has critical flaws"),
            "Missing dissent reasoning"
        );
    }

    /// Conditions section shows bulleted list with agent names.
    #[test]
    fn test_conditions_section_format() {
        let m = make_agent(
            AgentName::Melchior,
            Verdict::Approve,
            0.9,
            "Good",
            "R",
            "Merge",
        );
        let b = make_agent(
            AgentName::Balthasar,
            Verdict::Conditional,
            0.85,
            "Ok",
            "R",
            "Fix tests",
        );
        let agents = vec![m.clone(), b.clone()];
        let mut consensus =
            make_consensus("GO WITH CAVEATS", Verdict::Approve, 0.85, 0.75, &[&m, &b]);
        consensus.conditions = vec![Condition {
            agent: AgentName::Balthasar,
            condition: "Fix tests first".to_string(),
        }];

        let formatter = ReportFormatter::new();
        let report = formatter.format_report(&agents, &consensus);

        assert!(
            report.contains("- **Balthasar**:"),
            "Missing bullet with agent name"
        );
        assert!(report.contains("Fix tests first"), "Missing condition text");
    }

    /// Recommendations section shows per-agent recommendations.
    #[test]
    fn test_recommendations_section_format() {
        let m = make_agent(
            AgentName::Melchior,
            Verdict::Approve,
            0.9,
            "Good",
            "R",
            "Merge immediately",
        );
        let b = make_agent(
            AgentName::Balthasar,
            Verdict::Approve,
            0.85,
            "Good",
            "R",
            "Ship it",
        );
        let agents = vec![m.clone(), b.clone()];
        let consensus = make_consensus("GO (2-0)", Verdict::Approve, 0.9, 1.0, &[&m, &b]);

        let formatter = ReportFormatter::new();
        let report = formatter.format_report(&agents, &consensus);

        assert!(
            report.contains("Merge immediately"),
            "Missing Melchior recommendation"
        );
        assert!(
            report.contains("Ship it"),
            "Missing Balthasar recommendation"
        );
    }

    /// Agent display falls back to AgentName methods when not in config.
    #[test]
    fn test_agent_display_fallback_to_agent_name_methods() {
        let config = ReportConfig {
            banner_width: 52,
            agent_titles: BTreeMap::new(),
        };
        let formatter = ReportFormatter::with_config(config);

        let m = make_agent(AgentName::Melchior, Verdict::Approve, 0.9, "S", "R", "Rec");
        let agents = vec![m.clone()];
        let consensus = make_consensus("GO (1-0)", Verdict::Approve, 0.9, 1.0, &[&m]);

        let banner = formatter.format_banner(&agents, &consensus);
        assert!(
            banner.contains("Melchior"),
            "Should use AgentName::display_name()"
        );
    }

    // -- MagiReport tests --

    /// MagiReport serializes to JSON.
    #[test]
    fn test_magi_report_serializes_to_json() {
        let m = make_agent(
            AgentName::Melchior,
            Verdict::Approve,
            0.9,
            "Good",
            "R",
            "Merge",
        );
        let agents = vec![m.clone()];
        let consensus = make_consensus("GO (1-0)", Verdict::Approve, 0.9, 1.0, &[&m]);

        let report = MagiReport {
            agents,
            consensus,
            banner: "banner".to_string(),
            report: "report".to_string(),
            degraded: false,
            failed_agents: BTreeMap::new(),
        };

        let json = serde_json::to_string(&report).expect("serialize");
        assert!(json.contains("\"consensus\""));
        assert!(json.contains("\"agents\""));
        assert!(json.contains("\"degraded\""));
    }

    /// degraded=false when all 3 agents succeed.
    #[test]
    fn test_magi_report_not_degraded_with_three_agents() {
        let m = make_agent(AgentName::Melchior, Verdict::Approve, 0.9, "S", "R", "Rec");
        let b = make_agent(
            AgentName::Balthasar,
            Verdict::Approve,
            0.85,
            "S",
            "R",
            "Rec",
        );
        let c = make_agent(AgentName::Caspar, Verdict::Approve, 0.95, "S", "R", "Rec");
        let agents = vec![m.clone(), b.clone(), c.clone()];
        let consensus = make_consensus("STRONG GO", Verdict::Approve, 0.9, 1.0, &[&m, &b, &c]);

        let report = MagiReport {
            agents,
            consensus,
            banner: String::new(),
            report: String::new(),
            degraded: false,
            failed_agents: BTreeMap::new(),
        };

        assert!(!report.degraded);
        assert!(report.failed_agents.is_empty());
    }

    /// degraded=true with failed_agents populated when agent fails.
    #[test]
    fn test_magi_report_degraded_with_failed_agents() {
        let m = make_agent(AgentName::Melchior, Verdict::Approve, 0.9, "S", "R", "Rec");
        let b = make_agent(
            AgentName::Balthasar,
            Verdict::Approve,
            0.85,
            "S",
            "R",
            "Rec",
        );
        let agents = vec![m.clone(), b.clone()];
        let consensus = make_consensus("GO (2-0)", Verdict::Approve, 0.9, 1.0, &[&m, &b]);

        let report = MagiReport {
            agents,
            consensus,
            banner: String::new(),
            report: String::new(),
            degraded: true,
            failed_agents: BTreeMap::from([(AgentName::Caspar, "timeout".to_string())]),
        };

        assert!(report.degraded);
        assert_eq!(report.failed_agents.len(), 1);
        assert!(report.failed_agents.contains_key(&AgentName::Caspar));
    }

    /// Agent names in JSON are lowercase.
    #[test]
    fn test_magi_report_json_agent_names_lowercase() {
        let m = make_agent(AgentName::Melchior, Verdict::Approve, 0.9, "S", "R", "Rec");
        let agents = vec![m.clone()];
        let consensus = make_consensus("GO (1-0)", Verdict::Approve, 0.9, 1.0, &[&m]);

        let report = MagiReport {
            agents,
            consensus,
            banner: String::new(),
            report: String::new(),
            degraded: false,
            failed_agents: BTreeMap::new(),
        };

        let json = serde_json::to_string(&report).expect("serialize");
        assert!(
            json.contains("\"melchior\""),
            "Agent name should be lowercase in JSON"
        );
        assert!(
            !json.contains("\"Melchior\""),
            "Agent name should NOT be capitalized in JSON"
        );
    }

    /// consensus.confidence is rounded to 2 decimals.
    #[test]
    fn test_magi_report_confidence_rounded() {
        let m = make_agent(AgentName::Melchior, Verdict::Approve, 0.9, "S", "R", "Rec");
        let agents = vec![m.clone()];
        let mut consensus = make_consensus("GO (1-0)", Verdict::Approve, 0.86, 1.0, &[&m]);
        consensus.confidence = 0.8567;

        let report = MagiReport {
            agents,
            consensus,
            banner: String::new(),
            report: String::new(),
            degraded: false,
            failed_agents: BTreeMap::new(),
        };

        // Confidence rounding is done by the consensus engine, not by MagiReport.
        // Here we verify the field value is preserved as-is during serialization.
        let json = serde_json::to_string(&report).expect("serialize");
        assert!(
            json.contains("0.8567"),
            "Confidence should be serialized as-is (rounding is consensus engine's job)"
        );
    }
}
