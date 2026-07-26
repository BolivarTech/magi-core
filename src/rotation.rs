// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-07-26

//! Per-agent lineage rotation for MS2.
//!
//! (Module documentation is expanded in Task 15; this file starts with the
//! [`Lineage`] newtype — a declared, never-inferred model-family label.)

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use tokio::sync::Mutex;

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
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
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
}

impl RotationPolicy {
    /// Builds a policy over an ordered fallback list and a per-mage rotation cap
    /// (`max_rotations = 0` disables rotation entirely).
    pub fn new(fallback: Vec<Candidate>, max_rotations: u32) -> Self {
        Self {
            fallback,
            max_rotations,
        }
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
        // core; #5 (`window_rejected`) is empty without a probe (no-op) but honored
        // so a later re-propose loop cannot spin. Condition #6 (window) is added
        // with `capabilities` in a later task.
        self.fallback.iter().find(|c| {
            !lineages_in_play.contains(&c.lineage)          // 1: another live mage has this lineage
                && !failed_lineages.contains(&c.lineage)    // 2: this mage schema-failed it
                && !run_failed_lineages.contains(&c.lineage) // 3: condemned run-wide (transport)
                && !used.contains(&c.model)                 // 4: this mage already ran this model
                && !window_rejected.contains_key(&c.model) // 5: rejected by window/digest verify
        })
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

/// A single completed rotation hop. Fields and the sanitizing constructor are
/// added in a later task; here it is a placeholder so [`AgentRotationState`]
/// compiles (an empty `chain` is the common case).
#[derive(Clone)]
pub struct RotationEvent {}

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
    /// is cleared at entry so a dynamic rejection is re-evaluated on the next call.
    /// The digest re-propose loop (fail-open, R5a) replaces this body in a later
    /// task with the same signature and postcondition.
    pub async fn claim_next(
        &self,
        agent: AgentName,
        policy: &RotationPolicy,
        state: &mut AgentRotationState,
    ) -> Option<Candidate> {
        let mut g = self.lock.lock().await;
        state.window_rejected.clear();
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
        g.active.insert(
            agent,
            ActiveEntry {
                lineage: chosen.lineage.clone(),
                model: chosen.model.clone(),
            },
        );
        Some(chosen)
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
// Forward reference: exercised by the tests below and CONSUMED by
// `dispatch_one_agent` (Task 8). Until that caller lands, the non-test lib build
// sees it as unused — this `allow` documents that and is REMOVED in Task 8.
#[allow(dead_code)]
pub(crate) struct AgentSlotGuard {
    reg: Arc<LineageRegistry>,
    agent: AgentName,
    succeeded: bool,
    released: bool,
}
#[allow(dead_code)]
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
        let p = RotationPolicy::new(pool(&[("a", "ma"), ("b", "mb")]), 5);
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
        let p = RotationPolicy::new(
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
        let p = RotationPolicy::new(pool(&[("a", "ma")]), 2);
        assert!(
            p.next_model(&empty(), &empty(), &empty(), &empty_s(), &empty_wr(), 2)
                .is_none()
        ); // done==max
        assert!(
            p.next_model(&empty(), &empty(), &empty(), &empty_s(), &empty_wr(), 1)
                .is_some()
        );
        let p0 = RotationPolicy::new(pool(&[("a", "ma")]), 0); // 0 disables
        assert!(
            p0.next_model(&empty(), &empty(), &empty(), &empty_s(), &empty_wr(), 0)
                .is_none()
        );
    }

    #[test]
    fn test_empty_pool_and_all_ineligible_return_none() {
        assert!(
            RotationPolicy::new(vec![], 3)
                .next_model(&empty(), &empty(), &empty(), &empty_s(), &empty_wr(), 0)
                .is_none()
        );
    }

    #[test]
    fn test_next_model_is_deterministic() {
        let p = RotationPolicy::new(pool(&[("a", "ma"), ("b", "mb")]), 5);
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
        RotationPolicy::new(pool(&[("d", "md"), ("e", "me")]), 3)
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
        let p = RotationPolicy::new(pool(&[("d", "md")]), 3);
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
        let mk = || RotationPolicy::new(pool(&[("d", "md")]), 3);
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
            let mk = || RotationPolicy::new(pool(&[("d", "md"), ("e", "me")]), 3);
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
}
