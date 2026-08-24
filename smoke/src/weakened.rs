// Author: Julian Bolivar
// Version: 4.0.0
// Date: 2026-08-23

//! Format guard for `// WEAKENED_FOR: <id>` marks.
//!
//! SCOPE, declared next to the detection: this catches marks that are MALFORMED,
//! **not** marks that are ABSENT. Its value is that every deliberate weakening
//! becomes countable, so a later stage can close on "none alive" instead of on
//! trust.

use std::path::{Path, PathBuf};

/// The one accepted shape. `<id>` is non-empty and free of whitespace.
const MARK: &str = "// WEAKENED_FOR: ";

/// This module's own file name, matched against the scan root rather than by
/// bare basename: `src/` has subdirectories, and a second file of this name in
/// one of them would otherwise be exempted too.
///
/// The tree walk skips it, and the reason is not convenience: this file's test
/// fixtures ARE malformed marks, so scanning it would report the guard's own
/// test data as defects and the self-check could never pass. Narrowing the
/// trigger would not help — the fixtures contain the well-formed token too.
///
/// The hole this leaves is stated rather than hidden: a real malformed mark
/// written inside THIS file escapes the tree walk. It is one small file, it is
/// the guard itself, and `validate_line` still governs every line of it when
/// called directly.
const GUARD_OWN_FILE: &str = "weakened.rs";

/// The token the scan trips on, matched case-insensitively and **only inside a
/// line's comment text**.
///
/// It is deliberately the bare word rather than the well-formed mark: looking
/// for the exact token would find only the marks that are ALREADY correct, so
/// the check would agree with whatever is written — the failure it exists to
/// catch. That width is what lets `// weakened for EC-3`, which no grep for the
/// real mark would ever find, be rejected.
///
/// Restricting it to comment text is not a softening, it is what makes the width
/// usable: a mark IS a comment, while `mod weakened;` and `weakened::scan()` are
/// code. Trying the whole line first put the module's own declaration in
/// violation of its own guard, which is how this restriction was found.
const TRIGGER: &str = "WEAKENED";

/// Comment openers. Only what follows one of them on a line can be a mark.
///
/// BOTH shapes, because the line comment alone left a hole the width of the guard: a
/// weakening written `/* WEAKENED_FOR: X */` contains no `//`, so the scan returned early and
/// the mark escaped -- not malformed, simply invisible. The point of this module is that every
/// deliberate weakening stays countable, and one that cannot be seen is worse than one that is
/// badly formed, because the badly formed one at least fails loudly.
const COMMENT_OPENS: [&str; 2] = ["//", "/*"];

/// Checks ONE line. `Ok(())` also covers lines that carry no mark at all.
///
/// # Parameters
///
/// * `line` — one source line, with or without a mark.
///
/// # Errors
///
/// Returns the reason a line that looks like a mark is not one: either the shape
/// does not match `// WEAKENED_FOR: <id>`, or the id is missing or has spaces in
/// it.
///
/// # Deliberate over-matching, and its TWO sources
///
/// Any COMMENT containing the word in any casing is treated as an attempted
/// mark, so ordinary prose that happens to use it will be rejected. That is the
/// cheap direction of error, and it is chosen on purpose: a false positive costs
/// one reworded comment, while a false negative leaves a relaxed assertion in
/// the tree forever and the gate reporting green over it.
///
/// **The second source is that "comment" here means "whatever follows the first
/// `//` on the line", and that is not the same thing as a Rust comment.** A
/// `//` inside a STRING LITERAL — a URL, or a literal holding a snippet of
/// commented source — opens a "comment" as far as this function is concerned,
/// so a line such as `let s = "// WEAKENED_FOR EC-3";` is checked as a mark and
/// rejected for being malformed. This is a KNOWN and ACCEPTED consequence, not
/// an oversight, and it was left standing after review rather than closed:
///
/// * **It fails in the same cheap direction as the first source.** The cost is
///   an extra rejection, which one reword or one restructure clears. Nothing
///   about it can make a malformed mark pass.
/// * **Closing it means lexing Rust.** Telling a literal `//` from a comment
///   `//` needs string, raw-string, char-literal and escape handling — a lexer
///   living inside a line-oriented guard, whose own bugs would be the thing
///   that lets a mark through. A guard whose failure mode is "too strict" is
///   worth far more here than a cleverer one whose failure mode is "sometimes
///   silent".
///
/// What is NOT over-matched is code with no `//` in it at all: `mod weakened;`
/// and `weakened::scan()` are examined and pass, so the module's own name is
/// not a violation of itself.
///
/// # The friction this costs, priced — raised twice in review, and kept
///
/// Both sources above are FRICTION, and friction is a real objection rather
/// than a fussy one: a guard people find obstructive gets bypassed, and a
/// bypassed guard protects nothing. So the trade is priced here rather than
/// asserted, and the two sides are not the same size.
///
/// **What a false positive costs: one reworded comment.** It surfaces at
/// `cargo test`, from
/// `every_mark_outside_this_guards_own_fixtures_is_well_formed`, as a message
/// naming the file and quoting the offending line — so the author is looking
/// straight at the line to change, in the same edit-run cycle they were already
/// in. It never reaches CI as a mystery, never blocks anything for longer than
/// it takes to reword one sentence, and it is confined to comments that use one
/// specific word.
///
/// **What a false negative costs: the guard's entire reason to exist.** A
/// malformed mark is one that no grep for the real marker will ever find, so
/// the relaxed assertion it was supposed to make countable becomes invisible —
/// and stays invisible, while the suite reports green over it. Nobody goes
/// looking, because the mechanism whose job was to say something has said
/// nothing.
///
/// **Why the trade is taken in this direction.** The two errors are not
/// symmetric in cost, in duration, or in who bears them: the false positive is
/// bounded, immediate, self-announcing and paid by the person who can fix it in
/// one line; the false negative is unbounded, silent, permanent and paid by
/// whoever later trusts a green gate. Narrowing the trigger to the well-formed
/// token would remove the friction entirely and, in the same move, leave the
/// check finding only the marks that were already correct — a check that agrees
/// with whatever is written.
///
/// **On the adoption risk specifically.** What makes a guard get worked around
/// is a cost that is high, recurring or opaque. This one is none of those: the
/// only workaround it invites is rewording a comment, which is also the
/// compliant outcome, and the cheapest way past it is to write the mark
/// correctly. If the friction ever stops being cheap in practice — measured, by
/// how often it fires on innocent prose, not predicted — the answer is to
/// narrow the SHAPE that counts as a comment, never to narrow the trigger word.
///
/// # Complexity
///
/// `O(n)` in the line's length: one uppercase pass and one substring search.
pub fn validate_line(line: &str) -> Result<(), String> {
    // The EARLIEST opener on the line, not the first one that happens to match: a line
    // carrying both would otherwise be split at whichever the array lists first, and the text
    // before that point would be searched as if it were comment text.
    let Some(comment) = COMMENT_OPENS
        .iter()
        .filter_map(|open| line.find(open).map(|i| (i, i + open.len())))
        .min_by_key(|(start, _)| *start)
        .map(|(_, after)| &line[after..])
    else {
        return Ok(());
    };
    if !comment.to_ascii_uppercase().contains(TRIGGER) {
        return Ok(());
    }
    let Some(rest) = line.split_once(MARK).map(|(_, r)| r) else {
        return Err(format!(
            "malformed mark, expected `{}<id>`: {line:?}",
            MARK.trim_start()
        ));
    };
    let id = rest.trim();
    if id.is_empty() || id.split_whitespace().count() != 1 {
        return Err(format!("mark needs exactly one non-empty id: {line:?}"));
    }
    Ok(())
}

/// Checks a whole source file, returning EVERY malformed line rather than the
/// first — one run should show all the work, not one item of it.
///
/// # Parameters
///
/// * `src` — the file's full contents.
///
/// # Errors
///
/// One message per malformed line, in source order.
///
/// # Complexity
///
/// `O(n)` in the file's length.
pub fn scan(src: &str) -> Result<(), Vec<String>> {
    let bad: Vec<_> = src.lines().filter_map(|l| validate_line(l).err()).collect();
    if bad.is_empty() {
        Ok(())
    } else {
        Err(bad)
    }
}

/// Walks a directory and applies [`scan`] to every `.rs` file under it.
///
/// **A validator with no call site over the real tree validates nothing**:
/// `scan` on a string literal only proves the parser works.
///
/// # Parameters
///
/// * `root` — the directory to walk, recursively.
///
/// # Errors
///
/// One message per malformed line, each prefixed with the file it came from —
/// **and one per file or directory that could not be READ**, or that could not
/// be INSPECTED. An unreadable entry is reported, never skipped: a guard that
/// quietly passes over what it could not open is a guard that reports success
/// while guarding nothing, which is the exact failure this module exists to
/// prevent.
///
/// # Symlinks are SKIPPED, not followed
///
/// The same policy [`crate::payload::collect_rs`] applies to the same tree, for
/// the same reasons — two walks over one tree with opposite policies is a
/// disagreement waiting to be discovered by whichever one is wrong.
///
/// This walk branched on `Path::is_dir`, which FOLLOWS a link, and it keeps no
/// visited set. Concretely, that meant:
///
/// * **A directory link pointing OUT of the tree** made the guard scan files
///   that are not the harness's, and report their marks as this tree's defects.
///   The [`GUARD_OWN_FILE`] exemption compares against `root.join(..)`, so a
///   link reaching this very file under any other name would have had the
///   guard's own deliberately-malformed fixtures reported as violations —
///   turning `every_mark_outside_this_guards_own_fixtures_is_well_formed` red
///   over nothing.
/// * **A directory link pointing INTO an ancestor** made the walk cycle, and
///   with no visited set the stack grows without bound rather than terminating.
/// * **A `.rs` FILE link** made one malformed mark get reported twice, so the
///   count of defects stopped being a count of defects.
///
/// A link that cannot be inspected is REPORTED, on the same principle as an
/// unreadable directory: silence would be the guard passing over something it
/// could not classify.
///
/// # Complexity
///
/// `O(f + n)` for `f` filesystem entries and `n` total bytes of `.rs` source.
pub fn scan_tree(root: &Path) -> Result<(), Vec<String>> {
    let mut bad = Vec::new();
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) => {
                bad.push(format!("{}: could not read directory: {e}", dir.display()));
                continue;
            }
        };
        for entry in entries {
            let path = match entry {
                Ok(e) => e.path(),
                Err(e) => {
                    bad.push(format!("{}: could not read an entry: {e}", dir.display()));
                    continue;
                }
            };
            // `symlink_metadata`, NOT `path.is_dir()`: the latter FOLLOWS a
            // link, and following one here can escape the tree or cycle
            // forever (see this function's "Symlinks are SKIPPED" section).
            let meta = match std::fs::symlink_metadata(&path) {
                Ok(m) => m,
                Err(e) => {
                    bad.push(format!("{}: could not inspect: {e}", path.display()));
                    continue;
                }
            };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                stack.push(path);
                continue;
            }
            if path == root.join(GUARD_OWN_FILE) {
                continue;
            }
            if path.extension().is_some_and(|x| x == "rs") {
                match std::fs::read_to_string(&path) {
                    Ok(src) => {
                        if let Err(errs) = scan(&src) {
                            bad.extend(
                                errs.into_iter().map(|m| format!("{}: {m}", path.display())),
                            );
                        }
                    }
                    Err(e) => bad.push(format!("{}: could not read file: {e}", path.display())),
                }
            }
        }
    }
    if bad.is_empty() {
        Ok(())
    } else {
        Err(bad)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A weakening written in a BLOCK comment is SEEN, and therefore rejected.
    ///
    /// It used to be invisible rather than rejected: the scan looked only for `//`, so a
    /// `/* WEAKENED_FOR: ... */` returned early and the mark never entered the count at all.
    /// Being rejected is the right outcome and not a lesser one -- there is exactly ONE
    /// accepted shape, and `MARK` carries the `//` in it, so a block comment cannot express
    /// it. What matters is that it now fails LOUDLY instead of passing unseen: a weakening
    /// nobody can count is worse than one that is badly written, because the badly written
    /// one stops the build.
    #[test]
    fn a_weakening_in_a_block_comment_is_rejected_rather_than_skipped() {
        for line in [
            "/* WEAKENED_FOR: EC-3 */",
            "/* weakened for EC-3 */",
            "/* WEAKENED_FOR: */",
        ] {
            assert!(
                validate_line(line).is_err(),
                "a block-comment weakening must be caught, not skipped: {line:?}"
            );
        }
        // The accepted shape still passes, so the widening did not turn the guard into a wall.
        assert!(validate_line("// WEAKENED_FOR: EC-3").is_ok());
    }

    /// The EARLIEST opener wins, so an opener quoted inside another comment is just text.
    ///
    /// With a fixed opener order, a line carrying both would be split at whichever the array
    /// listed first and the code before that point would be searched as comment text.
    #[test]
    fn the_earliest_opener_wins_so_code_is_never_read_as_a_comment() {
        // Both of these mention the trigger inside a comment without being the accepted
        // shape, so both are refused -- the point here is WHERE the split happens, which the
        // messages below would reveal if it moved.
        assert!(validate_line("let x = 1; // /* WEAKENED_FOR: EC-9 */").is_err());
        assert!(validate_line("let y = 2; /* // WEAKENED_FOR: EC-9 */").is_err());
        // And a line whose only comment carries the accepted shape still passes, whichever
        // opener precedes it in the array.
        assert!(validate_line("let z = 3; // WEAKENED_FOR: EC-9").is_ok());
    }
    use crate::testkit::{make_dir_link, tempdir_with};

    /// A mark the guard must reject, used as the CONTENT of the file the link
    /// reaches. It is malformed on purpose: if the walk follows the link, this
    /// is what it finds and reports.
    const A_MALFORMED_MARK: &str = "assert!(x); // weakened for EC-3\n";

    #[test]
    fn a_directory_link_is_skipped_rather_than_followed_out_of_the_tree() {
        // The payload generator skips links deliberately; this walk branched on
        // `Path::is_dir`, which FOLLOWS one — two walks over the same tree with
        // opposite policies. Following a link here scans files that are not the
        // harness's and reports their marks as this tree's defects; a link into
        // an ancestor cycles forever, since the walk keeps no visited set.
        let outside = tempdir_with(&[("leaked.rs", A_MALFORMED_MARK)]);
        let scanned = tempdir_with(&[("keep.rs", "// nothing to see here")]);

        // POSITIVE CONTROL, and it is what stops this test passing for the
        // wrong reason: the fixture really is a mark the guard rejects, so an
        // `Ok` below can only mean the link was not followed — never that
        // there was nothing to find.
        let direct = scan_tree(outside.path())
            .expect_err("the fixture must be a mark the guard rejects, or this test is vacuous");
        assert_eq!(direct.len(), 1);

        if !make_dir_link(scanned.path().join("linked"), outside.path().to_path_buf()) {
            // A fact about the machine, not about `scan_tree`. Reported as
            // UNVERIFIED rather than passed — a green here would be the guard
            // reporting success while checking nothing, which is the defect
            // class this whole module exists to close.
            eprintln!(
                "SKIP a_directory_link_is_skipped_rather_than_followed_out_of_the_tree: this \
                 OS refused to create a directory link; the property is UNVERIFIED here, not \
                 passed"
            );
            return;
        }

        assert!(
            scan_tree(scanned.path()).is_ok(),
            "a followed link would have reached leaked.rs and reported a mark from OUTSIDE \
             the scanned tree as one of its defects"
        );
    }

    #[test]
    fn a_well_formed_mark_is_accepted() {
        assert!(validate_line("    // WEAKENED_FOR: EC-3").is_ok());
    }

    #[test]
    fn a_malformed_mark_fails_instead_of_becoming_invisible_to_grep() {
        // This is the whole point. `// weakened for EC-3` would silently not
        // match the closing grep, so the assertion stays weak FOREVER and the
        // gate reports green. The mechanism that reports success is the one that
        // must be attacked.
        for bad in [
            "// weakened for EC-3",
            "// WEAKENED_FOR EC-3",
            "// WEAKENED_FOR:",
            "//WEAKENED_FOR: x",
        ] {
            assert!(
                validate_line(bad).is_err(),
                "{bad:?} must be rejected, not ignored"
            );
        }
    }

    #[test]
    fn the_scan_covers_every_line_containing_the_word_in_any_casing() {
        // Scanning for the exact well-formed token would find only the marks
        // that are already correct — the check would agree with whatever is
        // written, which is the failure it exists to catch.
        let src = "let x = 1; // Weakened_For: nope\n";
        assert_eq!(scan(src).unwrap_err().len(), 1);
    }

    #[test]
    fn the_declared_limitation_is_absent_marks_not_malformed_ones() {
        // Someone who relaxes an assertion WITHOUT marking it stays invisible,
        // and there is no general way to detect that — it would mean guessing
        // the intent of a change. Detecting "assertions that look relaxed"
        // heuristically would give false positives and need maintaining.
        //
        // The comment below deliberately avoids the trigger word, because the
        // guard over-matches on purpose: prose using it IS rejected, and the
        // plan's original example for this test contained the word and would
        // therefore have contradicted the implementation it shipped with.
        let src = "assert!(true); // relaxed silently, no mark\n";
        assert!(scan(src).is_ok(), "the guard does not claim to catch this");
    }

    #[test]
    fn prose_using_the_trigger_word_is_rejected_and_that_is_the_cheap_direction() {
        // Pins the over-matching as DELIBERATE rather than incidental, so nobody
        // narrows the trigger to the well-formed token to silence a comment —
        // which would turn the guard into one that only ever agrees with marks
        // that were already correct.
        assert!(validate_line("// this assertion was weakened, see the notes").is_err());
    }

    #[test]
    fn a_marker_shaped_string_literal_is_treated_as_a_mark_and_that_is_declared() {
        // Raised in review: "comment" here means "text after the first `//` on
        // the line", so a `//` inside a STRING LITERAL opens one as far as
        // this guard is concerned. The over-match therefore has a second
        // source, and this test exists so that source is DELIBERATE and
        // documented rather than discovered again later.
        //
        // The behaviour is kept, not fixed: it errs strict, which is the same
        // cheap direction as the prose over-match, and closing it would mean
        // lexing Rust string literals inside a line-oriented guard. See
        // `validate_line`'s "Deliberate over-matching, and its TWO sources".
        let literal_holding_a_malformed_mark = concat!("let s = \"", "// WEAKENED_FOR EC-3\";");
        assert!(
            validate_line(literal_holding_a_malformed_mark).is_err(),
            "a marker-shaped literal is rejected — strict, which is the affordable direction"
        );
        // And a WELL-FORMED mark inside a literal is accepted, which is what
        // shows the rule really is "the shape decides", not "literals are
        // special": nothing about this second source can let a malformed mark
        // through.
        let literal_holding_a_well_formed_mark = concat!("let s = \"", "// WEAKENED_FOR: EC-3\";");
        assert!(validate_line(literal_holding_a_well_formed_mark).is_ok());
    }

    #[test]
    fn code_that_merely_names_the_module_is_not_a_violation_of_it() {
        // Found the hard way: the first version examined the whole line, so
        // `mod weakened;` in main.rs put the module in violation of its own
        // guard. A mark is a COMMENT; code is not.
        assert!(validate_line("mod weakened; // Task 11").is_ok());
        assert!(validate_line("use crate::weakened::scan_tree;").is_ok());
        assert!(validate_line("    weakened::scan(src)?;").is_ok());
    }

    #[test]
    fn an_unreadable_path_is_reported_and_never_skipped() {
        // A guard that quietly passes over what it could not open reports
        // success while guarding nothing. The plan's version used
        // `let Ok(..) else { continue }` and would have done exactly that.
        let missing = Path::new("this-directory-does-not-exist-anywhere");
        let errs = scan_tree(missing).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(
            errs[0].contains("could not read directory"),
            "the message must say what could not be read: {:?}",
            errs[0]
        );
    }

    #[test]
    fn a_source_file_that_cannot_be_decoded_is_reported_too() {
        // The directory-read failure had a test; the FILE-read failure did not,
        // so a mutation reintroducing a silent skip at that second point would
        // have gone unnoticed. A `.rs` file that is not valid UTF-8 forces it
        // portably, and is a realistic way for a source file to be unreadable.
        let dir = tempdir_with(&[("keep.rs", "// nothing to see")]);
        std::fs::write(dir.path().join("broken.rs"), [0xff_u8, 0xfe, 0xff])
            .expect("writing the fixture must succeed");
        let errs = scan_tree(dir.path()).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(
            errs[0].contains("could not read file"),
            "the message must say the file could not be read: {:?}",
            errs[0]
        );
    }

    #[test]
    fn every_mark_outside_this_guards_own_fixtures_is_well_formed() {
        // Runs over smoke/src/ itself, so a malformed mark added tomorrow fails
        // HERE and not in a grep nobody runs.
        //
        // The NAME says "outside this guard's own fixtures" because that is what
        // the scan does: `scan_tree` skips this file, whose test data IS a set of
        // deliberately malformed marks. Calling it "every mark in the harness
        // source" promised a completeness the code does not deliver — the same
        // naming defect this project already paid for once, in a field whose name
        // announced a record it never wrote.
        let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        if let Err(bad) = scan_tree(&src_dir) {
            panic!("malformed WEAKENED marks:\n{}", bad.join("\n"));
        }
    }
}
