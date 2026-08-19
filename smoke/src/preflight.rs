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
        reachable(&cfg.endpoint, cfg.probe_timeout())
            .await
            .map_err(|m| PreflightError::cannot_test(Stage::Backend, m))?; // backend

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
/// with a seat naming something that is not a mage, or with two seats naming the
/// SAME mage.
///
/// **Only the zero case used to be rejected**, and one or two seats walked
/// straight past — which is not a harmless permissiveness: the degradation
/// scenario asserts that exactly two agents answered out of three, so a
/// two-seat config produced a red row about the crate for a mistake in a TOML
/// file. Two seats naming the same mage does the same thing by another route,
/// since the builder registers per agent and the second silently replaces the
/// first.
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

/// Is anybody there at all? One request, bounded by `window`, with no retry:
/// a completely unreachable endpoint is a different failure than a reachable
/// one that is merely slow (see [`probe`] for that case).
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
async fn reachable(endpoint: &str, window: Duration) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(window)
        .build()
        .map_err(|e| format!("backend reachability: {e}"))?;
    let resp = client
        .get(format!("{endpoint}{REACHABILITY_PATH}"))
        .send()
        .await
        .map_err(|e| format!("backend at {endpoint} did not answer: {e}"))?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(format!(
            "backend at {endpoint} answered with status {}",
            resp.status()
        ))
    }
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
/// # A non-2xx answer is still an ANSWER
///
/// A model the backend does not hold replies `404`, and quickly. That says the
/// endpoint is responsive, which is this step's entire question — treating it
/// as a probe failure would report "saturated" for a config naming a model
/// nobody pulled. Only a request that does not COMPLETE in `window` counts
/// against the probe.
///
/// # Parameters
///
/// * `cfg` — the configuration, for the endpoint and the model to name.
/// * `window` — how long the whole request may take.
async fn try_once(cfg: &Config, window: Duration) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(window)
        .build()
        .map_err(|e| format!("probe: {e}"))?;
    let body = serde_json::json!({
        "model": cfg.probe_model(),
        "max_tokens": PROBE_MAX_TOKENS,
        "messages": [{ "role": "user", "content": PROBE_PROMPT }],
    });
    client
        .post(format!("{}{COMPLETIONS_PATH}", cfg.endpoint))
        .json(&body)
        .send()
        .await
        .map(|_| ())
        .map_err(|e| format!("probe: {e}"))
}

/// One trivial completion, retried ONCE with a widened window. The retry is
/// what makes "clone and run" work without pre-warming anything: a cold model
/// loads ONCE, so the second attempt passes.
pub async fn probe(cfg: &Config, window: Duration) -> Result<(), String> {
    if try_once(cfg, window).await.is_ok() {
        return Ok(());
    }
    // `Duration * u32`, NOT the other way round: `Mul<u32> for Duration`
    // exists, `Mul<Duration> for u32` does not. A type error, not a style
    // choice.
    if try_once(cfg, window * PROBE_RETRY_FACTOR).await.is_ok() {
        return Ok(());
    }
    Err(probe_failure_message())
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
    /// How long the runs took, captured by [`measure`](CostLedger::measure)
    /// after the work it was handed finished. `None` until then, and it is what
    /// the receipt reports.
    ///
    /// An elapsed DURATION rather than a start instant, so that neither end of
    /// the interval depends on where a call sits: an instant left
    /// [`record`](CostLedger::record) measuring to its own call site, so the
    /// interval grew by whatever was moved in between.
    measured: Option<std::time::Duration>,
    /// How many backend runs the announcement was about, so the receipt
    /// describes the same set.
    backend_runs: usize,
}

impl CostLedger {
    /// A ledger with nothing announced yet.
    pub fn new() -> Self {
        CostLedger {
            announced: false,
            measured: None,
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
    /// * the runs were never measured — there is no interval to report and
    ///   nothing has been spent, so a number here would be a receipt for work
    ///   that has not happened.
    pub fn record(&self) -> Result<String, String> {
        if !self.announced {
            return Err(
                "the real cost cannot be recorded before the estimate was announced: R31 asks \
                 for the estimate FIRST, because after the spend it is a receipt and before it, \
                 it is a decision the operator can still make"
                    .to_string(),
            );
        }
        let measured = self.measured.ok_or_else(|| {
            "the real cost cannot be recorded before the runs started: there is no interval to \
             report and nothing has been spent yet, so a number here would be a receipt for work \
             that has not happened"
                .to_string()
        })?;
        Ok(format!(
            "{} backend run(s) in {:.1}s",
            self.backend_runs,
            measured.as_secs_f64()
        ))
    }

    /// Times `work` as the runs, and stores the elapsed duration
    /// [`record`](CostLedger::record) reports.
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
    /// What it does NOT guarantee, since an earlier version of this paragraph
    /// claimed there was no ordering left to get wrong: work handed in as part
    /// of `work` is measured, because that is what measuring the argument
    /// means. A caller who passes `async {}` and runs the real work outside
    /// still gets a wrong receipt — a conspicuous `0.0s` for N runs, rather
    /// than a plausible number, which is the direction that fails usefully.
    ///
    /// What that excludes, said as what it is: the git baseline, the spec
    /// build, the fixture audit, the feature matrix's four `cargo check` runs
    /// under `--build-matrix`, and the transparency probe. The last one is a
    /// real round-trip to the backend, so it is excluded not for being local
    /// but for not being one of the N runs the receipt counts — it spends no
    /// tokens, and a series that swallows a probe on some releases and four
    /// builds on others cannot be compared across them.
    ///
    /// # Parameters
    ///
    /// * `work` — the runs, awaited here and returned untouched.
    pub async fn measure<T>(&mut self, work: impl std::future::Future<Output = T>) -> T {
        let started = std::time::Instant::now();
        let out = work.await;
        self.measured = Some(started.elapsed());
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
        run_with_broken_proxy, stub_that_is_always_slow, stub_that_is_slow_on_first_request_only,
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
        ledger.measure(async {}).await;
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
            .expect_err("a receipt for runs that never started is not a receipt");
        assert!(
            refusal.contains("before the runs started"),
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
        ledger.measure(async {}).await;

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
        // The sleep stands in for that work: it happens AFTER the measured call,
        // so it must NOT be measured either.
        let cfg = Config::default();
        let mut ledger = CostLedger::new();
        ledger.announce(&cfg, false);
        ledger.measure(async {}).await;
        std::thread::sleep(std::time::Duration::from_millis(250));

        let recorded = ledger.record().expect("announced, and the runs measured");
        assert!(
            recorded.contains("in 0.0s"),
            "the interval must cover the runs alone; 250ms of work done after they finished \
             leaked into the receipt because the end of the interval followed `record` instead \
             of the runs: {recorded}"
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

    #[test]
    fn the_startup_sweep_removes_only_temps_whose_pid_is_dead() {
        // R36. Cleaning up on exit is the normal path; sweeping on ENTRY is
        // what survives a run that had no exit. But a sweep that ignored the
        // PID would delete the temps of a CONCURRENT harness — turning a
        // cleanup into a race.
        let dir = temp_root_with(&[
            ("magi-smoke-999999-1-abc", false),
            ("magi-smoke-SELF-2-def", true),
        ]);
        sweep_stale_temps(&dir);
        assert!(!dir.join("magi-smoke-999999-1-abc").exists());
        assert!(
            dir.join("magi-smoke-SELF-2-def").exists(),
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
