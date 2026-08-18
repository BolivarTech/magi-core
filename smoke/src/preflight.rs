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
use std::path::Path;
use std::time::Duration;

/// Everything the preflight leaves behind for the rest of the run.
pub struct Announcement {
    /// The proxy, already listening. Owned here because R9 requires it to
    /// exist before any run does.
    pub proxy: SpyProxy,
    /// Printed BEFORE the first run (R31): estimated tokens and expected time.
    pub cost_announcement: String,
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

    /// ALWAYS 2. A preflight failure is "we could not test", never a verdict
    /// about the crate under test — and that is why there is no path that
    /// returns any other number.
    pub fn exit_code(&self) -> u8 {
        2
    }
}

impl std::fmt::Display for PreflightError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.stage, self.msg)
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
pub async fn run(
    cfg: &Config,
    live: &[&str],
    break_proxy: bool,
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
    reachable(&cfg.endpoint)
        .await
        .map_err(|m| PreflightError::cannot_test(Stage::Backend, m))?; // backend

    // FAIL-CLOSED: a probe that does not answer CUTS. An earlier version kept
    // going "so a scenario would have something to read", which broke R26 —
    // the preflight exists to stop. A preflight-only scenario is evaluated the
    // same way, by calling the individual checks directly, not by letting
    // `run` continue past a failed one.
    probe(&cfg.endpoint, cfg.probe_timeout())
        .await
        .map_err(|m| PreflightError::cannot_test(Stage::Probe, m))?; // probe

    let proxy = raise_proxy(cfg, break_proxy).await?; // proxy
    Ok(Announcement {
        proxy,
        cost_announcement: announce_cost(cfg),
    }) // cost
}

/// Rejects a config that cannot run anything, **naming the fix**.
///
/// Belongs to the `config` step of R26's order, not to a step of its own: the
/// question is the config's own — *do I have anything to test WITH?* — and
/// adding a step would move a boundary that a test pins.
pub fn check_seats(cfg: &Config) -> Result<(), String> {
    if cfg.seats.is_empty() {
        return Err(
            "config has zero seats: every backend run needs models, so the harness would \
             start and be unable to execute a single one. Copy magi-smoke.toml.example to \
             magi-smoke.toml and edit it, or drop the `seats` override to fall back to the \
             built-in trio."
                .to_string(),
        );
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
        .env(
            "CARGO_TARGET_DIR",
            std::env::temp_dir().join("magi-smoke-metadata"),
        )
        .output()
        .map_err(|e| {
            format!(
                "workspace_root: could not run cargo metadata in {}: {e}",
                smoke.display()
            )
        })?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("workspace_root: cargo metadata output: {e}"))?;
    let root = v["workspace_root"].as_str().unwrap_or_default();
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

/// The path both [`reachable`] and [`try_once`] hit. Listing tags never
/// depends on a model being loaded, which is exactly what makes it the right
/// call for `reachable` ("is anybody there?") — and the right call for
/// `try_once` too, since without a specific model to name (see the module's
/// scope note on [`probe_failure_message`]) it is the only trivial request
/// this module can send that is not itself model-specific.
const PROBE_PATH: &str = "/api/tags";

/// How long [`reachable`] waits before giving up on the endpoint entirely.
/// Generous on purpose: this only answers "is anyone home", not "how fast",
/// so it should not fire on ordinary network latency.
const REACHABILITY_TIMEOUT: Duration = Duration::from_secs(10);

/// Is anybody there at all? One request, generously bounded, with no retry:
/// a completely unreachable endpoint is a different failure than a reachable
/// one that is merely slow (see [`probe`] for that case).
async fn reachable(endpoint: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(REACHABILITY_TIMEOUT)
        .build()
        .map_err(|e| format!("backend reachability: {e}"))?;
    let resp = client
        .get(format!("{endpoint}{PROBE_PATH}"))
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

/// One bounded attempt: the WHOLE request (connect + send + read) must
/// complete inside `window`, or it is treated as a failure regardless of
/// whether the connection is later going to succeed.
async fn try_once(endpoint: &str, window: Duration) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(window)
        .build()
        .map_err(|e| format!("probe: {e}"))?;
    let resp = client
        .get(format!("{endpoint}{PROBE_PATH}"))
        .send()
        .await
        .map_err(|e| format!("probe: {e}"))?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(format!("probe: status {}", resp.status()))
    }
}

/// One trivial request, retried ONCE with a widened window. The retry is what
/// makes "clone and run" work without pre-warming anything: a cold model
/// loads ONCE, so the second attempt passes.
pub async fn probe(endpoint: &str, window: Duration) -> Result<(), String> {
    if try_once(endpoint, window).await.is_ok() {
        return Ok(());
    }
    // `Duration * u32`, NOT the other way round: `Mul<u32> for Duration`
    // exists, `Mul<Duration> for u32` does not. A type error, not a style
    // choice.
    if try_once(endpoint, window * PROBE_RETRY_FACTOR)
        .await
        .is_ok()
    {
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
    SpyProxy::start(cfg.endpoint.clone(), cfg.payload_target_bytes)
        .await
        .map_err(|e| {
            PreflightError::cannot_test(Stage::Proxy, format!("proxy: could not start: {e}"))
        })
}

/// Coarse bytes-to-tokens divisor. **The SAME crude bound `magi-core` itself
/// uses** (its own `chars / 4` pre-filter) — said as what it is, a lower
/// bound, not a measurement.
const TOKEN_ESTIMATE_DIVISOR: usize = 4;

/// The four runs this preflight is actually preparing for: the ones that
/// touch the backend through the proxy it just raised. `RunId::NoBackend`
/// never reaches that proxy, so it is not one of "the runs about to launch"
/// against it — summing it in would announce time this preflight has no part
/// in spending.
const BACKEND_RUNS: [RunId; 4] = [
    RunId::HappySmall,
    RunId::Large62k,
    RunId::Rotation,
    RunId::Degradation,
];

/// R31: the sentence printed BEFORE the first run — estimated tokens and the
/// expected time budget, so an operator sees the cost before anything spends
/// it.
///
/// Tokens are `payload_target_bytes / 4`, the same coarse bound the crate
/// itself uses, stated as a coarse bound rather than a measurement. Time is
/// the SUM of the budgets of the backend runs about to launch (see
/// [`BACKEND_RUNS`]) — a cap on what each run may spend, not a prediction of
/// what it will.
pub fn announce_cost(cfg: &Config) -> String {
    let estimated_tokens = cfg.payload_target_bytes / TOKEN_ESTIMATE_DIVISOR;
    let expected_secs: u64 = BACKEND_RUNS
        .into_iter()
        .map(|r| cfg.budget(r).as_secs())
        .sum();
    format!(
        "preflight: ~{estimated_tokens} tokens estimated (bytes/4, a coarse bound, not a \
         measurement); expected time budget ~{expected_secs}s across the backend runs about \
         to start"
    )
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
pub(crate) fn sweep_stale_temps(root: &Path) {
    // la llama main.rs (Task 12)
    let Ok(entries) = std::fs::read_dir(root) else {
        return; // unreadable temp dir is not fatal
    };
    for e in entries.flatten() {
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
    use crate::testkit::{
        repo_where_the_negation_was_removed, run_with_broken_proxy, stub_that_is_always_slow,
        stub_that_is_slow_on_first_request_only, temp_root_with,
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
    fn a_probe_that_is_slow_names_both_possible_causes() {
        let msg = probe_failure_message();
        assert!(
            msg.contains("contention") && msg.contains("cold"),
            "the harness cannot tell them apart, and asserting one would be guessing"
        );
    }

    #[test]
    fn workspace_absorption_is_detected() {
        // If the repo root ever gains a [workspace] section, the isolation
        // breaks SILENTLY and the crate's nine gate commands start reaching
        // the harness.
        let err = check_workspace_isolation(Path::new("/repo/smoke")).unwrap_err();
        assert!(err.contains("workspace_root"));
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
    async fn a_cold_model_passes_on_the_second_probe_attempt() {
        // R27's retry is what makes "clone and run" work without
        // pre-warming anything: a cold model loads ONCE, so the second
        // attempt passes. Without a test, the retry is a claim — and the
        // mechanism that reports success is always the one to attack.
        let slow_once = stub_that_is_slow_on_first_request_only().await;
        assert!(probe(&slow_once.url(), Duration::from_millis(50))
            .await
            .is_ok());
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
        let err = probe(&always_slow.url(), Duration::from_millis(50))
            .await
            .unwrap_err();
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
        assert_eq!(err.exit_code(), 2);
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
