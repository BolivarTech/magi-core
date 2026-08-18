// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-17

//! Shared test helpers. Each task adds the ones it needs, and the tasks that
//! follow reuse them; the module starts empty on purpose.
//!
//! Filesystem fixtures are built BY HAND rather than via a crate such as
//! `tempfile`: this package takes zero new dependencies (see the header of
//! `smoke/Cargo.toml`), and `#[cfg(test)]`-only code is exactly where
//! `unwrap`/`expect` are permitted, so a small hand-rolled helper costs
//! nothing extra in rigor.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// A directory under the OS temp root, removed on drop.
///
/// Owns the path so a test cannot outlive its own fixture directory by
/// accident: cleanup happens exactly once, when the last reference to the
/// `TempDir` goes out of scope.
pub struct TempDir(PathBuf);

impl TempDir {
    /// The directory's path, for handing to the code under test.
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    /// Best-effort cleanup. A `Drop` impl cannot return `Result`, and a test
    /// process exiting with the OS temp directory holding a few stray bytes
    /// is not a failure worth propagating — the next run gets a fresh,
    /// uniquely-named directory regardless (see [`tempdir_with`]).
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Monotonic counter mixed into each generated directory name so that two
/// `tempdir_with` calls within the same nanosecond (plausible on a fast
/// machine, since `SystemTime` resolution is not guaranteed) still collide
/// on nothing.
static UNIQUE: AtomicU64 = AtomicU64::new(0);

/// Creates a fresh temporary directory and populates it with `files`: pairs
/// of a path relative to the directory root and the text content to write
/// there. Parent directories are created as needed, so `("src/a/b.rs", "..")`
/// is valid without `src/a` existing beforehand.
///
/// # Panics
///
/// Panics if the directory or any file cannot be created. Acceptable here:
/// this is `#[cfg(test)]`-only fixture setup, and a setup failure should stop
/// the test immediately and loudly rather than run against a partial tree.
pub fn tempdir_with(files: &[(&str, &str)]) -> TempDir {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock is set to a time after the Unix epoch")
        .as_nanos();
    let unique = UNIQUE.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "magi-smoke-test-{}-{nanos}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("failed to create temp fixture directory");
    for (rel, content) in files {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("failed to create parent directory");
        }
        std::fs::write(&path, content).expect("failed to write fixture file");
    }
    TempDir(dir)
}

/// Creates a file symlink at `link` pointing at `target`.
///
/// Platform-specific because Unix and Windows expose different syscalls for
/// file symlinks; there is no portable `std` equivalent.
///
/// # Panics
///
/// Panics if the platform call fails. On Windows this most commonly means
/// Developer Mode is off and the process is not elevated — a test-environment
/// gap, not a harness defect.
/// Windows `ERROR_PRIVILEGE_NOT_HELD`. Creating a symlink there requires either
/// Developer Mode or an elevated process, and the refusal arrives as a raw OS
/// code that `std` does not map to a named [`std::io::ErrorKind`] — so matching
/// on `PermissionDenied` alone silently misses it, which is how this check
/// failed the first time it was written.
const WINDOWS_PRIVILEGE_NOT_HELD: i32 = 1314;

/// Whether the OS refused the operation for lack of privilege, as opposed to
/// failing for a reason that would be a real defect.
fn is_privilege_refusal(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::PermissionDenied
        || e.raw_os_error() == Some(WINDOWS_PRIVILEGE_NOT_HELD)
}

pub fn make_symlink(link: PathBuf, target: PathBuf) -> bool {
    #[cfg(unix)]
    let created = std::os::unix::fs::symlink(&target, &link);
    #[cfg(windows)]
    let created = std::os::windows::fs::symlink_file(&target, &link);

    match created {
        Ok(()) => true,
        // Creating a symlink is a PRIVILEGED operation on Windows unless Developer
        // Mode is on, and some Unix filesystems refuse it too. That refusal says
        // nothing about the code under test, so it is reported to the caller as
        // "could not create" and the caller decides — rather than panicking and
        // turning an environment fact into a red test.
        Err(e) if is_privilege_refusal(&e) => false,
        // Anything else IS a real failure and must not be swallowed.
        Err(e) => panic!("failed to create symlink {link:?} -> {target:?}: {e}"),
    }
}
