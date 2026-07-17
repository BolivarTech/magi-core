// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use unicode_normalization::UnicodeNormalization;

use crate::error::MagiError;
use crate::finding_id::generate_finding_id;
use crate::schema::{AgentName, AgentOutput, Category, Severity, Verdict};
use crate::validate::clean_title;

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

/// A finding after cross-agent deduplication.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct DedupFinding {
    /// Severity (promoted to highest across duplicates).
    pub severity: Severity,
    /// Finding title (first-seen form).
    pub title: String,
    /// Finding detail (from highest-severity contributor).
    pub detail: String,
    /// Agents that reported this finding.
    pub sources: Vec<AgentName>,
    /// Agent-reported, unverified (see `crate::schema::Finding::file`).
    #[serde(default)]
    pub file: Option<String>,
    /// 1-based line number; agent-reported, unverified.
    #[serde(default)]
    pub line: Option<u32>,
    /// Finding category (propagated from the first-seen finding).
    #[serde(default)]
    pub category: Category,
    /// Stable id; `Some` only for located findings (file + line present).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
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
    /// The condition text (from the agent's summary).
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

/// Computes the deduplication key for a finding title.
///
/// Applies a three-step transformation, matching Python's `_dedup_key` behavior:
///
/// 1. [`clean_title`] — strips zero-width Unicode characters, normalizes line
///    endings and control characters to a single space, trims leading/trailing
///    whitespace.  Interior runs of multiple spaces are **not** collapsed (aligned
///    with Python: `clean_title` does not coalesce interior whitespace).
/// 2. NFKC normalization — collapses compatibility variants (e.g., fullwidth Latin
///    `ＡＢＣ` → `ABC`, ligatures, circled letters).
/// 3. Unicode default casefold via `caseless::default_case_fold_str` — handles
///    characters that `to_lowercase` misses, e.g., `ß` → `"ss"`, `Σ`/`σ`/`ς` → same
///    folded form.  This is **not** locale-aware (Turkish dotted-I folds per Unicode
///    default tables, not Turkish locale rules).
///
/// ### Divergence from v0.1.x
///
/// v0.1.x applied `split_whitespace().join(" ")` before lowercasing, so
/// `"foo  bar"` and `"foo bar"` were treated as the same finding. This function
/// does **not** collapse interior whitespace — `"foo  bar"` and `"foo bar"`
/// produce distinct keys, matching Python behavior.
fn dedup_key(title: &str) -> String {
    caseless::default_case_fold_str(&clean_title(title).nfkc().collect::<String>())
}

/// Discriminated key for deduplication: stable id for located findings, normalized title otherwise.
///
/// Using an `id:` vs `title:` prefix ensures that a located finding and an unlocated finding
/// with the same title never collide into the same group.
enum DedupKey {
    /// Finding has file + line + category → use stable id hash.
    Id(String),
    /// Finding lacks location → use normalized title.
    Title(String),
}

/// Produces a [`DedupKey`] for a finding.
///
/// Located findings (non-empty `file` and a positive `line`) produce a stable
/// hash id via [`generate_finding_id`]. A non-positive `line` is treated as
/// unlocated: `de_opt_line` already maps it to `None` on the deserialize path,
/// and this guard also rejects a `line == 0` set directly via `with_location`,
/// so an invalid 1-based location never yields an id. Unlocated findings fall
/// back to [`dedup_key`] on the title.
fn finding_key(f: &crate::schema::Finding) -> DedupKey {
    match (f.file.as_deref(), f.line) {
        (Some(file), Some(line)) if !file.is_empty() && line > 0 => {
            DedupKey::Id(generate_finding_id(file, line, f.category))
        }
        _ => DedupKey::Title(dedup_key(&f.title)),
    }
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
                condition: a.summary.clone(),
            })
            .collect();

        // 11. Build votes map
        let votes: BTreeMap<AgentName, Verdict> =
            agents.iter().map(|a| (a.agent, a.verdict)).collect();

        // 12. Build majority summary
        let majority_summary = agents
            .iter()
            .filter(|a| a.effective_verdict() == majority_verdict)
            .map(|a| format!("{}: {}", a.agent.display_name(), a.summary))
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
            (
                format!("GO WITH CAVEATS ({}-{})", approve_count, reject_count),
                Verdict::Approve,
            )
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

    /// Deduplicates findings across agents using a two-tier keying strategy.
    ///
    /// **Located findings** (non-empty `file` and `line > 0`) are keyed by a
    /// stable 16-hex-char SHA-256 id (`generate_finding_id`), which is
    /// title-independent and stable across runs even when the LLM rewords the
    /// finding title. **Unlocated findings** fall back to NFKC + Unicode
    /// casefold on the title (existing behavior).
    ///
    /// A `"id:"` / `"title:"` namespace prefix in the internal key ensures a
    /// located finding and an unlocated finding with the same title can never
    /// collide into the same group.
    ///
    /// Ordering contract: findings appear in first-seen order (by agent slice
    /// position), then sorted by severity DESC (Critical → Info). Within equal
    /// severity, first-seen order is preserved (stable sort).
    fn deduplicate_findings(&self, agents: &[AgentOutput]) -> Vec<DedupFinding> {
        /// Accumulates state for a group of findings that share the same dedup key.
        struct GroupState {
            /// Severity promoted to the highest seen across all matching findings.
            severity: Severity,
            /// Title from the first-seen finding (insertion order preserved).
            title: String,
            /// Detail from the highest-severity finding.
            detail: String,
            /// Agents that contributed a matching finding, in first-seen order.
            sources: Vec<AgentName>,
            /// File path from the first-seen finding.
            file: Option<String>,
            /// Line number from the first-seen finding.
            line: Option<u32>,
            /// Category from the first-seen finding.
            category: Category,
            /// Stable id (Some for located, None for title-keyed).
            id: Option<String>,
        }

        // Intentional O(m²) — preserves insertion order without adding indexmap.
        // m is bounded by `ValidationLimits::max_findings × agent_count`. At the
        // default max_findings (100) × 3 agents = 300, this is ~90k string
        // comparisons on short strings, <1ms in practice. Consider switching to
        // `indexmap` if max_findings is configured above 500.
        let mut groups: Vec<(String, GroupState)> = Vec::new();

        for agent in agents {
            for finding in &agent.findings {
                let (key_str, id) = match finding_key(finding) {
                    DedupKey::Id(h) => (format!("id:{h}"), Some(h)),
                    DedupKey::Title(t) => (format!("title:{t}"), None),
                };
                if let Some((_, state)) = groups.iter_mut().find(|(k, _)| k == &key_str) {
                    // Promote severity and update detail if this finding is higher
                    if finding.severity > state.severity {
                        state.severity = finding.severity;
                        state.detail = finding.detail.clone();
                    }
                    state.sources.push(agent.agent);
                } else {
                    groups.push((
                        key_str,
                        GroupState {
                            severity: finding.severity,
                            title: finding.title.clone(),
                            detail: finding.detail.clone(),
                            sources: vec![agent.agent],
                            file: finding.file.clone(),
                            line: finding.line,
                            category: finding.category,
                            id,
                        },
                    ));
                }
            }
        }

        let mut result: Vec<DedupFinding> = groups
            .into_iter()
            .map(|(_, state)| DedupFinding {
                severity: state.severity,
                title: state.title,
                detail: state.detail,
                sources: state.sources,
                file: state.file,
                line: state.line,
                category: state.category,
                id: state.id,
            })
            .collect();

        // Sort by severity descending (Critical first); stable to preserve first-seen
        // order within equal severity groups.
        result.sort_by_key(|f| std::cmp::Reverse(f.severity));
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

    /// Approve + conditional + reject produces GO WITH CAVEATS (2-1).
    #[test]
    fn test_approve_conditional_reject_produces_go_with_caveats() {
        let agents = vec![
            make_output(AgentName::Melchior, Verdict::Approve, 0.9),
            make_output(AgentName::Balthasar, Verdict::Conditional, 0.8),
            make_output(AgentName::Caspar, Verdict::Reject, 0.7),
        ];
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&agents).unwrap();
        assert_eq!(result.consensus, "GO WITH CAVEATS (2-1)");
        assert_eq!(result.consensus_verdict, Verdict::Approve);
        assert!(!result.conditions.is_empty());
        assert_eq!(result.conditions[0].agent, AgentName::Balthasar);
    }

    // -- S05: GO WITH CAVEATS includes split count --

    /// Three conditional agents produce GO WITH CAVEATS (3-0) (unanimous go side).
    #[test]
    fn test_go_with_caveats_three_conditionals_unanimous() {
        let agents = vec![
            make_output(AgentName::Melchior, Verdict::Conditional, 0.9),
            make_output(AgentName::Balthasar, Verdict::Conditional, 0.8),
            make_output(AgentName::Caspar, Verdict::Conditional, 0.7),
        ];
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&agents).unwrap();
        assert_eq!(result.consensus, "GO WITH CAVEATS (3-0)");
        assert_eq!(result.consensus_verdict, Verdict::Approve);
    }

    /// Two conditionals + one approve produce GO WITH CAVEATS (3-0).
    #[test]
    fn test_go_with_caveats_two_conditionals_one_approve() {
        let agents = vec![
            make_output(AgentName::Melchior, Verdict::Conditional, 0.9),
            make_output(AgentName::Balthasar, Verdict::Conditional, 0.8),
            make_output(AgentName::Caspar, Verdict::Approve, 0.7),
        ];
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&agents).unwrap();
        assert_eq!(result.consensus, "GO WITH CAVEATS (3-0)");
        assert_eq!(result.consensus_verdict, Verdict::Approve);
    }

    /// Two conditionals + one reject.
    /// score = (0.5 + 0.5 - 1.0) / 3 = 0.0 → HOLD -- TIE (score is exactly zero).
    /// Reject pulls score to zero despite conditional majority on effective-verdict side.
    #[test]
    fn test_go_with_caveats_two_conditionals_one_reject() {
        let agents = vec![
            make_output(AgentName::Melchior, Verdict::Conditional, 0.9),
            make_output(AgentName::Balthasar, Verdict::Conditional, 0.8),
            make_output(AgentName::Caspar, Verdict::Reject, 0.7),
        ];
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&agents).unwrap();
        // score = (0.5 + 0.5 + (-1.0)) / 3 = 0.0 → HOLD -- TIE
        assert_eq!(result.consensus, "HOLD -- TIE");
        assert_eq!(result.consensus_verdict, Verdict::Reject);
    }

    /// Two conditionals (degraded, 2 agents) produce GO WITH CAVEATS (2-0).
    /// Degraded mode does NOT alter GO WITH CAVEATS — only caps STRONG labels.
    #[test]
    fn test_go_with_caveats_degraded_two_conditionals() {
        let agents = vec![
            make_output(AgentName::Melchior, Verdict::Conditional, 0.9),
            make_output(AgentName::Balthasar, Verdict::Conditional, 0.8),
        ];
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&agents).unwrap();
        assert_eq!(result.consensus, "GO WITH CAVEATS (2-0)");
        assert_eq!(result.consensus_verdict, Verdict::Approve);
        assert_eq!(result.agent_count, 2);
    }

    /// One conditional + one approve (degraded) produce GO WITH CAVEATS (2-0).
    #[test]
    fn test_go_with_caveats_degraded_one_conditional_one_approve() {
        let agents = vec![
            make_output(AgentName::Melchior, Verdict::Conditional, 0.9),
            make_output(AgentName::Balthasar, Verdict::Approve, 0.8),
        ];
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&agents).unwrap();
        assert_eq!(result.consensus, "GO WITH CAVEATS (2-0)");
        assert_eq!(result.consensus_verdict, Verdict::Approve);
        assert_eq!(result.agent_count, 2);
    }

    /// One conditional + one reject (degraded) produce HOLD (1-1).
    /// Score = (0.5 + -1.0) / 2 = -0.25 → negative → HOLD (1-1), not a tie.
    #[test]
    fn test_degraded_one_conditional_one_reject_produces_hold_1_1() {
        let agents = vec![
            make_output(AgentName::Melchior, Verdict::Conditional, 0.9),
            make_output(AgentName::Balthasar, Verdict::Reject, 0.8),
        ];
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&agents).unwrap();
        // score = (0.5 - 1.0) / 2 = -0.25, negative, so HOLD side wins
        // approve_count=1 (Conditional maps to Approve), reject_count=1
        // HOLD label uses (reject_count-approve_count) = (1-1)
        assert_eq!(result.consensus, "HOLD (1-1)");
        assert_eq!(result.consensus_verdict, Verdict::Reject);
    }

    /// Boundary test: score just above epsilon classifies as GO WITH CAVEATS.
    ///
    /// Uses custom epsilon (0.2) to straddle the real score of
    /// Approve(+1) + Conditional(+0.5) + Reject(-1) = 0.5/3 ≈ 0.1667.
    /// With epsilon=0.1, score 0.1667 > epsilon → GO WITH CAVEATS.
    #[test]
    fn test_score_just_above_epsilon_classifies_as_go_with_caveats() {
        // score = (1.0 + 0.5 - 1.0) / 3 ≈ 0.1667
        // epsilon = 0.1 → score > epsilon → GO WITH CAVEATS (2-1)
        let agents = vec![
            make_output(AgentName::Melchior, Verdict::Approve, 0.9),
            make_output(AgentName::Balthasar, Verdict::Conditional, 0.8),
            make_output(AgentName::Caspar, Verdict::Reject, 0.7),
        ];
        let engine = ConsensusEngine::new(ConsensusConfig {
            epsilon: 0.1,
            ..ConsensusConfig::default()
        });
        let result = engine.determine(&agents).unwrap();
        assert_eq!(result.consensus, "GO WITH CAVEATS (2-1)");
        assert_eq!(result.consensus_verdict, Verdict::Approve);
    }

    /// Boundary test: score just below epsilon classifies as HOLD.
    ///
    /// Uses custom epsilon (0.2) to straddle the real score of
    /// Approve(+1) + Conditional(+0.5) + Reject(-1) = 0.5/3 ≈ 0.1667.
    /// With epsilon=0.2, score 0.1667 < epsilon → HOLD -- TIE.
    #[test]
    fn test_score_just_below_epsilon_classifies_as_hold() {
        // score = (1.0 + 0.5 - 1.0) / 3 ≈ 0.1667
        // epsilon = 0.2 → score.abs() < epsilon → HOLD -- TIE
        let agents = vec![
            make_output(AgentName::Melchior, Verdict::Approve, 0.9),
            make_output(AgentName::Balthasar, Verdict::Conditional, 0.8),
            make_output(AgentName::Caspar, Verdict::Reject, 0.7),
        ];
        let engine = ConsensusEngine::new(ConsensusConfig {
            epsilon: 0.2,
            ..ConsensusConfig::default()
        });
        let result = engine.determine(&agents).unwrap();
        assert_eq!(result.consensus, "HOLD -- TIE");
        assert_eq!(result.consensus_verdict, Verdict::Reject);
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
        m.findings.push(Finding::new(
            Severity::Warning,
            "Security Issue",
            "detail_warning",
        ));
        let mut b = make_output(AgentName::Balthasar, Verdict::Approve, 0.9);
        b.findings.push(Finding::new(
            Severity::Critical,
            "security issue",
            "detail_critical",
        ));
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
        m.findings.push(Finding::new(
            Severity::Warning,
            "Security Issue",
            "detail_m",
        ));
        let mut b = make_output(AgentName::Balthasar, Verdict::Approve, 0.9);
        b.findings.push(Finding::new(
            Severity::Critical,
            "security issue",
            "detail_b",
        ));
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
        m.findings
            .push(Finding::new(Severity::Warning, "Issue", "detail_warning"));
        let mut b = make_output(AgentName::Balthasar, Verdict::Approve, 0.9);
        b.findings
            .push(Finding::new(Severity::Critical, "issue", "detail_critical"));
        let c = make_output(AgentName::Caspar, Verdict::Approve, 0.9);
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&[m, b, c]).unwrap();
        assert_eq!(result.findings[0].detail, "detail_critical");
    }

    /// On same severity, detail comes from the first-seen agent (slice order).
    #[test]
    fn test_merged_finding_detail_from_first_agent_on_same_severity() {
        let mut b = make_output(AgentName::Balthasar, Verdict::Approve, 0.9);
        b.findings
            .push(Finding::new(Severity::Warning, "Issue", "detail_b"));
        let mut m = make_output(AgentName::Melchior, Verdict::Approve, 0.9);
        m.findings
            .push(Finding::new(Severity::Warning, "issue", "detail_m"));
        let c = make_output(AgentName::Caspar, Verdict::Approve, 0.9);
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&[b, m, c]).unwrap();
        // Balthasar is first in the slice → first seen → Balthasar's detail preserved
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
        assert!(
            result
                .majority_summary
                .contains("Melchior: Melchior summary")
        );
        assert!(
            result
                .majority_summary
                .contains("Balthasar: Balthasar summary")
        );
        assert!(result.majority_summary.contains(" | "));
        assert!(!result.majority_summary.contains("Caspar summary"));
    }

    /// Majority summary uses agent display name capitalized (not lowercase).
    #[test]
    fn test_majority_summary_uses_display_name_capitalized() {
        let agents = vec![
            make_output(AgentName::Melchior, Verdict::Approve, 0.9),
            make_output(AgentName::Balthasar, Verdict::Approve, 0.8),
        ];
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&agents).unwrap();
        assert!(result.majority_summary.contains("Melchior:"));
        assert!(result.majority_summary.contains("Balthasar:"));
        // Ensure NOT lowercase
        assert!(!result.majority_summary.contains("melchior:"));
        assert!(!result.majority_summary.contains("balthasar:"));
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
        assert_eq!(result.conditions[0].condition, "Balthasar summary");
    }

    /// Conditions use summary field, not recommendation field.
    #[test]
    fn test_conditions_use_summary_field_not_recommendation_field() {
        let mut agent = make_output(AgentName::Melchior, Verdict::Conditional, 0.9);
        agent.summary = "Melchior condition summary".to_string();
        agent.recommendation = "Melchior detailed recommendation".to_string();
        let support = make_output(AgentName::Balthasar, Verdict::Approve, 0.8);
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&[agent, support]).unwrap();
        assert_eq!(result.conditions.len(), 1);
        assert_eq!(result.conditions[0].condition, "Melchior condition summary");
        assert_ne!(
            result.conditions[0].condition,
            "Melchior detailed recommendation"
        );
    }

    /// Conditions are distinct from recommendations section.
    #[test]
    fn test_conditions_are_distinct_from_recommendations_section() {
        let mut agent = make_output(AgentName::Balthasar, Verdict::Conditional, 0.85);
        agent.summary = "Short condition summary".to_string();
        agent.recommendation = "Long detailed recommendation text".to_string();
        let support = make_output(AgentName::Melchior, Verdict::Approve, 0.9);
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&[agent, support]).unwrap();
        // Conditions should be sourced from summary
        assert_eq!(result.conditions.len(), 1);
        assert_eq!(result.conditions[0].condition, "Short condition summary");
        // Recommendations should contain the recommendation field
        assert!(result.recommendations.contains_key(&AgentName::Balthasar));
        assert_eq!(
            result.recommendations[&AgentName::Balthasar],
            "Long detailed recommendation text"
        );
        // They must be distinct
        assert_ne!(
            result.conditions[0].condition,
            result.recommendations[&AgentName::Balthasar]
        );
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
        m.findings
            .push(Finding::new(Severity::Info, "Info issue", "info detail"));
        m.findings.push(Finding::new(
            Severity::Critical,
            "Critical issue",
            "critical detail",
        ));
        let b = make_output(AgentName::Balthasar, Verdict::Approve, 0.9);
        let c = make_output(AgentName::Caspar, Verdict::Approve, 0.9);
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&[m, b, c]).unwrap();
        assert_eq!(result.findings.len(), 2);
        assert_eq!(result.findings[0].severity, Severity::Critical);
        assert_eq!(result.findings[1].severity, Severity::Info);
    }

    /// Tab-separated and space-separated titles merge (clean_title replaces tab with space);
    /// but double-space interior is preserved — "SQL  injection" is a DISTINCT finding
    /// from "SQL injection". Aligned with Python dedup_key behavior (no split_whitespace).
    #[test]
    fn test_dedup_tab_normalizes_to_space_but_double_space_is_distinct() {
        // "SQL\tinjection" → clean_title → "SQL injection" → same key as "sql injection"
        // "SQL  injection" → clean_title → "SQL  injection" → different key (double space)
        let mut m = make_output(AgentName::Melchior, Verdict::Approve, 0.9);
        m.findings.push(Finding::new(
            Severity::Warning,
            "SQL  injection",
            "detail_m",
        )); // double space — distinct key
        let mut b = make_output(AgentName::Balthasar, Verdict::Approve, 0.9);
        b.findings.push(Finding::new(
            Severity::Warning,
            "SQL\tinjection",
            "detail_b",
        )); // tab → space → merges with "sql injection"
        let mut c = make_output(AgentName::Caspar, Verdict::Approve, 0.9);
        c.findings.push(Finding::new(
            Severity::Critical,
            "sql injection",
            "detail_c",
        ));
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&[m, b, c]).unwrap();
        // "SQL\tinjection" and "sql injection" share the same key → merge into 1
        // "SQL  injection" has a distinct key → separate finding
        // Total: 2 findings
        assert_eq!(
            result.findings.len(),
            2,
            "tab-normalized title merges with single-space; double-space is distinct"
        );
        // Critical finding (merged tab+space group) sorts first
        assert_eq!(result.findings[0].severity, Severity::Critical);
        assert_eq!(result.findings[0].sources.len(), 2);
        // Warning finding (double-space group) is separate
        assert_eq!(result.findings[1].severity, Severity::Warning);
        assert_eq!(result.findings[1].sources.len(), 1);
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

    // -- S03: dedup_key NFKC + casefold tests --

    /// dedup_key applies NFKC normalization: fullwidth Latin chars collapse to ASCII.
    /// "ＡＢＣ" (fullwidth) and "abc" must produce the same key after NFKC + casefold.
    #[test]
    fn test_dedup_key_nfkc_collapses_fullwidth_latin() {
        let key_fullwidth = dedup_key("\u{FF21}\u{FF22}\u{FF23}"); // ＡＢＣ
        let key_ascii = dedup_key("abc");
        assert_eq!(
            key_fullwidth, key_ascii,
            "NFKC must collapse fullwidth ＡＢＣ to abc"
        );
    }

    /// dedup_key applies NFKC: precomposed and combining forms of the same character
    /// produce the same key ("café" U+00E9 == "cafe\u{301}").
    #[test]
    fn test_dedup_key_nfkc_collapses_combining_accents() {
        let precomposed = dedup_key("caf\u{E9}"); // é precomposed
        let combining = dedup_key("cafe\u{301}"); // e + combining acute
        assert_eq!(
            precomposed, combining,
            "NFKC must collapse combining accents to precomposed form"
        );
    }

    /// dedup_key uses full Unicode casefold: sharp-S ß must fold to "ss" (not "ß").
    /// Python str.casefold() and caseless crate both produce "ss". No #[ignore] needed.
    #[test]
    fn test_dedup_key_casefold_sharp_s_equals_double_s() {
        let sharp_s = dedup_key("\u{DF}"); // ß
        let double_s = dedup_key("ss");
        assert_eq!(
            sharp_s, double_s,
            "casefold must fold ß to ss (full Unicode fold, not to_lowercase)"
        );
    }

    /// dedup_key casefolding: all Greek sigma variants (Σ, σ, ς) fold to the same key.
    ///
    /// Empirically verified (S03 Step 1):
    ///   Python: "ς".casefold() == "σ" (U+03C3)
    ///   caseless::default_case_fold_str("ς") == "σ" (U+03C3)
    /// Both agree, so all three variants are included in this test.
    #[test]
    fn test_dedup_key_casefold_greek_sigma_variants() {
        let capital = dedup_key("\u{03A3}"); // Σ GREEK CAPITAL LETTER SIGMA
        let small = dedup_key("\u{03C3}"); // σ GREEK SMALL LETTER SIGMA
        let final_s = dedup_key("\u{03C2}"); // ς GREEK SMALL LETTER FINAL SIGMA
        assert_eq!(capital, small, "Σ and σ must fold to the same key");
        assert_eq!(
            small, final_s,
            "σ and ς must fold to the same key (both caseless and Python agree)"
        );
    }

    /// dedup_key does NOT apply locale-aware Turkish dotted-I folding.
    /// Unicode default casefold: "İ" (U+0130) folds to "i\u{307}" (i + combining dot above).
    /// This test confirms default (non-locale) behavior, matching caseless crate semantics.
    #[test]
    fn test_dedup_key_casefold_turkish_dotted_i() {
        // U+0130 (LATIN CAPITAL LETTER I WITH DOT ABOVE) under default (non-locale) casefold
        // maps to 'i' (U+0069) + combining dot above (U+0307). This matches Python's casefold
        // behavior (default, not Turkish locale).
        let input = "\u{0130}";
        assert_eq!(dedup_key(input), "i\u{307}");
    }

    /// dedup_key preserves interior whitespace — aligning with Python clean_title + NFKC behavior.
    /// "foo  bar" (double space) and "foo bar" (single space) produce DIFFERENT keys.
    /// This is the correct Python-aligned behavior: no split_whitespace collapsing.
    #[test]
    fn test_dedup_key_preserves_interior_whitespace() {
        let double_space = dedup_key("foo  bar");
        let single_space = dedup_key("foo bar");
        assert_ne!(
            double_space, single_space,
            "dedup_key must NOT collapse interior whitespace (aligned with Python)"
        );
    }

    /// Findings with fullwidth ASCII titles (e.g., "ＳＱＬ injection") and ASCII titles
    /// ("SQL injection") are merged into a single deduplicated finding via NFKC.
    #[test]
    fn test_dedup_merges_fullwidth_and_ascii_titles() {
        // ＳＱＬ = U+FF33 U+FF31 U+FF2C (fullwidth)
        let mut m = make_output(AgentName::Melchior, Verdict::Approve, 0.9);
        m.findings.push(Finding::new(
            Severity::Warning,
            "\u{FF33}\u{FF31}\u{FF2C} injection", // ＳＱＬ injection
            "detail_fullwidth",
        ));
        let mut b = make_output(AgentName::Balthasar, Verdict::Approve, 0.9);
        b.findings.push(Finding::new(
            Severity::Critical,
            "sql injection",
            "detail_ascii",
        ));
        let c = make_output(AgentName::Caspar, Verdict::Approve, 0.9);
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        let result = engine.determine(&[m, b, c]).unwrap();
        assert_eq!(
            result.findings.len(),
            1,
            "fullwidth and ASCII titles must merge to one finding via NFKC"
        );
        assert_eq!(result.findings[0].severity, Severity::Critical);
    }

    // -- S03: ordering regression tests --

    /// When Melchior reports first in the agent slice, the deduplicated finding's
    /// title comes from Melchior's form and sources list is [Melchior, Balthasar].
    #[test]
    fn test_dedup_first_seen_order_preserved_when_melchior_reports_first() {
        let mut m = make_output(AgentName::Melchior, Verdict::Approve, 0.9);
        m.findings
            .push(Finding::new(Severity::Warning, "Issue A", "detail_m"));
        let mut b = make_output(AgentName::Balthasar, Verdict::Approve, 0.9);
        b.findings
            .push(Finding::new(Severity::Warning, "issue a", "detail_b"));
        let c = make_output(AgentName::Caspar, Verdict::Approve, 0.9);
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        // Melchior is first in slice
        let result = engine.determine(&[m, b, c]).unwrap();
        assert_eq!(result.findings.len(), 1);
        assert_eq!(
            result.findings[0].title, "Issue A",
            "title must come from Melchior (first seen)"
        );
        assert_eq!(result.findings[0].sources.len(), 2);
        // First source should be Melchior (first-seen insertion order)
        assert_eq!(result.findings[0].sources[0], AgentName::Melchior);
        assert_eq!(result.findings[0].sources[1], AgentName::Balthasar);
    }

    /// When Balthasar reports first in the agent slice, the deduplicated finding's
    /// title comes from Balthasar's form and sources list is [Balthasar, Melchior].
    #[test]
    fn test_dedup_first_seen_order_preserved_when_balthasar_reports_first() {
        let mut b = make_output(AgentName::Balthasar, Verdict::Approve, 0.9);
        b.findings
            .push(Finding::new(Severity::Warning, "issue a", "detail_b"));
        let mut m = make_output(AgentName::Melchior, Verdict::Approve, 0.9);
        m.findings
            .push(Finding::new(Severity::Warning, "Issue A", "detail_m"));
        let c = make_output(AgentName::Caspar, Verdict::Approve, 0.9);
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        // Balthasar is first in slice
        let result = engine.determine(&[b, m, c]).unwrap();
        assert_eq!(result.findings.len(), 1);
        assert_eq!(
            result.findings[0].title, "issue a",
            "title must come from Balthasar (first seen)"
        );
        assert_eq!(result.findings[0].sources.len(), 2);
        assert_eq!(result.findings[0].sources[0], AgentName::Balthasar);
        assert_eq!(result.findings[0].sources[1], AgentName::Melchior);
    }

    /// When two distinct findings have equal severity, the finding seen first in the
    /// agent slice appears first in the output (stable ordering).
    #[test]
    fn test_dedup_ordering_stable_across_equal_severity() {
        let mut m = make_output(AgentName::Melchior, Verdict::Approve, 0.9);
        m.findings.push(Finding::new(
            Severity::Warning,
            "Alpha Issue",
            "detail_alpha",
        ));
        let mut b = make_output(AgentName::Balthasar, Verdict::Approve, 0.9);
        b.findings
            .push(Finding::new(Severity::Warning, "Beta Issue", "detail_beta"));
        let c = make_output(AgentName::Caspar, Verdict::Approve, 0.9);
        let engine = ConsensusEngine::new(ConsensusConfig::default());
        // Melchior is first, so "Alpha Issue" should appear first when severity ties
        let result = engine.determine(&[m, b, c]).unwrap();
        assert_eq!(result.findings.len(), 2);
        assert_eq!(
            result.findings[0].title, "Alpha Issue",
            "first-seen finding must appear first when severity is equal"
        );
        assert_eq!(result.findings[1].title, "Beta Issue");
    }

    // -- T5: stable id deduplication --

    /// Two colocated findings (same file+line+category, different title/agent) merge by
    /// stable id; severity is promoted; golden id matches Python cross-language vector.
    #[test]
    fn test_dedup_merges_colocated_by_id_across_agents() {
        let mut m = make_output(AgentName::Melchior, Verdict::Approve, 0.9);
        let mut b = make_output(AgentName::Balthasar, Verdict::Reject, 0.9);
        m.findings.push(
            Finding::new(Severity::Warning, "Off-by-one", "d")
                .with_location("src/x.rs", 42)
                .with_category(Category::LogicError),
        );
        b.findings.push(
            Finding::new(Severity::Critical, "Indice fuera de rango", "d2")
                .with_location("src/x.rs", 42)
                .with_category(Category::LogicError),
        );
        let out = ConsensusEngine::default().deduplicate_findings(&[m, b]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].severity, Severity::Critical);
        assert_eq!(
            out[0].sources,
            vec![AgentName::Melchior, AgentName::Balthasar]
        );
        assert_eq!(out[0].id.as_deref(), Some("7fb2a28931164f30")); // golden parity
    }

    /// Unlocated findings (no file/line) still deduplicate by normalized title.
    #[test]
    fn test_dedup_falls_back_to_title_when_unlocated() {
        let mut m = make_output(AgentName::Melchior, Verdict::Approve, 0.9);
        let mut b = make_output(AgentName::Balthasar, Verdict::Approve, 0.9);
        m.findings
            .push(Finding::new(Severity::Warning, "Same Title", "d"));
        b.findings
            .push(Finding::new(Severity::Warning, "same title", "d"));
        let out = ConsensusEngine::default().deduplicate_findings(&[m, b]);
        assert_eq!(out.len(), 1);
        assert!(out[0].id.is_none());
    }

    /// Same location but different category → different stable ids → two findings.
    #[test]
    fn test_dedup_different_category_same_location_does_not_merge() {
        let mut m = make_output(AgentName::Melchior, Verdict::Approve, 0.9);
        m.findings.push(
            Finding::new(Severity::Warning, "a", "d")
                .with_location("src/x.rs", 42)
                .with_category(Category::LogicError),
        );
        m.findings.push(
            Finding::new(Severity::Warning, "b", "d")
                .with_location("src/x.rs", 42)
                .with_category(Category::Injection),
        );
        assert_eq!(
            ConsensusEngine::default().deduplicate_findings(&[m]).len(),
            2
        );
    }

    /// A located finding and an unlocated finding with the same title do NOT collide:
    /// they use different dedup keys (id: vs title:).
    #[test]
    fn test_dedup_located_and_unlocated_same_title_do_not_collide() {
        let mut m = make_output(AgentName::Melchior, Verdict::Approve, 0.9);
        m.findings
            .push(Finding::new(Severity::Warning, "T", "d").with_location("x", 1));
        m.findings.push(Finding::new(Severity::Warning, "T", "d"));
        assert_eq!(
            ConsensusEngine::default().deduplicate_findings(&[m]).len(),
            2
        );
    }

    #[test]
    fn test_dedup_treats_zero_line_as_unlocated() {
        // line 0 is invalid (1-based). `with_location` does not validate, so
        // `finding_key` must treat a zero line as unlocated (no stable id),
        // consistent with `de_opt_line` which maps non-positive lines to None.
        let mut m = make_output(AgentName::Melchior, Verdict::Approve, 0.9);
        m.findings
            .push(Finding::new(Severity::Warning, "t", "d").with_location("src/x.rs", 0));
        let out = ConsensusEngine::default().deduplicate_findings(&[m]);
        assert_eq!(out.len(), 1);
        assert!(out[0].id.is_none(), "line 0 must be treated as unlocated");
    }
}
