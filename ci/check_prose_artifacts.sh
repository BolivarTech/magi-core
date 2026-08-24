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

# THE SCAN SET, and what it is NOT. `cargo package --list` emits 184 files; this
# scans four directories and three root files, which is where prose that a human
# wrote lives. It does NOT scan `tests/`, `.github/`, or `graphify-out/` -- about
# fifty shipped files -- and an earlier version of this comment claimed the
# packaged-consumer step proved the set matched the tarball, which was false:
# that step parses `[[example]]` names and compiles examples, and never compares
# file lists. Said plainly rather than left as an implication.
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

scan() {
  _dir="$1"
  echo "$PATTERNS" | while IFS= read -r pat; do
    [ -n "$pat" ] || continue
    ( cd "$_dir" && scan_targets | xargs -r grep -nE "$pat" 2>/dev/null ) || true
  done
}

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

  # One probe per scanned root, so narrowing SCAN_DIRS or SCAN_FILES fails here.
  PROBE_FILES="src/lib.rs ci/run_all_checks.sh docs/migration-v4.0.md README.md CHANGELOG.md Cargo.toml"
  PROBE_FILES="$PROBE_FILES $(ls examples/*.rs 2>/dev/null | head -1)"

  fails=0
  for target in $PROBE_FILES; do
    [ -f "$TMP/$target" ] || { echo "SELF-TEST: probe target missing: $target" >&2; fails=$((fails + 1)); continue; }
    for probe in "# built ' + EM + ' here" "# a {EM} here" "# a chr(8212) here"; do
      printf '%s\n' "$probe" >> "$TMP/$target"
      if [ -z "$(scan "$TMP")" ]; then
        echo "check_prose_artifacts SELF-TEST FAILED: not detected in $target: $probe" >&2
        fails=$((fails + 1))
      fi
      sed -i '$ d' "$TMP/$target"
    done
  done

  if [ -n "$(scan "$TMP")" ]; then
    echo "check_prose_artifacts SELF-TEST FAILED: clean copy reported a hit" >&2
    fails=$((fails + 1))
  fi

  if [ "$fails" -ne 0 ]; then
    exit 1
  fi
  n_targets="$(printf '%s\n' $PROBE_FILES | grep -c .)"
  echo "check_prose_artifacts: self-test OK (3 shapes x $n_targets scanned roots caught; clean copy silent)"
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
