// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-18

//! The fail-closed preflight: everything that must be true BEFORE a single
//! scenario runs, in R26's fixed order — config, fixtures, workspace, lock,
//! backend, probe, proxy, cost.
//!
//! **Every early return here is exit 2.** A preflight failure means "we could
//! not test", never "the crate failed" — that distinction is the whole reason
//! this module exists as its own gate ahead of the scenarios, instead of
//! letting the first scenario discover a broken corpus or an absent backend
//! after it has already spent tokens finding out.
//!
//! # Deviations from the brief, found while making it compile
//!
//! 1. The brief's `Step 3` pseudo-code calls `.map_err(PreflightError::cannot_test)`
//!    with a single argument at several sites, but `cannot_test` is declared
//!    (and used elsewhere in the same brief) with TWO parameters, `(stage, msg)`.
//!    A bare function reference cannot supply the `stage` half, so every such
//!    call below is a closure that names its own [`Stage`] explicitly.
//! 2. [`check_workspace_isolation`]'s every error path is prefixed with
//!    `workspace_root:` — including the one where `cargo metadata` never runs
//!    at all because the directory does not exist. Without the prefix, that
//!    spawn failure would surface as a raw OS error with no mention of the
//!    property being checked, and the reader would have to already know which
//!    check produced it.
//! 3. [`probe_failure_message`] says "a cold model" in lowercase. The brief's
//!    listing capitalised it ("COLD"), which would not satisfy its own test
//!    (`msg.contains("cold")` is case-sensitive).

use crate::config::{Config, RunId, PROBE_RETRY_FACTOR};
use crate::fixtures;
use crate::paths::{fixture_dir, repo_root, smoke_dir};
use crate::proxy::SpyProxy;
use crate::runner::stage_e1_run_ids;
use std::path::Path;
use std::time::Duration;

/// Everything the preflight leaves behind for the rest of the run.
pub struct Announcement {
    /// The proxy, already listening. Owned here because R9 requires it to
    /// exist before any run does.
    pub proxy: SpyProxy,
    /// Printed BEFORE the first run (R31): estimated tokens and expected time.
    pub cost_announcement: String,
    /// R31's other half, which had no implementation anywhere: the ledger that
    /// records what the runs ACTUALLY cost once they are done.
    ///
    /// It travels from here rather than being created in `main` because the
    /// announcement is made here, and the ledger is what remembers that it was:
    /// [`CostLedger::record`] refuses without it, which makes "announced before,
    /// recorded after" structural instead of a convention about where two prints
    /// sit. The interval it reports is NOT opened here — see
    /// [`CostLedger::measure`].
    pub ledger: CostLedger,
    /// What the fixture audit counted (R23), on its way to the end-of-run
    /// report and the certificate.
    ///
    /// The audit is computed HERE and was dropped here: nothing read
    /// `unverified` outside `fixtures.rs`, so the count R23 asks for and the
    /// warning it feeds both went missing. This field is the connection that
    /// did not exist.
    pub fixtures: fixtures::FixtureSummary,
}

/// Hand-written rather than derived: [`SpyProxy`] itself does not implement
/// `Debug` (Task 4's scope). `S20`'s test needs `Result::unwrap_err`, which
/// requires the `Ok` side to be `Debug`, so this exists to satisfy that
/// without reaching into Task 4's module to add a derive it never asked for.
impl std::fmt::Debug for Announcement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Announcement")
            .field("proxy", &"SpyProxy { .. }")
            .field("cost_announcement", &self.cost_announcement)
            .field("ledger", &"CostLedger { .. }")
            .field("fixtures", &self.fixtures)
            .finish()
    }
}

/// Fail-closed. **Always exit 2**: a preflight failure is a fault of ours,
/// never a verdict about the crate.
#[derive(Debug)]
pub struct PreflightError {
    pub stage: Stage,
    pub msg: String,
}

/// R26's seven steps that can actually FAIL. `cost` is not a variant here: it
/// only formats a string (see [`announce_cost`]) and cannot fail, so it has no
/// place in an error's own classification.
///
/// **The stage travels in the TYPE** so a caller can recognise its own
/// preflight failure by matching on [`PreflightError::stage`] instead of
/// comparing text — the same reason `RotationKind` exists in the crate under
/// test rather than a bare string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Config,
    Fixtures,
    Workspace,
    Lock,
    Backend,
    Probe,
    Proxy,
}

impl PreflightError {
    /// Builds a fail-closed error naming which stage could not be verified.
    pub fn cannot_test(stage: Stage, msg: impl Into<String>) -> Self {
        Self {
            stage,
            msg: msg.into(),
        }
    }
}

impl std::fmt::Display for PreflightError {
    /// Names the stage, WHERE it sits in the fixed order, and the message.
    ///
    /// The position is not decoration: the order is the contract, and a reader
    /// who knows a failure happened at step 5 of 8 also knows the four before it
    /// passed — which is most of what they need to start looking.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let order = preflight_step_order();
        let label = format!("{:?}", self.stage).to_ascii_lowercase();
        match order.iter().position(|s| *s == label) {
            Some(i) => write!(
                f,
                "{:?}: {} (step {} of {} in the preflight order)",
                self.stage,
                self.msg,
                i + 1,
                order.len()
            ),
            None => write!(f, "{:?}: {}", self.stage, self.msg),
        }
    }
}

impl std::error::Error for PreflightError {}

/// R26's fixed step order, as the literal names printed alongside a failure —
/// the same order [`run`] executes them in.
///
/// A free-standing function rather than only living inside `run`'s body, so a
/// test can pin the ORDER on its own instead of exercising the whole async
/// pipeline (config, fixtures, a real workspace, a real git repo, a live
/// backend) just to observe it.
///
/// # It is a HAND-MAINTAINED MIRROR of `run`, and nothing ties the two together
///
/// This list is a literal. [`run`] executes its steps in the same order today,
/// and **no mechanism keeps that true**: reordering, adding or removing a step
/// there leaves this unchanged, and every test that reads it would still pass.
/// It is load-bearing beyond documentation, because [`PreflightError`]'s
/// `Display` looks a stage up in this list and prints its POSITION — so a
/// mirror that has drifted does not go quiet, it prints a confident wrong
/// number ("step 5 of 8") to the reader who most needs it.
///
/// **Recorded as a known guard-completeness gap rather than closed**, and the
/// reason is that the honest closure is not cheap. Tying the two would mean
/// driving `run` to failure at each step in turn, which needs a broken fixture
/// corpus, a non-isolated workspace, an untracked lock and an unreachable
/// backend — and `run` resolves the repository and smoke directories from
/// `paths.rs` at global scope rather than from arguments, so most of those
/// cannot be staged without restructuring the function under test. Building
/// that machinery to guard a list of eight strings is the trade the deferral
/// declines; leaving the risk unwritten is not.
pub fn preflight_step_order() -> [&'static str; 8] {
    [
        "config",
        "fixtures",
        "workspace",
        "lock",
        "backend",
        "probe",
        "proxy",
        "cost",
    ]
}

/// R26's order, EXECUTED — the step list a test pins is worthless if nothing
/// calls the steps in it. Every early return is exit 2: a preflight failure
/// is ours, never a verdict about the crate.
///
/// # Parameters
///
/// * `cfg` — the loaded configuration.
/// * `live` — the scenario ids this run will evaluate, for the fixture audit.
/// * `break_proxy` — the harness's own self-test hook; see [`raise_proxy`].
/// * `no_backend` — when true, the two steps that REQUIRE a live backend
///   (`backend` and `probe`) are skipped.
///
/// # Why `no_backend` reaches this function at all
///
/// It used not to, and the flag it comes from was therefore inert: `--no-backend`
/// promises a partition that runs with no backend, and the preflight ran
/// [`reachable`] and [`probe`] unconditionally, so the very first thing that
/// invocation did was demand the backend it had just been told there was none
/// of. Skipping them is not a softening — with the flag absent both still run
/// exactly as before, which is what
/// `no_backend_skips_the_backend_steps_and_nothing_else` pins from both sides.
pub async fn run(
    cfg: &Config,
    live: &[&str],
    break_proxy: bool,
    no_backend: bool,
) -> Result<Announcement, PreflightError> {
    check_seats(cfg).map_err(|m| PreflightError::cannot_test(Stage::Config, m))?; // config

    let audit = fixtures::Manifest::load(&fixture_dir())
        .and_then(|m| m.verify(&fixture_dir(), live))
        .map_err(|m| PreflightError::cannot_test(Stage::Fixtures, m))?; // fixtures
    if !audit.is_clean() {
        // The audit ACCUMULATES, so the message names them all: discovering
        // one problem per round is what makes a broken corpus expensive.
        return Err(PreflightError::cannot_test(
            Stage::Fixtures,
            format!(
                "fixture corpus: {} missing, {} orphaned, {} corrupt\n{:#?}",
                audit.missing.len(),
                audit.orphans.len(),
                audit.corrupt.len(),
                audit
            ),
        ));
    }

    check_workspace_isolation(&smoke_dir())
        .map_err(|m| PreflightError::cannot_test(Stage::Workspace, m))?; // workspace
    check_lock_is_tracked(&repo_root()).map_err(|m| PreflightError::cannot_test(Stage::Lock, m))?; // lock

    // The only two steps that need a live backend, and the only two the
    // no-backend partition skips. Everything before and after them still runs:
    // a run without a backend still has a config, a fixture corpus, an isolated
    // workspace, a tracked lock and a proxy to raise.
    if !no_backend {
        let listed = reachable(&cfg.endpoint, cfg.probe_timeout())
            .await
            .map_err(|m| PreflightError::cannot_test(Stage::Backend, m))?; // backend
        check_seat_models(cfg, listed.as_deref())
            .map_err(|m| PreflightError::cannot_test(Stage::Backend, m))?;

        // FAIL-CLOSED: a probe that does not answer CUTS. An earlier version
        // kept going "so a scenario would have something to read", which broke
        // R26 — the preflight exists to stop. A preflight-only scenario is
        // evaluated the same way, by calling the individual checks directly,
        // not by letting `run` continue past a failed one.
        probe(cfg, cfg.probe_timeout())
            .await
            .map_err(|m| PreflightError::cannot_test(Stage::Probe, m))?; // probe
    }

    let proxy = raise_proxy(cfg, break_proxy).await?; // proxy
    let mut ledger = CostLedger::new();
    let cost_announcement = ledger.announce(cfg, no_backend); // cost
    Ok(Announcement {
        proxy,
        cost_announcement,
        ledger,
        fixtures: audit.summary(),
    })
}

/// The mages a MAGI run is made of. Not a tunable: the scenarios read the
/// TRIO — one asserts three agents answered, another asserts exactly two did
/// once a seat is taken down — so a run with a different number of seats
/// contradicts them by arithmetic rather than by defect.
const REQUIRED_SEATS: usize = 3;

/// Where the reader is sent, in every seat-related refusal.
const SEAT_FIX: &str = "Copy magi-smoke.toml.example to magi-smoke.toml and edit it, or drop \
                        the `seats` override to fall back to the built-in trio.";

/// Rejects a config that cannot run the trio, **naming the fix**.
///
/// Belongs to the `config` step of R26's order, not to a step of its own: the
/// question is the config's own — *do I have anything to test WITH?* — and
/// adding a step would move a boundary that a test pins.
///
/// # Errors
///
/// A config with no seats, with a number of seats other than [`REQUIRED_SEATS`],
/// with a seat naming something that is not a mage, with two seats naming the
/// SAME mage, or with two seats declaring the same LINEAGE.
///
/// **Only the zero case used to be rejected**, and one or two seats walked
/// straight past — which is not a harmless permissiveness: the degradation
/// scenario asserts that exactly two agents answered out of three, so a
/// two-seat config produced a red row about the crate for a mistake in a TOML
/// file. Two seats naming the same mage does the same thing by another route,
/// since the builder registers per agent and the second silently replaces the
/// first. Two seats on one LINEAGE is the third route to the same red row: a
/// run-wide lineage condemnation takes both of them down together.
pub fn check_seats(cfg: &Config) -> Result<(), String> {
    if cfg.seats.is_empty() {
        return Err(format!(
            "config has zero seats: every backend run needs models, so the harness would \
             start and be unable to execute a single one. {SEAT_FIX}"
        ));
    }
    if cfg.seats.len() != REQUIRED_SEATS {
        return Err(format!(
            "config has {} seats but a MAGI run is a trio of {REQUIRED_SEATS}: the \
             degradation scenario asserts that exactly two of three agents answered, so \
             any other number makes it report a red row about the crate for a mistake in \
             this file. {SEAT_FIX}",
            cfg.seats.len()
        ));
    }
    // A rotation candidate is not optional for THIS stage. The rotation run
    // exists to observe a seat whose model fails reaching a DIFFERENT lineage,
    // and with an empty pool it has nowhere to go: the scenario would fail and
    // the run would exit 1 — a verdict about the crate — for a section missing
    // from this file. Caught here, in the config step, it is exit 2 instead.
    if cfg.fallbacks.is_empty() {
        return Err(format!(
            "config declares no [[fallbacks]]: the rotation run needs at least one candidate to rotate INTO, and without one it reports a red row about the crate for a mistake in this file. {SEAT_FIX}"
        ));
    }
    // A candidate sharing a seat's lineage is not a rotation target either:
    // rotation exists to LEAVE a lineage, so landing on the same one tests
    // nothing while looking like it did.
    for candidate in &cfg.fallbacks {
        if cfg.seats.iter().any(|s| s.lineage == candidate.lineage) {
            return Err(format!(
                "fallback {:?} declares lineage {:?}, which a seat already uses: rotating to it reaches the same lineage the run was trying to leave, so the scenario would pass over a rotation that proved nothing. {SEAT_FIX}",
                candidate.model, candidate.lineage
            ));
        }
    }
    // The SYMMETRIC case of the check above, and the more expensive one. A
    // lineage is condemned RUN-WIDE by `3.2.0` once a failure on it is
    // classified `Http` -> `Transport`, so two seats declaring one lineage lose
    // it TOGETHER: the injected failure takes down two mages instead of one,
    // the degradation scenario sees a single answer where it asserts two, and
    // the harness exits 1 — a verdict about the crate — for a duplicated field
    // in this file. That 1-versus-2 confusion is the whole reason this step
    // exists.
    let mut lineages: std::collections::BTreeMap<&str, &str> = std::collections::BTreeMap::new();
    for seat in &cfg.seats {
        if let Some(first) = lineages.insert(seat.lineage.as_str(), seat.agent.as_str()) {
            return Err(format!(
                "seats {:?} and {:?} both declare lineage {:?}: a run-wide lineage condemnation takes down BOTH of them at once, so the degradation scenario sees one agent where it asserts two and reports a red row about the crate for a mistake in this file. {SEAT_FIX}",
                first, seat.agent, seat.lineage
            ));
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    for seat in &cfg.seats {
        let name = seat.agent_name().map_err(|e| e.to_string())?;
        if !seen.insert(name) {
            return Err(format!(
                "config seats {:?} twice: the builder registers one provider per agent, so \
                 the second silently replaces the first and the trio is short a mage \
                 without anything saying so. {SEAT_FIX}",
                seat.agent
            ));
        }
    }
    Ok(())
}

/// Compares the `workspace_root` that `cargo metadata` reports from `smoke/`
/// against `smoke/` itself.
///
/// # Errors
///
/// Every error path is prefixed `workspace_root:` — including a `cargo
/// metadata` that could not even be spawned — so any failure of this check
/// names the property it was checking, not just the OS-level symptom.
pub fn check_workspace_isolation(smoke: &Path) -> Result<(), String> {
    // `--no-deps` does not build, but it still locks the target dir on some
    // platforms; a dedicated one keeps the preflight from contending with the
    // build that is about to start.
    let out = std::process::Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(smoke)
        .env("CARGO_TARGET_DIR", crate::paths::metadata_target_dir())
        .output()
        .map_err(|e| {
            format!(
                "workspace_root: could not run cargo metadata in {}: {e}",
                smoke.display()
            )
        })?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("workspace_root: cargo metadata output: {e}"))?;
    let root = v["workspace_root"].as_str().ok_or(
        "workspace_root: cargo metadata returned no workspace_root; the isolation check \
         cannot be answered, so it fails closed rather than guessing",
    )?;
    // Full canonical paths, NOT a suffix: `ends_with("smoke")` also matches
    // `/repo/notsmoke`, so the check would pass on a tree it was meant to
    // reject.
    let reported = std::fs::canonicalize(root).map_err(|e| format!("workspace_root: {e}"))?;
    let expected =
        std::fs::canonicalize(smoke).map_err(|e| format!("workspace_root: smoke dir: {e}"))?;
    if reported != expected {
        return Err(format!(
            "workspace_root is {root}, not smoke/: the repository root absorbed the harness. \
             The crate's gate commands now reach it, which rule 3 forbids."
        ));
    }
    Ok(())
}

/// Verifies that `smoke/Cargo.lock` is **actually tracked**.
///
/// The negation `!smoke/Cargo.lock` in `.gitignore` is fragile: line 3 is the
/// bare `Cargo.lock`, with no slash, so it matches at any depth. A line that
/// stopped applying does not announce itself; a check does.
pub fn check_lock_is_tracked(repo: &Path) -> Result<(), String> {
    let out = std::process::Command::new("git")
        .args(["ls-files", "--error-unmatch", "smoke/Cargo.lock"])
        .current_dir(repo)
        .output()
        .map_err(|e| format!("git ls-files failed: {e}"))?;
    if !out.status.success() {
        return Err(
            "smoke/Cargo.lock is NOT tracked: the `!smoke/Cargo.lock` negation in \
                    .gitignore stopped applying, so the reproducibility that motivated \
                    tracking it does not exist"
                .to_string(),
        );
    }
    Ok(())
}

/// The path [`reachable`] hits. Listing tags never depends on a model being
/// loaded, which is exactly what makes it the right call for "is anybody
/// there?" — and exactly what disqualifies it from [`try_once`], which has to
/// ask a question only a busy backend answers slowly.
const REACHABILITY_PATH: &str = "/api/tags";

/// The path [`try_once`] hits: the OpenAI-compatible completions endpoint
/// `OllamaProvider` itself speaks in `3.2.0`, so the probe queues behind the
/// same work the run is about to do rather than behind a different subsystem.
const COMPLETIONS_PATH: &str = "/v1/chat/completions";

/// The output bound on the probe's completion (R27: `max_tokens: 1`).
///
/// One token is the smallest request that still has to be GENERATED, which is
/// the whole point: generation is what enters the inference queue, and the
/// queue is what contention shows up in.
const PROBE_MAX_TOKENS: u32 = 1;

/// The prompt the probe sends. Deliberately trivial — the probe measures
/// whether the backend can get to the work at all, never how well it does it.
const PROBE_PROMPT: &str = "hi";

/// Is anybody there at all, **and what does it hold**? One request, bounded by
/// `window`, with no retry: a completely unreachable endpoint is a different
/// failure than a reachable one that is merely slow (see [`probe`] for that
/// case).
///
/// # It returns the listing instead of dropping it
///
/// The answer to "is anybody there?" is a listing of the models the backend
/// holds, and this function used to read its STATUS and throw the body away —
/// so the one place the harness could tell a mistyped seat model from a real
/// one discarded the evidence for free. It is returned instead, for
/// [`check_seat_models`]; `None` means the body was not a listing this harness
/// can read, which is not the same claim as "it holds nothing".
///
/// # `window` is the CONFIGURED probe timeout, not a constant of its own
///
/// It used to be a hardcoded ten seconds, which happened to equal the shipped
/// default of `probe_timeout_secs` and was independent of it. That
/// independence had a wrong direction: an operator who RAISES the probe
/// timeout for a slow backend is declaring exactly how long a trivial request
/// may take, and a fixed ten-second cut would still report their live backend
/// as unreachable — a preflight refusal, on the strength of a number they had
/// already overridden. One knob is what makes both steps answer to the same
/// declaration of patience.
///
/// # This is NOT the same request [`try_once`] sends, and that is the point
///
/// The two used to hit the same path with the same method, which made R26's
/// two steps one question asked twice — the second learning nothing the first
/// had not already answered. Reachability asks *is anybody there?*, and a
/// listing answers that without loading anything; the probe asks *can the
/// backend get to work?*, which only a completion can put to it.
///
/// # Parameters
///
/// * `endpoint` — the backend's base URL.
/// * `window` — how long the whole request may take, from
///   [`Config::probe_timeout`].
async fn reachable(endpoint: &str, window: Duration) -> Result<Option<Vec<String>>, String> {
    let client = reqwest::Client::builder()
        .timeout(window)
        .build()
        .map_err(|e| format!("backend reachability: {e}"))?;
    let resp = client
        .get(format!("{endpoint}{REACHABILITY_PATH}"))
        .send()
        .await
        .map_err(|e| format!("backend at {endpoint} did not answer: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!(
            "backend at {endpoint} answered with status {}",
            resp.status()
        ));
    }
    // The body is READ, not discarded: it is the model listing, and the step
    // after this one is the only place the harness ever gets to see it. A body
    // that cannot be read at all is reported as an unreadable listing rather
    // than as no models — see `check_seat_models` for what the difference
    // decides.
    let body = resp.bytes().await.map_err(|e| {
        format!("backend at {endpoint} answered but its listing could not be read: {e}")
    })?;
    Ok(listed_models(&body))
}

/// Rejects a run whose SEAT models the backend does not hold — **exit 2, and
/// never a verdict about the crate**.
///
/// The listing this reads is the one [`reachable`] already fetched: the
/// reachability step asks `/api/tags` and used to look only at its status,
/// throwing away the body that answers this question. A mistyped model
/// therefore reached the runs, where every mage that could not be built
/// reported a red row — exit 1, a verdict about the crate, for a typo in a
/// TOML file. That inversion is the one thing this harness exists to prevent.
///
/// # Only a PROVEN absence refuses, and that boundary is deliberate
///
/// `listed` is `None` when the backend answered the reachability path with
/// something this harness cannot read as a model listing. Nothing was
/// established then, and reading "I could not tell" as "it holds nothing"
/// would refuse every run against a backend that is merely shaped
/// differently. It is the same direction the crate under test chose for its
/// own digest verification: only a proven mismatch rejects, and an
/// unresolvable one trusts what was declared.
///
/// # The rotation candidates are checked here too, and the probe does not
/// cover them
///
/// This used to check the seats only, on the argument that the contention
/// probe already refuses a candidate the backend answers `404` for. That
/// argument covered exactly ONE candidate — [`Config::probe_model`] names the
/// LAST declared fallback and nothing else — while rotation reaches the FIRST
/// of another lineage that fits, out of a pool as long as the operator makes
/// it. Every entry between the two was unchecked, so a typo there survived the
/// preflight and produced a red row about the crate from the rotation run:
/// exit 1 for a mistake in a TOML file, which is the inversion this harness
/// exists to remove.
///
/// It is one `chain` over the listing already in hand, not a second
/// implementation: the probe answers a different question (*can the backend
/// get to work?*), and it answers it about one model at a time.
///
/// # Parameters
///
/// * `cfg` — the loaded configuration, for the seats and rotation candidates
///   to check.
/// * `listed` — the model names the backend listed, or `None` when it did not
///   list any.
///
/// # Errors
///
/// Names every declared model that is absent, with what declared it and what
/// the backend does hold — all of them, because discovering one typo per round
/// is what makes a wrong config expensive.
///
/// # Complexity
///
/// `O((s + f) * m)` over the three seats, the `f` rotation candidates and the
/// models listed.
pub fn check_seat_models(cfg: &Config, listed: Option<&[String]>) -> Result<(), String> {
    let Some(listed) = listed else {
        return Ok(());
    };
    let declared = cfg
        .seats
        .iter()
        .map(|s| (s.agent.as_str(), s.model.as_str()))
        .chain(cfg.fallbacks.iter().map(|f| ("rotation", f.model.as_str())));
    let absent: Vec<String> = declared
        .filter(|(_, model)| !listed.iter().any(|held| held == model))
        .map(|(who, model)| format!("{who} declares {model:?}"))
        .collect();
    if absent.is_empty() {
        return Ok(());
    }
    Err(format!(
        "the backend does not hold {} of the configured models ({}). It lists {:?}. A model it \
         cannot serve makes that mage fail to answer — or leaves rotation with nowhere to go — \
         which the runs report as a red row about the crate for a mistake in this file. Pull the \
         model, or correct the entry. {SEAT_FIX}",
        absent.len(),
        absent.join("; "),
        listed
    ))
}

/// The model names an `/api/tags` body lists, or `None` when the body is not a
/// listing this harness can read.
///
/// **`None` is not "no models"** — see [`check_seat_models`] for why the
/// difference decides whether a run is refused.
///
/// Both `name` and `model` are collected: Ollama carries the tag in `name` and
/// repeats it in `model`, and a backend that fills only one of them still
/// answered the question.
///
/// # Parameters
///
/// * `body` — the reachability response body, as received.
///
/// # Complexity
///
/// `O(n)` in the body's length.
fn listed_models(body: &[u8]) -> Option<Vec<String>> {
    let v: serde_json::Value = serde_json::from_slice(body).ok()?;
    let models = v.get("models")?.as_array()?;
    Some(
        models
            .iter()
            .filter_map(|m| {
                m.get("name")
                    .or_else(|| m.get("model"))
                    .and_then(|n| n.as_str())
                    .map(str::to_string)
            })
            .collect(),
    )
}

/// One bounded attempt at a REAL completion: the WHOLE request — connect, send
/// and read — must complete inside `window`, or it is treated as a failure
/// regardless of whether the connection is later going to succeed.
///
/// # Why a completion and not a listing
///
/// This used to be `GET /api/tags`, which reads manifests off disk: it never
/// loads a model, never touches the GPU and never enters the inference queue.
/// An Ollama saturated by three mages answers it INSTANTLY, so the probe
/// reported "clear" and the harness walked into the contention R27 exists to
/// detect. Generation is the thing that queues, so generation is what is asked
/// for — bounded to [`PROBE_MAX_TOKENS`] so asking costs almost nothing.
///
/// # A non-2xx answer is still an ANSWER — with ONE exception, and it is `404`
///
/// A backend that is busy answers slowly whatever the status it ends up
/// returning, so a `429`, a `500` or a `503` all arrive having gone through the
/// queue: they say the endpoint is responsive, which is this step's entire
/// question, and treating them as probe failures would report "saturated" for a
/// backend that merely refused.
///
/// `404` is not one of those. A model the backend does not hold is rejected on
/// INSPECTION — nothing is loaded, nothing is generated, and the request never
/// enters the inference queue at all — so a saturated backend answers it as
/// fast as an idle one. That is the same blind spot that made `GET /api/tags`
/// unusable here, re-entering through MODEL selection instead of path
/// selection, and it matters because [`Config::probe_model`] deliberately names
/// the LAST declared fallback: the model least likely to be resident.
///
/// So a `404` is neither "clear" nor "saturated" but [`Probe::Inconclusive`] —
/// the probe could not ask its question. See [`probe`] for what the preflight
/// does with that.
///
/// # Parameters
///
/// * `cfg` — the configuration, for the endpoint and the model to name.
/// * `window` — how long the whole request may take.
async fn try_once(cfg: &Config, window: Duration) -> Result<Probe, String> {
    let client = reqwest::Client::builder()
        .timeout(window)
        .build()
        .map_err(|e| format!("probe: {e}"))?;
    let body = serde_json::json!({
        "model": cfg.probe_model(),
        "max_tokens": PROBE_MAX_TOKENS,
        "messages": [{ "role": "user", "content": PROBE_PROMPT }],
    });
    let resp = client
        .post(format!("{}{COMPLETIONS_PATH}", cfg.endpoint))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("probe: {e}"))?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Probe::Inconclusive);
    }
    Ok(Probe::Served)
}

/// What one probe attempt established about the backend's queue.
///
/// Three outcomes and not two, because "the request completed" and "the request
/// was SERVED" are different claims and only the second answers R27. The third
/// state — the attempt did not complete in time — is the `Err` of
/// [`try_once`], since it is the one this step was built to detect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Probe {
    /// The backend took the request through its inference queue and answered.
    Served,
    /// The backend answered without generating anything, so the queue was
    /// never exercised and NOTHING was learned about contention. Never
    /// reported as clear: see [`probe`].
    Inconclusive,
}

/// One trivial completion, retried ONCE with a widened window. The retry is
/// what makes "clone and run" work without pre-warming anything: a cold model
/// loads ONCE, so the second attempt passes.
///
/// # An INCONCLUSIVE attempt is refused, and it is refused IMMEDIATELY
///
/// The three outcomes reach the preflight as two, because only one of them lets
/// the run proceed:
///
/// * served — the queue was exercised and answered in time: proceed;
/// * did not complete — contention or a cold model, retried once and then
///   [`probe_failure_message`];
/// * [`Probe::Inconclusive`] — the backend answered without generating, so the
///   probe never asked its question. Refused, and refused with a DIFFERENT
///   message: reporting the contention one would name two causes that are both
///   false, and reporting success would certify a backend nothing measured.
///
/// The retry is deliberately skipped for the inconclusive case. Widening the
/// window changes how long the harness waits, and a model the backend does not
/// hold is not going to appear because it was waited for — the second attempt
/// would return the same `404` after spending the operator's time.
///
/// # Errors
///
/// When the probe could not be served: a request that did not complete inside
/// the (retried) window, or an answer that arrived without generating.
pub async fn probe(cfg: &Config, window: Duration) -> Result<(), String> {
    match try_once(cfg, window).await {
        Ok(Probe::Served) => return Ok(()),
        Ok(Probe::Inconclusive) => return Err(probe_inconclusive_message(cfg)),
        Err(_) => {}
    }
    // `Duration * u32`, NOT the other way round: `Mul<u32> for Duration`
    // exists, `Mul<Duration> for u32` does not. A type error, not a style
    // choice.
    match try_once(cfg, window * PROBE_RETRY_FACTOR).await {
        Ok(Probe::Served) => Ok(()),
        Ok(Probe::Inconclusive) => Err(probe_inconclusive_message(cfg)),
        Err(_) => Err(probe_failure_message()),
    }
}

/// The refusal for a probe that was answered without being served.
///
/// It names the model, because that is the field the operator has to act on —
/// either pull it or stop declaring it — and it says what was NOT established
/// rather than guessing at a cause. Exit 2, "cannot test": no scenario ran, so
/// nothing here is a verdict about the crate.
///
/// # Parameters
///
/// * `cfg` — the configuration, for the model and endpoint to name.
fn probe_inconclusive_message(cfg: &Config) -> String {
    format!(
        "cannot test: the endpoint at {} answered the contention probe with 404 for model {:?}. \
         A model the backend does not hold is rejected without generating, so the request never \
         entered the inference queue and NOTHING was established about contention — a saturated \
         backend answers it just as fast as an idle one. Pull that model, or name one this \
         backend holds.",
        cfg.endpoint,
        cfg.probe_model()
    )
}

/// One trivial request. If it does not answer in time, the run reports
/// "cannot test", never "failed".
///
/// SCOPE, declared next to the detection: this does NOT distinguish MAGI from
/// any other load, and does NOT see contention that starts AFTER the
/// preflight. It catches the common case — starting the smoke while MAGI is
/// still running — and nothing more.
pub fn probe_failure_message() -> String {
    "cannot test: the endpoint did not answer a trivial request in time. Two causes are \
     possible and the harness cannot tell them apart: contention (another client holding the \
     backend) or a cold model still loading. The probe already retried once with a widened \
     window."
        .to_string()
}

/// Raises the spy proxy. **`break_proxy` is a self-test hook of the harness**,
/// not configuration: it makes the proxy fail to bind so `S20` can observe
/// that a proxy fault reports "cannot test" and never a scenario red.
///
/// A CLI flag and NOT a `MAGI_SMOKE_*` variable, deliberately: that namespace
/// is fail-closed over UNKNOWN keys, so a hook living there would have to be
/// registered as configuration — and the next reader would take it for one.
pub async fn raise_proxy(cfg: &Config, break_proxy: bool) -> Result<SpyProxy, PreflightError> {
    if break_proxy {
        return Err(PreflightError::cannot_test(
            Stage::Proxy,
            "proxy: refused to start (--break-proxy). This is a HARNESS fault, never a \
             verdict about the crate: no scenario is reported as passed.",
        ));
    }
    // The upstream bound is the LONGEST budget of any backend-using run, so
    // the proxy can never be what cuts first: a proxy cut is a HARNESS fault
    // ("cannot test"), and turning a slow-but-legal backend into one would
    // hide the very thing the run was measuring.
    SpyProxy::start(
        cfg.endpoint.clone(),
        cfg.payload_target_bytes,
        cfg.longest_backend_budget(),
    )
    .await
    .map_err(|e| PreflightError::cannot_test(Stage::Proxy, format!("proxy: could not start: {e}")))
}

/// Coarse bytes-to-tokens divisor. **The SAME crude bound `magi-core` itself
/// uses** (its own `chars / 4` pre-filter) — said as what it is, a lower
/// bound, not a measurement.
const TOKEN_ESTIMATE_DIVISOR: usize = 4;

/// R31: the sentence printed BEFORE the first run — estimated tokens and the
/// expected time budget, so an operator sees the cost before anything spends
/// it.
///
/// # It announces the runs that will ACTUALLY happen
///
/// Both halves of this sentence used to describe a run this stage never
/// launches. The time was the sum over a hand-written list that included
/// [`RunId::Large62k`], deliberately excluded from the stage
/// ([`RunSpec::for_stage_e1`](crate::runner::RunSpec::for_stage_e1)), so the
/// figure promised time nobody was going to spend. And the tokens came from
/// `payload_target_bytes`, which sizes that same absent run, while every run
/// that does launch analyses `run_payload_bytes` — two orders of magnitude
/// apart with the shipped defaults.
///
/// The run list now comes from [`stage_e1_run_ids`], the same function the
/// runner's own list is checked against, so an announcement that stops matching
/// the runs is a red test rather than a wrong number nobody re-reads.
///
/// # Both figures are stated as what they are
///
/// Tokens are `bytes / 4`, the same coarse bound the crate itself uses, said to
/// be a coarse bound and not a measurement. Time is the SUM of the budgets of
/// the backend runs about to launch — a cap on what each may spend, never a
/// prediction of what it will.
///
/// # Parameters
///
/// * `cfg` — the loaded configuration, for the payload size and the budgets.
/// * `no_backend` — the partition flag. With it, no run reaches the backend at
///   all, and the sentence says so rather than announcing a cost of zero as if
///   zero were an estimate.
///
/// # Complexity
///
/// `O(r)` in the number of runs the stage launches.
pub fn announce_cost(cfg: &Config, no_backend: bool) -> String {
    let backend_runs: Vec<RunId> = stage_e1_run_ids(no_backend)
        .into_iter()
        .filter(|r| r.uses_backend())
        .collect();
    if backend_runs.is_empty() {
        return "preflight: no run in this partition reaches the backend, so nothing will be \
                spent on it"
            .to_string();
    }
    let estimated_tokens = cfg.run_payload_bytes / TOKEN_ESTIMATE_DIVISOR;
    let expected_secs: u64 = backend_runs.iter().map(|r| cfg.budget(*r).as_secs()).sum();
    let names: Vec<&str> = backend_runs.iter().map(|r| r.as_str()).collect();
    format!(
        "preflight: {} backend run(s) about to start ({}), each analysing ~{estimated_tokens} \
         input tokens (bytes/4, a coarse bound, not a measurement); expected time budget \
         ~{expected_secs}s in total",
        backend_runs.len(),
        names.join(", ")
    )
}

/// R31, both halves: the estimate printed BEFORE the runs, and the real cost
/// recorded AFTER them.
///
/// # Why the ORDER is a type and not a convention
///
/// R31's second half was implemented nowhere at all — the harness announced an
/// estimate and never recorded what the run actually cost, so the historical
/// series that R37's fixed filename exists to produce had nothing to plot. And
/// the ORDER is the load-bearing half of the requirement: after the spend,
/// "it cost this much" is a receipt; before it, it is a decision the operator
/// can still act on.
///
/// A ledger makes that order checkable instead of hoping two `eprintln!`s stay
/// where somebody put them: [`CostLedger::record`] REFUSES unless the estimate
/// was announced AND the runs were measured, so a reordering that puts the
/// receipt first fails rather than printing something reasonable-looking in
/// the wrong place.
///
/// # Why the announcement alone was not enough, and why the runs are timed
/// separately from it
///
/// Hanging the refusal on the announcement made the guarantee UNREACHABLE:
/// [`announce`](CostLedger::announce) runs inside [`run`], so every ledger a
/// caller can hold has already announced, and `record` moved up to just after
/// the preflight answered with a plausible receipt for runs that had not
/// happened yet.
///
/// Timing the runs separately from the announcement also fixes what is
/// measured. Announcing and running are NOT the same instant: between them sits
/// the feature matrix, whose four `cargo check` runs landed inside the recorded
/// interval whenever `--build-matrix` was passed. A cost series that includes
/// four builds on some releases and not on others is not comparable across
/// them, which is the entire point of writing the certificate to one fixed
/// path.
pub struct CostLedger {
    /// Whether [`CostLedger::announce`] ran. The estimate is a statement rather
    /// than a measurement, so what matters is that it was made, not when.
    announced: bool,
    /// One elapsed duration per call to [`measure`](CostLedger::measure), in
    /// the order they were made. The receipt reports their SUM, and their
    /// COUNT is what [`record`](CostLedger::record) checks against
    /// `backend_runs`.
    ///
    /// Elapsed DURATIONS rather than start instants, so that neither end of an
    /// interval depends on where a call sits: an instant left
    /// [`record`](CostLedger::record) measuring to its own call site, so the
    /// interval grew by whatever was moved in between.
    ///
    /// A LIST rather than one total, because the count is the guard. A single
    /// accumulator would sum to the same seconds while saying nothing about how
    /// many calls produced them, and how many calls produced them is the only
    /// thing here that describes the SHAPE of the call site.
    measured: Vec<std::time::Duration>,
    /// How many backend runs the announcement was about, so the receipt
    /// describes the same set.
    backend_runs: usize,
}

impl CostLedger {
    /// A ledger with nothing announced yet.
    pub fn new() -> Self {
        CostLedger {
            announced: false,
            measured: Vec::new(),
            backend_runs: 0,
        }
    }

    /// The sentence printed BEFORE the first run.
    ///
    /// It measures nothing; see [`measure`](CostLedger::measure).
    ///
    /// # Parameters
    ///
    /// * `cfg` — the loaded configuration, for the payload size and budgets.
    /// * `no_backend` — the partition flag; see [`announce_cost`].
    pub fn announce(&mut self, cfg: &Config, no_backend: bool) -> String {
        self.announced = true;
        self.backend_runs = stage_e1_run_ids(no_backend)
            .into_iter()
            .filter(|r| r.uses_backend())
            .count();
        announce_cost(cfg, no_backend)
    }

    /// What the runs ACTUALLY cost, measured after them.
    ///
    /// Wall-clock time and the number of backend runs, said as what they are.
    /// Not tokens: the harness never sees a tokeniser, and the estimate it
    /// announced was already declared a coarse bound rather than a measurement
    /// — inventing a "real" token count here would turn the same guess into a
    /// figure that looks measured.
    ///
    /// # Errors
    ///
    /// When either half of the order is missing, and the two messages say which
    /// one, because they send the reader to different places:
    ///
    /// * nothing was announced — the estimate has to come first, since before
    ///   the spend the same number is a decision the operator can still make;
    /// * the runs were never measured — there is no interval to report, so a
    ///   number here would be a receipt for work nothing timed. It says
    ///   MEASURED and not "the runs never started", because the two are not
    ///   the same claim and only one of them is always true: a [`measure`]
    ///   future dropped mid-await leaves runs that did start and still no
    ///   interval, and the refusal has to be honest about that case too;
    /// * the number of intervals does not equal the number of backend runs the
    ///   announcement counted — see below. Its own message, because it sends
    ///   the reader to a third place: not to a missing call but to the SHAPE of
    ///   the call site.
    ///
    /// # Why the COUNT is checked, and what that does and does not close
    ///
    /// While one interval spanned the whole batch, which work sat inside it was
    /// decided by the caller and by nothing else, and two shipped regressions
    /// billed the certificate for neighbouring work with every test green. One
    /// interval per run turns that into arithmetic: a caller that wraps the
    /// batch again produces ONE interval for N runs, and a caller that adds a
    /// measured call produces N+1. Both refuse instead of returning a plausible
    /// number, and under-measurement — the direction that used to report a
    /// quiet `0.0s` — refuses too.
    ///
    /// **It is a partial closure and the boundary is stated.** Work smuggled
    /// INSIDE one of the N measured calls is still billed and still counted,
    /// so the count comparison cannot see it. What changed is that doing so is
    /// no longer a quiet reorder: the batch call that invited a neighbour to be
    /// dropped in is gone, and the one-per-run calls sit inside a loop, so
    /// billing a once-per-process step there also runs it N times — a
    /// conspicuous change rather than a line that moved.
    ///
    /// [`measure`]: CostLedger::measure
    pub fn record(&self) -> Result<String, String> {
        if !self.announced {
            return Err(
                "the real cost cannot be recorded before the estimate was announced: R31 asks \
                 for the estimate FIRST, because after the spend it is a receipt and before it, \
                 it is a decision the operator can still make"
                    .to_string(),
            );
        }
        if self.measured.is_empty() && self.backend_runs > 0 {
            return Err(
                "the real cost cannot be recorded before the runs were measured: there is no \
                 interval to report, so a number here would be a receipt for work nothing timed"
                    .to_string(),
            );
        }
        if self.measured.len() != self.backend_runs {
            return Err(format!(
                "the real cost cannot be recorded from {} interval(s) for {} announced backend \
                 run(s): the receipt bills one interval per run, so a different number means the \
                 call site measured something other than the runs — one interval for all of them \
                 sweeps in whatever sits between, and an extra one bills work the estimate never \
                 announced",
                self.measured.len(),
                self.backend_runs
            ));
        }
        Ok(format!(
            "{} backend run(s) in {:.1}s",
            self.backend_runs,
            self.measured
                .iter()
                .sum::<std::time::Duration>()
                .as_secs_f64()
        ))
    }

    /// Times ONE backend run, and appends its elapsed duration to the intervals
    /// [`record`](CostLedger::record) sums and counts.
    ///
    /// **Called once per backend run, never once around the batch.** That is
    /// the whole of the design, and `record` refuses when the number of
    /// intervals does not match the announced count — so the rule is arithmetic
    /// rather than a convention about which call the caller chose to wrap.
    ///
    /// The interval is captured around the work itself: the clock starts here
    /// and stops when the awaited work returns, so **neither end depends on
    /// where a call sits**. That is what the defect cost twice. A separate
    /// start mark drifted upwards into the preflight and the receipt silently
    /// grew by everything above it; then, with the start made structural, the
    /// end was still computed inside `record`, so moving the feature matrix
    /// down to sit between the runs and the receipt billed the certificate for
    /// four `cargo check` runs with every test green.
    ///
    /// # What the count comparison closes, and what it does not
    ///
    /// It closes the two SHAPES the call site can take wrongly. Going back to
    /// one call around the batch gives one interval for N runs; adding a
    /// measured call to bill something else gives N+1; running the work outside
    /// and passing `async {}` gives intervals that do not correspond to runs at
    /// all, and any of those refuses instead of returning a number.
    ///
    /// It does NOT close work smuggled inside one of the N calls: that work is
    /// measured, because measuring the argument is what this function does, and
    /// the count is unchanged. The residue is narrower than what it replaced,
    /// though. Both regressions that actually shipped were a line MOVED across
    /// a single batch-wide interval; there is no such interval left to move it
    /// into, and the per-run calls sit in a loop, so billing a once-per-process
    /// step from there also executes it N times — visible in the diff and in
    /// the run, rather than quiet.
    ///
    /// What that excludes, said as what it is: the git baseline, the spec
    /// build, the fixture audit, the feature matrix's four `cargo check` runs
    /// under `--build-matrix`, the transparency probe, the offline run — and
    /// now also anything the caller does BETWEEN two runs, which a single
    /// batch-wide interval swallowed by construction. The transparency probe is
    /// a real round-trip to the backend, so it is excluded not for being local
    /// but for not being one of the N runs the receipt counts; the offline run
    /// is excluded because the receipt counts BACKEND runs and billing it would
    /// put a run outside that count inside the interval that reports it.
    ///
    /// # Parameters
    ///
    /// * `work` — one backend run, awaited here and returned untouched.
    pub async fn measure<T>(&mut self, work: impl std::future::Future<Output = T>) -> T {
        let started = std::time::Instant::now();
        let out = work.await;
        self.measured.push(started.elapsed());
        out
    }
}

impl Default for CostLedger {
    fn default() -> Self {
        Self::new()
    }
}

/// Deletes leftover temp dirs whose PID has no live process. **Sweeping on
/// ENTRY is what survives a run that had no exit**; cleaning on exit is the
/// normal path.
///
/// The PID check is not optional: a sweep that ignored it would delete a
/// CONCURRENT harness's temps, turning a cleanup into a race.
///
/// Name format: `magi-smoke-<pid>-<ms>-<rand>`. A directory whose name does
/// not parse is **left alone** — it is not ours to judge.
///
/// # The one place in this harness where swallowing an error IS right
///
/// The two sibling scans fixed alongside this one — the fixture audit and the
/// payload walk — report every entry they could not read, because each is a
/// GUARD: skipping something lets it return a clean answer over a corpus it
/// did not fully see. This is not a guard. It answers no question, asserts
/// nothing, and reports nothing on success either, so there is no success
/// being claimed over what was skipped. An entry it cannot read costs exactly
/// one leaked temp directory, which the NEXT sweep collects on the next start.
/// The `Err` arm is written out rather than flattened so that the choice is
/// visible as a choice, and not read as the same oversight the other two had.
pub(crate) fn sweep_stale_temps(root: &Path) {
    // called by main.rs
    let Ok(entries) = std::fs::read_dir(root) else {
        return; // unreadable temp dir is not fatal
    };
    for entry in entries {
        let Ok(e) = entry else {
            // Deliberate, and argued in this function's doc: a missed entry
            // leaks one directory that the next sweep reclaims. Nothing is
            // asserted here, so nothing is falsely asserted by the skip.
            continue;
        };
        let name = e.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(rest) = name.strip_prefix("magi-smoke-") else {
            continue;
        };
        let Some((pid, _)) = rest.split_once('-') else {
            continue;
        };
        let Ok(pid) = pid.parse::<u32>() else {
            continue;
        };
        if !pid_is_alive(pid) {
            let _ = std::fs::remove_dir_all(e.path()); // best effort, never fatal
        }
    }
}

/// Liveness WITHOUT killing, on both platforms the project builds on, and
/// **without a new dependency** — `libc` / `winapi` would each be a whole
/// crate for one question asked once per start (§0.1.8: standard library
/// first).
///
/// **Errs toward "alive"**, deliberately: mistaking a live PID for dead
/// deletes a concurrent run's working directory; mistaking a dead one for
/// alive leaks one directory that the next sweep collects. Asymmetric costs,
/// asymmetric default.
#[cfg(unix)]
fn pid_is_alive(pid: u32) -> bool {
    // `kill -0` performs error checking only; it does not send a signal.
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(true) // cannot tell -> assume alive, the safe direction
}

#[cfg(windows)]
fn pid_is_alive(pid: u32) -> bool {
    // `/NH` drops the header, so the PID can only appear in a real row.
    // Without it, tasklist prints "INFO: No tasks are running..." and a
    // substring match on the filter echo would read every dead PID as alive.
    std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Seat;
    use crate::testkit::{
        repo_where_the_negation_was_removed, run_against_an_unreachable_backend,
        run_with_broken_proxy, stub_that_holds_no_model, stub_that_is_always_slow,
        stub_that_is_slow_on_first_request_only, stub_that_lists_models,
        stub_that_records_requests, temp_root_with, tempdir_with,
    };

    #[test]
    fn the_order_is_config_then_fixtures_then_backend() {
        // Fixture verification belongs to the PREFLIGHT, not to the scenario
        // that uses them: inside a scenario, the earlier ones already burned
        // backend and tokens before discovering the corpus was wrong. Same
        // question as the config's: do I have anything to test WITH?
        let steps = preflight_step_order();
        // R26 fixes this order. `proxy` is NOT optional: R9 says EVERY
        // request goes through it, so a run that starts before the proxy is
        // up would silently bypass the only thing that can observe the wire.
        assert_eq!(
            steps,
            [
                "config",
                "fixtures",
                "workspace",
                "lock",
                "backend",
                "probe",
                "proxy",
                "cost"
            ]
        );
    }

    #[test]
    fn the_cost_announced_is_the_cost_of_the_runs_that_will_actually_happen() {
        // It used to sum the large-payload run's budget — a run this stage
        // deliberately never launches — and to size its token estimate from
        // `payload_target_bytes`, which belongs to that same absent run. Both
        // halves therefore promised more than the invocation could ever spend,
        // and an announcement that overstates is one an operator learns to
        // ignore.
        let cfg = Config::default();
        let announced = announce_cost(&cfg, false);

        let expected_secs: u64 = cfg.budget(RunId::HappySmall).as_secs()
            + cfg.budget(RunId::Rotation).as_secs()
            + cfg.budget(RunId::Degradation).as_secs();
        assert!(
            announced.contains(&format!("~{expected_secs}s")),
            "the time must be the sum over the launched backend runs: {announced}"
        );
        assert!(
            !announced.contains(&format!(
                "~{}s",
                expected_secs + cfg.budget(RunId::Large62k).as_secs()
            )),
            "the large-payload run is not launched, so its budget must not be in the \
             announcement: {announced}"
        );
        assert!(
            !announced.contains(RunId::Large62k.as_str()),
            "and it must not be named as a run about to start: {announced}"
        );
        assert!(
            announced.contains(&format!(
                "~{} input tokens",
                cfg.run_payload_bytes / TOKEN_ESTIMATE_DIVISOR
            )),
            "the tokens must come from the payload the launched runs analyse, not from the \
             one that sizes the absent run: {announced}"
        );
    }

    #[allow(non_snake_case)]
    #[tokio::test]
    async fn the_cost_is_announced_BEFORE_the_runs_and_recorded_AFTER() {
        // R31, and it had no test: the plan named it in prose only, so its
        // implementation depended on somebody remembering an unverified
        // requirement — the class of omission this harness exists to catch in
        // others.
        //
        // ANNOUNCED BEFORE matters more than it looks. After the run, "it cost
        // this much" is a receipt; before it, the same number is a decision the
        // operator can still make. The ledger is what turns that ordering from a
        // convention about where two prints sit into something a test can fail:
        // `record` REFUSES when nothing was announced.
        let cfg = Config::default();
        let mut ledger = CostLedger::new();

        let refusal = ledger
            .record()
            .expect_err("a receipt before the estimate is not a receipt");
        assert!(
            refusal.contains("before the estimate was announced"),
            "the refusal must say what order was violated, not merely that something was: \
             {refusal}"
        );

        let announced = ledger.announce(&cfg, false);
        assert!(
            announced.contains("about to start"),
            "the estimate speaks of runs that have not happened yet: {announced}"
        );
        for _ in 0..backend_runs_of(&cfg) {
            ledger.measure(async {}).await;
        }
        let recorded = ledger
            .record()
            .expect("once announced and the runs measured, the real cost can be recorded");
        assert!(
            recorded.contains("backend run(s) in"),
            "the receipt reports what was actually spent: {recorded}"
        );
    }

    #[allow(non_snake_case)]
    #[test]
    fn a_receipt_BEFORE_the_runs_is_refused_even_though_the_estimate_was_announced() {
        // The half the sibling test above never pinned, and the only half that
        // is REACHABLE in production: `announce` runs inside `preflight::run`,
        // so every ledger `main` can hold has already announced. That made the
        // announced-nothing refusal unreachable, and moving `record` up to just
        // after the preflight — the literal reordering the rustdoc claimed
        // would fail — returned a plausible receipt in the wrong place.
        //
        // The runs must therefore have been MEASURED for a receipt to exist,
        // and that measurement is what a reordering loses.
        let cfg = Config::default();
        let mut ledger = CostLedger::new();
        ledger.announce(&cfg, false);

        let refusal = ledger
            .record()
            .expect_err("a receipt for runs nothing timed is not a receipt");
        assert!(
            refusal.contains("before the runs were measured"),
            "the refusal must name WHICH half is missing — the announcement was made, so a \
             message about the announcement would send the reader to the wrong place: {refusal}"
        );
    }

    #[allow(non_snake_case)]
    #[tokio::test]
    async fn the_measured_interval_is_the_RUNS_and_not_the_work_before_them() {
        // W1: the clock used to start at the end of the preflight, so under
        // `--build-matrix` the certificate's "real cost" silently swallowed
        // four `cargo check` runs. R37's payoff is that `git log -p` over one
        // fixed path is the historical cost series, and a series that includes
        // or excludes four builds depending on a flag is not comparable across
        // releases.
        //
        // The sleep stands in for that work: it happens after the estimate is
        // announced and outside the measured call, so it must NOT be measured.
        let cfg = Config::default();
        let mut ledger = CostLedger::new();
        ledger.announce(&cfg, false);
        std::thread::sleep(std::time::Duration::from_millis(250));
        for _ in 0..backend_runs_of(&cfg) {
            ledger.measure(async {}).await;
        }

        let recorded = ledger.record().expect("announced, and the runs measured");
        assert!(
            recorded.contains("in 0.0s"),
            "the interval must cover the runs alone; 250ms of work done before they started \
             leaked into the receipt: {recorded}"
        );
    }

    #[allow(non_snake_case)]
    #[tokio::test]
    async fn the_measured_interval_ENDS_with_the_runs_and_not_where_record_sits() {
        // The other end of the same defect. Making the START structural fixed
        // half of it: the clock cannot begin before the work, because the work
        // IS the argument. The END stayed a convention about where one call
        // sits — the elapsed time was computed inside `record`, so the interval
        // ran to wherever somebody had put it.
        //
        // That is the same failure the sibling test above pins, mirrored. Moving
        // the feature matrix down so it sits between the runs and `record` — a
        // plausible refactor that violates none of the ordering constraints the
        // caller documents — put four `cargo check` runs back inside the
        // recorded interval, and every test stayed green.
        //
        // Two sleeps, and BOTH ends are pinned by one window. An assertion that
        // only excluded — the old `in 0.0s` — could not tell a correct ledger
        // from one that bills nothing at all: moving the store one line up, so
        // it runs BEFORE the await, is a one-line refactor that compiles, lints
        // clean and reports `0.0s` for every workload forever, and every test
        // stayed green. A cost series that is uniformly zero is broken exactly
        // as badly as one inflated by four `cargo check` runs, which is what
        // three rounds of review were spent on.
        //
        // So the window has to separate THREE states, not two:
        //
        // * correct            — the inside sleep alone, ~0.5s;
        // * inclusion broken   — the interval stops before the work, ~0.0s;
        // * exclusion broken   — the end goes positional again, ~1.0s.
        //
        // The margins are asymmetric on purpose, because a sleep guarantees a
        // LOWER bound and never an upper one. Both broken states therefore fail
        // deterministically: 0.0 is below the floor and can never rise, ~1.0 is
        // above the ceiling and can only rise further. The single direction
        // that could flake is the correct case drifting up past the ceiling,
        // and it is given 400ms of slack — this machine would have to add four
        // tenths of a second of scheduling overhead INSIDE the measured region
        // alone. Half-second sleeps rather than the 250ms the exclusion half
        // used to run with: the receipt formats to one decimal, so a 250/250
        // split leaves only ~150ms before a loaded machine walks out of the
        // window, and a gate whose red is ambiguous teaches nothing — this
        // project has already paid once for a suite that needed five
        // consecutive green runs before anyone believed it again.
        //
        // Both sleeps block the thread rather than yielding, and deliberately:
        // the runtime has nothing else to run here, and using one mechanism on
        // both sides keeps the comparison between them free of any question
        // about timer resolution.
        //
        // The COUNT is pinned here too, in the test that already holds the
        // receipt, because it is the other half of the same claim: the interval
        // is a cost OF a number of runs, and a series whose denominator drifts
        // is as incomparable across releases as one whose numerator does. Only
        // the duration had ever been asserted, so adding seven to the count
        // reported a receipt for ten runs that never happened with all 214
        // tests green. The estimate is the reference because it counts the same
        // runs from the same config by its own route, so the two agreeing is a
        // statement about the runs rather than about one expression.
        //
        // The interval is now one per RUN and the receipt sums them, so a THIRD
        // exclusion joins the two above and is pinned by the same window: work
        // done BETWEEN two runs. That gap did not exist while a single interval
        // spanned the whole batch — everything between the runs was inside it by
        // construction — and it is where a future `measure` that goes back to
        // wrapping the batch would show up, since one interval covering all
        // three sleeps reports ~1.5s.
        const MEASURED_MS: u64 = 500;
        const BETWEEN_MS: u64 = 500;
        const AFTER_MS: u64 = 500;
        const FLOOR_SECS: f64 = 0.4;
        const CEILING_SECS: f64 = 0.9;

        let cfg = Config::default();
        let mut ledger = CostLedger::new();
        let announced = ledger.announce(&cfg, false);
        // The whole measured workload sits in the FIRST run, so the sum the
        // receipt reports stays the same ~500ms the window was calibrated for
        // while the number of intervals matches the announced count.
        ledger
            .measure(async {
                std::thread::sleep(std::time::Duration::from_millis(MEASURED_MS));
            })
            .await;
        std::thread::sleep(std::time::Duration::from_millis(BETWEEN_MS));
        for _ in 1..backend_runs_of(&cfg) {
            ledger.measure(async {}).await;
        }
        std::thread::sleep(std::time::Duration::from_millis(AFTER_MS));

        let recorded = ledger.record().expect("announced, and the runs measured");
        let runs_in = |line: &str| -> usize {
            line.split(" backend run(s) ")
                .next()
                .and_then(|head| head.split_whitespace().last())
                .and_then(|n| n.parse::<usize>().ok())
                .unwrap_or_else(|| {
                    panic!("the line must report a count it can be read from: {line}")
                })
        };
        assert_eq!(
            runs_in(&recorded),
            runs_in(&announced),
            "the receipt must bill the runs the estimate announced for this config. A count \
             that drifts from it is a receipt for work that never happened, and it breaks the \
             series exactly as a wrong duration does: {recorded} / {announced}"
        );
        let seconds = recorded
            .split(" in ")
            .nth(1)
            .and_then(|tail| tail.strip_suffix('s'))
            .and_then(|secs| secs.parse::<f64>().ok())
            .unwrap_or_else(|| {
                panic!("the receipt must report an interval it can be read from: {recorded}")
            });
        assert!(
            (FLOOR_SECS..CEILING_SECS).contains(&seconds),
            "the intervals must cover the runs and NOTHING else. Below {FLOOR_SECS}s the \
             {MEASURED_MS}ms of work handed to `measure` was never billed at all, so every \
             release would record the same zero; at or above {CEILING_SECS}s something outside \
             the runs leaked in — either the {BETWEEN_MS}ms between two of them, which one \
             interval spanning the whole batch would swallow, or the {AFTER_MS}ms after the \
             last, which an end that follows `record` would: {recorded}"
        );
    }

    #[test]
    fn the_proxy_bound_covers_every_run_that_reaches_the_backend() {
        // Two places decide which runs use the backend and they decide it by
        // different means: `announce_cost` DERIVES the set through
        // `RunId::uses_backend`, while `Config::longest_backend_budget` — the
        // bound `raise_proxy` hands the proxy — maxes over three budget fields
        // named by hand. A hand-maintained list that drifts from its derived
        // twin is how this project has already lost guards, and the drift here
        // is silent in the worst direction: a backend run whose budget is
        // longer than the bound makes the PROXY the component that cuts first,
        // which turns a slow-but-legal backend into a harness fault.
        //
        // Nothing in either module compares them, so this does. The derivation
        // is the same one the announcement uses, and the variant list below is
        // wildcard-free, so a `RunId` added later stops the compilation here
        // rather than quietly leaving this assertion over a subset.
        let all = [
            RunId::HappySmall,
            RunId::Large62k,
            RunId::Rotation,
            RunId::Degradation,
            RunId::NoBackend,
        ];
        for id in all {
            match id {
                RunId::HappySmall
                | RunId::Large62k
                | RunId::Rotation
                | RunId::Degradation
                | RunId::NoBackend => {}
            }
        }

        // Each budget raised past the others in turn: pinning only the default
        // would pass against a bound that ignored two of the three fields.
        for raise in [
            |c: &mut Config| c.budgets.happy_secs = 3_601,
            |c: &mut Config| c.budgets.large_payload_secs = 3_602,
            |c: &mut Config| c.budgets.injected_secs = 3_603,
            |c: &mut Config| c.budgets.no_backend_secs = 3_604,
        ] {
            let mut cfg = Config::default();
            raise(&mut cfg);
            let derived = all
                .into_iter()
                .filter(|id| id.uses_backend())
                .map(|id| cfg.budget(id))
                .max()
                .expect("some run reaches the backend");
            assert_eq!(
                cfg.longest_backend_budget(),
                derived,
                "the proxy's upstream bound must be the longest budget among the runs \
                 `RunId::uses_backend` selects. A bound below one of them makes the proxy cut \
                 before the run does, and a proxy cut is reported as a harness fault rather \
                 than as what the run was measuring"
            );
        }
    }

    /// How many backend runs `cfg` announces, by the ledger's own route.
    ///
    /// The ledger requires one measured interval per announced backend run, so
    /// a test that wants a valid receipt has to produce that many. Deriving the
    /// number here rather than writing `3` keeps these tests from asserting a
    /// count they fixed themselves.
    fn backend_runs_of(cfg: &Config) -> usize {
        let _ = cfg;
        stage_e1_run_ids(false)
            .into_iter()
            .filter(|r| r.uses_backend())
            .count()
    }

    #[allow(non_snake_case)]
    #[tokio::test]
    async fn a_receipt_is_refused_when_the_intervals_do_not_MATCH_the_announced_runs() {
        // The composition lever, closed by arithmetic instead of by convention.
        //
        // While one interval spanned the whole batch, WHICH work sat inside it
        // was a property of the call site and of nothing else: two shipped
        // regressions moved neighbouring work in, and every test stayed green
        // both times, because what a test can reach is `measure` while what
        // decides the receipt is what the caller hands it.
        //
        // One interval per run makes the SHAPE of the call site checkable. A
        // caller that wraps the batch again produces one interval for three
        // announced runs; a caller that bills work by adding a measured call
        // produces four. Both are now a refusal rather than a plausible number.
        let cfg = Config::default();
        let announced = backend_runs_of(&cfg);

        let mut too_few = CostLedger::new();
        too_few.announce(&cfg, false);
        too_few.measure(async {}).await;
        let refusal = too_few
            .record()
            .expect_err("one interval for three runs is the batch-wrapping shape");
        assert!(
            refusal.contains("1 interval(s) for 3 announced backend run(s)"),
            "the refusal must report both numbers, since the reader has to see WHICH way the \
             call site drifted: {refusal}"
        );

        let mut too_many = CostLedger::new();
        too_many.announce(&cfg, false);
        for _ in 0..announced + 1 {
            too_many.measure(async {}).await;
        }
        let refusal = too_many
            .record()
            .expect_err("a fourth interval bills work the estimate never announced");
        assert!(
            refusal.contains("4 interval(s) for 3 announced backend run(s)"),
            "and in the other direction too: {refusal}"
        );
    }

    #[tokio::test]
    async fn a_run_that_ends_badly_is_still_measured_so_the_receipt_is_not_withheld() {
        // The count check that closed the composition lever cuts both ways: it
        // refuses a receipt when the intervals do not match the announced runs,
        // so a run that is ANNOUNCED and then never reaches `measure` costs the
        // certificate its cost line — a silent withholding, and one that blames
        // the call site for something the operator did not do.
        //
        // What makes that unreachable is that `measure` records the interval
        // whatever the awaited work yields: it is generic over the output and
        // never looks at it, so a run ending "cannot test" or "skip" is timed
        // exactly like one that passed. This is what fails if a future version
        // decides to record only the runs it considers successful.
        let cfg = Config::default();
        let announced = backend_runs_of(&cfg);
        let mut ledger = CostLedger::new();
        ledger.announce(&cfg, false);
        // Values standing in for the outcomes a run can end with. `measure`
        // returns them untouched, which is the other half of the contract: a
        // ledger cannot be made to swallow a result.
        let outcomes: Vec<Result<&str, &str>> = vec![Err("cannot test"), Err("skip"), Ok("pass")];
        assert_eq!(outcomes.len(), announced, "one outcome per announced run");
        for outcome in outcomes {
            assert_eq!(ledger.measure(async { outcome }).await, outcome);
        }
        let receipt = ledger.record().unwrap_or_else(|refusal| {
            panic!("every announced run was measured, so the receipt must exist: {refusal}")
        });
        assert!(
            receipt.contains(&format!("{announced} backend run(s)")),
            "{receipt}"
        );
    }

    #[test]
    fn with_no_backend_the_announcement_says_nothing_will_be_spent() {
        // A partition where no run reaches the backend has no backend cost, and
        // saying "~0s across the backend runs" would read as an estimate of
        // something rather than as the absence of it.
        let announced = announce_cost(&Config::default(), true);
        assert!(
            announced.contains("no run in this partition reaches the backend"),
            "{announced}"
        );
    }

    #[test]
    fn a_probe_that_is_slow_names_both_possible_causes() {
        let msg = probe_failure_message();
        assert!(
            msg.contains("contention") && msg.contains("cold"),
            "the harness cannot tell them apart, and asserting one would be guessing"
        );
    }

    /// The absorption message's own words — the ONE of the six error paths that
    /// does not share the `workspace_root:` prefix with the other five, and
    /// therefore the only string that can tell "the comparison ran and failed"
    /// apart from "the comparison never ran".
    const ABSORPTION_MESSAGE: &str = "the repository root absorbed the harness";

    #[test]
    fn workspace_absorption_is_detected() {
        // If the repo root ever gains a [workspace] section, the isolation
        // breaks SILENTLY and the crate's nine gate commands start reaching
        // the harness.
        //
        // This test used to hand the check a path that DOES NOT EXIST
        // (`/repo/smoke`). `Command::current_dir` then failed to spawn, the
        // function returned through its FIRST error branch, and the
        // `reported != expected` comparison — which IS the requirement — never
        // ran at all. The assertion could not notice, because ALL SIX error
        // paths carry the `workspace_root:` prefix it was matching on: the test
        // agreed with whatever the function said. A guard over the milestone's
        // highest-blast-radius invariant guarded nothing.
        //
        // So the tree is REAL: a parent that declares `[workspace]`, and a child
        // package inside it. `cargo metadata` run from the child reports the
        // PARENT as its workspace root, which is exactly the shape of an
        // absorbed harness — and the assertion is on the absorption message, the
        // only one of the six that a spawn failure cannot produce.
        let tree = tempdir_with(&[
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"child\"]\nresolver = \"2\"\n",
            ),
            (
                "child/Cargo.toml",
                "[package]\nname = \"child\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
            ),
            ("child/src/lib.rs", "// absorbed on purpose\n"),
        ]);
        let err = check_workspace_isolation(&tree.path().join("child")).unwrap_err();
        assert!(
            err.contains(ABSORPTION_MESSAGE),
            "the check must report the ABSORPTION, not merely name the property it was \
             checking — every one of its six error paths does that: {err}"
        );
    }

    #[test]
    fn an_isolated_package_passes_the_workspace_check() {
        // The other direction, and only the pair proves anything: a check that
        // reported absorption unconditionally would satisfy the test above while
        // refusing every healthy tree, which is the same guard-that-guards-
        // nothing defect wearing the opposite sign. An empty `[workspace]` table
        // is what `smoke/Cargo.toml` itself uses to stay out of any parent.
        let tree = tempdir_with(&[
            (
                "Cargo.toml",
                "[package]\nname = \"lonely\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
                 [workspace]\n",
            ),
            ("src/lib.rs", "// isolated on purpose\n"),
        ]);
        assert!(
            check_workspace_isolation(tree.path()).is_ok(),
            "a package that owns its workspace must pass"
        );
    }

    #[test]
    fn the_preflight_stops_if_smoke_cargo_lock_is_not_actually_tracked() {
        // R8: the negation `!smoke/Cargo.lock` is FRAGILE against any future
        // edit of .gitignore, whose line 3 is a bare `Cargo.lock` matching at
        // every level. A line that stopped applying does not announce
        // itself; a check does.
        let err = check_lock_is_tracked(&repo_where_the_negation_was_removed()).unwrap_err();
        assert!(err.contains("smoke/Cargo.lock"));
    }

    #[tokio::test]
    async fn the_contention_probe_sends_a_real_completion_not_a_manifest_listing() {
        // R27/SD-3: the probe exists to detect that a MAGI instance is holding
        // the backend's inference slots. `GET /api/tags` lists manifests — it
        // never loads a model, never touches the GPU and never enters the
        // inference queue, so a saturated Ollama answers it INSTANTLY. The probe
        // would report "clear" and the harness would walk straight into the
        // contention the step exists to catch.
        //
        // What queues is a completion, so that is what goes on the wire:
        // `max_tokens: 1`, the smallest thing that still has to be generated.
        // Every expectation below is a LITERAL, never a constant this module
        // also defines: a test that reads the same constant as the code agrees
        // with whatever the code says, which is precisely how the isolation
        // guard came to guard nothing.
        let stub = stub_that_records_requests().await;
        let cfg = Config {
            endpoint: stub.url(),
            ..Config::default()
        };
        probe(&cfg, cfg.probe_timeout())
            .await
            .expect("the stub answers immediately");

        let seen = stub.seen();
        let req = seen.first().expect("the probe must send something");
        assert_eq!(
            req.method, "POST",
            "a listing is a GET; a completion is not"
        );
        assert_eq!(
            req.path, "/v1/chat/completions",
            "the probe must hit the path the crate's own provider completes on"
        );
        assert!(
            req.body.contains("\"max_tokens\":1"),
            "the probe must bound its output to one token: {}",
            req.body
        );
        // "a model from the config" is R27's own wording, and asserting THAT
        // rather than a particular name leaves the selection rule free to change
        // without this test having an opinion it was never given.
        let from_config = cfg
            .seats
            .iter()
            .map(|s| s.model.as_str())
            .chain(cfg.fallbacks.iter().map(|f| f.model.as_str()))
            .any(|m| req.body.contains(m));
        assert!(
            from_config,
            "the probe must name a model from the config, or nothing is queued: {}",
            req.body
        );
    }

    #[tokio::test]
    async fn the_backend_step_and_the_probe_step_ask_different_questions() {
        // R26 lists them as two steps, and two steps that send the same request
        // to the same path are one step asked twice: the second learns nothing
        // the first did not already answer. `reachable` asks "is anybody there?"
        // — a listing answers that without loading anything, which is what makes
        // it right THERE and wrong for the probe.
        let stub = stub_that_records_requests().await;
        let cfg = Config {
            endpoint: stub.url(),
            ..Config::default()
        };
        reachable(&cfg.endpoint, cfg.probe_timeout())
            .await
            .expect("the stub answers immediately");
        let after_reachable = stub.seen();
        probe(&cfg, cfg.probe_timeout())
            .await
            .expect("the stub answers immediately");
        let all = stub.seen();

        let reach = after_reachable.first().expect("reachability sent nothing");
        let probed = all
            .get(after_reachable.len())
            .expect("the probe sent nothing");
        assert_eq!(
            (reach.method.as_str(), reach.path.as_str()),
            ("GET", "/api/tags"),
            "reachability stays the cheap listing"
        );
        assert_ne!(
            (&reach.method, &reach.path),
            (&probed.method, &probed.path),
            "the two preflight steps must not be the same request twice"
        );
    }

    #[tokio::test]
    async fn the_window_bound_has_one_entry_per_request_the_preflight_makes() {
        // `PREFLIGHT_BACKEND_WINDOWS` is a MIRROR of this module: the config
        // sums it to bound how long the preflight may spend, and nothing ties
        // the two together at compile time. It was wrong once already, in the
        // direction that reports success -- it covered the probe and dropped
        // the reachability request that spends the same knob -- so the mirror
        // needs a guard that reads the requests instead of the source.
        //
        // A preflight whose every request is answered makes TWO of them: the
        // reachability listing and one probe attempt. The third entry is the
        // probe's widened retry, which only fires when the first attempt does
        // not complete, and `a_cold_model_passes_on_the_second_probe_attempt`
        // is what pins it at exactly one. Adding a bounded backend request to
        // this module without an entry in that list turns this red.
        let stub = stub_that_records_requests().await;
        let cfg = Config {
            endpoint: stub.url(),
            ..Config::default()
        };
        // `--break-proxy` stops the run at the LAST step, after both backend
        // steps have run and before anything binds a port.
        let err = run(&cfg, &[], true, false).await.unwrap_err();
        assert_eq!(err.stage, Stage::Proxy, "{err}");
        const RETRY_ENTRIES: usize = 1;
        assert_eq!(
            stub.seen().len() + RETRY_ENTRIES,
            crate::config::PREFLIGHT_BACKEND_WINDOWS.len(),
            "the window bound must carry one entry per bounded backend request the              preflight makes; it saw {:?}",
            stub.seen()
        );
    }

    #[tokio::test]
    async fn a_cold_model_passes_on_the_second_probe_attempt() {
        // R27's retry is what makes "clone and run" work without
        // pre-warming anything: a cold model loads ONCE, so the second
        // attempt passes. Without a test, the retry is a claim — and the
        // mechanism that reports success is always the one to attack.
        let slow_once = stub_that_is_slow_on_first_request_only().await;
        let cfg = Config {
            endpoint: slow_once.url(),
            ..Config::default()
        };
        assert!(probe(&cfg, Duration::from_millis(50)).await.is_ok());
        assert_eq!(
            slow_once.attempts(),
            2,
            "exactly one retry, with a widened window"
        );
    }

    #[tokio::test]
    async fn an_endpoint_slow_every_time_still_reports_cannot_test() {
        // The retry must not turn a genuinely saturated endpoint into a
        // green run.
        let always_slow = stub_that_is_always_slow().await;
        let cfg = Config {
            endpoint: always_slow.url(),
            ..Config::default()
        };
        let err = probe(&cfg, Duration::from_millis(50)).await.unwrap_err();
        assert!(err.contains("contention") && err.contains("cold"));
    }

    #[tokio::test]
    async fn the_preflight_itself_refuses_a_seat_model_the_backend_does_not_list() {
        // The pure check is worth nothing if the preflight never calls it, and
        // this is the half that decides the exit code: `Stage::Backend` is
        // exit 2, and reaching the runs with a mistyped model is exit 1 — a
        // verdict about the crate for a typo in a TOML file.
        let defaults = Config::default();
        let held: Vec<&str> = defaults
            .seats
            .iter()
            .skip(1)
            .map(|s| s.model.as_str())
            .collect();
        let stub = stub_that_lists_models(&held).await;
        let cfg = Config {
            endpoint: stub.url(),
            ..Config::default()
        };
        let err = run(&cfg, &[], true, false).await.unwrap_err();
        assert_eq!(err.stage, Stage::Backend, "{err}");
        assert!(err.msg.contains(&cfg.seats[0].model), "{err}");
    }

    #[tokio::test]
    async fn a_backend_listing_the_whole_trio_gets_past_the_backend_step() {
        // The other side of the check: it must not refuse a healthy config.
        // `--break-proxy` stops the run at the LAST step, so reaching
        // `Stage::Proxy` is the evidence that backend and probe both passed.
        //
        // A healthy backend holds the rotation candidates as well: the check
        // covers the whole declared pool, not the seats alone.
        let defaults = Config::default();
        let held: Vec<&str> = defaults
            .seats
            .iter()
            .map(|s| s.model.as_str())
            .chain(defaults.fallbacks.iter().map(|f| f.model.as_str()))
            .collect();
        let stub = stub_that_lists_models(&held).await;
        let cfg = Config {
            endpoint: stub.url(),
            ..Config::default()
        };
        let err = run(&cfg, &[], true, false).await.unwrap_err();
        assert_eq!(err.stage, Stage::Proxy, "{err}");
    }

    #[test]
    fn a_seat_model_the_backend_does_not_hold_is_refused_before_any_scenario_runs() {
        // The whole reason this harness exists is the 1-versus-2 distinction:
        // exit 1 is a verdict about the crate, exit 2 is "we could not test".
        // A mistyped model in this file is the operator's typo, and letting it
        // reach the runs turns it into a red row about the crate.
        let cfg = Config::default();
        let held: Vec<String> = cfg.seats.iter().skip(1).map(|s| s.model.clone()).collect();
        let absent = cfg.seats[0].model.clone();
        let err = check_seat_models(&cfg, Some(&held)).unwrap_err();
        assert!(
            err.contains(&absent) && err.contains(&cfg.seats[0].agent),
            "the refusal must name the model AND the seat that declared it, got {err:?}"
        );
    }

    #[test]
    fn a_backend_holding_every_seat_model_passes_the_check() {
        let cfg = Config::default();
        let held: Vec<String> = cfg
            .seats
            .iter()
            .map(|s| s.model.clone())
            .chain(cfg.fallbacks.iter().map(|f| f.model.clone()))
            .collect();
        assert!(check_seat_models(&cfg, Some(&held)).is_ok());
    }

    #[test]
    fn a_rotation_candidate_the_backend_does_not_hold_is_refused_too() {
        // The contention probe names ONE candidate — the last — and only
        // notices its absence through a 404 at probe time. Rotation reaches
        // the FIRST that fits, and the pool is as long as the operator makes
        // it, so every other entry was unchecked: a typo there survived the
        // preflight and produced a red row about the crate from the rotation
        // run, which is the inversion this harness exists to remove.
        let mut cfg = Config::default();
        cfg.fallbacks.push(crate::config::Fallback {
            model: "not-pulled-anywhere:latest".into(),
            lineage: "openai".into(),
        });
        let held: Vec<String> = cfg
            .seats
            .iter()
            .map(|s| s.model.clone())
            .chain(cfg.fallbacks.iter().take(1).map(|f| f.model.clone()))
            .collect();
        let err = check_seat_models(&cfg, Some(&held)).unwrap_err();
        assert!(
            err.contains("not-pulled-anywhere:latest"),
            "the refusal must name the absent candidate, got {err:?}"
        );
    }

    #[test]
    fn an_unreadable_listing_does_not_refuse_a_rotation_candidate_either() {
        // The fail-open boundary is the SAME one the seats keep: only a proven
        // absence refuses. Extending the check must not extend it past that,
        // or a backend merely shaped differently starts refusing every run.
        let mut cfg = Config::default();
        cfg.fallbacks.push(crate::config::Fallback {
            model: "not-pulled-anywhere:latest".into(),
            lineage: "openai".into(),
        });
        assert!(check_seat_models(&cfg, None).is_ok());
    }

    #[test]
    fn a_backend_that_lists_no_models_is_not_read_as_holding_none() {
        // The boundary, declared: only a PROVEN absence refuses. A backend
        // whose listing this harness cannot read has established nothing, and
        // reading "I could not tell" as "it holds nothing" would refuse every
        // run against anything that is not shaped like an Ollama.
        assert!(check_seat_models(&Config::default(), None).is_ok());
    }

    #[test]
    fn the_startup_sweep_removes_only_temps_whose_pid_is_dead() {
        // R36. Cleaning up on exit is the normal path; sweeping on ENTRY is
        // what survives a run that had no exit. But a sweep that ignored the
        // PID would delete the temps of a CONCURRENT harness — turning a
        // cleanup into a race.
        //
        // The live entry is named after THIS process, and that is the whole
        // test. It used to be the literal `magi-smoke-SELF-2-def`: `SELF` does
        // not parse as a `u32`, so the sweep skipped that entry before ever
        // reaching `pid_is_alive` — the directory survived because its name was
        // unparsable, not because its owner was alive. The assertion passed
        // with `pid_is_alive` stubbed to `false`, i.e. with the one mechanism
        // it exists to check completely defeated.
        let live = format!("magi-smoke-{}-2-def", std::process::id());
        let dir = temp_root_with(&[("magi-smoke-999999-1-abc", false), (&live, true)]);
        sweep_stale_temps(&dir);
        assert!(!dir.join("magi-smoke-999999-1-abc").exists());
        assert!(
            dir.join(&live).exists(),
            "a live PID's temps are NOT ours to delete"
        );
    }

    #[test]
    fn a_config_without_seats_is_rejected_by_name_in_the_config_step() {
        // Fail-closed, and it belongs to the step that already asks "do I
        // have anything to test WITH?". It is NOT a new preflight step:
        // adding one would change the R26 order that the first test pins,
        // for a question the config step already owns.
        let err = check_seats(&Config {
            seats: Vec::new(),
            ..Config::default()
        })
        .unwrap_err();
        assert!(
            err.contains("seats") && err.contains("magi-smoke.toml.example"),
            "an empty seat list must name the fix, not just the symptom"
        );
    }

    #[test]
    fn a_config_that_is_not_a_full_trio_is_rejected_as_a_config_fault() {
        // Only ZERO seats used to be rejected. One or two walked past, and the
        // degradation scenario — which asserts that exactly two of three agents
        // answered — then reported a red row about the CRATE for a mistake in a
        // TOML file. That is the 1-versus-2 confusion the harness exists to
        // eliminate, arriving through the config.
        let full = Config::default();
        assert!(check_seats(&full).is_ok(), "the built-in trio must pass");
        for keep in [1, 2] {
            let err = check_seats(&Config {
                seats: full.seats.iter().take(keep).cloned().collect(),
                ..full.clone()
            })
            .unwrap_err();
            assert!(
                err.contains("trio") && err.contains("magi-smoke.toml.example"),
                "{keep} seats must be rejected by name, with the fix: {err:?}"
            );
        }
    }

    #[test]
    fn two_seats_naming_the_same_mage_are_rejected_rather_than_silently_collapsing() {
        // Three entries, two mages: the builder registers one provider per
        // agent, so the duplicate replaces its twin and the run dispatches a
        // duo while the count check is satisfied. Counting alone cannot see it.
        let full = Config::default();
        let duplicated: Vec<Seat> = full
            .seats
            .iter()
            .map(|s| Seat {
                agent: full.seats[0].agent.clone(),
                ..s.clone()
            })
            .collect();
        let err = check_seats(&Config {
            seats: duplicated,
            ..full
        })
        .unwrap_err();
        assert!(err.contains("twice"), "{err:?}");
    }

    #[tokio::test]
    async fn no_backend_skips_the_backend_steps_and_nothing_else() {
        // Both directions, because only the pair proves anything. WITH the
        // flag, the preflight completes against an endpoint that answers
        // nothing — which is what `--no-backend` promises and what it could not
        // do while the flag never reached this function. WITHOUT it, the SAME
        // endpoint is rejected at the Backend stage, so the check was skipped
        // rather than broken.
        assert!(
            run_against_an_unreachable_backend(true).await.is_ok(),
            "--no-backend must not require a backend"
        );
        let err = run_against_an_unreachable_backend(false).await.unwrap_err();
        assert_eq!(
            err.stage,
            Stage::Backend,
            "without the flag the backend check must still cut: {err}"
        );
    }

    #[test]
    fn a_broken_proxy_is_requested_explicitly_and_never_by_accident() {
        // S20 needs the preflight to fail while raising the proxy. Without a
        // way to ASK for that, the scenario is unverifiable and would be
        // reported green by omission — which R25 forbids. The switch is a
        // CLI flag and NOT an env var: `MAGI_SMOKE_*` is a fail-closed
        // namespace whose unknown keys are rejected, and this is a harness
        // self-test hook, not configuration.
        assert!(!crate::Cli::parse_from(["magi-smoke"]).unwrap().break_proxy);
        assert!(
            crate::Cli::parse_from(["magi-smoke", "--break-proxy"])
                .unwrap()
                .break_proxy
        );
        assert!(crate::Cli::parse_from(["magi-smoke", "--break-proxies"]).is_err());
    }

    #[tokio::test]
    async fn a_broken_proxy_makes_the_preflight_say_cannot_test_not_failed() {
        // The whole point of S20: a proxy fault is OURS, never a verdict
        // about the crate. It must exit 2 with the proxy named, and report
        // zero scenarios passed.
        let err = run_with_broken_proxy().await.unwrap_err();
        assert!(err.to_string().contains("proxy"));
        // The code itself is asserted where it is DECIDED — one place, in the
        // report — so this checks the property that belongs here: a preflight
        // failure is an unanswered question, never a verdict about the crate.
        assert_eq!(
            crate::outcome::exit_code(&[crate::outcome::ScenarioState::Skip(err.to_string())]),
            2
        );
    }

    #[test]
    fn a_config_with_no_fallbacks_is_a_config_fault_not_a_crate_verdict() {
        // Without this the rotation run reaches the runner with nowhere to
        // rotate, its scenario fails, and the harness exits 1 — a verdict about
        // the crate — for a section missing from the config file.
        let mut cfg = Config::default();
        cfg.fallbacks.clear();
        let err = check_seats(&cfg).unwrap_err();
        assert!(
            err.contains("[[fallbacks]]"),
            "the message must name the missing section: {err}"
        );
    }

    #[test]
    fn a_fallback_sharing_a_seats_lineage_is_rejected() {
        // Rotation exists to LEAVE a lineage. A candidate on the same one is a
        // rotation that proves nothing while looking like it proved something.
        let mut cfg = Config::default();
        let taken = cfg.seats[0].lineage.clone();
        cfg.fallbacks[0].lineage = taken.clone();
        let err = check_seats(&cfg).unwrap_err();
        assert!(
            err.contains(&taken),
            "the message must name the colliding lineage: {err}"
        );
    }

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn a_backend_that_does_not_hold_the_probe_model_is_INCONCLUSIVE_not_clear() {
        // The blind spot the completion-instead-of-listing fix was supposed to
        // close, re-entering through MODEL selection instead of PATH selection:
        // `probe_model` deliberately names the LAST fallback, the one least
        // likely to be resident, and a model the backend does not hold is
        // rejected on inspection WITHOUT generating. The request never enters
        // the inference queue, so a saturated backend answers it instantly and
        // the probe would report "clear" over the contention it exists to
        // detect. Nothing was measured, so the probe must say so.
        let stub = stub_that_holds_no_model().await;
        let cfg = Config {
            endpoint: stub.url(),
            ..Config::default()
        };
        let err = probe(&cfg, Duration::from_millis(500))
            .await
            .expect_err("a 404 answers nothing about the queue, so it cannot pass as clear");
        assert!(
            err.contains(&cfg.probe_model()),
            "the refusal must name the model that was not held: {err}"
        );
        assert!(
            !err.contains("retried once"),
            "and must NOT be the contention message: nothing was slow, so widening the \
             window would prove nothing either: {err}"
        );
    }

    #[test]
    fn two_seats_sharing_one_lineage_are_rejected() {
        // The symmetric case of the fallback check above, and the one that
        // costs a verdict. With two seats on one lineage, an injected failure
        // is classified `Http` -> `Transport` in `3.2.0` and condemns that
        // lineage RUN-WIDE: both seats lose it, one agent answers, and the
        // degradation scenario — which asserts exactly two of three answered —
        // reports a red row about the crate for a mistake in this file.
        let mut cfg = Config::default();
        let taken = cfg.seats[0].lineage.clone();
        cfg.seats[1].lineage = taken.clone();
        let err = check_seats(&cfg).unwrap_err();
        assert!(
            err.contains(&taken),
            "the message must name the colliding lineage: {err}"
        );
        assert!(
            err.contains(&cfg.seats[1].agent),
            "and the seat that carries it, the way the neighbouring checks name their field: {err}"
        );
    }

    #[test]
    fn the_probe_window_is_configurable_and_defaults_to_ten_seconds() {
        // An idle endpoint answers one token in well under a second even
        // against cloud, so 10 s is an order of magnitude of slack — long
        // enough not to fire on normal network latency, short enough to add
        // no perceptible wait. Configurable for the same reason as
        // everything else here: the harness does not know what "too much"
        // means for someone else's deployment.
        assert_eq!(Config::default().probe_timeout(), Duration::from_secs(10));
    }
}
