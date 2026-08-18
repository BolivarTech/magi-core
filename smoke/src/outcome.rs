// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-18

//! Result model and exit codes.
//!
//! Three exit codes, five scenario states. The asymmetry is deliberate: a
//! scenario deliberately excluded from a run is not the same as one that could
//! not run, and collapsing them breaks "never green by omission" in both
//! directions.

/// Exit code for a run in which some scenario contradicted the crate.
const EXIT_FAILED: i32 = 1;
/// Exit code for a run that could not reach a conclusion — something was
/// skipped or timed out. Deliberately DIFFERENT from [`EXIT_FAILED`]: a slow
/// backend reported as a failure sends someone hunting in the code for a
/// problem that is in the cable.
const EXIT_INCONCLUSIVE: i32 = 2;
/// Exit code for a run in which everything that ran passed and nothing was left
/// unanswered.
const EXIT_OK: i32 = 0;

// NOT `Copy`: `Skip` carries its reason, and dropping the reason to keep `Copy`
// would trade the only field the operator can act on for a compiler convenience.
/// What became of one scenario.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScenarioState {
    /// The scenario ran and the property held.
    Pass,
    /// The scenario ran and the property did not hold.
    Fail,
    /// The scenario was still alive when its budget ran out.
    Timeout,
    /// **Carries its reason.** A bare `Skip` is "green by omission" wearing
    /// another name: the table would show a row nobody can act on, and the
    /// difference between "could not test" and "did not bother" would be
    /// invisible.
    Skip(String),
    /// Deliberately excluded from THIS run — a backend scenario under
    /// `--no-backend`, for instance. Not a failure, and not an unanswered
    /// question either.
    OutOfScope,
}

impl ScenarioState {
    /// The two-way shorthand. **There is no `From<bool>`**: an `Into` conversion
    /// at a call site reads as a cast, and the difference between FAIL and SKIP
    /// is the one thing this type exists to keep visible.
    ///
    /// # Parameters
    ///
    /// * `ok` — whether the asserted property held.
    pub fn from_bool(ok: bool) -> Self {
        if ok {
            ScenarioState::Pass
        } else {
            ScenarioState::Fail
        }
    }
}

/// Result of one REAL run. Several scenarios read the same run, so the mapping
/// to an exit code is decided HERE, once, instead of at every reporting site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    /// The run finished and produced a verdict.
    Complete,
    /// Still alive and exceeded its budget.
    TimedOut,
    /// The process or task died before finishing.
    Crashed,
    /// A panic attributed to `magi-core`.
    PanickedInCrate,
    /// A panic attributed to the harness, or to a dependency only the harness
    /// uses.
    PanickedInHarness,
}

impl RunOutcome {
    /// Whether a re-run could change the answer — one retry for anything
    /// INCONCLUSIVE, never for a run that produced a verdict.
    ///
    /// **`PanickedInCrate` is a VERDICT, not an inconclusive result** — the
    /// crate broke, and that is exactly what the harness came to find. Retrying
    /// it burns a second full run to arrive at the same place, which is the
    /// same waste the no-retry rule forbids for a completed run.
    pub fn is_inconclusive(&self) -> bool {
        matches!(
            self,
            RunOutcome::TimedOut | RunOutcome::Crashed | RunOutcome::PanickedInHarness
        )
    }
}

/// Collapses every scenario's state into the process exit code.
///
/// Precedence is `Fail` over inconclusive over clean, because a contradiction is
/// the strongest thing the run learned. `OutOfScope` never contributes: a
/// scenario deliberately excluded from this run must not make the automatic job
/// fail forever, which would train everyone to ignore it.
///
/// # Parameters
///
/// * `states` — every scenario's state, in any order.
///
/// # Complexity
///
/// `O(n)` in the number of scenarios, two passes at worst, no allocation.
pub fn exit_code(states: &[ScenarioState]) -> i32 {
    if states.contains(&ScenarioState::Fail) {
        return EXIT_FAILED;
    }
    if states
        .iter()
        .any(|s| matches!(s, ScenarioState::Skip(_) | ScenarioState::Timeout))
    {
        return EXIT_INCONCLUSIVE;
    }
    EXIT_OK
}

thread_local! {
    /// Where the last panic came from. Filled by the hook, read by the wrapper.
    static LAST_PANIC_LOCATION: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

/// Installs the process-wide hook that feeds [`classify_panic`]. Called **once**
/// from `main`, before any run.
///
/// The hook only RECORDS; it does not classify, because a hook runs inside the
/// panicking context and anything it decides there is hard to test. The previous
/// hook is chained so the default message still reaches stderr.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let loc = info.location().map(|l| l.file().to_string());
        LAST_PANIC_LOCATION.with(|c| *c.borrow_mut() = loc);
        previous(info);
    }));
}

/// Runs one attempt, turning an unwind into a [`RunOutcome`] instead of killing
/// the harness.
///
/// **`catch_unwind` is SYNC and the run is ASYNC**, so the bridge is explicit:
/// the future is wrapped in `AssertUnwindSafe` and polled through
/// `FutureExt::catch_unwind`, which catches a panic raised **inside a poll**.
/// Wrapping only the call that CREATES the future would catch nothing, because
/// creating a future runs no user code.
///
/// `AssertUnwindSafe` is sound here for the reason it usually is not: on a panic
/// the run is ABANDONED — its result is discarded, and nothing reads state that
/// the unwind may have left inconsistent.
///
/// # Parameters
///
/// * `fut` — the run to poll to completion.
///
/// # Declared limitation
///
/// It does NOT catch a panic in a thread the crate spawned, nor one crossing
/// FFI, nor an abort. Those end the process, and the exit code is what the
/// operator sees — which is why the README lists this among the known
/// limitations rather than implying full coverage.
pub async fn run_catching<F>(fut: F) -> RunOutcome
where
    F: std::future::Future<Output = RunOutcome>,
{
    use futures_util::FutureExt;
    match std::panic::AssertUnwindSafe(fut).catch_unwind().await {
        Ok(outcome) => outcome,
        Err(_) => {
            let loc = LAST_PANIC_LOCATION.with(|c| c.borrow().clone());
            match classify_panic(loc.as_deref()) {
                ScenarioState::Skip(_) => RunOutcome::PanickedInHarness,
                _ => RunOutcome::PanickedInCrate,
            }
        }
    }
}

/// Attributes a panic to the harness (`Skip`) or to the crate (`Fail`).
///
/// Three-way, not two. The harness's own dependencies are OURS — we chose them;
/// the crate's are ITS, for the same reason.
///
/// **The asymmetry decides every case.** Blaming the crate for a harness bug
/// costs one investigation that finds nothing. Blaming the harness for a CRATE
/// bug hides the defect and reports green — which is the "green by omission"
/// this harness exists to forbid. So: **`Skip` only on positive identification;
/// everything else is `Fail`.**
///
/// # Parameters
///
/// * `location` — the source file the panic reported, if it reported one.
///
/// # Limitations, declared next to the detection
///
/// Attribution is by panic **location**, and release builds can elide it
/// (`None` becomes `Fail`, the safe side). A panic in a thread the crate spawned
/// escapes `catch_unwind` entirely and never reaches here at all. `tokio` and
/// `reqwest` are used by the harness AND by the crate, so a panic in one of them
/// is ambiguous by nature and falls to `Fail`; disambiguating it would need
/// backtrace analysis, which is heavy machinery for a rare case.
///
/// # Complexity
///
/// `O(d · m)` for `d` harness-only dependency names against a location of length
/// `m` — five substring searches over one path.
pub fn classify_panic(location: Option<&str>) -> ScenarioState {
    /// The harness's own crate name, as it appears in a panic location.
    const HARNESS_CRATE: &str = "magi-smoke";
    // ONLY crates the crate under test does NOT depend on. `reqwest` and `sha2`
    // are used by BOTH, so a panic there may well be the crate misusing them —
    // calling that a harness problem would bury exactly what we came to find.
    const HARNESS_ONLY_DEPS: [&str; 5] = [
        "hyper-util",
        "http-body-util",
        "futures-util",
        "hyper",
        "toml",
    ];
    match location {
        Some(loc) if loc.contains(HARNESS_CRATE) => ScenarioState::Skip(format!(
            "panic inside the harness at {loc}: ours, not the crate's"
        )),
        Some(loc) if HARNESS_ONLY_DEPS.iter().any(|d| loc.contains(d)) => {
            ScenarioState::Skip(format!("panic in a HARNESS-only dependency at {loc}"))
        }
        _ => ScenarioState::Fail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_timeout_exits_2_not_1_because_it_contradicted_nothing() {
        // A slow backend returning exit 1 sends someone hunting in the code for
        // a problem that is in the cable.
        assert_eq!(exit_code(&[ScenarioState::Timeout]), 2);
    }

    #[test]
    fn out_of_scope_does_not_affect_the_exit_code() {
        // Scenarios deliberately excluded from the --no-backend run must not
        // make the automatic job fail forever, which would train everyone to
        // ignore it.
        assert_eq!(
            exit_code(&[ScenarioState::Pass, ScenarioState::OutOfScope]),
            0
        );
    }

    #[test]
    fn a_panic_from_magi_core_is_a_crate_defect() {
        assert_eq!(
            classify_panic(Some("/deps/magi-core-4.0.0/src/orchestrator.rs")),
            ScenarioState::Fail
        );
    }

    #[test]
    fn a_panic_from_a_harness_dependency_is_ours_not_the_crates() {
        // The two-way model sent a `hyper` panic to FAIL, accusing the crate of
        // a fault in a library the HARNESS chose.
        assert!(matches!(
            classify_panic(Some("/deps/hyper-1.0.0/src/server.rs")),
            ScenarioState::Skip(_)
        ));
    }

    #[test]
    fn an_unlocatable_panic_defaults_to_crate_defect() {
        // Release builds can elide location info, and a panic in a thread the
        // crate spawned escapes catch_unwind entirely. Misclassifying a harness
        // bug costs an investigation; misclassifying a crate defect buries it,
        // because nobody investigates a SKIP.
        assert_eq!(classify_panic(None), ScenarioState::Fail);
    }
}
