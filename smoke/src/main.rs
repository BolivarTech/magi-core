// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-17

// EVERY module of the harness is declared HERE, in Task 1, even though most of
// the files arrive later. Rust does not compile a file nobody declared, so a task
// that creates `foo.rs` without this line produces a module whose tests never run
// — a Red phase that is green because it does not exist. The tasks that follow
// CREATE the files; none of them has to remember to wire itself in.
mod alias;
mod config; // Task 2
mod external; // Task 10 (S1's outside provider)
mod fixtures; // Task 7 (Manifest lives here)
mod git; // The ONE git status invocation; the three callers keep their policies
mod outcome; // Task 6
mod paths; // Task 1 — repo_root() / smoke_dir()
mod payload; // Task 3
mod preflight; // Task 8
mod proxy; // Tasks 4-5
mod report; // Task 12
mod runner; // Task 9
mod scenarios; // Task 10
#[cfg(test)]
mod testkit;
// The assertion-relaxation mark guard is a TEST-time mechanism, not a runtime
// one: its whole job is to fail `cargo test` when a mark in this tree is
// malformed, and nothing in the binary calls it. Declaring it unconditionally
// would make every item in it dead code in a release build, for no gain.
// (This comment deliberately avoids the trigger word, because the guard
// over-matches on purpose and would flag its own declaration.)
#[cfg(test)]
mod weakened;

/// The CLI, **hand-rolled**: seven flags with no subcommands and no derived
/// types do not justify a dependency, and "prefer the standard library" plus
/// KISS both point the same way here.
///
/// - `--smoke-2` — this is SMOKE #2, so the certificate IS written.
/// - `--no-backend` — only the scenarios tagged `BackendNeed::None`.
/// - `--print-payload-size` — generate the payload, print its size, exit. Used
///   by Task 3 Step 5 to verify the target is reachable against the real tree,
///   without spending a backend.
/// - `--json` — also emit the machine-readable report.
/// - `--break-proxy` — HARNESS SELF-TEST HOOK: the proxy refuses to start, so
///   `S20` can be observed. Not configuration.
/// - `--config <path>` — config file; absent = built-in defaults.
/// - `--build-matrix` — run the four `cargo check` combinations so `S21` has
///   something to read. SLOW; off by default.
/// - `--round <n>` — which round of this release cycle this is, for the
///   certificate. Defaults to `1`.
#[derive(Debug, PartialEq)]
pub struct Cli {
    pub smoke_2: bool,
    pub no_backend: bool,
    pub print_payload_size: bool,
    pub json: bool,
    pub break_proxy: bool,
    pub build_matrix: bool,
    pub config: Option<std::path::PathBuf>,
    /// Which round of this release cycle this run is, for the certificate.
    ///
    /// **Declared, never inferred.** R37 wants it because "a release that
    /// needed three rounds is information about that release", and there is
    /// nothing in a single invocation that could tell which round it belongs
    /// to — the same reason `--smoke-2` is a flag rather than a detection.
    /// Defaults to `1`, which is what a release that needed one round is.
    pub round: u32,
}

/// The round a run belongs to when `--round` is not given: the first.
const DEFAULT_ROUND: u32 = 1;

impl Default for Cli {
    fn default() -> Self {
        Cli {
            smoke_2: false,
            no_backend: false,
            print_payload_size: false,
            json: false,
            break_proxy: false,
            build_matrix: false,
            config: None,
            round: DEFAULT_ROUND,
        }
    }
}

impl Cli {
    /// An unknown flag is an ERROR, never ignored: a typo'd `--no-backends` that
    /// silently ran the full suite would spend a backend nobody asked for.
    pub fn parse_from<I, S>(argv: I) -> Result<Cli, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut cli = Cli::default();
        let mut it = argv.into_iter().map(|s| s.as_ref().to_string()).skip(1);
        while let Some(a) = it.next() {
            match a.as_str() {
                "--smoke-2" => cli.smoke_2 = true,
                "--no-backend" => cli.no_backend = true,
                "--print-payload-size" => cli.print_payload_size = true,
                "--json" => cli.json = true,
                "--break-proxy" => cli.break_proxy = true,
                "--build-matrix" => cli.build_matrix = true,
                "--config" => cli.config = Some(it.next().ok_or("--config needs a path")?.into()),
                "--round" => {
                    let raw = it.next().ok_or("--round needs a number")?;
                    // Rejected rather than defaulted: a certificate saying
                    // "round 1" because the argument was unreadable states a
                    // fact nobody established.
                    cli.round = raw
                        .parse()
                        .map_err(|e| format!("--round {raw:?} is not a number: {e}"))?;
                }
                bad => {
                    return Err(format!(
                        "unknown flag {bad:?}; known: --smoke-2 --no-backend \
                         --print-payload-size --json --break-proxy --build-matrix \
                         --config <path> --round <n>"
                    ))
                }
            }
        }
        Ok(cli)
    }
}

/// The harness's only entry point. Everything it does is visible here; no step
/// hides inside another.
///
/// # Exit codes — the distinction this tool exists to preserve
///
/// `0` every assertion passed · `1` an assertion FAILED, which is a verdict
/// about the crate · `2` could not test, which is a fault of ours: config,
/// fixtures, backend, probe or proxy. Confusing `1` with `2` is the failure
/// `outcome.rs` exists to eliminate, so **every code derived from a report is
/// chosen in ONE place**: [`report::Report::exit_code`].
///
/// Two paths below return `2` without building a report, and both are honest
/// about it: an unparsable command line and `--print-payload-size`. Neither
/// evaluates a scenario, so there is no verdict for either to displace. *(A
/// third such path existed — a certificate that could not be written returned
/// `2` before the table was ever printed, discarding a `Fail` the run had
/// already found. It now travels as a row; see
/// [`report::Report::write_certificate_in`].)*
#[tokio::main]
async fn main() -> std::process::ExitCode {
    // A panic becomes an outcome instead of killing the harness.
    outcome::install_panic_hook();

    let cli = match Cli::parse_from(std::env::args()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return std::process::ExitCode::from(2);
        }
    };
    eprintln!("magi-smoke — mode: {}", alias::MODE);

    // `--print-payload-size` generates the payload against the REAL tree and
    // prints only the byte count, so the target is provably reachable without
    // spending a backend. Handled here and ONLY here, before anything else.
    if cli.print_payload_size {
        let cfg = config::Config::default();
        return match payload::generate(&paths::repo_root(), cfg.payload_target_bytes) {
            Ok(p) => {
                println!("{}", p.bytes);
                std::process::ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::ExitCode::from(2)
            }
        };
    }

    // Cleaning up on exit is the normal path; sweeping on ENTRY is what survives
    // a run that had no exit. Only temps whose PID has no live process go.
    preflight::sweep_stale_temps(&std::env::temp_dir());

    let scenarios = scenarios::e1_scenarios();

    // 1. Config, printed BEFORE anything runs: a run whose configuration is
    //    unstated cannot be read afterwards.
    //
    //    A failure here does NOT return bare. The scenario asserting that an
    //    illegible config is fatal reads exactly this path, so exiting without
    //    evaluating it would leave the only scenario that observes it with
    //    nothing to read.
    let (cfg, origin) = match config::Config::load_or_fail(cli.config.as_deref()) {
        Ok(v) => v,
        Err(e) => {
            let err =
                preflight::PreflightError::cannot_test(preflight::Stage::Config, e.to_string());
            let rows = evaluate_preflight_only(&scenarios, &err);
            return report::Report {
                rows,
                run: cycle_run(&cli),
            }
            .emit();
        }
    };

    eprintln!("{origin}");

    // 2. Preflight, in its fixed order. A failure is ALWAYS exit 2 and reports
    //    zero scenarios as passed — but it still evaluates the scenarios whose
    //    whole property IS what the preflight did.
    let live: Vec<&str> = scenarios.iter().map(|s| s.id).collect();
    let mut ready = match preflight::run(&cfg, &live, cli.break_proxy, cli.no_backend).await {
        Ok(r) => r,
        Err(e) => {
            let rows = evaluate_preflight_only(&scenarios, &e);
            return report::Report {
                rows,
                run: cycle_run(&cli),
            }
            .emit();
        }
    };
    // Announced BEFORE the runs, so a reader knows what it is about to spend.
    eprintln!("{}", ready.cost_announcement);

    // Taken BEFORE anything runs — and BEFORE the feature matrix in particular.
    // The no-trace scenario asks what the harness ADDED, so anything created
    // between the baseline and the final check is invisible to it. Taking the
    // baseline after the matrix grandfathered the matrix's own build directories
    // into it: the check would have reported success over exactly the artifacts
    // it exists to notice. They happen to be gitignored today, so nothing leaked
    // — but the ordering is the thing the check depends on, not the ignore file.
    let status_before = repo_status();

    // 3. The feature matrix, only when asked: four `cargo check` runs are slow,
    //    so the scenario reading it SKIPs unless the flag was passed.
    let matrix = if cli.build_matrix {
        Some(run_feature_matrix())
    } else {
        None
    };

    // 4. The shared runs.
    let specs = match runner::RunSpec::for_stage_e1(&cfg, &paths::repo_root(), cli.no_backend) {
        Ok(s) => s,
        Err(e) => return report::Report::cannot_test(&e.to_string()).emit(),
    };
    let mut run = runner::Runner::new(cfg.clone(), ready.proxy);
    // The one deliberate exception to "every request goes through the proxy",
    // run once, before the first run. Its failure leaves the fields empty, which
    // makes the scenario reading them SKIP — never FAIL.
    run.prime_transparency_probe(&cfg.endpoint).await;
    // ONE measured interval PER BACKEND RUN, and the ledger refuses to produce
    // a receipt unless their number matches the count it announced. That is the
    // point of the loop: it cost two rounds to get the ENDS of the interval
    // right — a start mark that drifted up into the preflight, then a stop
    // computed inside `record` that swallowed whatever was moved in between —
    // and both times the bill was the feature matrix's four `cargo check` runs,
    // billed with every test green, because which work sat inside a single
    // batch-wide interval was decided here and checked nowhere. There is no
    // batch-wide interval left to move a line into, and adding a `measure` call
    // to bill something else is now a refusal instead of a plausible number.
    //
    // The offline run is executed here too but NOT measured: the receipt counts
    // BACKEND runs, so billing a run outside that count would put work inside
    // an interval that reports a different set.
    let mut results = Vec::with_capacity(specs.len());
    for spec in &specs {
        if spec.id.uses_backend() {
            results.push(ready.ledger.measure(run.execute_one(spec)).await);
        } else {
            results.push(run.execute_one(spec).await);
        }
    }

    // R31's second half, and the reason it is read HERE: after the spend. The
    // ledger refuses to answer if nothing was announced first, so the order is
    // a property of the code rather than of where two prints happen to sit.
    // A refusal is printed as itself — a receipt nobody can produce is more
    // informative than a plausible-looking number.
    let real_cost = ready.ledger.record().unwrap_or_else(|refusal| refusal);
    eprintln!("magi-smoke — real cost: {real_cost}");
    // R23's end-of-run count. It is printed even when nothing is unverified,
    // because a corpus reported as clean is information and a silence is not.
    eprintln!("magi-smoke — {}", ready.fixtures.report_line());

    // 5. Evaluate. Each scenario reads ONE source and never touches the network:
    //    that is what makes "one run, many assertions" both cheap and honest.
    let rows = evaluate(
        &scenarios,
        &results,
        run.probe(),
        matrix.as_deref(),
        cli.no_backend,
        status_before.as_deref(),
    );

    // 6. Report, certificate, exit code.
    //
    // The certificate is attempted BEFORE the JSON is rendered so both forms
    // carry the same facts — a failure to write it becomes a row, and a row
    // that exists only in the human table is a second source of truth. It never
    // replaces the verdict: `exit_code` still decides, and a `Fail` outranks it.
    let mut report = report::Report {
        rows,
        run: cycle_run(&cli),
    };
    report.write_certificate_in(
        &paths::repo_root(),
        &report::CertificateFacts {
            version: crate_version(),
            commit: git_commit(),
            date: report::iso_date_utc(std::time::SystemTime::now()),
            mode: alias::MODE,
            cost: real_cost,
            round: cli.round,
            fixtures: ready.fixtures,
        },
    );
    if cli.json {
        println!("{}", report.render_json());
    }
    report.emit()
}

/// Which of the two cycle runs this invocation is.
///
/// There is no auto-detection: guessing which run this is would make the
/// certificate a guess too.
fn cycle_run(cli: &Cli) -> report::CycleRun {
    if cli.smoke_2 {
        report::CycleRun::Second
    } else {
        report::CycleRun::First
    }
}

/// Builds each scenario's context and evaluates it.
///
/// **How the context is built depends on the `Source`, and this is the only
/// place that decides it.** Returns rows for ALL scenarios, never omitting one:
/// a table that silently shrinks is green by omission.
///
/// A scenario filtered out by `--no-backend` is `OutOfScope`, **not `Skip`**.
/// The two mean different things and only one affects the exit code: `Skip` is
/// "I tried and could not test" (exit 2), while `OutOfScope` is "this partition
/// was not asked to run" (exit 0). Marking a deliberate partition as `Skip`
/// would make every `--no-backend` run exit 2 over a fault that did not happen.
///
/// # Parameters
///
/// * `repo_status_before` — the pre-run `git status` baseline, or `None` when
///   it could not be MEASURED. The two are not the same thing, and passing
///   `Some("")` for a measurement that never happened would tell the no-trace
///   scenario the tree was clean — so every pre-existing modification would be
///   attributed to the harness and reported as a `Fail`.
///
/// # Complexity
///
/// `O(s + r)` for `s` scenarios and `r` runs.
fn evaluate(
    scenarios: &[runner::Scenario],
    results: &[runner::RunResult],
    probe: &runner::TransparencyProbe,
    matrix: Option<&[(String, runner::BuildOutcome)]>,
    no_backend: bool,
    repo_status_before: Option<&str>,
) -> Vec<report::AssertionRow> {
    let mut rows = Vec::new();
    for scenario in scenarios {
        // The partition is selected by TAG, never by scenario name: a list
        // maintained by hand stops matching in silence.
        if no_backend && scenario.backend_tag == runner::BackendNeed::Required {
            rows.push(report::AssertionRow {
                scenario_id: scenario.id,
                run_id: config::RunId::NoBackend,
                scenario: "not part of the --no-backend partition",
                state: outcome::ScenarioState::OutOfScope,
                over_budget: None,
            });
            continue;
        }
        let mut ctx = absent_context(config::RunId::NoBackend);
        let mut run_id = config::RunId::NoBackend;
        let mut over_budget = None;
        match scenario.source {
            runner::Source::Run(id) => {
                run_id = id;
                // A run that ran out of time is reported as a TIME failure, not
                // handed to the assertion. Its data is absent for a reason that
                // says nothing about the crate — "the deployment is slower than
                // the cap someone chose" — and letting the assertion read an
                // empty context would render that as an ordinary skip, losing
                // the one distinction this row exists to keep.
                // A run the CRATE panicked in is a verdict, not an unanswered
                // question: the crate broke, which is exactly what the harness
                // came to find. Left to the assertion it would read an empty
                // context and SKIP — exit 2, "a fault of ours" — burying the
                // defect under the one code nobody investigates. This is the
                // dangerous direction of the 1-versus-2 inversion.
                if let Some(r) = results
                    .iter()
                    .find(|r| r.run == id && r.outcome == outcome::RunOutcome::PanickedInCrate)
                {
                    let _ = r;
                    rows.push(report::AssertionRow {
                        scenario_id: scenario.id,
                        run_id: id,
                        scenario: "the crate panicked during this run",
                        state: outcome::ScenarioState::Fail,
                        over_budget: None,
                    });
                    continue;
                }
                // A run that never STARTED is a fault of OURS, and it is the
                // only outcome that reaches an assertion carrying a MISLEADING
                // error: `RunResult::cannot_test` puts our own reason in
                // `error`, where an assertion reads it as if `analyze()` had
                // returned it. S1 asks whether that text names "insufficient
                // agents"; a configuration fault does not, so the row came out
                // `Fail` — exit 1, a verdict about the crate, for a run the
                // crate never entered.
                //
                // ONE arm, and the other two faults of ours need none: both
                // panic outcomes are built with `error: None` and `report:
                // None` (`Runner::execute_one`), so every assertion already
                // skips on them for want of anything to read. `CannotTest` is
                // the single case where absence was replaced by a string that
                // reads like evidence.
                if let Some(r) = results
                    .iter()
                    .find(|r| r.run == id && r.outcome == outcome::RunOutcome::CannotTest)
                {
                    rows.push(report::AssertionRow {
                        scenario_id: scenario.id,
                        run_id: id,
                        scenario: "the run could not be started",
                        state: outcome::ScenarioState::Skip(
                            r.error
                                .clone()
                                .unwrap_or_else(|| "no reason was recorded".to_string()),
                        ),
                        over_budget: None,
                    });
                    continue;
                }
                if let Some(r) = results
                    .iter()
                    .find(|r| r.run == id && r.outcome == outcome::RunOutcome::TimedOut)
                {
                    rows.push(report::AssertionRow {
                        scenario_id: scenario.id,
                        run_id: id,
                        scenario: "the run exceeded its time budget before it could be read",
                        state: outcome::ScenarioState::Timeout,
                        over_budget: r.over_budget,
                    });
                    continue;
                }
                match results.iter().find(|r| r.run == id) {
                    Some(r) => {
                        ctx.run = r.run;
                        ctx.report = r.report.as_ref();
                        ctx.error = r.error.as_deref();
                        ctx.records = &r.records;
                        ctx.proxy_degraded = r.proxy_degraded;
                        ctx.attempts = r.attempts;
                        ctx.over_budget = r.over_budget;
                        ctx.direct_probe_body = probe.direct_response.as_deref();
                        ctx.direct_probe_status = probe.direct_status;
                        ctx.probe_record = probe.record.as_ref();
                        ctx.probe_sent_body = probe.sent_body.as_deref();
                        ctx.injected_agent = r.injected_agent;
                        over_budget = r.over_budget;
                    }
                    // The run this scenario reads did not happen. Its assertions
                    // SKIP with that reason rather than being omitted.
                    None => ctx.run = id,
                }
            }
            // The preflight got far enough to hand back a proxy, so nothing
            // failed: a preflight-scoped scenario reads that as "no error".
            runner::Source::Preflight => {}
            runner::Source::Session => {
                // `records` stays EMPTY, and that is a decision rather than an
                // omission: the one session-scoped scenario reads the repository,
                // not the wire. Every run's records used to be cloned into one
                // slice here for a reader that does not exist — work nothing
                // consumed, and a path no assertion had ever exercised. A session
                // scenario that needs the traffic rebuilds it deliberately, at
                // which point the concatenation gets the reader that justifies it.
                //
                // Passed THROUGH, never wrapped: wrapping it in `Some(..)` made
                // the scenario's own "no baseline, so skip" branch dead code,
                // and its rustdoc said the opposite.
                ctx.repo_status_before = repo_status_before;
            }
            runner::Source::Build => ctx.build_matrix = matrix,
        }
        rows.extend(report::AssertionRow::of(
            scenario.id,
            run_id,
            (scenario.assert_fn)(&ctx),
            over_budget,
        ));
    }
    rows
}

/// A context carrying nothing, for a scenario whose run never happened.
///
/// `attempts: 0` rather than `1` on purpose: zero attempts is what actually
/// happened, and claiming one would describe a run nobody made.
fn absent_context<'a>(run: config::RunId) -> runner::RunContext<'a> {
    runner::RunContext {
        run,
        report: None,
        error: None,
        records: &[],
        proxy_degraded: false,
        attempts: 0,
        over_budget: None,
        direct_probe_body: None,
        direct_probe_status: None,
        probe_record: None,
        probe_sent_body: None,
        injected_agent: None,
        build_matrix: None,
        repo_status_before: None,
    }
}

/// The preflight-failure path.
///
/// Evaluates the preflight-scoped scenarios against the error; every other row
/// is a `Skip` naming the preflight as the reason, because a run that never
/// started cannot say anything about them.
///
/// **Each scenario recognises ITS OWN stage and skips the others.** Four of them
/// read the same error, so if each asserted merely "the preflight failed", all
/// four would pass on any one of the four causes — or worse, three would fail
/// when the one that broke was the fourth. The stage travels in the error's own
/// rendering, which is why they match on it rather than on substrings of prose.
///
/// # Complexity
///
/// `O(s)` in the number of scenarios.
fn evaluate_preflight_only(
    scenarios: &[runner::Scenario],
    err: &preflight::PreflightError,
) -> Vec<report::AssertionRow> {
    let rendered = err.to_string();
    let mut rows = Vec::new();
    for scenario in scenarios {
        if scenario.source == runner::Source::Preflight {
            let mut ctx = absent_context(config::RunId::NoBackend);
            ctx.error = Some(&rendered);
            rows.extend(report::AssertionRow::of(
                scenario.id,
                config::RunId::NoBackend,
                (scenario.assert_fn)(&ctx),
                None,
            ));
        } else {
            rows.push(report::AssertionRow {
                scenario_id: scenario.id,
                run_id: config::RunId::NoBackend,
                scenario: "not evaluated: the preflight stopped before any run",
                state: outcome::ScenarioState::Skip(rendered.clone()),
                over_budget: None,
            });
        }
    }
    rows
}

/// Turns one `cargo` invocation's outcome into a [`runner::BuildOutcome`].
///
/// **A failed BUILD and a failed SPAWN are not the same answer.** The first is
/// data the scenario needs; the second means nothing was learned. Collapsing
/// them — which `.map(|s| s.success()).unwrap_or(false)` did — made a `cargo`
/// that could not be run indistinguishable from four combinations that all
/// refused to compile, and the scenario reading that reported `Fail`: exit 1,
/// a verdict about the crate, over a fault of ours.
///
/// # A refusal is only DATA when it is the refusal under test
///
/// For the two combinations that must NOT compile, `expected` carries the
/// distinctive text their own `compile_error!` prints. A failure whose output
/// does not contain it failed for some OTHER reason — an unreachable registry is
/// the realistic one, since one of those combinations is the only one that has
/// to resolve the published dependency — and that teaches nothing about the
/// guard. Reading it as `DidNotBuild` would let the scenario report `Pass` while
/// the `compile_error!` was broken: green by omission, arrived at through the
/// network. Such a failure is `CouldNotRun`, whose documented meaning is exactly
/// "nothing was learned either way", and the scenario skips on it.
///
/// # Parameters
///
/// * `out` — what `Command::output()` returned.
/// * `expected` — the text this combination's refusal must contain, or `None`
///   for a combination that is expected to build (where any refusal is data).
fn build_outcome(
    out: std::io::Result<std::process::Output>,
    expected: Option<&str>,
) -> runner::BuildOutcome {
    let Ok(out) = out else {
        return runner::BuildOutcome::CouldNotRun;
    };
    if out.status.success() {
        return runner::BuildOutcome::Built;
    }
    let Some(expected) = expected else {
        return runner::BuildOutcome::DidNotBuild;
    };
    let stderr = String::from_utf8_lossy(&out.stderr);
    if stderr.contains(expected) {
        runner::BuildOutcome::DidNotBuild
    } else {
        // Printed, not swallowed: an operator whose matrix went unreadable needs
        // the reason, and the scenario's own skip can only name the combination.
        eprintln!("magi-smoke: cargo refused this combination for another reason:\n{stderr}");
        runner::BuildOutcome::CouldNotRun
    }
}

/// The four feature combinations, and what each one's `cargo check` did.
///
/// A build that FAILS is DATA, not an error: the scenario reading it needs two
/// of them to fail. All four are run so the row is complete. A `cargo` that
/// could not be run at all is a third state — see [`build_outcome`].
///
/// **Every inner `cargo` gets its OWN `CARGO_TARGET_DIR`.** These run while the
/// outer `cargo run` still holds the harness's own `target/`, and this project
/// has already paid for that exact contention: two feature sets sharing one
/// target relink the same binaries and produce link errors that read as code
/// defects. The directories live under the system temp directory
/// ([`paths::feature_matrix_target_dir`]), never inside the checkout.
///
/// **Each failing combination carries the text its own `compile_error!` prints**,
/// so a refusal for a different reason is not mistaken for the refusal under
/// test — see [`build_outcome`].
///
/// # Complexity
///
/// Four `cargo check` invocations, serially. Slow by construction, which is why
/// it sits behind a flag.
fn run_feature_matrix() -> Vec<(String, runner::BuildOutcome)> {
    const NO_FEATURES_TAG: &str = "none";
    FEATURE_MATRIX
        .iter()
        .map(|(combo, expected)| {
            let mut args = vec!["check", "--no-default-features", "--quiet"];
            if !combo.is_empty() {
                args.extend(["--features", combo]);
            }
            let tag = if combo.is_empty() {
                NO_FEATURES_TAG.to_string()
            } else {
                combo.replace(',', "-")
            };
            let outcome = build_outcome(
                std::process::Command::new("cargo")
                    .args(&args)
                    .current_dir(paths::smoke_dir())
                    .env("CARGO_TARGET_DIR", paths::feature_matrix_target_dir(&tag))
                    // The refusal is recognised by MATCHING TEXT in stderr, so
                    // the text must not depend on how the invoking environment
                    // happens to be configured. `cargo` colours its diagnostics
                    // when it believes it is talking to a terminal, and
                    // `CARGO_TERM_COLOR=always` forces that on even through a
                    // pipe — escapes land inside the rendered message, and a
                    // marker that no longer matches turns a combination that
                    // CORRECTLY refused to compile into `CouldNotRun`. The
                    // scenario would then skip forever, silently, which is the
                    // expensive direction: a broken `compile_error!` would stop
                    // being observed at all.
                    .env(CARGO_COLOUR_VAR, CARGO_COLOUR_OFF)
                    .output(),
                *expected,
            );
            (combo.to_string(), outcome)
        })
        .collect()
}

/// Every feature combination the matrix builds: `(features, the text its
/// refusal must contain)`. `None` marks a combination expected to build, where
/// any refusal is already data.
///
/// At the crate root rather than inside [`run_feature_matrix`] because the
/// combination NAMES are a contract with `S21`, which looks its verdicts up by
/// them. While the list was a local, the two sides agreed only by coincidence
/// and a rename on either would have left the scenario waiting for a row that
/// never arrives; `scenarios::e1` now pins the agreement against this list.
pub(crate) const FEATURE_MATRIX: [(&str, Option<&str>); 4] = [
    ("tree", None),
    ("published", None),
    ("tree,published", Some(BOTH_MODES_MARKER)),
    ("", Some(NEITHER_MODE_MARKER)),
];

/// The environment variable that decides whether `cargo` colours its output.
///
/// Set explicitly rather than relied upon: piped output is uncoloured BY
/// DEFAULT, but the default is not the contract — an operator or a CI job with
/// `CARGO_TERM_COLOR=always` exported would otherwise change what
/// [`build_outcome`] reads.
const CARGO_COLOUR_VAR: &str = "CARGO_TERM_COLOR";

/// The value that turns it off, whatever the surrounding environment says.
const CARGO_COLOUR_OFF: &str = "never";

/// A fragment of `alias.rs`'s "both modes selected" `compile_error!`.
///
/// A FRAGMENT rather than the whole sentence: the message wraps across source
/// lines, so `cargo`'s rendering of it is not byte-identical to the literal.
/// `the_matrix_markers_are_the_text_alias_actually_prints` keeps this in step
/// with `alias.rs` instead of trusting that nobody rewords it.
const BOTH_MODES_MARKER: &str = "mutually exclusive";

/// A fragment of `alias.rs`'s "no mode selected" `compile_error!`, for the same
/// reason as [`BOTH_MODES_MARKER`].
const NEITHER_MODE_MARKER: &str = "must be enabled";

/// The `magi-core` version the harness was built against.
///
/// Read from the DEPENDENCY's metadata, not from the harness's own
/// `CARGO_PKG_VERSION`: that would certify the harness's version while claiming
/// to certify the crate's.
///
/// Unlike [`run_feature_matrix`] this needs no colour setting: what is parsed
/// here is the JSON `cargo metadata` writes to STDOUT, and cargo colours
/// diagnostics on stderr — never the machine-readable document it was asked
/// for. The same holds for the `cargo metadata` call inside `preflight`.
fn crate_version() -> String {
    const UNKNOWN: &str = "unknown";
    std::process::Command::new("cargo")
        .args(["metadata", "--format-version", "1"])
        .current_dir(paths::smoke_dir())
        .output()
        .ok()
        .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok())
        .and_then(|v| version_of_crate_under_test(&v, alias::MODE))
        .unwrap_or_else(|| UNKNOWN.to_string())
}

/// The name both dependency modes resolve to. `smoke/Cargo.toml` renames them
/// to `magi_core_tree` and `magi_core_pub`, but the PACKAGE is `magi-core` on
/// both sides, which is why the metadata carries two of them.
const CRATE_UNDER_TEST: &str = "magi-core";

/// Picks the `magi-core` the binary actually links, out of a `cargo metadata`
/// document that carries BOTH of them.
///
/// # Parameters
///
/// * `metadata` — a parsed `cargo metadata --format-version 1` document.
/// * `mode` — which source this binary was built against; [`alias::MODE`].
///
/// # Complexity
///
/// `O(n)` in the number of packages.
fn version_of_crate_under_test(metadata: &serde_json::Value, mode: &str) -> Option<String> {
    let _ = mode;
    metadata["packages"].as_array().and_then(|ps| {
        ps.iter()
            .find(|p| p["name"] == CRATE_UNDER_TEST)
            .and_then(|p| p["version"].as_str().map(str::to_string))
    })
}

/// `git status --porcelain --untracked-files=all` over the repository.
///
/// See [`repo_status_of`], which this delegates to; it exists as its own
/// function so the "could not measure" half is testable against a directory
/// that is not a repository, instead of only against whatever state the real
/// tree happens to be in.
fn repo_status() -> Option<String> {
    repo_status_of(&paths::repo_root())
}

/// The BASELINE's reading of [`git::status_porcelain`] over `dir`.
///
/// The command lives in [`git`]; what lives here is this caller's POLICY, which
/// is the half that must not be shared: a failure to measure is **no baseline**,
/// not an empty one.
///
/// # Returns
///
/// `Some(status)` when git answered — **including `Some("")` for a genuinely
/// clean tree** — and `None` when it could not be spawned, exited non-zero, or
/// answered in something other than UTF-8.
///
/// **The distinction is the whole point.** This used to return `""` for both,
/// and the consequence was concrete: if this first call failed and the no-trace
/// scenario's own later call succeeded, every pre-existing uncommitted line
/// looked like something the harness had added, and the run reported `Fail` —
/// exit 1, a verdict about the crate, for a fault of ours. `None` reaches the
/// scenario as an absent baseline, which is a `Skip`.
///
/// # Parameters
///
/// * `dir` — the working tree to ask about.
fn repo_status_of(dir: &std::path::Path) -> Option<String> {
    git::status_porcelain(dir).ok()
}

/// The commit the harness is running on. Travels INSIDE the certificate.
fn git_commit() -> String {
    const UNKNOWN: &str = "unknown";
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(paths::repo_root())
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| UNKNOWN.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_git_status_that_could_not_run_is_not_reported_as_a_clean_tree() {
        // The two are opposite claims, and collapsing them makes the harness
        // attribute somebody else's uncommitted work to itself.
        assert!(
            repo_status_of(&paths::repo_root()).is_some(),
            "the real repository must be measurable"
        );
        // A directory that is not a git repository: git runs and exits non-zero.
        assert_eq!(
            repo_status_of(&std::env::temp_dir()),
            None,
            "a tree git could not report on is NOT a clean tree"
        );
        // A directory that does not exist at all: git cannot even be spawned.
        assert_eq!(
            repo_status_of(&std::env::temp_dir().join("magi-smoke-no-such-directory-anywhere")),
            None
        );
    }

    #[test]
    fn without_a_baseline_the_no_trace_scenario_skips_instead_of_blaming_the_harness() {
        // The other half of the same defect: `evaluate` wrapped the baseline in
        // `Some(..)` unconditionally, so the scenario's own "no baseline" branch
        // was unreachable and an unmeasurable tree read as an empty one.
        let scenarios = scenarios::e1_scenarios();
        let probe = runner::TransparencyProbe::default();
        let rows = evaluate(&scenarios, &[], &probe, None, false, None);
        let row = rows
            .iter()
            .find(|r| r.scenario_id == "S16")
            .expect("the no-trace scenario always produces a row");
        match &row.state {
            outcome::ScenarioState::Skip(reason) => assert!(
                reason.contains("baseline"),
                "the skip must name the missing baseline: {reason:?}"
            ),
            other => panic!("expected a Skip with no baseline, got {other:?}"),
        }
    }

    /// The five scenarios whose subject is a FAILURE the operator has to ask
    /// for: four preflight stages that only break when the invocation makes
    /// them (`S6` an unreachable backend, `S7` a slow one, `S14` a broken
    /// config, `S20` `--break-proxy`) and the build matrix (`S21`,
    /// `--build-matrix`).
    ///
    /// A plain `cargo run` requests none of them, which is why they are the set
    /// that decides whether exit `0` exists at all.
    const NOT_INDUCED_BY_A_PLAIN_RUN: [&str; 5] = ["S6", "S7", "S14", "S20", "S21"];

    #[test]
    fn a_healthy_run_can_reach_exit_zero() {
        // It could not. `exit_code` returns 2 on any `Skip`, and on a healthy
        // run these five skipped BY DESIGN — so none of the six README
        // invocations could ever return 0, and acceptance criterion 3bis ("E1 is
        // GREEN against 3.2.0") was not demonstrable by any run.
        //
        // The ruling: a scenario whose precondition INVOCATION was not requested
        // is OutOfScope, not Skip. `Skip` keeps its meaning — in partition, but
        // the run did not produce the data it needed — which is what the
        // companion test below pins.
        let probe = runner::TransparencyProbe::default();
        let rows = evaluate(
            &scenarios::e1_scenarios(),
            &[],
            &probe,
            None,
            false,
            Some(""),
        );
        for id in NOT_INDUCED_BY_A_PLAIN_RUN {
            let row = rows
                .iter()
                .find(|r| r.scenario_id == id)
                .unwrap_or_else(|| panic!("{id} always produces a row"));
            assert_eq!(
                row.state,
                outcome::ScenarioState::OutOfScope,
                "{id} was not asked for by this invocation, so it is out of scope — not a \
                 question the run tried and failed to answer: {:?}",
                row.state
            );
        }
        let uninduced: Vec<report::AssertionRow> = rows
            .iter()
            .filter(|r| NOT_INDUCED_BY_A_PLAIN_RUN.contains(&r.scenario_id))
            .cloned()
            .collect();
        assert_eq!(
            report::Report::with(&uninduced).exit_code(),
            0,
            "a run whose only unevaluated rows are ones nobody asked for is a clean run"
        );
    }

    #[test]
    fn a_genuine_skip_still_reports_exit_two() {
        // The distinction is the entire point of the ruling, so collapsing it
        // would trade one blindness for a worse one: a question the run TRIED to
        // answer and could not must still be exit 2. Two of them here.
        //
        // (1) The preflight stopped, so no run happened and every scenario that
        //     reads a run is unanswered.
        let err = preflight::PreflightError::cannot_test(
            preflight::Stage::Backend,
            "backend at http://127.0.0.1:1 did not answer",
        );
        let rows = evaluate_preflight_only(&scenarios::e1_scenarios(), &err);
        assert_eq!(
            report::Report::with(&rows).exit_code(),
            2,
            "a preflight that cut leaves real questions unanswered"
        );

        // (2) The matrix WAS asked for, and cargo could not be run — the harness
        //     tried and could not test, which is not the same as not being asked.
        let probe = runner::TransparencyProbe::default();
        let unrunnable = [
            ("tree".to_string(), runner::BuildOutcome::CouldNotRun),
            (
                "tree,published".to_string(),
                runner::BuildOutcome::CouldNotRun,
            ),
            ("".to_string(), runner::BuildOutcome::CouldNotRun),
        ];
        let rows = evaluate(
            &scenarios::e1_scenarios(),
            &[],
            &probe,
            Some(&unrunnable),
            false,
            Some(""),
        );
        let s21 = rows
            .iter()
            .find(|r| r.scenario_id == "S21")
            .expect("S21 always produces a row");
        assert!(
            matches!(s21.state, outcome::ScenarioState::Skip(_)),
            "a matrix that was requested and could not be built is an unanswered question, \
             not an unasked one: {:?}",
            s21.state
        );
    }

    /// A `cargo metadata` document carrying BOTH `magi-core` packages, which
    /// is what the real one carries: `smoke/Cargo.toml` declares the path and
    /// the registry dependency side by side, and cargo resolves an optional
    /// dependency whether or not its feature is on.
    ///
    /// The two versions are deliberately far apart so a wrong pick cannot be
    /// mistaken for a right one.
    ///
    /// # Parameters
    ///
    /// * `registry_first` — which of the two the array lists first, so the same
    ///   assertions can be run against both orderings.
    fn metadata_with_both_magi_cores(registry_first: bool) -> serde_json::Value {
        let from_tree = serde_json::json!({
            "name": CRATE_UNDER_TEST, "version": "9.9.9", "source": serde_json::Value::Null,
        });
        let from_registry = serde_json::json!({
            "name": CRATE_UNDER_TEST, "version": "3.2.0",
            "source": "registry+https://github.com/rust-lang/crates.io-index",
        });
        let pair = if registry_first {
            [from_registry, from_tree]
        } else {
            [from_tree, from_registry]
        };
        serde_json::json!({
            "packages": [
                pair[0].clone(),
                pair[1].clone(),
                { "name": "magi-smoke", "version": "0.1.0", "source": serde_json::Value::Null },
            ]
        })
    }

    #[test]
    fn the_certificate_names_the_magi_core_the_binary_actually_links() {
        // The certificate's central fact, decided by an undocumented ordering:
        // both dependencies are the PACKAGE `magi-core`, so the metadata holds
        // two of them, and `find` took whichever cargo listed first. In
        // `published` mode that could name the working tree's version — a
        // version nobody tested — on a document the harness does not control
        // the order of.
        //
        // Both orderings are asserted, because an answer that depends on the
        // order is exactly what is being ruled out.
        for registry_first in [false, true] {
            let md = metadata_with_both_magi_cores(registry_first);
            assert_eq!(
                version_of_crate_under_test(&md, "tree").as_deref(),
                Some("9.9.9"),
                "the tree mode links the PATH dependency, whose source is null \
                 (registry_first = {registry_first})"
            );
            assert_eq!(
                version_of_crate_under_test(&md, "published").as_deref(),
                Some("3.2.0"),
                "the published mode links the REGISTRY dependency \
                 (registry_first = {registry_first})"
            );
        }
    }

    #[test]
    fn the_version_lookup_is_driven_by_the_mode_this_binary_was_built_in() {
        // The pure function above is only correct if the caller hands it the
        // mode the binary was actually built with. `alias::MODE` is that fact,
        // and this ties the two together instead of trusting two spellings to
        // agree.
        let md = metadata_with_both_magi_cores(false);
        let expected = if cfg!(feature = "tree") {
            "9.9.9"
        } else {
            "3.2.0"
        };
        assert_eq!(
            version_of_crate_under_test(&md, alias::MODE).as_deref(),
            Some(expected),
            "alias::MODE is {:?}, which must select the source this binary links",
            alias::MODE
        );
    }

    #[test]
    fn a_run_that_could_not_start_is_never_a_verdict_about_the_crate() {
        // `RunResult::cannot_test`'s own rustdoc says "assertions reading it
        // SKIP", and they did not: `evaluate` gave `CannotTest` no arm, so the
        // reason string reached the assertion as if it were a typed failure
        // from `analyze()`. S1 asks whether that error names "insufficient
        // agents"; a configuration fault does not, so the row came out `Fail`
        // — exit 1, a verdict about the crate, for a run the crate never
        // entered. That is the exit-code inversion this harness exists to
        // eliminate, in the one direction nobody investigates.
        let probe = runner::TransparencyProbe::default();
        let could_not_start = [runner::RunResult::cannot_test(
            config::RunId::NoBackend,
            "the external provider would not build".to_string(),
        )];
        let rows = evaluate(
            &scenarios::e1_scenarios(),
            &could_not_start,
            &probe,
            None,
            false,
            Some(""),
        );
        let s1 = rows
            .iter()
            .find(|r| r.scenario_id == "S1")
            .expect("S1 always produces a row");
        match &s1.state {
            outcome::ScenarioState::Skip(reason) => assert!(
                reason.contains("would not build"),
                "the skip must carry the reason the run could not start: {reason:?}"
            ),
            other => panic!(
                "a run that never started is a fault of OURS and must skip, not fail: {other:?}"
            ),
        }
        assert_eq!(
            report::Report::with(std::slice::from_ref(s1)).exit_code(),
            2,
            "could not test is exit 2; reporting 1 here blames the crate for our own \
             configuration"
        );
    }

    thread_local! {
        /// How many records the counting scenario below saw.
        ///
        /// A thread-local and not a captured variable: `Scenario::assert_fn` is
        /// a bare `fn` pointer, so it cannot close over anything. Thread-local
        /// rather than static because `cargo test` runs tests on several
        /// threads, and a shared cell would let one test read another's count.
        static COUNTED_RECORDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    /// A run result carrying `n` recorded requests and nothing else.
    fn run_with(id: config::RunId, n: usize) -> runner::RunResult {
        runner::RunResult {
            run: id,
            outcome: outcome::RunOutcome::Complete,
            report: None,
            error: None,
            records: (0..n)
                .map(|_| proxy::RequestRecord {
                    path: runner::COMPLETIONS_PATH.to_string(),
                    body_sha256: String::new(),
                    response_status: 200,
                    response_recorded: true,
                    response_sha256: String::new(),
                    response_body: Vec::new(),
                })
                .collect(),
            proxy_degraded: false,
            attempts: 1,
            over_budget: None,
            injected_agent: None,
        }
    }

    #[allow(non_snake_case)]
    #[test]
    fn a_scenario_reading_one_run_never_sees_ANOTHER_runs_records() {
        // Every run shares ONE proxy, so the isolation comes from
        // `records_since(mark)`. If `evaluate` built the context from the whole
        // registry, an assertion about the happy-path run would count the
        // rotation run's requests too — and be right about a set nobody asked
        // it about.
        //
        // Pinned at the `evaluate` level, which is where the context is
        // assembled. The proxy's own `records_since` is tested in its module;
        // what was untested is the wiring between them.
        let probe = runner::TransparencyProbe::default();
        COUNTED_RECORDS.with(|c| c.set(usize::MAX));
        let scenario = runner::Scenario {
            id: "S-counting",
            source: runner::Source::Run(config::RunId::HappySmall),
            backend_tag: runner::BackendNeed::Required,
            assert_fn: |ctx| {
                COUNTED_RECORDS.with(|c| c.set(ctx.records.len()));
                vec![runner::assert_that("counted its own run's records", true)]
            },
        };
        evaluate(
            &[scenario],
            &[
                run_with(config::RunId::HappySmall, 2),
                run_with(config::RunId::Rotation, 5),
            ],
            &probe,
            None,
            false,
            None,
        );
        assert_eq!(
            COUNTED_RECORDS.with(|c| c.get()),
            2,
            "it must see 2, not 7: the other run's traffic is not its evidence"
        );
    }

    #[allow(non_snake_case)]
    #[test]
    fn a_scenario_filtered_out_by_no_backend_is_OUT_OF_SCOPE_not_omitted() {
        // "The table never shrinks in silence." `evaluate` uses `continue` for
        // a filtered scenario, which is correct — but nothing asserted that
        // every scenario still produced a row, so a filter that dropped one
        // would have been invisible: green by omission, which R25 forbids.
        let scenarios = scenarios::e1_scenarios();
        let probe = runner::TransparencyProbe::default();
        let rows = evaluate(&scenarios, &[], &probe, None, true, Some(""));

        for scenario in &scenarios {
            assert!(
                rows.iter().any(|r| r.scenario_id == scenario.id),
                "{} produced no row at all under --no-backend",
                scenario.id
            );
        }
        assert!(
            rows.len() >= scenarios.len(),
            "one scenario produces several assertions, so the table can only grow: {} rows \
             for {} scenarios",
            rows.len(),
            scenarios.len()
        );
        assert!(
            rows.iter()
                .any(|r| r.state == outcome::ScenarioState::OutOfScope),
            "a partition nobody asked to run is OutOfScope, not a failure to test"
        );
    }

    #[test]
    fn a_cargo_that_could_not_be_spawned_is_not_a_failed_build() {
        // A real `ExitStatus` on every side, not a stand-in.
        let quiet = |args: &[&str]| std::process::Command::new("cargo").args(args).output();
        assert_eq!(
            build_outcome(quiet(&["--version"]), None),
            runner::BuildOutcome::Built
        );
        assert_eq!(
            build_outcome(quiet(&["--magi-smoke-no-such-flag"]), None),
            runner::BuildOutcome::DidNotBuild,
            "cargo ran and refused: that IS data the scenario needs"
        );
        assert_eq!(
            build_outcome(
                std::process::Command::new("magi-smoke-no-such-executable-anywhere").output(),
                None
            ),
            runner::BuildOutcome::CouldNotRun,
            "a spawn failure teaches nothing about the crate and must not read as a \
             combination that refused to compile"
        );
    }

    #[test]
    fn a_refusal_for_another_reason_is_not_the_refusal_under_test() {
        // The hole this closes: the two combinations that must not compile are
        // also the ones that need the registry, so an unreachable crates.io made
        // them fail for a NETWORK reason — and the scenario read that as proof
        // the mutual-exclusion guard fired. It would have reported Pass with the
        // `compile_error!` deleted.
        let refused = std::process::Command::new("cargo")
            .args(["--magi-smoke-no-such-flag"])
            .output();
        assert_eq!(
            build_outcome(refused, Some(BOTH_MODES_MARKER)),
            runner::BuildOutcome::CouldNotRun,
            "a refusal whose output does not name the guard teaches nothing about the guard"
        );
    }

    #[test]
    fn the_matrix_markers_are_the_text_alias_actually_prints() {
        // Two hand-written strings that must agree with a file nobody edits
        // together with this one. Without this, rewording either `compile_error!`
        // turns every failing combination into CouldNotRun and the scenario skips
        // forever — silently, which is the direction that costs the most.
        const ALIAS_SRC: &str = include_str!("alias.rs");
        assert!(
            ALIAS_SRC.contains(BOTH_MODES_MARKER),
            "alias.rs no longer prints {BOTH_MODES_MARKER:?}"
        );
        assert!(
            ALIAS_SRC.contains(NEITHER_MODE_MARKER),
            "alias.rs no longer prints {NEITHER_MODE_MARKER:?}"
        );
    }
}
