// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-17

//! The three locations the harness resolves from, and nothing else.
//!
//! They live here rather than in their first consumer because THREE tasks need
//! them and they do not run in dependency order: the payload generator (Task 3)
//! runs before the preflight (Task 8), so defining them in the preflight would
//! leave them used-before-they-exist.
//!
//! All three are derived from `CARGO_MANIFEST_DIR`, which is a compile-time
//! constant pointing at `smoke/`. Deriving from the current directory instead
//! would make every answer depend on where the binary happened to be launched —
//! and the harness is deliberately its own workspace, so `cargo run` from the
//! repo root and from `smoke/` must resolve identically.

use std::path::PathBuf;

/// The harness's own directory: `<repo>/smoke`.
pub fn smoke_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The repository root: the directory above [`smoke_dir`].
///
/// **Expressed as `<smoke>/..` rather than obtained from `Path::parent`**, which
/// returns an `Option` that would have to be unwrapped. The invariant behind
/// that unwrap is real — `CARGO_MANIFEST_DIR` is absolute, so it always has a
/// parent — but `expect` outside `#[cfg(test)]` is forbidden by this project's
/// standards without exception for invariants that hold, and the rule is worth
/// more than the tidier path: a documented `expect` is how the next one, whose
/// invariant does NOT hold, gets written.
///
/// Every consumer joins onto this or hands it to `current_dir`, and for both the
/// two spellings are equivalent. The only visible difference is cosmetic: an
/// error message that prints this path shows the `..` component.
pub fn repo_root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
}

/// Where the tracked fixture corpus lives: `<repo>/smoke/fixtures`.
///
/// **Not a test helper.** The preflight calls it in production, so putting it in
/// `testkit` would have left it out of the binary.
pub fn fixture_dir() -> PathBuf {
    smoke_dir().join("fixtures")
}

/// The single parent of every feature-matrix build directory, under the system
/// temp directory.
///
/// **Chosen so the stale-temp sweep cannot claim it.**
/// `preflight::sweep_stale_temps` deletes `magi-smoke-<pid>-<...>` entries whose
/// PID is dead; here the segment after the prefix is `feature`, which does not
/// parse as a PID, so the sweep skips it and the build cache survives between
/// runs. `the_stale_temp_sweep_cannot_claim_the_matrix_cache` pins that.
const FEATURE_MATRIX_DIR: &str = "magi-smoke-feature-matrix";

/// Where ONE feature combination's `cargo check` builds:
/// `<temp>/magi-smoke-feature-matrix/<tag>`.
///
/// # Why not inside the repository
///
/// It used to be `<repo>/smoke/target-matrix/<tag>`. That is gitignored, so
/// nothing ever leaked into git — but it put hundreds of megabytes of build
/// output inside the user's checkout for a flag that answers one question, and
/// the no-trace scenario's guarantee then rested on an ignore rule rather than
/// on the harness not writing there. Build output is not the user's work; it
/// belongs where the operating system already collects it.
///
/// # Why one directory PER COMBINATION, which is not negotiable
///
/// Two feature sets sharing one target directory relink the same binaries and
/// produce link errors that read as code defects. This project has already paid
/// for that exact contention, and the fix was isolation — so the move out of the
/// tree keeps it: the `tag` segment is what makes each combination's build
/// independent of the others.
///
/// # Parameters
///
/// * `tag` — the combination's directory-safe name, e.g. `tree-published`.
pub fn feature_matrix_target_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(FEATURE_MATRIX_DIR).join(tag)
}

/// Where the preflight's `cargo metadata` builds: `<temp>/magi-smoke-metadata`.
///
/// `--no-deps` does not build, but it still locks the target directory on some
/// platforms, so the isolation check gets one of its own rather than contending
/// with the build that is about to start.
///
/// It lives here rather than inline in the preflight for one reason:
/// [`writable_locations`] has to be able to enumerate it. A write site the
/// enumeration cannot see is a write site the no-trace guard does not cover,
/// which is the difference between a guard and a list of the paths somebody
/// remembered.
pub fn metadata_target_dir() -> PathBuf {
    std::env::temp_dir().join("magi-smoke-metadata")
}

/// **Every directory this harness writes into.** The no-trace guard reads this
/// list; nothing else does.
///
/// # Why a list and not three assertions
///
/// The property R36 states is about the harness as a whole — *nothing it
/// generates lands in the repository* — and three tests each pinning one path
/// prove it for the three paths somebody thought of. A write site added
/// tomorrow is invisible to all three. Enumerating here means the guard is
/// wrong in a visible way (a location missing from a list someone must edit)
/// instead of in an invisible one.
///
/// # What is deliberately NOT here
///
/// The certificate (`docs/test/smoke-certificate.md`) is the one thing the
/// harness writes INSIDE the tree, and R37 makes it the declared exception. It
/// is excluded because it is the exception, not because it was forgotten — the
/// guard's own test names it.
///
/// The payload is not here either, and for a stronger reason: it is generated
/// in MEMORY and never touches a filesystem at all.
///
/// # `#[cfg(test)]`, and not because it is a fixture
///
/// Nothing in the binary calls it: the no-trace guard is a TEST-time mechanism
/// whose whole job is to fail `cargo test` when the harness gains a write site
/// inside the tree — the same shape as the assertion-relaxation mark guard this
/// crate already keeps behind the same gate. Inventing a runtime caller so it
/// could be `pub` would be fabricating a consumer to satisfy the linter, which
/// this project's standards forbid outright.
#[cfg(test)]
pub fn writable_locations() -> Vec<PathBuf> {
    vec![
        metadata_target_dir(),
        feature_matrix_target_dir("tree"),
        // The sweep's own root: the harness creates per-run temp directories
        // under it, and cleans them from it on entry.
        std::env::temp_dir(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_matrix_cache_is_built_outside_the_users_checkout() {
        // The whole point of the move: a flag that answers one question must not
        // leave build output inside somebody's working tree, ignore rule or no
        // ignore rule.
        let dir = feature_matrix_target_dir("tree");
        assert!(
            dir.starts_with(std::env::temp_dir()),
            "the matrix must build under the system temp directory, got {dir:?}"
        );
        assert!(
            !dir.starts_with(smoke_dir()),
            "the matrix must not build inside the harness's own directory: {dir:?}"
        );
    }

    #[test]
    fn each_combination_gets_its_own_directory() {
        // Sharing one target directory between feature sets relinks the same
        // binaries and produces link errors that read as code defects.
        assert_ne!(
            feature_matrix_target_dir("tree"),
            feature_matrix_target_dir("tree-published")
        );
    }

    #[test]
    fn nothing_the_harness_generates_lands_in_the_repo() {
        // R36, and the plan named this test in Step 1 of Task 12. Writing into
        // the tree breaks three things at once: §8 wants a clean `git status`,
        // Paso 0 would see files outside the plan, and `scoped_tests` widens to
        // the full suite on any unknown path.
        //
        // It asserts over `writable_locations` rather than over `git status`,
        // and the difference is the point: `git status` cannot see a gitignored
        // path, and `smoke/target/` and `smoke/target-matrix/` are both
        // gitignored — so a temporary landing in either would satisfy a
        // status-based check while sitting inside somebody's checkout. Asking
        // where the harness WRITES answers the question the requirement asks.
        let repo = repo_root()
            .canonicalize()
            .expect("the repository root must be resolvable");
        for location in writable_locations() {
            let resolved = resolve_deepest_existing(&location);
            assert!(
                !resolved.starts_with(&repo),
                "the harness writes to {resolved:?}, which is inside the repository at \
                 {repo:?}. The certificate is the ONE declared exception (R37); everything \
                 else belongs where the operating system already collects it."
            );
        }
    }

    /// Canonicalises the deepest ANCESTOR of `p` that exists, and re-appends the
    /// rest.
    ///
    /// # Why not just `canonicalize().unwrap_or(p)`
    ///
    /// That is what this test did first, and a mutation proved it vacuous: a
    /// build directory that has not been created yet cannot be canonicalised, so
    /// the fallback returned the raw path — which on Windows lacks the `\\?\`
    /// prefix the canonical repo root carries, so `starts_with` was false for a
    /// location sitting squarely inside the checkout. The guard compared two
    /// spellings of the filesystem and reported success.
    ///
    /// The locations under test are directories the harness creates ON DEMAND,
    /// so "does not exist yet" is their NORMAL state — which made the vacuous
    /// branch the one that always ran.
    fn resolve_deepest_existing(p: &std::path::Path) -> PathBuf {
        let mut cursor = p;
        let mut trailing = PathBuf::new();
        loop {
            if let Ok(found) = cursor.canonicalize() {
                return found.join(&trailing);
            }
            let Some(name) = cursor.file_name() else {
                return p.to_path_buf();
            };
            trailing = PathBuf::from(name).join(&trailing);
            match cursor.parent() {
                Some(parent) => cursor = parent,
                None => return p.to_path_buf(),
            }
        }
    }

    #[test]
    fn the_certificate_is_the_one_declared_exception_and_it_is_named() {
        // The other half of the guard above, and it has to be written down or
        // the exception becomes an oversight the next reader has to rediscover:
        // exactly one thing the harness writes lives inside the tree, R37 says
        // which, and it is deliberately absent from `writable_locations`.
        let cert = repo_root().join(crate::report::CERT_PATH);
        assert!(
            !writable_locations().contains(&cert),
            "the certificate must not be in the list the no-trace guard rejects, or the guard \
             would forbid the one write R37 requires"
        );
    }

    #[test]
    fn the_stale_temp_sweep_cannot_claim_the_matrix_cache() {
        // Both live under the system temp directory, and the sweep deletes
        // `magi-smoke-<pid>-...` entries whose PID is dead. If the matrix
        // directory's name parsed as one of those, every start would delete the
        // build cache and the matrix would rebuild from scratch each time.
        let root = crate::testkit::tempdir_with(&[]);
        let cache = root.path().join(FEATURE_MATRIX_DIR).join("tree");
        std::fs::create_dir_all(&cache).expect("creating the fixture must succeed");
        crate::preflight::sweep_stale_temps(root.path());
        assert!(
            cache.exists(),
            "the sweep deleted the matrix build cache at {cache:?}"
        );
    }
}
