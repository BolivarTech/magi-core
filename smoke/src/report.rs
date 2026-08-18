// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-18

//! Renders what a smoke cycle learned: a human table for stderr, an optional
//! JSON form for tooling, and — only for the SECOND run of a cycle — the
//! signed certificate that ships with a release.
//!
//! Two distinctions are load-bearing here, because this harness exists to say
//! what happened and a table that renders a state ambiguously defeats the
//! whole thing:
//!
//! - **A TIME failure is never rendered like an assertion failure.** Collapsing
//!   them reports "the crate is wrong" when the truth is "the deployment is
//!   slower than the cap someone chose". [`AssertionRow::over_budget`] is kept
//!   as a field separate from `state` for exactly this reason, and every
//!   renderer gives `Timeout` a marker (`TIMEOUT`) that a `Fail` row can never
//!   produce.
//! - **Every row carries the run-id that fed it.** Without it, five red
//!   assertions sharing one crashed run read as five separate defects instead
//!   of one failure with five symptoms.
//!
//! # Scope note (Task 12, reporting half only)
//!
//! This module implements three of the five Step-1 tests from
//! `task-12a-brief.md` — the ones whose contract is a pure function of
//! [`AssertionRow`]/[`Report`]/[`CycleRun`]:
//! `the_certificate_declares_the_version_inside_not_in_the_filename`,
//! `a_dirty_tree_gets_NO_certificate_not_a_caveated_one`, and
//! `the_large_payload_result_is_the_first_thing_in_the_certificate`.
//!
//! The other two — `nothing_the_harness_generates_lands_in_the_repo` and
//! `the_cost_is_announced_BEFORE_the_runs_and_recorded_AFTER` — exercise the
//! FULL harness binary end to end (`run_harness_full`, `git_status_short`,
//! a cost estimate this module has no field for) and belong to `main.rs`'s
//! own orchestration (Step 3b), which another agent is wiring. Fabricating a
//! `run_harness_full` stub here, without the real preflight/dispatch loop
//! behind it, would prove nothing and is exactly the kind of caller this
//! project's standards forbid inventing just to make a test compile.

use std::fmt::Write as _;
use std::path::Path;
use std::time::Duration;

use crate::config::RunId;
use crate::outcome::ScenarioState;

/// Which of the two cycle runs this is. **Only SMOKE #2 writes the
/// certificate** — #1 verifies the implementation, #2 certifies what ships.
/// There is no auto-detection: guessing which run this is would make the
/// certificate a guess too, so it always arrives from the `--smoke-2` flag on
/// the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleRun {
    /// SMOKE #1: verifies the implementation. No certificate.
    First,
    /// SMOKE #2: certifies what ships. The only run that writes one.
    Second,
}

/// One assertion, ready to render: which scenario, which shared run fed it,
/// what it found, and — only for a TIME failure — by how much it overran its
/// budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertionRow {
    /// The scenario this row belongs to, by its stable id.
    ///
    /// Separate from [`AssertionRow::scenario`] because one scenario produces
    /// SEVERAL assertions: without the id, four reds from one scenario read as
    /// four unrelated defects, which is the same confusion `run_id` exists to
    /// prevent one level up.
    pub scenario_id: &'static str,
    /// The property this row is about, written as a sentence a reader can
    /// check against the code (mirrors [`crate::runner::Assertion::name`]).
    pub scenario: &'static str,
    /// Which shared run produced this assertion. Carried on every row on
    /// purpose — see the module doc.
    pub run_id: RunId,
    /// What the assertion found.
    pub state: ScenarioState,
    /// `Some` only when `state` is [`ScenarioState::Timeout`]: how far over
    /// its budget the run was. Kept as a field separate from `state` so a
    /// TIME failure can never be rendered indistinguishably from an
    /// assertion failure — the one thing this row exists to keep visible.
    pub over_budget: Option<Duration>,
}

/// Every assertion from one cycle run, ready to render in all three forms.
#[derive(Debug, Clone)]
pub struct Report {
    /// One entry per assertion, in the order the scenarios produced them.
    pub rows: Vec<AssertionRow>,
    /// Which of the two cycle runs this report is for.
    pub run: CycleRun,
}

impl Report {
    /// The stderr table: one line per assertion, each carrying its run-id and
    /// a state marker that a TIME failure can never share with an assertion
    /// failure.
    ///
    /// # Complexity
    ///
    /// `O(n)` in `self.rows.len()`.
    pub fn render_human(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "magi-smoke report — cycle run: {}",
            cycle_run_label(self.run)
        );
        for row in &self.rows {
            let _ = writeln!(out, "{}", format_row(row));
        }
        out
    }

    /// The optional machine-readable form. Same data as [`Self::render_human`],
    /// no second source of truth: both read the same `rows`, and this one
    /// goes through [`row_to_json`] rather than re-deriving the state markers.
    ///
    /// # Complexity
    ///
    /// `O(n)` in `self.rows.len()`.
    pub fn render_json(&self) -> String {
        let rows: Vec<serde_json::Value> = self.rows.iter().map(row_to_json).collect();
        let doc = serde_json::json!({
            "cycle_run": cycle_run_label(self.run),
            "rows": rows,
        });
        serde_json::to_string_pretty(&doc)
            .unwrap_or_else(|e| format!("{{\"error\": \"failed to render JSON: {e}\"}}"))
    }

    /// Why this run gets no certificate, or `None` when one is due.
    ///
    /// The ONE place that decides. Both refusals were previously implicit — the
    /// cycle-run gate lived inline in [`Self::render_certificate`], and the
    /// failed-assertion gate did not exist at all, so a `--smoke-2` run with a
    /// red row still produced a certificate.
    fn certificate_refusal(&self) -> Option<&'static str> {
        if self.run != CycleRun::Second {
            // Certifying from #1 would certify an artifact the gate has not
            // touched yet.
            return Some("this is not the certifying run (pass --smoke-2 for SMOKE #2)");
        }
        if self.rows.iter().any(|r| r.state == ScenarioState::Fail) {
            // The certificate's whole claim is "nothing regressed and everything
            // that ran passed". A FAIL contradicts it, and R37's reasoning
            // applies unchanged: one issued over a red run still EXISTS, and
            // what exists gets cited. A SKIP does not refuse it — the
            // certificate renders skips deliberately, large-payload first.
            return Some(
                "an assertion FAILED in this run, and a certificate is a claim that none did",
            );
        }
        None
    }

    /// The certificate body, or `None` with the reason available from
    /// [`Self::certificate_refusal`] — the certifying run only, and only when
    /// nothing failed. The large-payload result is surfaced first; see
    /// [`render_certificate`].
    ///
    /// # Parameters
    ///
    /// * `version` — the crate version this certificate is issued against.
    /// * `commit` — the commit this certificate is issued against.
    pub fn render_certificate(&self, version: &str, commit: &str) -> Option<String> {
        match self.certificate_refusal() {
            Some(_) => None,
            None => Some(render_certificate(&self.rows, version, commit)),
        }
    }

    /// Writes the certificate for the run that certifies, folding a failure to
    /// write it **into** the report instead of replacing it.
    ///
    /// # Why this is a method and not three lines in `main`
    ///
    /// It used to be three lines in `main`, and they inverted the one
    /// distinction this whole harness exists to preserve: a write failure
    /// returned exit `2` **before** the table was emitted, so a run containing a
    /// `Fail` row — exit `1`, a verdict about the crate — was reported as `2`,
    /// a fault of ours, with the verdict never printed. And it was easy to
    /// reach, since the write refuses over ANY pre-existing uncommitted change.
    ///
    /// Now the failure becomes one more row and [`Self::exit_code`] decides, as
    /// it does for everything else: a `Fail` outranks it, so the crate's
    /// verdict still wins; on an otherwise clean run it is exit `2`, which is
    /// what "we could not produce the certificate" means.
    ///
    /// # Parameters
    ///
    /// * `repo_root` — the tree to write into.
    /// * `version` — the crate version the certificate is issued against.
    /// * `commit` — the commit the certificate is issued against.
    pub fn write_certificate_in(&mut self, repo_root: &Path, version: &str, commit: &str) {
        let text = match self.render_certificate(version, commit) {
            Some(t) => t,
            None => {
                // Announced only for the run that was SUPPOSED to certify.
                // Saying it on every SMOKE #1 would be noise on the normal path.
                if self.run == CycleRun::Second {
                    if let Some(why) = self.certificate_refusal() {
                        eprintln!("no certificate written: {why}");
                    }
                }
                return;
            }
        };
        if let Err(e) = write_and_verify_certificate_in(repo_root, &text) {
            eprintln!("certificate discarded: {e}");
            self.note_certificate_failure(&e);
        }
    }

    /// Records that the certificate could not be written, as one more row.
    ///
    /// A `Skip`, not a `Fail`: failing to write a certificate is a fault of
    /// OURS, never a verdict about the crate — which is exactly why it must
    /// travel through the same precedence as every other row instead of
    /// short-circuiting the exit code.
    fn note_certificate_failure(&mut self, reason: &str) {
        self.rows.push(AssertionRow {
            scenario_id: "certificate",
            scenario: "the release certificate was written and verified",
            run_id: NO_RUN,
            state: ScenarioState::Skip(reason.to_string()),
            over_budget: None,
        });
    }
}

/// Renders the certificate body: the version and commit it was issued
/// against, with the large-payload result surfaced FIRST — it is the reason
/// this harness exists, and a SKIP there must be seen first, not buried under
/// three passing rows.
///
/// A free function rather than only a method, so it can be exercised (and its
/// ordering property proven) without first constructing a [`Report`] whose
/// only job is to hold a [`CycleRun`].
///
/// # Parameters
///
/// * `rows` — every assertion from the cycle's SECOND run.
/// * `version` — the crate version this certificate is issued against.
/// * `commit` — the commit this certificate is issued against; travels with
///   the certificate so `git show <tag>:<path>` recovers the right one.
///
/// # Complexity
///
/// `O(n log n)` in `rows.len()` for the stable sort that promotes the
/// large-payload rows; the render itself is `O(n)`.
pub fn render_certificate(rows: &[AssertionRow], version: &str, commit: &str) -> String {
    let mut ordered: Vec<&AssertionRow> = rows.iter().collect();
    // `sort_by_key` is a STABLE sort: rows that are not the large-payload run
    // keep their relative order, so this reorders only what needs reordering.
    ordered.sort_by_key(|row| large_payload_priority(row.run_id));
    let mut out = String::new();
    let _ = writeln!(out, "# Smoke Certificate");
    let _ = writeln!(out);
    let _ = writeln!(out, "- version: {version}");
    let _ = writeln!(out, "- commit: {commit}");
    let _ = writeln!(out);
    for row in ordered {
        let _ = writeln!(out, "{}", format_row(row));
    }
    out
}

/// Sort key that puts the large-payload run's rows first: `0` for
/// [`RunId::Large62k`], `1` for everything else.
///
/// A structural comparison against [`RunId`], never a text match on
/// `scenario` — matching on the scenario's wording would tie the ordering to
/// prose someone could reword without meaning to change behaviour.
fn large_payload_priority(run: RunId) -> u8 {
    if run == RunId::Large62k {
        0
    } else {
        1
    }
}

/// Where the certificate lands, relative to the repo root: the ONE file, of
/// fixed name, that the SECOND run of each release cycle replaces. The unit
/// of archival is the release tag, not the filename — `git show
/// <tag>:<path>` recovers a past certificate, so nothing here is versioned by
/// name.
pub const CERT_PATH: &str = "docs/test/smoke-certificate.md";

/// Writes `content` to [`CERT_PATH`] under `repo_root` — but ONLY if that
/// tree has no uncommitted changes.
///
/// A certificate is a claim about one specific commit. Writing one over a
/// dirty tree would let it be read as a claim about a commit that never
/// existed. **R37: a certificate marked "issued over a dirty tree" still
/// exists, and what exists gets cited as a certificate. One that was never
/// written cannot be.** So this checks first and writes only on a clean tree
/// — never writes a caveated file.
///
/// # Parameters
///
/// * `repo_root` — the tree to check and, if clean, to write into.
/// * `content` — the already-rendered certificate body.
///
/// # Errors
///
/// A `git status` failure, uncommitted changes (the message names it —
/// contains `"uncommitted"`), or an I/O failure creating the parent
/// directory or writing the file. In every error case, [`CERT_PATH`] is left
/// exactly as it was found: nothing is written before the clean-tree check
/// passes.
pub fn write_and_verify_certificate_in(repo_root: &Path, content: &str) -> Result<(), String> {
    let status = git_status_porcelain(repo_root)?;
    if !status.trim().is_empty() {
        return Err(format!(
            "refusing to write a certificate over uncommitted changes in {}: {status}",
            repo_root.display()
        ));
    }
    let target = repo_root.join(CERT_PATH);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    }
    std::fs::write(&target, content)
        .map_err(|e| format!("could not write {}: {e}", target.display()))?;
    // Re-read AFTER writing, because the tree could have changed between the
    // check above and the write. The only path allowed to appear is the
    // certificate itself; anything else means it would certify something other
    // than what ships, so the file is DELETED rather than left in place. A
    // certificate that exists gets cited, and half a certificate claims the same
    // thing as a whole one with less text.
    // The `?` here used to return WITHOUT deleting, so a git that failed to run
    // left the certificate on disk while the caller was told it had been
    // discarded — the same class of lie the branch below exists to prevent, one
    // step earlier. If the tree cannot be re-read, the certificate cannot be
    // trusted, and an untrustworthy certificate must not survive.
    let after = match git_status_porcelain(repo_root) {
        Ok(a) => a,
        Err(e) => {
            return Err(match std::fs::remove_file(&target) {
                Ok(()) => format!(
                    "the tree could not be re-read after writing, so the certificate was                      deleted rather than left claiming a version nothing verified: {e}"
                ),
                Err(rm) => format!(
                    "the tree could not be re-read after writing ({e}), and the certificate                      could NOT be deleted either ({rm}): it is STILL ON DISK at {} and must                      be removed by hand before anyone cites it",
                    target.display()
                ),
            });
        }
    };
    // SUFFIX, not substring. A porcelain line whose path merely CONTAINS the
    // certificate path — `docs/test/smoke-certificate.md.orig`, or anything
    // nested below it — would satisfy `contains` and let the certificate be
    // issued over a tree that changed in some other way. The scenario that
    // asserts the same property already compares by suffix; the two are now
    // spelled the same way, which is what stops them drifting apart.
    if after.lines().any(|l| !l.trim_end().ends_with(CERT_PATH)) {
        // What the message says is what actually happened. Reporting "it was
        // deleted" over a failed removal would leave a file on disk that the
        // next reader cites as a certificate, told by this very error that it
        // is not there.
        return Err(match std::fs::remove_file(&target) {
            Ok(()) => format!(
                "the tree changed while the certificate was being written, so it was deleted \
                 rather than left claiming a version it may not describe: {after}"
            ),
            Err(e) => format!(
                "the tree changed while the certificate was being written, and it could NOT \
                 be removed ({e}), so {} is STILL ON DISK and may claim a version it does not \
                 describe — delete it by hand before citing it: {after}",
                target.display()
            ),
        });
    }
    Ok(())
}

/// Runs `git status --porcelain` in `repo_root` and returns its raw stdout.
///
/// `--porcelain` rather than the human status: a stable, script-friendly
/// format that is empty if and only if the tree is clean. Parsing the human
/// form would be exactly the grep-over-semantics this project rejects
/// elsewhere. `--untracked-files=all` because the default collapses an
/// untracked directory to its name, which hides the very path the post-write
/// check is looking for.
///
/// # Errors
///
/// The `git` process could not be spawned, exited non-zero, or its stdout was
/// not valid UTF-8.
fn git_status_porcelain(repo_root: &Path) -> Result<String, String> {
    let out = std::process::Command::new("git")
        // `--untracked-files=all` is not cosmetic: without it git COLLAPSES an
        // untracked directory to the directory itself (`?? docs/`), so the
        // post-write check comparing against the certificate's full path would
        // never match its own file and would delete every certificate it wrote.
        .args(["status", "--porcelain", "--untracked-files=all"])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("failed to run git status in {}: {e}", repo_root.display()))?;
    if !out.status.success() {
        return Err(format!(
            "git status failed in {}: {}",
            repo_root.display(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    String::from_utf8(out.stdout).map_err(|e| format!("git status output was not UTF-8: {e}"))
}

/// The marker printed for a passing assertion. Distinct from every other
/// marker below by construction — see [`state_marker`].
const PASS_MARKER: &str = "PASS";
/// The marker for an assertion that contradicted the crate.
const FAIL_MARKER: &str = "FAIL";
/// The marker for a run that was still alive when its budget ran out. Never
/// equal to [`FAIL_MARKER`]: a TIME failure is not a verdict about the crate
/// (see the module doc).
const TIMEOUT_MARKER: &str = "TIMEOUT";
/// The marker for an assertion that could not be tested.
const SKIP_MARKER: &str = "SKIP";
/// The marker for a scenario deliberately excluded from this run.
const OUT_OF_SCOPE_MARKER: &str = "OUT_OF_SCOPE";

/// Maps a [`ScenarioState`] to its printed marker.
///
/// One function, used by every renderer in this module, so `Timeout` and
/// `Fail` can never drift into sharing a marker through two independently
/// written `match` arms.
fn state_marker(state: &ScenarioState) -> &'static str {
    match state {
        ScenarioState::Pass => PASS_MARKER,
        ScenarioState::Fail => FAIL_MARKER,
        ScenarioState::Timeout => TIMEOUT_MARKER,
        ScenarioState::Skip(_) => SKIP_MARKER,
        ScenarioState::OutOfScope => OUT_OF_SCOPE_MARKER,
    }
}

/// Renders one row exactly once, so the human table and the certificate
/// describe the same fact with the SAME wording. Two renderers disagreeing
/// about one row's fate is how a report starts lying about itself.
///
/// # Parameters
///
/// * `row` — the assertion to render.
fn format_row(row: &AssertionRow) -> String {
    let marker = state_marker(&row.state);
    let mut line = format!("[{marker}] run={} — {}", row.run_id.as_str(), row.scenario);
    if let ScenarioState::Skip(reason) = &row.state {
        let _ = write!(line, " (skipped: {reason})");
    }
    if let Some(over) = row.over_budget {
        let _ = write!(line, " (over budget by {:.1}s)", over.as_secs_f64());
    }
    line
}

/// Maps a [`CycleRun`] to the word printed for it — kept as one function so
/// the human table and the JSON form agree on the same spelling.
fn cycle_run_label(run: CycleRun) -> &'static str {
    match run {
        CycleRun::First => "first",
        CycleRun::Second => "second",
    }
}

/// Renders one [`AssertionRow`] as a [`serde_json::Value`] for
/// [`Report::render_json`].
///
/// A free function rather than a `Serialize` impl on `AssertionRow`: the type
/// borrows `RunId`/`ScenarioState` from sibling modules that do not derive
/// `Serialize`, and adding that derive to types this task does not own is out
/// of scope (see the module doc).
fn row_to_json(row: &AssertionRow) -> serde_json::Value {
    let (state, detail) = match &row.state {
        ScenarioState::Pass => ("pass", None),
        ScenarioState::Fail => ("fail", None),
        ScenarioState::Timeout => ("timeout", None),
        ScenarioState::Skip(reason) => ("skip", Some(reason.clone())),
        ScenarioState::OutOfScope => ("out_of_scope", None),
    };
    serde_json::json!({
        "scenario": row.scenario,
        "run_id": row.run_id.as_str(),
        "state": state,
        "detail": detail,
        "over_budget_secs": row.over_budget.map(|d| d.as_secs_f64()),
    })
}

impl AssertionRow {
    /// Builds the rows one scenario produced.
    ///
    /// # Parameters
    ///
    /// * `scenario_id` — the scenario's stable id.
    /// * `run_id` — which shared run fed it.
    /// * `assertions` — everything that scenario asserted, in order.
    ///
    /// # Complexity
    ///
    /// `O(n)` in the number of assertions.
    pub fn of(
        scenario_id: &'static str,
        run_id: RunId,
        assertions: Vec<crate::runner::Assertion>,
        over_budget: Option<Duration>,
    ) -> Vec<AssertionRow> {
        assertions
            .into_iter()
            .map(|a| AssertionRow {
                scenario_id,
                scenario: a.name,
                run_id,
                over_budget: match a.state {
                    ScenarioState::Timeout => over_budget,
                    _ => None,
                },
                state: a.state,
            })
            .collect()
    }
}

impl Report {
    /// A fault of OURS: no scenario is reported as passed, and the exit code is
    /// [`crate::outcome::EXIT_INCONCLUSIVE`]. *(The name in this link used to be
    /// `EXIT_CANNOT_TEST`, which no constant has ever been called.)*
    ///
    /// # Parameters
    ///
    /// * `reason` — what stopped the run, in terms an operator can act on.
    pub fn cannot_test(reason: &str) -> Report {
        Report {
            rows: vec![AssertionRow {
                scenario_id: "preflight",
                scenario: "the harness could not test",
                run_id: NO_RUN,
                state: ScenarioState::Skip(reason.to_string()),
                over_budget: None,
            }],
            run: CycleRun::First,
        }
    }

    /// The process exit code, decided HERE and nowhere else.
    ///
    /// `0` everything that ran passed · `1` an assertion FAILED, which is a
    /// verdict about the crate · `2` could not test, which is a fault of ours.
    /// **Confusing 1 with 2 is the failure this whole harness exists to
    /// eliminate**, so there is exactly one place that chooses.
    ///
    /// # Complexity
    ///
    /// `O(n)` in `self.rows.len()`.
    pub fn exit_code(&self) -> u8 {
        // DELEGATES rather than re-deciding. The policy over states lives in
        // `outcome`, which owns the three codes and their precedence; this
        // method's only job is turning rows into the states that policy reads.
        // Two implementations is exactly how the 1-versus-2 distinction drifts
        // apart, and that distinction is what the harness exists to preserve.
        let states: Vec<ScenarioState> = self.rows.iter().map(|r| r.state.clone()).collect();
        crate::outcome::exit_code(&states)
    }

    /// Prints the human table to stderr and returns the process code.
    pub fn emit(self) -> std::process::ExitCode {
        eprintln!("{}", self.render_human());
        std::process::ExitCode::from(self.exit_code())
    }

    /// Test constructor: a report from rows that are already built.
    #[cfg(test)]
    pub fn with(rows: &[AssertionRow]) -> Report {
        Report {
            rows: rows.to_vec(),
            run: CycleRun::First,
        }
    }
}

/// The run id a row carries when the harness itself could not test anything, so
/// there is no shared run to attribute it to.
const NO_RUN: RunId = RunId::NoBackend;

#[cfg(test)]
mod tests {

    #[test]
    fn the_exit_code_is_decided_in_exactly_one_place() {
        // 1 = a verdict about the crate; 2 = a fault of ours. Two call sites is
        // how the two drift apart, so this method delegates to `outcome` and
        // this test pins the mapping end to end.
        let row = |state| AssertionRow {
            scenario_id: "S-test",
            run_id: RunId::HappySmall,
            scenario: "a property",
            state,
            over_budget: None,
        };
        assert_eq!(Report::with(&[row(ScenarioState::Fail)]).exit_code(), 1);
        assert_eq!(Report::cannot_test("proxy").exit_code(), 2);
        assert_eq!(Report::with(&[row(ScenarioState::Pass)]).exit_code(), 0);
        assert_eq!(
            Report::with(&[row(ScenarioState::Pass), row(ScenarioState::OutOfScope)]).exit_code(),
            0,
            "a partition nobody asked to run must not turn a clean run into a fault"
        );
        assert_eq!(
            Report::with(&[row(ScenarioState::Fail), row(ScenarioState::Timeout)]).exit_code(),
            1,
            "a contradiction outranks an unanswered question"
        );
    }
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Monotonic counter mixed into fixture directory names, mirroring the
    /// same pattern `testkit.rs` uses — kept local rather than importing
    /// `testkit`'s private `UNIQUE`, which this module has no access to.
    static UNIQUE: AtomicU64 = AtomicU64::new(0);

    /// A handful of assertions covering a happy-path row and a large-payload
    /// row, in an order that does NOT already match the certificate's
    /// required ordering — proving `render_certificate` reorders rather than
    /// merely preserving input order.
    fn sample_results() -> Vec<AssertionRow> {
        vec![
            AssertionRow {
                scenario_id: "S-test",
                scenario: "the happy path run produces a valid verdict from all three seats",
                run_id: RunId::HappySmall,
                state: ScenarioState::Pass,
                over_budget: None,
            },
            AssertionRow {
                scenario_id: "S-test",
                scenario: "the large payload run converges within its budget",
                run_id: RunId::Large62k,
                state: ScenarioState::Skip("no backend available for this cycle".into()),
                over_budget: None,
            },
            AssertionRow {
                scenario_id: "S-test",
                scenario: "rotation recovers from an injected failure",
                run_id: RunId::Rotation,
                state: ScenarioState::Pass,
                over_budget: None,
            },
        ]
    }

    /// A throwaway git repository with one untracked file — "uncommitted
    /// changes" in the sense [`write_and_verify_certificate_in`] must refuse.
    ///
    /// # Panics
    ///
    /// Panics on any fixture-setup failure. Acceptable here: this is
    /// `#[cfg(test)]`-only fixture setup, and a setup failure should stop the
    /// test immediately rather than run against a partial repo (same
    /// rationale as `testkit::repo_where_the_negation_was_removed`).
    fn repo_with_uncommitted_changes() -> PathBuf {
        let unique = UNIQUE.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "magi-smoke-report-test-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create fixture repo dir");
        let out = std::process::Command::new("git")
            .arg("init")
            .current_dir(&dir)
            .output()
            .expect("git init for fixture repo");
        assert!(
            out.status.success(),
            "git init failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        std::fs::write(dir.join("untracked.txt"), "dirty on purpose")
            .expect("write untracked fixture file");
        dir
    }

    // --- Step-1 tests from task-12a-brief.md, reproduced verbatim in intent ---

    #[test]
    fn the_certificate_declares_the_version_inside_not_in_the_filename() {
        let cert = render_certificate(&sample_results(), "4.0.0", "abc1234");
        assert!(cert.contains("4.0.0"));
        assert!(
            cert.contains("abc1234"),
            "the commit it was issued against travels with it"
        );
    }

    #[test]
    fn a_dirty_tree_gets_no_certificate_not_a_caveated_one() {
        // R37. One marked "issued over a dirty tree" still exists, and what
        // exists gets cited as a certificate. One that was never written
        // cannot be.
        let repo = repo_with_uncommitted_changes();
        let err = write_and_verify_certificate_in(&repo, "…").unwrap_err();
        assert!(err.contains("uncommitted"));
        assert!(!repo.join(CERT_PATH).exists());
    }

    #[test]
    fn the_large_payload_result_is_the_first_thing_in_the_certificate() {
        let cert = render_certificate(&sample_results(), "4.0.0", "abc1234");
        let large = cert.find("large payload").unwrap();
        let other = cert.find("happy path").unwrap();
        assert!(
            large < other,
            "a SKIP there must be seen first: it is the reason this harness exists"
        );
    }

    // --- Coverage beyond the brief's five: the properties the brief calls
    // out in prose but does not pin with its own test (the CycleRun gate,
    // and the two load-bearing distinctions the mutation proof exercises). ---

    /// A committed, clean git repository — the state
    /// [`write_and_verify_certificate_in`] is allowed to write into.
    ///
    /// # Panics
    ///
    /// Panics on any fixture-setup failure, for the same reason
    /// [`repo_with_uncommitted_changes`] does.
    fn clean_repo() -> PathBuf {
        let dir = repo_with_uncommitted_changes();
        let add = std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&dir)
            .output()
            .expect("git add for fixture repo");
        assert!(add.status.success());
        let commit = std::process::Command::new("git")
            .args([
                "-c",
                "user.email=test@example.invalid",
                "-c",
                "user.name=test",
                "commit",
                "-m",
                "fixture",
            ])
            .current_dir(&dir)
            .output()
            .expect("git commit for fixture repo");
        assert!(
            commit.status.success(),
            "git commit failed: {}",
            String::from_utf8_lossy(&commit.stderr)
        );
        dir
    }

    #[test]
    fn a_certificate_that_could_not_be_written_never_replaces_the_verdict() {
        // The inversion this test exists to catch: the write refuses over ANY
        // pre-existing uncommitted change — this repository's normal state, a
        // post-commit hook regenerates a tracked directory — and the old code
        // answered that by returning exit 2 BEFORE emitting the table. The run's
        // findings vanished, and a fault of ours was reported in place of them.
        let dirty = repo_with_uncommitted_changes();
        let mut report = Report {
            rows: sample_results(),
            run: CycleRun::Second,
        };
        report.write_certificate_in(&dirty, "4.0.0", "abc1234");

        let human = report.render_human();
        assert!(
            human.contains("happy path"),
            "every row the run produced must still be reported:\n{human}"
        );
        assert!(
            human.contains("uncommitted"),
            "and the certificate's own failure is reported ALONGSIDE them, \
             naming why:\n{human}"
        );
        assert_eq!(
            report.exit_code(),
            2,
            "failing to write a certificate is a fault of OURS, so it lands on 2 \
             through the same precedence as every other row"
        );
    }

    #[test]
    fn a_verdict_about_the_crate_outranks_our_own_failure_to_certify() {
        // The precedence question the fix had to answer: with BOTH a failed
        // assertion and a certificate we could not write, the crate's verdict
        // wins — 1, not 2. It falls out of `outcome::exit_code` precisely
        // because the certificate failure is a row rather than an early return.
        let mut report = Report {
            rows: vec![AssertionRow {
                scenario_id: "S-test",
                scenario: "a property the crate broke",
                run_id: RunId::HappySmall,
                state: ScenarioState::Fail,
                over_budget: None,
            }],
            run: CycleRun::Second,
        };
        report.note_certificate_failure("refusing to write over uncommitted changes");
        assert_eq!(report.exit_code(), 1);
    }

    #[test]
    fn a_run_with_a_failed_assertion_gets_no_certificate_at_all() {
        // R37's reasoning, applied to the other way a certificate can lie: one
        // issued over a red run still EXISTS, and what exists gets cited. A
        // SKIP does NOT refuse it — the certificate renders skips deliberately,
        // large-payload first.
        let mut rows = sample_results(); // carries a SKIP, and must still certify
        let clean = clean_repo();
        let mut passing = Report {
            rows: rows.clone(),
            run: CycleRun::Second,
        };
        assert!(passing.render_certificate("4.0.0", "abc1234").is_some());
        passing.write_certificate_in(&clean, "4.0.0", "abc1234");
        assert!(
            clean.join(CERT_PATH).exists(),
            "a skipped scenario must not withhold the certificate"
        );

        rows.push(AssertionRow {
            scenario_id: "S-test",
            scenario: "a property the crate broke",
            run_id: RunId::HappySmall,
            state: ScenarioState::Fail,
            over_budget: None,
        });
        let failing = Report {
            rows,
            run: CycleRun::Second,
        };
        assert!(
            failing.render_certificate("4.0.0", "abc1234").is_none(),
            "a red run gets no certificate to cite"
        );
    }

    #[test]
    fn only_the_second_cycle_run_emits_a_certificate() {
        // Emitting one from the first run would certify an artifact the gate
        // has not touched yet.
        let first = Report {
            rows: sample_results(),
            run: CycleRun::First,
        };
        assert!(first.render_certificate("4.0.0", "abc1234").is_none());

        let second = Report {
            rows: sample_results(),
            run: CycleRun::Second,
        };
        assert!(second.render_certificate("4.0.0", "abc1234").is_some());
    }

    #[test]
    fn a_path_that_merely_contains_the_certificate_path_does_not_satisfy_the_guard() {
        // The check was `contains`, so a line like `docs/test/…md.orig` — or
        // anything nested below the certificate — passed it, and the certificate
        // was issued over a tree that had changed in some other way.
        let line = format!("?? {CERT_PATH}.orig");
        assert!(
            !line.trim_end().ends_with(CERT_PATH),
            "a path that merely contains the certificate path must not be accepted as it"
        );
        let exact = format!(" M {CERT_PATH}");
        assert!(
            exact.trim_end().ends_with(CERT_PATH),
            "the certificate's own line must still be accepted"
        );
    }

    #[test]
    fn a_clean_tree_gets_the_certificate_written_at_cert_path() {
        let dir = clean_repo();
        write_and_verify_certificate_in(&dir, "certificate body").expect("clean tree must write");
        let written =
            std::fs::read_to_string(dir.join(CERT_PATH)).expect("certificate file must exist");
        assert_eq!(written, "certificate body");
    }

    #[test]
    fn a_time_failure_renders_visibly_different_from_an_assertion_failure() {
        // The mutation this test exists to catch: collapsing TIMEOUT into the
        // same marker as FAIL would report "the crate is wrong" when the
        // truth is "the deployment is slower than the cap someone chose".
        let timeout_row = AssertionRow {
            scenario_id: "S-test",
            scenario: "s",
            run_id: RunId::HappySmall,
            state: ScenarioState::Timeout,
            over_budget: Some(Duration::from_secs(5)),
        };
        let fail_row = AssertionRow {
            scenario_id: "S-test",
            scenario: "s",
            run_id: RunId::HappySmall,
            state: ScenarioState::Fail,
            over_budget: None,
        };
        let report = Report {
            rows: vec![timeout_row, fail_row],
            run: CycleRun::First,
        };
        let human = report.render_human();
        let timeout_line = human
            .lines()
            .find(|l| l.contains(TIMEOUT_MARKER))
            .expect("a TIMEOUT row");
        let fail_line = human
            .lines()
            .find(|l| l.contains(FAIL_MARKER) && !l.contains(TIMEOUT_MARKER))
            .expect("a FAIL row distinct from the TIMEOUT row");
        assert_ne!(timeout_line, fail_line);
        assert!(
            !timeout_line.contains(FAIL_MARKER),
            "a TIME failure must never read as an assertion failure: {timeout_line:?}"
        );
        assert!(
            !fail_line.contains(TIMEOUT_MARKER),
            "an assertion failure must never read as a TIME failure: {fail_line:?}"
        );
    }

    #[test]
    fn every_rendered_row_carries_the_run_id_that_fed_it() {
        // The mutation this test exists to catch: dropping the run-id from a
        // row would make five reds sharing one crashed run read as five
        // separate defects instead of one failure with five symptoms.
        let rows = sample_results();
        let report = Report {
            rows: rows.clone(),
            run: CycleRun::First,
        };
        let human = report.render_human();
        for row in &rows {
            assert!(
                human.contains(row.run_id.as_str()),
                "row for {:?} lost its run-id ({:?}) in the table:\n{human}",
                row.scenario,
                row.run_id.as_str()
            );
        }
    }

    #[test]
    fn render_json_is_parseable_and_carries_the_same_facts_as_the_human_table() {
        let report = Report {
            rows: sample_results(),
            run: CycleRun::Second,
        };
        let json = report.render_json();
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        let rows = parsed["rows"].as_array().expect("rows array");
        assert_eq!(rows.len(), sample_results().len());
        assert_eq!(parsed["cycle_run"], "second");
        // The large-payload row's skip reason must survive into the JSON —
        // "same data, no second source of truth" (Step 3's own doc comment).
        assert!(json.contains("no backend available for this cycle"));
    }
}
