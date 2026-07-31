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
use crate::reporting::{
    ExtractionFailure, InputSize, MagiReport, ReportConfig, ReportFormatter,
    TOKENS_PER_BYTE_DIVISOR, estimate_tokens,
};
use crate::rotation::{
    ActiveEntry, AgentRotation, AgentRotationState, AgentSlotGuard, FallbackPool, Lineage,
    LineageRegistry, ModelCapability, ProviderProbe, RotationConfig, RotationEvent, RotationKind,
    RotationPolicy, digest_collision, run_preflight,
};
use crate::schema::{AgentName, AgentOutput, Mode};
use crate::user_prompt::{FastrandSource, RngLike, build_retry_prompt, build_user_prompt};
use crate::validate::{ValidationLimits, Validator};
use crate::verdict_markers::ExtractionFailureCause;
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

/// Default estimated-token count above which `analyze` warns: 150 000.
///
/// See [`MagiConfig::input_warn_tokens`] for why this warns rather than rejects, and for when to
/// raise it.
pub const DEFAULT_INPUT_WARN_TOKENS: usize = 150_000;

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

    /// Estimated input size, in **TOKENS**, above which `analyze` emits a warning.
    ///
    /// Default: [`DEFAULT_INPUT_WARN_TOKENS`].
    ///
    /// # Warns. Never rejects.
    ///
    /// Crossing this marks [`MagiReport::input_size`] and emits a `tracing::warn!`; the analysis
    /// runs to completion either way. The field that **rejects** is [`max_input_len`], and the
    /// two are deliberately different things sitting next to each other:
    ///
    /// | Field | Unit | Effect |
    /// |---|---|---|
    /// | [`max_input_len`] | **bytes** | rejects |
    /// | `input_warn_tokens` | **tokens** | warns |
    ///
    /// **The units differ, and that is a trap worth naming.** Any comparison between them has to
    /// convert (see [`TOKENS_PER_BYTE_DIVISOR`]); a refactor that treats them as the same scale
    /// produces a check that means nothing.
    ///
    /// # `0` warns always — it does not disable
    ///
    /// Zero is literally zero, so any non-empty input exceeds it. There is no sentinel value and
    /// no off switch: the report field is always computed, and only *when it warns* is
    /// configurable. To silence the warning, set it high — `build()` will then tell you once
    /// that your threshold can never fire, which is precisely what you asked for.
    ///
    /// # Calibrating it
    ///
    /// The default is ~15% of the 4 MB hard bound: a reasonable signal for a ~200k-context model,
    /// and **premature** for a 1M-context one, where it would warn about inputs the model digests
    /// without effort. Raise it for large-window models. The default favours the common case —
    /// warning too early is cheap, warning too late is not.
    ///
    /// [`max_input_len`]: MagiConfig::max_input_len
    /// [`MagiReport::input_size`]: crate::reporting::MagiReport::input_size
    /// [`TOKENS_PER_BYTE_DIVISOR`]: crate::reporting::TOKENS_PER_BYTE_DIVISOR
    pub input_warn_tokens: usize,
}

/// True when `content`'s estimate exceeds `cfg`'s warning threshold.
///
/// The same predicate `analyze` uses, extracted so it is testable without a run.
///
/// # Parameters
/// - `content`: the analysis input.
/// - `cfg`: the configuration whose `input_warn_tokens` applies.
pub(crate) fn exceeds_warn_threshold(content: &str, cfg: &MagiConfig) -> bool {
    // Strictly greater: at exactly the threshold nothing is wrong yet. It also means an empty
    // input never warns, not even against a threshold of 0.
    estimate_tokens(content) > cfg.input_warn_tokens
}

/// True when the warning threshold can never fire, because the validator rejects first.
///
/// # Parameters
/// - `cfg`: the configuration to inspect.
///
/// # Why this is reported rather than corrected
///
/// Such a threshold leaves the telemetry mute with nobody the wiser. Silently clamping it would
/// substitute our guess for the caller's stated intent; saying so once at `build()` does not.
pub(crate) fn warn_threshold_is_unreachable(cfg: &MagiConfig) -> bool {
    // The two fields are in DIFFERENT UNITS — tokens here, bytes there — so the comparison has
    // to convert, and converting up (tokens → bytes) is what keeps the operands honest.
    //
    // Saturating, because the interesting input is an absurd threshold: `usize::MAX * 4` wraps
    // to a small number, which would answer "reachable" for the single most unreachable value
    // there is — the exact inversion this check exists to catch.
    cfg.input_warn_tokens
        .saturating_mul(TOKENS_PER_BYTE_DIVISOR)
        >= cfg.max_input_len
}

impl Default for MagiConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(300),
            max_input_len: DEFAULT_MAX_INPUT_LEN,
            completion: CompletionConfig::default(),
            retry_on_schema_error: true,
            input_warn_tokens: DEFAULT_INPUT_WARN_TOKENS,
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
    /// per-agent declared primary lineage (rotation diversity key).
    agent_lineages: BTreeMap<AgentName, Lineage>,
    /// probes declared on probing primaries (`with_probing_agent`).
    primary_probes: BTreeMap<AgentName, Arc<dyn ProviderProbe>>,
    /// the shared fallback pool; `None` ⇒ rotation disabled (2.0.x path).
    fallback_pool: Option<FallbackPool>,
    /// reject candidates whose context window can't be measured.
    strict_context_guard: bool,
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
            agent_lineages: BTreeMap::new(),
            primary_probes: BTreeMap::new(),
            fallback_pool: None,
            strict_context_guard: false,
        }
    }

    /// Registers an agent's primary provider AND its declared lineage
    /// (the rotation diversity key). A `Lineage` is trimmed at construction; an
    /// empty/blank one is rejected at [`build`](Self::build).
    pub fn with_agent(
        mut self,
        agent: AgentName,
        provider: Arc<dyn LlmProvider>,
        lineage: Lineage,
    ) -> Self {
        self.agent_providers.insert(agent, provider);
        self.agent_lineages.insert(agent, lineage);
        self.primary_probes.remove(&agent); // a plain primary declares no probe
        self
    }

    /// Like [`with_agent`](Self::with_agent) but the primary also
    /// declares a [`ProviderProbe`]: the preflight can then resolve its window and
    /// digest. `Arc<P>` is coerced to both trait objects (no downcast; `LlmProvider`
    /// untouched — G4). A down probe never blocks rotation (fail-open).
    pub fn with_probing_agent<P: LlmProvider + ProviderProbe + 'static>(
        mut self,
        agent: AgentName,
        provider: Arc<P>,
        lineage: Lineage,
    ) -> Self {
        let llm: Arc<dyn LlmProvider> = provider.clone();
        let probe: Arc<dyn ProviderProbe> = provider;
        self.agent_providers.insert(agent, llm);
        self.agent_lineages.insert(agent, lineage);
        self.primary_probes.insert(agent, probe);
        self
    }

    /// Declares the shared fallback pool. Without it, rotation is
    /// disabled and behavior is identical to 2.0.x.
    pub fn with_fallback_pool(mut self, pool: FallbackPool) -> Self {
        self.fallback_pool = Some(pool);
        self
    }

    /// When enabled, a fallback candidate whose context window cannot be
    /// measured by its probe is REJECTED during rotation. Default `false`
    /// (an unmeasured window is eligible; the definitive probe decides later).
    pub fn with_strict_context_guard(mut self, strict: bool) -> Self {
        self.strict_context_guard = strict;
        self
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
    /// Default: retry enabled.
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

    /// Sets the estimated-token count above which `analyze` warns.
    ///
    /// # Parameters
    /// - `tokens`: the threshold, in **tokens** (not bytes). `0` warns on every non-empty
    ///   input; it does not disable the warning.
    ///
    /// This never causes an input to be rejected — see [`MagiConfig::input_warn_tokens`].
    pub fn with_input_warn_tokens(mut self, tokens: usize) -> Self {
        self.config.input_warn_tokens = tokens;
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
        // A warning threshold the validator would reject before can never fire, leaving the
        // telemetry mute with nobody the wiser. Say it once, here — and do NOT clamp it: that
        // would substitute our guess for what the caller actually asked for.
        if warn_threshold_is_unreachable(&self.config) {
            tracing::warn!(
                input_warn_tokens = self.config.input_warn_tokens,
                max_input_len = self.config.max_input_len,
                "input warning threshold can never fire: the size limit rejects first"
            );
        }

        // MS2 (R3.2) — VALIDITY: reject empty/blank declared lineages before anything
        // else. This is malformed input (a `Lineage` is a declared label), distinct
        // from the G2 diversity warning below; it fires even for a single-provider config.
        for (agent, lineage) in &self.agent_lineages {
            if lineage.as_str().is_empty() {
                return Err(MagiError::InvalidInput {
                    reason: format!(
                        "lineage for primary {} must be a non-empty declared label",
                        agent.display_name()
                    ),
                });
            }
        }
        if let Some(pool) = &self.fallback_pool {
            for (i, fc) in pool.candidates().iter().enumerate() {
                if fc.lineage.as_str().is_empty() {
                    return Err(MagiError::InvalidInput {
                        reason: format!(
                            "lineage for fallback pool candidate {i} must be a non-empty declared label"
                        ),
                    });
                }
            }
        }
        // MS2 (G2) — DIVERSITY is a warning, never an error: two primaries may share
        // a lineage (a single-provider industrial user runs fine).
        {
            let mut seen = std::collections::BTreeSet::new();
            for (agent, lineage) in &self.agent_lineages {
                if !seen.insert(lineage.clone()) {
                    tracing::warn!(
                        agent = agent.display_name(),
                        lineage = lineage.as_str(),
                        "duplicate primary lineage (reduced rotation diversity, not fatal)"
                    );
                }
            }
        }

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

        // MS3 (R14) — THE PROMPT CONTRACT GUARD. Every RESOLVABLE prompt must carry the
        // verdict-marker block: the three embedded ones and every override, including
        // those loaded from `prompts_dir`.
        //
        // Placement: after the overrides merge above, so filesystem-loaded prompts are
        // covered too — those are exactly the ones a consumer is most likely to get
        // wrong. `AgentFactory::new` only stored an `Arc`; **no provider has been
        // called**, and returning `Err` here means none ever is (E20).
        //
        // This does NOT trigger retry or rotation: a stale prompt is not fixed by asking
        // the model again. It is a sibling of the validation path, not a child of it.
        //
        // The Python lesson this exists to avoid: the guard existed, was tested, and
        // NOBODY CALLED IT.
        for (agent, mode) in [AgentName::Melchior, AgentName::Balthasar, AgentName::Caspar]
            .into_iter()
            .map(|a| (a, None))
        {
            crate::prompts::validate_prompt_for(
                Some(agent),
                mode,
                crate::prompts::embedded_prompt_for(agent),
            )?;
        }
        for ((agent, mode), prompt) in &overrides {
            crate::prompts::validate_prompt_for(Some(*agent), *mode, prompt)?;
        }

        let rng_source = self
            .rng_source
            .unwrap_or_else(|| Box::new(FastrandSource) as Box<dyn RngLike + Send>);

        // MS2: engage the rotation subsystem when the user declares a fallback pool
        // OR probing primaries — the latter want window/digest measurement (and the
        // `ran_unmeasured` honesty flag) even with no pool to rotate into. Declaring
        // NOTHING reproduces 2.0.x behavior exactly (R11/S1).
        let engage_rotation = self.fallback_pool.is_some() || !self.primary_probes.is_empty();
        let rotation_config = engage_rotation.then(|| {
            Arc::new(RotationConfig {
                primary_lineages: self.agent_lineages,
                primary_probes: self.primary_probes,
                strict_context_guard: self.strict_context_guard,
                pool: self
                    .fallback_pool
                    .unwrap_or_else(|| FallbackPool::builder().build()),
            })
        });

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
            rotation_config,
        })
    }
}

/// RAII guard that aborts spawned tasks when dropped.
///
/// Ensures that if [`Magi::analyze`] is cancelled (e.g., the caller wraps it
/// in `tokio::time::timeout`), all in-flight agent tasks are aborted instead
/// of continuing to run in the background and consuming LLM API quota.
/// The full result of dispatching the trio: successful outputs, failure reasons,
/// the set of agents that hit the corrective retry, and the per-agent rotation
/// telemetry (populated for EVERY agent — successful or failed).
type DispatchOutcome = (
    Vec<AgentOutput>,
    BTreeMap<AgentName, String>,
    std::collections::BTreeSet<AgentName>,
    BTreeMap<AgentName, AgentRotation>,
    // MS3 — per-agent rejected outputs, seeded for every dispatched agent so a clean
    // seat certifies itself with an empty Vec. Read joined with the rotations above.
    BTreeMap<AgentName, Vec<ExtractionFailure>>,
);

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
    ///
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
    /// rotation configuration (primaries' lineages/probes + fallback
    /// pool). `None` ⇒ rotation disabled (2.0.x path). Read by `dispatch_with_retry`
    /// to route between the no-rotation and rotation dispatch paths.
    rotation_config: Option<Arc<RotationConfig>>,
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
    ///   per call; fastrand effective state ~64 bits).
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

        // 2.5. Measure the input. Telemetry only: this can mark the report and emit a warning,
        //      and it can NEVER stop the run. The single place that rejects on size is the
        //      `max_input_len` check in step 1, and it has already run.
        let input_size = InputSize {
            estimated_tokens: estimate_tokens(content),
            warn_threshold: self.config.input_warn_tokens,
            exceeded: exceeds_warn_threshold(content, &self.config),
        };
        if input_size.exceeded {
            tracing::warn!(
                estimated_tokens = input_size.estimated_tokens,
                warn_threshold = input_size.warn_threshold,
                "input exceeds the configured warning threshold; continuing"
            );
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
        let (successful, failed_agents, retried_agents, rotations, extraction_failures) =
            self.dispatch_with_retry(agents, &prompt).await?;

        // 6. Consensus
        let consensus = self.consensus_engine.determine(&successful)?;

        // 7. Report. The formatter assembles the whole report — banner, the MS2
        //    rotation sections, then the consensus sections — so the orchestrator
        //    never couples to the banner/section layout (no string-splicing). On a
        //    plain run (no rotation, nothing estimated) the output is byte-identical
        //    to 2.0.x (R11).
        let banner = self.formatter.format_banner(&successful, &consensus);
        let estimated = successful
            .iter()
            .any(|o| rotations.get(&o.agent).is_some_and(|r| r.ran_unmeasured));
        let report = self.formatter.format_report_with_input_size(
            &successful,
            &consensus,
            &rotations,
            estimated,
            &extraction_failures,
            Some(&input_size),
        );

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
            rotations,
            extraction_failures,
            input_size: Some(input_size),
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
    /// engine's required minimum (typically 2) succeed.
    async fn dispatch_with_retry(
        &self,
        agents: Vec<Agent>,
        user_prompt: &str,
    ) -> Result<DispatchOutcome, MagiError> {
        // MS2: rotation is engaged ONLY when a fallback pool was declared. With no
        // pool (`rotation_config == None`) the dispatch path is byte-identical to
        // 2.0.x — same FSM, same failure strings, no registry, no endpoint-down
        // (R11/S1). Each agent's configured model seeds a present, chain-empty
        // telemetry record so `rotations` is populated on both paths.
        let agent_models: BTreeMap<AgentName, String> = agents
            .iter()
            .map(|a| (a.name(), a.provider_model().to_string()))
            .collect();
        match self.rotation_config.clone() {
            None => {
                self.dispatch_no_rotation(agents, user_prompt, agent_models)
                    .await
            }
            Some(rotation) => {
                self.dispatch_with_rotation(agents, user_prompt, agent_models, rotation)
                    .await
            }
        }
    }

    /// The 2.0.x dispatch path (no rotation): one `tokio::spawn` per agent running
    /// the original single-shot retry FSM ([`dispatch_one_agent`]). Preserves every
    /// observable behavior of 2.0.x — this is what a consumer that declares no
    /// fallbacks gets. `rotations` is filled with default (chain-empty) records.
    async fn dispatch_no_rotation(
        &self,
        agents: Vec<Agent>,
        user_prompt: &str,
        agent_models: BTreeMap<AgentName, String>,
    ) -> Result<DispatchOutcome, MagiError> {
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
        // SEEDED for every dispatched agent, so a clean seat says so with an empty Vec
        // instead of vanishing from the report. See `MagiReport::extraction_failures`.
        let mut extraction_failures: BTreeMap<AgentName, Vec<ExtractionFailure>> = agent_models
            .keys()
            .map(|name| (*name, Vec::new()))
            .collect();
        for (name, handle) in handles {
            match handle.await {
                Ok((Ok(output), was_retried, failures)) => {
                    successful.push(output);
                    if was_retried {
                        retried.insert(name);
                    }
                    extraction_failures.insert(name, failures);
                }
                Ok((Err(reason), was_retried, failures)) => {
                    failed.insert(name, reason);
                    if was_retried {
                        retried.insert(name);
                    }
                    extraction_failures.insert(name, failures);
                }
                Err(join_err) => {
                    // A panicked task loses its in-flight records; the pre-seeded empty
                    // Vec stands, and the panic itself is the headline in `failed_agents`.
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

        let rotations = default_rotations(agent_models);
        Ok((successful, failed, retried, rotations, extraction_failures))
    }

    /// The rotation dispatch path. Seeds a per-run [`LineageRegistry`] from the
    /// trio's declared primary lineages (or a synthetic per-agent lineage when a
    /// pool is declared but a primary's lineage was not), spawns the rotation FSM
    /// ([`dispatch_one_agent_rotating`]) per agent, collects the real per-agent
    /// [`AgentRotation`] chains, and enforces the endpoint-down fast-fail: after
    /// EVERY agent outcome (success, failure, OR panic/`JoinError`) it consults the
    /// registry latch and, if set, returns `Err(EndpointDown)` **before** consensus
    /// — the single source of truth, robust to a panicked latch-holder.
    async fn dispatch_with_rotation(
        &self,
        agents: Vec<Agent>,
        user_prompt: &str,
        agent_models: BTreeMap<AgentName, String>,
        rotation: Arc<RotationConfig>,
    ) -> Result<DispatchOutcome, MagiError> {
        let timeout = self.config.timeout;
        let completion = self.config.completion.clone();
        let retry_enabled = self.config.retry_on_schema_error;
        let validator: Arc<Validator> = Arc::clone(&self.validator);

        // Seed the registry with each agent's active (lineage, model). A declared
        // primary lineage wins; otherwise a synthetic per-agent label keeps the
        // "one lineage, one mage" invariant well-formed (agent names are distinct)
        // and lets an un-declared primary still rotate into the pool.
        let mut initial: BTreeMap<AgentName, ActiveEntry> = BTreeMap::new();
        let mut primary_lineages: BTreeMap<AgentName, Lineage> = BTreeMap::new();
        for (name, model) in &agent_models {
            let lineage = rotation
                .primary_lineages
                .get(name)
                .cloned()
                .unwrap_or_else(|| Lineage::new(format!("__primary::{}", name.display_name())));
            initial.insert(
                *name,
                ActiveEntry {
                    lineage: lineage.clone(),
                    model: model.clone(),
                },
            );
            primary_lineages.insert(*name, lineage);
        }
        let registry = Arc::new(LineageRegistry::new(initial));

        // Preflight (R15): probe every probe-capable model (trio primaries + pool
        // candidates) ONCE, CONCURRENTLY, before dispatch — caching window/digest so
        // the pure rotation policy reads them with zero I/O and never under the lock.
        // A failed/timed-out probe degrades to unmeasured (fail-open, G3) — never an
        // abort. Providers without a probe contribute nothing (no window/digest).
        let capabilities =
            Arc::new(run_preflight(collect_probe_targets(&agent_models, &rotation)).await);
        // G2: warn (never error) if two primaries resolve to the SAME weights digest
        // — reduced ensemble diversity, but the run proceeds. Diversity never blocks
        // the run; only a PROVEN collision during rotation (R5a) rejects a candidate.
        let trio_digests: Vec<Option<String>> = agent_models
            .values()
            .map(|m| capabilities.get(m).and_then(|c| c.digest.clone()))
            .collect();
        if digest_collision(&trio_digests).is_some() {
            tracing::warn!(
                "two primary mages resolve to the same weights digest \
                 (reduced ensemble diversity, not fatal)"
            );
        }
        // Coarse lower bound on the raw payload (R16): reject only candidates whose
        // measured window is smaller than the prompt itself would need. `chars/4` is
        // the standard rough token estimate — a pre-filter, not precise budgeting.
        let min_window_tokens = user_prompt.chars().count().div_ceil(CHARS_PER_TOKEN_EST);
        let strict_context_guard = rotation.strict_context_guard;

        // Pre-seed telemetry OUTSIDE any task stack so a panicked agent still has a
        // present, chain-empty record (W1). A normal return replaces its entry.
        let mut rotations = default_rotations(agent_models);
        // Seeded per agent: an empty Vec is the positive certificate that the seat was
        // clean, and it keeps this map joinable with the rotations map on the same key.
        let mut extraction_failures: BTreeMap<AgentName, Vec<ExtractionFailure>> =
            rotations.keys().map(|name| (*name, Vec::new())).collect();

        let mut handles = Vec::new();
        let mut abort_handles = Vec::new();
        for agent in agents {
            let name = agent.name();
            let model_configured = rotations
                .get(&name)
                .map(|r| r.model_configured.clone())
                .unwrap_or_default();
            let primary_lineage = primary_lineages
                .get(&name)
                .cloned()
                .unwrap_or_else(|| Lineage::new("__primary::unknown"));
            let user_prompt_cloned = user_prompt.to_string();
            let config = completion.clone();
            let validator = Arc::clone(&validator);
            let registry = Arc::clone(&registry);
            let rotation = Arc::clone(&rotation);
            let capabilities = Arc::clone(&capabilities);
            let handle = tokio::spawn(async move {
                dispatch_one_agent_rotating(
                    agent,
                    user_prompt_cloned,
                    config,
                    validator,
                    timeout,
                    retry_enabled,
                    registry,
                    rotation,
                    primary_lineage,
                    model_configured,
                    capabilities,
                    strict_context_guard,
                    min_window_tokens,
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
        // ABNORMAL EXIT: the endpoint-down latch — NOT the per-agent error payload —
        // is the single source of truth, so it MUST be consulted after EVERY outcome
        // (success, normal failure, OR panic) before ANY return/continue. Do not drop
        // this check in a refactor; a panicked latch-holder that never propagated the
        // signal is recovered here (R8/W11).
        //
        // ABORT LATENCY (documented, not a bug): handles are awaited in a fixed order,
        // so the latch is only OBSERVED once the currently-awaited agent's task
        // resolves. For the endpoint-down TARGET — a shared destination (one Ollama
        // daemon, R7/G1) — a dead endpoint yields FAST connection-refused failures on
        // ALL mages simultaneously, so the abort fires promptly. In a multi-host
        // deployment (already a documented caveat, README (a)), one mage could sit on
        // a slow-but-alive host while two others connection-fail, delaying the abort
        // by that mage's timeout. The run stays CORRECT — it still aborts (and the
        // `AbortGuard` cancels the stragglers) — only the fast-fail *latency* grows.
        // Optimizing that out-of-scope multi-host case is deliberately not done here.
        for (name, handle) in handles {
            match handle.await {
                Ok((Ok(output), agent_rotation, was_retried, failures)) => {
                    rotations.insert(name, agent_rotation);
                    extraction_failures.insert(name, failures);
                    successful.push(output);
                    if was_retried {
                        retried.insert(name);
                    }
                }
                Ok((Err(reason), agent_rotation, was_retried, failures)) => {
                    rotations.insert(name, agent_rotation);
                    extraction_failures.insert(name, failures);
                    failed.insert(name, reason);
                    if was_retried {
                        retried.insert(name);
                    }
                }
                Err(join_err) => {
                    // Panic/abnormal: keep the pre-seed (empty chain — panic never
                    // rotates, R6). The lineage is already freed by the task's
                    // `AgentSlotGuard::drop` during unwind (never `mark_succeeded`).
                    // Lost-signal recovery (W11/W18): recover endpoint-down straight
                    // from the registry latch, robust to a panicked carrier.
                    failed.insert(name, format!("panic: {join_err}"));
                    if let Some(err) = resolve_abnormal_exit(name, &join_err, &registry).await {
                        return Err(err);
                    }
                    continue;
                }
            }
            // Normal outcome: a concurrent mage may still have tripped the latch.
            if let Some(err) = resolve_endpoint_down(&registry).await {
                return Err(err);
            }
        }

        let min_agents = self.consensus_engine.min_agents();
        if successful.len() < min_agents {
            return Err(MagiError::InsufficientAgents {
                succeeded: successful.len(),
                required: min_agents,
            });
        }

        Ok((successful, failed, retried, rotations, extraction_failures))
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
///:
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
pub(crate) async fn dispatch_one_agent(
    agent: Agent,
    user_prompt: String,
    config: CompletionConfig,
    validator: Arc<Validator>,
    timeout: Duration,
    retry_enabled: bool,
) -> (Result<AgentOutput, String>, bool, Vec<ExtractionFailure>) {
    // Attribution is STRUCTURAL here: the model is the one whose provider actually ran,
    // so a failure can never be credited to a model that had not executed yet (E23c).
    let model = agent.provider_model().to_string();
    let mut failures: Vec<ExtractionFailure> = Vec::new();
    // First attempt.
    let first_result = tokio::time::timeout(timeout, agent.execute(&user_prompt, &config)).await;
    let first_raw = match first_result {
        Ok(Ok(raw)) => raw,
        Ok(Err(provider_err)) => {
            return (
                Err(MagiError::Provider(provider_err).to_string()),
                false,
                failures,
            );
        }
        Err(_elapsed) => {
            return (
                Err(format!("timeout: agent timed out after {timeout:?}")),
                false,
                failures,
            );
        }
    };

    // Parse + validate first response. Success exits here.
    let first_err = match parse_validate_and_check(&first_raw, agent.name(), &validator) {
        Ok(output) => return (Ok(output), false, failures),
        Err(f) => f,
    };
    failures.push(ExtractionFailure {
        model: model.clone(),
        attempt: 1,
        cause: first_err.cause,
    });
    // Surface the TYPED cause the moment the output is rejected. Until MS3 the reason a
    // mage failed was invisible: a recovered retry left it in retried_agents with no
    // record of why. The structured field is the diagnosis an operator needs, and it is
    // deliberately the cause and not the message - the message is prose, the cause is a
    // value you can filter and count on.
    tracing::warn!(
        target: "magi_core::verdict",
        cause = ?first_err.cause,
        "agent output rejected; attempting one corrective retry"
    );

    // Retry gate: only on Validation or Deserialization, and only if
    // retry_enabled (set by MagiBuilder::with_retry_disabled = false).
    let should_retry = retry_enabled
        && matches!(
            first_err.error,
            MagiError::Validation(_) | MagiError::Deserialization(_)
        );
    if !should_retry {
        return (Err(first_err.error.to_string()), false, failures);
    }

    // Single-shot retry with corrective feedback prompt.
    let retry_prompt =
        build_retry_prompt(&user_prompt, first_err.cause, &first_err.error.to_string());
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
                failures,
            );
        }
        Err(_elapsed) => {
            return (
                Err(format!("retry-failed: timeout after {timeout:?}")),
                true,
                failures,
            );
        }
    };

    match parse_validate_and_check(&second_raw, agent.name(), &validator) {
        Ok(output) => (Ok(output), true, failures),
        Err(f) => {
            // `attempt: 2` — the corrective retry, on the SAME model. The counter is
            // per-model by construction, so it restarts at 1 if a rotation happens later
            // (E23c): that is what keeps "this model failed on its first try" readable.
            failures.push(ExtractionFailure {
                model,
                attempt: 2,
                cause: f.cause,
            });
            (Err(format!("retry-failed: {}", f.error)), true, failures)
        }
    }
}

/// Builds the default (chain-empty) rotation telemetry for every agent from its
/// configured model — `model_used == model_configured`, empty chain. Used on the
/// no-rotation path and as the pre-seed on the rotation path.
fn default_rotations(
    agent_models: BTreeMap<AgentName, String>,
) -> BTreeMap<AgentName, AgentRotation> {
    agent_models
        .into_iter()
        .map(|(agent, model)| {
            (
                agent,
                AgentRotation {
                    model_configured: model.clone(),
                    model_used: model,
                    chain: Vec::new(),
                    ran_unmeasured: false,
                },
            )
        })
        .collect()
}

/// Rough chars-per-token ratio for the coarse `min_window_tokens` pre-filter
///. Not precise budgeting — the crate is char-based and adds no tokenizer
/// dependency; this only rejects candidates smaller than the raw prompt needs.
const CHARS_PER_TOKEN_EST: usize = 4;

/// Collects the preflight probe targets: each probing PRIMARY (paired with its
/// agent's model) plus each pool candidate that declared a probe. Non-probing
/// providers contribute nothing — they simply have no window/digest to measure.
fn collect_probe_targets(
    agent_models: &BTreeMap<AgentName, String>,
    rotation: &RotationConfig,
) -> Vec<(String, Arc<dyn ProviderProbe>)> {
    let mut targets = Vec::new();
    for (agent, probe) in &rotation.primary_probes {
        if let Some(model) = agent_models.get(agent) {
            targets.push((model.clone(), Arc::clone(probe)));
        }
    }
    for cand in rotation.pool.candidates() {
        if let Some(probe) = &cand.probe {
            targets.push((cand.provider.model().to_string(), Arc::clone(probe)));
        }
    }
    targets
}

/// Classifies a surfaced [`ProviderError`] as a connection-level failure for the
/// endpoint-down fast-fail.
///
/// **Only [`ProviderError::Network`]** (connection refused / host unreachable /
/// DNS) counts as connection evidence. An `Http` (incl. 5xx), a `Timeout`, or a
/// `RetryAbandoned` condemns the lineage run-wide but is **not** connection
/// evidence — someone answered, or the model is merely slow.
///
/// The exclusion of [`ProviderError::RetryAbandoned`] is deliberate and is NOT a
/// bug: a truly-down endpoint yields **fast** connection-refused `Network` errors
/// that exhaust the `RetryProvider`'s retry COUNT (surfacing the last error,
/// `Network`) well before its time-`operation_budget` abandon path (which is the
/// only source of `RetryAbandoned`). The budget-abandon path fires on SLOW
/// failures, which connection-refused is not — so the `Network` branch is the one
/// that trips endpoint-down. This `match` is exhaustive so a new `ProviderError`
/// variant forces a conscious classification.
///
/// **Worst case is graceful, not a missed abort.** In the improbable event that a
/// genuinely dead endpoint surfaced only `RetryAbandoned` (never `Network`),
/// endpoint-down would not fast-fail — but the lineage is still condemned run-wide
/// and the mage rotates; the run then simply reaches `InsufficientAgents` (an
/// honest degraded result) instead of the faster `EndpointDown` abort. No mage
/// hangs and no incorrect verdict is produced — only the *speed* of the failure
/// path differs. Treating `RetryAbandoned` as a connection failure would instead
/// require inspecting `AbandonReason` + timing (a heuristic), which the design
/// deliberately
/// avoids. See the README endpoint-down runbook caveats (b)/(e).
fn is_connection(err: &ProviderError) -> bool {
    match err {
        ProviderError::Network { .. } => true,
        ProviderError::Http { .. }
        | ProviderError::Timeout { .. }
        | ProviderError::Auth { .. }
        | ProviderError::Process { .. }
        | ProviderError::NestedSession
        | ProviderError::RetryAbandoned { .. }
        // A server that answers TOO MUCH is not a server that is down: this must never feed the
        // endpoint-down latch, or one seat's content failure could abort the whole run.
        | ProviderError::ResponseTooLarge { .. } => false,
    }
}

/// Outcome of a single model attempt (including its own corrective schema retry).
enum ModelOutcome {
    /// A valid verdict was committed.
    Success(AgentOutput),
    /// Schema/parse failure after the corrective retry (or with retry disabled) —
    /// mage-local condemnation, then rotate.
    Schema(String),
    /// Transport failure (`ProviderError` surfaced by the wrapped provider) or a
    /// timeout — run-wide condemnation (a connection-class failure), then rotate. `kind`
    /// distinguishes a plain transport hop from a timeout hop for telemetry.
    Transport {
        detail: String,
        connection: bool,
        kind: RotationKind,
    },
    /// Body over the cap on a successful response — **mage-local**, then rotate.
    ///
    /// Not `Transport`: that is run-wide and feeds the endpoint-down latch. Not `Schema`: nothing
    /// failed to parse. Its own variant so the `match` below forces the consequence to be decided
    /// rather than inherited.
    OversizedResponse { limit: usize },

    /// A non-schema, non-transport failure — never rotates; surfaced verbatim.
    Unexpected(String),
}

/// Runs ONE model attempt against `provider` with this agent's identity/prompt,
/// including the single corrective schema retry (same model). Sets `*was_retried`
/// if the corrective retry fired. Never rotates — the caller decides that from the
/// returned [`ModelOutcome`].
#[allow(clippy::too_many_arguments)]
async fn attempt_model(
    agent: &Agent,
    provider: &Arc<dyn LlmProvider>,
    user_prompt: &str,
    config: &CompletionConfig,
    validator: &Validator,
    timeout: Duration,
    retry_enabled: bool,
    was_retried: &mut bool,
    failures: &mut Vec<ExtractionFailure>,
) -> ModelOutcome {
    // First attempt.
    let first =
        tokio::time::timeout(timeout, agent.execute_with(provider, user_prompt, config)).await;
    let first_raw = match first {
        Ok(Ok(raw)) => raw,
        Ok(Err(provider_err)) => return provider_err_outcome(provider_err),
        Err(_elapsed) => {
            return ModelOutcome::Transport {
                detail: format!("timeout: agent timed out after {timeout:?}"),
                connection: false,
                kind: RotationKind::Timeout,
            };
        }
    };

    let first_err = match parse_validate_and_check(&first_raw, agent.name(), validator) {
        Ok(output) => return ModelOutcome::Success(output),
        Err(f) => f,
    };
    // Attribution is STRUCTURAL: the model is the one whose provider actually ran, and
    // ttempt restarts at 1 for each model because this function handles exactly one
    // model. That is what makes E23c hold without depending on read ordering.
    failures.push(ExtractionFailure {
        model: provider.model().to_string(),
        attempt: 1,
        cause: first_err.cause,
    });
    // See the note at the non-rotating dispatch site: the typed cause is the diagnosis,
    // and on this path it also explains a rotation that would otherwise look arbitrary.
    tracing::warn!(
        target: "magi_core::verdict",
        cause = ?first_err.cause,
        "agent output rejected on this model"
    );
    let is_schema = matches!(
        first_err.error,
        MagiError::Validation(_) | MagiError::Deserialization(_)
    );
    // magi_error_for maps EVERY cause to one of those two variants, so today this
    // always holds and the Unexpected arm below is unreachable. The assert states that
    // dependency instead of leaving it implicit: if a future cause were ever mapped to a
    // third variant, the arm would silently start firing and a schema failure would stop
    // rotating — a mage lost to a mapping change nobody connected to this branch.
    debug_assert!(
        is_schema,
        "every ExtractionFailureCause must map to Validation or Deserialization; \
         see magi_error_for"
    );
    if !is_schema {
        // Not a schema failure and not transport — never rotate.
        return ModelOutcome::Unexpected(first_err.error.to_string());
    }
    if !retry_enabled {
        // Retry disabled → a schema failure rotates immediately (R6).
        return ModelOutcome::Schema(first_err.error.to_string());
    }

    // Single corrective retry on the SAME model.
    *was_retried = true;
    let retry_prompt =
        build_retry_prompt(user_prompt, first_err.cause, &first_err.error.to_string());
    let second =
        tokio::time::timeout(timeout, agent.execute_with(provider, &retry_prompt, config)).await;
    let second_raw = match second {
        Ok(Ok(raw)) => raw,
        Ok(Err(provider_err)) => return provider_err_outcome(provider_err),
        Err(_elapsed) => {
            return ModelOutcome::Transport {
                detail: format!("retry-failed: timeout after {timeout:?}"),
                connection: false,
                kind: RotationKind::Timeout,
            };
        }
    };
    match parse_validate_and_check(&second_raw, agent.name(), validator) {
        Ok(output) => ModelOutcome::Success(output),
        Err(f) => {
            failures.push(ExtractionFailure {
                model: provider.model().to_string(),
                attempt: 2,
                cause: f.cause,
            });
            ModelOutcome::Schema(format!("retry-failed: {}", f.error))
        }
    }
}

/// Maps a surfaced [`ProviderError`] to a [`ModelOutcome`].
///
/// # Why one variant is singled out
///
/// An oversized body is a **content** failure, not a transport one: the server answered perfectly,
/// it answered too much. Routing it through `Transport` would condemn the lineage **run-wide** —
/// taking it away from the other two seats over what one seat observed — and, for a connection-class
/// error, feed the endpoint-down latch. It gets its own outcome so the consequence is decided here
/// rather than inherited.
fn provider_err_outcome(err: ProviderError) -> ModelOutcome {
    if let ProviderError::ResponseTooLarge { limit } = err {
        return ModelOutcome::OversizedResponse { limit };
    }
    let connection = is_connection(&err);
    let kind = match err {
        ProviderError::Timeout { .. } => RotationKind::Timeout,
        _ => RotationKind::Transport,
    };
    ModelOutcome::Transport {
        detail: MagiError::Provider(err).to_string(),
        connection,
        kind,
    }
}

/// Returns `Some(MagiError::EndpointDown)` iff the registry's endpoint-down latch
/// is set, else `None`. The latch is the single source of truth for the fast-fail
///; the `lineages` come from the run's connection-condemned set.
async fn resolve_endpoint_down(reg: &LineageRegistry) -> Option<MagiError> {
    if reg.endpoint_down_signalled().await {
        Some(MagiError::EndpointDown {
            lineages: reg.connection_failed_lineages().await,
        })
    } else {
        None
    }
}

/// Lost-signal recovery for an ABNORMAL agent exit (panic / `JoinError`), factored
/// out for race-free unit testing.
///
/// A panicked task loses its transport classification, so the decision derives
/// **solely** from the registry latch — never from `err`. `agent`/`err` document
/// the call site (and feed a diagnostic `tracing` event); the verdict is exactly
/// [`resolve_endpoint_down`]. This catches a latch-holder that crossed the
/// endpoint-down threshold and then died before propagating the signal.
pub(crate) async fn resolve_abnormal_exit(
    agent: AgentName,
    err: &tokio::task::JoinError,
    reg: &LineageRegistry,
) -> Option<MagiError> {
    let decision = resolve_endpoint_down(reg).await;
    if decision.is_some() {
        tracing::warn!(
            agent = agent.display_name(),
            cause = %err,
            "abnormal agent exit with endpoint-down latch set; aborting run"
        );
    }
    decision
}

/// Dispatch a single agent through the rotation state machine.
///
/// Runs the agent's primary model, then — on a **transport** failure (condemned
/// run-wide) or a **schema** failure surviving its corrective retry (condemned
/// mage-local) — rotates to the next eligible fallback lineage via
/// [`LineageRegistry::claim_next`], up to the pool's `max_rotations`. A panic or a
/// non-schema/non-transport error **never rotates** and is surfaced.
///
/// The whole dispatch holds an [`AgentSlotGuard`]: success calls `mark_succeeded`
/// (the mage keeps its lineage); a normal failure explicitly `release`s then
/// `mark_released`; a panic/cancellation relies on the guard's `Drop`.
///
/// Returns `(Result<AgentOutput, String>, AgentRotation, was_retried)` — the
/// per-agent output plus its real rotation chain (empty when it never rotated).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn dispatch_one_agent_rotating(
    agent: Agent,
    user_prompt: String,
    config: CompletionConfig,
    validator: Arc<Validator>,
    timeout: Duration,
    retry_enabled: bool,
    registry: Arc<LineageRegistry>,
    rotation: Arc<RotationConfig>,
    primary_lineage: Lineage,
    model_configured: String,
    capabilities: Arc<BTreeMap<String, ModelCapability>>,
    strict_context_guard: bool,
    min_window_tokens: usize,
) -> (
    Result<AgentOutput, String>,
    AgentRotation,
    bool,
    Vec<ExtractionFailure>,
) {
    let agent_name = agent.name();
    let mut guard = AgentSlotGuard::new(Arc::clone(&registry), agent_name);

    let policy = RotationPolicy::new(
        rotation.pool.to_candidates(),
        rotation.pool.max_rotations(),
        (*capabilities).clone(),
        strict_context_guard,
        min_window_tokens,
    );

    let mut state = AgentRotationState {
        model_configured: model_configured.clone(),
        model_used: model_configured.clone(),
        chain: Vec::new(),
        used: [model_configured].into_iter().collect(),
        failed_lineages: std::collections::BTreeSet::new(),
        window_rejected: BTreeMap::new(),
        rotations_done: 0,
        ran_unmeasured: false,
    };

    let mut current_provider = agent.provider().clone();
    let mut current_lineage = primary_lineage;
    let mut was_retried = false;
    // Accumulates ACROSS rotations: each ttempt_model call appends its own model's
    // records, so the sequence reads as the seat's full history and ttempt restarts
    // at 1 per model (E23c).
    let mut failures: Vec<ExtractionFailure> = Vec::new();

    loop {
        let outcome = attempt_model(
            &agent,
            &current_provider,
            &user_prompt,
            &config,
            &validator,
            timeout,
            retry_enabled,
            &mut was_retried,
            &mut failures,
        )
        .await;

        // Success/Unexpected return directly; Schema/Transport yield the
        // `(kind, detail)` for the rotation hop after applying condemnation.
        let (kind, detail) = match outcome {
            ModelOutcome::Success(output) => {
                // R19 honesty: the committed model ran on an ESTIMATED window unless
                // a probe measured an exact one. A model with no capability entry
                // (non-probing) or a `None` window counts as unmeasured.
                state.ran_unmeasured = capabilities
                    .get(&state.model_used)
                    .and_then(|c| c.window)
                    .is_none();
                guard.mark_succeeded();
                return (Ok(output), state.to_rotation(), was_retried, failures);
            }
            ModelOutcome::Unexpected(detail) => {
                registry.release(agent_name).await;
                guard.mark_released();
                return (Err(detail), state.to_rotation(), was_retried, failures);
            }
            ModelOutcome::OversizedResponse { limit } => {
                // Mage-local, exactly like Schema: this mage will not retry this lineage, but the
                // other seats still may. Reported as `Transport` in telemetry because that enum is
                // public and not `#[non_exhaustive]` — a new variant would be a SemVer break — so
                // the precision rides in `detail` instead.
                state.failed_lineages.insert(current_lineage.clone());
                (
                    RotationKind::Transport,
                    format!("response body exceeded {limit} bytes"),
                )
            }
            ModelOutcome::Schema(detail) => {
                // Schema failure is mage-local: this mage will not retry this
                // lineage, but other mages still may.
                state.failed_lineages.insert(current_lineage.clone());
                (RotationKind::Schema, detail)
            }
            ModelOutcome::Transport {
                detail,
                connection,
                kind,
            } => {
                // Transport failure condemns the lineage run-wide (and may trip the
                // endpoint-down latch, which the collector detects).
                registry
                    .register_transport_failure(current_lineage.clone(), connection)
                    .await;
                (kind, detail)
            }
        };

        // Try to rotate to the next eligible lineage.
        match registry.claim_next(agent_name, &policy, &mut state).await {
            Some(cand) => {
                state.rotations_done += 1;
                let event = RotationEvent::new(
                    current_lineage.clone(),
                    cand.lineage.clone(),
                    cand.model.clone(),
                    kind,
                    detail,
                );
                state.chain.push(event);
                tracing::warn!(
                    agent = agent_name.display_name(),
                    from = %current_lineage,
                    to = %cand.lineage,
                    kind = %kind,
                    "mage rotated to a new lineage"
                );
                current_provider = rotation.pool.candidate(cand.provider_ix).provider.clone();
                current_lineage = cand.lineage.clone();
                state.used.insert(cand.model.clone());
                state.model_used = cand.model;
            }
            None => {
                // Needed to rotate but found no eligible candidate.
                registry.release(agent_name).await;
                guard.mark_released();
                return (
                    Err(format!("no_fitting_candidate: {detail}")),
                    state.to_rotation(),
                    was_retried,
                    failures,
                );
            }
        }
    }
}

/// Maps an [`ExtractionFailureCause`] to the `MagiError` variant it surfaces as.
///
/// **THE mapping, in one place.** Both variants trigger retry, so the choice does
/// not change control flow — it changes the **diagnosis** a reader of the report gets,
/// which is why it is pinned rather than left to whoever writes the next branch.
///
/// The line between them: `Deserialization` = *I never got an `AgentOutput`*;
/// `Validation` = *I had one and it did not survive*. The three `extract` causes and a
/// `serde_json` failure fall on the first side; the schema rejection and the two
/// post-validation checks — which run **on an already-deserialized output** — on the
/// second.
///
/// The `Other` arm exists because [`ExtractionFailureCause`] is `#[non_exhaustive]`. It
/// maps to `Deserialization`, the conservative choice: an unknown cause must not claim a
/// verdict object was obtained.
pub(crate) fn magi_error_for(cause: ExtractionFailureCause, message: &str) -> MagiError {
    match cause {
        ExtractionFailureCause::Schema
        | ExtractionFailureCause::EchoedExample
        | ExtractionFailureCause::AgentIdentity => MagiError::Validation(message.to_string()),
        ExtractionFailureCause::MissingMarkers
        | ExtractionFailureCause::Unterminated
        | ExtractionFailureCause::Ambiguous
        | ExtractionFailureCause::InvalidJson => MagiError::Deserialization(message.to_string()),
        _ => MagiError::Deserialization(message.to_string()),
    }
}

/// A parse/validate failure carrying its typed cause **alongside** the error.
///
/// The cause travels beside the error rather than inside its message on purpose: the
/// retry feedback is selected by the **type**, never by matching strings. String matching
/// is brittle (a rewording silently breaks the feedback) and is a second-order injection
/// surface. Because this type is `pub(crate)`, none of that costs any public API.
#[derive(Debug)]
pub(crate) struct ParseFailure {
    /// Why it failed. Selects the retry template and feeds the telemetry.
    pub(crate) cause: ExtractionFailureCause,
    /// The error as the dispatch layer and the report see it.
    pub(crate) error: MagiError,
}

impl ParseFailure {
    fn new(cause: ExtractionFailureCause, message: String) -> Self {
        Self {
            error: magi_error_for(cause, &message),
            cause,
        }
    }
}

/// Extracts an [`AgentOutput`] from an agent's raw response — **by extraction only**.
///
/// ```text
/// raw → verdict_markers::extract → serde_json::from_str::<AgentOutput>
///       (Err ⇒ typed cause)        (Err ⇒ InvalidJson)
/// ```
///
/// # There is no search, and no fast path
///
/// The brace-scanning recovery heuristic and its two bounds are **gone**, and so is the
/// attempt to deserialize the whole response. That fast path looked harmless — it only
/// accepted a response that was *nothing but* a valid verdict — but it is precisely the
/// door an echoed worked example walks through, and accepting naked JSON is the fallback
/// the no-search rule forbids.
///
/// **Review rule for anything added here:** the only legitimate call to
/// `serde_json::from_str` in this path is the one operating on the result of `extract`.
///
/// That claim was swept across all of `src/`: the only two call sites
/// that decode agent-produced verdict text are this one and the prompt guard's, and both
/// operate on the output of the shared delimitation. Every other occurrence either decodes
/// a **transport envelope** (`providers/*` unwrapping HTTP or CLI JSON to get at the text,
/// which then comes here) or is test code. There is no bypass path. The sweep is
/// `serde_json::from_str|from_value|from_reader` over `src/` — cheap to redo, and worth
/// redoing whenever a provider is added, because a new provider is the one place where a
/// second decode of agent text could plausibly appear.
/// Any other one breaks the no-search rule, whatever it is named. That is the
/// realistic shape a
/// regression would take — new code, new name, invisible to the symbol greps in CI — so
/// it is a rule for a human reader, not something a script can decide.
///
/// # Errors
///
/// [`ParseFailure`] whose `cause` says which stage failed and whose `error` is the
/// variant pinned for it.
fn parse_agent_response(raw: &str) -> Result<AgentOutput, ParseFailure> {
    let block = crate::verdict_markers::extract(raw)
        .map_err(|e| ParseFailure::new(e.cause(), e.to_string()))?;

    serde_json::from_str::<AgentOutput>(block).map_err(|e| {
        ParseFailure::new(
            ExtractionFailureCause::InvalidJson,
            format!("the delimited verdict block is not valid JSON: {e}"),
        )
    })
}

/// Parses an agent response and validates the resulting [`AgentOutput`].
///
/// Returns the parsed output, or a [`ParseFailure`] whose `error` is one of the two
/// variants the dispatch layer retries on — see [`magi_error_for`] for which cause maps
/// to which, and why.
pub(crate) fn parse_and_validate(
    raw: &str,
    validator: &Validator,
) -> Result<AgentOutput, ParseFailure> {
    let mut output = parse_agent_response(raw)?;
    validator
        .validate_mut(&mut output)
        .map_err(|e| ParseFailure::new(ExtractionFailureCause::Schema, e.to_string()))?;
    Ok(output)
}

/// [`parse_and_validate`] plus the two POST-VALIDATION checks, in the order that matters.
///
/// # The order is canary FIRST, identity second — and it is not arbitrary
///
/// Both run on the same validated output and **can fire together**: a mage that echoes
/// the worked example of *another* prompt matches the fingerprint **and** carries the
/// other mage's name in `agent`. Both observations are true, but only one names the root
/// cause — the model copied an example instead of analysing.
///
/// Reporting `AgentIdentity` there would tell it *"fix your agent field"*, and an obedient
/// model would **fix the name and resend the echoed example**, now with the right identity
/// and nothing left to catch it. The retry would have been spent making the problem
/// harder to see. `EchoedExample` tells it what it actually has to do — emit *its* own
/// analysis — and that feedback **subsumes** the identity fix. The reverse does not.
///
/// # Errors
///
/// [`ParseFailure`] with `EchoedExample` or `AgentIdentity`, both mapping to the
/// validation variant: an output WAS obtained, and then rejected.
pub(crate) fn parse_validate_and_check(
    raw: &str,
    dispatched_to: AgentName,
    validator: &Validator,
) -> Result<AgentOutput, ParseFailure> {
    let output = parse_and_validate(raw, validator)?;

    if output.summary == crate::prompts::ECHO_CANARY_SUMMARY
        && output.recommendation == crate::prompts::ECHO_CANARY_RECOMMENDATION
    {
        return Err(ParseFailure::new(
            ExtractionFailureCause::EchoedExample,
            "the verdict reproduces the worked example from the instructions verbatim, \
             so it is not an analysis"
                .to_string(),
        ));
    }

    // R17 asks for a case-insensitive comparison, and DESERIALIZATION is what provides
    // it: `agent` is an `AgentName`, so by the time it gets here the name has already
    // been canonicalized — a wrong-case spelling never produces an `AgentOutput` at all,
    // it fails as invalid JSON. Comparing the enums is therefore exact AND satisfies the
    // requirement; an `eq_ignore_ascii_case` on the rendered names would be dead
    // complexity suggesting a variation that cannot reach this point.
    if output.agent != dispatched_to {
        return Err(ParseFailure::new(
            ExtractionFailureCause::AgentIdentity,
            format!(
                "verdict claims to come from {} but was dispatched to {}",
                output.agent.display_name(),
                dispatched_to.display_name()
            ),
        ));
    }

    Ok(output)
}

#[cfg(test)]
mod input_threshold_tests {
    use super::*;

    #[test]
    fn a_zero_threshold_warns_always_and_does_not_disable() {
        // No sentinel values: 0 is literally zero, so any non-empty input exceeds it. A value
        // that flips the meaning of a knob is a hidden rule.
        let cfg = MagiConfig {
            input_warn_tokens: 0,
            ..Default::default()
        };
        assert!(
            exceeds_warn_threshold("some content", &cfg),
            "0 warns; it does not switch the warning off"
        );
    }

    #[test]
    fn an_empty_input_never_exceeds_even_a_zero_threshold() {
        // The one input a zero threshold does not catch, because `>` is strict. Pinned so the
        // boundary is a decision rather than an accident.
        let cfg = MagiConfig {
            input_warn_tokens: 0,
            ..Default::default()
        };
        assert!(!exceeds_warn_threshold("", &cfg));
    }

    #[test]
    fn the_default_threshold_does_not_warn_on_ordinary_input() {
        let cfg = MagiConfig::default();
        assert!(!exceeds_warn_threshold("a short review request", &cfg));
    }

    #[test]
    fn a_threshold_that_can_never_fire_is_detected() {
        // threshold * 4 >= max_input_len means the validator rejects first, so the warning is
        // mute. Saturating: an absurd threshold must not overflow into the opposite verdict.
        let cfg = MagiConfig {
            input_warn_tokens: usize::MAX,
            max_input_len: 4 * 1024 * 1024,
            ..Default::default()
        };
        assert!(
            warn_threshold_is_unreachable(&cfg),
            "a mute knob must be reported, not silently kept"
        );
    }

    #[test]
    fn the_shipped_defaults_are_not_degenerate() {
        // If this ever goes red, the crate's own defaults would emit a config warning on every
        // build — the fastest way to teach users to ignore it.
        assert!(!warn_threshold_is_unreachable(&MagiConfig::default()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompts::lookup_prompt;
    use crate::schema::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    /// What a COMPLIANT MODEL RETURNS: the 7-key verdict object wrapped in the sentinel
    /// markers, each alone on its own line.
    ///
    /// Since `3.0.0` the parser reads a verdict **only** from between the markers, so a mock
    /// that returns a bare object is no longer modelling a working agent — it is
    /// modelling one that fails extraction. Almost every test here wants a working agent,
    /// which is why this helper is the wrapped form and [`mock_agent_object`] is the
    /// escape hatch for the few that want the object itself.
    fn mock_agent_json(agent: &str, verdict: &str, confidence: f64) -> String {
        format!(
            "{}\n{}\n{}",
            crate::verdict_markers::VERDICT_OPEN,
            mock_agent_object(agent, verdict, confidence),
            crate::verdict_markers::VERDICT_CLOSE
        )
    }

    /// The bare 7-key verdict object, WITHOUT markers.
    ///
    /// For tests that need the object itself: as the payload inside a hand-built marker
    /// block, or as the thing whose bareness is the point (a model that emitted no
    /// markers must fail extraction).
    fn mock_agent_object(agent: &str, verdict: &str, confidence: f64) -> String {
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

    /// Wraps identifying text in a contract-compliant verdict-marker block.
    ///
    /// Since `3.0.0`, `build()` rejects any **resolvable** prompt that lacks exactly one
    /// ordered marker pair — that is the documented breaking change of `3.0.0`. These
    /// override-plumbing tests assert that a specific string reaches the agent as its
    /// system prompt; they are not about the contract, so they carry it via this helper
    /// and keep asserting exactly what they asserted before. The identifying text stays
    /// intact and findable.
    ///
    /// The placeholder between the markers is deliberately **not** valid JSON: a prompt
    /// whose delimited block deserializes as a verdict is a fabrication template, and
    /// the guard rejects it.
    fn contract_prompt(text: &str) -> String {
        format!(
            "{text}\n\n## Output format\n{}\n{{ ...your 7-key JSON object... }}\n{}",
            crate::verdict_markers::VERDICT_OPEN,
            crate::verdict_markers::VERDICT_CLOSE
        )
    }

    /// The worked example shipped **verbatim** inside `src/prompts_md/caspar.md`.
    ///
    /// It is a complete, valid 7-key verdict object. That is the residual the sentinel
    /// closes: an agent that echoes its own instructions emits something the
    /// current parser accepts as a verdict. `v1.1.1` reduced the severity by
    /// making it `"conditional"` instead of `"approve"`, but it still parses.
    const SHIPPED_WORKED_EXAMPLE: &str = r#"{"agent": "caspar", "verdict": "conditional", "confidence": 0.85, "summary": "One-line verdict", "reasoning": "Your risk-focused analysis", "findings": [{"severity": "warning", "title": "Short title", "detail": "Risk description with concrete scenario", "file": "src/x.py", "line": 42, "category": "logic-error"}], "recommendation": "What you recommend"}"#;

    /// CLOSED (was: the lone echoed example fabricating a verdict — variant 1 of 4, and
    /// the worst of them).
    ///
    /// The worked example lives OUTSIDE the markers in every shipped prompt, so a model
    /// that echoes it emits no marker block and nothing reaches consensus. Before the
    /// sentinel this same input parsed cleanly and produced a verdict no model ever
    /// formed — in the adversarial seat, an opinion out of thin air.
    ///
    /// Closed STRUCTURALLY: not by a check that could be forgotten, but because there is
    /// no longer any path from unmarked text to a verdict.
    #[test]
    fn test_lone_echoed_example_no_longer_fabricates_a_verdict() {
        let f = parse_and_validate(SHIPPED_WORKED_EXAMPLE, &Validator::new()).unwrap_err();
        assert_eq!(f.cause, ExtractionFailureCause::MissingMarkers);
    }

    /// CLOSED (was: truncation leaving the echoed example as the only verdict-shaped
    /// object — variant 2 of 4).
    ///
    /// There is nowhere to recover it FROM: prose around an unmarked object is not
    /// searched, so a truncated response fails instead of inventing an answer.
    #[test]
    fn test_truncation_plus_echo_no_longer_fabricates_a_verdict() {
        let raw = format!(
            "Let me restate the schema I must follow:\n{SHIPPED_WORKED_EXAMPLE}\n\nNow my analysis"
        );
        let f = parse_and_validate(&raw, &Validator::new()).unwrap_err();
        assert_eq!(f.cause, ExtractionFailureCause::MissingMarkers);
    }

    /// CLOSED (was: a real verdict placed beyond the probe cap being dropped — variant 3
    /// of 4).
    ///
    /// There is no probe cap because there is no probing, so DISTANCE NO LONGER DECIDES
    /// ANYTHING. That is what this asserts: the same unmarked object yields the same cause
    /// whether it sits at the start of the response or after thousands of braces. The
    /// witness constant the characterization needed is gone with the heuristic it mirrored.
    #[test]
    fn test_probe_distance_no_longer_decides_the_outcome() {
        let bare = mock_agent_object("melchior", "approve", 0.9);
        let noise = "{}".repeat(4_000);
        let far = parse_and_validate(&format!("{noise}\n{bare}"), &Validator::new()).unwrap_err();
        let near = parse_and_validate(&bare, &Validator::new()).unwrap_err();
        assert_eq!(far.cause, ExtractionFailureCause::MissingMarkers);
        assert_eq!(near.cause, far.cause, "distance must not change the cause");
    }

    /// CLOSED (was: a thinking model restating its schema getting DROPPED — variant 4 of
    /// 4, and the direct Ollama/Jetson win).
    ///
    /// Two verdict-shaped objects used to make recovery fail closed, killing a mage that
    /// had actually answered. Now the reasoning lives outside the markers, where it is
    /// never read, so it cannot compete with the verdict.
    ///
    /// NOTE: the input differs from the characterization on purpose. Back then the model
    /// emitted two BARE objects; under the sentinel a compliant model wraps its real
    /// verdict. Reusing the old input would assert `MissingMarkers` — i.e. that the mage
    /// is still dropped — which is the opposite of what closed.
    #[test]
    fn test_think_restatement_no_longer_drops_the_mage() {
        let restated = mock_agent_object("caspar", "approve", 0.0);
        let real = mock_agent_object("caspar", "reject", 0.8);
        let raw = format!(
            "<think>The schema is {restated}</think>\n{}\n{real}\n{}",
            crate::verdict_markers::VERDICT_OPEN,
            crate::verdict_markers::VERDICT_CLOSE
        );
        let out = parse_and_validate(&raw, &Validator::new())
            .expect("reasoning outside the markers must not compete with the verdict");
        assert_eq!(
            out.verdict,
            Verdict::Reject,
            "the REAL verdict, not the restatement"
        );
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

    // -- Task 7 (MS2): rotation builder API — R3 (declared lineage), R11 (additive) --

    /// A builder without `with_fallback_pool` behaves exactly like `2.0.x` — no
    /// rotation — yet the `rotations` map is populated for the whole trio with empty
    /// chains and `model_used == model_configured` (non-vacuous: the field is filled,
    /// not merely an empty map).
    #[tokio::test]
    async fn test_no_fallbacks_behaves_like_2_0_x() {
        let responses = vec![
            mock_agent_json("melchior", "approve", 0.9),
            mock_agent_json("balthasar", "approve", 0.85),
            mock_agent_json("caspar", "approve", 0.95),
        ];
        let provider = Arc::new(MockProvider::success("mock", "test-model", responses));
        let magi = MagiBuilder::new(provider as Arc<dyn LlmProvider>)
            .build()
            .expect("build without fallbacks must succeed");
        let report = magi
            .analyze(&Mode::CodeReview, "fn main() {}")
            .await
            .expect("analyze should succeed");
        assert_eq!(
            report.rotations.len(),
            3,
            "rotations populated for the whole trio"
        );
        for r in report.rotations.values() {
            assert!(r.chain.is_empty(), "no rotation → empty chain");
            assert_eq!(
                r.model_used, r.model_configured,
                "no rotation → used == configured"
            );
        }
    }

    /// Collects `tracing` events as text, so a test can assert one was emitted.
    ///
    /// Hand-rolled on the `tracing` facade rather than pulled from `tracing-subscriber`: the
    /// milestone adds no dependency, dev or otherwise, and the six no-op methods below are the
    /// whole price of that.
    #[derive(Clone, Default)]
    struct EventLog(Arc<std::sync::Mutex<Vec<String>>>);

    impl EventLog {
        fn lines(&self) -> Vec<String> {
            self.0
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone()
        }
    }

    /// Renders every field of an event, including the message, which `tracing` carries as a
    /// field literally named `message`.
    struct FieldWriter<'a>(&'a mut String);

    impl tracing::field::Visit for FieldWriter<'_> {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            use std::fmt::Write;
            let _ = write!(self.0, " {}={:?}", field.name(), value);
        }
    }

    impl tracing::Subscriber for EventLog {
        fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
            true
        }

        fn new_span(&self, _attrs: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            // Spans are irrelevant here; every span gets the same id and nothing reads it.
            tracing::span::Id::from_u64(1)
        }

        fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}

        fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}

        fn event(&self, event: &tracing::Event<'_>) {
            let mut line = event.metadata().level().to_string();
            event.record(&mut FieldWriter(&mut line));
            self.0
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(line);
        }

        fn enter(&self, _span: &tracing::span::Id) {}

        fn exit(&self, _span: &tracing::span::Id) {}
    }

    fn trio() -> Arc<dyn LlmProvider> {
        Arc::new(MockProvider::success(
            "mock",
            "test-model",
            vec![
                mock_agent_json("melchior", "approve", 0.9),
                mock_agent_json("balthasar", "approve", 0.85),
                mock_agent_json("caspar", "approve", 0.95),
            ],
        ))
    }

    /// R10 is warn-ONLY: the event fires **and** the analysis still produces a report.
    /// Asserting only the flag would leave "someone turned it into a rejection" undetected.
    #[tokio::test]
    async fn exceeding_the_threshold_warns_and_still_completes() {
        let log = EventLog::default();
        // `set_default` rather than `with_default`: the guard has to survive `.await`, and the
        // test runtime keeps this future on one thread.
        let _guard = tracing::subscriber::set_default(log.clone());

        let magi = MagiBuilder::new(trio())
            .with_input_warn_tokens(0)
            .build()
            .expect("builds");
        let report = magi
            .analyze(&Mode::CodeReview, "fn main() {}")
            .await
            .expect("warn-only: the analysis must still complete");

        let size = report.input_size.expect("this version always measures");
        assert!(size.exceeded, "the report states it crossed the threshold");
        assert_eq!(size.warn_threshold, 0, "and what it was compared against");
        assert!(
            !report.agents.is_empty(),
            "warn-only means the run produced verdicts anyway"
        );

        let lines = log.lines();
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("WARN") && l.contains("threshold")),
            "a warning must actually be emitted, not merely recorded in the struct: {lines:?}"
        );
    }

    /// The other side of the same requirement: under the threshold, nothing is announced.
    /// A warning that fires always is a warning nobody reads.
    #[tokio::test]
    async fn staying_under_the_threshold_announces_nothing() {
        let log = EventLog::default();
        let _guard = tracing::subscriber::set_default(log.clone());

        let magi = MagiBuilder::new(trio()).build().expect("builds");
        let report = magi
            .analyze(&Mode::CodeReview, "fn main() {}")
            .await
            .expect("analyze");

        let size = report.input_size.expect("measured even when small");
        assert!(!size.exceeded);
        assert_eq!(size.estimated_tokens, "fn main() {}".len() / 4);
        assert!(
            !log.lines().iter().any(|l| l.contains("threshold")),
            "silence below the threshold: {:?}",
            log.lines()
        );
    }

    /// S23: two primaries with the SAME lineage → `build()` succeeds (emits a
    /// WARNING), never `Err`. Diversity is advisory; a single-provider / duplicate
    /// config must run (G2).
    #[test]
    fn test_same_primary_lineage_warns_not_errors() {
        let b = MagiBuilder::new(Arc::new(MockProvider::success("d", "dm", vec!["r".into()])))
            .with_agent(
                AgentName::Melchior,
                Arc::new(MockProvider::success("m1", "m", vec!["r".into()])),
                Lineage::new("same"),
            )
            .with_agent(
                AgentName::Balthasar,
                Arc::new(MockProvider::success("m2", "m", vec!["r".into()])),
                Lineage::new("same"),
            );
        assert!(
            b.build().is_ok(),
            "duplicate-primary-lineage must not block build"
        );
    }

    /// S23b: a primary whose lineage trims to "" is malformed input →
    /// `build()` must `Err(InvalidInput)`. Validity, not diversity — runs even for a
    /// single-provider config.
    #[test]
    fn test_empty_primary_lineage_fails_build() {
        let b = MagiBuilder::new(Arc::new(MockProvider::success("d", "dm", vec!["r".into()])))
            .with_agent(
                AgentName::Melchior,
                Arc::new(MockProvider::success("m1", "m", vec!["r".into()])),
                Lineage::new("  "),
            );
        assert!(
            matches!(b.build(), Err(MagiError::InvalidInput { .. })),
            "empty/blank primary lineage is invalid input, rejected at build"
        );
    }

    /// S23b: a pool candidate with an empty lineage is caught at
    /// `MagiBuilder::build()` (which sees the pool), not at pool construction.
    #[test]
    fn test_empty_pool_lineage_fails_build() {
        let pool = FallbackPool::builder()
            .push(
                Arc::new(MockProvider::success("f", "fm", vec!["r".into()])),
                Lineage::new(""),
            )
            .build();
        let b = MagiBuilder::new(Arc::new(MockProvider::success("d", "dm", vec!["r".into()])))
            .with_agent(
                AgentName::Melchior,
                Arc::new(MockProvider::success("m1", "m", vec!["r".into()])),
                Lineage::new("alibaba"),
            )
            .with_agent(
                AgentName::Balthasar,
                Arc::new(MockProvider::success("m2", "m", vec!["r".into()])),
                Lineage::new("moonshot"),
            )
            .with_agent(
                AgentName::Caspar,
                Arc::new(MockProvider::success("m3", "m", vec!["r".into()])),
                Lineage::new("deepseek"),
            )
            .with_fallback_pool(pool);
        assert!(
            matches!(b.build(), Err(MagiError::InvalidInput { .. })),
            "empty/blank pool-candidate lineage is invalid input, rejected at build"
        );
    }

    // -- MS3 T10: extraction-failure telemetry with model attribution --

    /// A CLEAN run still says it was clean.
    ///
    /// The field is present with one entry per agent and an empty `Vec` each. That empty
    /// vector is a positive certificate of adherence; a field that vanished on success
    /// would make a clean 3.0 report indistinguishable from a 2.2 one.
    #[tokio::test]
    async fn test_extraction_failures_is_seeded_for_every_agent_on_a_clean_run() {
        let provider = Arc::new(MockProvider::success(
            "mock",
            "model",
            vec![
                mock_agent_json("melchior", "approve", 0.9),
                mock_agent_json("balthasar", "approve", 0.85),
                mock_agent_json("caspar", "approve", 0.95),
            ],
        )) as Arc<dyn LlmProvider>;
        let report = Magi::new(provider)
            .analyze(&Mode::Analysis, "x")
            .await
            .expect("clean run");

        assert_eq!(report.extraction_failures.len(), 3, "one entry per agent");
        assert!(
            report.extraction_failures.values().all(Vec::is_empty),
            "a clean seat certifies itself with an empty Vec"
        );
        // Joinable with `rotations` on the same key — the two halves of one story.
        assert_eq!(
            report.extraction_failures.keys().collect::<Vec<_>>(),
            report.rotations.keys().collect::<Vec<_>>(),
            "same key set, so the join is symmetric"
        );
    }

    /// A retry that RECOVERS still leaves the cause on the record — the gap this closes.
    /// Before this, such an agent appeared in `retried_agents` with no trace of why.
    #[tokio::test]
    async fn test_a_recovered_retry_still_records_its_cause() {
        let bad = "no markers at all".to_string();
        let good = mock_agent_json("melchior", "approve", 0.9);
        let melchior = Arc::new(MockProvider::success("m", "model-m", vec![bad, good]));
        let others = Arc::new(MockProvider::success(
            "o",
            "model-o",
            vec![
                mock_agent_json("balthasar", "approve", 0.85),
                mock_agent_json("caspar", "approve", 0.95),
            ],
        ));
        let report = MagiBuilder::new(others as Arc<dyn LlmProvider>)
            .with_provider(AgentName::Melchior, melchior as Arc<dyn LlmProvider>)
            .build()
            .expect("build")
            .analyze(&Mode::Analysis, "x")
            .await
            .expect("melchior recovers on retry");

        let mel = &report.extraction_failures[&AgentName::Melchior];
        assert_eq!(mel.len(), 1, "the first attempt was rejected");
        assert_eq!(mel[0].cause, ExtractionFailureCause::MissingMarkers);
        assert_eq!(mel[0].attempt, 1);
        assert_eq!(mel[0].model, "model-m", "attributed to the model that ran");
        assert!(report.retried_agents.contains(&AgentName::Melchior));
    }

    /// Both attempts on one model are recorded, numbered 1 then 2.
    #[tokio::test]
    async fn test_both_attempts_on_the_same_model_are_recorded() {
        let melchior = Arc::new(MockProvider::success(
            "m",
            "model-m",
            vec!["no markers".to_string(), "still no markers".to_string()],
        ));
        let others = Arc::new(MockProvider::success(
            "o",
            "model-o",
            vec![
                mock_agent_json("balthasar", "approve", 0.85),
                mock_agent_json("caspar", "approve", 0.95),
            ],
        ));
        let report = MagiBuilder::new(others as Arc<dyn LlmProvider>)
            .with_provider(AgentName::Melchior, melchior as Arc<dyn LlmProvider>)
            .build()
            .expect("build")
            .analyze(&Mode::Analysis, "x")
            .await
            .expect("two of three still reach consensus");

        let mel = &report.extraction_failures[&AgentName::Melchior];
        assert_eq!(mel.len(), 2);
        assert_eq!((mel[0].attempt, mel[1].attempt), (1, 2));
        assert!(mel.iter().all(|f| f.model == "model-m"));
        assert!(report.degraded, "melchior never produced a verdict");
    }

    /// THE TEMPORAL INVARIANT, and the only test that catches getting it wrong.
    ///
    /// A seat fails twice on its primary `pm`, rotates, and fails again on the fallback
    /// `fm`. The records must be `[{pm,1}, {pm,2}, {fm,1}]`.
    ///
    /// If the model were read AFTER the rotation instead of at the moment of failure, the
    /// result would be `[{fm,1},{fm,2},{fm,1}]` — plausible-looking, and accusing the
    /// model that had not run yet. Here the attribution is structural: each attempt is
    /// recorded by the code holding the provider that produced the output, so `attempt`
    /// also restarts at 1 per model without anyone having to remember to reset it.
    #[tokio::test]
    async fn test_a_failure_before_rotation_is_attributed_to_the_pre_rotation_model() {
        let bad = || "no markers here".to_string();
        let pool = FallbackPool::builder()
            .push(
                Arc::new(MockProvider::success("f", "fm", vec![bad(), bad()])),
                Lineage::new("zhipu"),
            )
            .build();
        let report = MagiBuilder::new(Arc::new(MockProvider::success(
            "d",
            "dm",
            vec![
                mock_agent_json("melchior", "approve", 0.9),
                mock_agent_json("balthasar", "approve", 0.85),
            ],
        )) as Arc<dyn LlmProvider>)
        .with_agent(
            AgentName::Caspar,
            Arc::new(MockProvider::success("p", "pm", vec![bad(), bad()])),
            Lineage::new("deepseek"),
        )
        .with_fallback_pool(pool)
        .build()
        .expect("build")
        .analyze(&Mode::Analysis, "x")
        .await
        .expect("two agents still reach consensus");

        let caspar = &report.extraction_failures[&AgentName::Caspar];
        assert_eq!(
            caspar
                .iter()
                .map(|f| (f.model.as_str(), f.attempt))
                .collect::<Vec<_>>(),
            vec![("pm", 1), ("pm", 2), ("fm", 1), ("fm", 2)],
            "pre-rotation failures belong to the pre-rotation model, and the attempt \
             counter restarts at 1 on the model rotated into"
        );
        assert!(
            caspar
                .iter()
                .all(|f| f.cause == ExtractionFailureCause::MissingMarkers)
        );
    }

    /// Per-seat and per-cause counts are DERIVABLE from the records, which is why the
    /// records are stored and the counts are not.
    #[tokio::test]
    async fn test_counts_are_derivable_from_the_records() {
        let report = report_with_one_failing_agent().await;
        let by_cause: BTreeMap<ExtractionFailureCause, usize> = report
            .extraction_failures
            .values()
            .flatten()
            .fold(BTreeMap::new(), |mut acc, f| {
                *acc.entry(f.cause).or_insert(0) += 1;
                acc
            });
        assert_eq!(by_cause[&ExtractionFailureCause::MissingMarkers], 2);
    }

    /// Shared setup: Melchior fails both attempts, the other two succeed.
    async fn report_with_one_failing_agent() -> MagiReport {
        let melchior = Arc::new(MockProvider::success(
            "m",
            "model-m",
            vec!["no markers".to_string(), "no markers".to_string()],
        ));
        let others = Arc::new(MockProvider::success(
            "o",
            "model-o",
            vec![
                mock_agent_json("balthasar", "approve", 0.85),
                mock_agent_json("caspar", "approve", 0.95),
            ],
        ));
        MagiBuilder::new(others as Arc<dyn LlmProvider>)
            .with_provider(AgentName::Melchior, melchior as Arc<dyn LlmProvider>)
            .build()
            .expect("build")
            .analyze(&Mode::Analysis, "x")
            .await
            .expect("two of three reach consensus")
    }

    /// A CLEAN run's human text is BYTE-IDENTICAL to one produced
    /// before this feature existed. The section must not appear, not even as a heading
    /// with nothing under it.
    #[tokio::test]
    async fn test_a_clean_run_adds_no_section_to_the_human_report() {
        let provider = Arc::new(MockProvider::success(
            "mock",
            "model",
            vec![
                mock_agent_json("melchior", "approve", 0.9),
                mock_agent_json("balthasar", "approve", 0.85),
                mock_agent_json("caspar", "approve", 0.95),
            ],
        )) as Arc<dyn LlmProvider>;
        let report = Magi::new(provider)
            .analyze(&Mode::Analysis, "x")
            .await
            .expect("clean run");

        assert!(
            !report.report.contains("Extraction Failures"),
            "a clean run must not grow a section: the text stays byte-identical"
        );
        // And the formatter agrees when asked directly with a fully-seeded clean map.
        let seeded: BTreeMap<AgentName, Vec<ExtractionFailure>> = report
            .extraction_failures
            .keys()
            .map(|k| (*k, Vec::new()))
            .collect();
        assert_eq!(
            ReportFormatter::new().format_extraction_failures(&seeded),
            "",
            "an all-empty map is a present certificate, not something to render"
        );
    }

    /// With failures, the section names the CAUSE and the MODEL per seat — the two facts
    /// that make it actionable.
    #[tokio::test]
    async fn test_the_section_attributes_cause_and_model_when_there_were_failures() {
        let report = report_with_one_failing_agent().await;
        let text = &report.report;
        assert!(text.contains("## Extraction Failures"), "{text}");
        assert!(text.contains("Melchior"), "names the seat: {text}");
        assert!(text.contains("model-m"), "names the MODEL: {text}");
        assert!(text.contains("missing markers"), "names the cause: {text}");
        assert!(text.contains("attempt 1"), "names the attempt: {text}");
    }

    /// A report written by 2.2.0 — i.e. without this field at all — must still
    /// deserialize. Built by serializing a real report and REMOVING the key, rather than
    /// hand-writing JSON that could drift from the actual schema.
    #[tokio::test]
    async fn test_a_2_2_0_report_without_the_field_still_deserializes() {
        let current = report_with_one_failing_agent().await;
        let mut value = serde_json::to_value(&current).expect("serialize");
        assert!(
            value
                .as_object_mut()
                .expect("object")
                .remove("extraction_failures")
                .is_some(),
            "the field IS serialized on a fresh report (no skip_serializing_if)"
        );

        let old: MagiReport = serde_json::from_value(value).expect("2.2.0 report still parses");
        assert!(old.extraction_failures.is_empty(), "absent means empty");
    }

    // -- Task 9 (MS2): lost-signal endpoint-down recovery on abnormal exit (W18) --

    /// An abnormal agent exit (a `JoinError` standing in for a panicked latch
    /// holder) must recover `EndpointDown` from the registry latch, NOT from the
    /// carrier — race-free, no dependency on WHEN the panic happened.
    #[tokio::test]
    async fn test_abnormal_exit_recovers_endpoint_down_from_registry() {
        let mut init = BTreeMap::new();
        init.insert(
            AgentName::Melchior,
            ActiveEntry {
                lineage: Lineage::new("alibaba"),
                model: "m".into(),
            },
        );
        init.insert(
            AgentName::Caspar,
            ActiveEntry {
                lineage: Lineage::new("deepseek"),
                model: "c".into(),
            },
        );
        let reg = LineageRegistry::new(init);
        reg.register_transport_failure(Lineage::new("alibaba"), true)
            .await; // connection=true
        reg.register_transport_failure(Lineage::new("deepseek"), true)
            .await; // 2 distinct → latch set
        assert!(reg.endpoint_down_signalled().await);

        // Simulate an abnormal outcome: a JoinError from an aborted spawn.
        let handle = tokio::spawn(async { std::future::pending::<()>().await });
        handle.abort();
        let join_err = handle.await.unwrap_err();

        let decision = resolve_abnormal_exit(AgentName::Caspar, &join_err, &reg).await;
        assert!(
            matches!(decision, Some(MagiError::EndpointDown { .. })),
            "abnormal exit must recover EndpointDown from the registry latch"
        );
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

    /// stateful predicate side effects MUST NOT fire
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

    /// MagiBuilder::with_retry_disabled() bypasses
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

        let (result, retried, _failures) = dispatch_one_agent(
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

        let (result, retried, _failures) = dispatch_one_agent(
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

        let (result, retried, _failures) = dispatch_one_agent(
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

    /// provider timeout does NOT trigger retry.
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

        let (result, retried, _failures) = dispatch_one_agent(
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
                retry_after_raw: vec![],
                received_at: None,
            })],
        ));
        let agent = Agent::new(AgentName::Caspar, provider as Arc<dyn LlmProvider>);
        let validator = Arc::new(Validator::new());
        let cfg = CompletionConfig::default();
        let (result, retried, _failures) = dispatch_one_agent(
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
                retry_after_raw: vec![],
                received_at: None,
            })],
        ));
        let agent = Agent::new(AgentName::Melchior, provider as Arc<dyn LlmProvider>);
        let validator = Arc::new(Validator::new());
        let cfg = CompletionConfig::default();
        let (result, retried, _failures) = dispatch_one_agent(
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
        let (result, retried, _failures) = dispatch_one_agent(
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
        let (result, retried, _failures) = dispatch_one_agent(
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
        let (result, retried, _failures) = dispatch_one_agent(
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

        let (result, retried, _failures) = dispatch_one_agent(
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

    /// with_retry_disabled bypasses the retry.
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
        let (result, retried, _failures) = dispatch_one_agent(
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
            matches!(err.error, MagiError::Deserialization(_)),
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
        let raw = format!(
            "{}\n{}\n{}",
            crate::verdict_markers::VERDICT_OPEN,
            r#"{"agent":"melchior","verdict":"approve","confidence":1.5,"summary":"s","reasoning":"r","findings":[],"recommendation":"rec"}"#,
            crate::verdict_markers::VERDICT_CLOSE
        );
        let err = parse_and_validate(&raw, &validator).unwrap_err();
        assert!(
            matches!(err.error, MagiError::Validation(_)),
            "expected Validation, got: {err:?}"
        );
    }

    // -- parse_agent_response: EXTRACT-ONLY (MS3 T7) --
    //
    // The 18 tests that used to live here exercised the brace-scanning recovery
    // heuristic and its two bounds. They are gone with it: what they covered is now
    // covered STRUCTURALLY, because there is no search to bound. The four
    // characterization tests above record the four residuals that disappeared.

    /// A bare 7-key object is NOT a verdict. The fast path is gone: accepting
    /// naked JSON would be exactly the fallback the no-search rule forbids, and it is
    /// the path an
    /// echoed example walks in through.
    #[test]
    fn test_parse_rejects_a_bare_json_object_without_markers() {
        let f = parse_and_validate(
            &mock_agent_object("caspar", "approve", 0.9),
            &Validator::new(),
        )
        .unwrap_err();
        assert_eq!(f.cause, ExtractionFailureCause::MissingMarkers);
    }

    /// Prose and a `<think>` block outside the markers are never read, so a
    /// thinking model no longer competes with its own verdict.
    #[test]
    fn test_parse_accepts_a_delimited_verdict_with_surrounding_prose() {
        let body = mock_agent_object("caspar", "approve", 0.9);
        let raw = format!(
            "<think>restating the schema</think>\n{}\n{body}\n{}\ntrailing prose",
            crate::verdict_markers::VERDICT_OPEN,
            crate::verdict_markers::VERDICT_CLOSE
        );
        let out = parse_and_validate(&raw, &Validator::new()).expect("delimited verdict");
        assert_eq!(out.agent, AgentName::Caspar);
    }

    /// A fence INSIDE the markers is stripped, so a model that wraps its JSON in
    /// ```json still parses — the permissiveness lives where it costs nothing.
    #[test]
    fn test_parse_strips_a_fence_inside_the_markers() {
        let body = mock_agent_object("melchior", "approve", 0.9);
        let raw = format!(
            "{}\n```json\n{body}\n```\n{}",
            crate::verdict_markers::VERDICT_OPEN,
            crate::verdict_markers::VERDICT_CLOSE
        );
        assert!(parse_and_validate(&raw, &Validator::new()).is_ok());
    }

    /// The cause→variant mapping, PINNED for ALL SEVEN causes.
    ///
    /// Tested against `magi_error_for` directly rather than through the parser: three of
    /// the seven are produced by later stages, so routing every case through the parser
    /// would leave the table partial — and a PARTIAL table is how the mapping's one
    /// discriminating line (Deserialization vs Validation) goes untested.
    #[test]
    fn test_every_cause_maps_to_the_pinned_error_variant() {
        use ExtractionFailureCause::*;
        // false = Deserialization ("I never got an AgentOutput")
        // true  = Validation      ("I had one and rejected it")
        let table = [
            (MissingMarkers, false),
            (Unterminated, false),
            (Ambiguous, false),
            (InvalidJson, false),
            (Schema, true),
            (EchoedExample, true),
            (AgentIdentity, true),
        ];
        for (cause, is_validation) in table {
            let e = magi_error_for(cause, "msg");
            assert_eq!(
                matches!(e, MagiError::Validation(_)),
                is_validation,
                "wrong variant for {cause:?}"
            );
            assert!(
                matches!(e, MagiError::Validation(_) | MagiError::Deserialization(_)),
                "{cause:?} must map to one of the two retry-eligible variants"
            );
        }
    }

    /// The unknown-cause arm is conservative: it must NOT claim an `AgentOutput` was
    /// obtained, so it maps to `Deserialization`.
    #[test]
    fn test_unknown_cause_maps_conservatively() {
        assert!(matches!(
            magi_error_for(ExtractionFailureCause::Other, "m"),
            MagiError::Deserialization(_)
        ));
    }

    /// Keeps the mapping function honest against what the parser actually emits — the
    /// three parser-reachable causes carry their pinned variant end to end.
    #[test]
    fn test_parser_reachable_causes_carry_their_pinned_variant_end_to_end() {
        let open = crate::verdict_markers::VERDICT_OPEN;
        let close = crate::verdict_markers::VERDICT_CLOSE;
        let cases: [(String, ExtractionFailureCause); 3] = [
            ("sin markers".into(), ExtractionFailureCause::MissingMarkers),
            (
                format!("{open}\n{{}}"),
                ExtractionFailureCause::Unterminated,
            ),
            (
                format!("{open}\nno json at all\n{close}"),
                ExtractionFailureCause::InvalidJson,
            ),
        ];
        for (raw, cause) in cases {
            let f = parse_and_validate(&raw, &Validator::new()).unwrap_err();
            assert_eq!(f.cause, cause, "raw was: {raw:?}");
            assert!(matches!(f.error, MagiError::Deserialization(_)));
        }
    }

    /// Selection is by TYPE. Rewording an error message must not move the cause,
    /// because matching strings is brittle AND a second-order injection surface.
    #[test]
    fn test_cause_survives_independently_of_the_error_message_text() {
        // Two inputs that fail the same WAY carry the same cause even though their
        // messages differ (the marker counts appear in the text). Nothing reads the text
        // to decide, which is the whole point: a rewording must not move the cause.
        let one_open = crate::verdict_markers::VERDICT_OPEN.to_string();
        let two_opens = format!(
            "{}\n{}",
            crate::verdict_markers::VERDICT_OPEN,
            crate::verdict_markers::VERDICT_OPEN
        );
        let a = parse_and_validate(&one_open, &Validator::new()).unwrap_err();
        let b = parse_and_validate(&two_opens, &Validator::new()).unwrap_err();
        assert_eq!(a.cause, ExtractionFailureCause::Unterminated);
        assert_eq!(a.cause, b.cause, "same cause despite different messages");
        assert_ne!(
            a.error.to_string(),
            b.error.to_string(),
            "the messages DO differ, which is exactly why the cause must not be parsed from them"
        );
    }

    /// A schema rejection is reported as `Schema`, not as a parse failure: the object
    /// WAS obtained, it just did not survive validation.
    #[test]
    fn test_schema_rejection_reports_the_schema_cause() {
        let bad = r#"{"agent":"melchior","verdict":"approve","confidence":9.5,"summary":"s",
            "reasoning":"r","findings":[],"recommendation":"rec"}"#;
        let raw = format!(
            "{}\n{bad}\n{}",
            crate::verdict_markers::VERDICT_OPEN,
            crate::verdict_markers::VERDICT_CLOSE
        );
        let f = parse_and_validate(&raw, &Validator::new()).unwrap_err();
        assert_eq!(f.cause, ExtractionFailureCause::Schema);
        assert!(matches!(f.error, MagiError::Validation(_)));
    }

    /// Builds a marker-delimited response whose `summary`/`recommendation` are the
    /// canary values — i.e. the worked example an agent must not echo.
    fn echoed_example_response(agent: &str) -> String {
        let object = format!(
            r#"{{"agent":"{agent}","verdict":"conditional","confidence":0.85,
                "summary":"{}","reasoning":"r","findings":[],"recommendation":"{}"}}"#,
            crate::prompts::ECHO_CANARY_SUMMARY,
            crate::prompts::ECHO_CANARY_RECOMMENDATION
        );
        format!(
            "{}\n{object}\n{}",
            crate::verdict_markers::VERDICT_OPEN,
            crate::verdict_markers::VERDICT_CLOSE
        )
    }

    /// The example copied INSIDE the markers is caught by the canary. Outside them
    /// it never reaches the parser at all; this is the second line of defence.
    #[test]
    fn test_echoed_example_inside_the_markers_is_rejected() {
        let f = parse_validate_and_check(
            &echoed_example_response("caspar"),
            AgentName::Caspar,
            &Validator::new(),
        )
        .unwrap_err();
        assert_eq!(f.cause, ExtractionFailureCause::EchoedExample);
        assert!(matches!(f.error, MagiError::Validation(_)));
    }

    /// A mage may not answer for another: echoed example from another prompt, role
    /// confusion, or context contamination.
    #[test]
    fn test_a_mage_may_not_answer_for_another() {
        let raw = mock_agent_json("melchior", "approve", 0.9);
        let f = parse_validate_and_check(&raw, AgentName::Caspar, &Validator::new()).unwrap_err();
        assert_eq!(f.cause, ExtractionFailureCause::AgentIdentity);
        assert!(matches!(f.error, MagiError::Validation(_)));
    }

    /// WHEN BOTH FIRE, THE CANARY WINS.
    ///
    /// A mage echoing ANOTHER prompt's example matches both checks. Reporting
    /// `AgentIdentity` would tell it "fix your agent field", and an obedient model would
    /// fix the name and RESEND the echoed example — right identity, nothing left to catch
    /// it. `EchoedExample` names the root cause, and its feedback subsumes the identity
    /// fix; the reverse does not.
    #[test]
    fn test_canary_wins_when_both_checks_fire() {
        let f = parse_validate_and_check(
            &echoed_example_response("melchior"),
            AgentName::Caspar,
            &Validator::new(),
        )
        .unwrap_err();
        assert_eq!(
            f.cause,
            ExtractionFailureCause::EchoedExample,
            "the canary must precede the identity check"
        );
    }

    /// The "case-insensitive" requirement is satisfied by DESERIALIZATION, not by the
    /// comparator: a wrong-case agent name never becomes an `AgentOutput`, so it can
    /// never reach the identity check. This pins that reasoning so nobody re-adds an
    /// `eq_ignore_ascii_case` for a variation that cannot occur.
    #[test]
    fn test_wrong_case_agent_name_fails_before_the_identity_check() {
        let raw = format!(
            "{}\n{}\n{}",
            crate::verdict_markers::VERDICT_OPEN,
            r#"{"agent":"CASPAR","verdict":"approve","confidence":0.9,"summary":"s",
               "reasoning":"r","findings":[],"recommendation":"rec"}"#,
            crate::verdict_markers::VERDICT_CLOSE
        );
        let f = parse_validate_and_check(&raw, AgentName::Caspar, &Validator::new()).unwrap_err();
        assert_eq!(
            f.cause,
            ExtractionFailureCause::InvalidJson,
            "the name is canonicalized by serde, so a wrong case is invalid JSON"
        );
    }

    /// A correct verdict from the right seat passes both checks untouched.
    #[test]
    fn test_a_compliant_verdict_passes_both_post_validation_checks() {
        let raw = mock_agent_json("caspar", "reject", 0.8);
        let out = parse_validate_and_check(&raw, AgentName::Caspar, &Validator::new())
            .expect("a compliant verdict must pass");
        assert_eq!(out.verdict, Verdict::Reject);
    }

    /// Never panics, whatever the model emits — including the deeply-nested input the
    /// deleted brace-scanner had to be bounded against.
    #[test]
    fn test_parse_never_panics_on_adversarial_input() {
        let deep = format!("{}{}", "{".repeat(5_000), "}".repeat(5_000));
        let open = crate::verdict_markers::VERDICT_OPEN;
        let close = crate::verdict_markers::VERDICT_CLOSE;
        for raw in [
            String::new(),
            "\r".to_string(),
            deep.clone(),
            format!("{open}\n{deep}\n{close}"),
            format!("{open}\n\u{2028}\n{close}"),
            "\u{feff}".to_string(),
        ] {
            let _ = parse_and_validate(&raw, &Validator::new());
        }
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
    /// agent identity
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
            mappings: Vec<(String, AgentName)>,
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
                routing.insert(custom, name);
            }
            Self {
                captured,
                routing: Arc::new(routing),
            }
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

    /// THE END-TO-END TEST THE REFERENCE IMPLEMENTATION'S LESSON DEMANDS.
    ///
    /// The reference implementation's guard *existed*, *was tested*, and **nobody called
    /// it**. Testing `validate_prompt` in isolation would reproduce that failure exactly,
    /// so this asserts the wiring: a corrupt custom prompt aborts `build()` and **not one
    /// request reaches the provider**.
    #[tokio::test]
    async fn test_build_aborts_on_a_corrupt_custom_prompt_before_any_provider_call() {
        struct TallyProvider {
            calls: Arc<AtomicUsize>,
        }
        #[async_trait::async_trait]
        impl LlmProvider for TallyProvider {
            async fn complete(
                &self,
                _s: &str,
                _u: &str,
                _c: &CompletionConfig,
            ) -> Result<String, ProviderError> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                Ok(String::new())
            }
            fn name(&self) -> &str {
                "tally"
            }
            fn model(&self) -> &str {
                "tally-model"
            }
        }

        let calls = Arc::new(AtomicUsize::new(0));
        let provider = Arc::new(TallyProvider {
            calls: calls.clone(),
        });
        let result = MagiBuilder::new(provider as Arc<dyn LlmProvider>)
            .with_custom_prompt_all_modes(AgentName::Caspar, "no markers here".to_string())
            .build();

        match result {
            Err(MagiError::PromptContract { agent, reason, .. }) => {
                assert_eq!(
                    agent,
                    Some(AgentName::Caspar),
                    "must name the seat: {reason}"
                );
            }
            Err(other) => panic!("expected PromptContract, got {other}"),
            Ok(_) => panic!("a prompt without markers must not build"),
        }
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "no request may reach the provider"
        );
    }

    /// A prompt whose delimited block IS a complete verdict is a fabrication template,
    /// and `build()` must refuse it — including when a fence hides it.
    #[test]
    fn test_build_rejects_a_fabricable_custom_prompt_even_inside_a_fence() {
        let object = r#"{"agent":"caspar","verdict":"approve","confidence":0.9,"summary":"s",
           "reasoning":"r","findings":[],"recommendation":"rec"}"#;
        for body in [object.to_string(), format!("```json\n{object}\n```")] {
            let provider: Arc<dyn LlmProvider> = Arc::new(MockProvider::success(
                "mock",
                "model",
                vec![mock_agent_json("caspar", "approve", 0.9)],
            ));
            let prompt = format!(
                "{}\n{body}\n{}",
                crate::verdict_markers::VERDICT_OPEN,
                crate::verdict_markers::VERDICT_CLOSE
            );
            assert!(
                matches!(
                    MagiBuilder::new(provider)
                        .with_custom_prompt_all_modes(AgentName::Caspar, prompt)
                        .build(),
                    Err(MagiError::PromptContract { .. })
                ),
                "a fabrication template must not build"
            );
        }
    }

    /// The guard covers the THREE EMBEDDED prompts too, not just overrides — a default
    /// build must succeed, which proves the re-pinned prompts satisfy their own contract.
    #[test]
    fn test_default_build_succeeds_because_embedded_prompts_satisfy_the_contract() {
        let provider: Arc<dyn LlmProvider> = Arc::new(MockProvider::success(
            "mock",
            "model",
            vec![mock_agent_json("melchior", "approve", 0.9)],
        ));
        MagiBuilder::new(provider)
            .build()
            .expect("the shipped prompts must satisfy the guard they are validated by");
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
            .with_custom_prompt_for_mode(
                AgentName::Melchior,
                Mode::CodeReview,
                contract_prompt("X"),
            )
            .build()
            .expect("build should succeed");
        assert_eq!(
            magi.overrides()
                .get(&(AgentName::Melchior, Some(Mode::CodeReview))),
            Some(&contract_prompt("X"))
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
            .with_custom_prompt_all_modes(AgentName::Balthasar, contract_prompt("Y"))
            .build()
            .expect("build should succeed");
        assert_eq!(
            magi.overrides().get(&(AgentName::Balthasar, None)),
            Some(&contract_prompt("Y"))
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
            .with_custom_prompt(AgentName::Caspar, Mode::Design, contract_prompt("Z"))
            .build()
            .expect("build should succeed");
        assert_eq!(
            magi.overrides()
                .get(&(AgentName::Caspar, Some(Mode::Design))),
            Some(&contract_prompt("Z"))
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
            vec![(contract_prompt("CUSTOM MEL"), AgentName::Melchior)],
        ));
        let magi = MagiBuilder::new(provider as Arc<dyn LlmProvider>)
            .with_custom_prompt_all_modes(AgentName::Melchior, contract_prompt("CUSTOM MEL"))
            .build()
            .expect("build should succeed");
        let _ = magi.analyze(&Mode::Design, "x").await.unwrap();
        let calls = captured.lock().unwrap();
        assert!(
            calls
                .iter()
                .any(|(sys, _)| *sys == contract_prompt("CUSTOM MEL")),
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
                (contract_prompt("GENERIC MEL"), AgentName::Melchior),
                (contract_prompt("SPECIFIC MEL"), AgentName::Melchior),
            ],
        ));
        let magi = MagiBuilder::new(provider as Arc<dyn LlmProvider>)
            .with_custom_prompt_all_modes(AgentName::Melchior, contract_prompt("GENERIC MEL"))
            .with_custom_prompt_for_mode(
                AgentName::Melchior,
                Mode::Design,
                contract_prompt("SPECIFIC MEL"),
            )
            .build()
            .expect("build should succeed");
        let _ = magi.analyze(&Mode::Design, "x").await.unwrap();
        let calls = captured.lock().unwrap();
        assert!(
            calls
                .iter()
                .any(|(sys, _)| *sys == contract_prompt("SPECIFIC MEL")),
            "mode-specific prompt should have been used for Mode::Design"
        );
        assert!(
            !calls
                .iter()
                .any(|(sys, _)| *sys == contract_prompt("GENERIC MEL")),
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
            vec![(contract_prompt("SHIM PROMPT"), AgentName::Caspar)],
        ));
        let provider_new = Arc::new(CapturingMockProvider::with_routing(
            captured_new.clone(),
            vec![(contract_prompt("SHIM PROMPT"), AgentName::Caspar)],
        ));

        let magi_legacy = MagiBuilder::new(provider_legacy as Arc<dyn LlmProvider>)
            .with_custom_prompt(
                AgentName::Caspar,
                Mode::CodeReview,
                contract_prompt("SHIM PROMPT"),
            )
            .build()
            .expect("legacy build should succeed");

        let magi_new = MagiBuilder::new(provider_new as Arc<dyn LlmProvider>)
            .with_custom_prompt_for_mode(
                AgentName::Caspar,
                Mode::CodeReview,
                contract_prompt("SHIM PROMPT"),
            )
            .build()
            .expect("new build should succeed");

        let _ = magi_legacy
            .analyze(&Mode::CodeReview, "test")
            .await
            .unwrap();
        let _ = magi_new.analyze(&Mode::CodeReview, "test").await.unwrap();

        let legacy_calls = captured_legacy.lock().unwrap();
        let new_calls = captured_new.lock().unwrap();

        // Both paths must have forwarded the same custom prompt to Caspar.
        let expected = contract_prompt("SHIM PROMPT");
        assert!(
            legacy_calls.iter().any(|(sys, _)| *sys == expected),
            "legacy shim must forward the custom prompt to Caspar"
        );
        assert!(
            new_calls.iter().any(|(sys, _)| *sys == expected),
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
        // The file content carries the verdict-marker contract because the MS3 guard
        // validates filesystem-loaded prompts too — which makes this test double as the
        // proof that `prompts_dir` prompts are covered, not just builder-level ones.
        std::fs::write(
            tmp.0.join("melchior_code_review.md"),
            contract_prompt("CUSTOM FROM FILESYSTEM"),
        )
        .unwrap();

        let captured: Arc<std::sync::Mutex<Vec<(String, String)>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let provider = Arc::new(CapturingMockProvider::with_routing(
            captured.clone(),
            vec![(
                contract_prompt("CUSTOM FROM FILESYSTEM"),
                AgentName::Melchior,
            )],
        ));
        let magi = MagiBuilder::new(provider as Arc<dyn LlmProvider>)
            .with_prompts_dir(tmp.0.clone())
            .build()
            .expect("build should succeed");
        let _ = magi.analyze(&Mode::CodeReview, "x").await.unwrap();

        let calls = captured.lock().unwrap();
        assert!(
            calls
                .iter()
                .any(|(sys, _)| *sys == contract_prompt("CUSTOM FROM FILESYSTEM")),
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
