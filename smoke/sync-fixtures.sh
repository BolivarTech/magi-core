#!/bin/sh
# Author: Julian Bolivar
# Version: 4.0.0
# Date: 2026-08-23

# smoke/sync-fixtures.sh — R23. Copies the fixture corpus from the gitignored
# ORIGIN into the tracked tree and REGENERATES manifest.toml with the hashes
# of what it copied.
#
# The manifest is GENERATED, never hand-edited: a hand-written hash is a hash
# that can match nothing.
#
# Until `wanted.txt` exists the sync step below reports that plainly and does
# nothing else — the machinery ships before the data does. That is deliberately
# DIFFERENT output from "wanted.txt exists and lists nothing": an absent input
# and a completed job with zero items must not read as the same thing.
set -eu

# ONE base for both paths: the directory this script lives in.
#
# They used to be resolved against different bases — the source against the
# caller's working directory, the destination against the repository root — so
# the script had no single working-directory contract and answered differently
# depending on where it was invoked from. From the repository root the source
# pointed OUTSIDE the repository; from `smoke/` the destination landed on
# `smoke/smoke/fixtures`, a directory inside the checkout that nothing is
# supposed to create and that no guard over the binary's write sites can see.
#
# `cd` into the directory and read it back with `pwd`, rather than string-joining
# `dirname "$0"`: that resolves `.`, `..` and a relative invocation in one step,
# with the shell doing the work. A `$0` with no `/` in it (`sh sync-fixtures.sh`
# from this directory) has `dirname` return `.`, which is exactly the case the
# `cd` handles.
SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH='' cd -- "$SCRIPT_DIR/.." && pwd)

# An explicit argument still wins, and is taken AS GIVEN — the caller who passes
# one is naming a directory they know, and re-anchoring it here would make an
# absolute path unusable.
SRC="${1:-$REPO_ROOT/sbtdd/ec-evidence}"
# The ONLY directory this script writes into. Everything below writes under it,
# so "creates nothing else" is a property of this one line rather than of every
# `cp` and `>` agreeing.
DST="$SCRIPT_DIR/fixtures"
WANTED="${DST}/wanted.txt"
# The manifest is built HERE and moved into place only once every copy and
# every hash has succeeded.
#
# It used to be built in place, which meant the tracked manifest was truncated
# to its header BEFORE the first copy: a missing source file, or a `sha256_of`
# that could not run, left the script dead half-way with every
# previously-synced fixture declared by nobody. The audit then calls the whole
# corpus orphaned, and the only recovery is a second successful sync — which
# the failure has just shown to be impossible.
#
# Removed on EXIT rather than only on the success path, so an aborted run
# leaves nothing behind for the orphan check to find.
MANIFEST="${DST}/manifest.toml"
MANIFEST_TMP="${DST}/manifest.toml.partial"
trap 'rm -f "$MANIFEST_TMP"' EXIT

test -d "$SRC" || {
  echo "FATAL: source $SRC is missing (it is gitignored, so it only exists" \
       "on a machine that captured it)"
  exit 2
}

# SHA-256, resolved ONCE and up front, because the tool is not the same
# everywhere: GNU coreutils ships `sha256sum` (Linux, git-bash, MSYS) and macOS
# ships none of coreutils — it has `shasum -a 256` instead. Hardcoding
# `sha256sum` made this script fail on macOS at the first fixture, AFTER it had
# already truncated manifest.toml to its header.
#
# Resolved BEFORE anything is written, and the "neither exists" case is FATAL
# rather than a fallback to an empty hash or a skipped column: the manifest's
# whole claim is that a fixture is the file we recorded, and a manifest that
# hashed nothing while looking complete is that claim reporting success while
# guaranteeing nothing. Its own header says a hand-written hash is a hash that
# can match nothing; an absent one is worse.
#
# Both tools print `<hash>  <file>`, so `cut -d' ' -f1` reads the same field
# from either.
if command -v sha256sum >/dev/null 2>&1; then
  sha256_of() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum >/dev/null 2>&1; then
  sha256_of() { shasum -a 256 "$1" | cut -d' ' -f1; }
else
  echo "FATAL: no SHA-256 tool found. This script needs either 'sha256sum'" \
       "(GNU coreutils: Linux, git-bash, MSYS) or 'shasum' (shipped with" \
       "macOS). Install coreutils ('brew install coreutils' provides" \
       "gsha256sum; 'apt install coreutils' on Debian), or run this script on" \
       "a machine that has one — refusing to write a manifest whose hashes" \
       "would be missing."
  exit 3
fi

mkdir -p "$DST"

# The header is (re)written on EVERY run, not just the first: regenerating
# the manifest must not silently drop the documentation of the format it
# produces — a manifest with only [[fixture]] blocks and no explanation of
# what "currency" or "integrity" mean is a manifest the next reader has to
# reverse-engineer.
cat > "$MANIFEST_TMP" <<'HEADER'
# Fixture manifest. GENERATED by smoke/sync-fixtures.sh, never hand-edited: a
# hand-written hash is a hash that can match nothing.
#
# An EMPTY manifest is a legitimate state, not a placeholder. A stage whose
# scenarios replay nothing has nothing to declare, and both directions of the
# cross in smoke/src/fixtures.rs are satisfied vacuously. Declaring entries
# ahead of the scenarios that would consume them trips the orphan check, so
# the corpus and the scenarios that read it arrive together.
#
# manifest.toml — one entry PER (scenario, fixture) PAIR.
#
# A fixture used by two scenarios appears twice: same path, same hash, but its
# OWN currency line — currency is a property of WHAT IT IS USED FOR, not of
# the file. Collapsing the entries would force one currency answer for two
# questions.
#
# INTEGRITY is not CURRENCY: the hash proves the file is the one we recorded,
# NOT that the backend still answers that way. Only a live scenario proves
# currency.
#
# [[fixture]]
# scenario = "S9"                          # must be in the LIVE list of the stage
# path     = "native-E-malformed.json"
# sha256   = "<64 lowercase hex>"
# currency = "verified-by: S9b"         # or "unverified: <reason>"
HEADER

# `wanted.txt` is TRACKED and written by whoever adds fixtures, one line per
# `<file> <scenario> <currency>`. It is the list of what
# the harness needs, and it lives in git even though the corpus does not:
# whoever clones the repo can see WHAT is missing, even before it exists.
#
# The absent-file case is checked EXPLICITLY, not folded into the loop's
# error handling: piping the loop's stderr to /dev/null with a trailing
# `|| true` would make "wanted.txt is not there yet" and "wanted.txt exists
# and lists zero fixtures" print the exact same "synced 0 fixtures" line —
# the one distinction this script exists to preserve.
# DEFERRED, and it must be closed by MS1's fixture task — the one that first
# writes a real `wanted.txt`:
#
#   `read` returns non-zero at end of input, so a final line with NO trailing
#   newline never enters the loop body. That fixture is silently not copied and
#   silently absent from the manifest, and the "synced N fixtures" line below
#   reports the smaller N as a success. Today `wanted.txt` does not exist, so
#   nothing is dropped and the defect is unreachable; the first hand-edited file
#   whose editor does not add a trailing newline makes it reachable.
#
#   The usual close is `while read -r ... || [ -n "$f" ]`, which runs the body
#   once more for the unterminated remainder. Whoever populates the corpus must
#   do it there and pin it with a `wanted.txt` deliberately written without a
#   trailing newline — a fixture list that quietly loses its last entry is the
#   "green by omission" this harness exists to forbid.
if [ -f "$WANTED" ]; then
  while read -r f scenario currency; do
    [ -n "$f" ] || continue
    cp "$SRC/$f" "$DST/$f"
    # The four keys `FixtureEntry` requires. Generating an incomplete entry
    # would make the manifest fail to deserialize — and that error would
    # surface at the preflight, far from the script that produced it.
    printf '\n[[fixture]]\nscenario = "%s"\npath = "%s"\nsha256 = "%s"\ncurrency = "%s"\n' \
      "$scenario" "$f" "$(sha256_of "$DST/$f")" "$currency" \
      >> "$MANIFEST_TMP"
  done < "$WANTED"
else
  echo "NOTE: $WANTED is absent — nothing to sync yet (expected until fixtures are declared)"
fi

# Everything copied and everything hashed: only NOW does the tracked manifest
# change. Reaching this line is the whole precondition — `set -e` means no
# earlier failure can get here.
mv "$MANIFEST_TMP" "$MANIFEST"

# `grep -c` prints a valid count ("0" included) on stdout REGARDLESS of
# whether it matched anything; its exit code just says whether it matched at
# least once (1 = no match). Trailing `|| true` swallows that exit code
# without invoking a second command, so the count is captured exactly once —
# `|| echo 0` here would run grep's own "0" AND echo's "0" into the same
# substitution, printing the count on two lines instead of one.
fixture_count=$(grep -c '^\[\[fixture\]\]' "$MANIFEST") || true
echo "synced ${fixture_count} fixtures"
