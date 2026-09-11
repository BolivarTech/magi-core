// Author: Julian Bolivar
// Version: 4.1.0
// Date: 2026-09-06

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use std::sync::Mutex;

use crate::agent::{Agent, AgentFactory};
use crate::consensus::{ConsensusConfig, ConsensusEngine};
use crate::error::{ExternalErrorKind, MagiError, ProviderError};
use crate::provider::{Completion, CompletionConfig, LlmProvider};
use crate::reporting::{
    CompletionRecord, ExtractionFailure, InputSize, MagiReport, ReportConfig, ReportFormatter,
    TOKENS_PER_BYTE_DIVISOR, estimate_tokens,
};
use crate::rotation::{
    ActiveEntry, AgentRotation, AgentRotationState, AgentSlotGuard, CandidateEligibility,
    CrateDefectRecord, FallbackPool, Lineage, LineageRegistry, ModelCapability, ProviderProbe,
    RotationConfig, RotationEvent, RotationKind, RotationPolicy, digest_collision, run_preflight,
    strict_guard_is_inert,
};
use crate::schema::{AgentName, AgentOutput, Mode};
use crate::user_prompt::{FastrandSource, RngLike, build_retry_prompt, build_user_prompt};
use crate::validate::{ValidationLimits, Validator};
use crate::verdict_markers::ExtractionFailureCause;
use tokio::task::AbortHandle;

/// Default value for [`MagiConfig::max_input_len`] — 4 MB.
///
/// This is a compromise between Python's 10 MB and v0.1.2's 1 MB.
/// A full 10 MB alignment with Python is unscheduled, pending
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
/// The per-agent ceiling, sized to cover the worst case of ONE retry chain.
///
/// # Where the number comes from
///
/// `(1 + limited_max_retries) x client_timeout + backoffs` = `2 x 300 + 1` = **601 s** for a
/// hang, and roughly **604 s** through the `Retry-After` path where the backstop cuts. 660 s is
/// that worst case plus margin.
///
/// # This is NOT the old invariant, and the old one is deliberately not satisfied
///
/// `operation_budget + client_timeout <= timeout` would give `450 + 300 = 750 > 660`. That
/// formulation dates from when the budget was the binding limit; with the attempt count binding
/// it is a backstop, and adding it to the worst case would charge 25 minutes per seat of ceiling
/// the chain cannot use.
///
/// # It is per CALL, not per chain
///
/// It wraps a single call and is applied twice per model (the call plus the corrective retry).
/// The worst case per SEAT multiplies it further by `1 + max_rotations`.
const DEFAULT_AGENT_TIMEOUT: Duration = Duration::from_secs(660);

/// Configuration for the MAGI orchestrator.
///
/// Controls timeout per agent, maximum input size, and LLM completion parameters.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct MagiConfig {
    /// Maximum time to wait for each agent (default: **660 seconds**).
    ///
    /// # Layering — the invariant, and why it is not the one you may remember
    ///
    /// The ceiling must cover the worst case of ONE retry chain:
    ///
    /// ```text
    /// timeout >= (1 + limited_max_retries) * client_timeout + backoffs
    /// ```
    ///
    /// With the shipped defaults that is `2 * 300 + 1 = 601 s` for a hang, and roughly `604 s`
    /// through the `Retry-After` path where the backstop cuts, against a ceiling of `660 s`.
    ///
    /// # What it covers, and what it does NOT
    ///
    /// It covers a **homogeneous** chain of attempt-limited failures — the hang case the count
    /// was chosen for. A **mixed** chain does not fit: a `429` is not attempt-limited, so it keeps
    /// the general count and each honoured `Retry-After` runs in full, bounding such a chain by
    /// `operation_budget + max(client_timeout, retry_after_cap + jitter)` = `751 s`, **above**
    /// this ceiling. The binding term is whichever wait is longer; with both shipped at 300 s
    /// they coincide, so naming only the client timeout would mislead anyone tuning the cap.
    ///
    /// **Consequence, stated rather than left to be discovered:** there this timeout cuts first
    /// and the abandonment is an opaque timeout rather than the typed
    /// `AbandonReason::OperationBudgetExhausted`. Raising the ceiling past `750 s` would recover
    /// it for mixed chains at the cost of a longer worst case for every seat; the shipped value
    /// optimises for the common case and says so here instead of implying a guarantee.
    ///
    /// **The older form — `operation_budget + client_timeout <= timeout` — is deliberately NOT
    /// satisfied** (`450 + 300 = 750 > 660`). It was formulated when the budget was the binding
    /// limit. With the per-class attempt count binding, the budget is a **backstop**, and adding
    /// it to the worst case would charge about 25 minutes per seat of ceiling the chain cannot
    /// use. A reader arriving from `3.1.0` will look for that sum; this is where it went.
    ///
    /// # Where the multiplication comes from — the value alone does not say it
    ///
    /// This wraps a **single call** and is applied **twice per model** (the call plus the
    /// corrective schema retry), across `1 + max_rotations` models. The worst case **per seat**
    /// is therefore `timeout * calls_per_model * (1 + max_rotations)`.
    ///
    /// **With the crate's own defaults that is 22 minutes, not 66 — but only if you declared
    /// NEITHER a pool nor a probe.** Rotation engages on either one, so `with_probing_agent` with
    /// no pool substitutes an empty pool carrying `DEFAULT_MAX_ROTATIONS` and the figure is 66
    /// after all. Measured, not derived: `660 x 2 x 1 = 1320 s` for the bare builder, and
    /// `660 x 2 x 3 = 3960 s` as soon as either is present. [`Magi::worst_case_per_seat`] computes it from the
    /// effective configuration rather than from either of those numbers.
    ///
    /// **Per seat, never per run:** whether the backend serves the three mages in parallel or
    /// serialises them is a property of the deployment, and this crate does not know it.
    ///
    /// # Local deployments
    ///
    /// The defaults are calibrated for **CLOUD** (36-96 s per attempt, measured). Against a local
    /// Ollama the same work can take 30 minutes per mage, and a single GPU **serialises** the
    /// three seats even though the orchestrator dispatches them in parallel — roughly 90 minutes
    /// per run, which is correct there.
    ///
    /// Raise them **IN THIS ORDER**; each one bounds the next:
    ///
    /// 1. **`client_timeout`** — the slowest legitimate request you have seen, set per provider
    ///    with its `with_timeout`. Everything else derives from it.
    /// 2. **[`RetryConfig::operation_budget`]** — keep it at or above the floor its own rustdoc
    ///    gives (`client_timeout + base_delay + 1`), or you lose the deterministic second attempt
    ///    of a hang. **That one the crate warns about**, computed from the SHIPPED client timeout,
    ///    so if you raised yours the guard cannot see your real floor. Going far above costs wall
    ///    clock on the `Retry-After` path and is deliberately NOT warned, because it is what step
    ///    1 above produces.
    /// 3. **`MagiConfig::timeout`** (this field) — at least the chain's worst case.
    ///
    /// Then read [`Magi::worst_case_per_seat`]: it tells you what you just bought. It is a
    /// **ceiling, not a prediction** — the chain usually ends earlier.
    ///
    /// A default calibrated on purpose for another deployment, without the guidance to move it,
    /// is indistinguishable from a badly chosen one — and the user finds out when their model
    /// dies at five minutes for a reason that is not theirs.
    ///
    /// **The ordering above assumes you opted into [`RetryProvider`]** — [`MagiBuilder::build`]
    /// does not wrap providers in one, so without it steps 1 and 2 have no subject. **This field
    /// applies either way**, and a consumer with no `RetryProvider` feels the 300 -> 660 s change
    /// most directly: nothing else bounds the call.
    ///
    /// [`RetryConfig`]: crate::provider::RetryConfig
    /// [`RetryConfig::operation_budget`]: crate::provider::RetryConfig::operation_budget
    /// [`RetryProvider`]: crate::provider::RetryProvider
    pub timeout: Duration,
    /// Maximum accepted size of the raw `content` argument to [`Magi::analyze`], in bytes.
    ///
    /// Default: [`DEFAULT_MAX_INPUT_LEN`] (4 MB).
    ///
    /// Note: for public-facing deployments where `content` is untrusted,
    /// consider lowering this via [`MagiBuilder::with_max_input_len`] to a value
    /// appropriate for your threat model. Default (4 MB) is a compromise between
    /// Python MAGI's 10 MB and v0.1.2's 1 MB; a full 10 MB alignment with Python
    /// is unscheduled, pending allocation audit of the analyze() pipeline.
    ///
    /// # Allocation audit (2026-04-18)
    ///
    /// An allocation audit of the `analyze()` pipeline for `magi-core v0.2.0` found
    /// 5 copy points on the content's path from `analyze()` entry to wire serialization:
    /// (1) user-prompt construction via `format!`, (2–4) per-agent `String::clone` to
    /// satisfy `tokio::spawn`'s `'static` bound (3 agents), and (5) HTTP/stdin
    /// serialization by the provider. Peak memory per analysis is approximately
    /// `content.len() × 5` plus fixed overhead. For the 4 MB default, peak ≈ 20 MB.
    /// A full 10 MB alignment with Python is unscheduled, pending an
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

/// The comparison, over a count already taken.
///
/// # Parameters
/// - `estimated_tokens`: a count from [`estimate_tokens`].
/// - `cfg`: the configuration whose `input_warn_tokens` applies.
fn exceeds(estimated_tokens: usize, cfg: &MagiConfig) -> bool {
    // Strictly greater: at exactly the threshold nothing is wrong yet. It also means an empty
    // input never warns, not even against a threshold of 0.
    estimated_tokens > cfg.input_warn_tokens
}

/// Measures `content` once and reports it against the configured threshold.
///
/// # Parameters
/// - `content`: the analysis input.
/// - `cfg`: the configuration whose `input_warn_tokens` applies.
///
/// # Why this exists rather than three field assignments
///
/// `exceeded` restates a relation between the other two fields, so the only way it can ever be
/// wrong is if it is computed from a different measurement than the one reported. Taking the count
/// once and deriving the flag from that same value makes disagreement impossible instead of
/// merely unlikely — and it drops a second pass over the input from the hot path.
pub(crate) fn measure_input(content: &str, cfg: &MagiConfig) -> InputSize {
    let estimated_tokens = estimate_tokens(content);
    InputSize {
        estimated_tokens,
        warn_threshold: cfg.input_warn_tokens,
        exceeded: exceeds(estimated_tokens, cfg),
    }
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
    //
    // The `+ 1` is the whole predicate, and leaving it out was wrong by up to three bytes.
    // Warning requires `len / 4 > T`, and integer division makes the smallest such input
    // `4 * (T + 1)` bytes — not `4 * T`. So the threshold is unreachable exactly when that
    // smallest input is already too large for the validator. Comparing `4 * T` instead called a
    // configuration reachable whenever the limit sat in the three bytes above it, which is
    // precisely the silence this check exists to announce.
    cfg.input_warn_tokens
        .saturating_add(1)
        .saturating_mul(TOKENS_PER_BYTE_DIVISOR)
        > cfg.max_input_len
}

impl Default for MagiConfig {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_AGENT_TIMEOUT,
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
    /// probes declared for a primary, through either `with_probing_agent` or
    /// `with_agent_and_probe`; a plain `with_agent` clears the entry.
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
        self.register_seat(agent, provider, Some(lineage), None);
        self
    }

    /// **The one place the three seat maps are written.**
    ///
    /// R-4 was `with_provider` setting the provider and leaving a probe declared earlier
    /// pointing at a model that seat no longer serves. `with_agent` had the `remove` and
    /// `with_provider` did not — so the invariant lived in whoever remembered it, which is
    /// how there came to be several writers of one piece of state.
    ///
    /// Written as prose in an acceptance criterion, the rule lasts until the next setter
    /// somebody adds without reading it. Routing every write through here makes it
    /// structural instead: the fields are private and a new setter has to come through this
    /// method to touch them.
    ///
    /// `lineage: None` means **leave the declared lineage alone** — `with_provider`
    /// overrides only the provider, and clearing it would silently un-declare the diversity
    /// key. `probe: None` means **this seat now has none**, which is R-4 itself: a
    /// registration that declares no probe must not inherit the previous one.
    fn register_seat(
        &mut self,
        agent: AgentName,
        provider: Arc<dyn LlmProvider>,
        lineage: Option<Lineage>,
        probe: Option<Arc<dyn ProviderProbe>>,
    ) {
        self.agent_providers.insert(agent, provider);
        if let Some(lineage) = lineage {
            self.agent_lineages.insert(agent, lineage);
        }
        match probe {
            Some(probe) => {
                self.primary_probes.insert(agent, probe);
            }
            None => {
                self.primary_probes.remove(&agent);
            }
        }
    }

    /// Registers a primary provider that also declares a [`ProviderProbe`], so the
    /// preflight can resolve its window and digest. This is the recommended door for the
    /// common case: `Arc<P>` is coerced to both trait objects here and forwarded to
    /// [`with_agent_and_probe`](Self::with_agent_and_probe), which performs the actual
    /// registration, so there is only one place that writes the three maps. It is safer
    /// than declaring the probe separately, because a single object cannot disagree with
    /// itself about which model it measures. A probe that is down or unmeasurable
    /// degrades to "not measured" and never blocks the run (fail-open).
    pub fn with_probing_agent<P: LlmProvider + ProviderProbe + 'static>(
        self,
        agent: AgentName,
        provider: Arc<P>,
        lineage: Lineage,
    ) -> Self {
        let llm: Arc<dyn LlmProvider> = provider.clone();
        let probe: Arc<dyn ProviderProbe> = provider;
        self.with_agent_and_probe(agent, llm, lineage, probe)
    }

    /// Registers a primary provider whose probe is declared separately from its
    /// completion view, for a consumer whose primary provider cannot also probe — the
    /// capability is declared here at the registration site, never discovered by
    /// downcast.
    ///
    /// # Contract
    ///
    /// The probe must measure the same model as `provider`, as recorded by this builder
    /// for `agent`: the preflight keys the capability by the agent's registered
    /// provider, while the measured value comes from the probe. If the two disagree, two
    /// things go wrong. First, the window ends up filed under the wrong model; since the
    /// window pre-filter never applies to a primary, this surfaces as a misreported
    /// estimated-window note on the primary rather than blocking the run. Second, that
    /// same key feeds the digest collision check, so a mismatched probe can make a
    /// healthy primary look like it collides with a lineage it never touched, or hide a
    /// collision that does exist. A probe that is down or unmeasurable degrades to "not
    /// measured" and never blocks the run (fail-open).
    ///
    /// Register **one probe per model**. The preflight keys by model across primaries and
    /// pool candidates alike, so a model declared here and again as a fallback candidate,
    /// with two different probes, keeps whichever answered last, in nondeterministic order.
    pub fn with_agent_and_probe(
        mut self,
        agent: AgentName,
        provider: Arc<dyn LlmProvider>,
        lineage: Lineage,
        probe: Arc<dyn ProviderProbe>,
    ) -> Self {
        self.register_seat(agent, provider, Some(lineage), Some(probe));
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
    /// A plain override declares no probe, so a probe declared earlier for this seat is
    /// cleared — the same guarantee [`with_agent`](Self::with_agent) already made. Left
    /// behind, it would be paired with the NEW model in the preflight and its measurement
    /// filed under the wrong key.
    ///
    /// # Parameters
    /// - `name`: Which agent to override.
    /// - `provider`: The provider for that agent.
    pub fn with_provider(mut self, name: AgentName, provider: Arc<dyn LlmProvider>) -> Self {
        self.register_seat(name, provider, None, None);
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
    /// - [`MagiError::Io`] if `prompts_dir` is set and cannot be read.
    /// - [`MagiError::InvalidInput`] for a configuration this type refuses to construct.
    /// - [`MagiError::PromptContract`] if any resolvable prompt — embedded or overridden —
    ///   violates the verdict-marker contract. Checked BEFORE touching any provider.
    /// - [`MagiError::Validation`] if the report configuration does not validate.
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
            warned: std::sync::atomic::AtomicU32::new(0),
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
    // 4.0.0 — one record per completion ATTEMPT, in the order the seat made them. NOT
    // seeded: an absent entry means the seat made no attempt at all, which is a different
    // claim from an empty one.
    BTreeMap<AgentName, Vec<CompletionRecord>>,
    // Axis E — which pool candidates each seat could and could not have rotated into, as of
    // BEFORE dispatch. Seeded for every dispatched seat: an empty Vec means "nothing to
    // reject", and an absent seat would mean the snapshot was never computed.
    BTreeMap<AgentName, Vec<CandidateEligibility>>,
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
    /// One bit per [`WarnOnce`] variant: set once that warning has been said.
    ///
    /// These warnings report CONFIGURATION, and a long-lived orchestrator would repeat the
    /// same sentence on every call. Nobody reads a line they have seen four hundred times,
    /// which is the failure the warnings exist to avoid; and the conditions they name are
    /// decided before dispatch and cannot be fixed mid-run, so a second telling describes a
    /// state that provably has not changed.
    ///
    /// **A mask rather than one flag per warning, because one-flag-each IS what R-18 was.**
    /// Two warnings had an `AtomicBool` and a third never got one; nothing noticed, because
    /// having a flag was up to whoever wrote the warning. Going through
    /// [`warn_once`](Magi::warn_once) makes the cadence structural on the render side.
    ///
    /// A `HashMap` would need a `Mutex` to mutate through `&self` — a lock on the dispatch
    /// path to hold a handful of flags. `Relaxed` suffices; see `warn_once`.
    warned: std::sync::atomic::AtomicU32,
}

/// Which one-shot warning is being asked for, and the data it renders.
///
/// **R-18 is why this exists.** Two builder warnings were latched behind their own
/// `AtomicBool` and a third simply never got one, so it repeated on every `analyze()`.
/// What failed was not the flag's TYPE — it was that having one was up to whoever wrote
/// the warning. One shared gate moves that from discipline to structure.
///
/// # What this guards, and what it does not
///
/// Both matches below are exhaustive, so a new variant does not compile until it states
/// **its bit** and **its rendering**: forgetting a cadence is no longer possible on the
/// render side. It does **not** stop anyone writing a bare `tracing::warn!` and bypassing
/// the gate — which is literally what R-18 was. That half is closed on the emit side by a
/// gate check. Claiming the enum makes it impossible would be a false statement about the
/// code beside it.
///
/// # Deviation from the plan's sketch, and why
///
/// The sketch used a field-less enum with explicit discriminants plus
/// `assert!(Last as u32 == COUNT - 1)`. The variants carry payloads here, because a
/// field-less enum forces the rendering data through a second parallel type whose variants
/// can disagree with this one's at runtime — a hole where the sketch wanted a compile-time
/// guarantee. `bit()` replaces the discriminant, and the discriminant assert is replaced by
/// `every_warning_owns_a_distinct_bit_inside_the_mask`, which is **stronger**: the assert
/// only checked the last variant, so a variant inserted in the middle with a duplicated bit
/// passed it, and a duplicated bit swallows one warning for the rest of the process.
pub(crate) enum WarnOnce<'a> {
    /// `strict_context_guard` is on with nothing measured, so rotation cannot fire.
    InertGuard { candidates: usize },
    /// A probe declares a different model than the provider it was registered beside.
    ProbeDeclaration {
        targets: &'a [(String, Arc<dyn ProviderProbe>)],
    },
}

impl WarnOnce<'_> {
    /// How many variants there are, maintained BY HAND because Rust generates no such
    /// item for an enum.
    ///
    /// The sketch prescribed `assert!(WarnOnce::COUNT <= 32)` against a `COUNT` that did
    /// not exist, so the guard protecting the mask's ceiling **did not compile** — a guard
    /// that reads as present while being absent, which is the class this release is about.
    /// It is asserted against the list it counts in
    /// `every_warning_owns_a_distinct_bit_inside_the_mask`.
    pub(crate) const COUNT: u32 = 2;

    /// This warning's bit in the mask.
    ///
    /// Exhaustive on purpose: a new variant must name its bit or the crate stops building.
    pub(crate) fn bit(&self) -> u32 {
        match self {
            Self::InertGuard { .. } => 0,
            Self::ProbeDeclaration { .. } => 1,
        }
    }

    /// Writes the warning.
    ///
    /// Exhaustive for the same reason as `bit`: a new variant must say what it prints.
    /// Called only once per variant per instance — the gate is `Magi::warn_once`.
    fn emit(&self) {
        match self {
            Self::InertGuard { candidates } => tracing::warn!(
                candidates = candidates,
                "strict_context_guard is on and no fallback candidate has a measured context                  window, so every candidate is filtered out and rotation cannot fire; declare a                  probe for the candidates or turn the guard off"
            ),
            // DELEGATES rather than reimplementing. Inlining the body here rewrote its two
            // messages from memory: the field names came out different and the SECOND
            // warning -- two probes registered for one model -- disappeared entirely. One
            // implementation is also what keeps this arm honest when that function grows a
            // third condition.
            Self::ProbeDeclaration { targets } => warn_on_probe_disagreement(targets),
        }
    }
}

// `1 << 32` on a `u32` is undefined in release and a panic in debug, so a 33rd variant
// would be a defect debug builds catch and release builds do not -- the exact shape of
// 3.0.1's `fit_content`. A const assert makes it a compile error instead.
const _: () = assert!(WarnOnce::COUNT <= 32);

/// The message an agent-timeout cut reports, naming the CONFIGURED ceiling.
///
/// # It names the ceiling as the ceiling, and does NOT pretend to a measurement
///
/// `tokio::time::timeout` returns `Elapsed` only when OUR ceiling fires, so the elapsed time here
/// is always exactly the ceiling. An earlier form printed the same value twice, shaped like
/// "measured versus configured" — two identical numbers say nothing, and the shape invited the
/// reader to compare them.
///
/// What it does instead is publish our own number plainly: an operator who sees a cut at 600 s
/// against a configured ceiling of 660 s knows the cut was not ours. That comparison is made by
/// the operator against their own infrastructure, not by this message — and the case it matters
/// for arrives as `ProviderError::Network`, not through this path at all, which the migration
/// guide says explicitly.
///
/// # One function for all four sites
///
/// There are four `tokio::time::timeout` calls on the agent path (the call and its corrective
/// retry, on the rotating path and the non-rotating one). Four separate `format!`s is how three
/// of them end up without the ceiling.
fn agent_timeout_message(is_corrective_retry: bool, ceiling: Duration) -> String {
    let phase = if is_corrective_retry {
        "retry-failed: timeout"
    } else {
        "timeout: agent timed out"
    };
    format!("{phase} at its configured ceiling of {ceiling:?}")
}

impl Magi {
    /// The worst-case wall clock **one seat** can spend, derived from THIS instance's effective
    /// configuration.
    ///
    /// ```text
    /// timeout * calls_per_model * (1 + max_rotations)
    /// ```
    ///
    /// where `calls_per_model` is **2** when [`MagiConfig::retry_on_schema_error`] is on (the
    /// call plus the corrective retry) and **1** when it is off.
    ///
    /// # It informs; it never rejects
    ///
    /// Returns a [`Duration`], not a `Result`. It does not fail the build and does not warn,
    /// however large the number: the moment it rejected something it would be the cap this crate
    /// deliberately does not impose. A consumer who wants an alarm builds it on top of the value
    /// in three lines.
    ///
    /// # Per SEAT, never per run
    ///
    /// Whether the backend serves the three mages in parallel or serialises them is a property of
    /// the deployment. Multiplying by three here would assert a serialisation this crate has not
    /// measured, so the run total is left to the caller, who knows their backend.
    ///
    /// # Transport retries are NOT a separate factor
    ///
    /// They happen **inside** one of those calls, and the agent ceiling wraps **each** call.
    /// Counting them again would count them twice — which is exactly why the ceiling, and not an
    /// estimate per failure class, is the unit here. What a 503 costs before it fails is not
    /// something this crate knows, and estimating it would fabricate precision.
    ///
    /// # A ceiling, not a prediction
    ///
    /// The chain usually ends earlier. This is the bound that can be guaranteed without modelling
    /// every failure path.
    ///
    /// # Why it lives on `Magi` and not on [`MagiConfig`]
    ///
    /// The `(1 + max_rotations)` factor lives in the fallback POOL, not in the config, so this is
    /// the only place all three pieces are visible at once. That makes "reads the EFFECTIVE
    /// configuration" true by construction rather than by convention.
    ///
    /// [`MagiConfig::retry_on_schema_error`]: MagiConfig::retry_on_schema_error
    pub fn worst_case_per_seat(&self) -> Duration {
        let calls_per_model: u32 = if self.config.retry_on_schema_error {
            2
        } else {
            1
        };
        let models: u32 = self
            .rotation_config
            .as_ref()
            .map_or(0, |r| r.pool.max_rotations())
            .saturating_add(1);
        // Saturating rather than wrapping: an absurd configuration must produce an absurd
        // number, never a small one that reads as safe.
        self.config
            .timeout
            .saturating_mul(calls_per_model)
            .saturating_mul(models)
    }

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

    /// Emits a one-shot warning, or says nothing if this instance already said it.
    ///
    /// **One gate for every builder warning.** R-18 was a third warning that never got a
    /// flag while its two neighbours had theirs, so it repeated on every `analyze()`. The
    /// flag's type was never the problem: having one was up to whoever wrote the warning.
    ///
    /// `Relaxed` is sufficient. The guarantee asked for is "at most once", not an ordering
    /// between distinct warnings, and it is what the two `AtomicBool`s this replaces used.
    /// `fetch_or` returns the mask BEFORE the write, so exactly one caller sees the bit
    /// clear however many arrive together.
    pub(crate) fn warn_once(&self, which: WarnOnce<'_>) {
        let bit = 1u32 << which.bit();
        if self
            .warned
            .fetch_or(bit, std::sync::atomic::Ordering::Relaxed)
            & bit
            == 0
        {
            which.emit();
        }
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
    /// - [`MagiError::InsufficientAgents`] if fewer than `min_agents` seats succeed.
    /// - [`MagiError::InvalidInput`] if nonce collision detected (probability ~2^-64
    ///   per call; fastrand effective state ~64 bits).
    /// - [`MagiError::SkippedByComplexityGate`] if a complexity gate was installed and
    ///   returned `false` — before any dispatch, so nothing was spent.
    /// - [`MagiError::EndpointDown`] once connection failures on distinct lineages cross the
    ///   latch AND the seats still in flight can no longer reach `min_agents`: the run is
    ///   abandoned rather than degraded. A run whose seats rotated and recovered continues.
    /// - [`MagiError::CrateDefect`] — **new in `4.0.0`** — when a backend accepts a request and
    ///   generates nothing with its token counters absent. That footprint is a defect of THIS
    ///   crate, so the run aborts instead of rotating, and this call is the only surface it
    ///   reaches a consumer on.
    /// - [`MagiError::Validation`] if consensus is asked to score duplicate agent names.
    ///   **Not** for a seat whose own output fails validation: that seat goes to
    ///   `failed_agents` and the run degrades, which is the opposite contract.
    ///
    /// # Concurrency
    ///
    /// The internal `rng_source` is guarded by a `std::sync::Mutex`, so concurrent
    /// calls to `analyze()` from multiple tasks serialize on nonce generation. In
    /// practice nonce generation is a single `u128` read (~nanoseconds), so
    /// contention is negligible under typical workloads. If profiling shows this
    /// becomes a bottleneck in a multi-tenant deployment, consider wrapping `Magi`
    /// in a pool of instances (one per tenant), or ask for a public `with_rng_source`, which
    /// would allow a thread-local RNG strategy. It was once slated for v0.4; that shipped in
    /// May 2026 without it and the setter is still `#[cfg(test)]`, so treat it as unscheduled.
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
        let input_size = measure_input(content, &self.config);
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
        let (
            successful,
            failed_agents,
            retried_agents,
            rotations,
            extraction_failures,
            completions,
            pool_eligibility,
        ) = self.dispatch_with_retry(agents, &prompt).await?;

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
            completions,
            pool_eligibility,
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
        // Rotation is engaged when a fallback pool OR a primary probe was declared (see
        // `build`). With no
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
        // `completions` is NOT seeded: unlike the extraction certificate, an absent entry
        // here means the seat produced no attempt at all, which is a different claim from
        // an empty one and worth being able to tell apart.
        let mut completions: BTreeMap<AgentName, Vec<CompletionRecord>> = BTreeMap::new();
        // A defect of OURS invalidates the run on this path too. It needs no registry here:
        // there is no rotation to coordinate, and the join loop below is the same point at
        // which the rotating path consults its latch.
        let mut crate_defect: Option<CrateDefectRecord> = None;
        let mut extraction_failures: BTreeMap<AgentName, Vec<ExtractionFailure>> = agent_models
            .keys()
            .map(|name| (*name, Vec::new()))
            .collect();
        for (name, handle) in handles {
            match handle.await {
                Ok((Ok(output), was_retried, failures, records, defect)) => {
                    successful.push(output);
                    if was_retried {
                        retried.insert(name);
                    }
                    extraction_failures.insert(name, failures);
                    completions.insert(name, records);
                    crate_defect = crate_defect.or(defect);
                }
                Ok((Err(reason), was_retried, failures, records, defect)) => {
                    failed.insert(name, reason);
                    if was_retried {
                        retried.insert(name);
                    }
                    extraction_failures.insert(name, failures);
                    completions.insert(name, records);
                    // SET-ONCE, exactly like the registry latch the rotating path uses: if two
                    // seats hit it the result is the same abort, and no state depends on which
                    // one was joined first.
                    crate_defect = crate_defect.or(defect);
                }
                Err(join_err) => {
                    // A panicked task loses its in-flight records. `extraction_failures` is
                    // pre-seeded so its empty Vec stands; `completions` deliberately is NOT, so
                    // this seat gets no entry — and by that field's own definition an absent
                    // entry means "made no attempt", which is not what happened. It is left
                    // absent anyway: seeding an empty Vec would claim a seat that attempted
                    // twice attempted nothing, and the panic is the headline either way.
                    failed.insert(name, format!("panic: {join_err}"));
                }
            }
            // Checked INSIDE the loop, on the same beat as the rotating path consults its
            // latch. Draining every handle first made `AbortGuard` inert here — nothing was left
            // in flight to cancel — and made the error's own documentation false, since
            // `joined_before_abort` would then always list every seat.
            if let Some(d) = crate_defect.take() {
                // The SAME two functions the rotating path uses. Written out inline here once,
                // and the copies diverged in ORDER while a comment claimed they were symmetric.
                return Err(crate_defect_error(d, &joined_so_far(&successful, &failed)));
            }
        }

        let min_agents = self.consensus_engine.min_agents();
        if successful.len() < min_agents {
            return Err(MagiError::InsufficientAgents {
                succeeded: successful.len(),
                required: min_agents,
            });
        }

        // No pool on this path, so no candidate can be rejected. Seeded per seat rather
        // than left empty: absent means "not computed", which is a different claim.
        //
        // From `agent_models` — the same source its sibling `extraction_failures` uses,
        // and taken before `default_rotations` consumes it. Deriving it from `successful`
        // plus `failed` reaches the same set, but only because every handle lands in
        // exactly one of them: a proof the reader has to redo, where this states it.
        let pool_eligibility = agent_models.keys().map(|a| (*a, Vec::new())).collect();
        let rotations = default_rotations(agent_models);
        Ok((
            successful,
            failed,
            retried,
            rotations,
            extraction_failures,
            completions,
            pool_eligibility,
        ))
    }

    /// The rotation dispatch path. Seeds a per-run [`LineageRegistry`] from the
    /// trio's declared primary lineages (or a synthetic per-agent lineage when a
    /// pool is declared but a primary's lineage was not), spawns the rotation FSM
    /// ([`dispatch_one_agent_rotating`]) per agent, collects the real per-agent
    /// [`AgentRotation`] chains, and enforces the endpoint-down fast-fail: after
    /// EVERY agent outcome (success, failure, OR panic/`JoinError`) it consults the
    /// registry latch and, if set AND the quorum is no longer reachable, returns
    /// `Err(EndpointDown)` **before** consensus — the single source of truth, robust
    /// to a panicked latch-holder, and never at the expense of a run that recovered.
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
        // Cloned before the registry takes ownership: the snapshot needs each seat's
        // lineage and model, and taking them back out of the registry would mean an
        // `await` under its lock for data we already hold here.
        let initial_for_snapshot = initial.clone();
        let registry = Arc::new(LineageRegistry::new(initial));

        // Preflight (R15): probe every probe-capable model (trio primaries + pool
        // candidates) ONCE, CONCURRENTLY, before dispatch — caching window/digest so
        // the pure rotation policy reads them with zero I/O and never under the lock.
        // A failed/timed-out probe degrades to unmeasured (fail-open, G3) — never an
        // abort. Providers without a probe contribute nothing (no window/digest).
        let probe_targets = collect_probe_targets(&agent_models, &rotation);
        // Latched like the inert-guard warning below, and for a stronger reason: what these
        // name is decided when the builder runs, so a second telling would describe a state
        // that provably has not changed since the first.
        self.warn_once(WarnOnce::ProbeDeclaration {
            targets: &probe_targets,
        });
        let capabilities = Arc::new(run_preflight(probe_targets).await);
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

        // MS3 — computed ONCE per run, here: the prompt exists (so `min_window_tokens`
        // does) and no seat has been dispatched yet, which is exactly what the field
        // claims. `&BTreeMap::new()` / `&BTreeSet::new()` are not placeholders: nobody
        // has failed anything yet, and reading the registry here would make the snapshot
        // depend on when it was called and falsify the one thing its rustdoc promises.
        let pool_eligibility =
            crate::rotation::pool_eligibility_snapshot(&crate::rotation::EligibilityInputs {
                seats: &initial_for_snapshot,
                progress: &BTreeMap::new(),
                run_failed_lineages: &BTreeSet::new(),
                capabilities: &capabilities,
                candidates: rotation.pool.candidates(),
                max_rotations: rotation.pool.max_rotations(),
                min_window_tokens,
                strict_context_guard,
            });

        // A strict guard rejects every UNMEASURED candidate, so with nothing measured the pool
        // is declared and never eligible: rotation does nothing, and until now it did so in
        // silence. Naming it is all that happens here — the filter is untouched.
        let candidate_models: Vec<String> = rotation
            .pool
            .candidates()
            .iter()
            .map(|c| c.provider.model().to_string())
            .collect();
        // `max_rotations(0)` disables rotation by configuration, and the pool never reaches the
        // window filter at all — so the pool being unmeasured is not why nothing rotates, and
        // this message's remedy ("declare a probe, or turn the guard off") would be wrong advice
        // for a state the consumer chose deliberately. A warning that misdiagnoses gets silenced,
        // and then the real case is invisible.
        //
        // This reports a CONFIGURATION condition but can only be detected after the preflight,
        // so it lives here rather than in `build()`. The latch is what keeps that from turning
        // into a line repeated on every call of a long-lived orchestrator — see the field.
        if rotation.pool.max_rotations() > 0
            && strict_guard_is_inert(strict_context_guard, &candidate_models, &capabilities)
        {
            self.warn_once(WarnOnce::InertGuard {
                candidates: candidate_models.len(),
            });
        }

        // Pre-seed telemetry OUTSIDE any task stack so a panicked agent still has a
        // present, chain-empty record (W1). A normal return replaces its entry.
        let mut rotations = default_rotations(agent_models);
        // Seeded per agent: an empty Vec is the positive certificate that the seat was
        // clean, and it keeps this map joinable with the rotations map on the same key.
        let mut extraction_failures: BTreeMap<AgentName, Vec<ExtractionFailure>> =
            rotations.keys().map(|name| (*name, Vec::new())).collect();
        // Not seeded, unlike the certificate above: an absent entry here means the seat made
        // no attempt at all, which is a different claim from an empty one.
        let mut completions: BTreeMap<AgentName, Vec<CompletionRecord>> = BTreeMap::new();

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

        // The two magnitudes of the abort criterion, taken BEFORE the loop: `min_agents`
        // was computed after it, and the `for` below consumes `handles` by value, so the
        // count of seats still to join has to be captured here and derived per iteration.
        let min_agents = self.consensus_engine.min_agents();
        let total_handles = handles.len();

        let mut successful = Vec::new();
        let mut failed = BTreeMap::new();
        let mut retried = std::collections::BTreeSet::new();
        // ABNORMAL EXIT: the endpoint-down latch — NOT the per-agent error payload —
        // is the single source of truth, so it MUST be consulted after EVERY outcome
        // (success, normal failure, OR panic) before ANY return/continue. Do not drop
        // this check in a refactor; a panicked latch-holder that never propagated the
        // signal is recovered here (R8/W11).
        //
        // CONSULTED every time, ACTED ON only when the run can no longer reach its quorum:
        // the latch turns on at the second condemned lineage, but the seats that condemned
        // them may rotate and succeed before the loop reaches their join. The conjunction
        // lives in `resolve_endpoint_down`; this loop only supplies the numbers, fresh on
        // every iteration.
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
        // With the quorum condition the abort can also land on the FINAL join, when the
        // seats joined earlier kept the quorum reachable until the last one failed: no
        // stragglers to cancel then, and the latency is the whole run. That is the
        // price of never abandoning a run that could still have recovered.
        // Optimizing that out-of-scope multi-host case is deliberately not done here.
        for (name, handle) in handles {
            match handle.await {
                Ok((Ok(output), agent_rotation, was_retried, failures, records)) => {
                    rotations.insert(name, agent_rotation);
                    extraction_failures.insert(name, failures);
                    completions.insert(name, records);
                    successful.push(output);
                    if was_retried {
                        retried.insert(name);
                    }
                }
                Ok((Err(reason), agent_rotation, was_retried, failures, records)) => {
                    rotations.insert(name, agent_rotation);
                    extraction_failures.insert(name, failures);
                    completions.insert(name, records);
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
                    // The SAME resolution the normal arm uses. It used to consult only the
                    // endpoint-down latch and then `continue`, so a crate defect latched by a
                    // concurrent seat was skipped on every panicked join.
                    let joined = joined_so_far(&successful, &failed);
                    if let Some(err) = resolve_abnormal_exit(
                        name,
                        &join_err,
                        &registry,
                        &joined,
                        successful.len(),
                        remaining_seats(total_handles, &joined),
                        min_agents,
                    )
                    .await
                    {
                        return Err(err);
                    }
                    continue;
                }
            }
            // Normal outcome: a concurrent mage may still have tripped either latch.
            // `AbortGuard` cancels whatever is still in flight when this returns — the existing
            // mechanism doing its job, not a new one.
            let joined = joined_so_far(&successful, &failed);
            if let Some(err) = resolve_run_abort(
                &registry,
                &joined,
                successful.len(),
                remaining_seats(total_handles, &joined),
                min_agents,
            )
            .await
            {
                return Err(err);
            }
        }

        if successful.len() < min_agents {
            return Err(MagiError::InsufficientAgents {
                succeeded: successful.len(),
                required: min_agents,
            });
        }

        Ok((
            successful,
            failed,
            retried,
            rotations,
            extraction_failures,
            completions,
            pool_eligibility,
        ))
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

/// Records ONE completion attempt, whatever it turned into.
///
/// # THE single place that decides what a record says
///
/// Success and failure both land here, in the one place that still knows the agent's model and
/// the budget it was given. A second construction site is how an attempt stops being recorded —
/// or starts being recorded differently — without anything failing.
///
/// # Parameters
///
/// - `records` — the seat's accumulator, in attempt order.
/// - `model` / `cap` — from the CALLER. A provider knows neither which budget it was handed nor
///   which seat it served, so neither is readable off a response.
/// - `outcome` — exactly what the timed call returned, before it is destructured.
///
/// # What a failed attempt records
///
/// Only [`ProviderError::EmptyCompletion`] knows anything past the model and the cap, and what it
/// knows is the termination reason. Everything else declares absence: `None` and
/// [`ReasoningState::NotMeasured`], never zeros, because a zero meaning "nobody counted" is
/// indistinguishable from a real one.
///
/// # What it deliberately does NOT record
///
/// [`ProviderError::NoGeneration`] is a defect in this crate: the run aborts and its report will
/// not exist, so a record for that attempt would assert there was something to measure. The guard
/// lives here rather than in the caller's match so the decision is not split in two.
///
/// # Complexity
///
/// O(n) in the length of a carried reasoning trace; O(1) otherwise.
fn record_attempt(
    records: &mut Vec<CompletionRecord>,
    model: &str,
    cap: u32,
    outcome: &Result<Result<Completion, ProviderError>, tokio::time::error::Elapsed>,
) {
    match outcome {
        Ok(Ok(completion)) => records.push(CompletionRecord::from_telemetry(
            model.to_string(),
            cap,
            &completion.telemetry,
        )),
        // Recorded NOWHERE, and this is CONTINGENT on the caller aborting: the run ends with
        // `Err`, so no `MagiReport` is ever built and a record here would die in a local
        // `Vec`. If that consequence is ever softened — a defect of ours degrading the seat
        // instead of ending the run — this arm silently starts DROPPING attempts that would
        // then have somewhere to go. Change the two together.
        Ok(Err(ProviderError::NoGeneration { .. })) => {}
        // Symmetric with the success arm on purpose: an empty completion now carries the SAME
        // telemetry a successful one does, so the record is filled the same way. It used to
        // keep only the termination reason, which said the completion was cut and threw away
        // the reasoning length that explains WHY -- on the one failure this release exists to
        // diagnose.
        Ok(Err(ProviderError::EmptyCompletion { telemetry, .. })) => records.push(
            CompletionRecord::from_telemetry(model.to_string(), cap, telemetry),
        ),
        // Every other failure, and a timeout: the attempt happened and nothing was measured.
        Ok(Err(_)) | Err(_) => records.push(CompletionRecord::new(model.to_string(), cap)),
    }
}

/// Whether a surfaced provider failure is a defect of THIS crate, and what to record if so.
///
/// # Why it asks the classifier instead of matching the variant here
///
/// A second place deciding what a crate defect is would be a second place that can disagree with
/// the first. `provider_err_outcome` already owns that decision, exhaustively and without a
/// catch-all, so this asks it rather than re-answering.
///
/// # Parameters
///
/// * `err` — the failure as the provider surfaced it.
/// * `agent` / `model` — the seat and the model in force, neither of which the classifier knows.
///
/// # Returns
///
/// `Some` only for the one classification whose consequence is the whole run.
///
/// # Complexity
///
/// O(n) in the rendered error text, which the classifier composes once.
fn crate_defect_of(err: ProviderError, agent: AgentName, model: &str) -> Option<CrateDefectRecord> {
    match provider_err_outcome(err) {
        ModelOutcome::CrateDefect {
            observation,
            hypothesis,
        } => Some(CrateDefectRecord {
            observation,
            hypothesis,
            agent,
            model: model.to_string(),
        }),
        _ => None,
    }
}

/// Dispatch a single agent with one-shot retry on schema/parse errors.
///
/// Returns a flat 5-tuple rather than an enum:
///
/// 1. `Ok(output)` on success (first or second attempt), `Err(reason)` on failure.
/// 2. `true` if a retry attempt was made, whatever its outcome — this is what populates
///    [`MagiReport::retried_agents`].
/// 3. The extraction failures this seat accumulated, in attempt order.
/// 4. One [`CompletionRecord`] per completion ATTEMPT, success or failure alike.
/// 5. `Some` when the failure was a defect of THIS crate, which the caller raises to abort the
///    run. `None` for every ordinary failure.
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
) -> (
    Result<AgentOutput, String>,
    bool,
    Vec<ExtractionFailure>,
    Vec<CompletionRecord>,
    Option<CrateDefectRecord>,
) {
    // Attribution is STRUCTURAL here: the model is the one whose provider actually ran,
    // so a failure can never be credited to a model that had not executed yet (E23c).
    let model = agent.provider_model().to_string();
    let mut failures: Vec<ExtractionFailure> = Vec::new();
    let mut records: Vec<CompletionRecord> = Vec::new();
    // First attempt.
    let first_result = tokio::time::timeout(timeout, agent.execute(&user_prompt, &config)).await;
    record_attempt(&mut records, &model, config.max_tokens, &first_result);
    let first_raw = match first_result {
        Ok(Ok(raw)) => raw,
        Ok(Err(provider_err)) => {
            // Routed through the SAME classifier the rotating path uses, so the two cannot
            // disagree about what counts as a defect of ours. Only that one case changes the
            // control flow; every other failure keeps the reason string it always had.
            //
            // Found by review: the abort existed only on the rotating path, so a defect of
            // ours in the DEFAULT configuration — no fallback pool — degraded the run to 2/3
            // and filed itself among ordinary model failures, which is exactly what B-5 says
            // it must never do.
            let reason = MagiError::Provider(provider_err.clone()).to_string();
            let defect = crate_defect_of(provider_err, agent.name(), &model);
            return (Err(reason), false, failures, records, defect);
        }
        Err(_elapsed) => {
            return (
                Err(agent_timeout_message(false, timeout)),
                false,
                failures,
                records,
                None,
            );
        }
    };

    // Parse + validate first response. Success exits here.
    let first_err = match parse_validate_and_check(&first_raw.text, agent.name(), &validator) {
        Ok(output) => return (Ok(output), false, failures, records, None),
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
        return (
            Err(first_err.error.to_string()),
            false,
            failures,
            records,
            None,
        );
    }

    // Single-shot retry with corrective feedback prompt.
    let retry_prompt = build_retry_prompt(
        &user_prompt,
        first_err.cause,
        &first_err.error.to_string(),
        // What the FIRST attempt's backend said about why it stopped. The crate has it in
        // hand here, and it is the difference between telling a model not to stop and
        // telling it the budget stopped it.
        first_raw.telemetry.finish.clone(),
    );
    let second_result = tokio::time::timeout(timeout, agent.execute(&retry_prompt, &config)).await;
    record_attempt(&mut records, &model, config.max_tokens, &second_result);
    let second_raw = match second_result {
        Ok(Ok(raw)) => raw,
        Ok(Err(provider_err)) => {
            let reason = format!(
                "retry-failed: {}",
                MagiError::Provider(provider_err.clone())
            );
            let defect = crate_defect_of(provider_err, agent.name(), &model);
            return (Err(reason), true, failures, records, defect);
        }
        Err(_elapsed) => {
            return (
                Err(agent_timeout_message(true, timeout)),
                true,
                failures,
                records,
                None,
            );
        }
    };

    match parse_validate_and_check(&second_raw.text, agent.name(), &validator) {
        Ok(output) => (Ok(output), true, failures, records, None),
        Err(f) => {
            // `attempt: 2` — the corrective retry, on the SAME model. The counter is
            // per-model by construction, so it restarts at 1 if a rotation happens later
            // (E23c): that is what keeps "this model failed on its first try" readable.
            failures.push(ExtractionFailure {
                model,
                attempt: 2,
                cause: f.cause,
            });
            (
                Err(format!("retry-failed: {}", f.error)),
                true,
                failures,
                records,
                None,
            )
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

/// The rotation detail for an oversized response body.
///
/// # Parameters
/// - `limit`: the byte cap the body exceeded.
///
/// # Why these are functions and not `format!` at the call site
///
/// The call site is the rotation loop, which is `async` and needs a provider, a registry and a
/// live agent to reach. A test written against that has to stand up the whole machine to check a
/// string, so in practice it never gets written — and the marker is exactly the kind of detail
/// that rots unobserved. Pulled out, each mapping is total, pure, and testable in two lines, while
/// the loop runs this same code. Returning `String` rather than `Option<String>` keeps the caller
/// from needing a fallback it would have to invent.
fn oversized_detail(limit: usize) -> String {
    format!("response body exceeded {limit} bytes")
}

/// The rotation detail for a failure reported by a third-party backend.
///
/// # Parameters
/// - `kind`: the shape the external provider declared.
/// - `detail`: the rendered error, already capped by the constructor.
fn external_failure_detail(kind: ExternalErrorKind, detail: &str) -> String {
    format!("external ({kind:?}): {detail}")
}

/// Rough chars-per-token ratio for the coarse `min_window_tokens` pre-filter
///. Not precise budgeting — the crate is char-based and adds no tokenizer
/// dependency; this only rejects candidates smaller than the raw prompt needs.
///
/// Deliberately SEPARATE from the report's byte-based divisor, despite both being 4 today. This
/// one counts characters and rounds up, because a pre-filter that under-estimates would admit a
/// candidate whose window cannot hold the prompt; that one counts bytes and rounds down, because
/// telemetry must not overstate. Unifying them would force one of the two to round the wrong way.
const CHARS_PER_TOKEN_EST: usize = 4;

/// Collects the preflight probe targets: each probing PRIMARY (paired with its
/// agent's model) plus each pool candidate that declared a probe. Non-probing
/// providers contribute nothing — they simply have no window/digest to measure.
///
/// Two conditions are NAMED here rather than left silent, because both became reachable
/// only once a probe could be declared apart from the provider it measures for, and both
/// are invisible from the outside: a probe that says it speaks for a different model than
/// the one it is filed under, and the same model filed twice. Neither rejects — the first
/// because a probe is not authoritative over what a provider serves, the second because
/// the surviving answer may well be correct. Warning is what the crate can honestly do.
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

/// Names the two ways a decoupled probe declaration can be wrong without anything failing:
/// a probe filed under a model it says is not its own, and one model filed twice.
///
/// Pure except for the `tracing` calls, and total. Split out so the collection above stays
/// a collection, and so the conditions can be exercised without a preflight.
fn warn_on_probe_disagreement(targets: &[(String, Arc<dyn ProviderProbe>)]) {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for (model, probe) in targets {
        if let Some(declared) = probe.declared_model()
            && declared != model
        {
            // The window lands under `model` while the probe measured `declared`, and the
            // same key drives the digest collision check — the one place in this subsystem
            // that REJECTS. A wrong digest there can turn a healthy candidate away.
            tracing::warn!(
                filed_under = %model,
                probe_declares = %declared,
                "a probe declares a different model than the provider it was registered \
                 with; its measurement will be filed under the provider's model and can \
                 mis-drive the digest collision check"
            );
        }
        if !seen.insert(model.as_str()) {
            // The preflight collects into a map keyed by model, so the later answer wins —
            // and "later" is completion order, not declaration order.
            tracing::warn!(
                model = %model,
                "two probes are registered for the same model; whichever answers last wins, \
                 and that order is not deterministic"
            );
        }
    }
}

/// Classifies a surfaced [`ProviderError`] as a connection-level failure for the
/// endpoint-down fast-fail.
///
/// **Only [`ProviderError::Network`]** (connection refused / host unreachable /
/// DNS) counts as connection evidence. An `Http` (incl. 5xx), a `Timeout`, or a
/// `RetryAbandoned` condemns the lineage run-wide but is **not** connection
/// evidence — someone joined, or the model is merely slow.
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
        // NEVER counts toward the endpoint-down latch, whatever shape it declares. Even
        // `ExternalErrorKind::Network` means only that a third-party backend was unreachable —
        // this crate has no way to know whether that says anything about the lineages the OTHER
        // two seats are using, and aborting the whole run on that guess is unrecoverable.
        ProviderError::External { .. } => false,
        // None of the three is connection-class: in all of them the endpoint ANSWERED. The
        // endpoint-down latch exists for a backend that cannot be reached, and feeding it from a
        // response that arrived would make the run abort on a healthy endpoint.
        ProviderError::ResponseContract { .. }
        | ProviderError::EmptyCompletion { .. }
        | ProviderError::NoGeneration { .. } => false,
    }
}

/// Outcome of a single model attempt (including its own corrective schema retry).
///
/// `Debug` so a failing classifier test can say what it got instead of only what it wanted. The
/// type is private, so this adds nothing to the public surface.
#[derive(Debug)]
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
    /// A content/contract failure the endpoint answered with — **mage-local**, then rotate.
    ///
    /// Separate from [`ModelOutcome::Transport`] because that arm calls
    /// `register_transport_failure`, which condemns the lineage **run-wide**. `connection` only
    /// governs the endpoint-down latch, NOT the scope of the condemnation, so routing a
    /// mage-local cause through `Transport` with `connection: false` would still take the
    /// lineage away from the other two seats — which is the defect this milestone exists to fix.
    MageLocal { detail: String, kind: RotationKind },
    /// The backend generated nothing at all: a defect in THIS crate, not a model failure.
    ///
    /// Carries only what the classifier OBSERVED. It does not build the final error, because
    /// that needs facts the classifier cannot have — which seats had already answered — and
    /// filling those with an empty vector would read as "none" when it means "not yet".
    ///
    /// Does not rotate, by construction: rotating would reproduce our own bad request against
    /// every seat in turn.
    CrateDefect {
        observation: String,
        hypothesis: &'static str,
    },
    /// Body over the cap on a successful response — **mage-local**, then rotate.
    ///
    /// Not `Transport`: that is run-wide and feeds the endpoint-down latch. Not `Schema`: nothing
    /// failed to parse. Its own variant so the `match` below forces the consequence to be decided
    /// rather than inherited.
    OversizedResponse { limit: usize },

    /// Failure surfaced by a provider implemented outside this crate — **mage-local**, then
    /// rotate.
    ///
    /// Not `Transport`: that is run-wide and feeds the endpoint-down latch, and this crate cannot
    /// know whether a third-party backend's failure says anything about the lineage the other
    /// seats are on. Not `Schema`: nothing failed to parse, so saying so would lie in both the
    /// detail and the telemetry.
    ExternalFailure {
        detail: String,
        kind: ExternalErrorKind,
    },

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
    records: &mut Vec<CompletionRecord>,
) -> ModelOutcome {
    // First attempt.
    let first =
        tokio::time::timeout(timeout, agent.execute_with(provider, user_prompt, config)).await;
    record_attempt(records, provider.model(), config.max_tokens, &first);
    let first_raw = match first {
        Ok(Ok(raw)) => raw,
        Ok(Err(provider_err)) => return provider_err_outcome(provider_err),
        Err(_elapsed) => {
            return ModelOutcome::Transport {
                detail: agent_timeout_message(false, timeout),
                connection: false,
                kind: RotationKind::Timeout,
            };
        }
    };

    let first_err = match parse_validate_and_check(&first_raw.text, agent.name(), validator) {
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
    let retry_prompt = build_retry_prompt(
        user_prompt,
        first_err.cause,
        &first_err.error.to_string(),
        first_raw.telemetry.finish.clone(),
    );
    let second =
        tokio::time::timeout(timeout, agent.execute_with(provider, &retry_prompt, config)).await;
    record_attempt(records, provider.model(), config.max_tokens, &second);
    let second_raw = match second {
        Ok(Ok(raw)) => raw,
        Ok(Err(provider_err)) => return provider_err_outcome(provider_err),
        Err(_elapsed) => {
            return ModelOutcome::Transport {
                detail: agent_timeout_message(true, timeout),
                connection: false,
                kind: RotationKind::Timeout,
            };
        }
    };
    match parse_validate_and_check(&second_raw.text, agent.name(), validator) {
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
/// # Why two variants are singled out
///
/// Both are failures this crate must not let condemn a lineage **run-wide** — taking it away from
/// the other two seats over what one seat observed — and, for a connection-class error, feed the
/// endpoint-down latch. Each gets its own outcome so the consequence is decided here rather than
/// inherited.
///
/// An oversized body is a **content** failure, not a transport one: the server answered perfectly,
/// it answered too much. An external failure is a report from a backend this crate did not write,
/// so it cannot say anything about the lineages the other seats are using.
fn provider_err_outcome(err: ProviderError) -> ModelOutcome {
    // EXHAUSTIVE, with no catch-all, and that is the whole point of the shape. An `if let` chain
    // ending in a fallthrough sent every unrecognised variant to `Transport` — which is run-wide
    // and feeds the endpoint-down latch, so the most consequential routing in this function was
    // also its default. A variant added later would have inherited it in silence.
    //
    // The sibling classifiers are already exhaustive for the same reason; this one was the gap.
    // Adding a variant now stops the build here and asks what its consequence should be.
    let connection = is_connection(&err);
    match err {
        ProviderError::ResponseTooLarge { limit } => ModelOutcome::OversizedResponse { limit },
        ProviderError::External { kind, .. } => ModelOutcome::ExternalFailure {
            detail: MagiError::Provider(err).to_string(),
            kind,
        },
        ProviderError::Timeout { .. } => ModelOutcome::Transport {
            detail: MagiError::Provider(err).to_string(),
            connection,
            kind: RotationKind::Timeout,
        },
        // A REAL HTTP status, and every one of them keeps the run-wide route -- including the
        // per-candidate-looking ones. `404 model not found` and `400` are the tempting
        // exceptions: they say something about ONE candidate, so condemning its whole lineage
        // costs the other two seats a model that may be fine.
        //
        // They stay here on purpose, and the reason is the one this crate already applies in
        // the other direction. Mage-local is the safe default when the crate CANNOT tell what a
        // failure implies; here it can tell far less than the status suggests. A `404` from a
        // gateway, a proxy, or a load balancer says nothing about a model — and this crate
        // cannot distinguish those from a daemon that genuinely lacks the tag, because they are
        // the same status on the same wire. Splitting on the number would claim a diagnosis
        // nobody made.
        //
        // What makes leaving it acceptable is that it is NOT new and NOT what this release is
        // about: `3.2.0` routed these identically through the compat provider, so nothing
        // regresses. Narrowing it needs its own evidence, the way `think: false` got measured
        // rather than assumed.
        ProviderError::Http { .. }
        | ProviderError::Network { .. }
        | ProviderError::Auth { .. }
        | ProviderError::Process { .. }
        | ProviderError::RetryAbandoned { .. }
        | ProviderError::NestedSession => ModelOutcome::Transport {
            detail: MagiError::Provider(err).to_string(),
            connection,
            kind: RotationKind::Transport,
        },
        ProviderError::ResponseContract { .. } => ModelOutcome::MageLocal {
            detail: MagiError::Provider(err).to_string(),
            kind: RotationKind::ResponseContract,
        },
        ProviderError::EmptyCompletion { .. } => ModelOutcome::MageLocal {
            detail: MagiError::Provider(err).to_string(),
            kind: RotationKind::EmptyCompletion,
        },
        // The observation and the hypothesis travel as SEPARATE fields, so the distinction
        // survives however someone later formats the message.
        ProviderError::NoGeneration { done_reason } => ModelOutcome::CrateDefect {
            observation: format!(
                "no generation - token counters absent (termination: {done_reason:?})"
            ),
            hypothesis: CRATE_DEFECT_HYPOTHESIS,
        },
    }
}

/// The one cause known to produce an accepted request that generates nothing.
///
/// Stated as a hypothesis and kept apart from the observation because it rests on a single
/// captured case. A second cause with the same footprint would not make the observation wrong;
/// it would make this wrong, and whoever finds it has to be able to tell which was which.
const CRATE_DEFECT_HYPOTHESIS: &str =
    "the known cause is a request without `messages`, which points at a defect in magi-core";

/// Returns `Some(MagiError::EndpointDown)` iff the registry's endpoint-down latch is set
/// AND the run can no longer reach its quorum; `None` otherwise. The `lineages` come from
/// the run's connection-condemned set.
///
/// # The conjunction, and why it lives here and nowhere else
///
/// The latch is evaluated early and consulted late: it turns on at the second condemned
/// lineage, but the seats that condemned them may rotate and succeed before the join loop
/// reaches them. Acting on the latch alone aborted after the FIRST successful join and
/// cancelled two seats that were about to succeed. So the latch decides the DIAGNOSIS —
/// `EndpointDown`, naming the condemned lineages — and the quorum decides whether to ACT on
/// it now. Both halves are needed: without the quorum half a recovered run is thrown away;
/// without the latch half an unreachable quorum for any other cause would be reported as an
/// outage nobody observed, where `InsufficientAgents` belongs.
///
/// The latch is still READ on every call and only the action is conditioned: a caller that
/// cached it before its loop would miss a signal that arrives mid-loop, which is exactly
/// what the per-outcome check exists to recover. The panic arm inherits this by delegation
/// through [`resolve_run_abort`], so the rule is written once.
///
/// # Parameters
///
/// * `successful` — seats that have already joined with an output.
/// * `remaining` — seats dispatched but not yet joined, each of which may still succeed.
/// * `min_agents` — the quorum the consensus engine requires.
async fn resolve_endpoint_down(
    reg: &LineageRegistry,
    successful: usize,
    remaining: usize,
    min_agents: usize,
) -> Option<MagiError> {
    if !reg.endpoint_down_signalled().await {
        return None;
    }
    if quorum_reachable(successful, remaining, min_agents) {
        return None;
    }
    Some(MagiError::EndpointDown {
        lineages: reg.connection_failed_lineages().await,
    })
}

/// Whether the run can still reach its quorum: every seat already successful plus every
/// seat not yet joined — each of which may still succeed — against `min_agents`.
///
/// With `min_agents <= 1` this is vacuously true whenever anything is done or pending, so
/// the endpoint-down abort never fires there — deliberately: the operator declared that one
/// verdict suffices, and cancelling a run that can still produce it would be the defect this
/// criterion removes.
fn quorum_reachable(successful: usize, remaining: usize, min_agents: usize) -> bool {
    successful + remaining >= min_agents
}

/// Seats dispatched but not yet joined.
///
/// DERIVED, never maintained: `joined` holds one entry per seat the loop has processed, so
/// `total - joined.len()` is the count of handles still to await, and the subtraction cannot
/// underflow because every joined seat was dispatched. Its failure direction is declared: if
/// a seat ever resolved without entering `successful` or `failed`, `joined` would undercount
/// and this would OVERcount, so the abort criterion would lean towards continuing and the
/// run would fall through to `InsufficientAgents` at the end of the loop — never towards
/// aborting a run that could still recover.
fn remaining_seats(total: usize, joined: &BTreeMap<AgentName, ()>) -> usize {
    total - joined.len()
}

/// The seats already JOINED when an abort was reached.
///
/// One owner, because this rule has drifted once already: an earlier version of the
/// non-rotating path counted successes only and claimed to be symmetric with this one.
///
/// The name says `joined`, not `answered`, and the distinction is load-bearing — membership is
/// decided by dispatch and join ORDER, so a seat that answered while this was being built is
/// absent. It is a diagnostic hint, never a census.
fn joined_so_far(
    successful: &[AgentOutput],
    failed: &BTreeMap<AgentName, String>,
) -> BTreeMap<AgentName, ()> {
    successful
        .iter()
        .map(|o| (o.agent, ()))
        .chain(failed.keys().map(|n| (*n, ())))
        .collect()
}

/// Builds the run-aborting error from a latched defect.
///
/// # Why the EXCLUSION lives here and not at the two call sites
///
/// The seat that hit the defect is dropped, because it already travels as `agent` and counting
/// it would make the field disagree with its own documentation. That rule was written out twice,
/// and the two copies differed: one produced a sorted set and the other produced whatever order
/// the successes happened to be in. Neither was wrong, but a field whose contents depend on
/// which dispatcher ran is a field nobody can reason about.
fn crate_defect_error(d: CrateDefectRecord, joined: &BTreeMap<AgentName, ()>) -> MagiError {
    MagiError::CrateDefect {
        observation: d.observation,
        hypothesis: d.hypothesis,
        agent: d.agent,
        model: d.model,
        joined_before_abort: joined.keys().copied().filter(|a| *a != d.agent).collect(),
    }
}

/// Resolves whether the run must abort, and in WHICH order the two reasons are considered.
///
/// # The order is the invariant, not an implementation detail
///
/// A defect of THIS crate is raised BEFORE endpoint-down. Both can be latched at once — a bad
/// request of ours reaches one seat while two other lineages genuinely lose their connection —
/// and whichever is reported is the one the operator investigates. Reporting the outage would
/// send them to inspect an environment that is not at fault, which is the exact misdirection
/// this milestone exists to remove, recreated one level up.
///
/// Losing the outage costs nothing: it is environmental, it persists, and the next run reports
/// it. Losing the defect costs the bug, because it hides in the noise of the normal.
///
/// It is a FUNCTION rather than two calls at each site because it had already drifted: the
/// panic arm consulted one latch and skipped the other entirely.
///
/// # Only the endpoint-down branch is conditioned on the quorum
///
/// A defect of this crate aborts unconditionally — its cost is bounded by construction, see
/// [`resolve_crate_defect`] — and the quorum numbers are passed straight through to
/// [`resolve_endpoint_down`], which is where the conjunction lives. Wrapping this whole body
/// instead would silently disable the crate-defect abort while the quorum is reachable, and
/// `a_crate_defect_aborts_even_when_the_quorum_is_reachable` is what tells the two readings
/// apart.
///
/// # Parameters
///
/// * `successful`, `remaining`, `min_agents` — the quorum magnitudes the join loop has at
///   hand, see [`resolve_endpoint_down`]. Loose numbers rather than a struct: their only use
///   is crossing this call.
async fn resolve_run_abort(
    reg: &LineageRegistry,
    joined_before_abort: &BTreeMap<AgentName, ()>,
    successful: usize,
    remaining: usize,
    min_agents: usize,
) -> Option<MagiError> {
    if let Some(err) = resolve_crate_defect(reg, joined_before_abort).await {
        return Some(err);
    }
    resolve_endpoint_down(reg, successful, remaining, min_agents).await
}

/// Raises a latched defect of THIS crate into the run-aborting error.
///
/// # Parameters
///
/// * `reg` — the run's registry, where the seat that hit it left the record.
/// * `joined_before_abort` — the seats that had already been joined when the abort was reached. Known
///   only here: the registry never learns it, which is why the record does not carry it.
///
/// # Returns
///
/// `Some` when a defect was latched, in which case the caller must return it and abandon the
/// run. `None` otherwise.
///
/// # Why it aborts rather than degrading the seat
///
/// `failed_agents` is where model failures land every day, so a bug of ours filed there is
/// invisible in the noise of the normal. The cost of aborting is bounded by construction: the
/// discriminant is that NO generation happened, so the backend answers in fractions of a second
/// and the other seats have barely started.
async fn resolve_crate_defect(
    reg: &LineageRegistry,
    joined_before_abort: &BTreeMap<AgentName, ()>,
) -> Option<MagiError> {
    reg.crate_defect()
        .await
        .map(|d| crate_defect_error(d, joined_before_abort))
}

/// Lost-signal recovery for an ABNORMAL agent exit (panic / `JoinError`), factored
/// out for race-free unit testing.
///
/// A panicked task loses its transport classification, so the decision derives
/// **solely** from the registry latches — never from `err`. `agent`/`err` document
/// the call site (and feed a diagnostic `tracing` event); the verdict is exactly
/// [`resolve_run_abort`], with the quorum magnitudes PASSED THROUGH and never
/// evaluated here, so the panic arm applies the same criterion as the normal arm
/// by construction. This catches a latch-holder that crossed the endpoint-down
/// threshold and then died before propagating the signal.
pub(crate) async fn resolve_abnormal_exit(
    agent: AgentName,
    err: &tokio::task::JoinError,
    reg: &LineageRegistry,
    joined_before_abort: &BTreeMap<AgentName, ()>,
    successful: usize,
    remaining: usize,
    min_agents: usize,
) -> Option<MagiError> {
    let decision =
        resolve_run_abort(reg, joined_before_abort, successful, remaining, min_agents).await;
    // The message names WHICH latch decided it. It used to say endpoint-down unconditionally,
    // which stopped being true the moment this started consulting both — so an abort caused by
    // a defect of ours was logged as an outage. That is this milestone's own thesis, reproduced
    // inside the abort path built to end it.
    match &decision {
        Some(MagiError::CrateDefect { .. }) => tracing::warn!(
            agent = agent.display_name(),
            cause = %err,
            "abnormal agent exit with a crate-defect latch set; aborting run"
        ),
        Some(_) => tracing::warn!(
            agent = agent.display_name(),
            cause = %err,
            "abnormal agent exit with endpoint-down latch set; aborting run"
        ),
        None => {}
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
    Vec<CompletionRecord>,
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
        digest_collisions: BTreeMap::new(),
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
    // Same shape and the same reason: the seat's completion records accumulate ACROSS
    // rotations, so the sequence reads as its full history, one entry per attempt.
    let mut records: Vec<CompletionRecord> = Vec::new();

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
            &mut records,
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
                return (
                    Ok(output),
                    state.to_rotation(),
                    was_retried,
                    failures,
                    records,
                );
            }
            ModelOutcome::Unexpected(detail) => {
                registry.release(agent_name).await;
                guard.mark_released();
                return (
                    Err(detail),
                    state.to_rotation(),
                    was_retried,
                    failures,
                    records,
                );
            }
            ModelOutcome::MageLocal { detail, kind } => {
                // Mage-local, exactly like `Schema`: this seat gives up on this lineage and the
                // other two keep it. Note what is NOT called here — `register_transport_failure`.
                state.failed_lineages.insert(current_lineage.clone());
                (kind, detail)
            }
            ModelOutcome::CrateDefect {
                observation,
                hypothesis,
            } => {
                // No rotation: our own bad request would reproduce on every seat. Surfaced as a
                // defect of THIS crate rather than dropped into `failed_agents`, where model
                // failures land every day and a bug of ours would be invisible in the noise.
                registry.release(agent_name).await;
                guard.mark_released();
                // LATCHED, so the join loop can abort the whole run. The registry deliberately
                // does not learn WHICH seats answered — only the orchestrator knows that, and
                // filling it here would write an empty vector meaning "not yet" into a field
                // that reads as "none".
                registry
                    .latch_crate_defect(CrateDefectRecord {
                        observation: observation.clone(),
                        hypothesis,
                        agent: agent_name,
                        model: current_provider.model().to_string(),
                    })
                    .await;
                // The seat's own error channel is a `String` (see `ModelOutcome::Unexpected`).
                // It is filled anyway rather than left blank: if the abort were ever bypassed,
                // a blank seat would be worse than a named one. `joined_before_abort` is empty HERE
                // because this task cannot know it — the abort path fills it from the map the
                // join loop already holds.
                return (
                    Err(MagiError::CrateDefect {
                        observation,
                        hypothesis,
                        agent: agent_name,
                        model: current_provider.model().to_string(),
                        joined_before_abort: Vec::new(),
                    }
                    .to_string()),
                    state.to_rotation(),
                    was_retried,
                    failures,
                    records,
                );
            }
            ModelOutcome::OversizedResponse { limit } => {
                // Mage-local, exactly like `Schema`: this seat will not retry this lineage, and
                // the other two keep it. It ALWAYS behaved this way; until `4.0.0` it could not
                // SAY so, because `RotationKind` was public and not `#[non_exhaustive]`, making a
                // new variant a SemVer break — so the precision rode in the `detail` text. The
                // major spends that break, and the text goes back to being just text.
                state.failed_lineages.insert(current_lineage.clone());
                (RotationKind::OversizedResponse, oversized_detail(limit))
            }
            ModelOutcome::ExternalFailure { detail, kind } => {
                // Mage-local for the same reason as `OversizedResponse`, and reported by its own
                // kind for the same reason: this crate cannot know whether a third-party
                // backend's failure says anything about the lineages the other seats are on, so
                // it never condemns run-wide — and now the telemetry says that in the type.
                state.failed_lineages.insert(current_lineage.clone());
                (
                    RotationKind::ExternalFailure,
                    external_failure_detail(kind, &detail),
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
                //
                // THIS IS THE SINGLE PRODUCTION CALLER, and a consistency test derives from
                // that: "a mage-local class does not enter `run_failed`" IS "its outcome is
                // not `Transport`" only while this stays true. A second caller silently
                // turns that derivation into a claim about nothing. Re-count before adding
                // one -- the count is the guard, it is not enforced mechanically, and it is
                // CRATE-WIDE, not file-scoped: `register_transport_failure` is `pub` on a
                // `pub(crate)` registry, so a second caller can legitimately land in
                // `rotation.rs`, which is where that registry lives.
                //   grep -rnE '\.register_transport_failure\(' src/
                //     10 hits today, and exactly ONE is production: this arm. The other nine
                //     sit below `#[cfg(test)]` in orchestrator.rs and rotation.rs.
                //   grep -nE '\bis_connection\(' src/orchestrator.rs
                //     2 above this file's test module: one definition, one use. THIS one stays
                //     file-scoped on purpose -- `is_connection` is a private `fn` here, so no
                //     other file can call it, and widening it would only add test noise.
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
                    records,
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

    /// The mage-local guarantee for external failures, checked at its source.
    ///
    /// `is_connection` is what decides whether a failure can enter the run-wide condemned set and
    /// feed the endpoint-down latch. Every declared shape must answer `false` — including
    /// `Network`, which is the tempting one: it looks like this crate's own connection failure,
    /// but it describes a backend this crate never contacted and knows nothing about.
    /// The other mage-local outcome, checked at the same source.
    ///
    /// A server that answered too much is not a server that is down. Routing it through the
    /// connection class would let one seat's CONTENT failure feed the run-wide endpoint-down latch
    /// and abort the whole run — which is why this branch, not the type, is what has to be pinned.
    #[test]
    fn an_oversized_response_is_never_connection_class() {
        assert!(!is_connection(&ProviderError::ResponseTooLarge {
            limit: 1 << 20
        }));
    }

    /// …and that it gets its OWN outcome rather than inheriting `Transport`'s run-wide scope.
    /// Both halves are needed: the first says it cannot reach the latch, this one says it does not
    /// take the lineage away from the other two seats either.
    #[test]
    fn an_oversized_response_routes_to_its_own_mage_local_outcome() {
        let outcome = provider_err_outcome(ProviderError::ResponseTooLarge { limit: 4096 });
        match outcome {
            ModelOutcome::OversizedResponse { limit } => assert_eq!(limit, 4096),
            ModelOutcome::Transport { .. } => {
                panic!(
                    "Transport is run-wide — that is the inheritance this variant exists to stop"
                )
            }
            _ => panic!("expected OversizedResponse"),
        }
    }

    #[test]
    fn a_mage_local_rotation_detail_says_what_happened_and_not_its_scope() {
        // Both of these used to report `Transport`, which everywhere else means the whole run was
        // condemned, so the scope had to be spelled out in prose for a reader of `rotations` to
        // tell a content failure that cost one seat from an outage that cost the run. Since
        // `4.0.0` each has its own kind, and the prose goes back to describing the event.
        let oversized = oversized_detail(4096);
        let external = external_failure_detail(ExternalErrorKind::Network, "backend refused");

        // The prefix these two used to carry is GONE, and its absence is asserted rather than
        // assumed. It existed as a stopgap while `RotationKind` was frozen: with a variant per
        // cause the scope is carried by the TYPE, and a copy of it in prose is information that
        // can only ever contradict the type it duplicates.
        //
        // Asserted against the functions the rotation loop itself calls, so this cannot pass
        // while the loop emits something else.
        for detail in [&oversized, &external] {
            assert!(
                !detail.starts_with("mage-local: "),
                "the type says the scope now; the text must not say it again: {detail}"
            );
        }
        // And they still say WHAT happened — deleting the prefix must not have deleted the
        // diagnosis with it.
        assert!(oversized.contains("4096"), "{oversized}");
        assert!(external.contains("backend refused"), "{external}");
        assert!(external.contains("Network"), "{external}");
    }

    #[test]
    fn every_external_shape_routes_to_its_own_mage_local_outcome() {
        // The mirror of the oversized-response test above, and its absence was an asymmetry: the
        // latch was pinned but the ROUTING was not, so a refactor could send an external failure
        // down the transport path and hand a third-party crate a run-wide consequence — the exact
        // inheritance the dedicated variant exists to prevent. Pinned per shape, because the
        // routing must not depend on which one arrived.
        for kind in [
            ExternalErrorKind::Network,
            ExternalErrorKind::Timeout,
            ExternalErrorKind::Auth,
            ExternalErrorKind::RateLimit,
            ExternalErrorKind::ServerError,
            ExternalErrorKind::Other,
        ] {
            let outcome = provider_err_outcome(ProviderError::external("backend refused", kind));
            match outcome {
                ModelOutcome::ExternalFailure { kind: got, .. } => assert_eq!(got, kind),
                ModelOutcome::Transport { .. } => panic!(
                    "Transport is run-wide and feeds the endpoint-down latch — {kind:?} must not \
                     reach it"
                ),
                _ => panic!("expected ExternalFailure for {kind:?}"),
            }
        }
    }

    #[test]
    fn no_external_shape_is_ever_connection_class() {
        for kind in [
            ExternalErrorKind::Network,
            ExternalErrorKind::Timeout,
            ExternalErrorKind::Auth,
            ExternalErrorKind::RateLimit,
            ExternalErrorKind::ServerError,
            ExternalErrorKind::Other,
        ] {
            assert!(
                !is_connection(&ProviderError::external("x", kind)),
                "{kind:?} must never license a run-wide consequence"
            );
        }
    }

    #[test]
    fn a_zero_threshold_warns_always_and_does_not_disable() {
        // No sentinel values: 0 is literally zero, so any non-empty input exceeds it. A value
        // that flips the meaning of a knob is a hidden rule.
        let cfg = MagiConfig {
            input_warn_tokens: 0,
            ..Default::default()
        };
        assert!(
            measure_input("some content", &cfg).exceeded,
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
        assert!(!measure_input("", &cfg).exceeded);
    }

    #[test]
    fn the_default_threshold_does_not_warn_on_ordinary_input() {
        let cfg = MagiConfig::default();
        assert!(!measure_input("a short review request", &cfg).exceeded);
    }

    #[test]
    fn the_reported_flag_always_agrees_with_the_reported_count() {
        // `exceeded` restates a relation between the other two fields, so the failure it invites
        // is a flag computed from a different measurement than the one shown. Asserted across the
        // boundary and both sides of it, since a disagreement would be invisible in the report:
        // the number and the flag would each look reasonable on their own.
        let cfg = MagiConfig {
            input_warn_tokens: 10,
            ..Default::default()
        };
        for len in [0, 40, 43, 44, 45, 400] {
            let size = measure_input(&"x".repeat(len), &cfg);
            assert_eq!(
                size.exceeded,
                size.estimated_tokens > size.warn_threshold,
                "a {len}-byte input reported {} tokens against a threshold of {}",
                size.estimated_tokens,
                size.warn_threshold
            );
        }
    }

    #[test]
    fn a_threshold_is_unreachable_when_the_smallest_warning_input_is_already_rejected() {
        // The three-byte band the earlier `threshold * 4 >= limit` form called reachable. With
        // T tokens, the smallest input that warns is 4 * (T + 1) bytes; any limit below that
        // leaves the knob mute. Both sides of the boundary are asserted, so a future
        // simplification that drops the `+ 1` fails here rather than going quiet.
        let smallest_warning_input = TOKENS_PER_BYTE_DIVISOR * (100 + 1);

        // The predicate INVERTS the estimator's formula, and nothing in the type system ties the
        // two together: change `estimate_tokens` and this arithmetic quietly becomes wrong while
        // still compiling. So the inversion is checked against the estimator itself, not assumed.
        // A different formula fails here, next to the code that depends on it.
        assert!(
            estimate_tokens(&"x".repeat(smallest_warning_input)) > 100,
            "the estimator must warn at the size the predicate calls the smallest warning input"
        );
        assert!(
            estimate_tokens(&"x".repeat(smallest_warning_input - 1)) <= 100,
            "and must not warn one byte below it, or the predicate is off by more than it thinks"
        );

        for (limit, unreachable) in [
            (smallest_warning_input - 1, true),
            (smallest_warning_input, false),
        ] {
            let cfg = MagiConfig {
                input_warn_tokens: 100,
                max_input_len: limit,
                ..Default::default()
            };
            assert_eq!(
                warn_threshold_is_unreachable(&cfg),
                unreachable,
                "limit {limit} against the smallest warning input {smallest_warning_input}"
            );
        }
    }

    #[test]
    fn a_threshold_that_can_never_fire_is_detected() {
        // Saturating is the point here: the widest threshold there is must not overflow into
        // the opposite verdict, which is what a plain multiply would do.
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
    use crate::provider::{FinishReason, ReasoningState};

    // ---- Task 3b: the arms are proved HERE, not seventeen tasks later ----

    /// `is_mage_local()` must AGREE with what the classifier actually does, for every error
    /// this crate can surface.
    ///
    /// # Why an accessor needs binding at all
    ///
    /// It is a second source of truth for scope: the classifier decides `MageLocal` versus
    /// `Transport` in one place, and this method answers the same question in another. Nothing
    /// made the two agree, so they could drift — and the whole point of `4.0.0`'s telemetry is
    /// that a consumer can trust the reported scope. A consumer branching on `is_mage_local()`
    /// while the run condemns run-wide would be told the opposite of what happened.
    ///
    /// This walks every variant rather than sampling, so a variant added later has to be added
    /// here to compile — which is the same forcing function the exhaustive match provides.
    #[test]
    fn the_scope_accessor_agrees_with_the_classifier_for_every_variant() {
        use crate::error::ResponseContractCause;
        use crate::provider::FinishReason;

        let cases: Vec<ProviderError> = vec![
            ProviderError::Timeout {
                message: "elapsed".into(),
            },
            ProviderError::Network {
                message: "refused".into(),
            },
            ProviderError::Http {
                status: 503,
                body: String::new(),
                retry_after_raw: Vec::new(),
                received_at: None,
            },
            ProviderError::ResponseTooLarge { limit: 1 },
            ProviderError::ResponseContract {
                reason: ResponseContractCause::Unreadable,
                detail: String::new(),
            },
            ProviderError::ResponseContract {
                reason: ResponseContractCause::NoMessage,
                detail: String::new(),
            },
            ProviderError::ResponseContract {
                reason: ResponseContractCause::RedirectRefused,
                detail: String::new(),
            },
            ProviderError::EmptyCompletion {
                telemetry: crate::provider::CompletionTelemetry::unmeasured()
                    .with_finish(FinishReason::Length),
                cap: 4096,
            },
            ProviderError::EmptyCompletion {
                telemetry: crate::provider::CompletionTelemetry::unmeasured(),
                cap: 4096,
            },
        ];

        for err in cases {
            let rendered = format!("{err:?}");
            match provider_err_outcome(err) {
                // The classifier says mage-local, so the accessor must too.
                ModelOutcome::MageLocal { kind, .. } => assert!(
                    kind.is_mage_local(),
                    "{rendered} is classified mage-local but its kind denies it"
                ),
                // And run-wide is the direction that costs the other two seats a lineage, so
                // an accessor claiming mage-local there is the more dangerous disagreement.
                ModelOutcome::Transport { kind, .. } => assert!(
                    !kind.is_mage_local(),
                    "{rendered} condemns run-wide but its kind claims to be mage-local"
                ),
                // These two carry their scope in the OUTCOME rather than in a `kind`: both are
                // mage-local, and `4.0.0` is what gave them their own `RotationKind` instead of
                // the `mage-local:` string prefix they wore since `3.1.0`. Pinned here so the
                // renaming cannot quietly put them back on a run-wide cause.
                ModelOutcome::OversizedResponse { .. } => assert!(
                    RotationKind::OversizedResponse.is_mage_local(),
                    "an oversized body is a content failure: the server answered perfectly"
                ),
                ModelOutcome::ExternalFailure { .. } => assert!(
                    RotationKind::ExternalFailure.is_mage_local(),
                    "this crate cannot know what a third-party backend's failure implies for                      the lineages the other seats are using"
                ),
                other => panic!("{rendered} produced no scoped outcome: {other:?}"),
            }
        }
    }

    #[test]
    fn each_contract_variant_gets_the_consequence_the_spec_assigned() {
        use crate::error::ResponseContractCause;

        // A unit test over the classifier — cheap, no registry, no async. It pins the decision at
        // the moment it is made instead of leaving the arms unexercised until the task that
        // observes the registry.
        //
        // WHY `MageLocal` AND NOT `Transport { connection: false }`, which is what a first
        // reading suggests: `connection` governs the endpoint-down LATCH, not the SCOPE of the
        // condemnation. `ModelOutcome::Transport` calls `register_transport_failure`, which
        // condemns the lineage RUN-WIDE for every seat regardless of `connection` — its own
        // rustdoc says so and that behaviour is deliberate for genuine transport faults. Routing
        // a content failure through it with `connection: false` would still take the lineage away
        // from the other two mages, which is precisely the defect this milestone exists to fix.
        let contract = ProviderError::ResponseContract {
            reason: ResponseContractCause::NoMessage,
            detail: String::new(),
        };
        match provider_err_outcome(contract) {
            ModelOutcome::MageLocal { kind, .. } => {
                assert_eq!(kind, RotationKind::ResponseContract);
                assert!(kind.is_mage_local());
            }
            other => panic!("family 1 is mage-local, got {other:?}"),
        }

        let empty = ProviderError::EmptyCompletion {
            telemetry: crate::provider::CompletionTelemetry::unmeasured()
                .with_finish(crate::provider::FinishReason::Length),
            cap: 4096,
        };
        match provider_err_outcome(empty) {
            ModelOutcome::MageLocal { kind, .. } => {
                assert_eq!(kind, RotationKind::EmptyCompletion);
                assert!(kind.is_mage_local());
            }
            other => panic!("family 2 is mage-local, got {other:?}"),
        }

        // The SIGNAL, not a MagiError: the classifier does not know which seats had already
        // joined, and filling that with an empty vector would read as "none" when it means
        // "not yet".
        let defect = ProviderError::NoGeneration {
            done_reason: Some(crate::provider::FinishReason::Load),
        };
        match provider_err_outcome(defect) {
            ModelOutcome::CrateDefect {
                observation,
                hypothesis,
            } => {
                assert!(observation.contains("counters absent"), "{observation}");
                assert!(hypothesis.contains("magi-core"), "{hypothesis}");
            }
            other => panic!("family 3 is a crate defect, got {other:?}"),
        }
    }

    #[test]
    fn no_contract_failure_is_connection_class() {
        use crate::error::ResponseContractCause;

        // In all three the endpoint ANSWERED. The endpoint-down latch exists for a backend that
        // cannot be reached; feeding it from a response that arrived would abort the run on a
        // healthy endpoint.
        for err in [
            ProviderError::ResponseContract {
                reason: ResponseContractCause::Unreadable,
                detail: String::new(),
            },
            ProviderError::EmptyCompletion {
                telemetry: crate::provider::CompletionTelemetry::unmeasured(),
                cap: 16_384,
            },
            ProviderError::NoGeneration { done_reason: None },
        ] {
            assert!(!is_connection(&err), "{err:?}");
        }
    }
    use super::*;
    use crate::prompts::lookup_prompt;
    use crate::provider::Completion;
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
        responses: Vec<Result<Completion, ProviderError>>,
        call_count: AtomicUsize,
    }

    impl MockProvider {
        fn success(name: &str, model: &str, responses: Vec<String>) -> Self {
            Self {
                name: name.to_string(),
                model: model.to_string(),
                responses: responses
                    .into_iter()
                    .map(|t| Ok(Completion::new(t)))
                    .collect(),
                call_count: AtomicUsize::new(0),
            }
        }

        fn mixed(name: &str, model: &str, responses: Vec<Result<String, ProviderError>>) -> Self {
            Self {
                name: name.to_string(),
                model: model.to_string(),
                responses: responses
                    .into_iter()
                    .map(|r| r.map(Completion::new))
                    .collect(),
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
        ) -> Result<Completion, ProviderError> {
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

    /// A one-shot warning is emitted once however many times it is asked for.
    ///
    /// R-18 existed because a third builder warning simply never got a flag: the two beside
    /// it were latched, it was not, and it repeated on every `analyze()`. What failed there
    /// was not the flag TYPE — it was that having one was up to whoever wrote the warning.
    /// One shared gate is what makes forgetting impossible on the render side.
    ///
    /// Two variants are exercised, not one, because a single-bit mistake is invisible with a
    /// single warning: a mask that used the same bit for everything would still emit each
    /// warning once here and would silently swallow every one after the first in a real run.
    #[tokio::test]
    async fn a_one_shot_warning_is_emitted_once_however_often_it_is_asked() {
        let log = EventLog::default();
        let _guard = tracing::subscriber::set_default(log.clone());
        let magi = Magi::new(trio());

        magi.warn_once(WarnOnce::InertGuard { candidates: 3 });
        magi.warn_once(WarnOnce::InertGuard { candidates: 3 });
        magi.warn_once(WarnOnce::InertGuard { candidates: 3 });

        let guard_lines = log
            .lines()
            .iter()
            .filter(|l| l.contains("strict_context_guard"))
            .count();
        assert_eq!(
            guard_lines, 1,
            "asked three times, said once: the condition it names cannot be fixed mid-run, so              a second telling describes a state that provably has not changed"
        );

        // A DIFFERENT warning still gets its turn. One bit per variant is the whole reason
        // the discriminants are explicit; sharing one would make this second line vanish.
        let probe: Arc<dyn ProviderProbe> = Arc::new(DeclaringProbe("declared-model"));
        let targets = vec![("served-model".to_string(), probe)];
        magi.warn_once(WarnOnce::ProbeDeclaration { targets: &targets });
        assert_eq!(
            log.lines()
                .iter()
                .filter(|l| l.contains("declared-model"))
                .count(),
            1,
            "a second warning must not be swallowed by the first one's bit"
        );
    }

    /// Every variant owns a distinct bit, and every bit fits the mask.
    ///
    /// This replaces the `COUNT - 1` discriminant assert the plan sketched, and it is
    /// strictly stronger: that one checks only the LAST variant, so inserting a variant in
    /// the middle with a duplicated bit passes it. Two variants sharing a bit swallow one
    /// warning for the rest of the process, which is R-18 with a different cause.
    #[test]
    fn every_warning_owns_a_distinct_bit_inside_the_mask() {
        let targets: Vec<(String, Arc<dyn ProviderProbe>)> = Vec::new();
        let all = [
            WarnOnce::InertGuard { candidates: 0 },
            WarnOnce::ProbeDeclaration { targets: &targets },
        ];
        assert_eq!(
            all.len() as u32,
            WarnOnce::COUNT,
            "COUNT is maintained by hand, so it is asserted against the list it counts"
        );
        let bits: std::collections::BTreeSet<u32> = all.iter().map(WarnOnce::bit).collect();
        assert_eq!(bits.len(), all.len(), "no two variants may share a bit");
        assert!(
            bits.iter().all(|b| *b < WarnOnce::COUNT),
            "a bit at or past COUNT is outside the mask this gate indexes"
        );
    }

    /// The provider `MagiBuilder::new` requires. It takes no part in the R-4 case.
    fn default_provider() -> Arc<dyn LlmProvider> {
        trio()
    }

    /// Primary **A**: both `LlmProvider` and `ProviderProbe`, which is what the generic on
    /// `with_probing_agent` demands. `MockProbe` already is both; no new double is written.
    fn provider_a_with_probe() -> Arc<crate::test_support::MockProbe> {
        crate::test_support::MockProbe::with_digest("model-a", &"a".repeat(64))
    }

    /// Primary **B**: replaces A and declares NO probe. Its `model()` is the key under
    /// which A's measurement would be filed if the stale probe survived.
    fn provider_b() -> Arc<dyn LlmProvider> {
        Arc::new(MockProvider::success(
            "b",
            "model-b",
            vec![mock_agent_json("melchior", "approve", 0.9)],
        ))
    }

    /// A replaced provider clears the probe it no longer matches.
    ///
    /// `with_agent` already removes it — "a plain primary declares no probe" — and
    /// `with_provider` did not, so the seat kept a probe pointing at a model it no longer
    /// serves.
    #[test]
    fn a_replaced_provider_clears_the_probe_it_no_longer_matches() {
        let lineage = Lineage::from("lin-a");
        let builder = MagiBuilder::new(default_provider())
            .with_probing_agent(AgentName::Melchior, provider_a_with_probe(), lineage)
            .with_provider(AgentName::Melchior, provider_b());

        assert!(
            !builder.primary_probes.contains_key(&AgentName::Melchior),
            "with_provider must clear a probe declared for the same seat"
        );

        // And the DAMAGE, which is the half that actually harms. Left behind, the entry
        // makes `collect_probe_targets` pair `agent_models[Melchior]` — the NEW model, B —
        // with the OLD probe, filing a measurement UNDER THE WRONG KEY. The window
        // pre-filter then admits or rejects using another model's number, and the digest
        // verify can turn a healthy candidate away over a collision that does not exist —
        // this subsystem's only fail-closed direction. Asserting the builder alone leaves
        // that consequence unpinned.
        let rotation = RotationConfig {
            primary_lineages: builder.agent_lineages.clone(),
            primary_probes: builder.primary_probes.clone(),
            strict_context_guard: builder.strict_context_guard,
            pool: FallbackPool::builder().build(),
        };
        let agent_models =
            BTreeMap::from([(AgentName::Melchior, provider_b().model().to_string())]);

        let targets = collect_probe_targets(&agent_models, &rotation);
        assert!(
            targets
                .iter()
                .all(|(model, _)| model != provider_b().model()),
            "no target may be keyed by B's model and measured by A's probe"
        );
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

    /// After R-1 the retry `warn!` is the ONLY carrier of the attempt count for a mage-local
    /// class: the error such a class returns is the original, which has no `AbandonReason` and
    /// no `attempts`. Nothing asserted that the line was emitted or that it named its three
    /// fields, so a refactor that tidied it -- or moved it behind a level check -- would leave
    /// both channels dead in a release whose stated purpose is to diagnose better.
    ///
    /// The same capture carries a second assertion that turns a declared hole into a mechanism:
    /// `abandon`'s `None` branch is unreachable by construction, and its reopening symptom is a
    /// log line. Asserting the line is ABSENT means a fourth exit that forgot its original error
    /// turns the suite red instead of waiting for an operator to notice. That is the cheap half
    /// of the hole, not its closure: a path no test walks is still unseen.
    #[tokio::test]
    async fn the_budget_abandon_warning_carries_the_attempt_count() {
        let log = EventLog::default();
        let _guard = tracing::subscriber::set_default(log.clone());

        // One failing response is enough: with a zero budget the loop makes its attempt and
        // abandons on the next check, which is the exit under test.
        // `mixed` CYCLES its responses, so a single failing entry is a provider that always
        // fails -- which is what a zero budget needs in order to reach its abandon exit.
        let inner: Arc<dyn LlmProvider> = Arc::new(MockProvider::mixed(
            "budget",
            "budget",
            vec![Err(ProviderError::Http {
                status: 503,
                body: String::new(),
                retry_after_raw: vec![],
                received_at: None,
            })],
        ));
        let cfg = crate::provider::RetryConfig {
            base_delay: std::time::Duration::from_millis(1),
            operation_budget: std::time::Duration::ZERO,
            max_retries: 5,
            ..Default::default()
        };
        let provider = crate::provider::RetryProvider::with_config(inner, cfg);
        let _ = provider
            .complete("sys", "user", &CompletionConfig::default())
            .await
            .expect_err("a zero budget cannot succeed");

        let lines = log.lines();
        let warn = lines
            .iter()
            .find(|l| l.starts_with("WARN") && l.contains("operation budget exhausted"))
            .unwrap_or_else(|| panic!("the abandon warning must be emitted: {lines:?}"));
        // The trailing `=` is load-bearing: the message itself reads "operation budget
        // exhausted", so a bare `contains("budget")` stays green after someone deletes the
        // FIELD. Matching `budget=` asserts the field and not the prose around it.
        //
        // What separates the line selected above from its sibling warning is one underscore
        // -- the sibling says "operation_budget exhausted". Nothing pins that character, and
        // it fails CLOSED: if the two ever collided, `find` would return the sibling, which
        // carries `elapsed=` and `budget=` but not `attempts=`, so this reddens rather than
        // passing quietly.
        for field in ["elapsed=", "budget=", "attempts="] {
            assert!(
                warn.contains(field),
                "the warning must still name `{field}`, which the returned error no longer carries for a mage-local class: {warn}"
            );
        }

        assert!(
            !lines
                .iter()
                .any(|l| l.contains("abandoning with no original error")),
            "the `None` branch of `abandon` is unreachable by construction; if a fourth exit ever calls it without the original error, this is where it surfaces: {lines:?}"
        );
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
        // Via the constant, not the literal: hard-coding 4 would let a change to the divisor
        // shift the estimate while this assertion kept agreeing with the old value.
        assert_eq!(
            size.estimated_tokens,
            "fn main() {}".len() / TOKENS_PER_BYTE_DIVISOR
        );
        assert!(
            !log.lines().iter().any(|l| l.contains("threshold")),
            "silence below the threshold: {:?}",
            log.lines()
        );
    }

    /// A probe that knows which model it speaks for. `test_support::MockProbe` leaves
    /// `declared_model` at its default `None`, which is the "makes no claim" case.
    struct DeclaringProbe(&'static str);

    #[async_trait::async_trait]
    impl ProviderProbe for DeclaringProbe {
        async fn window(&self) -> Result<Option<usize>, ProviderError> {
            Ok(Some(1_000))
        }
        async fn digest(&self) -> Result<Option<String>, ProviderError> {
            Ok(None)
        }
        fn declared_model(&self) -> Option<&str> {
            Some(self.0)
        }
    }

    #[test]
    fn a_probe_declaring_another_model_is_named_not_rejected() {
        let log = EventLog::default();
        let _guard = tracing::subscriber::set_default(log.clone());

        let targets: Vec<(String, Arc<dyn ProviderProbe>)> =
            vec![("m1".to_string(), Arc::new(DeclaringProbe("m2")))];
        warn_on_probe_disagreement(&targets);

        let lines = log.lines();
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("WARN") && l.contains("probe_declares")),
            "the disagreement must be named: {lines:?}"
        );
    }

    #[test]
    fn a_probe_declaring_its_own_model_is_quiet() {
        let log = EventLog::default();
        let _guard = tracing::subscriber::set_default(log.clone());

        let targets: Vec<(String, Arc<dyn ProviderProbe>)> =
            vec![("m1".to_string(), Arc::new(DeclaringProbe("m1")))];
        warn_on_probe_disagreement(&targets);

        assert!(
            log.lines().is_empty(),
            "agreement is the normal case and must stay silent: {:?}",
            log.lines()
        );
    }

    #[test]
    fn a_probe_that_claims_nothing_is_checked_against_nothing() {
        let log = EventLog::default();
        let _guard = tracing::subscriber::set_default(log.clone());

        // The default `declared_model()` is `None` — an implementation that makes no claim
        // must not be treated as claiming the wrong thing, or every pre-existing probe
        // would start warning.
        let targets: Vec<(String, Arc<dyn ProviderProbe>)> = vec![(
            "m1".to_string(),
            crate::test_support::MockProbe::with_window("m9", Some(10)),
        )];
        warn_on_probe_disagreement(&targets);

        assert!(log.lines().is_empty(), "{:?}", log.lines());
    }

    #[test]
    fn two_probes_for_one_model_are_named() {
        let log = EventLog::default();
        let _guard = tracing::subscriber::set_default(log.clone());

        let targets: Vec<(String, Arc<dyn ProviderProbe>)> = vec![
            ("m1".to_string(), Arc::new(DeclaringProbe("m1"))),
            ("m1".to_string(), Arc::new(DeclaringProbe("m1"))),
        ];
        warn_on_probe_disagreement(&targets);

        let lines = log.lines();
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("WARN") && l.contains("answers last wins")),
            "last-writer-wins must be named where it is created: {lines:?}"
        );
    }

    /// The warning is only worth anything if `analyze` actually reaches it with the run's
    /// real guard setting. Proving the predicate in isolation leaves the wiring untested —
    /// delete the emission and a predicate-only suite stays green.
    #[tokio::test]
    async fn a_strict_guard_with_nothing_measured_warns_and_still_completes() {
        let log = EventLog::default();
        let _guard = tracing::subscriber::set_default(log.clone());

        let pool = FallbackPool::builder()
            .push(
                Arc::new(MockProvider::success("cand", "cand-model", vec![])),
                Lineage::new("vendor"),
            )
            .build();
        let magi = MagiBuilder::new(trio())
            .with_fallback_pool(pool)
            .with_strict_context_guard(true)
            .build()
            .expect("builds");
        let report = magi
            .analyze(&Mode::CodeReview, "fn main() {}")
            .await
            .expect("warn-only: naming the condition must not abort the run");

        assert!(
            !report.agents.is_empty(),
            "the filter is untouched — this reports, it does not decide"
        );
        let lines = log.lines();
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("WARN") && l.contains("strict_context_guard")),
            "the run must announce that the guard leaves the pool inert: {lines:?}"
        );
    }

    /// The other side, and the one that keeps the channel usable: a pool with a measured
    /// candidate is a healthy configuration, and a warning that fires on it gets silenced.
    #[tokio::test]
    async fn a_measured_candidate_keeps_the_strict_guard_quiet() {
        let log = EventLog::default();
        let _guard = tracing::subscriber::set_default(log.clone());

        let pool = FallbackPool::builder()
            .push_probing(
                crate::test_support::MockProbe::with_window("cand-model", Some(200_000)),
                Lineage::new("vendor"),
            )
            .build();
        let magi = MagiBuilder::new(trio())
            .with_fallback_pool(pool)
            .with_strict_context_guard(true)
            .build()
            .expect("builds");
        let report = magi
            .analyze(&Mode::CodeReview, "fn main() {}")
            .await
            .expect("analyze");

        assert!(!report.agents.is_empty());
        let lines = log.lines();
        assert!(
            !lines.iter().any(|l| l.contains("strict_context_guard")),
            "nothing is inert here, so nothing should be announced: {lines:?}"
        );
    }

    /// The probe-declaration warnings are latched exactly like the inert-guard one. What
    /// they report is decided when the builder runs, so a second telling would describe a
    /// state that provably has not changed — the strongest case for latching in this file.
    #[tokio::test]
    async fn probe_declaration_warnings_are_told_once_per_instance() {
        let log = EventLog::default();
        let _guard = tracing::subscriber::set_default(log.clone());

        let pool = FallbackPool::builder()
            .push_with_probe(
                Arc::new(MockProvider::success("cand", "cand-model", vec![])),
                Lineage::new("vendor"),
                Arc::new(DeclaringProbe("a-different-model")),
            )
            .build();
        let magi = MagiBuilder::new(trio())
            .with_fallback_pool(pool)
            .build()
            .expect("builds");

        let _ = magi.analyze(&Mode::CodeReview, "fn main() {}").await;
        let first = log
            .lines()
            .iter()
            .filter(|l| l.contains("probe_declares"))
            .count();
        let _ = magi.analyze(&Mode::CodeReview, "fn main() {}").await;
        let second = log
            .lines()
            .iter()
            .filter(|l| l.contains("probe_declares"))
            .count();

        assert_eq!(first, 1, "the disagreement must be named once");
        assert_eq!(second, 1, "and not repeated on the next call");
    }

    /// A long-lived orchestrator must not repeat a configuration complaint on every call.
    /// The condition cannot be fixed mid-run, so the second telling carries no information
    /// and costs the channel its credibility.
    #[tokio::test]
    async fn the_inert_guard_warning_is_told_once_per_instance() {
        let log = EventLog::default();
        let _guard = tracing::subscriber::set_default(log.clone());

        let pool = FallbackPool::builder()
            .push(
                Arc::new(MockProvider::success("cand", "cand-model", vec![])),
                Lineage::new("vendor"),
            )
            .build();
        let magi = MagiBuilder::new(trio())
            .with_fallback_pool(pool)
            .with_strict_context_guard(true)
            .build()
            .expect("builds");

        let _ = magi.analyze(&Mode::CodeReview, "fn main() {}").await;
        let after_first = log
            .lines()
            .iter()
            .filter(|l| l.contains("strict_context_guard"))
            .count();
        let _ = magi.analyze(&Mode::CodeReview, "fn main() {}").await;
        let after_second = log
            .lines()
            .iter()
            .filter(|l| l.contains("strict_context_guard"))
            .count();

        assert_eq!(after_first, 1, "the first run must say it");
        assert_eq!(after_second, 1, "the second run must not say it again");
    }

    /// Rotation switched off by configuration is not the reported foot-gun. The pool never
    /// reaches the window filter, so blaming the missing measurements would be a wrong
    /// diagnosis attached to a state the consumer chose.
    #[tokio::test]
    async fn rotation_disabled_by_configuration_keeps_the_strict_guard_quiet() {
        let log = EventLog::default();
        let _guard = tracing::subscriber::set_default(log.clone());

        let pool = FallbackPool::builder()
            .push(
                Arc::new(MockProvider::success("cand", "cand-model", vec![])),
                Lineage::new("vendor"),
            )
            .max_rotations(0)
            .build();
        let magi = MagiBuilder::new(trio())
            .with_fallback_pool(pool)
            .with_strict_context_guard(true)
            .build()
            .expect("builds");
        let report = magi
            .analyze(&Mode::CodeReview, "fn main() {}")
            .await
            .expect("analyze");

        assert!(!report.agents.is_empty());
        let lines = log.lines();
        assert!(
            !lines.iter().any(|l| l.contains("strict_context_guard")),
            "nothing rotates here because rotation is off, not because of the guard: {lines:?}"
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

        // The eligibility snapshot REACHES the report, seeded for every seat.
        //
        // This `Magi` has no rotation config, so it pins the NO-ROTATION seeder and
        // nothing else — an earlier form of this comment claimed it covered "either
        // producer", which was false and would have told the next reader the wiring
        // was pinned when half of it was not. The rotating producer, which is the only
        // one that can emit a candidate row, is pinned by
        // `rotation_integration::test_rotates_on_transport_to_next_lineage`.
        assert_eq!(
            report.pool_eligibility.len(),
            3,
            "every seat is covered, including the ones that never rotated: {:?}",
            report.pool_eligibility
        );
        assert!(
            report.pool_eligibility.values().all(|rows| rows.is_empty()),
            "with no pool declared there is no candidate to rule out"
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

        // Nothing joined and nothing pending: the quorum is unreachable, so the latch
        // alone decides. What this pins is the RECOVERY of the signal, not the criterion.
        let decision = resolve_abnormal_exit(
            AgentName::Caspar,
            &join_err,
            &reg,
            &BTreeMap::new(),
            0,
            0,
            ConsensusConfig::default().min_agents,
        )
        .await;
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
        // 300 -> 660: the ceiling now covers the worst case of one retry chain rather than one
        // attempt. `the_agent_ceiling_covers_the_worst_case_of_the_chain` carries the reasoning;
        // this line moved with the value it pins.
        assert_eq!(config.timeout, Duration::from_secs(660));
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
            ) -> Result<Completion, ProviderError> {
                self.counter.fetch_add(1, OrderingV05::SeqCst);
                Ok(Completion::new(String::new()))
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

        let (result, retried, _failures, _records, _defect) = dispatch_one_agent(
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

        let (result, retried, _failures, _records, _defect) = dispatch_one_agent(
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

        let (result, retried, _failures, _records, _defect) = dispatch_one_agent(
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

        let (result, retried, _failures, _records, _defect) = dispatch_one_agent(
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
        let (result, retried, _failures, _records, _defect) = dispatch_one_agent(
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
        let (result, retried, _failures, _records, _defect) = dispatch_one_agent(
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
        let (result, retried, _failures, _records, _defect) = dispatch_one_agent(
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
        let (result, retried, _failures, _records, _defect) = dispatch_one_agent(
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
        let (result, retried, _failures, _records, _defect) = dispatch_one_agent(
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

        let (result, retried, _failures, _records, _defect) = dispatch_one_agent(
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
        let (result, retried, _failures, _records, _defect) = dispatch_one_agent(
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
        ) -> Result<Completion, ProviderError> {
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
            Ok(Completion::new(mock_agent_json(agent_str, "approve", 0.9)))
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
            ) -> Result<Completion, ProviderError> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                Ok(Completion::new(String::new()))
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

    #[tokio::test]
    async fn test_a_probe_measuring_another_model_is_filed_under_the_provider_model() {
        // Pins the contract the decoupled constructors document, in the only way that
        // matters: by showing what the crate does when that contract is BROKEN.
        //
        // The capability key comes from the COMPLETIONS provider and the value comes from
        // the probe. While one object played both roles those could not disagree; declared
        // separately they can, and this test fixes the consequence — the disagreement is
        // STORED, not detected. That is why the agreement is the caller's responsibility
        // and why both constructors say so. A future change that started rejecting or
        // renaming on mismatch would break here, which is the point.
        let provider: Arc<dyn LlmProvider> =
            crate::test_support::MockProbe::with_window("m1", Some(1));
        let probe: Arc<dyn ProviderProbe> =
            crate::test_support::MockProbe::with_digest("m2", "sha256:measured-elsewhere");
        let pool = FallbackPool::builder()
            .push_with_probe(provider, Lineage::new("vendor"), probe)
            .build();
        let rotation = RotationConfig {
            primary_lineages: BTreeMap::new(),
            primary_probes: BTreeMap::new(),
            pool,
            strict_context_guard: false,
        };

        let targets = collect_probe_targets(&BTreeMap::new(), &rotation);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].0, "m1"); // the key is the provider's model, never the probe's

        let caps = run_preflight(targets).await;
        let entry = caps.get("m1").expect("filed under the provider's model");
        // ...while the measurement came from a probe that speaks for a different model.
        assert_eq!(entry.digest.as_deref(), Some("sha256:measured-elsewhere"));
        assert!(
            !caps.contains_key("m2"),
            "nothing is filed under the model the probe actually measures"
        );
    }

    #[test]
    fn test_with_agent_and_probe_declares_a_probe_for_a_non_probing_primary() {
        let builder = MagiBuilder::new(Arc::new(RoutingMockProvider::new())).with_agent_and_probe(
            AgentName::Melchior,
            Arc::new(RoutingMockProvider::new()),
            Lineage::new("alibaba"),
            crate::test_support::MockProbe::with_window("m1", Some(200_000)),
        );
        assert!(builder.primary_probes.contains_key(&AgentName::Melchior)); // probe must be stored, not dropped
        assert_eq!(
            builder.agent_lineages.get(&AgentName::Melchior),
            Some(&Lineage::new("alibaba"))
        );
        assert!(builder.agent_providers.contains_key(&AgentName::Melchior));
    }

    #[test]
    fn test_reregistering_as_plain_primary_drops_a_previously_declared_probe() {
        let builder = MagiBuilder::new(Arc::new(RoutingMockProvider::new())).with_agent_and_probe(
            AgentName::Melchior,
            Arc::new(RoutingMockProvider::new()),
            Lineage::new("alibaba"),
            crate::test_support::MockProbe::with_window("m1", Some(200_000)),
        );
        assert!(builder.primary_probes.contains_key(&AgentName::Melchior)); // must be stored first
        let builder = builder.with_agent(
            AgentName::Melchior,
            Arc::new(RoutingMockProvider::new()),
            Lineage::new("alibaba"),
        );
        assert!(!builder.primary_probes.contains_key(&AgentName::Melchior)); // with_agent clears the probe
    }

    #[tokio::test]
    async fn test_either_registration_door_produces_equal_builder_state() {
        let via_generic = MagiBuilder::new(Arc::new(RoutingMockProvider::new()))
            .with_probing_agent(
                AgentName::Caspar,
                crate::test_support::MockProbe::with_window("m9", Some(100)),
                Lineage::new("deepseek"),
            );

        let probe_obj = crate::test_support::MockProbe::with_window("m9", Some(100));
        let llm: Arc<dyn LlmProvider> = probe_obj.clone();
        let probe: Arc<dyn ProviderProbe> = probe_obj;
        let via_erased = MagiBuilder::new(Arc::new(RoutingMockProvider::new()))
            .with_agent_and_probe(AgentName::Caspar, llm, Lineage::new("deepseek"), probe);

        assert_eq!(
            via_generic.agent_lineages.get(&AgentName::Caspar),
            via_erased.agent_lineages.get(&AgentName::Caspar)
        );

        // Identity, not presence — and anchored to LITERALS, not to each other. Comparing
        // the two doors' probes against one another proves nothing: both were handed the
        // same value, so they are equal by construction whatever the code does. Asserting
        // the answer each door actually stored is what would catch a delegation that kept
        // the wrong probe.
        let generic = via_generic
            .primary_probes
            .get(&AgentName::Caspar)
            .expect("probe stored");
        let erased = via_erased
            .primary_probes
            .get(&AgentName::Caspar)
            .expect("probe stored");
        for probe in [generic, erased] {
            assert_eq!(probe.window().await.expect("mock never errors"), Some(100));
            assert_eq!(
                probe.digest().await.expect("mock never errors").as_deref(),
                Some("sha:m9")
            );
        }
    }
    // ---------------------------------------------------------------------
    // Task 13b — populating `completions`: success AND failure, one entry per
    // ATTEMPT. Recording only the cut ones is the very blindness this release
    // exists to end, so the clean run is the first test, not an afterthought.
    // ---------------------------------------------------------------------

    #[tokio::test]
    async fn a_run_with_no_cuts_still_records_one_entry_per_completion() {
        // BD-5: ALL of them. Without this the consumer is blind until the first
        // cut — and 4096 did not fail all at once, it had been scraping by.
        let magi = MagiBuilder::new(trio()).build().expect("builds");
        let report = magi
            .analyze(&Mode::CodeReview, "fn main() {}")
            .await
            .expect("a clean run");
        assert!(!report.degraded, "nothing went wrong in this run");
        assert_eq!(
            report.completions.values().map(Vec::len).sum::<usize>(),
            3,
            "one entry per completion, on a run where nothing was cut"
        );
        assert!(
            report.extraction_failures.values().all(Vec::is_empty),
            "a recorded completion is not an extraction failure"
        );
    }

    #[tokio::test]
    async fn the_recorded_model_and_cap_come_from_the_caller_not_the_response() {
        // Neither is readable off a response: a provider does not know which budget
        // it was handed nor which seat it served. Both come from the orchestrator,
        // and that is what makes the record actionable under rotation.
        let magi = MagiBuilder::new(trio()).build().expect("builds");
        let report = magi
            .analyze(&Mode::CodeReview, "fn main() {}")
            .await
            .expect("a clean run");
        let recs: Vec<_> = report.completions.values().flatten().collect();
        assert!(!recs.is_empty());
        assert!(recs.iter().all(|r| r.model == "test-model"));
        assert!(
            recs.iter()
                .all(|r| r.cap == CompletionConfig::default().max_tokens)
        );
    }

    #[tokio::test]
    async fn a_completion_that_failed_is_recorded_too() {
        // The most diagnostic attempt of all is the one that produced no verdict,
        // and it is exactly the one that is lost if recording hangs off the happy
        // path.
        let melchior = Arc::new(MockProvider::success(
            "mock",
            "test-model",
            vec![mock_agent_json("melchior", "approve", 0.9)],
        ));
        let balthasar = Arc::new(MockProvider::success(
            "mock",
            "test-model",
            vec![mock_agent_json("balthasar", "approve", 0.85)],
        ));
        let caspar = Arc::new(MockProvider::mixed(
            "mock",
            "cut-model",
            vec![Err(ProviderError::EmptyCompletion {
                telemetry: crate::provider::CompletionTelemetry::unmeasured()
                    .with_finish(FinishReason::Length),
                cap: 4096,
            })],
        ));
        let magi = MagiBuilder::new(melchior as Arc<dyn LlmProvider>)
            .with_agent(
                AgentName::Balthasar,
                balthasar as Arc<dyn LlmProvider>,
                Lineage::new("b"),
            )
            .with_agent(
                AgentName::Caspar,
                caspar as Arc<dyn LlmProvider>,
                Lineage::new("c"),
            )
            .build()
            .expect("builds");
        let report = magi
            .analyze(&Mode::CodeReview, "fn main() {}")
            .await
            .expect("two seats still answer");

        let recs = &report.completions[&AgentName::Caspar];
        assert_eq!(recs.len(), 1, "the failed attempt is still an attempt");
        assert_eq!(recs[0].model, "cut-model");
        // Only `EmptyCompletion` knows anything past the model and the cap, and
        // what it knows is the termination.
        assert_eq!(recs[0].finish, Some(FinishReason::Length));
        // Absence is declared, never filled with zeros that read as a measurement.
        assert_eq!(recs[0].completion_tokens, None);
        assert_eq!(recs[0].reasoning, ReasoningState::NotMeasured);
        // Disjoint sets: nothing failed EXTRACTION here, there was nothing to extract.
        assert!(
            report
                .extraction_failures
                .get(&AgentName::Caspar)
                .is_none_or(Vec::is_empty)
        );
    }

    #[tokio::test]
    async fn the_schema_retry_leaves_two_entries_with_the_same_model() {
        // `21-ter` in its workable form: two calls by the ORCHESTRATOR, both
        // visible, against the same model. It is what pins that the record is per
        // ATTEMPT and not per model. (A transport retry is invisible here by
        // construction: it happens inside `RetryProvider`, which hands back one
        // result.)
        let caspar = Arc::new(MockProvider::success(
            "mock",
            "retry-model",
            vec![
                "no markers at all".to_string(),
                mock_agent_json("caspar", "approve", 0.95),
            ],
        ));
        let magi = MagiBuilder::new(trio())
            .with_agent(
                AgentName::Caspar,
                caspar as Arc<dyn LlmProvider>,
                Lineage::new("c"),
            )
            .build()
            .expect("builds");
        let report = magi
            .analyze(&Mode::CodeReview, "fn main() {}")
            .await
            .expect("the corrective retry recovers the seat");

        let models: Vec<_> = report.completions[&AgentName::Caspar]
            .iter()
            .map(|r| r.model.as_str())
            .collect();
        assert_eq!(models.len(), 2, "two attempts, two records");
        assert_eq!(models[0], models[1], "same model, corrected prompt");
    }

    /// A defect of OURS must be reported even when the environment is failing at the same time.
    ///
    /// # Why this is not a preference between two true statements
    ///
    /// Both latches can be set at once: our bad request reaches one seat while two other
    /// lineages genuinely lose their connection. Whichever the run reports is the one an
    /// operator investigates — and reporting the outage sends them to inspect an environment
    /// that is not at fault. That is the exact misdirection this milestone exists to remove,
    /// recreated one level up from where it was found.
    ///
    /// The asymmetry decides it: an outage is environmental and persists, so losing it costs
    /// one run's diagnosis. A defect of ours that is masked hides in the noise of ordinary
    /// model failure, which is how it survives for releases.
    #[tokio::test]
    async fn a_crate_defect_outranks_a_simultaneous_endpoint_outage() {
        let mut init = BTreeMap::new();
        for (agent, lineage, model) in [
            (AgentName::Melchior, "alibaba", "q"),
            (AgentName::Balthasar, "moonshot", "k"),
            (AgentName::Caspar, "deepseek", "d"),
        ] {
            init.insert(
                agent,
                ActiveEntry {
                    lineage: Lineage::new(lineage),
                    model: model.into(),
                },
            );
        }
        let reg = LineageRegistry::new(init);

        // Two distinct lineages lose their connection: the endpoint-down latch is set.
        reg.register_transport_failure(Lineage::new("alibaba"), true)
            .await;
        reg.register_transport_failure(Lineage::new("moonshot"), true)
            .await;
        assert!(
            reg.endpoint_down_signalled().await,
            "the outage must really be latched, or this test proves nothing"
        );

        // And a third seat hits a defect of ours.
        reg.latch_crate_defect(CrateDefectRecord {
            observation: "no generation - token counters absent".to_string(),
            hypothesis: CRATE_DEFECT_HYPOTHESIS,
            agent: AgentName::Caspar,
            model: "d".to_string(),
        })
        .await;

        // The quorum is unreachable, so BOTH latches would abort on their own: what is
        // tested is the order, not the criterion.
        let err = resolve_run_abort(&reg, &BTreeMap::new(), 0, 0, 3)
            .await
            .expect("both latches are set, so the run must abort");

        assert!(
            matches!(err, MagiError::CrateDefect { .. }),
            "an outage must not mask a defect of ours: {err}"
        );
    }

    /// The quorum condition wraps the endpoint-down branch ONLY. Read as "wrap the whole
    /// of `resolve_run_abort`" it would let a latched defect of this crate ride on a
    /// reachable quorum, and no other test would notice.
    #[tokio::test]
    async fn a_crate_defect_aborts_even_when_the_quorum_is_reachable() {
        let mut init = BTreeMap::new();
        init.insert(
            AgentName::Caspar,
            ActiveEntry {
                lineage: Lineage::new("deepseek"),
                model: "d".into(),
            },
        );
        let reg = LineageRegistry::new(init);
        assert!(
            !reg.endpoint_down_signalled().await,
            "no outage in this scenario"
        );
        reg.latch_crate_defect(CrateDefectRecord {
            observation: "no generation - token counters absent".to_string(),
            hypothesis: CRATE_DEFECT_HYPOTHESIS,
            agent: AgentName::Caspar,
            model: "d".to_string(),
        })
        .await;

        // Two successes in hand and one seat pending against a quorum of three: reachable.
        let err = resolve_run_abort(&reg, &BTreeMap::new(), 2, 1, 3)
            .await
            .expect("a defect of ours aborts regardless of the quorum");
        assert!(matches!(err, MagiError::CrateDefect { .. }), "{err}");
    }

    #[test]
    fn a_crate_defect_records_nothing_because_its_report_will_not_exist() {
        // Recording an attempt whose run is invalidated would assert there was
        // something to measure. The guard is the CALLER's, so that the other ten
        // cases are not complicated by an `Option` return for the sake of one.
        let mut records = Vec::new();
        record_attempt(
            &mut records,
            "m",
            4096,
            &Ok(Err(ProviderError::NoGeneration {
                done_reason: Some(FinishReason::Load),
            })),
        );
        assert!(records.is_empty());
    }

    #[tokio::test]
    async fn a_timed_out_attempt_is_recorded_with_nothing_measured() {
        // An attempt that timed out is still an attempt. Nothing came back, so
        // nothing is claimed — but the model and the cap are known regardless,
        // because the caller set them.
        let mut records = Vec::new();
        let timed_out = tokio::time::timeout(Duration::from_millis(1), async {
            tokio::time::sleep(Duration::from_secs(3600)).await;
            Ok(Completion::new(String::new()))
        })
        .await;
        record_attempt(&mut records, "slow-model", 16_384, &timed_out);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].model, "slow-model");
        assert_eq!(records[0].cap, 16_384);
        assert_eq!(records[0].finish, None);
        assert_eq!(records[0].reasoning, ReasoningState::NotMeasured);
    }

    #[test]
    fn a_completion_record_is_built_in_exactly_one_place() {
        // A second construction site is how an attempt stops being recorded, or starts being
        // recorded differently, without anything failing.
        //
        // Stated as the invariant rather than as a count: a magic number would have to be
        // edited every time an arm is added, and editing it is how the check stops checking.
        // Splitting on the module opener, not on a bare `#[cfg(test)]` — this file has an
        // earlier one on a `#[cfg(test)]` accessor, and splitting there put production code in
        // the "test" half and made the assertion pass while guarding nothing.
        // Line endings NORMALISED first, and this is not defensive tidying: the repo checks out
        // with CRLF on Windows, so an LF-anchored search over `include_str!` finds nothing and
        // the test fails for a reason unrelated to what it guards. It passed only because these
        // files happened to have been rewritten with LF in place.
        // The invariant is CRATE-WIDE, so the file list is WALKED, never enumerated. A
        // hand-maintained allowlist is the mechanism this project has already removed twice:
        // it reports success over whatever nobody remembered to add, and a new module is
        // exactly what nobody remembers. All three reviewers named this independently.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files: Vec<std::path::PathBuf> = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("src/ must be readable") {
                let path = entry.expect("a readable entry").path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    files.push(path);
                }
            }
        }
        files.sort();
        assert!(
            files.len() >= 10,
            "the walk found {} files; it is not reaching src/",
            files.len()
        );

        for path in files {
            let name = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            let raw = std::fs::read_to_string(&path).expect("a readable source file");
            // Line endings normalised first: the repo checks out with CRLF on Windows, so an
            // LF-anchored search finds nothing and the test fails for an unrelated reason.
            let src = raw.replace("\r\n", "\n");
            // ANY test module, not just one called `tests` -- this crate has several, and
            // treating one of them as production is how the guard gained a false positive.
            let mut production = String::new();
            let mut in_test_mod = false;
            let mut depth = 0i32;
            let mut prev_is_cfg_test = false;
            for line in src.lines() {
                let t = line.trim();
                if !in_test_mod && prev_is_cfg_test && t.starts_with("mod ") && t.ends_with('{') {
                    in_test_mod = true;
                    depth = 1;
                    prev_is_cfg_test = false;
                    continue;
                }
                if in_test_mod {
                    depth += line.matches('{').count() as i32;
                    depth -= line.matches('}').count() as i32;
                    if depth <= 0 {
                        in_test_mod = false;
                    }
                    continue;
                }
                prev_is_cfg_test = t == "#[cfg(test)]";
                // A doc EXAMPLE is prose that happens to compile, and constructs on purpose.
                if !t.starts_with("///") {
                    production.push_str(line);
                    production.push('\n');
                }
            }

            // In this file the one legal site is carved out; everywhere else there is none.
            let outside = if name == "orchestrator.rs" {
                let start = production
                    .find("fn record_attempt(")
                    .expect("the single recording site must exist");
                let closer = concat!(
                    "
", "}", "
"
                );
                let end = production[start..]
                    .find(closer)
                    .map(|k| start + k)
                    .expect("the helper must be a complete function");
                assert!(
                    production[start..end].contains("CompletionRecord::from_telemetry"),
                    "the carve-out must contain the site, or it excludes nothing"
                );
                format!("{}{}", &production[..start], &production[end..])
            } else {
                production
            };

            assert!(
                !outside.contains("CompletionRecord::"),
                "only `record_attempt` may build a record; a path-form site in {name}"
            );
            // The STRUCT-LITERAL form too. `#[non_exhaustive]` blocks it from OTHER crates
            // only, so inside this one it compiles and the check above would not see it.
            // Line-based, because `struct X {` and `impl X {` carry the same three tokens and
            // are DECLARATIONS: matching the raw substring reported both, and the tempting
            // fix was to drop the check rather than to narrow it.
            let literal = concat!("CompletionRecord", " {");
            let smuggled = outside.lines().find(|l| {
                let t = l.trim_start();
                l.contains(literal)
                    && !t.starts_with("struct ")
                    && !t.starts_with("pub struct ")
                    && !t.starts_with("impl ")
            });
            assert!(
                smuggled.is_none(),
                "only `record_attempt` may build a record; a struct-literal site in {name}: {smuggled:?}"
            );
        }
    }

    /// Acceptance criterion 1, observed from the REGISTRY — which is what the criterion asks for
    /// and what its first attempt did not do.
    ///
    /// # The test this replaces was circular, and the mutation proved it
    ///
    /// It asserted over `report_run_failed`, a helper that derives the condemned set from the
    /// rotation KINDS. So `condemned.is_empty()` and `hop.kind() == EmptyCompletion` were the
    /// same fact stated twice, and nothing read the registry at all. Injecting the exact
    /// regression it existed to catch — a `register_transport_failure` call in the mage-local
    /// arm — left it **green**.
    ///
    /// This drives the rotating dispatcher directly and asks the registry, which is the only
    /// thing that can distinguish "the arm did not condemn run-wide" from "the telemetry says it
    /// did not".
    #[tokio::test]
    async fn the_mage_local_arm_never_condemns_a_lineage_run_wide() {
        let registry = Arc::new(LineageRegistry::new(
            [(
                AgentName::Caspar,
                ActiveEntry {
                    lineage: Lineage::new("deepseek"),
                    model: "deepseek".to_string(),
                },
            )]
            .into_iter()
            .collect(),
        ));

        // A seat whose provider returns an empty completion, and a fallback that answers.
        let primary = Arc::new(MockProvider::mixed(
            "mock",
            "deepseek",
            vec![Err(ProviderError::EmptyCompletion {
                telemetry: crate::provider::CompletionTelemetry::unmeasured()
                    .with_finish(FinishReason::Length),
                cap: 16_384,
            })],
        )) as Arc<dyn LlmProvider>;
        let fallback = Arc::new(MockProvider::success(
            "mock",
            "glm",
            vec![mock_agent_json("caspar", "approve", 0.95)],
        )) as Arc<dyn LlmProvider>;

        let agent = Agent::new(AgentName::Caspar, Arc::clone(&primary));
        let pool = FallbackPool::builder()
            .push(fallback, Lineage::new("zhipu"))
            .max_rotations(2)
            .build();

        let (result, rotation, _retried, _failures, _records) = dispatch_one_agent_rotating(
            agent,
            "MODE: code-review\n---BEGIN USER CONTEXT n---\nx\n---END USER CONTEXT n---"
                .to_string(),
            CompletionConfig::default(),
            Arc::new(Validator::new()),
            Duration::from_secs(30),
            true,
            Arc::clone(&registry),
            Arc::new(RotationConfig {
                primary_lineages: BTreeMap::new(),
                primary_probes: BTreeMap::new(),
                pool,
                strict_context_guard: false,
            }),
            Lineage::new("deepseek"),
            "deepseek".to_string(),
            Arc::new(BTreeMap::new()),
            false,
            0,
        )
        .await;

        assert!(result.is_ok(), "the seat rotated and recovered: {result:?}");

        // THE observation, and the one the derived helper could not make: the registry's
        // run-wide condemned set is untouched, so the lineage stays claimable by any other seat.
        let condemned = registry.run_failed_lineages().await;
        assert!(
            condemned.is_empty(),
            "an empty completion is mage-local: it must never enter the run-wide set: {condemned:?}"
        );

        // And the seat itself DID give up on that lineage — otherwise the assertion above would
        // be satisfied by an arm that simply never condemned anything at all.
        assert_eq!(rotation.chain.len(), 1);
        assert_eq!(rotation.chain[0].kind(), RotationKind::EmptyCompletion);
    }

    /// The cross-milestone crossing: what the classifier decided is what the
    /// eligibility snapshot reports.
    ///
    /// Each milestone passes its own gate and both touch `rotation.rs`, so their
    /// interaction is verified by neither. The crossing is concrete: the classifier
    /// decides whether a mage-local failure condemns a lineage run-wide, and the
    /// snapshot reads exactly that set to decide whether the other seats may use it.
    ///
    /// # Why both halves, and why the second one is not decoration
    ///
    /// The first half alone would be **vacuous**: the condemned set comes back
    /// empty, so no cause is emitted and the assertion holds whatever the snapshot
    /// does — including if it never emitted that cause at all. The second half
    /// feeds a populated set, which is the only path by which the variant can
    /// appear, since the normal pre-dispatch call passes an empty one.
    #[tokio::test]
    async fn a_mage_local_failure_does_not_condemn_the_lineage_for_the_other_seats() {
        let registry = Arc::new(LineageRegistry::new(
            [(
                AgentName::Caspar,
                ActiveEntry {
                    lineage: Lineage::new("deepseek"),
                    model: "deepseek".to_string(),
                },
            )]
            .into_iter()
            .collect(),
        ));
        let primary = Arc::new(MockProvider::mixed(
            "mock",
            "deepseek",
            vec![Err(ProviderError::EmptyCompletion {
                telemetry: crate::provider::CompletionTelemetry::unmeasured()
                    .with_finish(FinishReason::Length),
                cap: 16_384,
            })],
        )) as Arc<dyn LlmProvider>;
        let fallback = Arc::new(MockProvider::success(
            "mock",
            "glm",
            vec![mock_agent_json("caspar", "approve", 0.95)],
        )) as Arc<dyn LlmProvider>;
        let pool = FallbackPool::builder()
            .push(Arc::clone(&fallback), Lineage::new("zhipu"))
            .max_rotations(2)
            .build();
        let (result, _rotation, _retried, _failures, _records) = dispatch_one_agent_rotating(
            Agent::new(AgentName::Caspar, primary),
            "MODE: code-review
---BEGIN USER CONTEXT n---
x
---END USER CONTEXT n---"
                .to_string(),
            CompletionConfig::default(),
            Arc::new(Validator::new()),
            Duration::from_secs(30),
            true,
            Arc::clone(&registry),
            Arc::new(RotationConfig {
                primary_lineages: BTreeMap::new(),
                primary_probes: BTreeMap::new(),
                pool,
                strict_context_guard: false,
            }),
            Lineage::new("deepseek"),
            "deepseek".to_string(),
            Arc::new(BTreeMap::new()),
            false,
            0,
        )
        .await;
        assert!(result.is_ok(), "the seat rotated and recovered: {result:?}");

        // The classifier's decision, read from where it is made. Asserted directly as
        // well as fed onward: the crossing is only meaningful if this half is stated.
        let condemned = registry.run_failed_lineages().await;
        assert!(
            condemned.is_empty(),
            "a mage-local failure must not enter the run-wide set: {condemned:?}"
        );

        // The snapshot, fed that exact set. Melchior is a DIFFERENT seat, and the
        // candidate carries the lineage CASPAR gave up on — which is the only way this
        // assertion can fail. An earlier form used an unrelated lineage, so injecting
        // the regression it exists to catch left it green: `LineageCondemnedRunWide`
        // only fires when the condemned set holds the CANDIDATE's lineage.
        let seats = [(
            AgentName::Melchior,
            ActiveEntry {
                lineage: Lineage::new("alibaba"),
                model: "mm".to_string(),
            },
        )]
        .into_iter()
        .collect::<BTreeMap<_, _>>();
        let candidates = [crate::rotation::FallbackCandidate {
            provider: Arc::clone(&fallback),
            lineage: Lineage::new("deepseek"),
            probe: None,
        }];
        let snapshot = |condemned: &BTreeSet<Lineage>| {
            crate::rotation::pool_eligibility_snapshot(&crate::rotation::EligibilityInputs {
                seats: &seats,
                progress: &BTreeMap::new(),
                run_failed_lineages: condemned,
                capabilities: &BTreeMap::new(),
                candidates: &candidates,
                max_rotations: 2,
                min_window_tokens: 0,
                strict_context_guard: false,
            })[&AgentName::Melchior][0]
                .causes
                .clone()
        };
        assert!(
            snapshot(&condemned).is_empty(),
            "a lineage one seat failed locally stays eligible for the others: {:?}",
            snapshot(&condemned)
        );

        // THE POSITIVE HALF. Without it the assertion above passes even if the cause
        // were never emitted at all, because the set it read was empty.
        assert_eq!(
            snapshot(&BTreeSet::from([Lineage::new("deepseek")])),
            vec![crate::rotation::IneligibilityCause::LineageCondemnedRunWide],
            "and a run-wide condemnation IS reported, exactly and alone"
        );
    }

    // -----------------------------------------------------------------------
    // MS2 — shared retry-budget helpers (Task 1a)
    //
    // Written here, once, because five MS2 tasks use them. A helper created twice is how
    // two of them diverge.
    // -----------------------------------------------------------------------

    /// A `Magi` built with the three values the time-budget tasks vary, and nothing else.
    ///
    /// Touches no network: every seat is a `MockProvider`. `max_rotations` lives on the
    /// fallback POOL rather than on `MagiConfig`, which is why it is threaded through here
    /// instead of being set on the config alongside the other two.
    pub(super) fn build_with(timeout: Duration, max_rotations: u32, schema_retry: bool) -> Magi {
        let pool = FallbackPool::builder()
            .max_rotations(max_rotations)
            .push(
                crate::test_support::ScriptProvider::new(
                    "m-fallback",
                    vec![crate::test_support::Beh::Ok],
                ) as Arc<dyn LlmProvider>,
                Lineage::new("zhipu"),
            )
            .build();

        let mut builder = MagiBuilder::new(crate::test_support::ScriptProvider::new(
            "m-default",
            vec![crate::test_support::Beh::Ok],
        ) as Arc<dyn LlmProvider>)
        .with_timeout(timeout)
        .with_fallback_pool(pool);

        if !schema_retry {
            builder = builder.with_retry_disabled();
        }
        builder
            .build()
            .expect("build_with: the mock trio always builds")
    }

    /// A provider that never answers, so the agent ceiling is what ends the call.
    ///
    /// Sleeps rather than opening a socket: the property under test is the CEILING, and a real
    /// hanging server would add a second thing that can fail.
    struct HangingProvider;

    #[async_trait::async_trait]
    impl LlmProvider for HangingProvider {
        async fn complete(
            &self,
            _s: &str,
            _u: &str,
            _c: &CompletionConfig,
        ) -> Result<Completion, ProviderError> {
            // Far longer than any ceiling a test sets; the timeout cancels this future.
            tokio::time::sleep(Duration::from_secs(3600)).await;
            unreachable!("the agent ceiling must cut this call before it returns")
        }
        fn name(&self) -> &str {
            "hanging"
        }
        fn model(&self) -> &str {
            "m-hanging"
        }
    }

    /// Runs a trio whose Caspar hangs, with the agent ceiling set to `d`.
    ///
    /// Returns `Ok`: one hung seat DEGRADES the run, it does not abort it. Taking `d` is what
    /// lets a test observe the ceiling message without waiting the shipped default.
    pub(super) async fn run_against_a_hanging_backend_with_ceiling(
        d: Duration,
    ) -> Result<MagiReport, MagiError> {
        let magi = MagiBuilder::new(crate::test_support::ScriptProvider::new(
            "m-default",
            vec![crate::test_support::Beh::Ok],
        ) as Arc<dyn LlmProvider>)
        .with_timeout(d)
        .with_agent(
            AgentName::Caspar,
            Arc::new(HangingProvider) as Arc<dyn LlmProvider>,
            Lineage::new("deepseek"),
        )
        .build()
        .expect("the hanging trio always builds");
        magi.analyze(&Mode::CodeReview, "fn main() {}").await
    }

    /// `build_with` must honour all three values it takes.
    ///
    /// A builder helper that quietly ignored one of its arguments would make every test built on
    /// it assert against a configuration it did not ask for.
    #[test]
    fn build_with_honours_the_three_values_it_takes() {
        let m = build_with(Duration::from_secs(42), 3, false);
        assert_eq!(m.config.timeout, Duration::from_secs(42));
        assert!(
            !m.config.retry_on_schema_error,
            "schema_retry = false must disable the corrective retry"
        );
    }

    /// The hung seat DEGRADES the run rather than aborting it, and the ceiling is what ends it.
    ///
    /// Asserting `Ok` is the point: a hang that came back as `Err` would mean one unreachable
    /// seat had taken the whole run down with it.
    #[tokio::test]
    async fn a_hanging_seat_degrades_the_run_within_the_given_ceiling() {
        let started = std::time::Instant::now();
        let report = run_against_a_hanging_backend_with_ceiling(Duration::from_millis(200))
            .await
            .expect("one hung seat degrades the run, it does not abort it");
        assert!(
            report.degraded,
            "a seat lost to the ceiling must leave the run degraded"
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the ceiling passed in must be what ends the call, not the shipped default"
        );
    }

    /// The agent ceiling, pinned by its number for the same reason the other six are.
    #[test]
    fn the_agent_ceiling_covers_the_worst_case_of_the_chain() {
        assert_eq!(MagiConfig::default().timeout, Duration::from_secs(660));
    }

    /// The worst case is per SEAT and never multiplies by the trio.
    ///
    /// The crate does not know whether the backend parallelises or serialises the three mages;
    /// multiplying by three would assert a serialisation nobody measured.
    #[test]
    fn the_worst_case_is_per_seat_and_never_multiplies_by_the_trio() {
        // `max_rotations` is deliberately NOT 2. With 2 the model count is `1 + 2 = 3`, which is
        // also the number of mages — so the correct formula and one that multiplied by the trio
        // produce the identical number, and this test would pass against the very thing its name
        // forbids. With 1 and 4 the two disagree.
        let magi = build_with(Duration::from_secs(1800), 1, true);
        assert_eq!(magi.worst_case_per_seat(), Duration::from_secs(7_200)); // 1800 * 2 * 2
        let magi2 = build_with(Duration::from_secs(1800), 1, false);
        assert_eq!(magi2.worst_case_per_seat(), Duration::from_secs(3_600)); // 1800 * 1 * 2
        // A third point, so no single wrong constant fits all three.
        let magi3 = build_with(Duration::from_secs(600), 4, true);
        assert_eq!(magi3.worst_case_per_seat(), Duration::from_secs(6_000)); // 600 * 2 * 5
    }

    /// It never returns `Err`, however absurd the configuration.
    ///
    /// The moment it rejects something it is the cap this project decided not to have.
    #[test]
    fn it_never_returns_err_no_matter_how_absurd_the_configuration() {
        let magi = build_with(Duration::from_secs(86_400), 9, true);
        // Returns a `Duration`, not a `Result`: the type is the assertion.
        let d: Duration = magi.worst_case_per_seat();
        assert!(d > Duration::ZERO);
    }

    /// The timeout message names the CONFIGURED ceiling as the ceiling.
    ///
    /// The message must name the ceiling AS the ceiling. It cannot report a measurement: this
    /// path is only reached when our own timeout fires, so elapsed is always exactly the ceiling
    /// — printing both would be one number twice, shaped like a comparison.
    ///
    /// The helper uses a SHORT ceiling and the assertion is about that number, not about 660:
    /// the property is "the message publishes the configured ceiling", true for any value, and a
    /// test that waited 660 s is a test nobody runs.
    ///
    /// `Ok` is expected: one hung seat DEGRADES the run. All four timeout sites write into
    /// `failed_agents` and none returns `Err`.
    #[tokio::test]
    async fn the_timeout_message_carries_the_configured_ceiling() {
        let report = run_against_a_hanging_backend_with_ceiling(Duration::from_millis(200))
            .await
            .expect("one hung seat degrades the run; it does not abort it");
        let s = report
            .failed_agents
            .values()
            .next()
            .expect("the hung seat is recorded");
        assert!(
            s.contains("200ms"),
            "must publish the configured ceiling, whatever it is: {s}"
        );
        assert!(
            s.contains("ceiling"),
            "the elapsed time alone does not say whose cut it was: {s}"
        );
    }

    /// Every agent-timeout site uses the shared message.
    ///
    /// SCOPE: this is a STRUCTURAL check and is fragile to a refactor that changes the spelling.
    /// It does not replace the semantic test above; it catches the one thing that one cannot —
    /// a FIFTH site nobody covered. (There are four today, and the semantic test exercises the
    /// behaviour of one of them; what is unguarded is a new one arriving with its own `format!`.)
    ///
    /// It reads the file it lives in, so the test module is cut off before counting: otherwise
    /// its own literals are counted and it fails for a reason that is not its own.
    #[test]
    fn every_timeout_site_uses_the_shared_message() {
        let src = include_str!("orchestrator.rs");
        // Cut at the test MODULE, not at the first `#[cfg(test)]`. That first one sits on a
        // production helper, hundreds of lines before any timeout site, so cutting there left
        // zero sites — which the plausibility assertion below caught rather than letting `0 == 0`
        // report success.
        //
        // The marker must be FOUND. Falling back to the whole file would make this test count its
        // own literals, and — the direction that matters — it would disarm silently the day
        // someone renames the module.
        //
        // What the fallback does TODAY is fail (`sites = 5`, `uses = 7`), but that is an accident
        // of how many times this module happens to name the helper: it counted 5 == 5 and PASSED
        // until the phase test added two more mentions. A guard whose correctness depends on a
        // tie being broken elsewhere is not a guard, which is why the marker is required instead.
        let cut = src
            .find(
                "
mod tests {",
            )
            .expect("the test module marker must exist, or this test is counting its own literals");
        let prod = &src[..cut];
        // Only the AGENT timeouts — but counted so that an UNRECOGNISED one fails rather than
        // ties. The previous form matched one exact spelling, so the fifth site this guard exists
        // to catch is precisely the one it would miss: written as `timeout(self.config.timeout,`
        // or across two lines, it counted zero and `uses == sites` still held. A guard that
        // reports success while guarding nothing is this codebase's recurring defect.
        let all = prod.matches("tokio::time::timeout(").count();
        let sites = prod.matches("tokio::time::timeout(timeout,").count();
        assert_eq!(
            all, sites,
            "a `tokio::time::timeout(` this guard does not recognise was added: it counts by an              exact spelling, so an unrecognised one would be invisible to the check below rather              than failing it. Either use the `(timeout,` form or teach this test the new one"
        );
        // Without this the test is VACUOUS if the cut lands early — a `#[cfg(test)]` over any
        // production helper is enough — because `0 == 0` passes. A test reporting success having
        // looked at nothing is the green-by-omission this project keeps finding.
        assert!(
            sites >= 4,
            "the test-module cut left {sites} sites, which is not plausible"
        );
        let uses = prod
            .matches("agent_timeout_message(")
            .count()
            .saturating_sub(1); // the definition
        assert_eq!(
            uses, sites,
            "{sites} timeout sites but {uses} use the shared message: the ones missing report a cut without naming the configured ceiling"
        );
    }

    /// The two phases render differently, and each names the ceiling.
    ///
    /// Without this, swapping the two phase strings leaves the whole suite green while a
    /// first-call timeout reports the `retry-failed:` prefix — which this module documents as
    /// meaning the corrective retry was reached.
    #[test]
    fn the_two_timeout_phases_are_distinguishable_and_both_name_the_ceiling() {
        let d = Duration::from_secs(42);
        let first = agent_timeout_message(false, d);
        let retry = agent_timeout_message(true, d);
        assert_ne!(first, retry, "the two phases must be tellable apart");
        assert!(
            first.starts_with("timeout: agent timed out"),
            "the first call must NOT claim the corrective retry was reached: {first}"
        );
        assert!(
            retry.starts_with("retry-failed:"),
            "the corrective retry must say so: {retry}"
        );
        for m in [&first, &retry] {
            assert!(
                m.contains("42s"),
                "both must name the configured ceiling: {m}"
            );
        }
    }

    /// The endpoint-down latch must not abort a run that can still reach its quorum.
    ///
    /// # The scaffolding is derived from ONE type
    ///
    /// `SeatBehaviour` is the source and every helper below is a function that builds
    /// from it. Fixtures declare BEHAVIOUR, never wiring: what a seat answers first, what
    /// the pool candidate answers if it rotates, and whether it waits for other seats to
    /// have joined before it answers at all. The bridge (`analyze_with`) turns three of
    /// them into a `Magi` with rotation ENGAGED — without a fallback pool the run goes
    /// through the non-rotating dispatcher, whose join loop never consults the latch, so
    /// `MagiError::EndpointDown` would be unconstructible and half the tests here could
    /// not pass in Red or in Green.
    mod endpoint_down_quorum {
        use super::*;
        use crate::error::ResponseContractCause;
        use crate::test_support::RoutingMockProvider;
        use crate::verdict_markers::{VERDICT_CLOSE, VERDICT_OPEN};

        /// What a seat does when it is dispatched. It is NOT a provider: it is the
        /// recipe the fixture builds one from, plus the mark that makes order countable.
        struct SeatBehaviour {
            lineage: &'static str,
            /// The first response. `Ok` completes; the `Err`s choose WHICH class of
            /// failure, which is what decides whether it feeds the latch (`Network`) or
            /// not (`schema`).
            first: Outcome,
            /// What the POOL CANDIDATE answers when this seat rotates.
            ///
            /// **The pool consumes it, NOT the `SeatProvider`**, and the distinction is
            /// structural: on rotation, `dispatch_one_agent_rotating` replaces
            /// `current_provider` with the pool candidate's, so the second attempt
            /// **never returns** to the seat's provider.
            ///
            /// `None` = "does not rotate" in the sense of **rotates and fails the same
            /// way again**, never "it is not asked a second time": the bridge translates
            /// it to `first`.
            after_rotation: Option<Outcome>,
            /// OPTIONAL barrier: the seat does not produce its result until `n` seats
            /// have completed theirs. It is what orders the fixtures WITHOUT a clock.
            waits_for_joins: Option<usize>,
        }

        /// `Copy` because the bridge reads it TWICE without moving the `SeatBehaviour`:
        /// once for the seat and once to seed the pool candidate.
        #[derive(Clone, Copy)]
        enum Outcome {
            Ok,
            ConnectionFailure,
            SchemaFailure,
            Panic,
        }

        /// Counter of seats that COMPLETED their work, incremented by the seat itself.
        /// It lives entirely in the scaffolding: production does not gain even a hook.
        ///
        /// # Why it does NOT count loop joins
        ///
        /// Counting processed joins requires instrumenting the `analyze()` loop, that is,
        /// production surface added for a test to look at. It is not needed: the property
        /// `a_truly_dead_endpoint_still_aborts_fast` asserts is "I do not wait for the
        /// remaining seats", and that is observed by its consequence — the `AbortGuard`
        /// cancels them, so they never complete and never increment.
        ///
        /// # Why a `watch` and NOT an `AtomicUsize` with `Relaxed`
        ///
        /// `waits_for_joins` needs a happens-before between "N seats completed" and "this
        /// seat produces its failure". `Relaxed` orders nothing, and the test would end up
        /// depending on the scheduler. A `watch` really synchronises, with no spin.
        /// (The `Relaxed` of the `warn_once` gate is another matter and it stays: there
        /// the guarantee asked for is "at most once", not an ordering between events.)
        type JoinCounter = tokio::sync::watch::Sender<usize>;

        /// The bridge. It does NOT take providers: it takes `SeatBehaviour` values and
        /// builds them, so the fixtures declare BEHAVIOUR and not wiring.
        ///
        /// The timeout is mandatory and comes first because it is what makes a miswired
        /// barrier FAIL instead of hang: 10 s is two orders of magnitude above what these
        /// tests take and a fraction of any CI timeout.
        ///
        /// The three seats are POSITIONAL: `seats[0]` is Melchior, `seats[1]` Balthasar,
        /// `seats[2]` Caspar, and the join loop joins them in that same order — which is
        /// what makes the barriers deterministic.
        async fn analyze_with(
            seats: Vec<SeatBehaviour>,
            consensus: ConsensusConfig,
        ) -> (Result<MagiReport, MagiError>, JoinCounter) {
            // Any input works -- these tests measure the abort criterion, not the
            // analysis -- but it is declared so the scaffolding has no loose symbol.
            const INPUT: &str = "fn main() {}";

            let (tx, _rx) = tokio::sync::watch::channel(0usize);

            let mut builder = MagiBuilder::new(default_provider()).with_consensus_config(consensus);

            let names = [AgentName::Melchior, AgentName::Balthasar, AgentName::Caspar];

            // The candidate's provider is `RoutingMockProvider`, which already does
            // exactly this: it reads `CURRENT_AGENT_IDENTITY`, dispenses a per-agent
            // FIFO and fails closed if the identity is not in scope. `Agent::execute_with`
            // wraps the fallback call in that identity, so ONE shared provider serves any
            // mage that rotated.
            let mut rot = RoutingMockProvider::new();
            for (seat_name, seat) in names.into_iter().zip(seats.iter()) {
                // `after_rotation: None` translates to `first`: "does not rotate" means
                // *rotates and fails the same way again*, never *it is not asked*.
                let on_rotation = seat.after_rotation.unwrap_or(seat.first);
                // A panicking seat never rotates, so its candidate is never asked -- and
                // `into_routing_response(Panic)` would evaluate the panic HERE, in the
                // test's own task, before `analyze()` runs. It stays unregistered, and
                // `RoutingMockProvider` fails closed if that assumption ever breaks.
                if matches!(on_rotation, Outcome::Panic) {
                    continue;
                }
                rot = rot.with_agent_responses(
                    seat_name,
                    vec![on_rotation.into_routing_response(seat_name)],
                );
            }
            let rot: Arc<dyn LlmProvider> = Arc::new(rot);

            // THREE candidates, THREE distinct lineages, ONE shared provider.
            // DISTINCT because `claim_next` rejects a candidate whose lineage is already
            // active in another seat: with a single candidate, the second seat to rotate
            // would be rejected for diversity and would degrade -- and the premise of
            // `a_recovered_run_is_not_aborted` is that BOTH rotate. SHARED because the
            // per-seat response comes from the identity and not from the object.
            //
            // The three lineages are also distinct from the ones the fixtures declare,
            // which is the other half of the diversity condition.
            let pool = FallbackPool::builder()
                .push(Arc::clone(&rot), "rot-a".into())
                .push(Arc::clone(&rot), "rot-b".into())
                .push(Arc::clone(&rot), "rot-c".into())
                .build();
            builder = builder.with_fallback_pool(pool);

            for (seat_name, seat) in names.into_iter().zip(seats) {
                // One provider per seat. `waits_for_joins` decides whether it gets a
                // Receiver; a seat with `None` gets none and resolves as soon as it can.
                let rx = seat.waits_for_joins.map(|_| tx.subscribe());
                // Read BEFORE moving `seat`: `SeatProvider::new` takes it by value.
                let lineage = seat.lineage;
                let provider = SeatProvider::new(seat_name, seat, tx.clone(), rx);
                builder = builder.with_agent(seat_name, Arc::new(provider), lineage.into());
            }

            let magi = builder
                .build()
                .expect("the scaffolding builds no invalid config");
            let out = tokio::time::timeout(
                Duration::from_secs(10),
                magi.analyze(&Mode::CodeReview, INPUT),
            )
            .await
            .expect("analyze_with hung: a barrier is waiting on a join that never came");
            (out, tx)
        }

        /// `ConsensusConfig { min_agents: n, ..Default::default() }` — the knob every
        /// test here varies. It is a `ConsensusConfig` field, not a `MagiConfig` one, and
        /// reaches the builder through `with_consensus_config`.
        fn min_agents(n: usize) -> ConsensusConfig {
            ConsensusConfig {
                min_agents: n,
                ..Default::default()
            }
        }

        /// How many seats completed their work before the run returned. It is what makes
        /// "without waiting for the remaining seats" verifiable rather than prose.
        fn joined_before_abort(joins: &JoinCounter) -> usize {
            *joins.borrow()
        }

        /// The provider `analyze_with` builds per seat. It waits on the barrier if it
        /// has a `Receiver`, produces **always** `seat.first` for `name`, and increments
        /// the join counter **after** producing. It does NOT consume `after_rotation`
        /// and carries no call counter: on rotation the second attempt goes to the pool
        /// candidate and never comes back to this object.
        struct SeatProvider {
            seat: SeatBehaviour,
            /// WHICH seat it serves. **Explicit, not read from `CURRENT_AGENT_IDENTITY`**:
            /// the verdict it produces carries this name, and the agent-identity
            /// post-validation compares it against the seat it was dispatched to.
            name: AgentName,
            joins: JoinCounter,
            barrier: Option<tokio::sync::watch::Receiver<usize>>,
        }

        impl SeatProvider {
            fn new(
                name: AgentName,
                seat: SeatBehaviour,
                joins: JoinCounter,
                barrier: Option<tokio::sync::watch::Receiver<usize>>,
            ) -> Self {
                Self {
                    seat,
                    name,
                    joins,
                    barrier,
                }
            }
        }

        #[async_trait::async_trait]
        impl LlmProvider for SeatProvider {
            async fn complete(
                &self,
                _system: &str,
                _user: &str,
                _cfg: &CompletionConfig,
            ) -> Result<Completion, ProviderError> {
                // THREE steps, in this order, exercised by
                // `a_seat_provider_honours_its_three_rules`.
                //
                // 1. Wait on the barrier, if this seat has one: until the counter
                //    reaches the seat's `waits_for_joins`. A seat with `None` skips this
                //    entirely -- and has no `.await` at all, so it is ready on the first
                //    poll.
                //
                // 2. Produce the result with `self.seat.first.into_result(self.name)`.
                //    The agent is a PARAMETER because the verdict carries its name.
                //
                // 3. Increment the join counter with `send_modify(|n| *n += 1)` --
                //    **AFTER producing, never before**. Another seat's barrier must
                //    release when this join HAS happened; moving it earlier reintroduces
                //    the race the `watch` exists to remove. `send_modify` takes the
                //    channel's lock, so two seats completing at once lose no count.
                if let (Some(rx), Some(n)) = (&self.barrier, self.seat.waits_for_joins) {
                    let mut rx = rx.clone();
                    while *rx.borrow_and_update() < n {
                        rx.changed()
                            .await
                            .expect("the join counter outlives every seat");
                    }
                }
                let result = self.seat.first.into_result(self.name);
                self.joins.send_modify(|n| *n += 1);
                result
            }
            fn name(&self) -> &str {
                "seat"
            }
            fn model(&self) -> &str {
                "seat-model"
            }
        }

        /// The `Outcome -> ProviderError` mapping, which is where it is decided WHAT
        /// feeds the latch. This is the load-bearing half: `is_connection` returns `true`
        /// ONLY for `Network`, so a `ConnectionFailure` built as any other variant would
        /// leave the tests green with no latch on -- green by omission, over the very
        /// mechanism under test.
        impl Outcome {
            fn into_result(self, agent: AgentName) -> Result<Completion, ProviderError> {
                match self {
                    Outcome::Ok => Ok(completion_with_a_valid_verdict(agent)),
                    // `Network`, and no other: it is the only one that feeds the latch.
                    Outcome::ConnectionFailure => Err(ProviderError::Network {
                        message: "connection refused".to_string(),
                    }),
                    // Mage-local on purpose: the unreachable-quorum-without-latch case
                    // needs a quorum that cannot be reached with NO latch.
                    Outcome::SchemaFailure => Err(ProviderError::ResponseContract {
                        reason: ResponseContractCause::Unreadable,
                        detail: String::new(),
                    }),
                    // Does not return: it panics inside the task, which is what the
                    // panic-path test exercises.
                    Outcome::Panic => panic!("seat panicked on purpose"),
                }
            }

            /// The same thing, but as TEXT, which is what `RoutingMockProvider`
            /// dispenses. DERIVED from `into_result` instead of repeating the error
            /// mapping: two copies of that table is how one gets updated and the other
            /// does not.
            fn into_routing_response(self, agent: AgentName) -> Result<String, ProviderError> {
                self.into_result(agent).map(|c| c.text)
            }
        }

        /// The `Ok` payload. It must PARSE: a response that does not satisfy the verdict
        /// sentinel turns every `Ok` seat into a contract failure, and then no test here
        /// measures what it claims to.
        fn valid_verdict_text(agent: AgentName) -> String {
            // THE AGENT IS A PARAMETER, and this is not cosmetic: the agent-identity
            // post-validation rejects a verdict whose `agent` is not the dispatched seat.
            // It is built with `json!` so the `agent` is SERIALIZED from the enum
            // (`rename_all = "lowercase"`) instead of transcribing the spelling.
            let body = serde_json::json!({
                "agent": agent,
                "verdict": "approve",
                "confidence": 0.9,
                "summary": "s",
                "reasoning": "r",
                "findings": [],
                "recommendation": "go",
            })
            .to_string();
            // The markers come from `verdict_markers`, for the same reason. The `join`
            // leaves each of them ALONE on its own line, which is what the sentinel
            // requires.
            [VERDICT_OPEN, body.as_str(), VERDICT_CLOSE].join("\n")
        }

        fn completion_with_a_valid_verdict(agent: AgentName) -> Completion {
            // Telemetry reports `unmeasured`, which is correct: this measures nothing.
            Completion::new(valid_verdict_text(agent))
        }

        /// Two seats with a connection failure in DISTINCT LINEAGES that then rotate
        /// successfully, and a third that completes on its first try.
        fn blip_then_recover() -> Vec<SeatBehaviour> {
            vec![
                SeatBehaviour {
                    lineage: "l1",
                    first: Outcome::ConnectionFailure,
                    after_rotation: Some(Outcome::Ok),
                    waits_for_joins: None,
                },
                SeatBehaviour {
                    lineage: "l2",
                    first: Outcome::ConnectionFailure,
                    after_rotation: Some(Outcome::Ok),
                    waits_for_joins: None,
                },
                SeatBehaviour {
                    lineage: "l3",
                    first: Outcome::Ok,
                    after_rotation: None,
                    waits_for_joins: None,
                },
            ]
        }

        /// A truly dead endpoint -- seat 1 fails on connection, rotates, and fails on
        /// connection again, which is two condemned lineages and the latch on.
        /// `after_rotation: None` is what makes rotating useless too.
        ///
        /// SEATS 2 AND 3 NEVER PRODUCE ON THEIR OWN, and that is what makes "it does not
        /// wait for the remaining seats" observable. Measured, not assumed: with three
        /// unbarriered seats the current-thread scheduler polls every one of them to
        /// completion before the join loop gets its first turn, so the counter read 3
        /// with `EndpointDown` already returned at seat 1's join -- the count could not
        /// separate a fast abort from a slow one. Barriered on a join that never comes,
        /// they stand for connection attempts still in flight: the only way they resolve
        /// is being cancelled by the `AbortGuard`, so the counter reads 1 exactly when
        /// the loop did not wait for them, and an implementation that waits hangs into
        /// the 10 s guard instead of passing.
        fn dead_endpoint() -> Vec<SeatBehaviour> {
            vec![
                SeatBehaviour {
                    lineage: "l1",
                    first: Outcome::ConnectionFailure,
                    after_rotation: None,
                    waits_for_joins: None,
                },
                SeatBehaviour {
                    lineage: "l2",
                    first: Outcome::ConnectionFailure,
                    after_rotation: None,
                    waits_for_joins: Some(usize::MAX),
                },
                SeatBehaviour {
                    lineage: "l3",
                    first: Outcome::ConnectionFailure,
                    after_rotation: None,
                    waits_for_joins: Some(usize::MAX),
                },
            ]
        }

        /// An unreachable quorum WITHOUT the latch. `SchemaFailure` is mage-local, so by
        /// construction it cannot turn the endpoint-down latch on -- which is the whole
        /// precondition of the scenario.
        fn two_schema_failures() -> Vec<SeatBehaviour> {
            vec![
                SeatBehaviour {
                    lineage: "l1",
                    first: Outcome::SchemaFailure,
                    after_rotation: None,
                    waits_for_joins: None,
                },
                SeatBehaviour {
                    lineage: "l2",
                    first: Outcome::SchemaFailure,
                    after_rotation: None,
                    waits_for_joins: None,
                },
                SeatBehaviour {
                    lineage: "l3",
                    first: Outcome::Ok,
                    after_rotation: None,
                    waits_for_joins: None,
                },
            ]
        }

        /// A seat panics with the latch on and the quorum still REACHABLE.
        ///
        /// The panicking seat goes THIRD and without a barrier, and both are deliberate:
        /// the join loop joins in dispatch order, so by the time its panic is processed
        /// the other two have already joined successfully -- `successful = 2`,
        /// `remaining = 0`, and with `min_agents(2)` the criterion gives `2 + 0 >= 2`,
        /// that is, it does NOT abort. A barrier here would be worse than useless: the
        /// panicking seat cannot wait on joins if it goes first, because no join can
        /// happen until it resolves.
        fn one_panicking_seat_with_reachable_quorum() -> Vec<SeatBehaviour> {
            vec![
                SeatBehaviour {
                    lineage: "l1",
                    first: Outcome::ConnectionFailure,
                    after_rotation: Some(Outcome::Ok),
                    waits_for_joins: None,
                },
                SeatBehaviour {
                    lineage: "l2",
                    first: Outcome::ConnectionFailure,
                    after_rotation: Some(Outcome::Ok),
                    waits_for_joins: None,
                },
                SeatBehaviour {
                    lineage: "l3",
                    first: Outcome::Panic,
                    after_rotation: None,
                    waits_for_joins: None,
                },
            ]
        }

        /// Leaves the EXACT state the criterion crosses -- `successful = 1`,
        /// `remaining = 1`, latch on -- at the check that follows seat 2's join.
        ///
        /// THE BARRIER GOES ON SEAT 2, NOT ON SEAT 3, and it is the only thing that makes
        /// this deterministic. The latch needs TWO registered connection failures; seat 3
        /// is the second one. With seat 2 waiting for the counter to reach 2 -- seat 1
        /// produces (1), seat 3 produces (2) -- seat 2 only then produces, so by the time
        /// the loop joins it the latch is ALREADY on, and seat 3 is STILL UNJOINED
        /// because the loop joins in dispatch order.
        fn one_ok_one_err_one_pending() -> Vec<SeatBehaviour> {
            vec![
                SeatBehaviour {
                    lineage: "l1",
                    first: Outcome::Ok,
                    after_rotation: None,
                    waits_for_joins: None,
                },
                SeatBehaviour {
                    lineage: "l2",
                    first: Outcome::ConnectionFailure,
                    after_rotation: None,
                    waits_for_joins: Some(2),
                },
                // IT ROTATES SUCCESSFULLY, and the test closing in BOTH of its directions
                // depends on that. Seat 3 has to fail on connection -- it is the second
                // lineage that turns the latch on -- but if it also stayed failed, with
                // `min_agents(2)` the run would abort at ITS join (`successful=1`,
                // `remaining=0`, `1 < 2`) and the assertion "1+1 >= 2 -> keep going"
                // would be false because of an abort later than the one the scenario
                // names. By rotating, `successful` reaches 2 and the criterion holds.
                SeatBehaviour {
                    lineage: "l3",
                    first: Outcome::ConnectionFailure,
                    after_rotation: Some(Outcome::Ok),
                    waits_for_joins: None,
                },
            ]
        }

        /// The latch turns on AFTER the loop started. Seat 1 completes clean and only
        /// then do the two connection failures land, each waiting for that join to have
        /// happened. A latch read cached before the loop would never see it.
        fn latch_after_first_join() -> Vec<SeatBehaviour> {
            vec![
                SeatBehaviour {
                    lineage: "l1",
                    first: Outcome::Ok,
                    after_rotation: None,
                    waits_for_joins: None,
                },
                SeatBehaviour {
                    lineage: "l2",
                    first: Outcome::ConnectionFailure,
                    after_rotation: None,
                    waits_for_joins: Some(1),
                },
                SeatBehaviour {
                    lineage: "l3",
                    first: Outcome::ConnectionFailure,
                    after_rotation: None,
                    waits_for_joins: Some(1),
                },
            ]
        }

        #[tokio::test]
        async fn a_recovered_run_is_not_aborted() {
            // Two connection failures in distinct lineages, both seats rotate
            // successfully, the third completes first try. Before the fix: seat 1 joins
            // with Ok, resolve_run_abort finds the latch true, returns EndpointDown --
            // and the AbortGuard cancels seats 2 and 3, which were about to succeed.
            let (res, _joins) = analyze_with(blip_then_recover(), min_agents(3)).await;
            let report = res.expect("the run recovered");
            // `agents`, NOT `verdicts`: `MagiReport` has no such field, and its field of
            // produced outputs is `pub agents: Vec<AgentOutput>`.
            assert_eq!(report.agents.len(), 3);
        }

        #[tokio::test]
        async fn a_truly_dead_endpoint_still_aborts_fast() {
            // Without this, the fix silently degrades into "consult only at the end"
            // and nothing would tell us. "It does not wait" is asserted by the 10 s
            // hang guard inside `analyze_with`: seats 2 and 3 never produce on their
            // own, so a loop that waited for them would hang into it instead of
            // returning. The count below cannot vary with the implementation -- it is
            // 1 whenever the run returns at all -- so it pins the FIXTURE's shape:
            // exactly one seat produced before the abort. If it ever reads 3 the
            // barriers are gone and this test measures nothing about waiting.
            let (res, joins) = analyze_with(dead_endpoint(), min_agents(3)).await;
            assert!(matches!(res, Err(MagiError::EndpointDown { .. })));
            assert_eq!(
                joined_before_abort(&joins),
                1,
                "only seat 1 produces in this fixture; the other two are cancelled"
            );
        }

        #[tokio::test]
        async fn the_criterion_follows_min_agents_in_both_directions() {
            // successful=1, remaining=1, latch on -- which blip_then_recover() does NOT
            // produce: it ends all-Ok, so the criterion is never even evaluated with a
            // remaining handle. Its own fixture: seat 1 Ok, seat 2 Err, seat 3 still
            // pending, with seat 2 barriered behind seat 1's join so the state at the
            // decision point is the one the spec names.
            let aborted = |consensus| async move {
                let (res, _) = analyze_with(one_ok_one_err_one_pending(), consensus).await;
                matches!(res, Err(MagiError::EndpointDown { .. }))
            };
            assert!(!aborted(min_agents(2)).await, "1+1 >= 2 -> keep going");
            assert!(aborted(min_agents(3)).await, "1+1 < 3 -> abort here");

            // And the THIRD direction, which was declared correct and left unpinned:
            // with min_agents <= 1 the criterion is vacuous BY ARITHMETIC -- a single
            // success already reaches the quorum, so `successful + remaining <
            // min_agents` never holds and the run does not abort on endpoint-down. A
            // declaration without a test is a preference; this line makes it a property.
            assert!(
                !aborted(min_agents(1)).await,
                "min_agents 1 -> the criterion is vacuous"
            );
        }

        #[tokio::test]
        async fn an_unreachable_quorum_without_the_latch_is_not_endpoint_down() {
            // Two schema failures, no connection failure at all.
            let (res, _) = analyze_with(two_schema_failures(), min_agents(3)).await;
            let err = res.unwrap_err();
            assert!(matches!(err, MagiError::InsufficientAgents { .. }));
        }

        #[tokio::test]
        async fn the_panic_path_applies_the_same_criterion() {
            // resolve_abnormal_exit also consults the latch; omitting it leaves the
            // same door open by another route -- literally the defect 4.0.0 found there.
            //
            // THIS TEST ENTERS THROUGH THE JOIN LOOP, it does not call
            // `resolve_abnormal_exit` directly, and that is not a style preference: the
            // two magnitudes of the criterion -- the successes so far and the handles
            // still to join -- are captured by the join loop itself at the call site,
            // and `resolve_abnormal_exit` inherits the conjunction by delegating to
            // `resolve_run_abort` with those numbers, without evaluating it on its own.
            let (res, _) =
                analyze_with(one_panicking_seat_with_reachable_quorum(), min_agents(2)).await;
            assert!(res.is_ok());
        }

        #[tokio::test]
        async fn the_latch_is_read_fresh_on_every_iteration() {
            // The latch turns on AFTER the loop started -- seat 1 joins clean, THEN two
            // connection failures land -- so a value cached before the loop would never
            // see it. That is the signal the per-outcome check exists to recover, and
            // with the other five green a cached read stays invisible.
            let (res, _) = analyze_with(latch_after_first_join(), min_agents(3)).await;
            // A cached read would never see a latch that turns on mid-loop, so the run
            // would NOT abort. The variant alone separates the two implementations; a
            // count assertion here would need a production hook to observe, and buys
            // nothing this does not already decide.
            assert!(matches!(res, Err(MagiError::EndpointDown { .. })));
        }

        /// Scaffolding proving scaffolding, and it earns its place: these three rules
        /// hold up every test in this module, so if the bridge behaved differently they
        /// would all measure something else and none would say so.
        #[tokio::test]
        async fn a_seat_provider_honours_its_three_rules() {
            fn beh(first: Outcome, waits: Option<usize>) -> SeatBehaviour {
                SeatBehaviour {
                    lineage: "l",
                    first,
                    after_rotation: None,
                    waits_for_joins: waits,
                }
            }
            let (tx, _rx) = tokio::sync::watch::channel(0usize);
            let cfg = CompletionConfig::default();

            // Rule 2 -- it produces `first`, and it produces it for ITS OWN agent. A
            // verdict carrying another mage's name is rejected by the agent-identity
            // post-validation, so this is what keeps every `Ok` seat countable.
            let ok = SeatProvider::new(
                AgentName::Balthasar,
                beh(Outcome::Ok, None),
                tx.clone(),
                None,
            );
            let out = ok
                .complete("", "", &cfg)
                .await
                .expect("Ok produces a completion");
            assert!(
                out.text.contains("balthasar"),
                "the verdict carries the agent this seat serves"
            );

            let bad = SeatProvider::new(
                AgentName::Caspar,
                beh(Outcome::ConnectionFailure, None),
                tx.clone(),
                None,
            );
            assert!(bad.complete("", "", &cfg).await.is_err());

            // Rules 1 and 3 are asserted BY POLLING, not with a clock: a heuristic
            // deadline is the flaky scenario the harness doctrine names last among its
            // antipatterns, and none is needed here.
            let mut cx = std::task::Context::from_waker(std::task::Waker::noop());

            // Rule 1 -- with no barrier the body has no `.await` at all, so the FIRST
            // poll already returns Ready. "Ready without yielding" is strictly stronger
            // than "finished within a second", and it cannot flake.
            {
                let mut fut = std::pin::pin!(ok.complete("", "", &cfg));
                assert!(
                    std::future::Future::poll(fut.as_mut(), &mut cx).is_ready(),
                    "a seat with no barrier is ready on the first poll"
                );
            }

            // Rule 3 -- the counter is bumped AFTER producing, so a seat barriered at 1
            // stays blocked until one join HAS happened. `Pending` on the first poll IS
            // "still blocked", measured and not timed.
            let (btx, brx) = tokio::sync::watch::channel(0usize);
            let barriered = SeatProvider::new(
                AgentName::Melchior,
                beh(Outcome::Ok, Some(1)),
                btx.clone(),
                Some(brx),
            );
            let mut fut = std::pin::pin!(barriered.complete("", "", &cfg));
            assert!(
                std::future::Future::poll(fut.as_mut(), &mut cx).is_pending(),
                "no join yet -> still blocked"
            );
            btx.send_modify(|n| *n += 1);
            // This timeout is NOT the assertion: it is a guard so that an implementation
            // which never releases FAILS instead of hanging the suite.
            assert!(
                tokio::time::timeout(std::time::Duration::from_secs(5), fut)
                    .await
                    .expect("the barrier never released")
                    .is_ok(),
                "one join -> released"
            );
        }
    }
}

#[cfg(test)]
mod characterization_tests {
    use super::*;
    use crate::error::{AbandonReason, ExternalErrorKind, ResponseContractCause};
    use crate::provider::{CompletionTelemetry, FinishReason, is_mage_local, is_retryable};
    use std::time::Duration;

    /// ROWS of the characterization. Lives attached to `enumerated` ON PURPOSE: it is a
    /// conscious-edit ceiling, not a compiler guarantee. If you add an arm there, add its
    /// entry to `all_errors` and raise this number -- in that order, and never the number
    /// alone.
    ///
    /// 19 = 12 variants - 2 with an inner cause + 6 (`ExternalErrorKind`)
    ///                                          + 3 (`ResponseContractCause`)
    const CHARACTERIZED_ROWS: usize = 19;

    /// Forces the compiler to demand a decision per variant AND per inner cause.
    ///
    /// It returns nothing and nothing compares its output: the expected column comes from
    /// the dump. Its whole value is that a new variant -- or a new `ExternalErrorKind`, or a
    /// new `ResponseContractCause` -- does not compile until someone opens this function.
    /// Collapsing the inner matches to `External { .. }` would save six lines and lose the
    /// only guarantee the mechanism gives.
    fn enumerated(err: &ProviderError) {
        match err {
            // THE TWO AXES. A match on the variant alone gives twelve arms while the table
            // has nineteen rows: the cross product of variant x cause.
            ProviderError::External { kind, .. } => match kind {
                ExternalErrorKind::Network => (),
                ExternalErrorKind::Timeout => (),
                ExternalErrorKind::Auth => (),
                ExternalErrorKind::RateLimit => (),
                ExternalErrorKind::ServerError => (),
                ExternalErrorKind::Other => (),
                // no `_`: a new kind breaks compilation
            },
            // THREE causes. That all three yield the same shape does not make them one:
            // what this axis enumerates are CAUSES, not distinct shapes.
            ProviderError::ResponseContract { reason, .. } => match reason {
                ResponseContractCause::Unreadable => (),
                ResponseContractCause::NoMessage => (),
                ResponseContractCause::RedirectRefused => (),
            },
            ProviderError::ResponseTooLarge { .. } => (),
            ProviderError::Timeout { .. } => (),
            ProviderError::Http { .. } => (),
            ProviderError::Network { .. } => (),
            ProviderError::Auth { .. } => (),
            ProviderError::Process { .. } => (),
            ProviderError::RetryAbandoned { .. } => (),
            ProviderError::NestedSession => (),
            ProviderError::EmptyCompletion { .. } => (),
            ProviderError::NoGeneration { .. } => (),
            // NONE `_`: a new variant breaks compilation here first.
        }
    }

    /// The nineteen entries, in the table's order. ONE list of errors, written against the
    /// intact tree -- the shapes are not here on purpose, see `all_cases`.
    fn all_errors() -> Vec<ProviderError> {
        vec![
            ProviderError::ResponseTooLarge { limit: 1 << 20 },
            ProviderError::external("backend", ExternalErrorKind::Network),
            ProviderError::external("backend", ExternalErrorKind::Timeout),
            ProviderError::external("backend", ExternalErrorKind::Auth),
            ProviderError::external("backend", ExternalErrorKind::RateLimit),
            ProviderError::external("backend", ExternalErrorKind::ServerError),
            ProviderError::external("backend", ExternalErrorKind::Other),
            ProviderError::Timeout {
                message: "t".to_string(),
            },
            ProviderError::Http {
                status: 500,
                body: String::new(),
                retry_after_raw: vec![],
                received_at: None,
            },
            ProviderError::Network {
                message: "n".to_string(),
            },
            ProviderError::Auth {
                message: "a".to_string(),
            },
            ProviderError::Process {
                exit_code: Some(1),
                stderr: "p".to_string(),
            },
            ProviderError::RetryAbandoned {
                reason: AbandonReason::OperationBudgetExhausted {
                    elapsed: Duration::from_secs(1),
                    budget: Duration::from_secs(1),
                },
                attempts: 2,
            },
            ProviderError::NestedSession,
            ProviderError::ResponseContract {
                reason: ResponseContractCause::Unreadable,
                detail: String::new(),
            },
            ProviderError::ResponseContract {
                reason: ResponseContractCause::NoMessage,
                detail: String::new(),
            },
            ProviderError::ResponseContract {
                reason: ResponseContractCause::RedirectRefused,
                detail: String::new(),
            },
            ProviderError::EmptyCompletion {
                telemetry: CompletionTelemetry::unmeasured().with_finish(FinishReason::Length),
                cap: 4096,
            },
            ProviderError::NoGeneration {
                done_reason: Some(FinishReason::Length),
            },
        ]
    }

    /// The comparable shape of an outcome: variant, kind, and -- for transports -- the
    /// `connection` flag.
    ///
    /// `ModelOutcome` derives only `Debug`, never `PartialEq`, so equality is not available
    /// and adding the derive would be production surface added to accommodate a test. The
    /// `connection` flag is INCLUDED and that is mandatory: it is the exact field a
    /// narrowing of the connection classifier would move, and a shape blind to it leaves
    /// the net blind precisely where the risk lives.
    fn shape(o: &ModelOutcome) -> String {
        match o {
            ModelOutcome::Success(_) => "Success".to_string(),
            ModelOutcome::Schema(_) => "Schema".to_string(),
            ModelOutcome::Transport {
                connection, kind, ..
            } => format!("Transport/{kind:?}/conn={connection}"),
            ModelOutcome::MageLocal { kind, .. } => format!("MageLocal/{kind:?}"),
            ModelOutcome::CrateDefect { .. } => "CrateDefect".to_string(),
            ModelOutcome::OversizedResponse { .. } => "OversizedResponse".to_string(),
            ModelOutcome::ExternalFailure { kind, .. } => format!("ExternalFailure/{kind:?}"),
            ModelOutcome::Unexpected(_) => "Unexpected".to_string(),
        }
    }

    /// The expected column, PASTED from the dump of the intact tree, in `all_errors`' order.
    ///
    /// It is not transcribed from a reading of `provider_err_outcome`: it is what that
    /// function actually returned, which is the whole point of generating it. Its length is
    /// checked BY THE COMPILER against `CHARACTERIZED_ROWS` -- one shape too many or too few
    /// does not compile, which is what makes the positional `zip` below safe.
    const SHAPES: [&str; CHARACTERIZED_ROWS] = [
        "OversizedResponse",
        "ExternalFailure/Network",
        "ExternalFailure/Timeout",
        "ExternalFailure/Auth",
        "ExternalFailure/RateLimit",
        "ExternalFailure/ServerError",
        "ExternalFailure/Other",
        "Transport/Timeout/conn=false",
        "Transport/Transport/conn=false",
        // The ONLY row with `conn=true`, and it is the premise criterion (d) rests on:
        // `is_connection` answers true for `Network` and for nothing else.
        "Transport/Transport/conn=true",
        "Transport/Transport/conn=false",
        "Transport/Transport/conn=false",
        "Transport/Transport/conn=false",
        "Transport/Transport/conn=false",
        "MageLocal/ResponseContract",
        "MageLocal/ResponseContract",
        "MageLocal/ResponseContract",
        "MageLocal/EmptyCompletion",
        "CrateDefect",
    ];

    /// DERIVED, never a second hand-written list.
    ///
    /// The entries cannot drift from `all_errors` because there is only one list of errors,
    /// and the pairing is positional by construction rather than by a transcription somebody
    /// has to keep aligned.
    fn all_cases() -> Vec<(ProviderError, &'static str)> {
        all_errors().into_iter().zip(SHAPES).collect()
    }

    /// One row per `ProviderError` variant and inner cause, with TODAY's outcome.
    ///
    /// This is a photograph, not a TDD test: it is written and run against the untouched
    /// tree, so it is born green. Its lasting value is downstream -- if a later milestone
    /// touched `provider_err_outcome`, this names the change row by row instead of letting it
    /// arrive mixed with an intended one.
    #[test]
    fn provider_err_outcome_is_unchanged_for_every_variant() {
        assert_eq!(
            all_errors().len(),
            CHARACTERIZED_ROWS,
            "entries and ceiling diverge: a variant was added without its row, or the number \
             was raised without one. THE FIX IS TO ADD THE ROW, never to edit the constant"
        );

        for (err, expected) in all_cases() {
            // Its effect is of COMPILATION, not of execution: nothing compares its output.
            enumerated(&err);
            // `provider_err_outcome` takes the error BY VALUE and computes the connection
            // flag itself, so the clone is what lets the assertion message name the input.
            assert_eq!(
                shape(&provider_err_outcome(err.clone())),
                expected,
                "variant {err:?}"
            );
        }
    }

    /// What a `ModelOutcome` condemns.
    ///
    /// THREE categories, not two, and the third is what makes the table satisfiable. A boolean
    /// `is_mage_local(err) == !condemns_run_wide(outcome)` is false by construction for
    /// `NoGeneration`: it maps to `CrateDefect` and the run ABORTS — it condemns the lineage
    /// neither for one seat nor for all. Under the boolean, `CrateDefect` would fall into "does
    /// not condemn run-wide" while the assertion demanded it be mage-local.
    #[derive(Debug)]
    enum Condemnation {
        MageLocal,
        RunWide,
        AbortsRun,
        NotAFailure,
    }

    /// Read from production, never guessed: the only arm that calls
    /// `register_transport_failure` is `Transport`, and the `MageLocal` arm says in writing
    /// what it does NOT call.
    ///
    /// This IS a hand-written replica, and that is the declared limit of the arms it governs:
    /// if a later milestone moved a `ModelOutcome`'s consequence, this table would go stale and
    /// the test would stay green.
    ///
    /// **Criteria (a) and (d) do not share that weakness** — they assert against
    /// `provider_err_outcome` and `is_connection` directly, with no replica in between.
    /// **The `AbortsRun` row DOES share it**, and saying otherwise would be the defect this
    /// release exists to remove: what that row still asserts for real is that a class which
    /// aborts the run is not retryable, which a change to either predicate would redden. That
    /// the run aborts at all is the replica's claim, not production's.
    fn outcome_condemnation(o: &ModelOutcome) -> Condemnation {
        match o {
            ModelOutcome::Transport { .. } => Condemnation::RunWide,
            ModelOutcome::MageLocal { .. } => Condemnation::MageLocal,
            ModelOutcome::Schema(_) => Condemnation::MageLocal,
            ModelOutcome::ExternalFailure { .. } => Condemnation::MageLocal,
            ModelOutcome::OversizedResponse { .. } => Condemnation::MageLocal,
            ModelOutcome::CrateDefect { .. } => Condemnation::AbortsRun,
            ModelOutcome::Success(_) => Condemnation::NotAFailure,
            // Read, not guessed: the join loop releases the seat and returns the error without
            // registering a transport failure and without rotating — the run continues with the
            // other two seats, which is mage-local by definition.
            //
            // THREE ROWS ARE UNREACHABLE FROM THE TABLE and are pure assertion (named by
            // identifier, not by position -- one of them is the row below this comment):
            // `provider_err_outcome` never produces `Schema`, `Success` or `Unexpected`, so
            // nothing here exercises them. `Success` is at least guarded by
            // `unreachable_in_this_table`; the other two are hand-written claims about
            // production that no row checks. Said plainly so the next reader knows where the
            // replica stops being verified.
            ModelOutcome::Unexpected(_) => Condemnation::MageLocal,
            // NO `_`: a new variant breaks compilation.
        }
    }

    /// Fails with the offending error in view rather than panicking blind.
    fn unreachable_in_this_table(err: &ProviderError) -> ! {
        panic!("the characterization table has no failure-free rows, and this one is: {err:?}");
    }

    /// The mechanism that ties `is_mage_local` to `provider_err_outcome`.
    ///
    /// The predicate and the outcome mapper encode one rule in two places. Rather than
    /// refactoring the mapper to call the predicate — which would mean collapsing its arms
    /// into an `if` and reintroducing the catch-all `4.0.0` removed from that very function —
    /// the two are tied by this test. It is the stronger of the two options: the
    /// arms survive, so a new variant still breaks compilation until someone decides its
    /// consequence, AND the test fails the day the two writings disagree.
    #[test]
    fn mage_local_classes_never_reach_the_run_wide_arm() {
        // THE SAME rows as the characterization, through the same helper. Two parallel lists
        // could let this pass over a subset.
        let mut reached = 0usize;
        for (err, _expected) in all_cases() {
            let outcome = provider_err_outcome(err.clone());

            if is_mage_local(&err) {
                // CRITERION (a), DERIVED: `run_failed` has one writer, inside
                // `register_transport_failure`, and that has one production caller — the
                // `Transport` arm. So "does not enter run_failed" IS "the outcome is not
                // Transport", through the only path that exists.
                assert!(
                    !matches!(outcome, ModelOutcome::Transport { .. }),
                    "a mage-local class cannot exit through the arm that condemns run-wide: {err:?}"
                );
                // CRITERION (d), DERIVED: the latch feeds from `is_connection`, which answers
                // true only for `Network`. Asserted against the REAL function, not a copy.
                assert!(
                    !is_connection(&err),
                    "a mage-local class cannot feed the endpoint-down latch: {err:?}"
                );
            }

            match outcome_condemnation(&outcome) {
                Condemnation::MageLocal => assert!(
                    is_mage_local(&err),
                    "the classifier leaves it mage-local and the predicate does not: {err:?}"
                ),
                Condemnation::RunWide => assert!(
                    !is_mage_local(&err),
                    "the classifier condemns it run-wide and the predicate thinks it local: {err:?}"
                ),
                // The third, DERIVED from production like (a) and (d) rather than left empty:
                // `crate_defect_of` returns `Some` for exactly the `CrateDefect` outcome and no
                // other. NOT claimed: that it is the only producer of a `CrateDefectRecord` —
                // the join loop builds one too, and this derivation does not need it not to.
                // The live half is the retryability one. A `crate_defect_of(...).is_some()`
                // check stood here and was REMOVED: that function IS
                // `matches!(provider_err_outcome(err), CrateDefect { .. })`, and this arm is
                // only entered because the same outcome classified as `AbortsRun` -- it
                // recomputed the condition it was standing inside and could not fail.
                Condemnation::AbortsRun => {
                    assert!(
                        !is_retryable(&err),
                        "a class that aborts the run cannot be retryable: {err:?}"
                    );
                }
                Condemnation::NotAFailure => unreachable_in_this_table(&err),
            }

            // THE COUPLING `is_mage_local` <-> `is_retryable`, load-bearing and until now
            // implicit. R-1's guarantee only BITES on a class that is both: only a retryable
            // one reaches an abandon exit, and only a mage-local one comes back unwrapped. If
            // either predicate moved, the guarantee would keep holding VACUOUSLY over a smaller
            // set and nothing would say so.
            if is_mage_local(&err) && is_retryable(&err) {
                reached += 1;
            }
        }

        // FIVE, and the number is DERIVED rather than counted by hand: `is_mage_local` is true
        // for every `External` and every `ResponseContract`; `is_retryable` is true for four of
        // the six `ExternalErrorKind` — not `Auth`, not `Other` — and for one of the three
        // `ResponseContractCause`, `Unreadable`. 4 + 1.
        //
        // WHAT THIS PINS, exactly: a COUNT, not the identity of the five. It catches the set
        // growing or shrinking, which is the direction that matters. It does NOT catch a swap
        // that keeps the size. Declared rather than dressed up.
        assert_eq!(
            reached, 5,
            "the set R-1 actually reaches moved: it is External{{Network,Timeout,RateLimit,\
             ServerError}} plus ResponseContract{{Unreadable}} -- RECOUNT before touching this 5"
        );
    }
}
