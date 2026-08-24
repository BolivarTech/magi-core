#!/bin/sh
# Author: Julian Bolivar
# Version: 4.0.0
# Date: 2026-08-24
#
# Generator artifacts must not reach a file that ships.
#
# ## The defect this exists for, which actually happened
#
# A Python string-concatenation placeholder reached `ci/run_all_checks.sh` verbatim,
# reading `# docs.rs builds` followed by an unsubstituted concatenation instead of
# an em dash. `ci/` is packaged, so it was on its way to crates.io, where nothing
# can be corrected in place. It was harmless as a shell comment, and that is the
# point: it was invisible to every gate this project had. `cargo fmt`, `clippy` and
# the doc build all read over a comment without looking at it, and the thing that
# caught it was a person reading the diff.
#
# The cause recurs: prose gets edited by throwaway scripts that build strings by
# concatenation, and a botched concatenation produces text that is syntactically
# fine and semantically nonsense. The same family already cost this project a
# control byte where a backreference belonged, and 97 runs of mojibake that shipped
# to docs.rs for two releases.
#
# ## What it does NOT do, stated so nobody reads it as more
#
# It cannot judge prose. It matches a short list of literal shapes that have no
# reason to appear in this crate's shipped text today. A sentence that is merely
# wrong passes here; that is what review is for.
#
# The shapes are not universally illegitimate, and claiming they were would be the
# same unchecked absolute this guard exists to catch. If a document about Python
# ever needs one of them, the check goes red LOUDLY on a real file, which is the
# safe direction to be wrong in, and the answer then is to narrow the pattern
# rather than to widen the exemption.
#
# Usage: sh ci/check_prose_artifacts.sh [--self-test]
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# The roots that are scanned. Named as a variable because the SELF-TEST injects a
# probe into each one: without that, narrowing this list would leave the self-test
# green while the guard covered nothing. That is not hypothetical -- an earlier
# version of this file injected all three probes into a single file, and a review
# proved by mutation that replacing this list with `ci` alone passed both modes
# while a real artifact sat in `src/lib.rs`.
SCAN_DIRS='src ci docs examples'
SCAN_FILES='README.md CHANGELOG.md Cargo.toml'

# The scan set is declared TWICE, and the duplication IS the check. Above is what
# the scanner walks; below is what it is REQUIRED to walk. Deriving the second
# from the first would make the self-test agree with whatever the first says, and
# a narrowing would sail through -- which is the defect this file already paid for
# once. An expectation computed from the thing it checks asserts nothing.
# Dropping a root is still allowed; it now costs an edit in two places, which is
# exactly the deliberation such a change deserves.
REQUIRED_ROOTS='src ci docs examples README.md CHANGELOG.md Cargo.toml'

# And the same treatment for the extensions, because pinning only the roots left
# the identical hole one level down: the self-test used to discover its probes
# THROUGH `scan_targets`, so narrowing that function's `-name` filter narrowed the
# expectation with it. Measured, not feared -- dropping `*.sh` from the filter and
# planting a placeholder in `ci/run_all_checks.sh`, the exact file this guard was
# written for, left BOTH modes exiting 0. Dropping `*.rs` or `*.md` did go red,
# but only by luck: `examples/` is all `.rs` and `docs/` all `.md`, so those roots
# went empty, while `ci/` holds 90 `.rs` fixtures that kept it looking populated.
#
# Re-running those four mutations, three now go red and `toml` stays green. That
# green is CORRECT and not a fourth hole: no `.toml` exists under any scanned
# directory, and the only one that ships is `Cargo.toml`, which arrives through
# SCAN_FILES rather than the `find`. It is listed for the day one appears, and on
# that day the self-test demands it -- its discovery is a `find` of its own, so it
# sees the file whatever the scanner's filter says.
REQUIRED_EXTS='rs sh md toml'

# THE PATTERNS, one per line. Each is LITERAL on purpose; see below for why the
# brace shapes are not generalised.
#
#   ' + X + '   and   " + X + "     Python concatenation that never got substituted
#   {EM}  {NL}                      the two placeholder names this repo's scripts use
#   chr(NNNN)                       a character escape that leaked instead of resolving
#
# The brace patterns match those two names and NOT a generic `{IDENT}`, which the
# legend of an earlier version implied and a review disproved with `{AUTHOR}`. The
# reason is not laziness: `{NAME}` in uppercase is a legitimate Rust inline format
# argument (`println!("{VERSION}")`), so a generic brace pattern would fire on real
# code in `src/`. A guard that is noisy on correct code gets silenced, and then it
# guards nothing -- the failure this project has written down for its warnings.
# Adding a third placeholder name here is one line; generalising is not safe.
PATTERNS='["'"'"'] \+ [A-Za-z_][A-Za-z0-9_]* \+ ["'"'"']
\{EM\}
\{NL\}
chr\([0-9][0-9]*\)'

# EVERY PATTERN IS EXERCISED BY AT LEAST ONE SHAPE, and the self-test asserts
# that relation rather than an equality of counts. Counting was the previous
# form and it was wrong in both directions at once: too strong, because a pattern
# legitimately needs more than one shape, and too weak, because it says nothing
# about WHICH pattern a shape exercises.
#
# The axis this closes is the ALTERNATION INSIDE a pattern, which is a level
# below the pattern list itself. The concat rule's legend declares two shapes,
# `' + X + '` and `" + X + "`, and one regex covers both through `["']` -- but
# only the single-quoted one was ever injected. Measured: narrowing that class to
# `[']` left BOTH modes exiting 0 with a live `" + EM + "` in `README.md`, the
# crates.io landing page. Same for the digit class: with only `chr(8212)` pinned,
# narrowing `[0-9][0-9]*` to `8212` would keep the probe green while `chr(10)`
# walked out.
#
# So the shapes below vary what the classes range over, not just the rule they
# belong to: both quote characters, an identifier that is upper-only and one that
# carries lowercase, a digit and an underscore, and two `chr` arities with
# different leading digits.
PROBE_SHAPES="# built ' + EM + ' here
# built \" + n_l1 + \" here
# a {EM} here
# a {NL} here
# a chr(8212) here
# a chr(10) here"

# And the shape list is itself declared twice, like the roots and the extensions,
# because it was the one rung of this ratchet with nothing above it. The relation
# below only demands that every PATTERN be covered, so deleting a shape that
# SHARES a pattern with another -- the double-quoted concat, say -- left the
# self-test green at "5 shapes", and from there reopening the exact defect this
# list was written for takes one more edit. Measured. A count is the wrong tool
# for equality and the right one for a floor.
#
# LIMIT, declared rather than left to be found: this catches a shape DELETED, not
# a shape EDITED. Rewriting `" + n_l1 + "` into a second copy of the single-quoted
# form keeps the count at six and loses the alternation it existed to cover. The
# regress stops here on purpose -- pinning that would mean asserting each shape's
# content, which is the shape list a third time.
REQUIRED_SHAPE_COUNT=6

# THE SCAN SET, and what it is NOT. `cargo package --list` emits 175 files; this
# scans four directories and three root files, which is where prose that a human
# wrote lives. It does NOT scan 41 of them: `tests/` and `.github/` are 34, and
# the other seven are cargo-generated or legal files at the root
# (`.cargo_vcs_info.json`, `.gitattributes`, `Cargo.lock`, `Cargo.toml.orig` and
# the three licence files). The full number is given because naming only the two
# directories reads as the complete enumeration, and
# an earlier version of this comment claimed the packaged-consumer step proved
# the set matched the tarball, which was false: that step parses `[[example]]`
# names and compiles examples, and never compares file lists. Said plainly
# rather than left as an implication.
#
# WHAT THE SELF-TEST STILL CANNOT SEE, declared rather than left for the next
# reviewer to find: an exclusion aimed at ONE FILE. The probes are one per
# (root, extension) pair, chosen by `sort | head -1`, so adding
# `| grep -v 'migration-v4.0.md'` to `scan_targets` hides that file while every
# probe keeps passing. Excluding a whole root, an extension, or the anchoring of
# the one exemption is caught; excluding a single file is not. That is the price
# of one probe per pair, and it is paid knowingly: probing every scanned file
# would multiply a check that already runs for a minute by fifty.
#
# It also runs slightly WIDER than the tarball in one place: `docs/test/` is
# scanned and is `exclude`d from the package, so this can go red over the smoke
# certificate. Left that way on purpose -- being noisy about a file that does not
# ship is the harmless direction, and carving an exception into the scan is how
# the interesting direction gets carved next.
#
# THIS FILE IS THE ONE EXEMPTION, and an exemption is the dangerous part of any
# guard, so it is anchored to the exact path rather than matched as a substring
# (an unanchored filter exempted anything whose path merely CONTAINED this name).
# It has to spell out the shapes it looks for in order to document them, so
# scanning itself would fail always. The cost is stated rather than hidden: this
# file's own prose is unguarded, which is acceptable only because it is the file
# whose subject IS those shapes.

scan_targets() {
  find $SCAN_DIRS -type f \
    \( -name '*.rs' -o -name '*.sh' -o -name '*.md' -o -name '*.toml' \) 2>/dev/null |
    sed 's#^\./##' |
    grep -v '^ci/check_prose_artifacts\.sh$'
  ls $SCAN_FILES 2>/dev/null
}

# A scanned path containing whitespace word-splits at the `xargs` below, so
# `grep` receives two paths that do not exist, its complaint goes to /dev/null,
# and the file is skipped in silence. Verified with `docs/my note.md` carrying a
# live placeholder: the plain mode exited 0. None of the 175 packaged paths has
# whitespace, so this REFUSES the condition rather than paying a `grep` per file
# (~500 invocations per pattern, forty times over, in a check that already takes
# a minute) to support a filename this repository does not use.
assert_no_whitespace_paths() {
  _bad="$(scan_targets | grep '[[:space:]]' || true)"
  if [ -n "$_bad" ]; then
    echo "check_prose_artifacts: these paths cannot be scanned (whitespace):" >&2
    printf '%s\n' "$_bad" >&2
    exit 1
  fi
}

scan() {
  _dir="$1"
  echo "$PATTERNS" | while IFS= read -r pat; do
    [ -n "$pat" ] || continue
    ( cd "$_dir" && scan_targets | xargs -r grep -nE "$pat" 2>/dev/null ) || true
  done
}

assert_no_whitespace_paths

if [ "${1:-}" = "--self-test" ]; then
  # A rule verified only against the fixture it was written from proves that it
  # matches itself. This injects each shape into a REAL shipped file and asserts
  # the check goes red -- the standard this project adopted after four rules in
  # `check_redaction.sh` turned out to report success while guarding nothing, one
  # of them because discovery filtered a file out of the scan entirely. That last
  # one is why the probes below cover EVERY scanned root and not just one file:
  # a pattern test cannot see a discovery defect.
  TMP="$(mktemp -d)"
  trap 'rm -rf "$TMP"' EXIT
  cp -r $SCAN_DIRS "$TMP/" 2>/dev/null || true
  cp $SCAN_FILES "$TMP/" 2>/dev/null || true

  # One probe per REQUIRED root, and the probe file itself is discovered rather
  # than named. The previous version named six paths and derived the seventh from
  # `examples/*.rs`, which is the flat layout only: with `examples/<name>/main.rs`
  # -- the form the packaged-consumer check supports -- the substitution yielded
  # nothing, the entry vanished, and nothing complained, because a loop can only
  # report a target that is LISTED. Reproduced: that shape plus a later narrowing
  # of SCAN_DIRS left both modes exiting 0 with a real placeholder in a shipped
  # file.
  #
  # Note which halves are fixed and which is derived, because swapping them
  # silently disarms this. The ROOT and the EXTENSION both come from fixed lists,
  # so narrowing either one is refused; only WHICH FILE of that kind gets picked
  # is discovered, so a rename is absorbed. And the discovery below is a `find` of
  # its own rather than a call into `scan_targets`: deriving the expectation from
  # the function under test is what let the extension hole through.
  fails=0
  PROBE_FILES=""
  for _root in $REQUIRED_ROOTS; do
    # Coverage first: a required root that the scanner no longer walks is the
    # narrowing this self-test exists to refuse, and it is reported as such
    # instead of as a puzzling missing file three lines later.
    case " $SCAN_DIRS $SCAN_FILES " in
      *" $_root "*) ;;
      *)
        echo "SELF-TEST: $_root is required but is not in the scan set" >&2
        fails=$((fails + 1))
        continue
        ;;
    esac
    if [ ! -d "$_root" ]; then
      PROBE_FILES="$PROBE_FILES $_root"
      continue
    fi
    # One probe per (root, extension) pair that EXISTS in the tree. A pair that
    # does not occur is skipped rather than demanded -- `docs/` holds no `.rs`
    # and requiring one would fail on a true statement about the repository.
    _found_any=0
    for _ext in $REQUIRED_EXTS; do
      _probe="$(find "$_root" -type f -name "*.$_ext" 2>/dev/null |
        sed 's#^\./##' |
        grep -v '^ci/check_prose_artifacts\.sh$' |
        LC_ALL=C sort | head -1 || true)"
      [ -n "$_probe" ] || continue
      PROBE_FILES="$PROBE_FILES $_probe"
      _found_any=1
    done
    if [ "$_found_any" -eq 0 ]; then
      echo "SELF-TEST: no scannable file under $_root" >&2
      fails=$((fails + 1))
    fi
  done

  # A shape per pattern, asserted rather than assumed. Fed by here-document and
  # not by a pipe on purpose: a `while read` at the end of a pipeline runs in a
  # subshell, so every `fails` increment inside it would be discarded and this
  # loop would report success no matter what it found.
  n_shapes="$(printf '%s\n' "$PROBE_SHAPES" | grep -c . || true)"
  if [ "$n_shapes" -lt "$REQUIRED_SHAPE_COUNT" ]; then
    echo "SELF-TEST: $n_shapes probe shapes, $REQUIRED_SHAPE_COUNT required" >&2
    fails=$((fails + 1))
  fi

  while IFS= read -r pat; do
    [ -n "$pat" ] || continue
    if ! printf '%s\n' "$PROBE_SHAPES" | grep -qE "$pat"; then
      echo "SELF-TEST: no probe shape exercises pattern: $pat" >&2
      fails=$((fails + 1))
    fi
  done <<EOF
$PATTERNS
EOF
  [ "$fails" -eq 0 ] || exit 1

  # The converse -- a shape that exercises no pattern -- needs no check of its
  # own: the detection loop below injects every shape and demands a hit, so a
  # shape matching nothing goes red there, naming itself.

  for target in $PROBE_FILES; do
    [ -f "$TMP/$target" ] || { echo "SELF-TEST: probe target missing: $target" >&2; fails=$((fails + 1)); continue; }
    while IFS= read -r probe; do
      [ -n "$probe" ] || continue
      printf '%s\n' "$probe" >> "$TMP/$target"
      if [ -z "$(scan "$TMP")" ]; then
        echo "check_prose_artifacts SELF-TEST FAILED: not detected in $target: $probe" >&2
        fails=$((fails + 1))
      fi
      sed -i '$ d' "$TMP/$target"
    done <<EOF
$PROBE_SHAPES
EOF
  done

  # The exemption is the dangerous part of any guard, so it gets a probe of its
  # own: a shipped path that merely CONTAINS this script's name must still be
  # scanned. The anchoring that makes that true was added without a test, and a
  # fix left unpinned is how the previous one came back -- widening the filter to
  # an unanchored substring passes every check above while this one goes red.
  _decoy_dir="$TMP/docs/$(basename "$0").d"
  mkdir -p "$_decoy_dir"
  printf '%s\n' "# a {EM} here" > "$_decoy_dir/probe.md"
  if [ -z "$(scan "$TMP")" ]; then
    echo "check_prose_artifacts SELF-TEST FAILED: the exemption swallowed $_decoy_dir/probe.md" >&2
    fails=$((fails + 1))
  fi
  rm -rf "$_decoy_dir"

  if [ -n "$(scan "$TMP")" ]; then
    echo "check_prose_artifacts SELF-TEST FAILED: clean copy reported a hit" >&2
    fails=$((fails + 1))
  fi

  if [ "$fails" -ne 0 ]; then
    exit 1
  fi
  n_targets="$(printf '%s\n' $PROBE_FILES | grep -c . || true)"
  echo "check_prose_artifacts: self-test OK ($n_shapes shapes x $n_targets probe files caught; clean copy silent)"
  exit 0
fi

HITS="$(scan "$ROOT")"
if [ -n "$HITS" ]; then
  echo "check_prose_artifacts: generator artifact(s) in files that SHIP:" >&2
  echo "$HITS" >&2
  echo "" >&2
  echo "These are unsubstituted placeholders from a script that edited prose by" >&2
  echo "string concatenation. crates.io is immutable, so one of these reaching a" >&2
  echo "release costs another release to withdraw." >&2
  exit 1
fi

echo "check_prose_artifacts: OK (no generator artifacts in shipped files)"
