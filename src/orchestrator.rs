// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::agent::{Agent, AgentFactory};
use crate::consensus::{ConsensusConfig, ConsensusEngine};
use crate::error::{MagiError, ProviderError};
use crate::provider::{CompletionConfig, LlmProvider};
use crate::reporting::{MagiReport, ReportConfig, ReportFormatter};
use crate::schema::{AgentName, AgentOutput, Mode};
use crate::validate::{ValidationLimits, Validator};

/// Configuration for the MAGI orchestrator.
///
/// Controls timeout per agent, maximum input size, and LLM completion parameters.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct MagiConfig {
    /// Maximum time to wait for each agent (default: 300 seconds).
    pub timeout: Duration,
    /// Maximum content size in bytes (default: 1_048_576 = 1MB).
    pub max_input_len: usize,
    /// Completion parameters forwarded to each agent.
    pub completion: CompletionConfig,
}

impl Default for MagiConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(300),
            max_input_len: 1_048_576,
            completion: CompletionConfig::default(),
        }
    }
}

/// Consuming builder for constructing [`Magi`] instances.
///
/// The only required field is `default_provider`, passed to the constructor.
/// All other fields have sensible defaults.
///
/// # Examples
///
/// ```no_run
/// # use std::sync::Arc;
/// # use std::time::Duration;
/// # use magi_core::orchestrator::MagiBuilder;
/// # use magi_core::schema::AgentName;
/// // let magi = MagiBuilder::new(provider)
/// //     .with_provider(AgentName::Caspar, caspar_provider)
/// //     .with_timeout(Duration::from_secs(60))
/// //     .build()
/// //     .expect("build");
/// ```
pub struct MagiBuilder {
    default_provider: Arc<dyn LlmProvider>,
    agent_providers: BTreeMap<AgentName, Arc<dyn LlmProvider>>,
    custom_prompts: BTreeMap<AgentName, String>,
    prompts_dir: Option<PathBuf>,
    config: MagiConfig,
    validation_limits: ValidationLimits,
    consensus_config: ConsensusConfig,
    report_config: ReportConfig,
}

impl MagiBuilder {
    /// Creates a new builder with the given default provider.
    ///
    /// # Parameters
    /// - `default_provider`: The LLM provider shared by all agents unless overridden.
    pub fn new(default_provider: Arc<dyn LlmProvider>) -> Self {
        Self {
            default_provider,
            agent_providers: BTreeMap::new(),
            custom_prompts: BTreeMap::new(),
            prompts_dir: None,
            config: MagiConfig::default(),
            validation_limits: ValidationLimits::default(),
            consensus_config: ConsensusConfig::default(),
            report_config: ReportConfig::default(),
        }
    }

    /// Sets a per-agent provider override.
    ///
    /// # Parameters
    /// - `name`: Which agent to override.
    /// - `provider`: The provider for that agent.
    pub fn with_provider(mut self, name: AgentName, provider: Arc<dyn LlmProvider>) -> Self {
        self.agent_providers.insert(name, provider);
        self
    }

    /// Sets a custom system prompt for a specific agent.
    ///
    /// # Parameters
    /// - `name`: Which agent to override.
    /// - `prompt`: The custom system prompt.
    pub fn with_custom_prompt(mut self, name: AgentName, prompt: String) -> Self {
        self.custom_prompts.insert(name, prompt);
        self
    }

    /// Sets a directory from which to load custom prompt files.
    ///
    /// # Parameters
    /// - `dir`: Path to the prompts directory.
    pub fn with_prompts_dir(mut self, dir: PathBuf) -> Self {
        self.prompts_dir = Some(dir);
        self
    }

    /// Sets the per-agent timeout.
    ///
    /// # Parameters
    /// - `timeout`: Maximum wait time per agent.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.config.timeout = timeout;
        self
    }

    /// Sets the maximum input content length in bytes.
    ///
    /// # Parameters
    /// - `max`: Maximum content size.
    pub fn with_max_input_len(mut self, max: usize) -> Self {
        self.config.max_input_len = max;
        self
    }

    /// Sets the completion configuration forwarded to agents.
    ///
    /// # Parameters
    /// - `config`: Completion parameters (max_tokens, temperature).
    pub fn with_completion_config(mut self, config: CompletionConfig) -> Self {
        self.config.completion = config;
        self
    }

    /// Sets custom validation limits.
    ///
    /// # Parameters
    /// - `limits`: Validation thresholds for agent outputs.
    pub fn with_validation_limits(mut self, limits: ValidationLimits) -> Self {
        self.validation_limits = limits;
        self
    }

    /// Sets custom consensus engine configuration.
    ///
    /// # Parameters
    /// - `config`: Consensus parameters (min_agents, epsilon).
    pub fn with_consensus_config(mut self, config: ConsensusConfig) -> Self {
        self.consensus_config = config;
        self
    }

    /// Sets custom report formatter configuration.
    ///
    /// # Parameters
    /// - `config`: Report parameters (banner_width, agent_titles).
    pub fn with_report_config(mut self, config: ReportConfig) -> Self {
        self.report_config = config;
        self
    }

    /// Builds the [`Magi`] orchestrator from accumulated configuration.
    ///
    /// Loads prompts from `prompts_dir` if set (may fail with `MagiError::Io`).
    ///
    /// # Errors
    /// Returns `MagiError::Io` if `prompts_dir` is set and cannot be read.
    pub fn build(self) -> Result<Magi, MagiError> {
        let mut factory = AgentFactory::new(self.default_provider);
        for (name, provider) in self.agent_providers {
            factory = factory.with_provider(name, provider);
        }
        for (name, prompt) in self.custom_prompts {
            factory = factory.with_custom_prompt(name, prompt);
        }
        if let Some(dir) = self.prompts_dir {
            factory = factory.from_directory(&dir)?;
        }

        Ok(Magi {
            config: self.config,
            agent_factory: factory,
            validator: Validator::with_limits(self.validation_limits),
            consensus_engine: ConsensusEngine::new(self.consensus_config),
            formatter: ReportFormatter::with_config(self.report_config),
        })
    }
}

/// Main entry point for the MAGI multi-perspective analysis system.
///
/// Composes agents, validation, consensus, and reporting into a single
/// orchestration flow. The [`analyze`](Magi::analyze) method launches three
/// agents in parallel, parses and validates their responses, computes consensus,
/// and generates a formatted report.
///
/// # Examples
///
/// ```no_run
/// # use std::sync::Arc;
/// # use magi_core::orchestrator::Magi;
/// # use magi_core::schema::Mode;
/// // let magi = Magi::new(provider);
/// // let report = magi.analyze(&Mode::CodeReview, content).await?;
/// ```
pub struct Magi {
    config: MagiConfig,
    agent_factory: AgentFactory,
    validator: Validator,
    consensus_engine: ConsensusEngine,
    formatter: ReportFormatter,
}

impl Magi {
    /// Creates a MAGI orchestrator with a single provider and all defaults.
    ///
    /// Equivalent to `MagiBuilder::new(provider).build().unwrap()`.
    /// This cannot fail because all defaults are valid.
    ///
    /// # Parameters
    /// - `provider`: The LLM provider shared by all three agents.
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        // Safe to unwrap: no prompts_dir means no I/O, so build cannot fail.
        MagiBuilder::new(provider).build().expect(
            "Magi::new uses all defaults and cannot fail; \
             this is an internal invariant violation",
        )
    }

    /// Returns a builder for configuring a MAGI orchestrator.
    ///
    /// # Parameters
    /// - `provider`: The default LLM provider.
    pub fn builder(provider: Arc<dyn LlmProvider>) -> MagiBuilder {
        MagiBuilder::new(provider)
    }

    /// Runs a full multi-perspective analysis.
    ///
    /// Launches three agents in parallel, parses their JSON responses,
    /// validates outputs, computes consensus, and generates a formatted report.
    ///
    /// # Parameters
    /// - `mode`: The analysis mode (CodeReview, Design, Analysis).
    /// - `content`: The content to analyze.
    ///
    /// # Errors
    /// - [`MagiError::InputTooLarge`] if `content.len()` exceeds `max_input_len`.
    /// - [`MagiError::InsufficientAgents`] if fewer than 2 agents succeed.
    pub async fn analyze(&self, mode: &Mode, content: &str) -> Result<MagiReport, MagiError> {
        // 1. Input validation
        if content.len() > self.config.max_input_len {
            return Err(MagiError::InputTooLarge {
                size: content.len(),
                max: self.config.max_input_len,
            });
        }

        // 2. Create agents
        let agents = self.agent_factory.create_agents(*mode);

        // 3. Build user prompt
        let prompt = build_prompt(mode, content);

        // 4. Launch agents in parallel and collect results
        let agent_results = self.launch_agents(agents, &prompt).await;

        // 5. Process results: parse, validate, separate successes/failures
        let (successful, failed_agents) = self.process_results(agent_results)?;

        // 6. Consensus
        let consensus = self.consensus_engine.determine(&successful)?;

        // 7. Report
        let banner = self.formatter.format_banner(&successful, &consensus);
        let report = self.formatter.format_report(&successful, &consensus);

        // 8. Build MagiReport
        let degraded = successful.len() < 3;
        Ok(MagiReport {
            agents: successful,
            consensus,
            banner,
            report,
            degraded,
            failed_agents,
        })
    }

    /// Launches all agents in parallel using `JoinSet` for cancellation safety.
    ///
    /// Each agent task is wrapped in `tokio::time::timeout`. Dropping the
    /// `JoinSet` automatically aborts all spawned tasks, preventing wasted
    /// LLM API quota.
    async fn launch_agents(
        &self,
        agents: Vec<Agent>,
        prompt: &str,
    ) -> Vec<(AgentName, Result<String, MagiError>)> {
        let mut join_set = tokio::task::JoinSet::new();
        let timeout = self.config.timeout;
        let completion = self.config.completion.clone();

        for agent in agents {
            let user_prompt = prompt.to_string();
            let config = completion.clone();

            join_set.spawn(async move {
                let name = agent.name();
                let result =
                    tokio::time::timeout(timeout, agent.execute(&user_prompt, &config)).await;
                let mapped = match result {
                    Ok(Ok(response)) => Ok(response),
                    Ok(Err(provider_err)) => Err(MagiError::Provider(provider_err)),
                    Err(_elapsed) => Err(MagiError::Provider(ProviderError::Timeout {
                        message: format!("agent timed out after {timeout:?}"),
                    })),
                };
                (name, mapped)
            });
        }

        let mut results = Vec::new();
        while let Some(join_result) = join_set.join_next().await {
            match join_result {
                Ok(result) => results.push(result),
                Err(join_err) => {
                    results.push((
                        AgentName::Melchior,
                        Err(MagiError::Provider(ProviderError::Process {
                            exit_code: None,
                            stderr: format!("agent task failed: {join_err}"),
                        })),
                    ));
                }
            }
        }

        results
    }

    /// Separates successful agent outputs from failures.
    ///
    /// Parses and validates each raw response. Returns an error if fewer
    /// than 2 agents succeeded.
    fn process_results(
        &self,
        results: Vec<(AgentName, Result<String, MagiError>)>,
    ) -> Result<(Vec<AgentOutput>, Vec<AgentName>), MagiError> {
        let mut successful = Vec::new();
        let mut failed_agents = Vec::new();

        for (name, result) in results {
            match result {
                Ok(raw) => match parse_agent_response(&raw) {
                    Ok(output) => match self.validator.validate(&output) {
                        Ok(()) => successful.push(output),
                        Err(_) => failed_agents.push(name),
                    },
                    Err(_) => failed_agents.push(name),
                },
                Err(_) => {
                    failed_agents.push(name);
                }
            }
        }

        let min_agents = 2;
        if successful.len() < min_agents {
            return Err(MagiError::InsufficientAgents {
                succeeded: successful.len(),
                required: min_agents,
            });
        }

        Ok((successful, failed_agents))
    }
}

/// Formats the user prompt sent to each agent.
///
/// # Parameters
/// - `mode`: The analysis mode.
/// - `content`: The content to analyze.
fn build_prompt(mode: &Mode, content: &str) -> String {
    format!("MODE: {mode}\nCONTEXT:\n{content}")
}

/// Extracts an [`AgentOutput`] from raw LLM response text.
///
/// Handles common LLM output quirks:
/// 1. Strips code fences (` ```json ` and ` ``` `).
/// 2. Finds JSON boundaries (first `{` to last `}`).
/// 3. Deserializes via serde (unknown fields are ignored).
///
/// # Errors
/// Returns `MagiError::Deserialization` if no valid JSON object is found.
fn parse_agent_response(raw: &str) -> Result<AgentOutput, MagiError> {
    let trimmed = raw.trim();

    // Strip code fences
    let stripped = if trimmed.starts_with("```") {
        let without_opening = if let Some(rest) = trimmed.strip_prefix("```json") {
            rest
        } else {
            trimmed.strip_prefix("```").unwrap_or(trimmed)
        };
        without_opening
            .strip_suffix("```")
            .unwrap_or(without_opening)
            .trim()
    } else {
        trimmed
    };

    // Find JSON boundaries: first { to last }
    let start = stripped.find('{');
    let end = stripped.rfind('}');

    match (start, end) {
        (Some(s), Some(e)) if s < e => {
            let json_str = &stripped[s..=e];
            let output: AgentOutput = serde_json::from_str(json_str)?;
            Ok(output)
        }
        _ => Err(MagiError::Deserialization(
            "no JSON object found in agent response".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    /// Helper: build a valid AgentOutput JSON string for a given agent name and verdict.
    fn mock_agent_json(agent: &str, verdict: &str, confidence: f64) -> String {
        format!(
            r#"{{
                "agent": "{agent}",
                "verdict": "{verdict}",
                "confidence": {confidence},
                "summary": "Summary from {agent}",
                "reasoning": "Reasoning from {agent}",
                "findings": [],
                "recommendation": "Recommendation from {agent}"
            }}"#
        )
    }

    /// Mock provider that returns a configurable response per call.
    /// Uses a call counter to track invocations and can return different
    /// responses for each agent by cycling through the responses vec.
    struct MockProvider {
        name: String,
        model: String,
        responses: Vec<Result<String, ProviderError>>,
        call_count: AtomicUsize,
    }

    impl MockProvider {
        fn success(name: &str, model: &str, responses: Vec<String>) -> Self {
            Self {
                name: name.to_string(),
                model: model.to_string(),
                responses: responses.into_iter().map(Ok).collect(),
                call_count: AtomicUsize::new(0),
            }
        }

        fn mixed(name: &str, model: &str, responses: Vec<Result<String, ProviderError>>) -> Self {
            Self {
                name: name.to_string(),
                model: model.to_string(),
                responses,
                call_count: AtomicUsize::new(0),
            }
        }

        fn calls(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for MockProvider {
        async fn complete(
            &self,
            _system_prompt: &str,
            _user_prompt: &str,
            _config: &CompletionConfig,
        ) -> Result<String, ProviderError> {
            let idx = self.call_count.fetch_add(1, Ordering::SeqCst);
            let idx = idx % self.responses.len();
            self.responses[idx].clone()
        }

        fn name(&self) -> &str {
            &self.name
        }

        fn model(&self) -> &str {
            &self.model
        }
    }

    // -- BDD Scenario 1: successful analysis with 3 unanimous agents --

    /// analyze returns MagiReport with 3 outputs, consensus, banner, report, degraded=false.
    #[tokio::test]
    async fn test_analyze_unanimous_approve_returns_complete_report() {
        let responses = vec![
            mock_agent_json("melchior", "approve", 0.9),
            mock_agent_json("balthasar", "approve", 0.85),
            mock_agent_json("caspar", "approve", 0.95),
        ];
        let provider = Arc::new(MockProvider::success("mock", "test-model", responses));
        let magi = Magi::new(provider as Arc<dyn LlmProvider>);

        let result = magi.analyze(&Mode::CodeReview, "fn main() {}").await;
        let report = result.expect("analyze should succeed");

        assert_eq!(report.agents.len(), 3);
        assert!(!report.degraded);
        assert!(report.failed_agents.is_empty());
        assert_eq!(report.consensus.consensus_verdict, Verdict::Approve);
        assert!(!report.banner.is_empty());
        assert!(!report.report.is_empty());
    }

    // -- BDD Scenario 6: degradation - 1 agent timeout --

    /// 2 succeed + 1 timeout produces Ok(MagiReport), degraded=true, failed_agents contains agent.
    #[tokio::test]
    async fn test_analyze_one_agent_timeout_degrades_gracefully() {
        let responses = vec![
            Ok(mock_agent_json("melchior", "approve", 0.9)),
            Ok(mock_agent_json("balthasar", "approve", 0.85)),
            Err(ProviderError::Timeout {
                message: "exceeded timeout".to_string(),
            }),
        ];
        let provider = Arc::new(MockProvider::mixed("mock", "test-model", responses));
        let magi = Magi::new(provider as Arc<dyn LlmProvider>);

        let result = magi.analyze(&Mode::CodeReview, "fn main() {}").await;
        let report = result.expect("analyze should succeed with degradation");

        assert!(report.degraded);
        assert_eq!(report.failed_agents.len(), 1);
        assert_eq!(report.agents.len(), 2);
    }

    // -- BDD Scenario 7: degradation - 1 agent invalid JSON --

    /// 2 succeed + 1 bad JSON produces Ok(MagiReport), degraded=true.
    #[tokio::test]
    async fn test_analyze_one_agent_bad_json_degrades_gracefully() {
        let responses = vec![
            Ok(mock_agent_json("melchior", "approve", 0.9)),
            Ok(mock_agent_json("balthasar", "approve", 0.85)),
            Ok("not valid json at all".to_string()),
        ];
        let provider = Arc::new(MockProvider::mixed("mock", "test-model", responses));
        let magi = Magi::new(provider as Arc<dyn LlmProvider>);

        let result = magi.analyze(&Mode::CodeReview, "fn main() {}").await;
        let report = result.expect("analyze should succeed with degradation");

        assert!(report.degraded);
    }

    // -- BDD Scenario 8: 2 agents fail --

    /// 1 succeed + 2 fail returns Err(InsufficientAgents { succeeded: 1, required: 2 }).
    #[tokio::test]
    async fn test_analyze_two_agents_fail_returns_insufficient_agents() {
        let responses = vec![
            Ok(mock_agent_json("melchior", "approve", 0.9)),
            Err(ProviderError::Timeout {
                message: "timeout".to_string(),
            }),
            Err(ProviderError::Network {
                message: "connection refused".to_string(),
            }),
        ];
        let provider = Arc::new(MockProvider::mixed("mock", "test-model", responses));
        let magi = Magi::new(provider as Arc<dyn LlmProvider>);

        let result = magi.analyze(&Mode::CodeReview, "fn main() {}").await;

        match result {
            Err(MagiError::InsufficientAgents {
                succeeded,
                required,
            }) => {
                assert_eq!(succeeded, 1);
                assert_eq!(required, 2);
            }
            other => panic!("Expected InsufficientAgents, got: {other:?}"),
        }
    }

    // -- BDD Scenario 9: all agents fail --

    /// 0 succeed returns Err(InsufficientAgents { succeeded: 0, required: 2 }).
    #[tokio::test]
    async fn test_analyze_all_agents_fail_returns_insufficient_agents() {
        let responses = vec![
            Err(ProviderError::Timeout {
                message: "timeout".to_string(),
            }),
            Err(ProviderError::Network {
                message: "network".to_string(),
            }),
            Err(ProviderError::Auth {
                message: "auth".to_string(),
            }),
        ];
        let provider = Arc::new(MockProvider::mixed("mock", "test-model", responses));
        let magi = Magi::new(provider as Arc<dyn LlmProvider>);

        let result = magi.analyze(&Mode::CodeReview, "fn main() {}").await;

        match result {
            Err(MagiError::InsufficientAgents {
                succeeded,
                required,
            }) => {
                assert_eq!(succeeded, 0);
                assert_eq!(required, 2);
            }
            other => panic!("Expected InsufficientAgents, got: {other:?}"),
        }
    }

    // -- BDD Scenario 14: LLM returns non-JSON --

    /// Agent returns plain text, treated as failed, system continues with remaining.
    #[tokio::test]
    async fn test_analyze_plain_text_response_treated_as_failure() {
        let responses = vec![
            Ok(mock_agent_json("melchior", "approve", 0.9)),
            Ok(mock_agent_json("balthasar", "approve", 0.85)),
            Ok("I think the code is good".to_string()),
        ];
        let provider = Arc::new(MockProvider::mixed("mock", "test-model", responses));
        let magi = Magi::new(provider as Arc<dyn LlmProvider>);

        let result = magi.analyze(&Mode::CodeReview, "fn main() {}").await;
        let report = result.expect("should succeed with degradation");

        assert!(report.degraded);
        assert_eq!(report.agents.len(), 2);
    }

    // -- BDD Scenario 28: Magi::new with single provider --

    /// new creates Magi with 3 agents sharing same provider, all defaults.
    #[tokio::test]
    async fn test_magi_new_creates_with_defaults() {
        let responses = vec![
            mock_agent_json("melchior", "approve", 0.9),
            mock_agent_json("balthasar", "approve", 0.85),
            mock_agent_json("caspar", "approve", 0.95),
        ];
        let provider = Arc::new(MockProvider::success(
            "test-provider",
            "test-model",
            responses,
        ));
        let magi = Magi::new(provider as Arc<dyn LlmProvider>);

        let result = magi.analyze(&Mode::CodeReview, "test content").await;
        let report = result.expect("should succeed");

        // All 3 agents used the same provider
        assert_eq!(report.agents.len(), 3);
    }

    // -- BDD Scenario 29: builder with mixed providers and custom config --

    /// Builder sets per-agent providers and custom timeout.
    #[tokio::test]
    async fn test_builder_with_mixed_providers_and_custom_config() {
        let default_responses = vec![
            mock_agent_json("melchior", "approve", 0.9),
            mock_agent_json("balthasar", "approve", 0.85),
        ];
        let caspar_responses = vec![mock_agent_json("caspar", "reject", 0.8)];

        let default_provider = Arc::new(MockProvider::success(
            "default-provider",
            "model-a",
            default_responses,
        ));
        let caspar_provider = Arc::new(MockProvider::success(
            "caspar-provider",
            "model-b",
            caspar_responses,
        ));

        let magi = MagiBuilder::new(default_provider.clone() as Arc<dyn LlmProvider>)
            .with_provider(
                AgentName::Caspar,
                caspar_provider.clone() as Arc<dyn LlmProvider>,
            )
            .with_timeout(Duration::from_secs(60))
            .build()
            .expect("build should succeed");

        let result = magi.analyze(&Mode::CodeReview, "test content").await;
        let report = result.expect("should succeed");

        assert_eq!(report.agents.len(), 3);
        // Caspar used the override provider
        assert!(caspar_provider.calls() > 0);
    }

    // -- BDD Scenario 32: input too large --

    /// Content exceeding max_input_len returns Err(InputTooLarge) without launching agents.
    #[tokio::test]
    async fn test_analyze_input_too_large_rejects_without_launching_agents() {
        let responses = vec![mock_agent_json("melchior", "approve", 0.9)];
        let provider = Arc::new(MockProvider::success("mock", "test-model", responses));

        let magi = MagiBuilder::new(provider.clone() as Arc<dyn LlmProvider>)
            .with_max_input_len(100)
            .build()
            .expect("build should succeed");

        let content = "x".repeat(200);
        let result = magi.analyze(&Mode::CodeReview, &content).await;

        match result {
            Err(MagiError::InputTooLarge { size, max }) => {
                assert_eq!(size, 200);
                assert_eq!(max, 100);
            }
            other => panic!("Expected InputTooLarge, got: {other:?}"),
        }

        // Provider should NOT have been called
        assert_eq!(provider.calls(), 0, "No agents should have been launched");
    }

    // -- MagiConfig defaults --

    /// MagiConfig::default has timeout=300s, max_input_len=1MB.
    #[test]
    fn test_magi_config_default_values() {
        let config = MagiConfig::default();
        assert_eq!(config.timeout, Duration::from_secs(300));
        assert_eq!(config.max_input_len, 1_048_576);
    }

    // -- build_prompt formatting --

    /// build_prompt formats "MODE: {mode}\nCONTEXT:\n{content}".
    #[test]
    fn test_build_prompt_formats_mode_and_content() {
        let result = build_prompt(&Mode::CodeReview, "fn main() {}");
        assert_eq!(result, "MODE: code-review\nCONTEXT:\nfn main() {}");
    }

    // -- parse_agent_response --

    /// parse_agent_response strips code fences from JSON.
    #[test]
    fn test_parse_agent_response_strips_code_fences() {
        let json = mock_agent_json("melchior", "approve", 0.9);
        let raw = format!("```json\n{json}\n```");

        let result = parse_agent_response(&raw);
        let output = result.expect("should parse successfully");
        assert_eq!(output.agent, AgentName::Melchior);
        assert_eq!(output.verdict, Verdict::Approve);
    }

    /// parse_agent_response finds JSON object in preamble text.
    #[test]
    fn test_parse_agent_response_extracts_json_from_preamble() {
        let json = mock_agent_json("melchior", "approve", 0.9);
        let raw = format!("Here is my analysis:\n{json}");

        let result = parse_agent_response(&raw);
        assert!(result.is_ok(), "should find JSON in preamble text");
    }

    /// parse_agent_response fails on completely invalid input.
    #[test]
    fn test_parse_agent_response_fails_on_invalid_input() {
        let result = parse_agent_response("no json here");
        assert!(result.is_err(), "should fail on invalid input");
    }

    // -- MagiBuilder --

    /// MagiBuilder::build returns Ok(Magi) with required provider.
    #[test]
    fn test_magi_builder_build_returns_result() {
        let responses = vec![mock_agent_json("melchior", "approve", 0.9)];
        let provider =
            Arc::new(MockProvider::success("mock", "model", responses)) as Arc<dyn LlmProvider>;

        let magi = MagiBuilder::new(provider).build();
        assert!(magi.is_ok());
    }
}
