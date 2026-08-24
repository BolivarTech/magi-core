#!/bin/sh
# Author: Julian Bolivar
# Version: 4.0.0
# Date: 2026-08-23
#
# Generator artifacts must not reach a file that ships.
#
# ## The defect this exists for, which actually happened
#
# A Python string-concatenation placeholder reached `ci/run_all_checks.sh` verbatim:
#
#     # docs.rs builds ' + EM + ' which is no `default` at all
#
# `ci/` is packaged, so it was on its way to crates.io, where nothing can be
# corrected in place. It was harmless as a shell comment, and that is the point:
# it was invisible to every gate the project had. `cargo fmt`, `clippy` and the
# doc build all pass over a comment without looking at it, and the one thing that
# caught it was a human reading the diff.
#
# The cause is worth naming because it recurs: prose gets edited by throwaway
# scripts that build strings by concatenation, and a botched concatenation
# produces text that is syntactically fine and semantically nonsense. The same
# family already cost this project a control byte where a backreference belonged,
# and 97 runs of mojibake that shipped to docs.rs for two releases.
#
# ## What it does NOT do
#
# It cannot judge prose. It matches a small set of shapes that are never
# legitimate in shipped text, and nothing else. A sentence that is merely wrong
# passes here; that is what review is for.
#
# Usage: sh ci/check_prose_artifacts.sh [--self-test]
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# One pattern per line: a shape that has no legitimate reason to appear in text
# this crate publishes. Kept short on purpose — a long list of guesses ages badly,
# and the next unlisted shape is the one that leaks, so this stays at "artifacts
# that provably reached the tree" rather than "everything imaginable".
#
#   ' + X + '   " + X + "     Python concatenation that never got substituted
#   {X}                       an unfilled format placeholder in prose
#   chr(NNNN)                 a character escape that leaked instead of resolving
PATTERNS='["'"'"'] \+ [A-Za-z_][A-Za-z0-9_]* \+ ["'"'"']
\{EM\}
\{NL\}
chr\([0-9][0-9]*\)'

# The files that SHIP. `cargo package --list` is the authority on that, but this
# check must run without packaging (it is cheap and runs per-commit), so the set
# is the shipped directories, and the packaged-consumer step is what proves the
# set matches the tarball.
#
# THIS FILE IS THE ONE EXEMPTION, and an exemption is the dangerous part of any
# guard, so it is narrow and it is named: this script has to spell out the shapes
# it looks for in order to document them, so scanning itself would fail always.
# Nothing else is exempt. The cost is real and stated rather than hidden: this
# file's own prose is unguarded, which is acceptable only because it is the file
# whose subject IS those shapes. `check_redaction.sh` carries the same exemption
# for its fixtures, for the same reason.
scan_targets() {
  find src ci docs examples -type f \
    \( -name '*.rs' -o -name '*.sh' -o -name '*.md' -o -name '*.toml' \) 2>/dev/null |
    grep -v 'check_prose_artifacts\.sh'
  ls README.md CHANGELOG.md Cargo.toml 2>/dev/null
}

scan() {
  _dir="$1"
  _hits=0
  echo "$PATTERNS" | while IFS= read -r pat; do
    [ -n "$pat" ] || continue
    ( cd "$_dir" && scan_targets | xargs -r grep -nE "$pat" 2>/dev/null ) || true
  done
}

if [ "${1:-}" = "--self-test" ]; then
  # A rule verified only against the fixture it was written from proves that it
  # matches itself. This injects each shape into a REAL shipped file, in a copy of
  # the tree, and asserts the check goes red — the standard `check_redaction.sh`
  # set for this project after four of its rules turned out to report success
  # while guarding nothing.
  TMP="$(mktemp -d)"
  trap 'rm -rf "$TMP"' EXIT
  cp -r src ci docs README.md CHANGELOG.md Cargo.toml "$TMP/" 2>/dev/null || true
  mkdir -p "$TMP/examples" && cp examples/*.rs "$TMP/examples/" 2>/dev/null || true

  fails=0
  for probe in "# built ' + EM + ' here" "# a {EM} here" "# a chr(8212) here"; do
    printf '%s\n' "$probe" >> "$TMP/ci/run_all_checks.sh"
    if [ -z "$(scan "$TMP")" ]; then
      echo "check_prose_artifacts SELF-TEST FAILED: not detected: $probe" >&2
      fails=$((fails + 1))
    fi
    # Undo, so each probe is tested alone rather than riding the previous one.
    sed -i '$ d' "$TMP/ci/run_all_checks.sh"
  done

  if [ -n "$(scan "$TMP")" ]; then
    echo "check_prose_artifacts SELF-TEST FAILED: clean copy reported a hit" >&2
    fails=$((fails + 1))
  fi

  if [ "$fails" -ne 0 ]; then
    exit 1
  fi
  echo "check_prose_artifacts: self-test OK (3 shapes injected and caught, clean copy silent)"
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
