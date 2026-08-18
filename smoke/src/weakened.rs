// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-18

//! Format guard for `// WEAKENED_FOR: <id>` marks.
//!
//! SCOPE, declared next to the detection: this catches marks that are MALFORMED,
//! **not** marks that are ABSENT. Its value is that every deliberate weakening
//! becomes countable, so a later stage can close on "none alive" instead of on
//! trust.

use std::path::{Path, PathBuf};

/// The one accepted shape. `<id>` is non-empty and free of whitespace.
const MARK: &str = "// WEAKENED_FOR: ";

/// This module's own file name.
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

/// Comment opener. Only what follows it on a line can be a mark.
const COMMENT_OPEN: &str = "//";

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
/// # Deliberate over-matching
///
/// Any COMMENT containing the word in any casing is treated as an attempted
/// mark, so ordinary prose that happens to use it will be rejected. That is the
/// cheap direction of error, and it is chosen on purpose: a false positive costs
/// one reworded comment, while a false negative leaves a relaxed assertion in
/// the tree forever and the gate reporting green over it. Code outside comments
/// is not examined, so the module's own name is not a violation of itself.
///
/// # Complexity
///
/// `O(n)` in the line's length: one uppercase pass and one substring search.
pub fn validate_line(line: &str) -> Result<(), String> {
    let Some((_, comment)) = line.split_once(COMMENT_OPEN) else {
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
/// **and one per file or directory that could not be READ**. An unreadable entry
/// is reported, never skipped: a guard that quietly passes over what it could not
/// open is a guard that reports success while guarding nothing, which is the
/// exact failure this module exists to prevent.
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
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.file_name().is_some_and(|n| n == GUARD_OWN_FILE) {
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
    fn every_mark_in_the_harness_source_is_well_formed() {
        // Runs over smoke/src/ itself, so a malformed mark added tomorrow fails
        // HERE and not in a grep nobody runs.
        let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        if let Err(bad) = scan_tree(&src_dir) {
            panic!("malformed WEAKENED marks:\n{}", bad.join("\n"));
        }
    }
}
