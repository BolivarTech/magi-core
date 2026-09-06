#!/bin/sh
# Author: Julian Bolivar
# Version: 4.1.0
# Date: 2026-09-05
#
# Generator artifacts must not reach a file that ships.
#
# ## The defect this exists for, which actually happened
#
# A Python string-concatenation placeholder reached `ci/run_all_checks.sh` verbatim,
# reading `# docs.rs builds` followed by an unsubstituted concatenation instead of
# an em dash. `ci/` WAS packaged then, so it was on its way to crates.io, where
# nothing can be corrected in place. It is excluded as of 4.1.0, so that exact path
# is closed -- but the guard is not: `src/`, `docs/`, `examples/`, `README.md`,
# `CHANGELOG.md` and `Cargo.toml` all still ship, and they are where prose a human
# wrote reaches a consumer. The founding incident is kept as history, not as a
# live description. The placeholder was harmless as a shell comment, and that is
# the point: it was invisible to every gate this project had. `cargo fmt`, `clippy` and
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

# `CDPATH=` here too, and this is the site that made the claim below false for
# one round. `$(dirname "$0")/..` is RELATIVE with no leading `./` when the
# script is invoked as `sh ci/check_prose_artifacts.sh`, which is exactly how
# `run_all_checks.sh` invokes it -- so CDPATH applies, `cd` prints the directory
# it landed in, and even a benign `CDPATH=.` breaks the gate. It fails CLOSED,
# which is why nobody noticed.
ROOT="$(CDPATH= cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# The roots that are scanned. Named as a variable because the SELF-TEST injects a
# probe into each one: without that, narrowing this list would leave the self-test
# green while the guard covered nothing. That is not hypothetical -- an earlier
# version of this file injected all three probes into a single file, and a review
# proved by mutation that replacing this list with `ci` alone passed both modes
# while a real artifact sat in `src/lib.rs`.
# `tests/fixtures` is here because R-32 keeps it in the package: it holds
# `tests/fixtures/ec/README.md`, prose that a human wrote and that SHIPS. The third
# review pass found it sitting outside the scan while a comment below claimed the
# only unscanned packaged files were cargo-generated -- so the hole is closed rather
# than described. The rest of `tests/` is excluded from the package and stays out.
SCAN_DIRS='src ci docs examples tests/fixtures'
SCAN_FILES='README.md CHANGELOG.md Cargo.toml'

# The scan set is declared TWICE, and the duplication IS the check. Above is what
# the scanner walks; below is what it is REQUIRED to walk. Deriving the second
# from the first would make the self-test agree with whatever the first says, and
# a narrowing would sail through -- which is the defect this file already paid for
# once. An expectation computed from the thing it checks asserts nothing.
# Dropping a root is still allowed; it now costs an edit in two places, which is
# exactly the deliberation such a change deserves.
# `tests/fixtures` entered BOTH halves in the same commit, and the reason gets a
# sentence because an earlier version of this comment got the history wrong. A
# draft in the working tree had it in SCAN_DIRS alone; deleting it from there
# left the self-test GREEN, because an unpinned root is never probed. That draft
# was never committed -- `git log -S` finds this DECLARATION in one commit only;
# the bare string now matches more, since every correction touches it -- so
# calling it a round that happened was a false record about a state no later
# reader can reconstruct. What the measurement showed is the part that survives:
# the two halves have to move together, or the second buys nothing.
REQUIRED_ROOTS='src ci docs examples tests/fixtures README.md CHANGELOG.md Cargo.toml'

# AND A FLOOR UNDER THAT LIST, because without it the ratchet held in ONE
# DIRECTION ONLY. Measured, both ways: deleting a root from SCAN_DIRS goes red,
# and deleting the same root from REQUIRED_ROOTS is GREEN -- the probe count
# silently drops and nothing objects. So the narrowing this pair exists to make
# expensive was available in two edits that are green at every step, provided
# you removed the expectation first. That is the rung with nothing above it,
# exactly as the shape list was one level down, and it is closed the same way.
# Adding a legitimate root means bumping this number, which is the deliberation
# such a change deserves.
#
# It counts DISTINCT roots, and the `sort -u` is the whole fix rather than a
# tidiness. Without it the floor was satisfied by a REPEAT: dropping
# `tests/fixtures` and writing `docs` twice keeps the length at eight, and the
# fifth review pass demonstrated the consequence -- a live artifact planted in
# `tests/fixtures/ec/README.md`, a file that SHIPS, passed both modes with the
# probe count silently down from 13 to 12. A floor over a multiset is not a
# floor.
#
# `sort -u` over the raw strings was not enough either, and THREE consecutive
# review passes each broke it with a cheaper spelling than the last: `docs` twice,
# then `docs/`, then `docs/.`. Two of those were met by stripping the shape that
# had just been used -- a leading `./`, a trailing `/` -- and the next pass
# arrived with `tests//fixtures` and `tests/./fixtures`, which those strips do not
# touch. Every time, the same symptom: probe count silently down, a live artifact
# in a file that SHIPS, both modes green.
#
# So the enumeration was abandoned, and that is the point of this block rather
# than a detail of it. Guessing which spellings someone might write is the shape
# that failed three times; asking the FILESYSTEM what a path IS cannot be
# out-spelled, because `..`, `//`, `/.` and a symlink all resolve before the
# comparison ever happens. `canonical_root` below does that with `pwd -P`. It is
# the same lesson this crate learned about enumerating invisible code points and
# about a vendor map of model names: a list of the cases you thought of ages, and
# the next one nobody listed is the one that gets through.
#
# Canonicalising was necessary and not sufficient, and the eighth pass found the
# gap in the REASON this paragraph gave for stopping rather than in its claim.
# `src/prompts` is a genuinely different root by the letter of it, so swapping
# `tests/fixtures` for it held the count at eight -- and cost NOTHING, because it
# is a subdirectory of a root already listed: no new directory, no new file, and
# it reads as plausible in a diff. Probe count 15 to 12, live artifact shipping,
# both modes green. The old sentence implied that escape was expensive. It is
# not even the quietest one: swapping in `smoke` costs the same and takes the
# probe count UP, from 15 to 16, where nesting took it down -- so a reviewer
# watching that number for a drop would miss it. Said here because the sentence
# this replaced called nesting the cheapest available, which is a superlative
# nobody measured.
#
# It is closed by a RELATION over the canonical roots, which is why the reason
# for stopping was wrong: asking whether one root sits under another asserts
# nothing about what the roots ought to be, so it is not REQUIRED_ROOTS a third
# time. Nesting is refused below, beside the count.
#
# LIMIT, declared rather than left to be found, in the same terms as the shape
# count above -- and this is the fourth version of this paragraph, each of the
# previous three having claimed a smaller residual than the mechanism had. What
# survives is a root swapped for one that is neither nested nor nesting, and a
# CASE variant on a case-insensitive filesystem: `pwd -P` returns `DOCS` as
# typed, so Windows counts it twice while Linux has two different directories and
# is right to. Folding case would be wrong on the platform CI runs. The regress
# stops here, and the reason is COST rather than impossibility -- which is the
# fifth version of this sentence and the first that does not outrun its
# mechanism. A relation does exist: the roots ought to cover every packaged
# directory holding a scannable file, and `cargo package --list` can be read for
# it, as the paragraph further down already does by hand. It is not encoded
# because that check would run the packaging step on every self-test, and the
# earlier claim that it was "not a relation" was simply false.
REQUIRED_ROOT_COUNT=8

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
#
# The paragraph above analyses FOUR mutations and there are now SEVEN
# extensions: `py` arrived one round later, then `sha256`, then `json`. The
# ratchet was measured on `py` rather than assumed -- removing `*.py` from the
# filter while leaving it here goes RED, twelve failures across both roots that
# have one -- and the mechanism is the same for the other two.
#
# `json` rests on ONE root, which is worth saying because `_found_any` is per
# ROOT and nothing asserts that a given extension is exercised anywhere:
# `tests/fixtures` is the only scanned root that holds a `.json`, so if it ever
# left the scan this entry would go vacuous in silence. `toml` has the same
# shape and its paragraph says so; this one had not. `sha256` is in the same
# position and was missing from that sentence when it was written.
REQUIRED_EXTS='rs sh md toml py sha256 json'

# AND THIS LIST HAS A FLOOR, which took until the ninth review pass because the
# audit kept being aimed at the list that had just failed. THREE lists were
# pinned and TWO had counts; nobody had counted the counts. Measured, not
# argued: dropping `py` from this list AND `*.py` from the filter -- one more
# word in the same diff as the half-mutation the paragraph above boasts about
# catching -- took the probes from 15 to 13 and left a live `{EM}` sitting in
# `tests/fixtures/_magi_ref.py`, a PACKAGED file, with both modes exiting 0.
# Same signature as all four root escapes, through the rung nobody had floored.
#
# LIMIT, in the same terms as the other two: this catches an extension DELETED,
# not one SWAPPED for another. The regress stops for the reason the roots block
# gives, and no stronger.
REQUIRED_EXT_COUNT=7

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

# THE FOURTH PINNED LIST, and it had no floor either -- found by asking the
# generalised question the ninth pass raised rather than by another report. The
# relation below (every pattern exercised by a shape) survives a DELETION
# intact: fewer patterns, all still exercised. Measured: removing the `chr()`
# pattern with its two shapes and lowering the shape floor to match left both
# modes at 0 with a live `chr(8212)` in a shipped file.
#
# LIMIT: catches a pattern DELETED, not one narrowed in place -- which is what
# the alternation paragraph below is about, and why the shapes vary what their
# character classes range over.
REQUIRED_PATTERN_COUNT=4

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

# THE SCAN SET, and what it is NOT. As of this commit `cargo package --list` emits 59
# files; this scans four directories and three root files, which is where prose
# that a human wrote lives. The arithmetic that used to live here -- 175 packaged,
# 41 unscanned, of which `tests/` and `.github/` were 34 -- is DEAD: R-32 excluded
# `.github/`, `ci/` and the SUITE half of `tests/` -- `tests/*.rs`,
# `tests/common/` and `tests/support/`. `tests/fixtures/` is deliberately NOT
# excluded and still ships, which is the whole reason the paragraphs below care
# what is in it; a reader who took this sentence to mean all of `tests/` was
# gone would conclude that losing that root from the scan costs nothing. What
# remains unscanned of the 59 are the cargo-generated and legal files at the root
# (`.cargo_vcs_info.json`, `.gitattributes`, `Cargo.lock`, `Cargo.toml.orig` and
# the three licence files). EVERY tracked file under `tests/fixtures/` is now
# scanned -- all sixteen -- and getting there took four review passes, each of
# which refuted the exclusion argument the previous one had accepted.
#
# The sequence is the finding, not the individual files. Pass three: `ec/README.md`
# was outside the scan while this comment said the only unscanned packaged files
# were cargo-generated. Pass four: the three `.py` files, excused because `ci/`
# does not ship -- true of `ci/`, false here. Pass six: `magi_ref_prompts.sha256`,
# excused as "checksum data carrying no prose by construction" while it opens
# with seven lines of generated English. Pass seven: `magi_report_v0_3_1.json`,
# excused as a backend response captured from a live provider -- it is neither
# captured nor under `ec/`, it is hand-composed English, and it is
# `include_str!`'d at `reporting.rs`.
#
# FOUR ARGUMENTS FOR EXCLUDING A CATEGORY, EACH FALSE FOR AT LEAST ONE MEMBER.
# At that point the argument itself is the defect, so the `.json` are scanned
# too and the reasoning stops. The ten captured responses under `ec/` cost
# nothing to read and carry no pattern today; if a captured payload ever trips
# one it will be a LOUD false red with a file and a line, which is diagnosable,
# and the answer then is an anchored exemption for that file -- not a category
# argument, which is the shape that failed four times. Note what that remedy
# costs, said here rather than left in the paragraph further down that says it:
# a one-file exclusion is the ONE narrowing this self-test provably cannot see,
# so recommending it means recommending an unpinned move. It is still the right
# answer -- it names the file in the source, where a reviewer reads it, instead
# of widening a category argument -- but it is not a guarded one.
#
# What that leaves for the `.py` files is a note rather than a justification,
# since they no longer need one: only `gen` assembles prose at all -- f-strings
# and `join`, never the `' + X + '` shape this guard hunts -- while `extract`
# copies bytes out of `git show` and `_magi_ref` holds the constants and the
# blob reader. The reason first written for them, that they build prose by
# concatenation, was false for all three.
#
# That site and the `.sha256`'s at `prompts/mod.rs` both sit inside
# `#[cfg(test)]`, so a
# consumer building this as a dependency never compiles them; a distribution
# packager building the tarball's tests does, which is the case `Cargo.toml`
# spends ten lines on and the reason the files are in the package at all. A
# generator writing prose into a shipped file that gets compiled is this guard's
# founding scenario, and for two of those four passes the coverage map said no
# such file existed. The enumeration is given because naming
# only the directories reads as a complete enumeration, and
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
# It runs WIDER than the tarball in TWO places, and the second is not slight:
# `docs/test/` is scanned and excluded from the package, and since 4.1.0 the `ci/`
# files are scanned and excluded too -- 101 of the 102 tracked there, the missing
# one being this file, which exempts itself. It was 98 for one
# round, while the filter matched `.rs`, `.sh`, `.md` and `.toml` and this round's
# three guards were `.py`; the gap was named and left open, on the argument that
# `ci/` no longer ships so the cost was bounded. The fourth review pass showed the
# argument does not transfer: `tests/fixtures` holds three `.py` files that DO
# ship. `.py` joined the filter and both are covered. So most of what this reads does
# not ship. Left that way on purpose -- being noisy about a file that does not
# ship is the harmless direction, and carving an exception into the scan is how
# the interesting direction gets carved next. *(This paragraph said "slightly
# WIDER in one place" until R-32 excluded `ci/`; the number moved and the word did
# not.)*
#
# THIS FILE IS THE ONE EXEMPTION, and an exemption is the dangerous part of any
# guard, so it is anchored to the exact path rather than matched as a substring
# (an unanchored filter exempted anything whose path merely CONTAINED this name).
# It has to spell out the shapes it looks for in order to document them, so
# scanning itself would fail always. The cost is stated rather than hidden: this
# file's own prose is unguarded, which is acceptable only because it is the file
# whose subject IS those shapes.

# Resolve a root to what the FILESYSTEM calls it, so no re-spelling counts twice.
# `pwd -P` does the work: it resolves `.`, `..`, repeated slashes and symlinks in
# one step, which is why this replaced a growing list of `sed` strips. A path that
# does not exist falls back to its raw string -- the safe direction, since it then
# counts as distinct and the coverage check names it a few lines later.
# CDPATH is cleared at every `cd` that takes a RELATIVE path -- here and at the
# `ROOT=` line near the top; the two `cd`s that take absolute paths are immune,
# since CDPATH is not consulted for one. The claim used to say "every `cd`"
# without qualification and was one grep from being checkable, in the file whose
# whole subject is prose that contradicts adjacent code. It is not hygiene: when
# `cd` resolves
# through a CDPATH component it PRINTS the directory it landed in, so the
# subshell emits two lines instead of one and the count moves. Measured on this
# machine with a CDPATH holding a sibling `docs`, `n_roots` went from 8 to 9 --
# and worse, the root resolved OUTSIDE the repository. Inherited environment is
# not an input a guard gets to assume away.
canonical_root() {
  if [ -d "$1" ]; then
    ( CDPATH= cd "$1" 2>/dev/null && pwd -P ) || printf '%s\n' "$1"
  elif [ -f "$1" ]; then
    _cr_d="$(dirname "$1")"
    _cr_b="$(basename "$1")"
    ( CDPATH= cd "$_cr_d" 2>/dev/null && printf '%s/%s\n' "$(pwd -P)" "$_cr_b" ) ||
      printf '%s\n' "$1"
  else
    printf '%s\n' "$1"
  fi
}

scan_targets() {
  find $SCAN_DIRS -type f \
    \( -name '*.rs' -o -name '*.sh' -o -name '*.md' -o -name '*.toml' \
       -o -name '*.py' -o -name '*.sha256' -o -name '*.json' \) 2>/dev/null |
    sed 's#^\./##' |
    grep -v '^ci/check_prose_artifacts\.sh$'
  ls $SCAN_FILES 2>/dev/null
}

# A scanned path containing whitespace word-splits at the `xargs` below, so
# `grep` receives two paths that do not exist, its complaint goes to /dev/null,
# and the file is skipped in silence. Verified with `docs/my note.md` carrying a
# live placeholder: the plain mode exited 0. No path in the SCANNED set has
# whitespace -- which is the set that matters here, and it is not the packaged set:
# `ci/` is scanned and not packaged. So this REFUSES the condition rather than
# paying a `grep` per file
# (~500 invocations per pattern, forty times over, in a check that already takes
# a minute) to support a filename this repository does not use.
# An ABSENT root is a failure, not an empty scan. Run where `src/`, `ci/`, `docs/`
# and `examples/` do not exist, the `find` above sends its complaint to /dev/null,
# `scan_targets` yields nothing, and the plain mode printed OK having read zero
# bytes -- success reported by a check that never looked at anything. The
# self-test refuses that condition correctly, but the two modes are separate
# invocations and only one had the precondition.
#
# The distinction that matters is ABSENT versus EMPTY: a root that exists and
# holds no matching file is a legitimate state (a repository may genuinely have
# no `examples/` content yet), while a root that is not there means the check is
# being run from the wrong place. Only the second is refused, and it is refused
# loudly rather than by silence.
assert_roots_exist() {
  _missing=""
  for _root in $SCAN_DIRS; do
    [ -d "$_root" ] || _missing="$_missing $_root"
  done
  for _file in $SCAN_FILES; do
    [ -r "$_file" ] || _missing="$_missing $_file"
  done
  if [ -n "$_missing" ]; then
    echo "check_prose_artifacts: FAIL -- scan roots missing:$_missing" >&2
    echo "check_prose_artifacts: run this from the repository root." >&2
    exit 1
  fi
}

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
  # `printf`, not `echo`: POSIX leaves echo's handling of backslashes
  # unspecified, and this gate runs under `sh`, which is dash on the CI runner
  # and bash locally. Today's patterns survive both -- measured, `\{`, `\+` and
  # `\(` come through dash identically -- but a pattern carrying `\t` or `\n`
  # would become a literal tab under one shell and stay an escape under the
  # other, and the difference would show up as a rule that quietly matches
  # nothing on CI while passing here.
  printf '%s\n' "$PATTERNS" | while IFS= read -r pat; do
    [ -n "$pat" ] || continue
    ( cd "$_dir" && scan_targets | xargs -r grep -nE "$pat" 2>/dev/null ) || true
  done
}

# Both preconditions, in the order they matter: there is something to scan at all,
# and every path it will produce survives the word-splitting below.
assert_roots_exist
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
  # `_bare` is in the trap although it is created far below: the command is
  # evaluated when the trap fires, so the `${_bare:+...}` form stays empty until
  # then. It is still removed by hand further down, and that stays -- the trap is
  # the backstop for the window between its creation and that line, where an exit
  # under `set -e` used to leak it. The second `rm -rf` on a path already gone is
  # a no-op and does not trip `set -e`.
  trap 'rm -rf "$TMP" ${_bare:+"$_bare"}' EXIT
  # NESTED ROOTS KEEP THEIR PARENT PATH, and this is not defensive coding -- it
  # is a measured defect. `cp -r tests/fixtures "$TMP/"` lands the tree at
  # `$TMP/fixtures`, so the probe for `tests/fixtures/ec/README.md` looked in a
  # directory that does not exist and the self-test went red the moment that root
  # was pinned. A working-tree draft had put it in SCAN_DIRS alone and nothing
  # objected, because an unpinned root is never probed -- which is precisely the
  # argument for pinning it. (That draft was never committed. An earlier version
  # of this sentence called it a round that happened, and the correction landed
  # at the REQUIRED_ROOTS block and not here, so for one commit this file
  # asserted two opposite histories eleven lines apart.)
  #
  # AND THE COPY FAILS LOUDLY. `2>/dev/null || true` meant a root that could not
  # be copied produced a sandbox missing it, silently: the self-test would then
  # scan less than it claims and still be able to pass. Same class as an empty
  # scan set reporting OK -- a setup step that swallows its own failure is a gate
  # that reports on work it did not do.
  for _d in $SCAN_DIRS; do
    mkdir -p "$TMP/$(dirname "$_d")"
    cp -r "$_d" "$TMP/$(dirname "$_d")/" || {
      echo "SELF-TEST: could not stage scan root into the sandbox: $_d" >&2
      exit 1
    }
  done
  for _f in $SCAN_FILES; do
    cp "$_f" "$TMP/" || {
      echo "SELF-TEST: could not stage scan file into the sandbox: $_f" >&2
      exit 1
    }
  done

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

  # The floor, before anything is discovered: shrinking REQUIRED_ROOTS is
  # refused rather than absorbed. Without it the loop below happily iterates a
  # shorter list and reports success on the coverage that is left.
  _canon_roots="$(for _r in $REQUIRED_ROOTS; do canonical_root "$_r"; done |
    LC_ALL=C sort -u)"
  n_roots="$(printf '%s\n' "$_canon_roots" | grep -c . || true)"
  if [ "$n_roots" -lt "$REQUIRED_ROOT_COUNT" ]; then
    echo "SELF-TEST: $n_roots required roots, $REQUIRED_ROOT_COUNT expected" >&2
    fails=$((fails + 1))
  fi

  # The count alone was not enough: a root NESTED under another is distinct, so
  # it holds the number while the root it displaced leaves the scan -- and it is
  # the cheapest escape available, since it needs no new directory. Refused as a
  # relation over the canonical paths, which asserts nothing about which roots
  # are the right ones and so is not a second copy of REQUIRED_ROOTS.
  #
  # The word-split below is safe only while no canonical path carries
  # whitespace, so that is checked rather than assumed -- fail closed, since a
  # split path would silently compare the wrong strings.
  #
  # Checked as a RELATION rather than by listing the characters: words and lines
  # must agree. Enumerating them caught space and tab and missed NEWLINE, which
  # is the one that inflates the count and so helps hold the floor -- and the
  # obvious repair is a trap, since `*"$(printf '\n')"*` collapses to `**` and
  # false-reds on everything. Counting catches all three at once.
  _n_words="$(printf '%s\n' $_canon_roots | grep -c . || true)"
  if [ "$_n_words" -ne "$n_roots" ]; then
    echo "SELF-TEST: a canonical required root contains whitespace" >&2
    fails=$((fails + 1))
  fi
  for _a in $_canon_roots; do
    for _b in $_canon_roots; do
      [ "$_a" = "$_b" ] && continue
      case "$_a" in
        "$_b"/*)
          echo "SELF-TEST: required root $_a is nested under $_b" >&2
          fails=$((fails + 1))
          ;;
      esac
    done
  done

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

  # The other two pinned lists, floored here for the same reason and in the same
  # shape. Four lists, four floors -- the audit is over the SET now, because
  # aiming it at whichever list last failed is how two of them went four review
  # passes without one.
  n_exts="$(printf '%s\n' $REQUIRED_EXTS | grep -c . || true)"
  if [ "$n_exts" -lt "$REQUIRED_EXT_COUNT" ]; then
    echo "SELF-TEST: $n_exts required extensions, $REQUIRED_EXT_COUNT expected" >&2
    fails=$((fails + 1))
  fi

  n_patterns="$(printf '%s\n' "$PATTERNS" | grep -c . || true)"
  if [ "$n_patterns" -lt "$REQUIRED_PATTERN_COUNT" ]; then
    echo "SELF-TEST: $n_patterns patterns, $REQUIRED_PATTERN_COUNT expected" >&2
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

  # THE EMPTY-SCAN-SET CASE, which nothing pinned. `assert_roots_exist` runs from the
  # repository root on every other invocation, where it always passes, so deleting the
  # call in a refactor would reopen the defect with the whole gate green. This runs the
  # PLAIN mode from a bare directory and requires a refusal -- the same rule the roots
  # check itself enforces: a guard that did not find what to look at is not a guard that
  # passed.
  # Copied into `$_bare/ci/`, not into `$_bare` itself: the probe resolves its own
  # ROOT as the PARENT of its directory, so a copy at the top level pointed ROOT at
  # the shared temp root and the case then depended on unrelated temp content.
  _bare="$(mktemp -d)"
  mkdir -p "$_bare/ci"
  cp "$ROOT/ci/check_prose_artifacts.sh" "$_bare/ci/probe.sh"
  _out="$( cd "$_bare/ci" && sh probe.sh 2>&1 )" && _rc=0 || _rc=1
  # WHICH refusal, not merely that it refused: any error would satisfy a bare
  # non-zero, including one that has nothing to do with an empty scan set.
  if [ "$_rc" = 0 ] || ! printf %s "$_out" | grep -q "scan roots missing"; then
    echo "check_prose_artifacts: SELF-TEST FAILED -- the plain mode did not refuse an" >&2
    echo "empty scan set with the roots-missing message. Got rc=$_rc, output:" >&2
    printf '%s\n' "$_out" >&2
    rm -rf "$_bare"
    exit 1
  fi
  rm -rf "$_bare"
  echo "  [ok] empty scan set is refused"

  echo "check_prose_artifacts: self-test OK ($n_shapes shapes x $n_targets probe files caught; clean copy silent; empty scan set refused)"
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
