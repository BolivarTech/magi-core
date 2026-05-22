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
use crate::error::MagiError;
#[cfg(test)]
use crate::error::ProviderError;
use crate::provider::{CompletionConfig, LlmProvider};
use crate::reporting::{MagiReport, ReportConfig, ReportFormatter};
use crate::schema::{AgentName, AgentOutput, Mode};
use crate::user_prompt::{FastrandSource, RngLike, build_retry_prompt, build_user_prompt};
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
    /// **v0.4.0** — enable the single-shot retry on schema/parse errors.
    ///
    /// Default: `true`. When enabled, an agent whose first response fails
    /// `MagiError::Validation` or `MagiError::Deserialization` is retried
    /// once with a corrective prompt (Python v2.2.0/v2.2.4 parity).
    ///
    /// When disabled (via [`MagiBuilder::with_retry_disabled`]), the first
    /// schema/parse error becomes the failure reason without retry. Useful
    /// for latency-sensitive deployments where 2× worst-case timeout per
    /// agent is unacceptable.
    pub retry_on_schema_error: bool,
}

impl Default for MagiConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(300),
            max_input_len: DEFAULT_MAX_INPUT_LEN,
            completion: CompletionConfig::default(),
            retry_on_schema_error: true,
        }
    }
}

/// Type alias for the complexity-gate predicate. Erased trait object
/// shared via `Arc` so it can be cloned across spawned tasks at zero
/// runtime cost (Arc clone is a refcount bump).
///
/// Predicate signature uses `&Mode` (not `Mode` by value) so that future
/// growth of `Mode` (e.g., variants holding non-`Copy` data) does not
/// silently change predicate ergonomics. `Mode` is currently `Copy` so
/// the by-reference choice has zero runtime cost.
///
/// **Future: a fallible variant** — a `Result<bool, MagiError>`-returning
/// alternative may be added in v0.6.x if callers need predicate-supplied
/// error context. The current `bool` form is the simple-case API; it
/// will not be removed (the type alias may grow a sibling, not change).
pub(crate) type ComplexityGate = Arc<dyn Fn(&str, &Mode) -> bool + Send + Sync>;

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
    complexity_gate: Option<ComplexityGate>,
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
            complexity_gate: None,
        }
    }

    /// **v0.5.0** — Set a complexity-gate predicate. Called by
    /// [`Magi::analyze`] **after** input-size validation but before any
    /// LLM dispatch. If the predicate returns `false`, `analyze` returns
    /// [`MagiError::SkippedByComplexityGate`] without invoking the LLM
    /// (zero token cost on skipped calls).
    ///
    /// # Evaluation order
    ///
    /// `analyze` checks (in order):
    /// 1. Input length vs `max_input_len` → `MagiError::InputTooLarge`
    ///    on oversize.
    /// 2. **This gate.** Side effects (rate-limiter increments, cache
    ///    lookups) ONLY fire on inputs that passed size validation.
    ///    Stateful predicates can safely assume the input is bounded.
    /// 3. Agent factory + nonce + dispatch.
    ///
    /// This order was chosen over "gate first" because stateful predicates
    /// (e.g., rate limiters via shared atomics) on oversize inputs would
    /// burn caller budget on inputs that would have failed validation
    /// anyway. Validate-first is the safer default.
    ///
    /// # Predicate contract
    ///
    /// The predicate receives the raw `content: &str` and `mode: &Mode`
    /// (by reference, future-proofing against non-`Copy` Mode growth).
    /// Common patterns:
    /// - Length thresholds per mode
    /// - Code-vs-prose classification heuristics
    /// - Rate limiting via shared atomic counters
    /// - Pre-flight LLM triage via cheap models (wrap async in
    ///   `pollster::block_on` consciously)
    ///
    /// Bounds: `Fn(&str, &Mode) -> bool + Send + Sync + 'static`. The
    /// closure is stored as `Arc<dyn Fn>` so it must be `Send + Sync`
    /// even though `analyze` does not currently spawn the gate call
    /// (defensive — keeps the `Magi` struct `Send + Sync`).
    ///
    /// **The predicate runs synchronously on the calling task's
    /// executor.** It must be cheap (microseconds, not milliseconds).
    /// Long-running predicates block the async runtime; offload heavy
    /// classification to a separate task in the caller or use the
    /// pre-flight LLM pattern above.
    ///
    /// Default: no gate (every `analyze` proceeds to dispatch).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::sync::Arc;
    /// # use magi_core::orchestrator::MagiBuilder;
    /// # use magi_core::schema::Mode;
    /// # fn make_provider() -> Arc<dyn magi_core::provider::LlmProvider> { unimplemented!() }
    /// let magi = MagiBuilder::new(make_provider())
    ///     .with_complexity_gate(|content, mode| match mode {
    ///         Mode::CodeReview => content.len() >= 200,
    ///         Mode::Design => content.len() >= 500,
    ///         Mode::Analysis => !content.trim().is_empty(),
    ///     })
    ///     .build()
    ///     .expect("build");
    /// ```
    ///
    /// See `docs/migration-v0.5.md` for cost-control patterns.
    pub fn with_complexity_gate<F>(mut self, predicate: F) -> Self
    where
        F: Fn(&str, &Mode) -> bool + Send + Sync + 'static,
    {
        self.complexity_gate = Some(Arc::new(predicate));
        self
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
    /// Intended for testing only — `#[cfg(test)]` gated to avoid dead-code
    /// warnings in release builds (the method is unused outside test code).
    /// The nonce is shared across all agents for a single `analyze()`
    /// invocation (one call per request).
    ///
    /// # Parameters
    /// - `rng`: A boxed [`RngLike`] implementation to use instead of the default
    ///   [`FastrandSource`].
    #[cfg(test)]
    pub(crate) fn with_rng_source(mut self, rng: Box<dyn RngLike + Send>) -> Self {
        self.rng_source = Some(rng);
        self
    }

    /// **v0.4.0** — Disable the single-shot retry on schema/parse errors.
    ///
    /// Agents whose first response fails `MagiError::Validation` or
    /// `MagiError::Deserialization` go directly to `failed_agents` without
    /// a second attempt. `retried_agents` is always empty in the resulting
    /// [`MagiReport`].
    ///
    /// Useful for latency-sensitive deployments where the 2× worst-case
    /// timeout per agent (one for the first attempt + one for the retry,
    /// each with a fresh `timeout` budget) is unacceptable.
    ///
    /// Default: retry enabled. See `docs/migration-v0.4.md` and
    /// `docs/adr/002-retry-on-schema-error.md`.
    pub fn with_retry_disabled(mut self) -> Self {
        self.config.retry_on_schema_error = false;
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
        let mut overrides = self.overrides;
        if let Some(dir) = self.prompts_dir {
            factory = factory.from_directory(&dir)?;
            // Merge filesystem-loaded prompts into overrides so that
            // `lookup_prompt` finds them during `analyze`.
            // `.or_insert_with` ensures builder-level `with_custom_prompt_for_mode`
            // wins over filesystem prompts (higher precedence).
            for ((agent, mode), prompt) in factory.custom_prompts() {
                overrides
                    .entry((*agent, Some(*mode)))
                    .or_insert_with(|| prompt.clone());
            }
        }

        let rng_source = self
            .rng_source
            .unwrap_or_else(|| Box::new(FastrandSource) as Box<dyn RngLike + Send>);

        Ok(Magi {
            config: self.config,
            agent_factory: factory,
            validator: Arc::new(Validator::with_limits(self.validation_limits)),
            consensus_engine: ConsensusEngine::new(self.consensus_config),
            formatter: ReportFormatter::with_config(self.report_config)
                .map_err(|e| MagiError::Validation(e.to_string()))?,
            overrides,
            rng_source: Arc::new(Mutex::new(rng_source)),
            complexity_gate: self.complexity_gate,
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
    /// **v0.4.0** — wrapped in `Arc` (was bare `Validator`) so the dispatch
    /// layer can share it across spawned tasks without per-task deep clones.
    /// Validator's compiled regexes are amortized over the lifetime of
    /// the Magi instance instead of being rebuilt per `analyze()` call.
    /// See MAGI R1 W6/W14.
    validator: Arc<Validator>,
    consensus_engine: ConsensusEngine,
    formatter: ReportFormatter,
    overrides: BTreeMap<(AgentName, Option<Mode>), String>,
    rng_source: Arc<Mutex<Box<dyn RngLike + Send>>>,
    /// **v0.5.0** — Caller-supplied predicate evaluated at the start of
    /// `analyze`. If `Some(p)` and `p(content, mode)` returns `false`,
    /// the call short-circuits with [`MagiError::SkippedByComplexityGate`]
    /// before any LLM dispatch. Default: `None` (no gate).
    complexity_gate: Option<ComplexityGate>,
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
    ///
    /// # Concurrency
    ///
    /// The internal `rng_source` is guarded by a `std::sync::Mutex`, so concurrent
    /// calls to `analyze()` from multiple tasks serialize on nonce generation. In
    /// practice nonce generation is a single `u128` read (~nanoseconds), so
    /// contention is negligible under typical workloads. If profiling shows this
    /// becomes a bottleneck in a multi-tenant deployment, consider wrapping `Magi`
    /// in a pool of instances (one per tenant), or await v0.4 which may expose
    /// `with_rng_source` publicly to allow a thread-local RNG strategy.
    pub async fn analyze(&self, mode: &Mode, content: &str) -> Result<MagiReport, MagiError> {
        // 1. Input validation — runs BEFORE the complexity gate so that
        //    stateful predicates (rate limiters, cache counters) do NOT
        //    fire on oversized inputs. v0.5.0 MAGI R2 W5: gate-first
        //    ordering allowed adversarial side-effect burn on inputs
        //    that would have failed validation anyway.
        if content.len() > self.config.max_input_len {
            return Err(MagiError::InputTooLarge {
                size: content.len(),
                max: self.config.max_input_len,
            });
        }

        // 2. v0.5.0 complexity gate — caller-supplied predicate runs
        //    AFTER input validation but BEFORE agent factory, nonce
        //    generation, and LLM dispatch. Short-circuit on `false`
        //    avoids the cost of all three.
        if let Some(gate) = &self.complexity_gate
            && !gate(content, mode)
        {
            return Err(MagiError::SkippedByComplexityGate {
                reason: format!(
                    "complexity gate rejected: mode={mode}, content_len={}",
                    content.len()
                ),
            });
        }

        // 3. Create agents, resolving system prompts via lookup_prompt so that
        //    overrides registered through with_custom_prompt_for_mode /
        //    with_custom_prompt_all_modes take effect.
        let agents = self
            .agent_factory
            .create_agents_with_prompts(*mode, &self.overrides);

        // 4. Build user prompt with sanitization and nonce injection.
        //    Lock is released immediately after prompt construction.
        let prompt = {
            let mut rng = self
                .rng_source
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            build_user_prompt(*mode, content, &mut **rng)?
        };

        // 5. Dispatch agents in parallel with single-shot retry on schema/parse errors.
        //    (v0.4.0 replaces launch_agents + process_results — MAGI R2 W9 atomic merge.)
        let (successful, failed_agents, retried_agents) =
            self.dispatch_with_retry(agents, &prompt).await?;

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
            retried_agents,
        })
    }

    /// Dispatches all agents in parallel via `tokio::spawn`, with each agent
    /// independently running the single-shot retry FSM via [`dispatch_one_agent`].
    ///
    /// Returns the trio `(successful, failed_agents, retried_agents)`:
    /// - `successful`: parsed+validated `AgentOutput` for each agent that
    ///   completed (first attempt or retry).
    /// - `failed_agents`: name → reason map for failures. Reasons starting
    ///   with `"retry-failed: "` indicate the retry path was exercised and
    ///   also failed.
    /// - `retried_agents`: names of agents whose first attempt triggered the
    ///   retry path, regardless of whether the retry succeeded.
    ///
    /// An [`AbortGuard`] holds abort handles for all spawned tasks. If this
    /// future is cancelled (caller drops or times out), the guard aborts
    /// every running task to prevent wasted LLM API quota.
    ///
    /// Returns `MagiError::InsufficientAgents` if fewer than the consensus
    /// engine's required minimum (typically 2) succeed. See ADR 002.
    async fn dispatch_with_retry(
        &self,
        agents: Vec<Agent>,
        user_prompt: &str,
    ) -> Result<
        (
            Vec<AgentOutput>,
            BTreeMap<AgentName, String>,
            std::collections::BTreeSet<AgentName>,
        ),
        MagiError,
    > {
        let timeout = self.config.timeout;
        let completion = self.config.completion.clone();
        let retry_enabled = self.config.retry_on_schema_error;
        let validator: Arc<Validator> = Arc::clone(&self.validator);

        let mut handles = Vec::new();
        let mut abort_handles = Vec::new();

        for agent in agents {
            let name = agent.name();
            let user_prompt_cloned = user_prompt.to_string();
            let config = completion.clone();
            let validator = Arc::clone(&validator);
            let handle = tokio::spawn(async move {
                dispatch_one_agent(
                    agent,
                    user_prompt_cloned,
                    config,
                    validator,
                    timeout,
                    retry_enabled,
                )
                .await
            });
            abort_handles.push(handle.abort_handle());
            handles.push((name, handle));
        }

        let _guard = AbortGuard(abort_handles);

        let mut successful = Vec::new();
        let mut failed = BTreeMap::new();
        let mut retried = std::collections::BTreeSet::new();
        for (name, handle) in handles {
            match handle.await {
                Ok((Ok(output), was_retried)) => {
                    successful.push(output);
                    if was_retried {
                        retried.insert(name);
                    }
                }
                Ok((Err(reason), was_retried)) => {
                    failed.insert(name, reason);
                    if was_retried {
                        retried.insert(name);
                    }
                }
                Err(join_err) => {
                    failed.insert(name, format!("panic: {join_err}"));
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

        Ok((successful, failed, retried))
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

/// Dispatch a single agent with one-shot retry on schema/parse errors.
///
/// Returns `(Result<AgentOutput, String>, bool)` — a flat tuple, no enum
/// (MAGI R1 C2/W2: avoids dead-variant + unreachable! noise):
/// - First element: `Ok(output)` on success (first or second attempt),
///   `Err(reason)` on failure.
/// - Second element: `true` if a retry attempt was made (regardless of
///   outcome), `false` otherwise. Used by orchestrator to populate
///   [`MagiReport::retried_agents`] telemetry.
///
/// Retry trigger: `MagiError::Validation` or `MagiError::Deserialization`
/// from [`parse_and_validate`] on the first attempt. Provider errors and
/// timeouts skip retry — they're surfaced via the dedicated transient-error
/// layer ([`RetryProvider`](crate::provider::RetryProvider)) instead.
///
/// When `retry_enabled` is `false`, the retry path is skipped entirely
/// even on schema/parse errors. The first error becomes the failure reason
/// without the `retry-failed:` prefix. Used by
/// [`MagiBuilder::with_retry_disabled`] for latency-sensitive deployments.
///
/// See `docs/adr/002-retry-on-schema-error.md`.
pub(crate) async fn dispatch_one_agent(
    agent: Agent,
    user_prompt: String,
    config: CompletionConfig,
    validator: Arc<Validator>,
    timeout: Duration,
    retry_enabled: bool,
) -> (Result<AgentOutput, String>, bool) {
    // First attempt.
    let first_result = tokio::time::timeout(timeout, agent.execute(&user_prompt, &config)).await;
    let first_raw = match first_result {
        Ok(Ok(raw)) => raw,
        Ok(Err(provider_err)) => {
            return (Err(MagiError::Provider(provider_err).to_string()), false);
        }
        Err(_elapsed) => {
            return (
                Err(format!("timeout: agent timed out after {timeout:?}")),
                false,
            );
        }
    };

    // Parse + validate first response. Success exits here.
    let first_err = match parse_and_validate(&first_raw, &validator) {
        Ok(output) => return (Ok(output), false),
        Err(e) => e,
    };

    // Retry gate: only on Validation or Deserialization, and only if
    // retry_enabled (set by MagiBuilder::with_retry_disabled = false).
    let should_retry = retry_enabled
        && matches!(
            first_err,
            MagiError::Validation(_) | MagiError::Deserialization(_)
        );
    if !should_retry {
        return (Err(first_err.to_string()), false);
    }

    // Single-shot retry with corrective feedback prompt.
    let retry_prompt = build_retry_prompt(&user_prompt, &first_err.to_string());
    let second_result = tokio::time::timeout(timeout, agent.execute(&retry_prompt, &config)).await;
    let second_raw = match second_result {
        Ok(Ok(raw)) => raw,
        Ok(Err(provider_err)) => {
            return (
                Err(format!(
                    "retry-failed: {}",
                    MagiError::Provider(provider_err)
                )),
                true,
            );
        }
        Err(_elapsed) => {
            return (
                Err(format!("retry-failed: timeout after {timeout:?}")),
                true,
            );
        }
    };

    match parse_and_validate(&second_raw, &validator) {
        Ok(output) => (Ok(output), true),
        Err(e) => (Err(format!("retry-failed: {e}")), true),
    }
}

/// Two discriminator keys that mark a JSON object as an agent verdict during
/// lenient recovery. Kept to the two distinguishing keys (not the full 7-key
/// schema) so a verdict merely missing a key is still recovered and then
/// rejected by the full deserialize — preserving the single-retry path.
const VERDICT_KEYS: [&str; 2] = ["agent", "verdict"];

/// Upper bound on input size eligible for lenient prose recovery. Above this,
/// the input is almost certainly echoed tool-use content rather than a clean
/// verdict, and scanning it risks the O(n^2) decode worst case — so recovery
/// is skipped and the agent fails closed (and is retried).
const LENIENT_RECOVERY_MAX_CHARS: usize = 1_000_000;

/// Hard cap on candidate `{` positions probed during recovery, bounding the
/// scan against adversarial deeply-nested-unterminated input. A legitimate
/// verdict is found within the first few probes.
const MAX_BRACE_PROBES: usize = 2_000;

/// Returns the *sole* embedded JSON object carrying the verdict discriminator
/// keys ([`VERDICT_KEYS`]), or `None` when zero qualify, two or more qualify
/// (ambiguous), or the probe budget is exhausted.
///
/// Scans `{` positions and decodes one complete JSON value per position with a
/// streaming decoder, so any prose trailing a value is ignored and nested
/// braces / braces inside strings are handled without hand-rolled counting.
/// Selection is schema-aware, not span-based: a large JSON document an agent
/// echoes from tool use cannot shadow the verdict because it lacks the
/// discriminator keys. Ambiguity (two qualifying objects) returns `None` so
/// the caller fails closed rather than risk a fabricated verdict entering
/// consensus. The scan is bounded by [`MAX_BRACE_PROBES`].
///
/// Port of Python MAGI v2.4.2 `_embedded_verdict_object`.
fn embedded_verdict_object(text: &str) -> Option<serde_json::Value> {
    let mut matches: Vec<serde_json::Value> = Vec::new();
    let mut index = 0;
    let mut probes = 0;
    while index < text.len() && probes < MAX_BRACE_PROBES {
        let Some(rel) = text[index..].find('{') else {
            break;
        };
        let brace = index + rel;
        probes += 1;
        // Streaming decode parses one complete value and reports where it
        // ended, so trailing prose after the object does not fail the parse.
        let mut stream =
            serde_json::Deserializer::from_str(&text[brace..]).into_iter::<serde_json::Value>();
        match stream.next() {
            Some(Ok(value)) => {
                let end = brace + stream.byte_offset();
                if value.is_object() && VERDICT_KEYS.iter().all(|key| value.get(key).is_some()) {
                    matches.push(value);
                    if matches.len() > 1 {
                        return None; // ambiguous — fail closed rather than guess
                    }
                }
                // `end` lands on a value boundary (a char boundary); advance
                // past it, guarding a zero-width decode from pinning the scan.
                index = if end > brace { end } else { brace + 1 };
            }
            // Decode failure (incl. serde_json's recursion limit on deeply
            // nested input) — skip this `{` and continue.
            _ => index = brace + 1,
        }
    }
    // At most one match survives (the second triggers the early return above),
    // so this yields the sole verdict object or `None` when none qualified.
    matches.pop()
}

/// Extracts an [`AgentOutput`] from raw LLM response text.
///
/// Handles common LLM output quirks:
/// 1. Strips code fences (` ```json ` and ` ``` `).
/// 2. On failure, recovers the sole verdict object embedded in prose.
/// 3. Deserializes via serde (unknown fields are ignored).
///
/// This tolerates prose before and after the JSON payload but fails closed
/// when recovery is ambiguous (two verdict-shaped objects) or the input
/// exceeds the recovery budget — see [`embedded_verdict_object`].
///
/// # Errors
/// Returns `MagiError::Deserialization` if no single valid verdict object is
/// recovered.
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

    // Fallback: an agent doing multi-turn tool use may wrap the verdict in
    // prose. Recover the sole embedded verdict object, but skip oversized
    // input (likely echoed tool-use content; a scan hazard) so it fails
    // closed and the orchestrator retries instead. The 2-key candidate is
    // re-checked against the full 7-key schema; a partial object falls
    // through to Deserialization, preserving the retry path.
    if stripped.len() <= LENIENT_RECOVERY_MAX_CHARS
        && let Some(value) = embedded_verdict_object(stripped)
        && let Ok(output) = serde_json::from_value::<AgentOutput>(value)
    {
        return Ok(output);
    }

    Err(MagiError::Deserialization(
        "no valid JSON object found in agent response".to_string(),
    ))
}

/// Parse a raw agent response and validate the resulting `AgentOutput`
/// against the supplied [`Validator`]. Returns the parsed output on
/// success, or one of the two error variants that trigger retry in the
/// dispatch layer (T07):
/// - `MagiError::Deserialization` from `parse_agent_response`
/// - `MagiError::Validation` from `validator.validate_mut`
///
/// Other `MagiError` variants are not produced here; the retry gate
/// matches only these two.
pub(crate) fn parse_and_validate(
    raw: &str,
    validator: &Validator,
) -> Result<AgentOutput, MagiError> {
    let mut output = parse_agent_response(raw)?;
    validator.validate_mut(&mut output)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompts::lookup_prompt;
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
        // v0.4.0: explicit retry-disabled to preserve v0.3.1 single-shot test
        // semantics. With retry enabled, MockProvider's modulo-cycling
        // response queue would return melchior's valid response for the
        // retry, producing duplicate-agent rejection rather than the
        // intended degradation behavior. RoutingMockProvider exists for
        // retry-aware tests; this test predates v0.4 and is intentionally
        // scoped to the no-retry path.
        let magi = MagiBuilder::new(provider as Arc<dyn LlmProvider>)
            .with_retry_disabled()
            .build()
            .expect("build");

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
        // v0.4.0: see comment in test_analyze_one_agent_bad_json_degrades_gracefully.
        let magi = MagiBuilder::new(provider as Arc<dyn LlmProvider>)
            .with_retry_disabled()
            .build()
            .expect("build");

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

    // -- v0.5.0: with_complexity_gate tests --

    use std::sync::atomic::{AtomicUsize as AtomicUsizeV05, Ordering as OrderingV05};

    /// Gate returning true allows analyze to proceed normally.
    #[tokio::test]
    async fn test_complexity_gate_allows_when_predicate_returns_true() {
        let provider = Arc::new(
            RoutingMockProvider::new()
                .with_agent_responses(
                    AgentName::Melchior,
                    vec![Ok(mock_agent_json("melchior", "approve", 0.9))],
                )
                .with_agent_responses(
                    AgentName::Balthasar,
                    vec![Ok(mock_agent_json("balthasar", "approve", 0.85))],
                )
                .with_agent_responses(
                    AgentName::Caspar,
                    vec![Ok(mock_agent_json("caspar", "approve", 0.95))],
                ),
        );
        let magi = MagiBuilder::new(provider as Arc<dyn LlmProvider>)
            .with_complexity_gate(|_content, _mode| true)
            .build()
            .expect("build");
        let report = magi
            .analyze(&Mode::CodeReview, "fn main() {}")
            .await
            .unwrap();
        assert_eq!(report.agents.len(), 3);
    }

    /// Gate returning false short-circuits with SkippedByComplexityGate error
    /// and the provider is NEVER called (zero LLM cost).
    #[tokio::test]
    async fn test_complexity_gate_blocks_when_predicate_returns_false() {
        let calls = Arc::new(AtomicUsizeV05::new(0));
        let calls_for_provider = Arc::clone(&calls);
        struct CountingProvider {
            counter: Arc<AtomicUsizeV05>,
        }
        #[async_trait::async_trait]
        impl LlmProvider for CountingProvider {
            async fn complete(
                &self,
                _s: &str,
                _u: &str,
                _c: &CompletionConfig,
            ) -> Result<String, ProviderError> {
                self.counter.fetch_add(1, OrderingV05::SeqCst);
                Ok(String::new())
            }
            fn name(&self) -> &str {
                "count"
            }
            fn model(&self) -> &str {
                "x"
            }
        }
        let provider = Arc::new(CountingProvider {
            counter: calls_for_provider,
        });
        let magi = MagiBuilder::new(provider as Arc<dyn LlmProvider>)
            .with_complexity_gate(|_content, _mode| false)
            .build()
            .expect("build");
        let result = magi.analyze(&Mode::CodeReview, "fn main() {}").await;
        assert!(matches!(
            result,
            Err(MagiError::SkippedByComplexityGate { .. })
        ));
        // Critical: the provider must NEVER have been called.
        assert_eq!(
            calls.load(OrderingV05::SeqCst),
            0,
            "complexity gate must short-circuit BEFORE any LLM dispatch"
        );
    }

    /// Gate predicate sees the exact content and mode passed to analyze.
    #[tokio::test]
    async fn test_complexity_gate_receives_correct_content_and_mode() {
        use std::sync::Mutex;
        let captured: Arc<Mutex<Option<(String, Mode)>>> = Arc::new(Mutex::new(None));
        let captured_for_gate = Arc::clone(&captured);
        let provider = Arc::new(
            RoutingMockProvider::new()
                .with_agent_responses(
                    AgentName::Melchior,
                    vec![Ok(mock_agent_json("melchior", "approve", 0.9))],
                )
                .with_agent_responses(
                    AgentName::Balthasar,
                    vec![Ok(mock_agent_json("balthasar", "approve", 0.85))],
                )
                .with_agent_responses(
                    AgentName::Caspar,
                    vec![Ok(mock_agent_json("caspar", "approve", 0.95))],
                ),
        );
        let magi = MagiBuilder::new(provider as Arc<dyn LlmProvider>)
            .with_complexity_gate(move |content, mode| {
                // mode: &Mode (v0.5.0 sig); deref to store the Copy value.
                let mut g = captured_for_gate.lock().unwrap();
                *g = Some((content.to_string(), *mode));
                true
            })
            .build()
            .expect("build");
        let _ = magi
            .analyze(&Mode::Analysis, "needle-content-marker")
            .await
            .unwrap();
        let g = captured.lock().unwrap();
        let (content, mode) = g.as_ref().expect("gate was called");
        assert_eq!(content, "needle-content-marker");
        assert_eq!(*mode, Mode::Analysis);
    }

    /// Default (no gate set) preserves v0.4.x behavior — analyze proceeds.
    #[tokio::test]
    async fn test_complexity_gate_default_no_gate_preserves_v04_behavior() {
        let provider = Arc::new(
            RoutingMockProvider::new()
                .with_agent_responses(
                    AgentName::Melchior,
                    vec![Ok(mock_agent_json("melchior", "approve", 0.9))],
                )
                .with_agent_responses(
                    AgentName::Balthasar,
                    vec![Ok(mock_agent_json("balthasar", "approve", 0.85))],
                )
                .with_agent_responses(
                    AgentName::Caspar,
                    vec![Ok(mock_agent_json("caspar", "approve", 0.95))],
                ),
        );
        // Magi::new path — no gate configured.
        let magi = Magi::new(provider as Arc<dyn LlmProvider>);
        let report = magi.analyze(&Mode::CodeReview, "x").await.unwrap();
        assert_eq!(report.agents.len(), 3);
    }

    /// Stateful closure: rate limiter that blocks after N calls.
    #[tokio::test]
    async fn test_complexity_gate_stateful_rate_limiter() {
        let calls = Arc::new(AtomicUsizeV05::new(0));
        let calls_for_gate = Arc::clone(&calls);
        let limit = 2;
        let provider = Arc::new(
            RoutingMockProvider::new()
                .with_agent_responses(
                    AgentName::Melchior,
                    vec![
                        Ok(mock_agent_json("melchior", "approve", 0.9)),
                        Ok(mock_agent_json("melchior", "approve", 0.9)),
                    ],
                )
                .with_agent_responses(
                    AgentName::Balthasar,
                    vec![
                        Ok(mock_agent_json("balthasar", "approve", 0.85)),
                        Ok(mock_agent_json("balthasar", "approve", 0.85)),
                    ],
                )
                .with_agent_responses(
                    AgentName::Caspar,
                    vec![
                        Ok(mock_agent_json("caspar", "approve", 0.95)),
                        Ok(mock_agent_json("caspar", "approve", 0.95)),
                    ],
                ),
        );
        let magi = MagiBuilder::new(provider as Arc<dyn LlmProvider>)
            .with_complexity_gate(move |_content, _mode| {
                let n = calls_for_gate.fetch_add(1, OrderingV05::SeqCst);
                n < limit
            })
            .build()
            .expect("build");

        assert!(magi.analyze(&Mode::Analysis, "a").await.is_ok());
        assert!(magi.analyze(&Mode::Analysis, "b").await.is_ok());
        let third = magi.analyze(&Mode::Analysis, "c").await;
        assert!(matches!(
            third,
            Err(MagiError::SkippedByComplexityGate { .. })
        ));
    }

    /// Reason string from the gate is propagated through the error variant.
    #[tokio::test]
    async fn test_complexity_gate_error_includes_synthesized_reason() {
        let provider = Arc::new(RoutingMockProvider::new());
        let magi = MagiBuilder::new(provider as Arc<dyn LlmProvider>)
            .with_complexity_gate(|content, _mode| content.len() >= 100)
            .build()
            .expect("build");
        let err = magi.analyze(&Mode::Analysis, "short").await.unwrap_err();
        match err {
            // `..` rest pattern matches the documented #[non_exhaustive] contract
            // on the variant (see error.rs). Downstream callers MUST use this
            // pattern; in-crate code can match exhaustively but uses `..` here
            // for consistency with the documented user-facing pattern.
            MagiError::SkippedByComplexityGate { reason, .. } => {
                // Loop 1 I2: tightened from `contains("content_len") ||
                // contains("len")` — the loose disjunct would silently
                // accept regressions to unrelated strings containing "len".
                assert!(
                    reason.contains("content_len"),
                    "reason should contain exactly 'content_len'; got: {reason}"
                );
                assert!(
                    reason.contains("mode="),
                    "reason should contain 'mode='; got: {reason}"
                );
            }
            other => panic!("expected SkippedByComplexityGate, got: {other:?}"),
        }
    }

    /// MAGI R2 W5 invariant: stateful predicate side effects MUST NOT fire
    /// when input fails `max_input_len` validation. Validates-first
    /// ordering means oversize inputs hit `InputTooLarge` before the
    /// gate is ever evaluated. Critical for rate limiters: a budget-burner
    /// adversary sending oversized payloads cannot deplete the caller's
    /// quota without ever triggering an LLM call.
    #[tokio::test]
    async fn test_complexity_gate_does_not_fire_on_oversized_input() {
        let gate_calls = Arc::new(AtomicUsizeV05::new(0));
        let gate_calls_for_closure = Arc::clone(&gate_calls);
        let provider = Arc::new(RoutingMockProvider::new());

        let magi = MagiBuilder::new(provider as Arc<dyn LlmProvider>)
            .with_max_input_len(10) // tiny cap to force InputTooLarge
            .with_complexity_gate(move |_content, _mode| {
                gate_calls_for_closure.fetch_add(1, OrderingV05::SeqCst);
                true
            })
            .build()
            .expect("build");

        let oversized = "X".repeat(1_000); // far exceeds 10-byte cap
        let result = magi.analyze(&Mode::Analysis, &oversized).await;

        assert!(
            matches!(result, Err(MagiError::InputTooLarge { .. })),
            "must return InputTooLarge, got: {result:?}"
        );
        assert_eq!(
            gate_calls.load(OrderingV05::SeqCst),
            0,
            "gate MUST NOT fire on oversize input — side effects must not run"
        );
    }

    // -- T08: integration tests via Magi::analyze --

    /// BDD-03: Melchior fails first attempt with empty JSON, recovers on
    /// retry. retried_agents contains Melchior, failed_agents empty.
    #[tokio::test]
    async fn test_analyze_populates_retried_agents_on_recovery() {
        let provider = Arc::new(
            RoutingMockProvider::new()
                .with_agent_responses(
                    AgentName::Melchior,
                    vec![
                        Ok("{}".to_string()),
                        Ok(mock_agent_json("melchior", "approve", 0.9)),
                    ],
                )
                .with_agent_responses(
                    AgentName::Balthasar,
                    vec![Ok(mock_agent_json("balthasar", "approve", 0.85))],
                )
                .with_agent_responses(
                    AgentName::Caspar,
                    vec![Ok(mock_agent_json("caspar", "approve", 0.95))],
                ),
        );
        let magi = Magi::new(provider as Arc<dyn LlmProvider>);
        let report = magi
            .analyze(&Mode::CodeReview, "fn main() {}")
            .await
            .unwrap();

        assert!(
            report.failed_agents.is_empty(),
            "failed: {:?}",
            report.failed_agents
        );
        assert_eq!(report.retried_agents.len(), 1);
        assert!(report.retried_agents.contains(&AgentName::Melchior));
        assert_eq!(report.agents.len(), 3);
    }

    /// BDD-05: Caspar fails both attempts; lands in failed_agents AND
    /// retried_agents. Degraded mode triggers (2/3 agents).
    #[tokio::test]
    async fn test_analyze_retry_also_fails_lands_in_both_sets() {
        let provider = Arc::new(
            RoutingMockProvider::new()
                .with_agent_responses(
                    AgentName::Caspar,
                    vec![Ok("bad".to_string()), Ok("still bad".to_string())],
                )
                .with_agent_responses(
                    AgentName::Melchior,
                    vec![Ok(mock_agent_json("melchior", "approve", 0.9))],
                )
                .with_agent_responses(
                    AgentName::Balthasar,
                    vec![Ok(mock_agent_json("balthasar", "approve", 0.85))],
                ),
        );
        let magi = Magi::new(provider as Arc<dyn LlmProvider>);
        let report = magi.analyze(&Mode::CodeReview, "x").await.unwrap();

        assert_eq!(report.agents.len(), 2);
        assert!(report.failed_agents.contains_key(&AgentName::Caspar));
        assert!(
            report.failed_agents[&AgentName::Caspar].starts_with("retry-failed: "),
            "got: {}",
            report.failed_agents[&AgentName::Caspar]
        );
        assert!(report.retried_agents.contains(&AgentName::Caspar));
        assert!(report.degraded);
    }

    /// BDD-06: Provider timeout for Balthasar — no retry, retried_agents empty.
    #[tokio::test]
    async fn test_analyze_no_retry_on_timeout_keeps_retried_empty() {
        let provider = Arc::new(
            RoutingMockProvider::new()
                .with_agent_responses(
                    AgentName::Balthasar,
                    vec![Err(ProviderError::Timeout {
                        message: "t".to_string(),
                    })],
                )
                .with_agent_responses(
                    AgentName::Melchior,
                    vec![Ok(mock_agent_json("melchior", "approve", 0.9))],
                )
                .with_agent_responses(
                    AgentName::Caspar,
                    vec![Ok(mock_agent_json("caspar", "approve", 0.95))],
                ),
        );
        let magi = Magi::new(provider as Arc<dyn LlmProvider>);
        let report = magi.analyze(&Mode::CodeReview, "x").await.unwrap();

        assert_eq!(report.agents.len(), 2);
        assert!(report.failed_agents.contains_key(&AgentName::Balthasar));
        assert!(
            report.retried_agents.is_empty(),
            "no retry on timeout; got: {:?}",
            report.retried_agents
        );
    }

    /// MAGI R2 W6 / spec [D-24]: MagiBuilder::with_retry_disabled() bypasses
    /// the retry layer end-to-end. Melchior's first invalid response becomes
    /// the failure reason WITHOUT "retry-failed:" prefix; the sentinel in
    /// the second slot must never be consumed.
    #[tokio::test]
    async fn test_analyze_with_retry_disabled_skips_retry() {
        let provider = Arc::new(
            RoutingMockProvider::new()
                .with_agent_responses(
                    AgentName::Melchior,
                    vec![
                        Ok("{}".to_string()),                 // invalid
                        Ok("MUST NOT BE CALLED".to_string()), // sentinel
                    ],
                )
                .with_agent_responses(
                    AgentName::Balthasar,
                    vec![Ok(mock_agent_json("balthasar", "approve", 0.85))],
                )
                .with_agent_responses(
                    AgentName::Caspar,
                    vec![Ok(mock_agent_json("caspar", "approve", 0.95))],
                ),
        );
        let magi = MagiBuilder::new(provider as Arc<dyn LlmProvider>)
            .with_retry_disabled()
            .build()
            .expect("build");
        let report = magi.analyze(&Mode::CodeReview, "x").await.unwrap();

        assert_eq!(report.agents.len(), 2);
        assert!(report.failed_agents.contains_key(&AgentName::Melchior));
        assert!(
            report.retried_agents.is_empty(),
            "retry disabled => no retry telemetry"
        );
        // MAGI R3 Melchior: tighten — must NOT see retry-failed prefix.
        let mel_reason = &report.failed_agents[&AgentName::Melchior];
        assert!(
            !mel_reason.starts_with("retry-failed:"),
            "disabled retry MUST NOT produce retry-failed: prefix. Got: {mel_reason}"
        );
    }

    // -- T07: dispatch_one_agent retry FSM + BDD-19 no-retry suite --

    use crate::agent::CURRENT_AGENT_IDENTITY;
    use crate::test_support::RoutingMockProvider;

    /// First attempt succeeds: result Ok, retried=false.
    #[tokio::test]
    async fn test_dispatch_one_agent_success_first_attempt_no_retry() {
        let valid = mock_agent_json("melchior", "approve", 0.9);
        let provider = Arc::new(
            RoutingMockProvider::new().with_agent_responses(AgentName::Melchior, vec![Ok(valid)]),
        );
        let agent = Agent::new(AgentName::Melchior, provider as Arc<dyn LlmProvider>);
        let validator = Arc::new(Validator::new());
        let cfg = CompletionConfig::default();

        let (result, retried) = dispatch_one_agent(
            agent,
            "MODE: code-review\n---BEGIN USER CONTEXT n---\nx\n---END USER CONTEXT n---"
                .to_string(),
            cfg,
            validator,
            Duration::from_secs(30),
            true,
        )
        .await;

        assert!(result.is_ok());
        assert!(!retried);
    }

    /// First attempt validation-error, retry succeeds: result Ok, retried=true.
    #[tokio::test]
    async fn test_dispatch_one_agent_retries_on_validation_error_and_succeeds() {
        let bad = r#"{"agent":"melchior"}"#.to_string();
        let good = mock_agent_json("melchior", "approve", 0.9);
        let provider = Arc::new(
            RoutingMockProvider::new()
                .with_agent_responses(AgentName::Melchior, vec![Ok(bad), Ok(good)]),
        );
        let agent = Agent::new(AgentName::Melchior, provider as Arc<dyn LlmProvider>);
        let validator = Arc::new(Validator::new());
        let cfg = CompletionConfig::default();

        let (result, retried) = dispatch_one_agent(
            agent,
            "MODE: code-review\n---BEGIN USER CONTEXT n---\nx\n---END USER CONTEXT n---"
                .to_string(),
            cfg,
            validator,
            Duration::from_secs(30),
            true,
        )
        .await;

        assert!(result.is_ok(), "got: {result:?}");
        assert!(retried);
    }

    /// First and second attempts both fail parsing: result Err with
    /// "retry-failed:" prefix, retried=true.
    #[tokio::test]
    async fn test_dispatch_one_agent_retries_on_deserialization_and_fails() {
        let provider = Arc::new(RoutingMockProvider::new().with_agent_responses(
            AgentName::Caspar,
            vec![
                Ok("not json {{{".to_string()),
                Ok("still not json".to_string()),
            ],
        ));
        let agent = Agent::new(AgentName::Caspar, provider as Arc<dyn LlmProvider>);
        let validator = Arc::new(Validator::new());
        let cfg = CompletionConfig::default();

        let (result, retried) = dispatch_one_agent(
            agent,
            "MODE: design\n---BEGIN USER CONTEXT n---\nx\n---END USER CONTEXT n---".to_string(),
            cfg,
            validator,
            Duration::from_secs(30),
            true,
        )
        .await;

        assert!(result.is_err());
        let reason = result.unwrap_err();
        assert!(reason.starts_with("retry-failed: "), "got: {reason}");
        assert!(retried);
    }

    /// MAGI R1 W5 / BDD-19: provider timeout does NOT trigger retry.
    #[tokio::test]
    async fn test_dispatch_one_agent_does_not_retry_on_provider_timeout() {
        let provider = Arc::new(RoutingMockProvider::new().with_agent_responses(
            AgentName::Balthasar,
            vec![
                Err(ProviderError::Timeout {
                    message: "t".to_string(),
                }),
                Ok("MUST NOT BE CALLED".to_string()), // sentinel
            ],
        ));
        let agent = Agent::new(AgentName::Balthasar, provider as Arc<dyn LlmProvider>);
        let validator = Arc::new(Validator::new());
        let cfg = CompletionConfig::default();

        let (result, retried) = dispatch_one_agent(
            agent,
            "p".to_string(),
            cfg,
            validator,
            Duration::from_secs(30),
            true,
        )
        .await;

        assert!(result.is_err());
        let reason = result.unwrap_err();
        assert!(reason.to_lowercase().contains("timeout"));
        assert!(!retried, "provider errors must NOT trigger retry");
    }

    /// BDD-19: HTTP 500 does not retry.
    #[tokio::test]
    async fn test_dispatch_one_agent_does_not_retry_on_http_500() {
        let provider = Arc::new(RoutingMockProvider::new().with_agent_responses(
            AgentName::Caspar,
            vec![Err(ProviderError::Http {
                status: 500,
                body: "ISE".to_string(),
            })],
        ));
        let agent = Agent::new(AgentName::Caspar, provider as Arc<dyn LlmProvider>);
        let validator = Arc::new(Validator::new());
        let cfg = CompletionConfig::default();
        let (result, retried) = dispatch_one_agent(
            agent,
            "p".to_string(),
            cfg,
            validator,
            Duration::from_secs(30),
            true,
        )
        .await;
        assert!(result.is_err());
        assert!(
            !retried,
            "HTTP 500 must NOT retry — RetryProvider handles transient HTTP"
        );
    }

    /// BDD-19: HTTP 429 does not retry.
    #[tokio::test]
    async fn test_dispatch_one_agent_does_not_retry_on_http_429() {
        let provider = Arc::new(RoutingMockProvider::new().with_agent_responses(
            AgentName::Melchior,
            vec![Err(ProviderError::Http {
                status: 429,
                body: "rate".to_string(),
            })],
        ));
        let agent = Agent::new(AgentName::Melchior, provider as Arc<dyn LlmProvider>);
        let validator = Arc::new(Validator::new());
        let cfg = CompletionConfig::default();
        let (result, retried) = dispatch_one_agent(
            agent,
            "p".to_string(),
            cfg,
            validator,
            Duration::from_secs(30),
            true,
        )
        .await;
        assert!(result.is_err());
        assert!(!retried);
    }

    /// BDD-19: Auth error does not retry.
    #[tokio::test]
    async fn test_dispatch_one_agent_does_not_retry_on_auth_error() {
        let provider = Arc::new(RoutingMockProvider::new().with_agent_responses(
            AgentName::Balthasar,
            vec![Err(ProviderError::Auth {
                message: "401".to_string(),
            })],
        ));
        let agent = Agent::new(AgentName::Balthasar, provider as Arc<dyn LlmProvider>);
        let validator = Arc::new(Validator::new());
        let cfg = CompletionConfig::default();
        let (result, retried) = dispatch_one_agent(
            agent,
            "p".to_string(),
            cfg,
            validator,
            Duration::from_secs(30),
            true,
        )
        .await;
        assert!(result.is_err());
        assert!(!retried);
    }

    /// BDD-19: NestedSession error does not retry.
    #[tokio::test]
    async fn test_dispatch_one_agent_does_not_retry_on_nested_session() {
        let provider = Arc::new(
            RoutingMockProvider::new()
                .with_agent_responses(AgentName::Caspar, vec![Err(ProviderError::NestedSession)]),
        );
        let agent = Agent::new(AgentName::Caspar, provider as Arc<dyn LlmProvider>);
        let validator = Arc::new(Validator::new());
        let cfg = CompletionConfig::default();
        let (result, retried) = dispatch_one_agent(
            agent,
            "p".to_string(),
            cfg,
            validator,
            Duration::from_secs(30),
            true,
        )
        .await;
        assert!(result.is_err());
        assert!(!retried);
    }

    /// BDD-19: Network error does not retry.
    #[tokio::test]
    async fn test_dispatch_one_agent_does_not_retry_on_network_error() {
        let provider = Arc::new(RoutingMockProvider::new().with_agent_responses(
            AgentName::Melchior,
            vec![Err(ProviderError::Network {
                message: "dns".to_string(),
            })],
        ));
        let agent = Agent::new(AgentName::Melchior, provider as Arc<dyn LlmProvider>);
        let validator = Arc::new(Validator::new());
        let cfg = CompletionConfig::default();
        let (result, retried) = dispatch_one_agent(
            agent,
            "p".to_string(),
            cfg,
            validator,
            Duration::from_secs(30),
            true,
        )
        .await;
        assert!(result.is_err());
        assert!(!retried);
    }

    /// BDD-08: first attempt validation error → retry hits provider error.
    /// retried=true must be preserved (telemetry semantics).
    #[tokio::test]
    async fn test_dispatch_one_agent_retry_then_provider_error_marks_retried() {
        let provider = Arc::new(RoutingMockProvider::new().with_agent_responses(
            AgentName::Caspar,
            vec![
                Ok("{}".to_string()), // validation error
                Err(ProviderError::Timeout {
                    message: "t2".to_string(),
                }),
            ],
        ));
        let agent = Agent::new(AgentName::Caspar, provider as Arc<dyn LlmProvider>);
        let validator = Arc::new(Validator::new());
        let cfg = CompletionConfig::default();

        let (result, retried) = dispatch_one_agent(
            agent,
            "MODE: x\n---BEGIN USER CONTEXT n---\nx\n---END USER CONTEXT n---".to_string(),
            cfg,
            validator,
            Duration::from_secs(30),
            true,
        )
        .await;

        assert!(result.is_err());
        let reason = result.unwrap_err();
        assert!(reason.starts_with("retry-failed: "), "got: {reason}");
        assert!(retried);
    }

    /// MAGI R2 W6 / spec [D-24]: with_retry_disabled bypasses the retry.
    /// First validation-error becomes the failure reason WITHOUT
    /// "retry-failed:" prefix. The second slot's sentinel is never consumed.
    #[tokio::test]
    async fn test_dispatch_one_agent_retry_disabled_skips_retry_path() {
        let provider = Arc::new(RoutingMockProvider::new().with_agent_responses(
            AgentName::Melchior,
            vec![
                Ok("{}".to_string()),                 // validation error
                Ok("MUST NOT BE CALLED".to_string()), // sentinel
            ],
        ));
        let agent = Agent::new(AgentName::Melchior, provider as Arc<dyn LlmProvider>);
        let validator = Arc::new(Validator::new());
        let cfg = CompletionConfig::default();

        // retry_enabled=false
        let (result, retried) = dispatch_one_agent(
            agent,
            "p".to_string(),
            cfg,
            validator,
            Duration::from_secs(30),
            false,
        )
        .await;

        assert!(result.is_err());
        let reason = result.unwrap_err();
        assert!(
            !reason.starts_with("retry-failed:"),
            "disabled retry must NOT produce retry-failed: prefix; got: {reason}"
        );
        assert!(!retried, "retry disabled => retried=false");
        // Silence unused warning for CURRENT_AGENT_IDENTITY (used elsewhere).
        let _ = &CURRENT_AGENT_IDENTITY;
    }

    // -- T06: parse_and_validate helper --

    /// parse_and_validate returns Ok for valid JSON + valid agent output.
    #[test]
    fn test_parse_and_validate_ok_for_valid_json() {
        let validator = Validator::new();
        let raw = mock_agent_json("melchior", "approve", 0.9);
        let out = parse_and_validate(&raw, &validator).unwrap();
        assert_eq!(out.agent, AgentName::Melchior);
    }

    /// parse_and_validate surfaces MagiError::Deserialization on bad JSON.
    /// This is the variant that triggers retry in dispatch_one_agent (T07).
    #[test]
    fn test_parse_and_validate_returns_deserialization_for_bad_json() {
        let validator = Validator::new();
        let raw = "not json at all {{{";
        let err = parse_and_validate(raw, &validator).unwrap_err();
        assert!(
            matches!(err, MagiError::Deserialization(_)),
            "expected Deserialization, got: {err:?}"
        );
    }

    /// parse_and_validate surfaces MagiError::Validation when schema fields are valid
    /// JSON but fail validator rules (e.g., confidence out of range).
    /// This is the other variant that triggers retry in dispatch_one_agent (T07).
    #[test]
    fn test_parse_and_validate_returns_validation_for_out_of_range_confidence() {
        let validator = Validator::new();
        // confidence > 1.0 violates Validator rules.
        let raw = r#"{"agent":"melchior","verdict":"approve","confidence":1.5,"summary":"s","reasoning":"r","findings":[],"recommendation":"rec"}"#;
        let err = parse_and_validate(raw, &validator).unwrap_err();
        assert!(
            matches!(err, MagiError::Validation(_)),
            "expected Validation, got: {err:?}"
        );
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

    // -- v0.6.0 prose-wrapped JSON recovery (port of Python MAGI v2.4.2) --
    //
    // Gap A: recover the verdict when prose TRAILS the JSON object.
    // Gap B: fail closed when two verdict-shaped objects are present
    //        (ambiguous — picking either risks a fabricated verdict).
    // Gap C: bound the recovery scan (size budget + probe cap) against
    //        oversized / adversarial input.

    /// Gap A — recover the JSON verdict when natural-language prose follows it.
    /// Agents doing multi-turn tool use sometimes append a closing sentence
    /// after the JSON object; a strict whole-string parse rejects the trailing
    /// text, so the embedded object must be recovered.
    #[test]
    fn test_parse_agent_response_recovers_json_with_trailing_prose() {
        let json = mock_agent_json("melchior", "approve", 0.9);
        let raw = format!("{json}\n\nThat concludes my analysis.");

        let output = parse_agent_response(&raw).expect("should recover JSON before trailing prose");
        assert_eq!(output.agent, AgentName::Melchior);
        assert_eq!(output.verdict, Verdict::Approve);
    }

    /// Gap A (Rust-specific) — trailing prose containing multi-byte UTF-8
    /// (em dash U+2014, ellipsis U+2026) must not panic the post-JSON byte
    /// offset arithmetic / slicing. Python is immune (code-point indexed);
    /// Rust slices by byte, so this pins char-boundary safety.
    #[test]
    fn test_parse_agent_response_recovers_json_with_multibyte_trailing_prose() {
        let json = mock_agent_json("balthasar", "conditional", 0.8);
        let raw = format!("{json}\n\nConcluido — fin del analisis\u{2026}");

        let output =
            parse_agent_response(&raw).expect("should recover before multi-byte trailing prose");
        assert_eq!(output.agent, AgentName::Balthasar);
    }

    /// Gap B — two complete verdict-shaped objects are ambiguous, so the parser
    /// must fail closed rather than return one. Here the fabricated `approve`
    /// example follows the real `reject` verdict: a first/last-match heuristic
    /// would leak the fabricated verdict into consensus.
    #[test]
    fn test_parse_agent_response_fails_closed_when_example_follows_verdict() {
        let real = mock_agent_json("melchior", "reject", 0.9);
        let echoed = mock_agent_json("melchior", "approve", 0.9);
        let raw = format!("My verdict:\n{real}\n\nFor reference the schema is:\n{echoed}");

        let result = parse_agent_response(&raw);
        assert!(
            result.is_err(),
            "two verdict-shaped objects are ambiguous -> fail closed"
        );
    }

    /// Gap B — same ambiguity with the quoted schema example PRECEDING the real
    /// verdict. Both orderings must fail closed.
    #[test]
    fn test_parse_agent_response_fails_closed_when_example_precedes_verdict() {
        let echoed = mock_agent_json("balthasar", "approve", 0.9);
        let real = mock_agent_json("balthasar", "reject", 0.9);
        let raw = format!("For reference:\n{echoed}\n\nMy actual verdict:\n{real}");

        let result = parse_agent_response(&raw);
        assert!(
            result.is_err(),
            "ambiguous multi-verdict output -> fail closed"
        );
    }

    /// Gap C — input beyond the recovery size budget is not scanned; recovery
    /// is skipped and the parser fails closed. A multi-MB blob is almost
    /// certainly echoed tool-use content, and scanning risks the O(n^2)
    /// raw-decode worst case.
    #[test]
    fn test_parse_agent_response_skips_recovery_for_oversized_input() {
        let json = mock_agent_json("melchior", "approve", 0.9);
        // Exceeds LENIENT_RECOVERY_MAX_CHARS (1_000_000 bytes) on its own.
        let filler = "x".repeat(1_000_001);
        let raw = format!("{filler}\n\n{json}");

        let result = parse_agent_response(&raw);
        assert!(
            result.is_err(),
            "oversized input must skip recovery and fail closed"
        );
    }

    /// Gap C — the brace scan stops after a bounded number of probes. A verdict
    /// placed after more than MAX_BRACE_PROBES (2_000) lone `{` is not reached,
    /// guarding against O(n^2) on adversarial deeply-nested-unterminated input.
    #[test]
    fn test_parse_agent_response_bounds_brace_scan() {
        let json = mock_agent_json("melchior", "approve", 0.9);
        let lone_braces = "{".repeat(2_005);
        let raw = format!("{lone_braces}{json}");

        let result = parse_agent_response(&raw);
        assert!(
            result.is_err(),
            "a verdict beyond the probe budget must not be recovered"
        );
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
        /// RAII guard that removes a directory on drop, ensuring cleanup even on panic.
        struct TmpDir(std::path::PathBuf);
        impl Drop for TmpDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        // Build a collision-resistant name: PID + nanosecond timestamp.
        let uniq = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let tmp = TmpDir(std::env::temp_dir().join(format!(
            "magi_v03_test_{}_{}",
            std::process::id(),
            uniq
        )));
        std::fs::create_dir_all(&tmp.0).unwrap();

        // Create a temp dir with a custom melchior prompt file.
        std::fs::write(
            tmp.0.join("melchior_code_review.md"),
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
            .with_prompts_dir(tmp.0.clone())
            .build()
            .expect("build should succeed");
        let _ = magi.analyze(&Mode::CodeReview, "x").await.unwrap();

        let calls = captured.lock().unwrap();
        assert!(
            calls.iter().any(|(sys, _)| sys == "CUSTOM FROM FILESYSTEM"),
            "with_prompts_dir file-based prompt should reach Melchior"
        );
        // tmp is dropped here, removing the directory automatically.
    }

    /// All three agents must receive the same nonce in their user_prompt for a
    /// single `analyze` invocation (RF-10).
    ///
    /// Regression guard: if the RNG is called more than once per `analyze`
    /// each agent would receive a different nonce, breaking injection-fence
    /// isolation guarantees.
    #[tokio::test]
    async fn test_analyze_shares_same_nonce_across_all_three_agents() {
        let captured: Arc<std::sync::Mutex<Vec<(String, String)>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let provider = Arc::new(CapturingMockProvider::for_default_prompts(captured.clone()));
        let fixed: u128 = 0xabcd_ef01_2345_6789_0000_0000_0000_0001;
        let expected_nonce = format!("{fixed:032x}");
        let magi = MagiBuilder::new(provider as Arc<dyn LlmProvider>)
            .with_rng_source(Box::new(crate::user_prompt::FixedRng::new(vec![fixed])))
            .build()
            .expect("build should succeed");
        let _ = magi.analyze(&Mode::Analysis, "hello").await.unwrap();
        let calls = captured.lock().unwrap();
        assert_eq!(calls.len(), 3, "expected 3 agent calls per analyze");
        for (idx, (_, up)) in calls.iter().enumerate() {
            assert!(
                up.contains(&expected_nonce),
                "call {idx} user_prompt missing expected nonce"
            );
        }
    }
}
