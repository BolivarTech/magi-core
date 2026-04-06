// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::error::MagiError;
use crate::schema::{AgentName, AgentOutput, Finding, Severity, Verdict};

/// Configuration for the consensus engine.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct ConsensusConfig {
    /// Minimum number of successful agent outputs required (default: 2).
    pub min_agents: usize,
    /// Tolerance for floating-point comparisons (default: 1e-9).
    pub epsilon: f64,
}

/// Result of the consensus determination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusResult {
    /// Classification label (e.g., "STRONG GO", "GO WITH CAVEATS").
    pub consensus: String,
    /// Final verdict enum.
    pub consensus_verdict: Verdict,
    /// Computed confidence, rounded to 2 decimals.
    pub confidence: f64,
    /// Raw normalized score.
    pub score: f64,
    /// Number of agents that contributed.
    pub agent_count: usize,
    /// Per-agent verdicts.
    pub votes: BTreeMap<AgentName, Verdict>,
    /// Joined summaries from majority side.
    pub majority_summary: String,
    /// Dissenting agent details.
    pub dissent: Vec<Dissent>,
    /// Deduplicated findings sorted by severity (Critical first).
    pub findings: Vec<DedupFinding>,
    /// Conditions from Conditional agents.
    pub conditions: Vec<Condition>,
    /// Per-agent recommendations.
    pub recommendations: BTreeMap<AgentName, String>,
}

/// A deduplicated finding aggregated across agents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DedupFinding {
    /// Severity level (promoted to highest across duplicates).
    pub severity: Severity,
    /// Finding title.
    pub title: String,
    /// Finding detail (from highest-severity contributor).
    pub detail: String,
    /// Agents that reported this finding.
    pub sources: Vec<AgentName>,
}

/// A dissenting agent's summary and reasoning.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Dissent {
    /// The dissenting agent.
    pub agent: AgentName,
    /// The agent's summary.
    pub summary: String,
    /// The agent's reasoning.
    pub reasoning: String,
}

/// A condition extracted from a Conditional-verdict agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Condition {
    /// The agent that set the condition.
    pub agent: AgentName,
    /// The condition text (from the agent's recommendation).
    pub condition: String,
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        Self {
            min_agents: 2,
            epsilon: 1e-9,
        }
    }
}

/// Stateless consensus engine for synthesizing agent outputs.
///
/// The engine takes a slice of [`AgentOutput`] values and produces a
/// [`ConsensusResult`] containing the consensus label, confidence score,
/// deduplicated findings, dissent tracking, and condition extraction.
///
/// The engine is stateless: each call to [`determine`](ConsensusEngine::determine)
/// is independent and the engine is safe to share across threads.
pub struct ConsensusEngine {
    config: ConsensusConfig,
}

impl ConsensusEngine {
    /// Returns the minimum number of agents required by this engine's configuration.
    pub fn min_agents(&self) -> usize {
        self.config.min_agents
    }

    /// Creates a new consensus engine with the given configuration.
    ///
    /// If `config.min_agents` is 0, it is clamped to 1.
    pub fn new(config: ConsensusConfig) -> Self {
        let min_agents = if config.min_agents == 0 {
            1
        } else {
            config.min_agents
        };
        Self {
            config: ConsensusConfig {
                min_agents,
                ..config
            },
        }
    }

    /// Synthesizes agent outputs into a unified consensus result.
    ///
    /// # Errors
    ///
    /// - [`MagiError::InsufficientAgents`] if fewer than `min_agents` are provided.
    /// - [`MagiError::Validation`] if duplicate agent names are detected.
    pub fn determine(&self, agents: &[AgentOutput]) -> Result<ConsensusResult, MagiError> {
        // 1. Validate input count
        if agents.len() < self.config.min_agents {
            return Err(MagiError::InsufficientAgents {
                succeeded: agents.len(),
                required: self.config.min_agents,
            });
        }

        // 2. Reject duplicates
        let mut seen = std::collections::HashSet::new();
        for agent in agents {
            if !seen.insert(agent.agent) {
                return Err(MagiError::Validation(format!(
                    "duplicate agent name: {}",
                    agent.agent.display_name()
                )));
            }
        }

        let n = agents.len() as f64;
        let epsilon = self.config.epsilon;

        // 3. Compute normalized score
        let score: f64 = agents.iter().map(|a| a.verdict.weight()).sum::<f64>() / n;

        // 4. Determine majority verdict
        let approve_count = agents
            .iter()
            .filter(|a| a.effective_verdict() == Verdict::Approve)
            .count();
        let reject_count = agents.len() - approve_count;

        let has_conditional = agents.iter().any(|a| a.verdict == Verdict::Conditional);

        let majority_verdict = match approve_count.cmp(&reject_count) {
            std::cmp::Ordering::Greater => Verdict::Approve,
            std::cmp::Ordering::Less => Verdict::Reject,
            std::cmp::Ordering::Equal => {
                // Tie: break by alphabetically first agent on each side
                let first_approve = agents
                    .iter()
                    .filter(|a| a.effective_verdict() == Verdict::Approve)
                    .map(|a| a.agent)
                    .min();
                let first_reject = agents
                    .iter()
                    .filter(|a| a.effective_verdict() == Verdict::Reject)
                    .map(|a| a.agent)
                    .min();
                match (first_approve, first_reject) {
                    (Some(a), Some(r)) if a < r => Verdict::Approve,
                    (Some(_), None) => Verdict::Approve,
                    _ => Verdict::Reject,
                }
            }
        };

        // 5. Classify score to label + consensus verdict
        let (mut label, consensus_verdict) =
            self.classify(score, epsilon, approve_count, reject_count, has_conditional);

        // 6. Degraded mode cap
        if agents.len() < 3 {
            if label == "STRONG GO" {
                label = format!("GO ({}-0)", agents.len());
            } else if label == "STRONG NO-GO" {
                label = format!("HOLD ({}-0)", agents.len());
            }
        }

        // 7. Compute confidence
        // base_confidence: sum of majority-side confidences divided by TOTAL agent
        // count (not majority count). This intentionally penalizes non-unanimous
        // results — a dissenting agent dilutes the overall confidence even though
        // it is not on the majority side.
        // weight_factor: maps |score| from [0,1] to [0.5,1.0], so unanimous
        // verdicts (|score|=1) get full weight while ties (score=0) halve it.
        let base_confidence: f64 = agents
            .iter()
            .filter(|a| a.effective_verdict() == majority_verdict)
            .map(|a| a.confidence)
            .sum::<f64>()
            / n;
        let weight_factor = (score.abs() + 1.0) / 2.0;
        let confidence = (base_confidence * weight_factor).clamp(0.0, 1.0);
        let confidence = (confidence * 100.0).round() / 100.0;

        // 8. Deduplicate findings
        let findings = self.deduplicate_findings(agents);

        // 9. Identify dissent
        let dissent: Vec<Dissent> = agents
            .iter()
            .filter(|a| a.effective_verdict() != majority_verdict)
            .map(|a| Dissent {
                agent: a.agent,
                summary: a.summary.clone(),
                reasoning: a.reasoning.clone(),
            })
            .collect();

        // 10. Extract conditions
        let conditions: Vec<Condition> = agents
            .iter()
            .filter(|a| a.verdict == Verdict::Conditional)
            .map(|a| Condition {
                agent: a.agent,
                condition: a.recommendation.clone(),
            })
            .collect();

        // 11. Build votes map
        let votes: BTreeMap<AgentName, Verdict> =
            agents.iter().map(|a| (a.agent, a.verdict)).collect();

        // 12. Build majority summary
        let majority_summary = agents
            .iter()
            .filter(|a| a.effective_verdict() == majority_verdict)
            .map(|a| a.summary.as_str())
            .collect::<Vec<_>>()
            .join(" | ");

        // 13. Build recommendations map
        let recommendations: BTreeMap<AgentName, String> = agents
            .iter()
            .map(|a| (a.agent, a.recommendation.clone()))
            .collect();

        Ok(ConsensusResult {
            consensus: label,
            consensus_verdict,
            confidence,
            score,
            agent_count: agents.len(),
            votes,
            majority_summary,
            dissent,
            findings,
            conditions,
            recommendations,
        })
    }

    /// Classifies a score into a consensus label and verdict.
    fn classify(
        &self,
        score: f64,
        epsilon: f64,
        approve_count: usize,
        reject_count: usize,
        has_conditional: bool,
    ) -> (String, Verdict) {
        if (score - 1.0).abs() < epsilon {
            ("STRONG GO".to_string(), Verdict::Approve)
        } else if (score - (-1.0)).abs() < epsilon {
            ("STRONG NO-GO".to_string(), Verdict::Reject)
        } else if score > epsilon && has_conditional {
            ("GO WITH CAVEATS".to_string(), Verdict::Approve)
        } else if score > epsilon {
            (
                format!("GO ({}-{})", approve_count, reject_count),
                Verdict::Approve,
            )
        } else if score.abs() < epsilon {
            ("HOLD -- TIE".to_string(), Verdict::Reject)
        } else {
            (
                format!("HOLD ({}-{})", reject_count, approve_count),
                Verdict::Reject,
            )
        }
    }

    /// Deduplicates findings across agents by case-insensitive stripped title.
    fn deduplicate_findings(&self, agents: &[AgentOutput]) -> Vec<DedupFinding> {
        // Collect (agent_name, finding) pairs sorted by agent name for determinism
        let mut agent_findings: Vec<(AgentName, &Finding)> = Vec::new();
        for agent in agents {
            for finding in &agent.findings {
                agent_findings.push((agent.agent, finding));
            }
        }
        // Sort by agent name for deterministic tiebreaking
        agent_findings.sort_by(|a, b| a.0.cmp(&b.0));

        // Group by case-insensitive stripped title
        let mut groups: std::collections::HashMap<String, Vec<(AgentName, &Finding)>> =
            std::collections::HashMap::new();
        let mut order: Vec<String> = Vec::new();

        for (agent_name, finding) in &agent_findings {
            let key = finding
                .stripped_title()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase();
            if !groups.contains_key(&key) {
                order.push(key.clone());
            }
            groups.entry(key).or_default().push((*agent_name, finding));
        }

        let mut result: Vec<DedupFinding> = Vec::new();
        for key in &order {
            let entries = &groups[key];
            // Find highest severity
            let max_severity = entries.iter().map(|(_, f)| f.severity).max().unwrap();
            // Find the first entry with that severity (already sorted by agent name)
            let best = entries
                .iter()
                .find(|(_, f)| f.severity == max_severity)
                .unwrap();
            let sources: Vec<AgentName> = entries.iter().map(|(name, _)| *name).collect();
            result.push(DedupFinding {
                severity: max_severity,
                title: best.1.title.clone(),
                detail: best.1.detail.clone(),
                sources,
            });
        }

        // Sort by severity (Critical first = descending)
        result.sort_by(|a, b| b.severity.cmp(&a.severity));
        result
    }
}

impl Default for ConsensusEngine {
    fn default() -> Self {
        Self::new(ConsensusConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::*;

    fn make_output(agent: AgentName, verdict: Verdict, confidence: f64) -> AgentOutput {
        AgentOutput {
            agent,
            verdict,
            confidence,
            summary: format!("{} summary", agent.display_name()),
            reasoning: format!("{} reasoning", agent.display_name()),
            findings: vec![],
            recommendation: format!("{} recommendation", agent.display_name()),
        }
    }

    // -- BDD Scenario 1: unanimous approve --

    /// Three approve agents produce STRONG GO with score=1.0.
    #[test]
    fn test_unanimous_approve_produces_strong_go() {
        let agents = vec![
            make_output(AgentName::Melchior, Verdict::Approve, 0.9),
            make_output(AgentName::Balthasar, Verdict::Approve, 0.9),
            make_output(AgentName::Caspar, Verdict::Approve, 0.9),
        ];
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&agents).unwrap();
        assert_eq!(result.consensus, "STRONG GO");
        assert_eq!(result.consensus_verdict, Verdict::Approve);
        assert!((result.score - 1.0).abs() < 1e-9);
    }

    // -- BDD Scenario 2: mixed 2 approve + 1 reject --

    /// Two approve + one reject produces GO (2-1) with positive score.
    #[test]
    fn test_two_approve_one_reject_produces_go_2_1() {
        let agents = vec![
            make_output(AgentName::Melchior, Verdict::Approve, 0.9),
            make_output(AgentName::Balthasar, Verdict::Approve, 0.8),
            make_output(AgentName::Caspar, Verdict::Reject, 0.7),
        ];
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&agents).unwrap();
        assert_eq!(result.consensus, "GO (2-1)");
        assert_eq!(result.consensus_verdict, Verdict::Approve);
        assert!(result.score > 0.0);
        assert_eq!(result.dissent.len(), 1);
        assert_eq!(result.dissent[0].agent, AgentName::Caspar);
    }

    // -- BDD Scenario 3: approve + conditional + reject --

    /// Approve + conditional + reject produces GO WITH CAVEATS.
    #[test]
    fn test_approve_conditional_reject_produces_go_with_caveats() {
        let agents = vec![
            make_output(AgentName::Melchior, Verdict::Approve, 0.9),
            make_output(AgentName::Balthasar, Verdict::Conditional, 0.8),
            make_output(AgentName::Caspar, Verdict::Reject, 0.7),
        ];
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&agents).unwrap();
        assert_eq!(result.consensus, "GO WITH CAVEATS");
        assert_eq!(result.consensus_verdict, Verdict::Approve);
        assert!(!result.conditions.is_empty());
        assert_eq!(result.conditions[0].agent, AgentName::Balthasar);
    }

    // -- BDD Scenario 4: unanimous reject --

    /// Three reject agents produce STRONG NO-GO with score=-1.0.
    #[test]
    fn test_unanimous_reject_produces_strong_no_go() {
        let agents = vec![
            make_output(AgentName::Melchior, Verdict::Reject, 0.9),
            make_output(AgentName::Balthasar, Verdict::Reject, 0.8),
            make_output(AgentName::Caspar, Verdict::Reject, 0.7),
        ];
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&agents).unwrap();
        assert_eq!(result.consensus, "STRONG NO-GO");
        assert_eq!(result.consensus_verdict, Verdict::Reject);
        assert!((result.score - (-1.0)).abs() < 1e-9);
    }

    // -- BDD Scenario 5: tie with 2 agents --

    /// One approve + one reject (2 agents) produces HOLD -- TIE.
    #[test]
    fn test_tie_with_two_agents_produces_hold_tie() {
        let agents = vec![
            make_output(AgentName::Melchior, Verdict::Approve, 0.9),
            make_output(AgentName::Caspar, Verdict::Reject, 0.9),
        ];
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&agents).unwrap();
        assert_eq!(result.consensus, "HOLD -- TIE");
        assert_eq!(result.consensus_verdict, Verdict::Reject);
    }

    // -- BDD Scenario 13: finding deduplication --

    /// Same title different case merges into single finding with severity promoted.
    #[test]
    fn test_duplicate_findings_merged_with_severity_promoted() {
        let mut m = make_output(AgentName::Melchior, Verdict::Approve, 0.9);
        m.findings.push(Finding {
            severity: Severity::Warning,
            title: "Security Issue".to_string(),
            detail: "detail_warning".to_string(),
        });
        let mut b = make_output(AgentName::Balthasar, Verdict::Approve, 0.9);
        b.findings.push(Finding {
            severity: Severity::Critical,
            title: "security issue".to_string(),
            detail: "detail_critical".to_string(),
        });
        let c = make_output(AgentName::Caspar, Verdict::Approve, 0.9);
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&[m, b, c]).unwrap();
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].severity, Severity::Critical);
    }

    /// Merged finding sources include both contributing agents.
    #[test]
    fn test_merged_finding_sources_include_both_agents() {
        let mut m = make_output(AgentName::Melchior, Verdict::Approve, 0.9);
        m.findings.push(Finding {
            severity: Severity::Warning,
            title: "Security Issue".to_string(),
            detail: "detail_m".to_string(),
        });
        let mut b = make_output(AgentName::Balthasar, Verdict::Approve, 0.9);
        b.findings.push(Finding {
            severity: Severity::Critical,
            title: "security issue".to_string(),
            detail: "detail_b".to_string(),
        });
        let c = make_output(AgentName::Caspar, Verdict::Approve, 0.9);
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&[m, b, c]).unwrap();
        assert_eq!(result.findings[0].sources.len(), 2);
        assert!(result.findings[0].sources.contains(&AgentName::Melchior));
        assert!(result.findings[0].sources.contains(&AgentName::Balthasar));
    }

    /// Detail preserved from highest-severity finding.
    #[test]
    fn test_merged_finding_detail_from_highest_severity() {
        let mut m = make_output(AgentName::Melchior, Verdict::Approve, 0.9);
        m.findings.push(Finding {
            severity: Severity::Warning,
            title: "Issue".to_string(),
            detail: "detail_warning".to_string(),
        });
        let mut b = make_output(AgentName::Balthasar, Verdict::Approve, 0.9);
        b.findings.push(Finding {
            severity: Severity::Critical,
            title: "issue".to_string(),
            detail: "detail_critical".to_string(),
        });
        let c = make_output(AgentName::Caspar, Verdict::Approve, 0.9);
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&[m, b, c]).unwrap();
        assert_eq!(result.findings[0].detail, "detail_critical");
    }

    /// On same severity, detail comes from first agent by AgentName ordering.
    #[test]
    fn test_merged_finding_detail_from_first_agent_on_same_severity() {
        let mut b = make_output(AgentName::Balthasar, Verdict::Approve, 0.9);
        b.findings.push(Finding {
            severity: Severity::Warning,
            title: "Issue".to_string(),
            detail: "detail_b".to_string(),
        });
        let mut m = make_output(AgentName::Melchior, Verdict::Approve, 0.9);
        m.findings.push(Finding {
            severity: Severity::Warning,
            title: "issue".to_string(),
            detail: "detail_m".to_string(),
        });
        let c = make_output(AgentName::Caspar, Verdict::Approve, 0.9);
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&[b, m, c]).unwrap();
        // Balthasar < Melchior alphabetically, same severity → Balthasar's detail
        assert_eq!(result.findings[0].detail, "detail_b");
    }

    // -- BDD Scenario 33: degraded mode caps STRONG labels --

    /// Two approve agents (degraded) produce GO (2-0) not STRONG GO.
    #[test]
    fn test_degraded_mode_caps_strong_go_to_go() {
        let agents = vec![
            make_output(AgentName::Melchior, Verdict::Approve, 0.9),
            make_output(AgentName::Balthasar, Verdict::Approve, 0.9),
        ];
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&agents).unwrap();
        assert_eq!(result.consensus, "GO (2-0)");
        assert_ne!(result.consensus, "STRONG GO");
    }

    /// Two reject agents (degraded) produce HOLD (2-0) not STRONG NO-GO.
    #[test]
    fn test_degraded_mode_caps_strong_no_go_to_hold() {
        let agents = vec![
            make_output(AgentName::Melchior, Verdict::Reject, 0.9),
            make_output(AgentName::Balthasar, Verdict::Reject, 0.9),
        ];
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&agents).unwrap();
        assert_eq!(result.consensus, "HOLD (2-0)");
        assert_ne!(result.consensus, "STRONG NO-GO");
    }

    // -- Error cases --

    /// Determine rejects fewer than min_agents with InsufficientAgents.
    #[test]
    fn test_determine_rejects_fewer_than_min_agents() {
        let agents = vec![make_output(AgentName::Melchior, Verdict::Approve, 0.9)];
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&agents);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(
            err,
            MagiError::InsufficientAgents {
                succeeded: 1,
                required: 2,
            }
        ));
    }

    /// Determine rejects duplicate agent names with Validation error.
    #[test]
    fn test_determine_rejects_duplicate_agent_names() {
        let agents = vec![
            make_output(AgentName::Melchior, Verdict::Approve, 0.9),
            make_output(AgentName::Melchior, Verdict::Reject, 0.8),
        ];
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&agents);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), MagiError::Validation(_)));
    }

    // -- Score and confidence calculations --

    /// Epsilon-aware classification near score boundaries.
    #[test]
    fn test_epsilon_aware_classification_near_boundaries() {
        // Score very close to 0 (within epsilon) should be HOLD -- TIE
        // Use Conditional(+0.5) + Approve(+1.0) + Reject(-1.0) = +0.5/3 ≈ 0.1667
        // That's not near zero. Instead, use a direct near-zero scenario:
        // We need agents whose weights sum to ~0. e.g. 1 approve + 1 reject = 0/2 = 0
        let agents = vec![
            make_output(AgentName::Melchior, Verdict::Approve, 0.9),
            make_output(AgentName::Balthasar, Verdict::Reject, 0.9),
        ];
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&agents).unwrap();
        assert_eq!(result.consensus, "HOLD -- TIE");
    }

    /// Confidence formula: base * weight_factor, clamped [0,1], rounded 2 decimals.
    #[test]
    fn test_confidence_formula_clamped_and_rounded() {
        // 3 approve agents with confidence 0.9 each
        // score = (1+1+1)/3 = 1.0
        // majority side = all 3 (approve), base = (0.9+0.9+0.9)/3 = 0.9
        // weight_factor = (1.0 + 1.0) / 2.0 = 1.0
        // confidence = 0.9 * 1.0 = 0.9
        let agents = vec![
            make_output(AgentName::Melchior, Verdict::Approve, 0.9),
            make_output(AgentName::Balthasar, Verdict::Approve, 0.9),
            make_output(AgentName::Caspar, Verdict::Approve, 0.9),
        ];
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&agents).unwrap();
        assert!((result.confidence - 0.9).abs() < 1e-9);
    }

    /// Confidence with mixed verdicts applies weight_factor correctly.
    #[test]
    fn test_confidence_with_mixed_verdicts() {
        // 2 approve (0.9, 0.8) + 1 reject (0.7)
        // score = (1+1-1)/3 = 1/3 ≈ 0.3333
        // majority = approve side: Melchior(0.9), Balthasar(0.8)
        // base = (0.9 + 0.8) / 3 = 0.5667
        // weight_factor = (0.3333 + 1.0) / 2.0 = 0.6667
        // confidence = 0.5667 * 0.6667 = 0.3778 → rounded = 0.38
        let agents = vec![
            make_output(AgentName::Melchior, Verdict::Approve, 0.9),
            make_output(AgentName::Balthasar, Verdict::Approve, 0.8),
            make_output(AgentName::Caspar, Verdict::Reject, 0.7),
        ];
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&agents).unwrap();
        assert!((result.confidence - 0.38).abs() < 0.01);
    }

    /// Majority summary joins majority agent summaries with " | ".
    #[test]
    fn test_majority_summary_joins_with_pipe() {
        let agents = vec![
            make_output(AgentName::Melchior, Verdict::Approve, 0.9),
            make_output(AgentName::Balthasar, Verdict::Approve, 0.8),
            make_output(AgentName::Caspar, Verdict::Reject, 0.7),
        ];
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&agents).unwrap();
        assert!(result.majority_summary.contains("Melchior summary"));
        assert!(result.majority_summary.contains("Balthasar summary"));
        assert!(result.majority_summary.contains(" | "));
        assert!(!result.majority_summary.contains("Caspar summary"));
    }

    /// Conditions extracted from agents with Conditional verdict.
    #[test]
    fn test_conditions_extracted_from_conditional_agents() {
        let agents = vec![
            make_output(AgentName::Melchior, Verdict::Approve, 0.9),
            make_output(AgentName::Balthasar, Verdict::Conditional, 0.8),
            make_output(AgentName::Caspar, Verdict::Reject, 0.7),
        ];
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&agents).unwrap();
        assert_eq!(result.conditions.len(), 1);
        assert_eq!(result.conditions[0].agent, AgentName::Balthasar);
        assert_eq!(result.conditions[0].condition, "Balthasar recommendation");
    }

    /// Recommendations map includes all agents.
    #[test]
    fn test_recommendations_includes_all_agents() {
        let agents = vec![
            make_output(AgentName::Melchior, Verdict::Approve, 0.9),
            make_output(AgentName::Balthasar, Verdict::Approve, 0.8),
            make_output(AgentName::Caspar, Verdict::Reject, 0.7),
        ];
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&agents).unwrap();
        assert_eq!(result.recommendations.len(), 3);
        assert!(result.recommendations.contains_key(&AgentName::Melchior));
        assert!(result.recommendations.contains_key(&AgentName::Balthasar));
        assert!(result.recommendations.contains_key(&AgentName::Caspar));
    }

    /// ConsensusConfig enforces min_agents >= 1.
    #[test]
    fn test_consensus_config_enforces_min_agents_at_least_one() {
        let config = ConsensusConfig {
            min_agents: 0,
            ..ConsensusConfig::default()
        };
        assert_eq!(config.min_agents, 0);
        // Engine should clamp to 1 internally
        let engine = ConsensusEngine::new(config);
        let agents = vec![make_output(AgentName::Melchior, Verdict::Approve, 0.9)];
        // Should succeed with 1 agent even though min_agents was 0 (clamped to 1)
        let result = engine.determine(&agents);
        assert!(result.is_ok());
    }

    /// ConsensusConfig::default() returns min_agents=2, epsilon=1e-9.
    #[test]
    fn test_consensus_config_default_values() {
        let config = ConsensusConfig::default();
        assert_eq!(config.min_agents, 2);
        assert!((config.epsilon - 1e-9).abs() < 1e-15);
    }

    /// Tiebreak by AgentName::cmp() — alphabetically first agent's side wins.
    #[test]
    fn test_tiebreak_by_agent_name_ordering() {
        // Balthasar=Approve, Melchior=Reject → tie (score=0)
        // Balthasar < Melchior alphabetically → Balthasar's side (Approve) wins tiebreak
        // But label should still be HOLD -- TIE
        let agents = vec![
            make_output(AgentName::Balthasar, Verdict::Approve, 0.9),
            make_output(AgentName::Melchior, Verdict::Reject, 0.9),
        ];
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&agents).unwrap();
        // Score is 0, so label is HOLD -- TIE, verdict is Reject
        assert_eq!(result.consensus, "HOLD -- TIE");
        assert_eq!(result.consensus_verdict, Verdict::Reject);
    }

    /// Findings sorted by severity (Critical first).
    #[test]
    fn test_findings_sorted_by_severity_critical_first() {
        let mut m = make_output(AgentName::Melchior, Verdict::Approve, 0.9);
        m.findings.push(Finding {
            severity: Severity::Info,
            title: "Info issue".to_string(),
            detail: "info detail".to_string(),
        });
        m.findings.push(Finding {
            severity: Severity::Critical,
            title: "Critical issue".to_string(),
            detail: "critical detail".to_string(),
        });
        let b = make_output(AgentName::Balthasar, Verdict::Approve, 0.9);
        let c = make_output(AgentName::Caspar, Verdict::Approve, 0.9);
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&[m, b, c]).unwrap();
        assert_eq!(result.findings.len(), 2);
        assert_eq!(result.findings[0].severity, Severity::Critical);
        assert_eq!(result.findings[1].severity, Severity::Info);
    }

    /// Titles differing only in whitespace (tabs, multiple spaces, NBSP) are deduplicated.
    #[test]
    fn test_duplicate_findings_merged_with_whitespace_normalization() {
        let mut m = make_output(AgentName::Melchior, Verdict::Approve, 0.9);
        m.findings.push(Finding {
            severity: Severity::Warning,
            title: "SQL  injection".to_string(),
            detail: "detail_m".to_string(),
        });
        let mut b = make_output(AgentName::Balthasar, Verdict::Approve, 0.9);
        b.findings.push(Finding {
            severity: Severity::Warning,
            title: "SQL\tinjection".to_string(),
            detail: "detail_b".to_string(),
        });
        let mut c = make_output(AgentName::Caspar, Verdict::Approve, 0.9);
        c.findings.push(Finding {
            severity: Severity::Critical,
            title: "sql injection".to_string(),
            detail: "detail_c".to_string(),
        });
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&[m, b, c]).unwrap();
        assert_eq!(result.findings.len(), 1, "should merge all three into one");
        assert_eq!(result.findings[0].severity, Severity::Critical);
        assert_eq!(result.findings[0].sources.len(), 3);
    }

    /// Votes map contains all agent verdicts.
    #[test]
    fn test_votes_map_contains_all_agents() {
        let agents = vec![
            make_output(AgentName::Melchior, Verdict::Approve, 0.9),
            make_output(AgentName::Balthasar, Verdict::Reject, 0.8),
            make_output(AgentName::Caspar, Verdict::Conditional, 0.7),
        ];
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&agents).unwrap();
        assert_eq!(result.votes.len(), 3);
        assert_eq!(result.votes[&AgentName::Melchior], Verdict::Approve);
        assert_eq!(result.votes[&AgentName::Balthasar], Verdict::Reject);
        assert_eq!(result.votes[&AgentName::Caspar], Verdict::Conditional);
    }

    /// Agent count reflects number of inputs.
    #[test]
    fn test_agent_count_reflects_input_count() {
        let agents = vec![
            make_output(AgentName::Melchior, Verdict::Approve, 0.9),
            make_output(AgentName::Balthasar, Verdict::Approve, 0.8),
            make_output(AgentName::Caspar, Verdict::Approve, 0.7),
        ];
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&agents).unwrap();
        assert_eq!(result.agent_count, 3);
    }
}
