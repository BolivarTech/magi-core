// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-18

//! Result model and exit codes.
//!
//! Three exit codes, five scenario states. The asymmetry is deliberate: a
//! scenario deliberately excluded from a run is not the same as one that could
//! not run, and collapsing them breaks "never green by omission" in both
//! directions.

use std::path::Path;

/// Exit code for a run in which some scenario contradicted the crate.
pub(crate) const EXIT_FAILED: u8 = 1;
/// Exit code for a run that could not reach a conclusion — something was
/// skipped or timed out. Deliberately DIFFERENT from [`EXIT_FAILED`]: a slow
/// backend reported as a failure sends someone hunting in the code for a
/// problem that is in the cable.
pub(crate) const EXIT_INCONCLUSIVE: u8 = 2;
/// Exit code for a run in which everything that ran passed and nothing was left
/// unanswered.
pub(crate) const EXIT_OK: u8 = 0;

// NOT `Copy`: `Skip` carries its reason, and dropping the reason to keep `Copy`
// would trade the only field the operator can act on for a compiler convenience.
//
// And deliberately NO `From<bool>`: an `Into` conversion at a call site reads as
// a cast, and the difference between FAIL and SKIP is the one thing this type
// exists to keep visible. A caller decides which one it means, in the open.
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

/// Result of one REAL run. Several scenarios read the same run, so the mapping
/// to an exit code is decided HERE, once, instead of at every reporting site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    /// The run finished and produced a verdict.
    Complete,
    /// Still alive and exceeded its budget.
    TimedOut,
    /// The run never STARTED, for a reason that is ours — a bad configuration,
    /// or a provider that would not build.
    ///
    /// Deliberately **not** `Crashed`, and the distinction is load-bearing: a
    /// crash is inconclusive and earns the one retry, while a configuration
    /// fault reproduces exactly, so retrying it doubles the most expensive thing
    /// this harness does to arrive at the same place. Sharing one variant is how
    /// the retry rule lost the ability to tell them apart.
    CannotTest,
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
    ///
    /// **`CannotTest` is not inconclusive either**, for the opposite reason: it
    /// is perfectly conclusive about OUR configuration, and a second attempt
    /// reads the same configuration.
    ///
    /// # Why an exhaustive `match` and not `matches!`
    ///
    /// Review asked whether `CannotTest`'s conclusiveness should be enforced by
    /// the TYPE — a second enum, or a wrapper splitting the two classes — rather
    /// than by this body. It should not: that type would have exactly one
    /// consumer (`runner::attempts_for`), so it would be public structure
    /// carrying no information this function does not, and "no API surface
    /// without a consumer" rules it out.
    ///
    /// What the objection is RIGHT about is the enforcement, and that is bought
    /// here for nothing. `matches!` hides an implicit `_ => false`, so a sixth
    /// variant added tomorrow would silently inherit "conclusive, never retried"
    /// — the retry rule deciding a case nobody decided, which is precisely how
    /// `CannotTest` came to share a variant with a crash in the first place.
    /// Spelling every variant out means that variant does not compile until
    /// someone chooses its side.
    pub fn is_inconclusive(&self) -> bool {
        match self {
            RunOutcome::TimedOut | RunOutcome::PanickedInHarness => true,
            RunOutcome::Complete | RunOutcome::CannotTest | RunOutcome::PanickedInCrate => false,
        }
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
pub fn exit_code(states: &[ScenarioState]) -> u8 {
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
///
/// **`Location` is an `Option` here and stays one.** A panic can report no
/// location — a release build may elide it, and a panic raised through code
/// compiled without location tracking has none to give. The absence is recorded
/// AS an absence and travels to [`classify_panic`], which sends it to the
/// crate's side. Substituting a stand-in string here would fabricate a location
/// nobody measured, and the guess would decide the attribution.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // `None` when the panic carried no location. Kept, not defaulted: see
        // this function's rustdoc, and the `_` arm of `classify_panic` for what
        // it costs.
        let loc = info.location().map(|l| l.file().to_string());
        LAST_PANIC_LOCATION.with(|c| *c.borrow_mut() = loc);
        previous(info);
    }));
}

/// Runs one attempt, turning an unwind into a [`RunOutcome`] instead of killing
/// the harness — and handing back the future's own value when it does not panic.
///
/// **The generic output is not flexibility for its own sake.** The only caller
/// produces a full run result, so a version fixed to `RunOutcome` could not be
/// called at all without throwing that result away: the wrapper would have been
/// a mechanism nobody could use, which is the same inertness this harness exists
/// to catch elsewhere.
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
pub async fn run_catching<T, F>(fut: F) -> Result<T, RunOutcome>
where
    F: std::future::Future<Output = T>,
{
    use futures_util::FutureExt;
    match std::panic::AssertUnwindSafe(fut).catch_unwind().await {
        Ok(value) => Ok(value),
        Err(_) => {
            let loc = LAST_PANIC_LOCATION.with(|c| c.borrow().clone());
            Err(match classify_panic(loc.as_deref()) {
                ScenarioState::Skip(_) => RunOutcome::PanickedInHarness,
                _ => RunOutcome::PanickedInCrate,
            })
        }
    }
}

/// ONLY crates the crate under test does NOT depend on. `reqwest` and `sha2`
/// are used by BOTH, so a panic there may well be the crate misusing them —
/// calling that a harness problem would bury exactly what we came to find.
///
/// # It used to name four crates that fail its own criterion
///
/// `smoke/Cargo.toml` builds `magi-core` with `features = ["ollama"]`, so the
/// crate links `reqwest`, which is built on `hyper` and pulls `hyper-util`,
/// `http-body-util` and `futures-util`. All four were listed here as
/// harness-only, so a panic raised inside hyper from the crate's OWN request
/// path — an invalid header, a body polled after completion, a `HeaderMap`
/// capacity panic — was attributed to the harness, returned as
/// [`ScenarioState::Skip`] and mapped to exit 2. That is the direction
/// [`classify_panic`] documents as the one that hides a defect and reports
/// green, produced by the list written to prevent it.
///
/// Verified against `cargo tree -p magi-core --features ollama`: of the five,
/// only `toml` is genuinely absent from that graph. The narrowing costs the
/// other direction — a real hyper panic from the harness's own proxy is now
/// attributed to the crate — and that is the cheap error the whole function is
/// built around: one investigation that finds nothing, against a buried defect.
///
/// # At module scope, so the criterion is CHECKED
///
/// `every_harness_only_dep_is_ABSENT_from_the_crate_under_tests_own_graph`
/// reads the real graph and fails if a name reappears in it. A criterion that
/// nothing compares against the thing it is about is a comment, not a rule —
/// which is how four names sat here contradicting it.
const HARNESS_ONLY_DEPS: [&str; 5] = [
    "hyper-util",
    "http-body-util",
    "futures-util",
    "hyper",
    "toml",
];

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
/// * `location` — the source file the panic reported, or `None` when it
///   reported none.
///
/// # What `None` means, stated because the code decides it silently
///
/// A panic with **no location at all** is attributed to the CRATE. It is a
/// deliberate choice and not a fallthrough nobody noticed: `None` carries no
/// evidence in either direction, the rule of this function is "`Skip` only on
/// positive identification", and of the two ways to be wrong the cheap one is
/// blaming the crate. That costs one investigation which finds nothing; blaming
/// the harness would hide a real defect behind a `Skip`, which is exit 2 —
/// the code nobody investigates — and report green over it.
///
/// `None` is reachable in ordinary use: a release build may elide location
/// information. `an_unlocatable_panic_defaults_to_crate_defect` pins it.
///
/// # Other limitations, declared next to the detection
///
/// Attribution is by panic **location** — see [`is_harness_source`] for which
/// shapes of path count as ours and which known shapes deliberately do not. A
/// panic in a thread the crate spawned escapes `catch_unwind` entirely and never
/// reaches here at all. `tokio` and `reqwest` are used by the harness AND by the
/// crate, so a panic in one of them is ambiguous by nature and falls to `Fail`;
/// disambiguating it would need backtrace analysis, which is heavy machinery for
/// a rare case.
///
/// # Complexity
///
/// `O(d · m)` for `d` harness-only dependency names against a location of length
/// `m` — one split of the path per name, five names.
pub fn classify_panic(location: Option<&str>) -> ScenarioState {
    match location {
        Some(loc) if is_harness_source(loc) => ScenarioState::Skip(format!(
            "panic inside the harness at {loc}: ours, not the crate's"
        )),
        Some(loc)
            if HARNESS_ONLY_DEPS
                .iter()
                .any(|d| is_dependency_source(loc, d)) =>
        {
            ScenarioState::Skip(format!("panic in a HARNESS-only dependency at {loc}"))
        }
        // Two cases, one answer, and both on purpose: a location that is neither
        // ours nor a harness-only dependency, and `None` — a panic that reported
        // NO location, which a release build can produce. Neither is positive
        // identification, so both fall to the crate's side. See "What `None`
        // means" above for why that is the cheap direction of error.
        _ => ScenarioState::Fail,
    }
}

/// The two path separators a panic location can carry. Both are checked
/// regardless of platform: the harness is developed on Windows and read on
/// Linux CI, and a test that writes a `/` path must classify the same way as
/// the `\` path the same build would really produce.
const PATH_SEPARATORS: [char; 2] = ['/', '\\'];

/// This package's own directory, baked in at COMPILE time by Cargo. It is what
/// makes the ABSOLUTE form of a harness path recognisable — see
/// [`is_harness_source`].
const HARNESS_MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

/// First path segment of a harness source path in its RELATIVE form.
const HARNESS_SOURCE_DIR: &str = "src";

/// Whether a panic location is a file of THIS package's own source.
///
/// # Both forms are accepted, and both were MEASURED
///
/// Cargo passes rustc a path relative to the workspace root for the package
/// being built, and an absolute one for a path dependency. Read out of this
/// project's own `cargo build -v`:
///
/// ```text
/// --crate-name magi_smoke ... 'src\main.rs'                       <- relative
/// --crate-name magi_core  ... 'C:\...\MAGI-Core\src\lib.rs'       <- absolute
/// ```
///
/// and a panic raised in this crate accordingly reports `src\outcome.rs`,
/// confirmed by running the panicking test under `--nocapture` from `smoke/`
/// and again through `--manifest-path` from the repository root.
///
/// The relative form is therefore the one this build produces today. It is
/// **not** the only form a build can produce — the same measurement shows a
/// package compiled as somebody else's path dependency reporting an absolute
/// path — so the absolute form is recognised too, by containment in
/// [`HARNESS_MANIFEST_DIR`]. Without it, a harness panic under such a build
/// would be blamed on the crate.
///
/// # Why containment and not a string prefix
///
/// [`Path::starts_with`] compares whole components, so a SIBLING directory
/// whose name merely begins with this package's — `smoke-other/` — is not
/// contained. The relative arm is a whole first SEGMENT for the same reason:
/// the previous `loc.starts_with("src")` also matched `srcfoo/bar.rs`, and
/// matching too much here is the expensive direction (it hands a crate defect
/// a `Skip`).
///
/// # Direction of error
///
/// Anything this cannot positively identify falls through to `Fail`, which is
/// the rule the whole module follows. Two known cases land there: a Windows
/// path that differs from [`HARNESS_MANIFEST_DIR`] only in letter case, and a
/// build that reports harness source relative to an OUTER workspace root
/// (`smoke/src/outcome.rs`) — a layout the preflight's own workspace-isolation
/// check exists to reject before a run starts.
///
/// # Parameters
///
/// * `location` — the source path the panic reported.
///
/// # Complexity
///
/// `O(m)` in the length of `location`: one component walk, one segment split.
fn is_harness_source(location: &str) -> bool {
    if Path::new(location).starts_with(HARNESS_MANIFEST_DIR) {
        return true;
    }
    location.split(PATH_SEPARATORS).next() == Some(HARNESS_SOURCE_DIR)
}

/// Whether `location` lies inside the source tree of the dependency `dep`, as
/// opposed to merely containing its name somewhere.
///
/// # Why a substring is not good enough
///
/// The previous form was `loc.contains(dep)`, and it matched any path with the
/// name anywhere in it — including `magi-core`'s own sources. A crate file
/// named `hyper_compat.rs`, or a checkout under a directory called `toml`, was
/// attributed to a HARNESS dependency and reported as `Skip`, which buries a
/// crate defect and reports green: the one direction this function's own
/// contract says must never happen.
///
/// # What identifies a dependency, MEASURED rather than assumed
///
/// Cargo compiles a registry dependency from a directory named
/// `{name}-{version}`, and a panic location is the full path to the file inside
/// it. Read out of this harness's own built binary on this platform:
///
/// ```text
/// C:\Users\...\registry\src\index.crates.io-1949cf8c6b5b557f\hyper-1.11.0\src\body\incoming.rs
/// ```
///
/// So the test is a whole path SEGMENT equal to `{dep}-{version}`, with the
/// version recognised by its leading digit. The digit is what keeps `hyper`
/// from claiming `hyper-util-0.1.20`, and what keeps a source file called
/// `hyper_compat.rs` from being a segment at all.
///
/// `magi-core` is a path dependency here, so its panics report an absolute path
/// (`C:\...\MAGI-Core\src\orchestrator.rs`) with no `{name}-{version}` segment
/// anywhere — which is why it can never satisfy this and always falls to
/// `Fail`.
///
/// # Direction of error
///
/// A dependency whose directory does NOT carry a numeric version — a `git`
/// checkout, a `[patch]`, a vendored tree — fails this test and lands on
/// `Fail`. That is the safe side, and the same rule the whole function
/// follows: `Skip` only on positive identification.
///
/// # Parameters
///
/// * `location` — the source path the panic reported.
/// * `dep` — the dependency's crate name, e.g. `"hyper"`.
///
/// # Complexity
///
/// `O(m)` in the length of `location`: one split, one prefix test per segment.
fn is_dependency_source(location: &str, dep: &str) -> bool {
    location.split(PATH_SEPARATORS).any(|segment| {
        segment
            .strip_prefix(dep)
            .and_then(|rest| rest.strip_prefix('-'))
            .is_some_and(|version| version.starts_with(|c: char| c.is_ascii_digit()))
    })
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
        // The two-way model sent a `toml` panic to FAIL, accusing the crate of
        // a fault in a library the HARNESS chose.
        assert!(matches!(
            classify_panic(Some("/deps/toml-0.8.0/src/de.rs")),
            ScenarioState::Skip(_)
        ));
    }

    #[test]
    #[allow(non_snake_case)]
    fn every_harness_only_dep_is_ABSENT_from_the_crate_under_tests_own_graph() {
        // The list's own stated criterion — crates the crate under test does
        // NOT depend on — was never checked against the crate under test. Four
        // of its five names failed it: `smoke/Cargo.toml` builds `magi-core`
        // with `features = ["ollama"]`, so the crate links `reqwest`, which is
        // built on `hyper` and pulls `hyper-util`, `http-body-util` and
        // `futures-util`. A panic raised inside hyper from the crate's OWN
        // request path was therefore attributed to the harness, returned as
        // Skip and mapped to exit 2 — the direction this function's doc names
        // as the one that hides a defect and reports green.
        //
        // Asked of the real dependency graph rather than of a list written
        // here, because a second hand-maintained list is the same defect one
        // step removed: this reads what the SUT actually resolves, so a name
        // that reappears in it fails here instead of being discovered by a
        // buried panic.
        let out = std::process::Command::new("cargo")
            .args([
                "tree",
                "-p",
                "magi-core",
                "--features",
                "ollama",
                "--prefix",
                "none",
                "--no-dedupe",
            ])
            .current_dir(crate::paths::repo_root())
            .output()
            .expect("cargo tree over the crate under test");
        assert!(
            out.status.success(),
            "cargo tree failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let tree = String::from_utf8_lossy(&out.stdout);
        let resolved: std::collections::BTreeSet<&str> = tree
            .lines()
            .filter_map(|l| l.split_whitespace().next())
            .collect();

        for dep in HARNESS_ONLY_DEPS {
            assert!(
                !resolved.contains(dep),
                "{dep:?} is in the crate under test's own dependency graph, so a panic inside \
                 it may be the CRATE misusing it. Calling that a harness fault returns Skip, \
                 which is exit 2, and buries exactly what this harness came to find."
            );
        }
    }

    #[test]
    fn a_crate_path_that_merely_contains_a_dependency_name_is_still_the_crates() {
        // `contains` matched a NAME anywhere in the path, so a `magi-core`
        // source file whose own path happened to spell one was attributed to a
        // harness dependency and reported as Skip — burying a crate defect and
        // reporting green, which is the one direction this function must never
        // fail in.
        //
        // Both platforms' separators, because the harness is written on Windows
        // and read on Linux CI, and the paths are shaped the same way on both.
        for path in [
            r"C:\Users\dev\Projects\MAGI-Core\src\providers\hyper_compat.rs",
            "/home/dev/MAGI-Core/src/providers/hyper_compat.rs",
            // A checkout directory that merely spells a dependency's name.
            r"C:\toml\MAGI-Core\src\orchestrator.rs",
            "/work/futures-utils/MAGI-Core/src/rotation.rs",
        ] {
            assert_eq!(
                classify_panic(Some(path)),
                ScenarioState::Fail,
                "{path} is crate source; only a real {{name}}-{{version}} directory is a \
                 harness dependency"
            );
        }
    }

    #[test]
    fn a_real_dependency_directory_is_still_recognised_on_both_platforms() {
        // The companion to the test above: narrowing the match must not turn the
        // harness-only arm into dead code, which would send every `hyper` panic
        // to Fail and accuse the crate of a fault in a library the HARNESS chose.
        //
        // The Windows path is the shape MEASURED out of this harness's own built
        // binary, not an invented one. All three shapes are kept and spelled
        // with the one crate that really is harness-only: what this pins is that
        // the ARM still fires, and the arm does not care which name matched.
        for path in [
            concat!(
                r"C:\Users\dev\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f",
                r"\toml-0.8.23\src\de.rs"
            ),
            "/home/dev/.cargo/registry/src/index.crates.io-6f17d22bba15001f/toml-0.8.23/src/lib.rs",
            "/deps/toml-0.8.23/src/ser.rs",
        ] {
            assert!(
                matches!(classify_panic(Some(path)), ScenarioState::Skip(_)),
                "{path} is a harness-only dependency and must not be blamed on the crate"
            );
        }
    }

    #[test]
    fn a_panic_in_a_library_the_CRATE_also_links_is_the_crates() {
        // These four used to be listed as harness-only, which they are not: the
        // crate under test is built with `features = ["ollama"]`, so it links
        // `reqwest`, and `reqwest` is built on `hyper` and pulls the other
        // three. A panic raised inside any of them may be the crate misusing
        // them from its own request path, and calling that a harness fault
        // returns Skip — exit 2, the code nobody investigates.
        //
        // Attributing a genuine harness-side hyper panic to the crate is the
        // other direction, and it is the cheap one: one investigation that
        // finds nothing, against a buried defect reported green.
        for path in [
            "/deps/hyper-1.11.0/src/body/incoming.rs",
            "/deps/hyper-util-0.1.20/src/rt/tokio.rs",
            "/deps/http-body-util-0.1.3/src/full.rs",
            "/deps/futures-util-0.3.32/src/stream/try_stream/mod.rs",
        ] {
            assert_eq!(
                classify_panic(Some(path)),
                ScenarioState::Fail,
                "{path} is in the crate under test's own graph, so it cannot be positively \
                 identified as ours"
            );
        }
    }

    #[test]
    fn a_dependency_name_only_claims_its_own_versioned_directory() {
        // `hyper` must not claim `hyper-util`'s directory, and the leading digit
        // of the version is the whole reason it cannot: `hyper-util-0.1.20`
        // continues with `u`, not a digit. Checked on the helper directly so the
        // property is pinned per (path, name) pair rather than through a
        // classification that both names happen to agree on.
        assert!(is_dependency_source(
            "/deps/hyper-1.11.0/src/lib.rs",
            "hyper"
        ));
        assert!(!is_dependency_source(
            "/deps/hyper-util-0.1.20/src/lib.rs",
            "hyper"
        ));
        assert!(is_dependency_source(
            "/deps/hyper-util-0.1.20/src/lib.rs",
            "hyper-util"
        ));
        // No numeric version: a git checkout or a vendored tree. Positive
        // identification failed, so it falls to the safe side.
        assert!(!is_dependency_source(
            "/git/checkouts/hyper-abc123/src/lib.rs",
            "hyper"
        ));
    }

    #[test]
    fn a_run_that_could_not_start_is_not_inconclusive_and_earns_no_retry() {
        // `cannot_test` used to report `Crashed`, which IS inconclusive, so a
        // configuration fault was retried — doubling the most expensive thing
        // the harness does to read the same configuration twice. The two
        // concepts now have two variants, which is what lets the retry rule
        // tell them apart at all.
        assert!(!RunOutcome::CannotTest.is_inconclusive());
        assert!(RunOutcome::TimedOut.is_inconclusive());
    }

    #[test]
    fn a_failure_exits_1_and_outranks_everything_else() {
        // The 1-vs-2 split is the whole point of this function, and until now
        // only the 2 side was pinned: a mutation sending Fail to 2 passed the
        // suite untouched.
        assert_eq!(exit_code(&[ScenarioState::Fail]), 1);
        assert_eq!(
            exit_code(&[ScenarioState::Fail, ScenarioState::OutOfScope]),
            1
        );
        assert_eq!(
            exit_code(&[
                ScenarioState::Skip("no backend".into()),
                ScenarioState::Fail
            ]),
            1,
            "a contradiction is the strongest thing the run learned; it outranks \
             an unanswered question regardless of order"
        );
    }

    #[test]
    fn out_of_scope_never_changes_an_answer_the_run_already_had() {
        // Checked against a run that is already non-zero, not only against a
        // clean one: OutOfScope must be inert in every direction, not just the
        // convenient one.
        assert_eq!(
            exit_code(&[ScenarioState::Timeout, ScenarioState::OutOfScope]),
            2
        );
        assert_eq!(exit_code(&[ScenarioState::OutOfScope]), 0);
    }

    #[test]
    fn a_panic_in_a_dependency_the_crate_also_uses_is_not_blamed_on_the_harness() {
        // `reqwest` and `tokio` are used by the harness AND by the crate, so a
        // panic there may well be the crate misusing them. Calling it ours would
        // bury exactly what the harness came to find — and nothing pinned that
        // until now, so adding either name to the harness-only list would have
        // passed the whole suite.
        assert_eq!(
            classify_panic(Some("/deps/reqwest-0.13.0/src/async_impl/client.rs")),
            ScenarioState::Fail
        );
        assert_eq!(
            classify_panic(Some("/deps/tokio-1.40.0/src/runtime/mod.rs")),
            ScenarioState::Fail
        );
    }

    #[test]
    fn a_panic_in_the_harnesss_own_source_is_identified_as_ours() {
        // MEASURED, not assumed: a panic raised in this crate reports a path
        // like `src\outcome.rs`, relative to the package root and carrying no
        // crate name. The arm that matched on "magi-smoke" therefore matched
        // nothing, and every harness panic was blamed on the crate.
        assert!(matches!(
            classify_panic(Some(r"src\outcome.rs")),
            ScenarioState::Skip(_)
        ));
        assert!(matches!(
            classify_panic(Some("src/proxy.rs")),
            ScenarioState::Skip(_)
        ));
    }

    #[test]
    fn a_harness_panic_reported_with_an_absolute_path_is_still_ours() {
        // Fix round 2, Finding 2: attribution matched `starts_with("src")`,
        // which holds for THIS build invocation (measured: cargo passes
        // `'src\main.rs'` to rustc for the package being built) and not for
        // every one — the same measurement shows a package compiled as
        // somebody else's path dependency getting an ABSOLUTE path
        // (`'C:\...\MAGI-Core\src\lib.rs'` for magi-core). Under such a build a
        // harness panic was blamed on the crate.
        //
        // Built from the real manifest dir rather than a literal, so it is the
        // shape this package would actually report and it works on both
        // platforms.
        let abs = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("outcome.rs");
        assert!(
            matches!(
                classify_panic(Some(&abs.to_string_lossy())),
                ScenarioState::Skip(_)
            ),
            "{} is this package's own source: {:?}",
            abs.display(),
            classify_panic(Some(&abs.to_string_lossy()))
        );
    }

    #[test]
    fn a_path_that_merely_begins_like_harness_source_is_not_harness_source() {
        // The companion, and the reason the widening is containment rather than
        // a string prefix. Over-matching here is the EXPENSIVE direction: it
        // hands a crate defect a Skip, which nobody investigates.
        //
        // `srcfoo/` was matched by the old `starts_with("src")` too — the fix
        // closes both at once.
        assert_eq!(classify_panic(Some("srcfoo/bar.rs")), ScenarioState::Fail);
        let sibling = format!("{}-other/src/lib.rs", env!("CARGO_MANIFEST_DIR"));
        assert_eq!(
            classify_panic(Some(&sibling)),
            ScenarioState::Fail,
            "a sibling directory whose name merely begins with this package's is not ours"
        );
    }

    #[test]
    fn the_crate_under_test_is_still_the_crates_when_it_reports_an_absolute_path() {
        // magi-core is a PATH dependency, and `cargo build -v` shows it
        // compiled from an absolute path — so this is the shape its panics
        // really carry. It sits beside `smoke/`, never inside it, which is what
        // keeps the containment test above from swallowing a crate defect.
        let crate_src = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("smoke/ has a parent: the repository root")
            .join("src")
            .join("orchestrator.rs");
        assert_eq!(
            classify_panic(Some(&crate_src.to_string_lossy())),
            ScenarioState::Fail,
            "{} belongs to the crate under test",
            crate_src.display()
        );
    }

    #[tokio::test]
    async fn a_panic_inside_a_run_is_caught_and_attributed_instead_of_killing_the_harness() {
        // The riskiest function in this module had no test at all: it bridges a
        // SYNC `catch_unwind` to an ASYNC future through a thread-local that a
        // panic hook fills, and every claim about it was reasoning rather than
        // execution. This exercises the whole pipeline — hook, panic, catch,
        // attribution — end to end.
        install_panic_hook();
        // The turbofish names the output type the panicking block never produces.
        // Writing an unreachable value instead would need an `#[allow]`, and this
        // project does not add one to satisfy a compiler it can answer honestly.
        let outcome = run_catching::<RunOutcome, _>(async {
            panic!("simulated panic inside a run");
        })
        .await
        .expect_err("a panicking future must not report a value");
        // The panic's location is this file, which IS harness source, so it is
        // attributed to the harness — and an inconclusive result earns the one
        // retry that a verdict does not.
        assert_eq!(outcome, RunOutcome::PanickedInHarness);
        assert!(outcome.is_inconclusive());
    }

    #[tokio::test]
    async fn a_run_that_finishes_is_returned_untouched() {
        // The companion to the test above: without it, a `run_catching` that
        // always reported a panic would still pass.
        let outcome = run_catching(async { RunOutcome::Complete })
            .await
            .expect("a future that finishes must hand back its value");
        assert_eq!(outcome, RunOutcome::Complete);
        assert!(!outcome.is_inconclusive());
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
