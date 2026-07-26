// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-07-26

//! Per-agent lineage rotation (MS2): a dead model **rotates** to another lineage
//! instead of degrading the run.
//!
//! When a mage's model fails — transport, schema, or timeout — the orchestrator
//! rotates it to the next eligible fallback [`Lineage`] rather than dropping to a
//! degraded consensus. A [`Lineage`] is a **declared** model-family label, never
//! inferred from a model name or response.
//!
//! # [`RotationPolicy`] — pure and total
//!
//! [`RotationPolicy::next_model`] is the eligibility decision: given a mage's
//! per-attempt state it returns the first eligible fallback in declared order, or
//! `None`. It performs **no I/O, never `await`s, never panics, and never returns
//! `Err`** — the probe-derived window/digest data it reads was gathered once in the
//! preflight ([`run_preflight`]) and cached in a [`ModelCapability`] map.
//!
//! # [`LineageRegistry`] — concurrency invariants
//!
//! Run-wide rotation state lives behind **exactly one** `tokio::sync::Mutex`, and
//! it is **never held across an `await`**. Therefore deadlock is impossible by
//! construction and there is no second lock to order against.
//! [`LineageRegistry::claim_next`] does the whole read-decide-commit under that
//! single lock; its postcondition is strict: `Some` → the mage's active entry was
//! **replaced**; `None` → the registry is left **intact**. Its digest re-propose
//! loop terminates because each rejection grows `window_rejected` (which
//! `next_model` excludes) over a finite pool. Slot cleanup runs on **every** mage
//! exit — success, error, panic, and cancellation — via the succeeded-flag guard
//! (`AgentSlotGuard`): a valid verdict keeps the lineage; anything else releases
//! it.
//!
//! # Scope asymmetry (R6) — why the failure class matters
//!
//! A **transport** failure condemns the lineage **run-wide** (every mage then
//! avoids it); a **schema** failure is **mage-local** (only that mage stops
//! retrying it). A panic **never** rotates. Worked example: pool `[A, B, C]`,
//! Caspar rotates `A → B`, then Melchior fails. `B` is in play, so Melchior takes
//! **C** if `A` was condemned run-wide (transport), but takes **A** if `A` only
//! schema-failed for Caspar (mage-local, freed once Caspar moved on). The naive
//! "A already failed, avoid it" intuition is wrong for the schema path.
//!
//! # Digest verify — fail-OPEN (R17)
//!
//! To catch *ensemble collapse* (two lineages that are secretly the same weights),
//! [`claim_next`](LineageRegistry::claim_next) compares model digests. The rule is
//! **fail-open**: a candidate is rejected ONLY on a *proven* collision — its
//! **resolvable** digest equals a **resolvable** active mage's digest. An
//! unresolvable (`None`) digest — because a probe is down or a provider has no
//! probe — is **trusted by the declared lineage** and never collides (user
//! decision 2026-07-26). The operator's lineage labels are therefore load-bearing:
//! two identical models mislabeled as distinct are caught **only if** their digests
//! resolve, so a provider without a [`ProviderProbe`] gets no ensemble-collapse
//! protection. The architecture↔lineage check (`family_verdict`) is out of scope
//! here (MS4).

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use crate::error::ProviderError;
use crate::provider::LlmProvider;
use crate::schema::AgentName;

/// A declared model-family lineage label (e.g. `"alibaba"`, `"deepseek"`).
///
/// A `Lineage` is a **declared** identity — never inferred from a model name or
/// response. It is the primary diversity key for rotation: two live mages never
/// share a lineage. Backed by `Cow<'static, str>` so a `&'static str` literal is
/// zero-alloc (`Borrowed`) while a runtime `String` is `Owned`.
///
/// # Normalization (R3.2)
///
/// All constructors **trim** leading/trailing whitespace. Construction is
/// infallible: an empty result (`""`, or all-whitespace) is a valid `Lineage`
/// *value*; it is rejected as invalid input later, at `MagiBuilder::build()`.
///
/// ```
/// use magi_core::rotation::Lineage;
/// assert_eq!(Lineage::new(" alibaba ").as_str(), "alibaba");
/// assert_eq!(Lineage::new("deepseek"), Lineage::from("deepseek"));
/// ```
#[derive(
    Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, serde::Serialize, serde::Deserialize,
)]
pub struct Lineage(Cow<'static, str>);

impl Lineage {
    /// Creates a `Lineage`, trimming surrounding whitespace (R3.2).
    ///
    /// Accepts anything convertible into `Cow<'static, str>` — a `&'static str`
    /// stays `Borrowed` (zero-alloc), a `String` is `Owned`.
    pub fn new(s: impl Into<Cow<'static, str>>) -> Self {
        Self(trim_cow(s.into()))
    }

    /// Returns the lineage label as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Trims a `Cow<'static, str>` while preserving its variant: a `Borrowed`
/// `&'static str` stays borrowed (a trimmed `&'static str` is still `'static`),
/// and an already-trimmed `Owned` string is not re-allocated.
fn trim_cow(c: Cow<'static, str>) -> Cow<'static, str> {
    match c {
        Cow::Borrowed(s) => Cow::Borrowed(s.trim()),
        Cow::Owned(s) => {
            let trimmed = s.trim();
            if trimmed.len() == s.len() {
                Cow::Owned(s)
            } else {
                Cow::Owned(trimmed.to_owned())
            }
        }
    }
}

impl From<&'static str> for Lineage {
    fn from(s: &'static str) -> Self {
        Self(Cow::Borrowed(s.trim()))
    }
}

impl From<String> for Lineage {
    fn from(s: String) -> Self {
        Self(trim_cow(Cow::Owned(s)))
    }
}

impl fmt::Display for Lineage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A resolved fallback candidate the [`RotationPolicy`] may select.
///
/// `provider_ix` indexes into the owning `FallbackPool`'s provider list; `model`
/// is the candidate's model-id (from `provider.model()`). No `is_cloud`: the
/// digest verify is fail-open on an unresolvable digest, so provider kind is
/// irrelevant to eligibility.
#[derive(Clone)]
pub struct Candidate {
    pub provider_ix: usize,
    pub lineage: Lineage,
    pub model: String,
}

/// Pure, total rotation policy: given a mage's per-attempt state it returns the
/// first eligible fallback [`Candidate`] in declared order, or `None`.
///
/// It performs no I/O, never `await`s, never panics, and never returns `Err`.
/// The window/digest conditions (5–6) and their `capabilities` are added in a
/// later task; the [`RotationPolicy::next_model`] signature is stable from here.
pub struct RotationPolicy {
    fallback: Vec<Candidate>,
    max_rotations: u32,
    /// Probe-derived capabilities, keyed by model-id. Read zero-I/O by the window
    /// pre-filter (cond 6) and the digest lookup — the probe ran in the preflight.
    capabilities: BTreeMap<String, ModelCapability>,
    strict_context_guard: bool,
    min_window_tokens: usize,
}

impl RotationPolicy {
    /// Builds a policy over an ordered fallback list, a per-mage rotation cap
    /// (`max_rotations = 0` disables rotation entirely), and the preflight-derived
    /// `capabilities` map plus the window pre-filter config (`strict_context_guard`,
    /// `min_window_tokens`). The policy performs no I/O — it only reads this cached
    /// capability data.
    pub fn new(
        fallback: Vec<Candidate>,
        max_rotations: u32,
        capabilities: BTreeMap<String, ModelCapability>,
        strict_context_guard: bool,
        min_window_tokens: usize,
    ) -> Self {
        Self {
            fallback,
            max_rotations,
            capabilities,
            strict_context_guard,
            min_window_tokens,
        }
    }

    /// Test-only shim: a probe-free policy (empty capabilities, non-strict, zero
    /// min-window) that preserves the pure 4-condition behavior. Keeps the many
    /// probe-agnostic tests on ONE call so future arg growth touches only this shim.
    #[cfg(test)]
    pub(crate) fn for_tests_no_probe(fallback: Vec<Candidate>, max_rotations: u32) -> Self {
        Self::new(fallback, max_rotations, BTreeMap::new(), false, 0)
    }

    /// Returns the first eligible candidate in declared order, or `None`.
    ///
    /// Total: no I/O, no `await`, no panic, no `Err`. `window_rejected`
    /// (condition #5) is honored from the start so a later re-propose loop
    /// cannot spin. Deterministic in its arguments.
    pub fn next_model(
        &self,
        failed_lineages: &BTreeSet<Lineage>,
        run_failed_lineages: &BTreeSet<Lineage>,
        lineages_in_play: &BTreeSet<Lineage>,
        used: &BTreeSet<String>,
        window_rejected: &BTreeMap<String, &'static str>,
        rotations_done: u32,
    ) -> Option<&Candidate> {
        // Gate first: `max_rotations` reached (or 0 = disabled) → no candidate.
        if rotations_done >= self.max_rotations {
            return None;
        }
        // First eligible candidate in declared order. Conditions 1-4 are the pure
        // core; #5 (`window_rejected`) is empty without a probe but honored so the
        // digest re-propose loop cannot spin; #6 (window) reads the cached probe
        // capabilities (a model without a capability entry — no probe — passes,
        // preserving no-probe behavior).
        self.fallback.iter().find(|c| {
            !lineages_in_play.contains(&c.lineage)          // 1: another live mage has this lineage
                && !failed_lineages.contains(&c.lineage)    // 2: this mage schema-failed it
                && !run_failed_lineages.contains(&c.lineage) // 3: condemned run-wide (transport)
                && !used.contains(&c.model)                 // 4: this mage already ran this model
                && !window_rejected.contains_key(&c.model)  // 5: rejected by window/digest verify
                && self.window_admits(&c.model) // 6: context window large enough (or unknown)
        })
    }

    /// Condition #6 (R16): the candidate's measured context window is large enough,
    /// or (unmeasured) is admitted unless `strict_context_guard`. A model with no
    /// capability entry (non-probing provider) has `window: None` → admitted
    /// (non-strict), preserving no-probe behavior.
    fn window_admits(&self, model: &str) -> bool {
        let window = self.capabilities.get(model).and_then(|c| c.window);
        window_ok(window, self.min_window_tokens, self.strict_context_guard)
    }

    /// The digest cached for `model` by the preflight, or `None` when unresolved
    /// (probe down, or a non-probing provider). Read by the digest verify.
    pub(crate) fn digest_of(&self, model: &str) -> Option<String> {
        self.capabilities.get(model).and_then(|c| c.digest.clone())
    }
}

/// Number of DISTINCT connection-failing lineages that trips the run-wide
/// endpoint-down fast-fail.
pub const ENDPOINT_DOWN_LINEAGE_THRESHOLD: usize = 2;

/// What each ACTIVE mage is running — carries `model` (not just `lineage`) so the
/// digest verify can look up its digest.
#[derive(Clone)]
pub struct ActiveEntry {
    pub lineage: Lineage,
    pub model: String,
}

/// Mutable run-wide rotation state, guarded by the registry's single lock.
struct RegistryInner {
    active: BTreeMap<AgentName, ActiveEntry>,
    run_failed: BTreeSet<Lineage>,
    connection_failed: BTreeSet<Lineage>,
    endpoint_down_signalled: bool,
}

/// Shared run-wide rotation state behind **a single** `tokio::sync::Mutex`.
///
/// # Concurrency invariants
///
/// There is exactly ONE lock, and it is **never held across an `await`** (every
/// method locks, does synchronous work, and drops the guard). Therefore deadlock
/// is impossible by construction and there is no second lock to order against.
/// `register_transport_failure` registers a condemned lineage and decides the
/// endpoint-down latch **atomically under the lock** — reading the count in a
/// separate call would be a TOCTOU race.
pub struct LineageRegistry {
    lock: Mutex<RegistryInner>,
}

impl LineageRegistry {
    /// Builds a registry seeded with the trio's active `(lineage, model)` entries.
    pub fn new(initial: BTreeMap<AgentName, ActiveEntry>) -> Self {
        Self {
            lock: Mutex::new(RegistryInner {
                active: initial,
                run_failed: BTreeSet::new(),
                connection_failed: BTreeSet::new(),
                endpoint_down_signalled: false,
            }),
        }
    }

    /// Snapshot of the lineages held by every OTHER live mage (excludes `exclude`).
    pub async fn lineages_in_play(&self, exclude: AgentName) -> BTreeSet<Lineage> {
        let g = self.lock.lock().await;
        g.active
            .iter()
            .filter(|(a, _)| **a != exclude)
            .map(|(_, e)| e.lineage.clone())
            .collect()
    }

    /// Snapshot copy of the run-wide condemned lineages.
    pub async fn run_failed_lineages(&self) -> BTreeSet<Lineage> {
        self.lock.lock().await.run_failed.clone()
    }

    /// Connection-level condemned lineages — used to populate `EndpointDown`.
    pub async fn connection_failed_lineages(&self) -> Vec<Lineage> {
        self.lock
            .lock()
            .await
            .connection_failed
            .iter()
            .cloned()
            .collect()
    }

    /// Condemns `lineage` run-wide and, atomically, decides the endpoint-down
    /// fast-fail. Returns `true` for **exactly one** caller — the one whose
    /// connection failure crosses [`ENDPOINT_DOWN_LINEAGE_THRESHOLD`] distinct
    /// connection lineages (the latch fires once). A non-connection failure
    /// (`connection = false`, e.g. `Http 5xx`/timeout) condemns but never counts
    /// toward the latch and always returns `false`.
    pub async fn register_transport_failure(&self, lineage: Lineage, connection: bool) -> bool {
        let mut g = self.lock.lock().await;
        g.run_failed.insert(lineage.clone());
        if !connection {
            return false;
        }
        g.connection_failed.insert(lineage);
        if g.connection_failed.len() < ENDPOINT_DOWN_LINEAGE_THRESHOLD {
            return false;
        }
        if g.endpoint_down_signalled {
            return false;
        }
        g.endpoint_down_signalled = true;
        true
    }

    /// Releases a mage's active slot. Idempotent — releasing an absent agent is a
    /// no-op.
    pub async fn release(&self, agent: AgentName) {
        self.lock.lock().await.active.remove(&agent);
    }

    /// `true` iff endpoint-down was already signalled (for lost-signal recovery).
    pub async fn endpoint_down_signalled(&self) -> bool {
        self.lock.lock().await.endpoint_down_signalled
    }
}

/// Maximum length, in Unicode scalar values, of a [`RotationEvent`]'s `detail`.
const MAX_ROTATION_DETAIL_CHARS: usize = 256;

/// Why a mage left a model — the cause that triggered a rotation hop. Connection
/// and HTTP failures both normalize to `Transport`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RotationKind {
    /// Transport failure (connection refused, HTTP error, `RetryProvider` exhausted).
    Transport,
    /// The model's response failed the verdict schema (after the corrective retry).
    Schema,
    /// The attempt timed out.
    Timeout,
}

impl fmt::Display for RotationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            RotationKind::Transport => "transport",
            RotationKind::Schema => "schema",
            RotationKind::Timeout => "timeout",
        })
    }
}

/// A single completed rotation hop (`from` → `to`) with its cause and a
/// human-readable diagnostic `detail`.
///
/// Fields are `pub(crate)`: the ONLY construction path is `RotationEvent::new`,
/// which sanitizes and length-caps `detail`, so no unsanitized or oversized string
/// can enter. Read via the accessors; serde serializes the fields directly for JSON.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct RotationEvent {
    pub(crate) from: Lineage,
    pub(crate) to: Lineage,
    pub(crate) model_resolved: String,
    pub(crate) kind: RotationKind,
    pub(crate) detail: String,
}

impl RotationEvent {
    /// Builds a rotation hop, **sanitizing** `detail` (zero-width / control chars
    /// stripped via `clean_title`) and **truncating** it to
    /// [`MAX_ROTATION_DETAIL_CHARS`] on a char boundary. `detail` may carry an
    /// untrusted error body, so this bounds every telemetry record.
    pub(crate) fn new(
        from: Lineage,
        to: Lineage,
        model_resolved: String,
        kind: RotationKind,
        detail: String,
    ) -> Self {
        let cleaned = crate::validate::clean_title(&detail);
        let detail = cleaned.chars().take(MAX_ROTATION_DETAIL_CHARS).collect();
        Self {
            from,
            to,
            model_resolved,
            kind,
            detail,
        }
    }
    /// The lineage the mage rotated away FROM.
    pub fn from(&self) -> &Lineage {
        &self.from
    }
    /// The lineage the mage rotated TO.
    pub fn to(&self) -> &Lineage {
        &self.to
    }
    /// The model-id resolved behind the destination lineage.
    pub fn model_resolved(&self) -> &str {
        &self.model_resolved
    }
    /// The cause of the hop.
    pub fn kind(&self) -> RotationKind {
        self.kind
    }
    /// The sanitized, length-capped human-readable diagnostic.
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

/// Per-agent rotation telemetry — populated for EVERY mage (successful OR failed).
/// `model_used` is the last model attempted; `chain` is the ordered list of hops
/// (empty when the mage never rotated).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct AgentRotation {
    pub model_configured: String,
    pub model_used: String,
    pub chain: Vec<RotationEvent>,
    pub ran_unmeasured: bool,
}

impl AgentRotationState {
    /// Produces the always-present [`AgentRotation`] telemetry record from this
    /// state — the FSM's single sink, so a panicked or first-try mage still yields
    /// a present, chain-empty record (never an absent one). Maps the four shared
    /// fields 1:1.
    pub(crate) fn to_rotation(&self) -> AgentRotation {
        AgentRotation {
            model_configured: self.model_configured.clone(),
            model_used: self.model_used.clone(),
            chain: self.chain.clone(),
            ran_unmeasured: self.ran_unmeasured,
        }
    }
}

/// Per-mage, per-run rotation state — **local to each mage, never shared**.
///
/// `used`/`failed_lineages`/`rotations_done` persist across a mage's rotation
/// attempts; `window_rejected` is cleared at the start of each `claim_next`
/// (dynamic rejections must be re-evaluated). `succeeded` gates cleanup.
pub struct AgentRotationState {
    pub model_configured: String,
    pub model_used: String,
    pub chain: Vec<RotationEvent>,
    pub used: BTreeSet<String>,
    pub failed_lineages: BTreeSet<Lineage>,
    pub window_rejected: BTreeMap<String, &'static str>,
    pub rotations_done: u32,
    pub succeeded: bool,
    pub ran_unmeasured: bool,
}

impl LineageRegistry {
    /// Reserve the next eligible fallback for `agent`, committing it under the
    /// single lock (read-decide-commit).
    ///
    /// Postcondition: `Some` → the mage's `active` entry was **replaced** by the
    /// chosen candidate; `None` → the registry is left **intact**. `window_rejected`
    /// is cleared at entry so a dynamic (digest) rejection is re-evaluated on the
    /// next call.
    ///
    /// Digest verify (R17, **fail-open**): after `next_model` picks a candidate, it
    /// is rejected ONLY on a *proven* collision — its **resolvable** digest equals a
    /// **resolvable** ACTIVE mage's digest (the calling agent is excluded, W6). An
    /// unresolvable (`None`) digest — candidate or active — never collides, so it is
    /// trusted by the declared lineage. A rejected candidate is marked in
    /// `window_rejected` and the loop re-proposes the next eligible one; the set
    /// only grows over a finite pool, so the loop terminates. The whole
    /// read-decide-commit runs under the single lock (no `await` inside — the probe
    /// ran in the preflight).
    pub async fn claim_next(
        &self,
        agent: AgentName,
        policy: &RotationPolicy,
        state: &mut AgentRotationState,
    ) -> Option<Candidate> {
        let mut g = self.lock.lock().await;
        state.window_rejected.clear();
        loop {
            let in_play: BTreeSet<Lineage> = g
                .active
                .iter()
                .filter(|(a, _)| **a != agent)
                .map(|(_, e)| e.lineage.clone())
                .collect();
            let chosen = policy
                .next_model(
                    &state.failed_lineages,
                    &g.run_failed,
                    &in_play,
                    &state.used,
                    &state.window_rejected,
                    state.rotations_done,
                )?
                .clone();

            // The candidate is index 0; every OTHER active mage's digest follows.
            // `digest_collision` returning `Some((0, _))` means the candidate's
            // resolvable digest matches a resolvable active's → proven collision.
            let mut digests = vec![policy.digest_of(&chosen.model)];
            for (other, entry) in g.active.iter() {
                if *other == agent {
                    continue; // W6: never compare the calling agent against itself
                }
                digests.push(policy.digest_of(&entry.model));
            }
            if matches!(digest_collision(&digests), Some((0, _))) {
                state
                    .window_rejected
                    .insert(chosen.model.clone(), "digest_collision");
                continue; // re-propose the next eligible candidate
            }

            g.active.insert(
                agent,
                ActiveEntry {
                    lineage: chosen.lineage.clone(),
                    model: chosen.model.clone(),
                },
            );
            return Some(chosen);
        }
    }
}

/// RAII guard that releases a mage's lineage slot on drop **unless** the mage
/// succeeded or was already released explicitly.
///
/// This is the cancellation-safe cleanup: an explicit `release()` call in the
/// exit branches does NOT run when the task future is dropped mid-`await`
/// (cancellation), but a `Drop` impl DOES — so the guard covers success-skip,
/// normal failure, panic-unwind, AND cancellation-drop. `release` is async and
/// `Drop` is sync, so drop does a best-effort synchronous release via the
/// registry mutex's `try_lock`; if contended, it detaches the cleanup on the
/// current runtime handle. `remove` is idempotent and keyed by `agent`, so a
/// redundant release is harmless and never touches another mage's slot.
///
/// Contended-drop nuance (documented, NOT a defect): on the detached-spawn path
/// the slot frees a scheduling tick later; in a pool-exhausted scenario that can
/// turn a would-be success into an honest `InsufficientAgents`, never an incorrect
/// verdict. The common paths (success/normal-failure via `mark_released`,
/// panic/cancellation via the uncontended `try_lock`) have no gap.
pub(crate) struct AgentSlotGuard {
    reg: Arc<LineageRegistry>,
    agent: AgentName,
    succeeded: bool,
    released: bool,
}
impl AgentSlotGuard {
    pub(crate) fn new(reg: Arc<LineageRegistry>, agent: AgentName) -> Self {
        Self {
            reg,
            agent,
            succeeded: false,
            released: false,
        }
    }
    /// Call the instant a valid verdict is committed — suppresses the release.
    pub(crate) fn mark_succeeded(&mut self) {
        self.succeeded = true;
    }
    /// Call after an explicit release on a normal exit path — suppresses the
    /// Drop fallback (no double-release, no contention window).
    pub(crate) fn mark_released(&mut self) {
        self.released = true;
    }
}
impl Drop for AgentSlotGuard {
    fn drop(&mut self) {
        // Success or an explicit release already handled it → nothing to do.
        if self.succeeded || self.released {
            return;
        }
        let agent = self.agent;
        // Fast path: uncontended (the unwinding/cancelled task holds no registry lock).
        if let Ok(mut g) = self.reg.lock.try_lock() {
            g.active.remove(&agent);
            return;
        }
        // Contended: `Drop` is sync and `release` is async, so detach the cleanup on
        // the current runtime. `remove` is idempotent and keyed by `agent`.
        if let Ok(h) = tokio::runtime::Handle::try_current() {
            let reg = self.reg.clone();
            h.spawn(async move {
                reg.release(agent).await;
            });
        } else {
            // No runtime to schedule on (e.g. runtime shutting down). Benign: the
            // per-run registry is being torn down anyway, so the slot has no observer.
            tracing::warn!(
                agent = agent.display_name(),
                "AgentSlotGuard: could not schedule lineage release on drop (no runtime); \
                 slot left to registry teardown"
            );
        }
    }
}

/// Optional capability trait, separate from [`LlmProvider`] (G4). A provider that
/// can be probed for its context window and weights digest implements it; one that
/// cannot simply does not, and rotation still works (a `None` digest is trusted).
///
/// The preflight ([`run_preflight`]) calls these once per model **before** dispatch
/// and caches the results in a [`ModelCapability`] map, so the pure rotation policy
/// reads them with zero I/O. Its only production impl is `OllamaProvider`
/// (feature `ollama`, filled in a later task).
#[async_trait::async_trait]
pub trait ProviderProbe: Send + Sync {
    /// Context window in tokens, or `None` if it cannot be measured.
    async fn window(&self) -> Result<Option<usize>, ProviderError>;
    /// Model weights fingerprint, or `None` if it cannot be resolved.
    async fn digest(&self) -> Result<Option<String>, ProviderError>;
}

/// Probe-derived capabilities of one model, cached by the preflight so the pure
/// rotation policy reads them with zero I/O.
#[derive(Debug, Clone)]
pub struct ModelCapability {
    /// Context window in tokens, `None` if unmeasured.
    pub window: Option<usize>,
    /// Weights fingerprint, `None` if unresolved (trusted by lineage — fail-open).
    pub digest: Option<String>,
    /// Always `true` for a probe-capable provider.
    pub supports_completion: bool,
}

/// Per-model probe timeout in the preflight. A probe that does not answer within
/// this is treated as unmeasured (window/digest `None`), never an abort (G3).
pub const DEFAULT_PREFLIGHT_TIMEOUT: Duration = Duration::from_secs(30);
/// Max probes hitting the shared endpoint at once (its agent cap is small).
const MAX_PREFLIGHT_CONCURRENCY: usize = 4;

/// Probes every `(model_id, probe)` pair CONCURRENTLY (bounded by
/// `MAX_PREFLIGHT_CONCURRENCY`) and returns `model_id -> ModelCapability`.
///
/// Each probe runs under [`DEFAULT_PREFLIGHT_TIMEOUT`]; a timeout or any probe
/// error degrades that model to `window: None, digest: None` (fail-open) rather
/// than aborting the preflight. A panicked probe task is skipped (no entry). The
/// bounded concurrency avoids both the serial N×timeout latency cliff and
/// overwhelming the shared endpoint's small agent cap.
pub async fn run_preflight(
    probes: Vec<(String, Arc<dyn ProviderProbe>)>,
) -> BTreeMap<String, ModelCapability> {
    let sem = Arc::new(tokio::sync::Semaphore::new(MAX_PREFLIGHT_CONCURRENCY));
    let mut set = tokio::task::JoinSet::new();

    for (model_id, probe) in probes {
        let id = model_id;
        let sem = Arc::clone(&sem);
        set.spawn(async move {
            let unmeasured = ModelCapability {
                window: None,
                digest: None,
                supports_completion: true,
            };
            let Ok(_permit) = sem.acquire_owned().await else {
                return (id, unmeasured);
            };
            let measured = tokio::time::timeout(DEFAULT_PREFLIGHT_TIMEOUT, async {
                let window = probe.window().await.ok().flatten();
                let digest = probe.digest().await.ok().flatten();
                (window, digest)
            })
            .await;
            let cap = match measured {
                Ok((window, digest)) => ModelCapability {
                    window,
                    digest,
                    supports_completion: true,
                },
                Err(_) => unmeasured,
            };
            (id, cap)
        });
    }

    let mut capabilities = BTreeMap::new();
    while let Some(joined) = set.join_next().await {
        if let Ok((id, cap)) = joined {
            capabilities.insert(id, cap);
        }
    }
    capabilities
}

/// Returns the first index pair `(i, j)` with `i < j` whose digests are BOTH
/// `Some(_)` and EQUAL — a proven weights collision. `None` is non-comparable and
/// never collides (not even with another `None`); returns `None` when no proven
/// collision exists. Pure, total.
///
/// The digest check in [`LineageRegistry::claim_next`] is **fail-open**: only a
/// `Some((0, _))` (the candidate, placed at index 0, matching a *resolvable*
/// active digest) rejects a rotation. An unresolvable (`None`) digest — on the
/// candidate or on an active mage — never collides, so it is trusted by the
/// declared lineage (R17, user decision 2026-07-26).
pub(crate) fn digest_collision(digests: &[Option<String>]) -> Option<(usize, usize)> {
    for (i, di) in digests.iter().enumerate() {
        let Some(di) = di else {
            continue;
        };
        for (j, dj) in digests.iter().enumerate().skip(i + 1) {
            if dj.as_deref() == Some(di.as_str()) {
                return Some((i, j));
            }
        }
    }
    None
}

/// Window pre-filter (R16): a KNOWN window passes iff it is at least `min_window`;
/// an UNKNOWN window (`None`) is eligible UNLESS `strict` (then rejected). Pure.
fn window_ok(window: Option<usize>, min_window: usize, strict: bool) -> bool {
    match window {
        Some(w) => w >= min_window,
        None => !strict,
    }
}

/// A fallback entry: the provider, its declared lineage, and an OPTIONAL probe
/// (present iff registered via `push_probing`).
pub struct FallbackCandidate {
    pub provider: Arc<dyn LlmProvider>,
    pub lineage: Lineage,
    pub probe: Option<Arc<dyn ProviderProbe>>,
}

/// Default per-mage rotation cap when the builder does not set one.
pub const DEFAULT_MAX_ROTATIONS: u32 = 2;

/// Immutable, encapsulated fallback pool shared run-wide (R3.1). Built via
/// [`FallbackPool::builder`]; construction is infallible (an empty pool is valid,
/// duplicate lineages only warn — G2).
pub struct FallbackPool {
    candidates: Vec<FallbackCandidate>,
    max_rotations: u32,
}

impl FallbackPool {
    /// Starts a [`FallbackPoolBuilder`] seeded with [`DEFAULT_MAX_ROTATIONS`].
    ///
    /// # Example
    ///
    /// ```
    /// use magi_core::rotation::FallbackPool;
    /// let pool = FallbackPool::builder().max_rotations(2).build();
    /// assert!(pool.is_empty()); // an empty pool is valid — it means "no rotation"
    /// ```
    pub fn builder() -> FallbackPoolBuilder {
        FallbackPoolBuilder {
            candidates: vec![],
            max_rotations: DEFAULT_MAX_ROTATIONS,
        }
    }
    /// Number of fallback candidates.
    pub fn len(&self) -> usize {
        self.candidates.len()
    }
    /// `true` iff the pool has no candidates (equivalent to "no rotation").
    pub fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }
    pub(crate) fn candidate(&self, i: usize) -> &FallbackCandidate {
        &self.candidates[i]
    }
    pub(crate) fn max_rotations(&self) -> u32 {
        self.max_rotations
    }
    pub(crate) fn candidates(&self) -> &[FallbackCandidate] {
        &self.candidates
    }
    /// Resolve the pool into the `Vec<Candidate>` fed to [`RotationPolicy::new`].
    /// `provider_ix` indexes into `candidates`, `model` comes from
    /// `provider.model()`. (No `is_cloud`: the digest verify is fail-open on an
    /// unresolvable digest, so provider kind is irrelevant to eligibility.)
    pub(crate) fn to_candidates(&self) -> Vec<Candidate> {
        self.candidates
            .iter()
            .enumerate()
            .map(|(i, fc)| Candidate {
                provider_ix: i,
                lineage: fc.lineage.clone(),
                model: fc.provider.model().to_string(),
            })
            .collect()
    }
}

/// Builder for [`FallbackPool`]. `build` is **infallible**.
pub struct FallbackPoolBuilder {
    candidates: Vec<FallbackCandidate>,
    max_rotations: u32,
}

impl FallbackPoolBuilder {
    /// Appends a non-probing fallback (no digest/window; trusted by lineage).
    pub fn push(mut self, provider: Arc<dyn LlmProvider>, lineage: Lineage) -> Self {
        self.candidates.push(FallbackCandidate {
            provider,
            lineage,
            probe: None,
        });
        self
    }
    /// Appends a probing fallback. `Arc<P>` is coerced to BOTH an
    /// `Arc<dyn LlmProvider>` and an `Arc<dyn ProviderProbe>` — the capability is
    /// DECLARED here, not discovered by downcast, so `LlmProvider` stays intact (G4).
    pub fn push_probing<P: LlmProvider + ProviderProbe + 'static>(
        mut self,
        p: Arc<P>,
        lineage: Lineage,
    ) -> Self {
        let provider: Arc<dyn LlmProvider> = p.clone();
        let probe: Arc<dyn ProviderProbe> = p;
        self.candidates.push(FallbackCandidate {
            provider,
            lineage,
            probe: Some(probe),
        });
        self
    }
    /// Sets the per-mage rotation cap (default [`DEFAULT_MAX_ROTATIONS`]).
    pub fn max_rotations(mut self, n: u32) -> Self {
        self.max_rotations = n;
        self
    }
    /// Builds the pool. Infallible: an empty pool is valid; duplicate lineages
    /// only emit a `tracing::warn!` (diversity never blocks the run — G2).
    pub fn build(self) -> FallbackPool {
        let mut seen = BTreeSet::new();
        for c in &self.candidates {
            if !seen.insert(c.lineage.clone()) {
                tracing::warn!(
                    lineage = c.lineage.as_str(),
                    "duplicate lineage in fallback pool (redundant, not fatal)"
                );
            }
        }
        FallbackPool {
            candidates: self.candidates,
            max_rotations: self.max_rotations,
        }
    }
}

/// Run-wide rotation configuration assembled by the builder: the trio's declared
/// primary lineages (+ any declared primary probes) and the shared fallback pool.
/// `None` on the orchestrator means rotation is disabled (2.0.x behavior).
pub(crate) struct RotationConfig {
    pub(crate) primary_lineages: BTreeMap<AgentName, Lineage>,
    pub(crate) primary_probes: BTreeMap<AgentName, Arc<dyn ProviderProbe>>,
    pub(crate) pool: FallbackPool,
    /// When set, a candidate whose context window cannot be measured is REJECTED
    /// by the window pre-filter (R16). Default `false` (unknown windows are
    /// eligible — the definitive probe decides later).
    pub(crate) strict_context_guard: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lineage_equality_and_display() {
        assert_eq!(Lineage::new("alibaba"), Lineage::from("alibaba"));
        assert_ne!(Lineage::new("alibaba"), Lineage::new("moonshot"));
        assert_eq!(format!("{}", Lineage::new("glm")), "glm");
        assert_eq!(Lineage::new("glm").as_str(), "glm");
    }

    #[test]
    fn test_lineage_from_static_is_borrowed_zero_alloc() {
        // A &'static str constructs a Borrowed Cow (no heap alloc).
        let l = Lineage::from("alibaba");
        assert!(matches!(l.0, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn test_lineage_from_owned_string_is_owned() {
        let s = String::from("run-time");
        assert!(matches!(Lineage::from(s).0, std::borrow::Cow::Owned(_)));
    }

    #[test]
    fn test_lineage_ord_for_btree_keys() {
        use std::collections::BTreeSet;
        let mut s = BTreeSet::new();
        s.insert(Lineage::new("b"));
        s.insert(Lineage::new("a"));
        assert_eq!(s.iter().next().unwrap().as_str(), "a"); // Ord works
    }

    #[test]
    fn test_lineage_new_trims_whitespace() {
        // R3.2 — constructors normalize.
        assert_eq!(Lineage::new(" alibaba ").as_str(), "alibaba");
        assert_eq!(Lineage::from("  glm\t").as_str(), "glm");
        assert_eq!(
            Lineage::from(String::from("\n moonshot ")).as_str(),
            "moonshot"
        );
        // A trimmed &'static str stays Borrowed (zero-alloc preserved).
        assert!(matches!(
            Lineage::new(" alibaba ").0,
            std::borrow::Cow::Borrowed(_)
        ));
        // Empty / all-whitespace trims to "" — a VALID Lineage value here; rejected at build (Task 7).
        assert_eq!(Lineage::new("   ").as_str(), "");
    }

    // ---- Task 2: RotationPolicy::next_model (4 pure conditions) ----

    fn pool(pairs: &[(&'static str, &str)]) -> Vec<Candidate> {
        pairs
            .iter()
            .enumerate()
            .map(|(i, (lin, m))| Candidate {
                provider_ix: i,
                lineage: Lineage::from(*lin),
                model: m.to_string(),
            })
            .collect()
    }
    fn empty() -> BTreeSet<Lineage> {
        BTreeSet::new()
    }
    fn empty_s() -> BTreeSet<String> {
        BTreeSet::new()
    }
    fn empty_wr() -> BTreeMap<String, &'static str> {
        BTreeMap::new()
    }

    #[test]
    fn test_next_model_returns_first_eligible_in_declared_order() {
        let p = RotationPolicy::for_tests_no_probe(pool(&[("a", "ma"), ("b", "mb")]), 5);
        assert_eq!(
            p.next_model(&empty(), &empty(), &empty(), &empty_s(), &empty_wr(), 0)
                .unwrap()
                .lineage
                .as_str(),
            "a"
        );
    }

    #[test]
    fn test_next_model_skips_in_play_failed_runfailed_used_and_windowrejected() {
        let p = RotationPolicy::for_tests_no_probe(
            pool(&[("a", "ma"), ("b", "mb"), ("c", "mc"), ("d", "md")]),
            5,
        );
        let in_play: BTreeSet<_> = [Lineage::from("a")].into();
        let failed: BTreeSet<_> = [Lineage::from("b")].into();
        let runf: BTreeSet<_> = [Lineage::from("c")].into();
        // a in_play, b mage-failed, c run-failed → first eligible = d
        assert_eq!(
            p.next_model(&failed, &runf, &in_play, &empty_s(), &empty_wr(), 0)
                .unwrap()
                .lineage
                .as_str(),
            "d"
        );
        // 'md' in window_rejected (cond #5) → d skipped → None (re-propose loop can terminate, W12)
        let wr: BTreeMap<String, &'static str> = [("md".to_string(), "digest_collision")].into();
        assert!(
            p.next_model(&failed, &runf, &in_play, &empty_s(), &wr, 0)
                .is_none()
        );
        // 'md' in used → None
        let used2: BTreeSet<_> = ["md".to_string()].into();
        assert!(
            p.next_model(&failed, &runf, &in_play, &used2, &empty_wr(), 0)
                .is_none()
        );
    }

    #[test]
    fn test_max_rotations_gate() {
        let p = RotationPolicy::for_tests_no_probe(pool(&[("a", "ma")]), 2);
        assert!(
            p.next_model(&empty(), &empty(), &empty(), &empty_s(), &empty_wr(), 2)
                .is_none()
        ); // done==max
        assert!(
            p.next_model(&empty(), &empty(), &empty(), &empty_s(), &empty_wr(), 1)
                .is_some()
        );
        let p0 = RotationPolicy::for_tests_no_probe(pool(&[("a", "ma")]), 0); // 0 disables
        assert!(
            p0.next_model(&empty(), &empty(), &empty(), &empty_s(), &empty_wr(), 0)
                .is_none()
        );
    }

    #[test]
    fn test_empty_pool_and_all_ineligible_return_none() {
        assert!(
            RotationPolicy::for_tests_no_probe(vec![], 3)
                .next_model(&empty(), &empty(), &empty(), &empty_s(), &empty_wr(), 0)
                .is_none()
        );
    }

    #[test]
    fn test_next_model_is_deterministic() {
        let p = RotationPolicy::for_tests_no_probe(pool(&[("a", "ma"), ("b", "mb")]), 5);
        let a = p
            .next_model(&empty(), &empty(), &empty(), &empty_s(), &empty_wr(), 0)
            .unwrap()
            .lineage
            .clone();
        let b = p
            .next_model(&empty(), &empty(), &empty(), &empty_s(), &empty_wr(), 0)
            .unwrap()
            .lineage
            .clone();
        assert_eq!(a, b);
    }

    // ---- Task 3: LineageRegistry (state, in_play, latch, release) ----

    use std::sync::Arc;

    fn ae(lin: &'static str, model: &str) -> ActiveEntry {
        ActiveEntry {
            lineage: Lineage::from(lin),
            model: model.into(),
        }
    }
    fn reg() -> Arc<LineageRegistry> {
        let init: BTreeMap<_, _> = [
            (AgentName::Melchior, ae("a", "ma")),
            (AgentName::Balthasar, ae("b", "mb")),
            (AgentName::Caspar, ae("c", "mc")),
        ]
        .into();
        Arc::new(LineageRegistry::new(init))
    }

    #[tokio::test]
    async fn test_in_play_excludes_self() {
        let r = reg();
        let ip = r.lineages_in_play(AgentName::Melchior).await;
        assert!(!ip.contains(&Lineage::from("a"))); // self excluded
        assert!(ip.contains(&Lineage::from("b")) && ip.contains(&Lineage::from("c")));
    }

    #[tokio::test]
    async fn test_register_transport_condemns_run_wide() {
        let r = reg();
        let _ = r
            .register_transport_failure(Lineage::from("a"), false)
            .await;
        assert!(r.run_failed_lineages().await.contains(&Lineage::from("a")));
    }

    #[tokio::test]
    async fn test_endpoint_down_latch_exactly_one_true_concurrent() {
        let r = reg();
        // Two DISTINCT connection lineages failing concurrently → exactly ONE true.
        let (r1, r2) = (r.clone(), r.clone());
        let h1 = tokio::spawn(async move {
            r1.register_transport_failure(Lineage::from("a"), true)
                .await
        });
        let h2 = tokio::spawn(async move {
            r2.register_transport_failure(Lineage::from("b"), true)
                .await
        });
        let trues = [h1.await.unwrap(), h2.await.unwrap()]
            .iter()
            .filter(|b| **b)
            .count();
        assert_eq!(trues, 1, "latch must fire exactly once at threshold");
    }

    #[tokio::test]
    async fn test_5xx_does_not_count_toward_endpoint_down() {
        let r = reg();
        // connection=false (5xx/timeout) → condemns but NEVER returns true
        assert!(
            !r.register_transport_failure(Lineage::from("a"), false)
                .await
        );
        assert!(
            !r.register_transport_failure(Lineage::from("b"), false)
                .await
        );
    }

    #[tokio::test]
    async fn test_release_is_idempotent() {
        let r = reg();
        r.release(AgentName::Melchior).await;
        r.release(AgentName::Melchior).await; // no panic, no error
        assert!(
            !r.lineages_in_play(AgentName::Balthasar)
                .await
                .contains(&Lineage::from("a"))
        );
    }

    // ---- Task 4: claim_next + AgentSlotGuard ----

    fn state(configured: &str) -> AgentRotationState {
        AgentRotationState {
            model_configured: configured.into(),
            model_used: configured.into(),
            chain: vec![],
            used: [configured.to_string()].into(),
            failed_lineages: BTreeSet::new(),
            window_rejected: BTreeMap::new(),
            rotations_done: 0,
            succeeded: false,
            ran_unmeasured: false,
        }
    }
    fn policy() -> RotationPolicy {
        RotationPolicy::for_tests_no_probe(pool(&[("d", "md"), ("e", "me")]), 3)
    }

    #[tokio::test]
    async fn test_claim_next_reserves_and_replaces_active() {
        let r = reg();
        let p = policy();
        let mut s = state("mc");
        let c = r.claim_next(AgentName::Caspar, &p, &mut s).await.unwrap();
        assert_eq!(c.lineage.as_str(), "d");
        assert!(
            r.lineages_in_play(AgentName::Melchior)
                .await
                .contains(&Lineage::from("d"))
        );
    }

    #[tokio::test]
    async fn test_claim_next_none_leaves_registry_intact() {
        let init: BTreeMap<_, _> = [(AgentName::Caspar, ae("d", "md"))].into();
        let r = Arc::new(LineageRegistry::new(init));
        let p = RotationPolicy::for_tests_no_probe(pool(&[("d", "md")]), 3);
        let mut s = state("md");
        assert!(
            r.claim_next(AgentName::Melchior, &p, &mut s)
                .await
                .is_none()
        );
        // Registry intact: Caspar still holds "d". Query with a DIFFERENT exclude
        // (Melchior) so Caspar's lineage is visible (`in_play` excludes its argument).
        assert!(
            r.lineages_in_play(AgentName::Melchior)
                .await
                .contains(&Lineage::from("d"))
        ); // intact
    }

    #[tokio::test]
    async fn test_two_mages_same_free_lineage_exactly_one_reserves() {
        // S9
        let init: BTreeMap<_, _> = [(AgentName::Melchior, ae("x", "mx"))].into();
        let r = Arc::new(LineageRegistry::new(init));
        let mk = || RotationPolicy::for_tests_no_probe(pool(&[("d", "md")]), 3);
        let (r1, r2) = (r.clone(), r.clone());
        let h1 = tokio::spawn(async move {
            let mut s = state("b");
            r1.claim_next(AgentName::Balthasar, &mk(), &mut s)
                .await
                .map(|c| c.lineage)
        });
        let h2 = tokio::spawn(async move {
            let mut s = state("c");
            r2.claim_next(AgentName::Caspar, &mk(), &mut s)
                .await
                .map(|c| c.lineage)
        });
        let got: Vec<_> = [h1.await.unwrap(), h2.await.unwrap()]
            .into_iter()
            .flatten()
            .collect();
        assert_eq!(
            got.len(),
            1,
            "exactly one mage reserves the single free lineage"
        );
    }

    #[tokio::test]
    async fn test_concurrent_claims_never_double_reserve_stress() {
        // W7 (loop6) — 200× two-mage concurrent claim over TWO free lineages; distinct, no deadlock.
        for _ in 0..200 {
            let init: BTreeMap<_, _> = [(AgentName::Melchior, ae("x", "mx"))].into();
            let r = Arc::new(LineageRegistry::new(init));
            let mk = || RotationPolicy::for_tests_no_probe(pool(&[("d", "md"), ("e", "me")]), 3);
            let (r1, r2) = (r.clone(), r.clone());
            let h1 = tokio::spawn(async move {
                let mut s = state("b");
                r1.claim_next(AgentName::Balthasar, &mk(), &mut s)
                    .await
                    .map(|c| c.lineage)
            });
            let h2 = tokio::spawn(async move {
                let mut s = state("c");
                r2.claim_next(AgentName::Caspar, &mk(), &mut s)
                    .await
                    .map(|c| c.lineage)
            });
            let got: Vec<_> = [h1.await.unwrap(), h2.await.unwrap()]
                .into_iter()
                .flatten()
                .collect();
            assert_eq!(got.len(), 2, "both reserve (two free lineages)");
            assert_ne!(
                got[0], got[1],
                "concurrent claims must NEVER double-reserve the same lineage"
            );
        }
    }

    #[tokio::test]
    async fn test_slot_guard_releases_on_drop_unless_succeeded() {
        // S12 / W5
        let r = reg();
        {
            let mut g = AgentSlotGuard::new(r.clone(), AgentName::Melchior);
            g.mark_succeeded();
        } // drop with succeeded → NOT released
        assert!(
            r.lineages_in_play(AgentName::Balthasar)
                .await
                .contains(&Lineage::from("a"))
        );
        {
            let _g = AgentSlotGuard::new(r.clone(), AgentName::Melchior);
        } // drop without success → released
        assert!(
            !r.lineages_in_play(AgentName::Balthasar)
                .await
                .contains(&Lineage::from("a"))
        );
    }

    #[tokio::test]
    async fn test_slot_guard_releases_on_cancellation() {
        // W5 — cancellation cleanup, deterministic sync via oneshot READY.
        let r = reg();
        let r2 = r.clone();
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            let _g = AgentSlotGuard::new(r2, AgentName::Melchior); // never mark_succeeded
            let _ = ready_tx.send(());
            std::future::pending::<()>().await; // park until aborted (guard still held)
        });
        ready_rx
            .await
            .expect("task must reach the park point with the guard held");
        handle.abort();
        let _ = handle.await;
        assert!(
            !r.lineages_in_play(AgentName::Balthasar)
                .await
                .contains(&Lineage::from("a")),
            "cancellation must release the slot via the guard's Drop"
        );
    }

    #[tokio::test]
    async fn test_slot_guard_contended_drop_releases_via_spawn() {
        // W2 (loop7) — CONTENDED Drop: hold the lock while the guard drops → detached-spawn path.
        let r = reg();
        let held = r.lock.lock().await; // hold the mutex → guard.try_lock() fails
        {
            let _g = AgentSlotGuard::new(r.clone(), AgentName::Melchior);
        } // drop under contention → schedules a detached release task
        drop(held);
        let mut released = false;
        for _ in 0..100 {
            if !r
                .lineages_in_play(AgentName::Balthasar)
                .await
                .contains(&Lineage::from("a"))
            {
                released = true;
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(
            released,
            "contended Drop must release the slot via the detached spawn once the lock frees"
        );
    }

    #[tokio::test]
    async fn test_slot_guard_mark_released_suppresses_drop() {
        // `mark_released` signals the slot was already released explicitly → Drop skips.
        let r = reg();
        {
            let mut g = AgentSlotGuard::new(r.clone(), AgentName::Melchior);
            g.mark_released();
        } // drop is a no-op (released) — we did NOT actually release here, so "a" stays in play
        assert!(
            r.lineages_in_play(AgentName::Balthasar)
                .await
                .contains(&Lineage::from("a"))
        );
    }

    // ---- Task 5: FallbackPool builder ----

    use crate::error::ProviderError;
    use crate::provider::{CompletionConfig, LlmProvider};

    struct MockProvider {
        name: String,
        model: String,
        resp: String,
    }
    impl MockProvider {
        fn new(name: &str, model: &str, resp: &str) -> Self {
            Self {
                name: name.into(),
                model: model.into(),
                resp: resp.into(),
            }
        }
    }
    #[async_trait::async_trait]
    impl LlmProvider for MockProvider {
        async fn complete(
            &self,
            _s: &str,
            _u: &str,
            _c: &CompletionConfig,
        ) -> Result<String, ProviderError> {
            Ok(self.resp.clone())
        }
        fn name(&self) -> &str {
            &self.name
        }
        fn model(&self) -> &str {
            &self.model
        }
    }

    /// A mock that is BOTH an `LlmProvider` and a `ProviderProbe`.
    struct MockProbe {
        model: String,
    }
    impl MockProbe {
        fn new(model: &str) -> Self {
            Self {
                model: model.into(),
            }
        }
    }
    #[async_trait::async_trait]
    impl LlmProvider for MockProbe {
        async fn complete(
            &self,
            _s: &str,
            _u: &str,
            _c: &CompletionConfig,
        ) -> Result<String, ProviderError> {
            Ok(String::new())
        }
        fn name(&self) -> &str {
            "mock-probe"
        }
        fn model(&self) -> &str {
            &self.model
        }
    }
    #[async_trait::async_trait]
    impl ProviderProbe for MockProbe {
        async fn window(&self) -> Result<Option<usize>, ProviderError> {
            Ok(Some(200_000))
        }
        async fn digest(&self) -> Result<Option<String>, ProviderError> {
            Ok(Some(format!("sha:{}", self.model)))
        }
    }

    #[test]
    fn test_empty_pool_is_valid_and_keeps_max_rotations() {
        let p = FallbackPool::builder().max_rotations(0).build();
        assert_eq!(p.len(), 0);
        assert!(p.is_empty());
        assert_eq!(p.max_rotations(), 0);
    }

    #[test]
    fn test_push_probing_stores_both_views() {
        let mp = Arc::new(MockProbe::new("m1"));
        let pool = FallbackPool::builder()
            .push_probing(mp, Lineage::new("ollama"))
            .build();
        assert!(pool.candidate(0).probe.is_some()); // probe view stored
        assert_eq!(pool.candidate(0).provider.model(), "m1"); // llm view usable
    }

    #[test]
    fn test_push_without_probe_has_none() {
        let pool = FallbackPool::builder()
            .push(
                Arc::new(MockProvider::new("c", "m1", "r")),
                Lineage::new("cloud"),
            )
            .build();
        assert!(pool.candidate(0).probe.is_none());
    }

    #[test]
    fn test_duplicate_lineage_warns_but_builds() {
        // duplicate lineage → build still succeeds (WARNING only, G2). Assert len=2.
        let pool = FallbackPool::builder()
            .push(
                Arc::new(MockProvider::new("a", "m1", "r")),
                Lineage::new("dup"),
            )
            .push(
                Arc::new(MockProvider::new("b", "m2", "r")),
                Lineage::new("dup"),
            )
            .build();
        assert_eq!(pool.len(), 2);
    }

    #[test]
    fn test_to_candidates_resolves_ix_and_model() {
        let pool = FallbackPool::builder()
            .push(
                Arc::new(MockProvider::new("a", "ma", "r")),
                Lineage::new("x"),
            )
            .push(
                Arc::new(MockProvider::new("b", "mb", "r")),
                Lineage::new("y"),
            )
            .build();
        assert_eq!(pool.candidates().len(), 2);
        let cands = pool.to_candidates();
        assert_eq!(cands[0].provider_ix, 0);
        assert_eq!(cands[0].model, "ma");
        assert_eq!(cands[0].lineage.as_str(), "x");
        assert_eq!(cands[1].provider_ix, 1);
        assert_eq!(cands[1].model, "mb");
    }

    // ---- Task 6: telemetry types ----

    #[test]
    fn test_rotation_kind_display() {
        assert_eq!(RotationKind::Transport.to_string(), "transport");
        assert_eq!(RotationKind::Schema.to_string(), "schema");
        assert_eq!(RotationKind::Timeout.to_string(), "timeout");
    }

    #[test]
    fn test_agent_rotation_serializes_with_chain() {
        let ar = AgentRotation {
            model_configured: "mc".into(),
            model_used: "mc".into(),
            chain: vec![],
            ran_unmeasured: false,
        };
        let j = serde_json::to_string(&ar).unwrap();
        assert!(j.contains("model_used") && j.contains("chain"));
    }

    #[test]
    fn test_state_to_rotation_preserves_empty_chain() {
        // Panicked/first-try agent → present, chain-empty record.
        let mut s = state("deepseek");
        s.succeeded = true;
        let ar = s.to_rotation();
        assert_eq!(ar.model_configured, "deepseek");
        assert_eq!(ar.model_used, "deepseek");
        assert!(ar.chain.is_empty());
    }

    #[test]
    fn test_rotation_event_new_sanitizes_detail() {
        // A zero-width char in `detail` is stripped by clean_title.
        let ev = RotationEvent::new(
            Lineage::from("a"),
            Lineage::from("b"),
            "mb".into(),
            RotationKind::Transport,
            "http\u{200B}503".into(),
        );
        assert_eq!(ev.detail(), "http503");
    }

    #[test]
    fn test_rotation_event_new_truncates_long_detail() {
        // A 10 000-char detail comes out ≤256 chars, on a char boundary (no panic).
        let long = "x".repeat(10_000);
        let ev = RotationEvent::new(
            Lineage::from("a"),
            Lineage::from("b"),
            "mb".into(),
            RotationKind::Transport,
            long,
        );
        assert!(ev.detail().chars().count() <= 256);
    }

    // ---- Task 11: run_preflight (concurrent, timeout, fail-open) ----

    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A probe whose calls always error — exercises the fail-open degradation.
    struct FailingProbe;
    #[async_trait::async_trait]
    impl ProviderProbe for FailingProbe {
        async fn window(&self) -> Result<Option<usize>, ProviderError> {
            Err(ProviderError::Network {
                message: "down".into(),
            })
        }
        async fn digest(&self) -> Result<Option<String>, ProviderError> {
            Err(ProviderError::Network {
                message: "down".into(),
            })
        }
    }

    /// A probe that records the maximum number of concurrently in-flight probes
    /// via a shared counter + a rendezvous barrier (deterministic, no timing).
    struct OverlapProbe {
        inflight: Arc<AtomicUsize>,
        max_overlap: Arc<AtomicUsize>,
        barrier: Arc<tokio::sync::Barrier>,
    }
    #[async_trait::async_trait]
    impl ProviderProbe for OverlapProbe {
        async fn window(&self) -> Result<Option<usize>, ProviderError> {
            let cur = self.inflight.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_overlap.fetch_max(cur, Ordering::SeqCst);
            self.barrier.wait().await; // all probes rendezvous → guaranteed overlap
            self.inflight.fetch_sub(1, Ordering::SeqCst);
            Ok(Some(8000))
        }
        async fn digest(&self) -> Result<Option<String>, ProviderError> {
            Ok(Some("d".into()))
        }
    }

    #[tokio::test]
    async fn test_preflight_builds_capabilities_from_probes() {
        let probe: Arc<dyn ProviderProbe> = Arc::new(MockProbe::new("m1"));
        let caps = run_preflight(vec![("m1".to_string(), probe)]).await;
        let c = caps.get("m1").expect("m1 capability present");
        assert_eq!(c.window, Some(200_000));
        assert_eq!(c.digest.as_deref(), Some("sha:m1"));
        assert!(c.supports_completion);
    }

    #[tokio::test]
    async fn test_probe_failure_yields_none_and_does_not_abort() {
        // S24 — a probe error degrades to unmeasured (None/None); preflight returns.
        let probe: Arc<dyn ProviderProbe> = Arc::new(FailingProbe);
        let caps = run_preflight(vec![("bad".to_string(), probe)]).await;
        let c = caps
            .get("bad")
            .expect("entry present even on probe failure");
        assert_eq!(c.window, None);
        assert_eq!(c.digest, None);
    }

    #[tokio::test]
    async fn test_probes_run_concurrently_via_overlap_counter() {
        // Overlap-counter, NOT wall-clock (CI-stable): two probes rendezvous at a
        // barrier, so both are in-flight at once → observed overlap >= 2.
        let inflight = Arc::new(AtomicUsize::new(0));
        let max_overlap = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        let mk = |m: &str| -> (String, Arc<dyn ProviderProbe>) {
            (
                m.to_string(),
                Arc::new(OverlapProbe {
                    inflight: Arc::clone(&inflight),
                    max_overlap: Arc::clone(&max_overlap),
                    barrier: Arc::clone(&barrier),
                }) as Arc<dyn ProviderProbe>,
            )
        };
        let _ = run_preflight(vec![mk("m1"), mk("m2")]).await;
        assert!(
            max_overlap.load(Ordering::SeqCst) >= 2,
            "preflight must run probes concurrently (observed overlap >= 2)"
        );
    }

    // ---- Task 12: window pre-filter + digest R5a (fail-open) ----

    fn cap(digest: Option<&str>) -> ModelCapability {
        ModelCapability {
            window: None,
            digest: digest.map(String::from),
            supports_completion: true,
        }
    }
    fn caps_map(pairs: &[(&str, Option<&str>)]) -> BTreeMap<String, ModelCapability> {
        pairs
            .iter()
            .map(|(m, d)| (m.to_string(), cap(*d)))
            .collect()
    }

    /// Registry with ONE active mage (Melchior), a policy carrying `caps`, and a
    /// fresh Caspar state — Caspar is the mage rotating.
    fn digest_case(
        active_model: &str,
        active_digest: Option<&str>,
        cand: &[(&'static str, &str)],
        caps: Vec<(&str, Option<&str>)>,
    ) -> (Arc<LineageRegistry>, RotationPolicy, AgentRotationState) {
        let init: BTreeMap<_, _> = [(
            AgentName::Melchior,
            ActiveEntry {
                lineage: Lineage::from("mel-lin"),
                model: active_model.into(),
            },
        )]
        .into();
        let mut cm = caps_map(&caps);
        cm.insert(active_model.to_string(), cap(active_digest));
        let policy = RotationPolicy::new(pool(cand), 3, cm, false, 0);
        (Arc::new(LineageRegistry::new(init)), policy, state("cas"))
    }

    /// Registry with TWO active mages (Melchior, Balthasar) — for the dynamic-R5a
    /// re-evaluation test (W1).
    fn digest_case_two_active(
        mel: &str,
        mel_d: Option<&str>,
        bal: &str,
        bal_d: Option<&str>,
        cand: &[(&'static str, &str)],
        caps: Vec<(&str, Option<&str>)>,
    ) -> (Arc<LineageRegistry>, RotationPolicy, AgentRotationState) {
        let init: BTreeMap<_, _> = [
            (
                AgentName::Melchior,
                ActiveEntry {
                    lineage: Lineage::from("mel-lin"),
                    model: mel.into(),
                },
            ),
            (
                AgentName::Balthasar,
                ActiveEntry {
                    lineage: Lineage::from("bal-lin"),
                    model: bal.into(),
                },
            ),
        ]
        .into();
        let mut cm = caps_map(&caps);
        cm.insert(mel.to_string(), cap(mel_d));
        cm.insert(bal.to_string(), cap(bal_d));
        let policy = RotationPolicy::new(pool(cand), 3, cm, false, 0);
        (Arc::new(LineageRegistry::new(init)), policy, state("cas"))
    }

    /// Registry where the CALLING agent (Caspar) is itself active with `self_digest`
    /// — for the calling-agent exclusion test (W6).
    fn digest_case_self(
        self_digest: &str,
        cand: &[(&'static str, &str)],
        caps: Vec<(&str, Option<&str>)>,
    ) -> (Arc<LineageRegistry>, RotationPolicy, AgentRotationState) {
        let self_model = "cas-self";
        let init: BTreeMap<_, _> = [(
            AgentName::Caspar,
            ActiveEntry {
                lineage: Lineage::from("cas-lin"),
                model: self_model.into(),
            },
        )]
        .into();
        let mut cm = caps_map(&caps);
        cm.insert(self_model.to_string(), cap(Some(self_digest)));
        let policy = RotationPolicy::new(pool(cand), 3, cm, false, 0);
        (
            Arc::new(LineageRegistry::new(init)),
            policy,
            state("cas-other"),
        )
    }

    #[test]
    fn test_digest_collision_table_including_none() {
        assert_eq!(
            digest_collision(&[Some("a".into()), Some("b".into()), Some("a".into())]),
            Some((0, 2))
        );
        assert_eq!(digest_collision(&[None, None]), None); // None never collides
        assert_eq!(
            digest_collision(&[Some("a".into()), None, Some("b".into())]),
            None
        );
        assert_eq!(digest_collision(&[]), None);
        assert_eq!(
            digest_collision(&[Some("a".into()), Some("a".into())]),
            Some((0, 1))
        );
    }

    #[test]
    fn test_window_ok_rejects_too_small_and_unknown_eligible_unless_strict() {
        // S20
        assert!(!window_ok(Some(100), 1000, false)); // too small
        assert!(window_ok(None, 1000, false)); // unknown → eligible (non-strict)
        assert!(!window_ok(None, 1000, true)); // unknown → rejected (strict)
        assert!(window_ok(Some(2000), 1000, true)); // big enough
    }

    #[tokio::test]
    async fn test_claim_next_rejects_proven_digest_collision_and_reproposes() {
        // S18 — R5a proven collision is rejected; the next unique candidate reserved.
        let (r, p, mut s) = digest_case(
            "mx",
            Some("sha:a"),
            &[("d", "md"), ("e", "me")],
            vec![("md", Some("sha:a")), ("me", Some("sha:b"))],
        );
        let got = r.claim_next(AgentName::Caspar, &p, &mut s).await.unwrap();
        assert_eq!(got.model, "me"); // d (proven collision) rejected, e reserved
        assert!(s.window_rejected.contains_key("md"));
    }

    #[tokio::test]
    async fn test_candidate_without_digest_is_accepted_trusting_lineage() {
        // S19 — an unverifiable candidate digest is trusted (fail-open), not rejected.
        let (r, p, mut s) = digest_case("mx", Some("sha:a"), &[("d", "md")], vec![("md", None)]);
        assert!(
            r.claim_next(AgentName::Caspar, &p, &mut s).await.is_some(),
            "an unverifiable candidate digest is trusted, not rejected"
        );
    }

    #[tokio::test]
    async fn test_active_unverifiable_digest_does_not_block_rotation() {
        // S19b (was R5c×G3) — an ACTIVE mage's None digest must not block rotation.
        let (r, p, mut s) = digest_case("mx", None, &[("d", "md")], vec![("md", Some("sha:b"))]);
        assert!(
            r.claim_next(AgentName::Caspar, &p, &mut s).await.is_some(),
            "unverifiable ACTIVE digest is trusted; rotation proceeds"
        );
    }

    #[tokio::test]
    async fn test_window_rejected_cleared_so_dynamic_r5a_reevaluated() {
        // W1 — d collides with Melchior (sha:x) in call 1 → e reserved. Melchior
        // departs, e spent → call 2 must RE-EVALUATE d (no longer colliding).
        let (r, p, mut s) = digest_case_two_active(
            "mm",
            Some("sha:x"),
            "mb",
            Some("sha:y"),
            &[("d", "md"), ("e", "me")],
            vec![("md", Some("sha:x")), ("me", Some("sha:e"))],
        );
        let first = r.claim_next(AgentName::Caspar, &p, &mut s).await.unwrap();
        assert_eq!(first.model, "me"); // d rejected (R5a), e reserved
        r.release(AgentName::Melchior).await; // colliding mage departs
        s.used.insert("me".into());
        s.rotations_done += 1; // e spent; next rotation
        let second = r.claim_next(AgentName::Caspar, &p, &mut s).await;
        assert_eq!(
            second.map(|c| c.model),
            Some("md".into()),
            "d must be re-eligible once the colliding mage departed"
        );
    }

    #[tokio::test]
    async fn test_calling_agent_excluded_from_digest_check() {
        // W6 — Caspar's OWN active digest equals its candidate's → must NOT self-collide.
        let (r, p, mut s) =
            digest_case_self("sha:self", &[("d", "md")], vec![("md", Some("sha:self"))]);
        assert!(
            r.claim_next(AgentName::Caspar, &p, &mut s).await.is_some(),
            "calling agent excluded → no spurious self-collision"
        );
    }
}
