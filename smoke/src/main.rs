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
mod weakened; // Task 11

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

fn main() -> std::process::ExitCode {
    let cli = match Cli::parse_from(std::env::args()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return std::process::ExitCode::from(2);
        }
    };
    eprintln!("magi-smoke — mode: {}", alias::MODE);

    // `--print-payload-size` is Task 3's own verification hook (Step 5): it
    // generates the payload against the REAL tree and prints only the byte
    // count, so the target is provably reachable without spending a backend.
    // It is handled here, and ONLY here, before anything else in `main` runs —
    // config loading beyond the built-in default, the preflight, and every
    // scenario belong to the run flow that later tasks still own.
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

    let _ = &cli;
    // NOTHING ELSE from a later task is called here. Task 1 must compile ON ITS
    // OWN — Step 5 checks exactly that — and reaching further into the run flow
    // (scenarios, the preflight, the report) would make the scaffold depend on
    // modules that are still empty files.
    std::process::ExitCode::SUCCESS
}
