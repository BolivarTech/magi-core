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
/// The message always names the thing that could not be satisfied: the
/// requested target byte count for a short payload (see [`generate`]), or the
/// path for a file that could not be read. Omitting either would send the
/// reader hunting for what was actually being compared or opened.
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
///
/// # Known limitation: non-UTF-8 path bytes
///
/// The comparison key comes from `Path::to_string_lossy()`, which replaces any
/// byte sequence that is not valid UTF-8 (or, on Windows, not valid UTF-16)
/// with `U+FFFD`. Two DIFFERENT invalid sequences could therefore collide onto
/// the same replacement character and compare as equal, weakening — not
/// breaking, ties still fall back to the sort's own stable order — the
/// bytewise guarantee for those specific paths.
///
/// This is accepted rather than worked around: the only genuinely
/// cross-platform fix would be comparing `OsStr` bytes directly, but Windows
/// paths are natively UTF-16, not a byte sequence — a Unix-only `as_bytes()`
/// fast path would reintroduce the exact Windows/Linux divergence this
/// function exists to remove, just for a different input class. The tree this
/// generator walks is this project's OWN `.rs` source, which is ASCII by the
/// project's own English-only convention, so a non-UTF-8 filename is not an
/// expected input here — if that ever changes, this is the function to
/// revisit.
pub fn sort_deterministically(mut files: Vec<PathBuf>) -> Vec<PathBuf> {
    files.sort_by_key(|p| p.to_string_lossy().replace('\\', "/").into_bytes());
    files
}

/// Collects every `.rs` file under a TOP-LEVEL root into `out`, skipping
/// symlinks entirely and appending one message to `errors` for anything the
/// walk found but could not inspect.
///
/// Symlinks are skipped rather than followed: following one could duplicate
/// content already reached through another path, or escape the tree the
/// caller intended to read, and the two platforms resolve them differently —
/// exactly the divergence [`sort_deterministically`] exists to eliminate.
///
/// # The root exemption, and exactly how far it reaches
///
/// An unreadable ROOT is skipped rather than reported: the caller widens
/// across multiple roots (see [`generate`]), and one missing root (e.g. a
/// project with no `examples/`) must not abort the whole walk.
///
/// **That exemption stops at this one directory.** Everything deeper goes
/// through [`collect_subdir`], which REPORTS a directory it cannot read
/// instead of returning quietly — a subdirectory reached by the walk is one
/// the walk already saw listed, so failing to open it drops files from the
/// concatenation and the same tree stops producing the same bytes. That is the
/// same non-determinism [`collect_from_entries`] refuses for a single entry,
/// reached one level up.
fn collect_rs(root: &Path, out: &mut Vec<PathBuf>, errors: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    collect_from_entries(entries, root, out, errors);
}

/// The recursive half of the walk: like [`collect_rs`], except that a
/// directory it cannot READ is recorded in `errors` rather than skipped.
///
/// # Why this is not the same question as an unreadable root
///
/// A root is a directory the caller merely HOPES exists — [`ROOTS`] is a
/// widening list and a project without `examples/` is an ordinary, expected
/// input. A subdirectory, by contrast, was already listed as an entry of a
/// directory that opened: it exists, the walk found it, and failing to descend
/// into it silently removes whatever `.rs` files it holds from the payload.
/// The output would then depend on what happened to be readable on this
/// machine, which is precisely the property [`sort_deterministically`] is
/// bought to remove — so it aborts the generation loudly instead
/// ([`generate`]).
///
/// Skipping it was the previous behaviour, inherited from [`collect_rs`]'s
/// single `let ... else { return }`: the entry-level and root-level cases had
/// both been fixed while this one, one level between them, kept reporting
/// success while guarding nothing.
///
/// # Parameters
///
/// * `dir` — the subdirectory to descend into.
/// * `out` — accumulator for the `.rs` paths found.
/// * `errors` — accumulator for what could not be read.
fn collect_subdir(dir: &Path, out: &mut Vec<PathBuf>, errors: &mut Vec<String>) {
    match std::fs::read_dir(dir) {
        Ok(entries) => collect_from_entries(entries, dir, out, errors),
        Err(e) => errors.push(format!(
            "could not read the directory {}: {e}",
            dir.display()
        )),
    }
}

/// The per-entry half of [`collect_rs`], over an ITERATOR of entries rather
/// than over a path.
///
/// # Why an entry failure is an error here and not a skip
///
/// It used to be `entries.flatten()`, which dropped it. A dropped entry is a
/// file missing from the concatenation, so the SAME tree stops producing the
/// same bytes depending on what happened to be readable on this machine —
/// precisely the non-determinism [`sort_deterministically`] is bought to
/// remove, and the same reason [`generate`] already refuses to skip a file it
/// cannot READ. A failure to `stat` an entry the walk already found is the
/// same case by another route, so it is recorded too.
///
/// This is NOT the missing-root exemption in disguise: that one is about a
/// directory that never opened (a project with no `examples/`), while this is
/// about a directory that opened and then hid part of itself.
///
/// # Why it takes the iterator
///
/// So the failure branch is reachable from a test — there is no portable way
/// to make a real `ReadDir` yield an `Err` on demand, and a branch nothing can
/// reach is a branch nothing pins. `std::fs::ReadDir` already **is** an
/// `Iterator<Item = io::Result<DirEntry>>`, so [`collect_rs`] hands over its
/// own iterator unchanged.
///
/// # Parameters
///
/// * `entries` — the directory's entries, each possibly a read failure.
/// * `root` — the directory they came from, for naming it in a message.
/// * `out` — accumulator for the `.rs` paths found.
/// * `errors` — accumulator for what could not be inspected.
///
/// # Complexity
///
/// `O(f)` in the entries under `root`, recursively: one `stat` per entry.
fn collect_from_entries<I>(
    entries: I,
    root: &Path,
    out: &mut Vec<PathBuf>,
    errors: &mut Vec<String>,
) where
    I: IntoIterator<Item = std::io::Result<std::fs::DirEntry>>,
{
    for entry in entries {
        let path = match entry {
            Ok(e) => e.path(),
            Err(e) => {
                errors.push(format!(
                    "could not read an entry of {}: {e}",
                    root.display()
                ));
                continue;
            }
        };
        // Skip symlinks: following them can duplicate content or escape the tree.
        let meta = match std::fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) => {
                errors.push(format!("could not inspect {}: {e}", path.display()));
                continue;
            }
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            // `collect_subdir`, NOT `collect_rs`: the missing-root exemption
            // belongs to the top of the walk only (see both functions' docs).
            collect_subdir(&path, out, errors);
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

/// Extra capacity reserved on top of `target_bytes` when allocating the
/// accumulator string, in bytes.
///
/// The read loop below only checks `target_bytes` BETWEEN files, so the one
/// file that finally crosses the threshold is read and appended in full
/// before the loop notices — this margin absorbs that single-file overshoot
/// so the final `push_str` does not force a reallocation. It is a sizing
/// heuristic, not a correctness bound: if a source file is larger than this,
/// `String::push_str` simply reallocates, exactly as it would with no margin
/// at all.
const CAPACITY_HEADROOM_BYTES: usize = 4096;

/// Character appended when truncating to `target_bytes` lands on a character
/// boundary BELOW it, so the returned payload is exactly the requested size.
///
/// ASCII on purpose: it is one byte per push, so the repair cannot itself
/// overshoot the target. A space is inert in the source text this walks, and
/// at most three are ever added — a UTF-8 character is at most four bytes
/// long, so cutting inside one moves the boundary back by at most three.
const BOUNDARY_PAD: char = ' ';

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
/// The generated [`Payload`], **exactly `target_bytes` bytes** long. The cut
/// itself lands on a UTF-8 character boundary, which can fall up to three
/// bytes short of the target; those bytes are made up with [`BOUNDARY_PAD`]
/// so the returned size is the requested one and not "the requested one,
/// usually". A caller that is told a payload is large enough and receives a
/// smaller one has been told something false, which is exactly the class of
/// silent weakening this harness exists to refuse.
///
/// # Errors
///
/// Returns [`PayloadError`] if the concatenated source across every root in
/// [`ROOTS`] falls short of `target_bytes`. This is deliberate: silently
/// returning a short payload would defeat the scenario this generator exists
/// to serve (see the module docs), so falling short fails loudly and names
/// both the size reached and the target. That check is about the SOURCE SET;
/// it is not what guarantees the returned size, because it runs before the
/// truncation — the padding above is.
///
/// Also returns [`PayloadError`] if a collected `.rs` file cannot be read, if
/// the walk found an entry it could not read or `stat`
/// ([`collect_from_entries`]), or if it found a SUBDIRECTORY it could not open
/// ([`collect_subdir`]). A skip in any of those makes the output depend on
/// which files happened to be readable on this machine, which is precisely the
/// non-determinism [`sort_deterministically`] is bought to remove. An
/// unreadable ROOT DIRECTORY is still skipped, and deliberately so — see
/// [`collect_rs`]: a root that does not exist at all (a project with no
/// `examples/`) is an expected input to the widening, while anything inside a
/// directory that did open is not.
///
/// # Complexity
///
/// `O(f + n)`, NOT `O(n)` alone: [`collect_rs`] walks and `stat`s a root's
/// ENTIRE `.rs` tree unconditionally before a single file is read — the early
/// break on accumulated bytes applies only to the read loop below, never to
/// collection — so the full file count `f` under each walked root is paid
/// even when one file's contents would already satisfy `target_bytes`. `n` is
/// the bytes actually read and concatenated (bounded by roughly
/// `target_bytes` plus one file's overshoot), and the deterministic sort is
/// `O(f log f)` per root, subsumed by the `f` term above.
pub fn generate(repo_root: &Path, target_bytes: usize) -> Result<Payload, PayloadError> {
    let mut acc = String::with_capacity(target_bytes + CAPACITY_HEADROOM_BYTES);
    for root in ROOTS {
        if acc.len() >= target_bytes {
            break;
        }
        let mut files = Vec::new();
        let mut walk_errors = Vec::new();
        collect_rs(&repo_root.join(root), &mut files, &mut walk_errors);
        if !walk_errors.is_empty() {
            return Err(PayloadError(format!(
                "the walk of {} could not inspect everything it found: {}. Dropping those \
                 entries would make the payload depend on the state of the machine, which is \
                 the one property the deterministic order exists to remove.",
                repo_root.join(root).display(),
                walk_errors.join("; ")
            )));
        }
        for f in sort_deterministically(files) {
            if acc.len() >= target_bytes {
                break;
            }
            // A file that cannot be READ is an error, never a skip. The
            // deterministic sort above exists so the same tree yields the same
            // bytes; silently dropping whatever happened to be unreadable makes
            // the payload depend on the state of the machine instead, which is
            // the property the sort was bought to remove.
            let text = std::fs::read_to_string(&f).map_err(|e| {
                PayloadError(format!(
                    "could not read {}: {e}. Skipping it would make the payload depend on \
                     which files happened to be readable, so the same tree would stop \
                     producing the same bytes — the one property the deterministic order \
                     above exists to provide.",
                    f.display()
                ))
            })?;
            acc.push_str(&text);
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
    // The size check above ran BEFORE this cut, and the cut can move the end
    // BACKWARDS by up to `MAX_UTF8_CHAR_BYTES - 1`: a payload validated as
    // large enough came out SHORTER than the target, silently breaking the one
    // contract this generator exists to honour. The shortfall is repaired here,
    // where it is known, rather than turned into an error message about
    // widening the source set — the source set was never the problem, the
    // boundary was.
    for _ in acc.len()..target_bytes {
        acc.push(BOUNDARY_PAD);
    }
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
    fn a_target_landing_inside_a_multibyte_character_still_returns_exactly_target_bytes() {
        // Fix round 2, Finding 3: the "long enough?" check ran BEFORE the UTF-8
        // truncation, so a payload could be validated as large enough and then
        // cut back below the target — the contract broken silently, in the one
        // direction the generator exists to prevent.
        //
        // `"aaaé"` is five bytes: `a a a` then `é` at 3..5. A target of 4 falls
        // INSIDE `é`, so `floor_char_boundary` moves the cut back to 3 and the
        // unfixed generator returned a 3-byte payload while reporting success.
        const TARGET_BYTES: usize = 4;
        let dir = tempdir_with(&[("src/a.rs", "aaaé")]);
        let out = generate(dir.path(), TARGET_BYTES).unwrap();
        assert_eq!(
            out.text.len(),
            TARGET_BYTES,
            "a payload that passed the size check must not come back smaller than the target: \
             {:?}",
            out.text
        );
        assert_eq!(
            out.bytes, TARGET_BYTES,
            "the carried size must be the real one"
        );
        // The cut still lands on a character boundary — the repair adds ASCII
        // padding, it does not slice a character in half.
        assert!(out.text.is_char_boundary(out.text.len()));
    }

    #[test]
    fn an_entry_the_walk_cannot_read_is_an_error_not_a_dropped_file() {
        // The other `.flatten()` of this round. A dropped entry is a file
        // missing from the concatenation, so the same tree stops producing the
        // same bytes — the exact non-determinism the fixed ordering above is
        // bought to remove.
        //
        // Injected rather than forced, and that limit is stated: no portable
        // way exists to make a real `ReadDir` fail on an entry. What this pins
        // is the decision the code makes when handed one.
        let dir = tempdir_with(&[]);
        let mut out = Vec::new();
        let mut errors = Vec::new();
        collect_from_entries(
            vec![Err(std::io::Error::other("simulated entry read failure"))],
            dir.path(),
            &mut out,
            &mut errors,
        );
        assert!(out.is_empty());
        assert_eq!(errors.len(), 1, "the failure must be recorded: {errors:?}");
        assert!(
            errors[0].contains("simulated entry read failure"),
            "the message must name what failed: {:?}",
            errors[0]
        );
    }

    #[test]
    fn symlinks_are_skipped() {
        // Following them could duplicate content or escape the tree, and they
        // behave differently on Windows and Linux — the very divergence the
        // fixed ordering was introduced to eliminate.
        //
        // The fixture makes the two outcomes OBSERVABLE, not merely plausible.
        // Sorted bytewise on `/`-normalised paths the candidates would be
        // `src/aaa.rs` < `src/mmm_link.rs` < `src/zzz_pad.rs`. With the guard
        // intact `collect_rs` never adds the link, so `generate` reads the
        // marker once and pads to TARGET_BYTES from `zzz_pad.rs`. With the
        // guard removed it adds the link too, reads the SAME nine bytes again,
        // reaches TARGET_BYTES there and stops — the marker appears TWICE and
        // `zzz_pad.rs` is never read. TARGET_BYTES sits between the two file
        // counts (9 < 15 < 18) so both branches still return `Ok` and only the
        // count differs: the assertion cannot pass for the wrong reason.
        //
        // A JUNCTION IS NOT A SUBSTITUTE, and it was measured rather than
        // assumed. A Windows junction needs no privilege and `is_symlink()`
        // does report it — but `symlink_metadata()` reports `is_dir() == false`
        // for one, so `collect_rs` reaches neither its directory branch nor,
        // lacking a `.rs` extension, its file branch. The walker therefore
        // ignores a junction whether the guard is present or not, and a test
        // built on one passes identically against a deliberately broken filter.
        // The only input that reaches this guard is a `.rs` FILE symlink, and
        // creating one on Windows needs Developer Mode or elevation.
        const TARGET_BYTES: usize = 15;
        const MARKER_FILE: &str = "fn a() {}";
        const PADDING_FILE: &str =
            "// filler filler filler filler filler filler filler filler filler filler";
        let dir = tempdir_with(&[
            ("src/aaa.rs", MARKER_FILE),
            ("src/zzz_pad.rs", PADDING_FILE),
        ]);
        if !make_symlink(
            dir.path().join("src/mmm_link.rs"),
            dir.path().join("src/aaa.rs"),
        ) {
            // The OS refused the privilege. That is a fact about the machine,
            // not about `collect_rs`, so the property goes UNVERIFIED and says
            // so — it is never reported as passed. It is verified for real on
            // any Linux runner, or on Windows with Developer Mode on.
            eprintln!(
                "SKIP symlinks_are_skipped: this OS refused to create a symlink (no \
                 privilege); the property is UNVERIFIED here, not passed"
            );
            return;
        }
        let out = generate(dir.path(), TARGET_BYTES).unwrap();
        assert_eq!(
            out.text.matches("fn a()").count(),
            1,
            "a followed symlink would duplicate aaa.rs's content and push this to 2"
        );
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

    /// The name the E2 token guard above is declared under. Held as a constant
    /// so the tripwire and the guard cannot drift apart silently.
    const E2_TOKEN_GUARD_FN: &str = "the_large_payload_still_produces_a_large_prompt";

    /// Returns the block-comment nesting depth at the END of `line`, given the
    /// depth it started at. Rust block comments nest, so this counts rather
    /// than latches.
    ///
    /// A `//` encountered at depth `0` ends the scan of that line: everything
    /// after it is a line comment, so a `/*` written there opens nothing.
    ///
    /// # Known limitation
    ///
    /// It does not lex string literals, so a `/*` or `*/` inside one is
    /// counted. That is the CHEAP direction of error: a stray `/*` in a
    /// literal raises the depth and makes the tripwire report the guard
    /// missing — red, loudly, on a file someone just edited — whereas the
    /// direction that matters is a tripwire staying GREEN over a guard that is
    /// gone. Written by hand rather than with a lexer for the same reason:
    /// this is a tripwire over one known file, not a parser.
    ///
    /// # Complexity
    ///
    /// `O(n)` in the line's length.
    fn block_comment_depth_after(line: &str, mut depth: usize) -> usize {
        let b = line.as_bytes();
        let mut i = 0;
        while i + 1 < b.len() {
            if depth == 0 && b[i] == b'/' && b[i + 1] == b'/' {
                break;
            }
            if b[i] == b'/' && b[i + 1] == b'*' {
                depth += 1;
                i += 2;
            } else if depth > 0 && b[i] == b'*' && b[i + 1] == b'/' {
                depth -= 1;
                i += 2;
            } else {
                i += 1;
            }
        }
        depth
    }

    /// Whether `src` DECLARES a function named `name`, as opposed to merely
    /// MENTIONING it.
    ///
    /// "Declares" means a line whose trimmed text begins with `fn <name>` and
    /// which is not commented out. The two comment forms are excluded by
    /// different halves of the check: a `//` in front of the declaration stops
    /// the trimmed line beginning with `fn`, and a surrounding `/* … */` is
    /// caught by the depth carried in from earlier lines
    /// ([`block_comment_depth_after`]).
    ///
    /// # Why a substring search is not enough
    ///
    /// The tripwire below exists so a later stage cannot enable `e2` over a
    /// guard someone removed. `str::contains` answers "is the name written
    /// anywhere in this file", and commenting the guard out leaves the name
    /// written — so the tripwire stayed green while the guard was gone: a
    /// mechanism reporting success while guarding nothing, which is the exact
    /// class of defect this milestone keeps producing. Requiring the name at
    /// the start of an uncommented line means a mention (in prose, in a string
    /// literal, in this very docstring) can never satisfy it.
    ///
    /// # Parameters
    ///
    /// * `src` — the source text to inspect.
    /// * `name` — the function's bare name, without the `fn ` keyword.
    ///
    /// # Complexity
    ///
    /// `O(n)` in the source length.
    fn source_declares_fn(src: &str, name: &str) -> bool {
        let needle = format!("fn {name}");
        let mut depth = 0usize;
        for line in src.lines() {
            if depth == 0 && line.trim_start().starts_with(&needle) {
                return true;
            }
            depth = block_comment_depth_after(line, depth);
        }
        false
    }

    #[test]
    fn the_e2_token_guard_is_still_present_in_the_source() {
        // Compiled out today, so nothing else would notice its deletion.
        assert!(
            source_declares_fn(include_str!("payload.rs"), E2_TOKEN_GUARD_FN),
            "the R17 stub was removed or commented out; MS1 would enable `e2` over a guard that \
             no longer runs"
        );
    }

    #[test]
    fn a_mention_of_the_guard_does_not_satisfy_the_tripwire() {
        // The fifteenth instance of this milestone's recurring defect: the
        // tripwire matched a SUBSTRING, so two slashes in front of the guard
        // left it green while the guard was gone. Every input here contains
        // the name; none of them declares the function.
        let mentions = [
            format!("// fn {E2_TOKEN_GUARD_FN}() {{"),
            format!("    //fn {E2_TOKEN_GUARD_FN}() {{"),
            format!("    let name = \"fn {E2_TOKEN_GUARD_FN}\";"),
            format!("/*\nfn {E2_TOKEN_GUARD_FN}() {{}}\n*/"),
            format!("/* fn {E2_TOKEN_GUARD_FN}() {{}} */"),
            format!("/// See [`fn {E2_TOKEN_GUARD_FN}`] for the token bound."),
        ];
        for src in mentions {
            assert!(
                !source_declares_fn(&src, E2_TOKEN_GUARD_FN),
                "a mention must not satisfy the tripwire: {src:?}"
            );
        }
    }

    #[test]
    fn a_real_declaration_satisfies_the_tripwire_however_it_is_formatted() {
        // The other direction, and it matters just as much: a tripwire that
        // goes red when someone re-indents the guard or adds an attribute is a
        // tripwire that gets deleted. Reformatting must not break it.
        let declarations = [
            format!("fn {E2_TOKEN_GUARD_FN}() {{}}"),
            format!("                fn {E2_TOKEN_GUARD_FN}() {{}}"),
            format!("\t\tfn {E2_TOKEN_GUARD_FN}(\n) {{}}"),
            format!("#[cfg(feature = \"e2\")]\n    #[test]\n    fn {E2_TOKEN_GUARD_FN}() {{}}"),
            // A block comment that OPENED and CLOSED earlier must not leave
            // the scanner stuck thinking the rest of the file is commented.
            format!("/* an earlier note */\nfn {E2_TOKEN_GUARD_FN}() {{}}"),
            // Nor must a `/*` that only ever appears inside a line comment.
            format!("// see /* the note */\nfn {E2_TOKEN_GUARD_FN}() {{}}"),
        ];
        for src in declarations {
            assert!(
                source_declares_fn(&src, E2_TOKEN_GUARD_FN),
                "a real declaration must satisfy the tripwire: {src:?}"
            );
        }
    }

    #[test]
    fn a_subdirectory_that_cannot_be_read_is_an_error_not_a_silent_skip() {
        // The root-level exemption and the entry-level failure were both fixed
        // in earlier rounds; the level BETWEEN them still returned quietly, so
        // a subdirectory the walk had already listed could vanish from the
        // concatenation and the same tree would stop producing the same bytes.
        //
        // Forced by handing `collect_subdir` a path that is a FILE: `read_dir`
        // refuses it on both platforms this project builds on (`ENOTDIR` /
        // `ERROR_DIRECTORY`), which needs no privilege and no permission
        // juggling. What it pins is the DECISION the code makes when a
        // directory read fails, which is the thing that regressed.
        let dir = tempdir_with(&[("src/not_a_dir.rs", "fn main() {}")]);
        let not_a_dir = dir.path().join("src/not_a_dir.rs");

        let mut out = Vec::new();
        let mut errors = Vec::new();
        collect_subdir(&not_a_dir, &mut out, &mut errors);
        assert!(out.is_empty());
        assert_eq!(
            errors.len(),
            1,
            "a subdirectory the walk found and could not read must be reported: {errors:?}"
        );
        assert!(
            errors[0].contains("not_a_dir.rs"),
            "the message must NAME what could not be read: {:?}",
            errors[0]
        );

        // And the root exemption is still exactly that — an exemption for the
        // TOP of the walk only. Asserting the new error without this would
        // pass just as well against a version that reported both, which would
        // break the widening over a project with no `examples/`.
        let mut root_out = Vec::new();
        let mut root_errors = Vec::new();
        collect_rs(&not_a_dir, &mut root_out, &mut root_errors);
        assert!(
            root_errors.is_empty(),
            "a root that cannot be opened is skipped, not reported: {root_errors:?}"
        );
    }
}
