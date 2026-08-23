// Author: Julian Bolivar
// Version: 4.0.0
// Date: 2026-08-23

//! The ONE `git status --porcelain` invocation in the harness.
//!
//! # Why it is one function and not three
//!
//! Three modules asked git the same question with the same flags, each spelling
//! the command out for itself: the baseline taken before any run (`main`), the
//! pre- and post-write checks around the certificate (`report`), and the
//! no-trace scenario (`scenarios::e1`). Changing a flag therefore meant three
//! edits, and missing one of them would have been invisible — the two callers
//! that still agreed would keep passing while the third silently asked a
//! different question. This project has already paid once for one rule with two
//! implementations, when a verdict-block guard and the parser it simulated
//! drifted apart and let a fabricable object through the guard.
//!
//! # What is deliberately NOT unified: the error policy
//!
//! The three callers disagree about what a failure MEANS, and they are all
//! right:
//!
//! * the baseline treats it as **no baseline**, so the no-trace scenario skips
//!   instead of attributing somebody's uncommitted work to the harness;
//! * the certificate treats it as **fatal**, and deletes a certificate it can no
//!   longer vouch for;
//! * the scenario treats it as **its own skip**, carrying git's message.
//!
//! So this module returns a `Result` and decides nothing. Collapsing the three
//! into one policy here would move a decision away from the only place that
//! knows what it costs.

use std::path::Path;

/// The flags, in one place, because they are the thing that must not drift.
///
/// `--porcelain` rather than the human status: a stable, script-friendly format
/// that is empty if and only if the tree is clean. Parsing the human form would
/// be the grep-over-semantics this project rejects elsewhere.
///
/// `--untracked-files=all` is not cosmetic: without it git COLLAPSES an
/// untracked directory to the directory itself (`?? docs/`), so a check
/// comparing against a full path would never match its own file.
const STATUS_ARGS: [&str; 3] = ["status", "--porcelain", "--untracked-files=all"];

/// Runs `git status --porcelain --untracked-files=all` in `dir` and returns its
/// raw stdout.
///
/// # No colour handling, and that is measured rather than assumed
///
/// `--porcelain` output carries no ANSI escapes even under
/// `color.status=always` or `color.ui=always` — checked against this very
/// repository, whose porcelain output contains zero `ESC` bytes with either
/// forced on. So a caller matching on these lines cannot be broken by a
/// colour setting, and no `-c color.status=never` is needed to make that true.
///
/// # Parameters
///
/// * `dir` — the working tree to ask about.
///
/// # Returns
///
/// `Ok(status)` when git answered — **including `Ok("")` for a genuinely clean
/// tree**, which is a different claim from "could not measure" and is why the
/// caller gets a `Result` rather than an `Option<String>` that conflates them.
///
/// # Errors
///
/// The `git` process could not be spawned, exited non-zero, or its stdout was
/// not valid UTF-8. Every message names the directory, since the three callers
/// print it in three different shapes.
///
/// # Complexity
///
/// One process spawn; `O(n)` in the length of git's output.
pub fn status_porcelain(dir: &Path) -> Result<String, String> {
    let out = std::process::Command::new("git")
        .args(STATUS_ARGS)
        .current_dir(dir)
        .output()
        .map_err(|e| format!("failed to run git status in {}: {e}", dir.display()))?;
    if !out.status.success() {
        return Err(format!(
            "git status failed in {}: {}",
            dir.display(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    String::from_utf8(out.stdout).map_err(|e| format!("git status output was not UTF-8: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_real_repository_answers_and_a_non_repository_does_not() {
        // The distinction the `Result` exists to keep: "clean" and "could not
        // ask" are opposite claims, and the callers that treat them differently
        // can only do so if this function tells them apart.
        assert!(
            status_porcelain(&crate::paths::repo_root()).is_ok(),
            "the real repository must be measurable"
        );
        // git runs here and exits non-zero: a tree it could not report on.
        assert!(status_porcelain(&std::env::temp_dir()).is_err());
        // git cannot even be spawned with this working directory.
        assert!(status_porcelain(
            &std::env::temp_dir().join("magi-smoke-no-such-directory-anywhere")
        )
        .is_err());
    }

    #[test]
    fn the_flags_are_the_ones_every_caller_depended_on() {
        // The extraction's whole point is that these stop being written three
        // times. Pinning them here means dropping `--untracked-files=all` —
        // which would hide the very paths the no-trace check compares — fails
        // one test instead of silently changing what three callers ask.
        assert_eq!(
            STATUS_ARGS,
            ["status", "--porcelain", "--untracked-files=all"]
        );
    }
}
