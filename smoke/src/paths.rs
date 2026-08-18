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
