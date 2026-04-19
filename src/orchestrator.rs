// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use std::sync::Mutex;

use crate::agent::{Agent, AgentFactory};
use crate::consensus::{ConsensusConfig, ConsensusEngine};
use crate::error::{MagiError, ProviderError};
use crate::provider::{CompletionConfig, LlmProvider};
use crate::reporting::{MagiReport, ReportConfig, ReportFormatter};
use crate::schema::{AgentName, AgentOutput, Mode};
use crate::user_prompt::{FastrandSource, RngLike, build_user_prompt};
use crate::validate::{ValidationLimits, Validator};
use tokio::task::AbortHandle;

/// Default value for [`MagiConfig::max_input_len`] — 4 MB.
///
/// This is a compromise between Python's 10 MB and v0.1.2's 1 MB.
/// A full 10 MB alignment with Python is deferred to v0.3.0 pending
/// an allocation audit of the `analyze()` pipeline.
///
/// For public-facing deployments where `content` is untrusted, consider
/// using [`MagiBuilder::with_max_input_len`] to set a lower limit.
pub const DEFAULT_MAX_INPUT_LEN: usize = 4 * 1024 * 1024;

/// Configuration for the MAGI orchestrator.
///
/// Controls timeout per agent, maximum input size, and LLM completion parameters.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct MagiConfig {
    /// Maximum time to wait for each agent (default: 300 seconds).
    pub timeout: Duration,
    /// Maximum accepted size of the raw `content` argument to [`Magi::analyze`], in bytes.
    ///
    /// Default: [`DEFAULT_MAX_INPUT_LEN`] (4 MB).
    ///
    /// Note: for public-facing deployments where `content` is untrusted,
    /// consider lowering this via [`MagiBuilder::with_max_input_len`] to a value
    /// appropriate for your threat model. Default (4 MB) is a compromise between
    /// Python MAGI's 10 MB and v0.1.2's 1 MB; a full 10 MB alignment with Python
    /// is deferred to v0.3.0 pending allocation audit of the analyze() pipeline.
    ///
    /// # Allocation audit (2026-04-18)
    ///
    /// An allocation audit of the `analyze()` pipeline for `magi-core v0.2.0` found
    /// 5 copy points on the content's path from `analyze()` entry to wire serialization:
    /// (1) user-prompt construction via `format!`, (2–4) per-agent `String::clone` to
    /// satisfy `tokio::spawn`'s `'static` bound (3 agents), and (5) HTTP/stdin
    /// serialization by the provider. Peak memory per analysis is approximately
    /// `content.len() × 5` plus fixed overhead. For the 4 MB default, peak ≈ 20 MB.
    /// A full 10 MB alignment with Python is deferred to v0.3.0, pending an
    /// `Arc<str>` refactor of the orchestrator-to-provider path to reduce copies.
    pub max_input_len: usize,
    /// Completion parameters forwarded to each agent.
    pub completion: CompletionConfig,
}

impl Default for MagiConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(300),
            max_input_len: DEFAULT_MAX_INPUT_LEN,
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
    overrides: BTreeMap<(AgentName, Option<Mode>), String>,
    prompts_dir: Option<PathBuf>,
    config: MagiConfig,
    validation_limits: ValidationLimits,
    consensus_config: ConsensusConfig,
    report_config: ReportConfig,
    rng_source: Option<Box<dyn RngLike + Send>>,
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
            overrides: BTreeMap::new(),
            prompts_dir: None,
            config: MagiConfig::default(),
            validation_limits: ValidationLimits::default(),
            consensus_config: ConsensusConfig::default(),
            report_config: ReportConfig::default(),
            rng_source: None,
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

    /// Sets a custom system prompt for a specific agent and mode.
    ///
    /// Stores the override under the `(agent, Some(mode))` key so that
    /// [`Magi::analyze`] can select it for the matching `(agent, mode)` pair.
    ///
    /// # Parameters
    /// - `agent`: Which agent to override.
    /// - `mode`: The analysis mode for which this prompt applies.
    /// - `prompt`: The custom system prompt.
    pub fn with_custom_prompt_for_mode(
        mut self,
        agent: AgentName,
        mode: Mode,
        prompt: String,
    ) -> Self {
        self.overrides.insert((agent, Some(mode)), prompt);
        self
    }

    /// Sets a custom system prompt for a specific agent across all modes.
    ///
    /// Stores the override under the `(agent, None)` key, which serves as a
    /// mode-agnostic fallback when no mode-specific override exists.
    ///
    /// # Parameters
    /// - `agent`: Which agent to override.
    /// - `prompt`: The custom system prompt applied for all analysis modes.
    pub fn with_custom_prompt_all_modes(mut self, agent: AgentName, prompt: String) -> Self {
        self.overrides.insert((agent, None), prompt);
        self
    }

    /// Injects a custom RNG source for nonce generation in `build_user_prompt`.
    ///
    /// Intended for testing only (`pub(crate)`). The nonce is shared across
    /// all agents for a single `analyze()` invocation (one call per request).
    ///
    /// # Parameters
    /// - `rng`: A boxed [`RngLike`] implementation to use instead of the default
    ///   [`FastrandSource`].
    pub(crate) fn with_rng_source(mut self, rng: Box<dyn RngLike + Send>) -> Self {
        self.rng_source = Some(rng);
        self
    }

    /// Sets a custom system prompt for a specific agent and mode.
    ///
    /// # Deprecated
    ///
    /// Use [`with_custom_prompt_for_mode`](Self::with_custom_prompt_for_mode) instead.
    ///
    /// # Parameters
    /// - `agent`: Which agent to override.
    /// - `mode`: The analysis mode.
    /// - `prompt`: The custom system prompt.
    #[deprecated(since = "0.3.0", note = "use `with_custom_prompt_for_mode`")]
    pub fn with_custom_prompt(self, agent: AgentName, mode: Mode, prompt: String) -> Self {
        self.with_custom_prompt_for_mode(agent, mode, prompt)
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
        if let Some(dir) = self.prompts_dir {
            factory = factory.from_directory(&dir)?;
        }

        let rng_source = self
            .rng_source
            .unwrap_or_else(|| Box::new(FastrandSource) as Box<dyn RngLike + Send>);

        Ok(Magi {
            config: self.config,
            agent_factory: factory,
            validator: Validator::with_limits(self.validation_limits),
            consensus_engine: ConsensusEngine::new(self.consensus_config),
            formatter: ReportFormatter::with_config(self.report_config)
                .map_err(|e| MagiError::Validation(e.to_string()))?,
            overrides: self.overrides,
            rng_source: Arc::new(Mutex::new(rng_source)),
        })
    }
}

/// RAII guard that aborts spawned tasks when dropped.
///
/// Ensures that if [`Magi::analyze`] is cancelled (e.g., the caller wraps it
/// in `tokio::time::timeout`), all in-flight agent tasks are aborted instead
/// of continuing to run in the background and consuming LLM API quota.
struct AbortGuard(Vec<AbortHandle>);

impl Drop for AbortGuard {
    fn drop(&mut self) {
        for handle in &self.0 {
            handle.abort();
        }
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
    overrides: BTreeMap<(AgentName, Option<Mode>), String>,
    rng_source: Arc<Mutex<Box<dyn RngLike + Send>>>,
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
    /// - [`MagiError::InvalidInput`] if nonce collision detected (probability ~2^-64
    ///   per call; fastrand effective state ~64 bits; see ADR 001 §Decision: Nonce RNG choice).
    pub async fn analyze(&self, mode: &Mode, content: &str) -> Result<MagiReport, MagiError> {
        // 1. Input validation
        if content.len() > self.config.max_input_len {
            return Err(MagiError::InputTooLarge {
                size: content.len(),
                max: self.config.max_input_len,
            });
        }

        // 2. Create agents, resolving system prompts via lookup_prompt so that
        //    overrides registered through with_custom_prompt_for_mode /
        //    with_custom_prompt_all_modes take effect.
        let agents = self
            .agent_factory
            .create_agents_with_prompts(*mode, &self.overrides);

        // 3. Build user prompt with sanitization and nonce injection.
        //    Lock is released immediately after prompt construction.
        let prompt = {
            let mut rng = self.rng_source.lock().expect("rng_source mutex poisoned");
            build_user_prompt(*mode, content, &mut **rng)?
        };

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

    /// Launches all agents in parallel using individual `tokio::spawn` tasks.
    ///
    /// Each agent task is wrapped in `tokio::time::timeout`. Agent names are
    /// tracked alongside their `JoinHandle`s so that panicked tasks are correctly
    /// attributed to the right agent (unlike `JoinSet`, which loses task identity
    /// on panic).
    ///
    /// An [`AbortGuard`] holds abort handles for all spawned tasks. If this
    /// future is dropped (e.g., the caller times out), the guard aborts every
    /// running task, preventing wasted LLM API quota.
    async fn launch_agents(
        &self,
        agents: Vec<Agent>,
        prompt: &str,
    ) -> Vec<(AgentName, Result<String, MagiError>)> {
        let timeout = self.config.timeout;
        let completion = self.config.completion.clone();
        let mut handles = Vec::new();
        let mut abort_handles = Vec::new();

        for agent in agents {
            let name = agent.name();
            let user_prompt = prompt.to_string();
            let config = completion.clone();

            let handle = tokio::spawn(async move {
                let result =
                    tokio::time::timeout(timeout, agent.execute(&user_prompt, &config)).await;
                match result {
                    Ok(Ok(response)) => Ok(response),
                    Ok(Err(provider_err)) => Err(MagiError::Provider(provider_err)),
                    Err(_elapsed) => Err(MagiError::Provider(ProviderError::Timeout {
                        message: format!("agent timed out after {timeout:?}"),
                    })),
                }
            });
            abort_handles.push(handle.abort_handle());
            handles.push((name, handle));
        }

        // Guard aborts all tasks if this future is cancelled before completion.
        // Once all handles are awaited below, abort() on a finished task is a no-op.
        let _guard = AbortGuard(abort_handles);

        let mut results = Vec::new();
        for (name, handle) in handles {
            match handle.await {
                Ok(result) => results.push((name, result)),
                Err(join_err) => results.push((
                    name,
                    Err(MagiError::Provider(ProviderError::Process {
                        exit_code: None,
                        stderr: format!("agent task panicked: {join_err}"),
                    })),
                )),
            }
        }

        results
    }

    /// Separates successful agent outputs from failures.
    ///
    /// Parses and validates each raw response, preserving failure reasons
    /// for diagnostic visibility. Returns an error if fewer than 2 agents
    /// succeeded.
    fn process_results(
        &self,
        results: Vec<(AgentName, Result<String, MagiError>)>,
    ) -> Result<(Vec<AgentOutput>, BTreeMap<AgentName, String>), MagiError> {
        let mut successful = Vec::new();
        let mut failed_agents = BTreeMap::new();

        for (name, result) in results {
            match result {
                Ok(raw) => match parse_agent_response(&raw) {
                    Ok(mut output) => match self.validator.validate_mut(&mut output) {
                        Ok(()) => successful.push(output),
                        Err(e) => {
                            failed_agents.insert(name, format!("validation: {e}"));
                        }
                    },
                    Err(e) => {
                        failed_agents.insert(name, format!("parse: {e}"));
                    }
                },
                Err(e) => {
                    failed_agents.insert(name, e.to_string());
                }
            }
        }

        let min_agents = self.consensus_engine.min_agents();
        if successful.len() < min_agents {
            return Err(MagiError::InsufficientAgents {
                succeeded: successful.len(),
                required: min_agents,
            });
        }

        Ok((successful, failed_agents))
    }

    /// Returns the custom prompt overrides map for inspection in tests.
    ///
    /// Keys are `(AgentName, Some(Mode))` for mode-specific overrides and
    /// `(AgentName, None)` for mode-agnostic overrides.
    #[cfg(test)]
    pub(crate) fn overrides(&self) -> &BTreeMap<(AgentName, Option<Mode>), String> {
        &self.overrides
    }
}

/// Resolves the system prompt for an agent given a mode and the overrides map.
///
/// Priority order:
/// 1. Mode-specific override: `(agent, Some(mode))`
/// 2. Mode-agnostic override: `(agent, None)`
/// 3. Compiled-in embedded default for the agent
///
/// # Parameters
/// - `agent`: Which MAGI agent (Melchior, Balthasar, Caspar).
/// - `mode`: The current analysis mode.
/// - `overrides`: Map of custom prompt overrides keyed by `(AgentName, Option<Mode>)`.
///
/// # Returns
/// A string slice of the resolved prompt (borrowed from the map or `'static` from embedded).
pub(crate) fn lookup_prompt(
    agent: AgentName,
    mode: Mode,
    overrides: &BTreeMap<(AgentName, Option<Mode>), String>,
) -> &str {
    if let Some(s) = overrides.get(&(agent, Some(mode))) {
        return s.as_str();
    }
    if let Some(s) = overrides.get(&(agent, None)) {
        return s.as_str();
    }
    crate::prompts::embedded_prompt_for(agent)
}

/// Extracts an [`AgentOutput`] from raw LLM response text.
///
/// Handles common LLM output quirks:
/// 1. Strips code fences (` ```json ` and ` ``` `).
/// 2. Tries to parse JSON from each `{` position until one succeeds.
/// 3. Deserializes via serde (unknown fields are ignored).
///
/// This approach is resilient to LLM responses that contain stray `{}`
/// in prose before the actual JSON payload.
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

    // Fast path: try parsing the entire string as a single JSON object.
    // This handles the common case where the LLM returns only JSON.
    if let Ok(output) = serde_json::from_str::<AgentOutput>(stripped) {
        return Ok(output);
    }

    // Fallback: search forward through each '{' position for the first valid
    // AgentOutput JSON. Forward search is preferred because it finds the first
    // complete object, avoiding false matches from trailing prose that might
    // coincidentally contain valid JSON.
    for (start, _) in stripped.match_indices('{') {
        let candidate = &stripped[start..];
        if let Ok(output) = serde_json::from_str::<AgentOutput>(candidate) {
            return Ok(output);
        }
    }

    Err(MagiError::Deserialization(
        "no valid JSON object found in agent response".to_string(),
    ))
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

    /// MagiConfig::default has timeout=300s, max_input_len=4MB.
    #[test]
    fn test_magi_config_default_values() {
        let config = MagiConfig::default();
        assert_eq!(config.timeout, Duration::from_secs(300));
        assert_eq!(config.max_input_len, 4 * 1024 * 1024);
    }

    /// MagiBuilder::with_max_input_len overrides the default max_input_len.
    #[tokio::test]
    async fn test_builder_with_max_input_len_overrides_default() {
        let responses = vec![mock_agent_json("melchior", "approve", 0.9)];
        let provider =
            Arc::new(MockProvider::success("mock", "model", responses)) as Arc<dyn LlmProvider>;

        let magi = MagiBuilder::new(provider.clone())
            .with_max_input_len(512)
            .build()
            .expect("build should succeed");

        let too_large = "x".repeat(513);
        let result = magi.analyze(&Mode::CodeReview, &too_large).await;
        match result {
            Err(MagiError::InputTooLarge { size, max }) => {
                assert_eq!(size, 513);
                assert_eq!(max, 512);
            }
            other => panic!("Expected InputTooLarge, got: {other:?}"),
        }
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

    // -- T11: MagiBuilder API — for_mode / all_modes / rng_source --
    // -- T13: CapturingMockProvider upgrade — explicit agent-routing table --

    /// Mock provider with an explicit `(system_prompt → AgentName)` routing
    /// table. Eliminates the need to parse system-prompt content to infer
    /// agent identity (MAGI R1 W6).
    ///
    /// Captures every `(system_prompt, user_prompt)` pair so tests can inspect
    /// exactly what each agent received.
    #[derive(Clone)]
    struct CapturingMockProvider {
        /// Recorded calls: `(system_prompt, user_prompt)` in call order.
        captured: Arc<std::sync::Mutex<Vec<(String, String)>>>,
        /// Maps a recognized system prompt to the agent name the mock should
        /// emit in its JSON response.
        routing: Arc<std::collections::HashMap<String, AgentName>>, // MAGI R3 W2
    }

    impl CapturingMockProvider {
        /// Build a mock that routes each known default prompt back to its
        /// owning agent.  Used when no custom overrides are in play.
        fn for_default_prompts(captured: Arc<std::sync::Mutex<Vec<(String, String)>>>) -> Self {
            let mut routing = std::collections::HashMap::new();
            routing.insert(
                crate::prompts::melchior_prompt().to_string(),
                AgentName::Melchior,
            );
            routing.insert(
                crate::prompts::balthasar_prompt().to_string(),
                AgentName::Balthasar,
            );
            routing.insert(
                crate::prompts::caspar_prompt().to_string(),
                AgentName::Caspar,
            );
            Self {
                captured,
                routing: Arc::new(routing),
            }
        }

        /// Build a mock with explicit `(custom_prompt → agent)` mappings for
        /// tests that inject overrides.  Default prompts are included as
        /// fallback so unoverridden agents still resolve correctly.
        fn with_routing(
            captured: Arc<std::sync::Mutex<Vec<(String, String)>>>,
            mappings: Vec<(&'static str, AgentName)>,
        ) -> Self {
            let mut routing = std::collections::HashMap::new();
            // Default prompts as fallback.
            routing.insert(
                crate::prompts::melchior_prompt().to_string(),
                AgentName::Melchior,
            );
            routing.insert(
                crate::prompts::balthasar_prompt().to_string(),
                AgentName::Balthasar,
            );
            routing.insert(
                crate::prompts::caspar_prompt().to_string(),
                AgentName::Caspar,
            );
            for (custom, name) in mappings {
                routing.insert(custom.to_string(), name);
            }
            Self {
                captured,
                routing: Arc::new(routing),
            }
        }

        /// Alias for `for_default_prompts`.  Used when a test only wants to
        /// inspect captured inputs and does not care about response routing.
        #[allow(dead_code)]
        fn for_prompt_capture(captured: Arc<std::sync::Mutex<Vec<(String, String)>>>) -> Self {
            Self::for_default_prompts(captured)
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for CapturingMockProvider {
        async fn complete(
            &self,
            system_prompt: &str,
            user_prompt: &str,
            _config: &CompletionConfig,
        ) -> Result<String, ProviderError> {
            self.captured
                .lock()
                .unwrap()
                .push((system_prompt.to_string(), user_prompt.to_string()));
            let agent = self
                .routing
                .get(system_prompt)
                .copied()
                .unwrap_or(AgentName::Melchior);
            let agent_str = match agent {
                AgentName::Melchior => "melchior",
                AgentName::Balthasar => "balthasar",
                AgentName::Caspar => "caspar",
            };
            Ok(mock_agent_json(agent_str, "approve", 0.9))
        }

        fn name(&self) -> &str {
            "capturing-mock"
        }

        fn model(&self) -> &str {
            "test-model"
        }
    }

    /// with_custom_prompt_for_mode stores entry with Some(mode) key.
    #[test]
    fn test_with_custom_prompt_for_mode_stores_with_some_key() {
        let provider: Arc<dyn LlmProvider> = Arc::new(MockProvider::success(
            "mock",
            "model",
            vec![mock_agent_json("melchior", "approve", 0.9)],
        ));
        let magi = MagiBuilder::new(provider)
            .with_custom_prompt_for_mode(AgentName::Melchior, Mode::CodeReview, "X".into())
            .build()
            .expect("build should succeed");
        assert_eq!(
            magi.overrides()
                .get(&(AgentName::Melchior, Some(Mode::CodeReview))),
            Some(&"X".to_string())
        );
    }

    /// with_custom_prompt_all_modes stores entry with None key.
    #[test]
    fn test_with_custom_prompt_all_modes_stores_with_none_key() {
        let provider: Arc<dyn LlmProvider> = Arc::new(MockProvider::success(
            "mock",
            "model",
            vec![mock_agent_json("melchior", "approve", 0.9)],
        ));
        let magi = MagiBuilder::new(provider)
            .with_custom_prompt_all_modes(AgentName::Balthasar, "Y".into())
            .build()
            .expect("build should succeed");
        assert_eq!(
            magi.overrides().get(&(AgentName::Balthasar, None)),
            Some(&"Y".to_string())
        );
    }

    /// Deprecated with_custom_prompt delegates to with_custom_prompt_for_mode.
    #[test]
    fn test_legacy_with_custom_prompt_delegates_to_for_mode() {
        let provider: Arc<dyn LlmProvider> = Arc::new(MockProvider::success(
            "mock",
            "model",
            vec![mock_agent_json("melchior", "approve", 0.9)],
        ));
        #[allow(deprecated)]
        let magi = MagiBuilder::new(provider)
            .with_custom_prompt(AgentName::Caspar, Mode::Design, "Z".into())
            .build()
            .expect("build should succeed");
        assert_eq!(
            magi.overrides()
                .get(&(AgentName::Caspar, Some(Mode::Design))),
            Some(&"Z".to_string())
        );
    }

    // -- T12: lookup_prompt resolution --

    /// lookup_prompt prefers mode-specific override when both mode-specific and
    /// mode-agnostic overrides exist for the same agent.
    #[test]
    fn test_lookup_prompt_prefers_mode_specific_override() {
        let mut overrides = BTreeMap::new();
        overrides.insert(
            (AgentName::Melchior, Some(Mode::CodeReview)),
            "SPECIFIC".to_string(),
        );
        overrides.insert((AgentName::Melchior, None), "GENERIC".to_string());
        assert_eq!(
            lookup_prompt(AgentName::Melchior, Mode::CodeReview, &overrides),
            "SPECIFIC"
        );
    }

    /// lookup_prompt falls back to mode-agnostic override when only (agent, None) is present.
    #[test]
    fn test_lookup_prompt_falls_back_to_mode_agnostic_when_mode_specific_missing() {
        let mut overrides = BTreeMap::new();
        overrides.insert((AgentName::Melchior, None), "GENERIC".to_string());
        assert_eq!(
            lookup_prompt(AgentName::Melchior, Mode::CodeReview, &overrides),
            "GENERIC"
        );
    }

    /// lookup_prompt falls back to embedded default when overrides map is empty.
    #[test]
    fn test_lookup_prompt_falls_back_to_embedded_default_when_no_override() {
        let overrides: BTreeMap<(AgentName, Option<Mode>), String> = BTreeMap::new();
        let result = lookup_prompt(AgentName::Caspar, Mode::Analysis, &overrides);
        assert_eq!(result, crate::prompts::caspar_prompt());
    }

    /// lookup_prompt returns the correct embedded default for each agent.
    #[test]
    fn test_lookup_prompt_returns_correct_embedded_default_per_agent() {
        let overrides: BTreeMap<(AgentName, Option<Mode>), String> = BTreeMap::new();
        assert_eq!(
            lookup_prompt(AgentName::Melchior, Mode::CodeReview, &overrides),
            crate::prompts::melchior_prompt()
        );
        assert_eq!(
            lookup_prompt(AgentName::Balthasar, Mode::Design, &overrides),
            crate::prompts::balthasar_prompt()
        );
        assert_eq!(
            lookup_prompt(AgentName::Caspar, Mode::Analysis, &overrides),
            crate::prompts::caspar_prompt()
        );
    }

    /// with_rng_source injects a fixed nonce observable in the captured user_prompt.
    #[tokio::test]
    async fn test_with_rng_source_injects_nonce_observable_in_user_prompt() {
        // Strengthened per MAGI R2 W9 — not a no-op assertion; observes
        // the fixed nonce flowing through to the captured user_prompt.
        let captured: Arc<std::sync::Mutex<Vec<(String, String)>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let provider = Arc::new(CapturingMockProvider::for_default_prompts(captured.clone()));
        let nonce_val: u128 = 0x1234_5678_9abc_def0_fedc_ba98_7654_3210;
        let expected_nonce_hex = format!("{nonce_val:032x}");

        // Single nonce shared across all agents for one analyze call (RF-10).
        let rng = Box::new(crate::user_prompt::FixedRng::new(vec![nonce_val]))
            as Box<dyn crate::user_prompt::RngLike + Send>;
        let magi = MagiBuilder::new(provider as Arc<dyn LlmProvider>)
            .with_rng_source(rng)
            .build()
            .expect("build should succeed");
        let _ = magi.analyze(&Mode::Analysis, "hello").await.unwrap();

        let calls = captured.lock().unwrap();
        assert!(
            !calls.is_empty(),
            "mock should have received at least one call"
        );
        let (_, user_prompt) = &calls[0];
        assert!(
            user_prompt.contains(&expected_nonce_hex),
            "user_prompt should contain the fixed nonce {expected_nonce_hex}"
        );
    }

    // -- T13: End-to-end integration tests --

    /// A mode-agnostic override registered via `with_custom_prompt_all_modes`
    /// must be forwarded as the system prompt to the targeted agent regardless
    /// of which `Mode` is passed to `analyze`.
    #[tokio::test]
    async fn test_analyze_applies_mode_agnostic_override_to_melchior() {
        let captured = Arc::new(std::sync::Mutex::new(Vec::new()));
        let provider = Arc::new(CapturingMockProvider::with_routing(
            captured.clone(),
            vec![("CUSTOM MEL", AgentName::Melchior)],
        ));
        let magi = MagiBuilder::new(provider as Arc<dyn LlmProvider>)
            .with_custom_prompt_all_modes(AgentName::Melchior, "CUSTOM MEL".into())
            .build()
            .expect("build should succeed");
        let _ = magi.analyze(&Mode::Design, "x").await.unwrap();
        let calls = captured.lock().unwrap();
        assert!(
            calls.iter().any(|(sys, _)| sys == "CUSTOM MEL"),
            "Melchior should have received the mode-agnostic custom prompt"
        );
    }

    /// A mode-specific override registered via `with_custom_prompt_for_mode`
    /// must supersede a mode-agnostic override for the same agent when `analyze`
    /// is called with the matching mode.
    #[tokio::test]
    async fn test_analyze_per_mode_override_supersedes_all_modes() {
        let captured = Arc::new(std::sync::Mutex::new(Vec::new()));
        let provider = Arc::new(CapturingMockProvider::with_routing(
            captured.clone(),
            vec![
                ("GENERIC MEL", AgentName::Melchior),
                ("SPECIFIC MEL", AgentName::Melchior),
            ],
        ));
        let magi = MagiBuilder::new(provider as Arc<dyn LlmProvider>)
            .with_custom_prompt_all_modes(AgentName::Melchior, "GENERIC MEL".into())
            .with_custom_prompt_for_mode(AgentName::Melchior, Mode::Design, "SPECIFIC MEL".into())
            .build()
            .expect("build should succeed");
        let _ = magi.analyze(&Mode::Design, "x").await.unwrap();
        let calls = captured.lock().unwrap();
        assert!(
            calls.iter().any(|(sys, _)| sys == "SPECIFIC MEL"),
            "mode-specific prompt should have been used for Mode::Design"
        );
        assert!(
            !calls.iter().any(|(sys, _)| sys == "GENERIC MEL"),
            "mode-agnostic prompt must NOT be used when a mode-specific one is present"
        );
    }

    /// When the injected `FixedRng` produces a nonce whose hex encoding
    /// appears verbatim in the (sanitized) input, `analyze` must propagate
    /// `MagiError::InvalidInput` from `build_user_prompt`.
    #[tokio::test]
    async fn test_analyze_nonce_collision_returns_invalid_input() {
        let captured = Arc::new(std::sync::Mutex::new(Vec::new()));
        let provider = Arc::new(CapturingMockProvider::for_default_prompts(captured));
        let fixed_nonce_val: u128 = 0x1234_5678_9012_3456_7890_1234_5678_9012;
        let fixed_nonce_hex = format!("{fixed_nonce_val:032x}");
        // Content that is exactly the nonce hex — guaranteed collision.
        let colliding_content = fixed_nonce_hex.clone();

        let magi = MagiBuilder::new(provider as Arc<dyn LlmProvider>)
            .with_rng_source(Box::new(crate::user_prompt::FixedRng::new(vec![
                fixed_nonce_val,
            ])))
            .build()
            .expect("build should succeed");

        let result = magi.analyze(&Mode::Analysis, &colliding_content).await;
        assert!(
            matches!(result, Err(MagiError::InvalidInput { .. })),
            "nonce collision must yield MagiError::InvalidInput, got: {result:?}"
        );
    }

    /// The deprecated `with_custom_prompt` shim must round-trip through the
    /// new `with_custom_prompt_for_mode` path and produce a result identical
    /// to calling `with_custom_prompt_for_mode` directly.
    #[tokio::test]
    #[allow(deprecated)]
    async fn test_legacy_with_custom_prompt_shim_roundtrip() {
        let captured_legacy = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured_new = Arc::new(std::sync::Mutex::new(Vec::new()));

        let provider_legacy = Arc::new(CapturingMockProvider::with_routing(
            captured_legacy.clone(),
            vec![("SHIM PROMPT", AgentName::Caspar)],
        ));
        let provider_new = Arc::new(CapturingMockProvider::with_routing(
            captured_new.clone(),
            vec![("SHIM PROMPT", AgentName::Caspar)],
        ));

        let magi_legacy = MagiBuilder::new(provider_legacy as Arc<dyn LlmProvider>)
            .with_custom_prompt(AgentName::Caspar, Mode::CodeReview, "SHIM PROMPT".into())
            .build()
            .expect("legacy build should succeed");

        let magi_new = MagiBuilder::new(provider_new as Arc<dyn LlmProvider>)
            .with_custom_prompt_for_mode(AgentName::Caspar, Mode::CodeReview, "SHIM PROMPT".into())
            .build()
            .expect("new build should succeed");

        let _ = magi_legacy
            .analyze(&Mode::CodeReview, "test")
            .await
            .unwrap();
        let _ = magi_new.analyze(&Mode::CodeReview, "test").await.unwrap();

        let legacy_calls = captured_legacy.lock().unwrap();
        let new_calls = captured_new.lock().unwrap();

        // Both paths must have forwarded "SHIM PROMPT" to Caspar.
        assert!(
            legacy_calls.iter().any(|(sys, _)| sys == "SHIM PROMPT"),
            "legacy shim must forward the custom prompt to Caspar"
        );
        assert!(
            new_calls.iter().any(|(sys, _)| sys == "SHIM PROMPT"),
            "new API must forward the custom prompt to Caspar"
        );
    }

    /// with_prompts_dir-loaded files must reach the targeted agent as system prompt.
    ///
    /// Regression guard for the v0.3 bug where `factory.custom_prompts` was
    /// populated by `from_directory` but never merged into `self.overrides`,
    /// causing filesystem-loaded prompts to be silently dropped in `analyze`.
    #[tokio::test]
    async fn test_analyze_respects_prompts_dir_loaded_files() {
        // Create a temp dir with a custom melchior prompt file.
        let tmp = std::env::temp_dir()
            .join(format!("magi_v03_test_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(
            tmp.join("melchior_code_review.md"),
            "CUSTOM FROM FILESYSTEM",
        )
        .unwrap();

        let captured: Arc<std::sync::Mutex<Vec<(String, String)>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let provider = Arc::new(CapturingMockProvider::with_routing(
            captured.clone(),
            vec![("CUSTOM FROM FILESYSTEM", AgentName::Melchior)],
        ));
        let magi = MagiBuilder::new(provider as Arc<dyn LlmProvider>)
            .with_prompts_dir(tmp.clone())
            .build()
            .expect("build should succeed");
        let _ = magi.analyze(&Mode::CodeReview, "x").await.unwrap();

        let calls = captured.lock().unwrap();
        assert!(
            calls.iter().any(|(sys, _)| sys == "CUSTOM FROM FILESYSTEM"),
            "with_prompts_dir file-based prompt should reach Melchior"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
