// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-17

//! Generates the large payload from the working tree.
//!
//! It is GENERATED rather than read from a fixture because `sbtdd/` is gitignored:
//! a developer who clones does not have it, and the harness would fail on a
//! dependency it could never satisfy. Generated, it also grows with the project.
//!
//! The load-bearing property is the TOKEN count, but the controllable magnitude
//! is BYTES — the token count is asserted at runtime by the scenario instead.

use std::path::{Path, PathBuf};

/// Payload generation could not reach the requested size, or an underlying
/// filesystem operation failed while building it.
///
/// The message always names the requested target byte count (see [`generate`]):
/// omitting it would send the reader hunting for the number that was actually
/// being compared against.
#[derive(Debug)]
pub struct PayloadError(String);

impl std::fmt::Display for PayloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Bytewise on `/`-normalized paths. NOT filesystem order and NOT locale-aware:
/// either of those makes Windows and Linux produce different payloads, and then
/// "the same input byte for byte" is false between the two environments that are
/// being compared.
pub fn sort_deterministically(mut files: Vec<PathBuf>) -> Vec<PathBuf> {
    files.sort_by_key(|p| p.to_string_lossy().replace('\\', "/").into_bytes());
    files
}

/// Recursively collects every `.rs` file under `root` into `out`, skipping
/// symlinks entirely.
///
/// Symlinks are skipped rather than followed: following one could duplicate
/// content already reached through another path, or escape the tree the
/// caller intended to read, and the two platforms resolve them differently —
/// exactly the divergence [`sort_deterministically`] exists to eliminate.
///
/// An unreadable directory is skipped rather than propagated as an error: the
/// caller widens across multiple roots (see [`generate`]), and one missing
/// root (e.g. a project with no `examples/`) must not abort the whole walk.
fn collect_rs(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Skip symlinks: following them can duplicate content or escape the tree.
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// The `Payload` the harness passes to a run. **A struct and not a bare `String`**
/// so the measured size travels with the text: every assertion about "the large
/// payload" is about its SIZE, and re-measuring it downstream is how two numbers
/// start to disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payload {
    /// The generated source text, concatenated from the tree.
    pub text: String,
    /// `text.len()` in bytes, carried alongside so callers never re-measure it.
    pub bytes: usize,
}

/// The three roots walked, in order, until `target_bytes` is reached.
///
/// R17's widening, IMPLEMENTED and not merely described. The roots are tried in
/// this order and each one is walked with the SAME deterministic sort: a
/// fallback without a fixed order produces a different payload on every
/// machine, and "the same input byte for byte" — the property R17 buys to
/// compare modes — would stop being true exactly when it is needed.
///
/// Verified today: `src/` ALONE is 1 024 059 B against a 250 000 B target, four
/// times the margin. The extra roots are dormant, not dead — they exist for the
/// day someone splits a large module, which is a healthy thing to do.
const ROOTS: [&str; 3] = ["src", "tests", "examples"];

/// Builds the large payload by concatenating `.rs` source under `repo_root`,
/// walking [`ROOTS`] in order until at least `target_bytes` bytes have been
/// gathered, then truncating to exactly `target_bytes`.
///
/// # Parameters
///
/// - `repo_root`: the directory whose `src/`, `tests/` and `examples/`
///   subdirectories are walked.
/// - `target_bytes`: the minimum size to reach before truncating; also the
///   exact size of the returned payload on success.
///
/// # Returns
///
/// The generated [`Payload`], truncated to exactly `target_bytes` bytes on a
/// UTF-8 character boundary.
///
/// # Errors
///
/// Returns [`PayloadError`] if the concatenated source across every root in
/// [`ROOTS`] falls short of `target_bytes`. This is deliberate: silently
/// returning a short payload would defeat the scenario this generator exists
/// to serve (see the module docs), so falling short fails loudly and names
/// both the size reached and the target.
///
/// # Complexity
///
/// `O(n)` in the total bytes read across the walked roots: each file is read
/// once, and the deterministic sort is `O(k log k)` in the file count `k` per
/// root, dominated in practice by the file I/O.
pub fn generate(repo_root: &Path, target_bytes: usize) -> Result<Payload, PayloadError> {
    let mut acc = String::with_capacity(target_bytes + 4096);
    for root in ROOTS {
        if acc.len() >= target_bytes {
            break;
        }
        let mut files = Vec::new();
        collect_rs(&repo_root.join(root), &mut files);
        for f in sort_deterministically(files) {
            if acc.len() >= target_bytes {
                break;
            }
            if let Ok(text) = std::fs::read_to_string(&f) {
                acc.push_str(&text);
            }
        }
    }

    if acc.len() < target_bytes {
        return Err(PayloadError(format!(
            "generated payload is {} bytes but the target is {}, even after widening to {:?}. \
             Falling short is the CORRECT failure: the answer is to widen the source set \
             further, NEVER to lower the target — lowering it turns this scenario into the \
             blind spot it exists to cover.",
            acc.len(),
            target_bytes,
            ROOTS
        )));
    }
    // `String::truncate` **PANICS** if the index does not fall on a character
    // boundary, and the payload comes from concatenating sources with accents
    // and symbols: cutting at `target_bytes` outright is a panic waiting for
    // the wrong file. It is the same defect `fit_content` cost in `3.0.1`, and
    // the crate already carries the fix.
    // `floor_char_boundary` is stable since 1.73 and this project's MSRV is
    // **1.91** (`Cargo.toml`), so it is available — `magi-core` itself has used
    // it in `reporting.rs` since `1.0.1` replaced its workaround.
    acc.truncate(acc.floor_char_boundary(target_bytes));
    Ok(Payload {
        bytes: acc.len(),
        text: acc,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{make_symlink, tempdir_with};

    #[test]
    fn ordering_is_bytewise_on_slash_normalized_paths_not_filesystem_order() {
        let files = vec![
            PathBuf::from("src/z/a.rs"),
            PathBuf::from("src/a/z.rs"),
            PathBuf::from("src/A/b.rs"),
        ];
        let sorted = sort_deterministically(files);
        // Bytewise: uppercase 'A' (0x41) sorts before lowercase 'a' (0x61).
        // A locale-aware sort would put them together, and Windows and Linux would
        // then disagree — making "the same input byte for byte" false between the
        // developer's tree and CI, which is exactly what this ordering buys.
        assert_eq!(sorted[0], PathBuf::from("src/A/b.rs"));
        assert_eq!(sorted[1], PathBuf::from("src/a/z.rs"));
        assert_eq!(sorted[2], PathBuf::from("src/z/a.rs"));
    }

    #[test]
    fn falls_short_of_target_fails_loudly_instead_of_returning_a_small_payload() {
        let dir = tempdir_with(&[("src/tiny.rs", "fn main() {}")]);
        let err = generate(dir.path(), 250_000).unwrap_err();
        assert!(
            format!("{err}").contains("250000"),
            "the error must state the target; a short payload that passes silently is the exact \
             blind spot this scenario exists to cover"
        );
    }

    #[test]
    fn symlinks_are_skipped() {
        // Following them could duplicate content or escape the tree, and they behave
        // differently on Windows and Linux — the very divergence the fixed ordering
        // was introduced to eliminate.
        let dir = tempdir_with(&[("src/real.rs", "fn a() {}")]);
        if !make_symlink(
            dir.path().join("src/link.rs"),
            dir.path().join("src/real.rs"),
        ) {
            // Creating a symlink needs a privilege this OS is refusing. That is a
            // fact about the machine, not about `collect_rs`, so the property goes
            // UNVERIFIED and says so — it never reports as passed. It is verified
            // for real on any Linux runner, or on Windows with Developer Mode on.
            eprintln!(
                "SKIP symlinks_are_skipped: this OS refused to create a symlink                  (no privilege); the property is UNVERIFIED here, not passed"
            );
            return;
        }
        let out = generate(dir.path(), 8).unwrap();
        assert_eq!(out.text.matches("fn a()").count(), 1);
    }

    /// R17 token guard. Gated on `e2`, which **MS1 turns on**.
    ///
    /// Until then it is not compiled into the suite; the moment MS1 enables `e2`,
    /// an unimplemented guard **FAILS LOUDLY** instead of silently not existing.
    /// That is the whole point: a stage boundary that someone can forget is not a
    /// boundary.
    #[cfg(feature = "e2")]
    #[test]
    fn the_large_payload_still_produces_a_large_prompt() {
        unimplemented!(
            "R17: read prompt_tokens from the A-5 telemetry of the large run and fail \
             below 50_000. Bytes are what the generator controls; TOKENS are what the \
             scenario is for."
        );
    }

    #[test]
    fn the_e2_token_guard_is_still_present_in_the_source() {
        // Compiled out today, so nothing else would notice its deletion.
        let src = include_str!("payload.rs");
        assert!(
            src.contains("fn the_large_payload_still_produces_a_large_prompt"),
            "the R17 stub was removed; MS1 would enable `e2` over a guard that no longer exists"
        );
    }
}
