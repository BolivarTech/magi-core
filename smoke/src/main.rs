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
#[derive(Debug, Default, PartialEq)]
pub struct Cli {
    pub smoke_2: bool,
    pub no_backend: bool,
    pub print_payload_size: bool,
    pub json: bool,
    pub break_proxy: bool,
    pub build_matrix: bool,
    pub config: Option<std::path::PathBuf>,
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
                bad => {
                    return Err(format!(
                        "unknown flag {bad:?}; known: --smoke-2 --no-backend \
                         --print-payload-size --json --break-proxy --build-matrix \
                         --config <path>"
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
/// `outcome.rs` exists to eliminate, so the code is chosen in ONE place:
/// [`report::Report::exit_code`].
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
    let (cfg, origin) = match config::Config::load_or_default(cli.config.as_deref()) {
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
    let ready = match preflight::run(&cfg, &live, cli.break_proxy).await {
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
    let results = run.execute(&specs).await;

    // 5. Evaluate. Each scenario reads ONE source and never touches the network:
    //    that is what makes "one run, many assertions" both cheap and honest.
    let rows = evaluate(
        &scenarios,
        &results,
        run.probe(),
        matrix.as_deref(),
        cli.no_backend,
    );

    // 6. Report, certificate, exit code.
    let report = report::Report {
        rows,
        run: cycle_run(&cli),
    };
    if cli.json {
        println!("{}", report.render_json());
    }
    if let Some(text) = report.render_certificate(&crate_version(), &git_commit()) {
        if let Err(e) = report::write_and_verify_certificate_in(&paths::repo_root(), &text) {
            eprintln!("certificate discarded: {e}");
            return std::process::ExitCode::from(2);
        }
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
/// # Complexity
///
/// `O(s + r)` for `s` scenarios and `r` runs, plus one concatenation of every
/// run's records for the session-scoped scenarios.
fn evaluate(
    scenarios: &[runner::Scenario],
    results: &[runner::RunResult],
    probe: &runner::TransparencyProbe,
    matrix: Option<&[(String, bool)]>,
    no_backend: bool,
) -> Vec<report::AssertionRow> {
    // Built once: a session-scoped scenario reads every run's traffic, and
    // borrowing it per scenario would rebuild it per scenario.
    let session_records: Vec<proxy::RequestRecord> = results
        .iter()
        .flat_map(|r| r.records.iter().cloned())
        .collect();

    let mut rows = Vec::new();
    for scenario in scenarios {
        // The partition is selected by TAG, never by scenario name: a list
        // maintained by hand stops matching in silence.
        if no_backend && scenario.backend_tag == Some(runner::BackendNeed::Required) {
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
                ctx.records = &session_records;
                ctx.proxy_degraded = results.iter().any(|r| r.proxy_degraded);
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
        probe_record: None,
        probe_sent_body: None,
        injected_agent: None,
        build_matrix: None,
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

/// The four feature combinations, and whether each BUILT.
///
/// A failure here is DATA, not an error: the scenario reading it needs two of
/// them to fail. All four are run so the row is complete.
///
/// **Every inner `cargo` gets its OWN `CARGO_TARGET_DIR`.** These run while the
/// outer `cargo run` still holds the harness's own `target/`, and this project
/// has already paid for that exact contention: two feature sets sharing one
/// target relink the same binaries and produce link errors that read as code
/// defects.
///
/// # Complexity
///
/// Four `cargo check` invocations, serially. Slow by construction, which is why
/// it sits behind a flag.
fn run_feature_matrix() -> Vec<(String, bool)> {
    const COMBINATIONS: [&str; 4] = ["tree", "published", "tree,published", ""];
    const NO_FEATURES_TAG: &str = "none";
    COMBINATIONS
        .iter()
        .map(|combo| {
            let mut args = vec!["check", "--no-default-features", "--quiet"];
            if !combo.is_empty() {
                args.extend(["--features", combo]);
            }
            let tag = if combo.is_empty() {
                NO_FEATURES_TAG.to_string()
            } else {
                combo.replace(',', "-")
            };
            let built = std::process::Command::new("cargo")
                .args(&args)
                .current_dir(paths::smoke_dir())
                .env(
                    "CARGO_TARGET_DIR",
                    paths::smoke_dir().join("target-matrix").join(&tag),
                )
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            (combo.to_string(), built)
        })
        .collect()
}

/// The `magi-core` version the harness was built against.
///
/// Read from the DEPENDENCY's metadata, not from the harness's own
/// `CARGO_PKG_VERSION`: that would certify the harness's version while claiming
/// to certify the crate's.
fn crate_version() -> String {
    const UNKNOWN: &str = "unknown";
    const CRATE_UNDER_TEST: &str = "magi-core";
    std::process::Command::new("cargo")
        .args(["metadata", "--format-version", "1"])
        .current_dir(paths::smoke_dir())
        .output()
        .ok()
        .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok())
        .and_then(|v| {
            v["packages"].as_array().and_then(|ps| {
                ps.iter()
                    .find(|p| p["name"] == CRATE_UNDER_TEST)
                    .and_then(|p| p["version"].as_str().map(str::to_string))
            })
        })
        .unwrap_or_else(|| UNKNOWN.to_string())
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
