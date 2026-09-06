#!/usr/bin/env sh
# Author: Julian Bolivar
# Version: 1.0.0
# Date: 2026-09-05
#
# The README's install requirement must agree with Cargo.toml.
#
# It shipped WRONG at 2.0.0 and at 3.0.0: `git show v2.0.0:README.md` still said
# "1.0" while the crate was 2.0.0, so a consumer copying the Quick Start got a
# crate without the APIs the same README documented. 4.0.0 was the first major to
# get it right, and it got it right by review rather than by machine.
#
# It travels inside the immutable package and is the crates.io landing page, so
# the fix after publishing is publishing again -- the shape 3.0.2 cost a whole
# release.
#
# RELEASE PATH, never the round gate. A mismatch is only wrong at the moment of
# publishing; in the per-round gate this would fire on every branch that has not
# bumped yet, and a guard that is red by default is a guard that gets ignored.
#
# MAJOR AND MINOR, with the patch free. `magi-core = "4.1"` is a caret
# requirement: correct for 4.1.0 AND for 4.1.3. Demanding exact equality would
# break on every patch. Comparing only the major would miss the likelier defect --
# the forgotten bump inside one major, publishing 4.2.0 with a README saying 4.1.
#
# ZERO MATCHES IS A FAILURE. A README reflow would disarm this in silence, and
# READMEs get reflowed. A guard that did not find what to look at is not a guard
# that passed -- the same rule R-29 enforces for an empty scan set.
#
# N MATCHES: all of them are compared and all must agree, naming the ones that do
# not. Not "exactly one" -- that would break the day someone adds a second
# legitimate example, and adding examples is good. What is inadmissible is picking
# one by position and letting the others drift.
set -eu

README="${1:-README.md}"
MANIFEST="${2:-Cargo.toml}"

# TWO SHAPES, and the second is not in the contract's literal pattern -- it is
# added here because both are install requirements a reader copies, and they drift
# together:
#   magi-core = "4.0"
#   magi-core = { version = "4.0", features = [...] }
# Commented-out lines count: a `#`-prefixed example is still what someone copies.
readme_requirements() {
    # Two shapes are EXTRACTED: the bare requirement, and an inline table with
    # `version` anywhere among its keys. The old pattern required `version` to be
    # the FIRST key, so `magi-core = { features = [...], version = "3.9" }` was
    # not extracted -- and since the check only compares what it extracts, a stale
    # requirement in that ordinary spelling passed as long as one other agreed.
    # Measured, twice, before this was changed.
    grep -oE 'magi-core[[:space:]]*=[[:space:]]*(\{[^}]*version[[:space:]]*=[[:space:]]*)?"[0-9]+\.[0-9]+' "$1" |
        grep -oE '[0-9]+\.[0-9]+$'
}

# ...and everything else is REFUSED rather than skipped. Widening the pattern
# closes the spelling that was measured; it cannot close the one nobody thought
# of, and this project's history with enumerated patterns is that the next
# unlisted case is the one that gets through. So the mentions are counted
# independently of the extractions and a shortfall FAILS: a requirement this
# script cannot parse is one it must not silently pass over. `[dependencies.
# magi-core]` with its `version` on a later line counts as a mention here and
# yields no extraction, which is exactly the intended refusal.
# COUNTED PER MATCH, not per line, because the two sides are compared to each
# other and `grep -c` counts LINES. Two parseable requirements on one line then
# inflated the extraction count past the mention count and masked an unparseable
# one elsewhere -- constructible, and over the exact shape the self-test pins as
# refused. A comparison between two different units is not a comparison.
readme_mentions() {
    grep -oE 'magi-core[[:space:]]*=|\[[^]]*dependencies\.magi-core\]' "$1" |
        grep -c . || true
}

manifest_version() {
    grep -m1 -E '^version[[:space:]]*=' "$1" | grep -oE '[0-9]+\.[0-9]+\.[0-9]+'
}

check() {
    _readme="$1"
    _manifest="$2"

    _full="$(manifest_version "$_manifest" || true)"
    if [ -z "$_full" ]; then
        echo "check_readme_version: FAIL -- no version found in $_manifest" >&2
        return 1
    fi
    # major.minor of the manifest, patch discarded.
    _want="${_full%.*}"

    _found="$(readme_requirements "$_readme" || true)"
    if [ -z "$_found" ]; then
        echo "check_readme_version: FAIL -- no 'magi-core = \"X.Y\"' requirement found in $_readme" >&2
        echo "check_readme_version: zero matches is a failure, not a pass: the pattern found nothing to compare." >&2
        return 1
    fi

    _mentions="$(readme_mentions "$_readme")"
    _n_found="$(printf '%s\n' "$_found" | grep -c . || true)"
    if [ "$_n_found" -lt "$_mentions" ]; then
        echo "check_readme_version: FAIL -- $_mentions magi-core requirement(s) in $_readme, only $_n_found parsed" >&2
        echo "check_readme_version: a requirement this script cannot read is not one it may pass over." >&2
        return 1
    fi

    _bad=""
    _n=0
    for _got in $_found; do
        _n=$((_n + 1))
        [ "$_got" = "$_want" ] || _bad="$_bad $_got"
    done
    if [ -n "$_bad" ]; then
        echo "check_readme_version: FAIL -- $_manifest is $_full (major.minor $_want)" >&2
        echo "check_readme_version: these README requirements disagree:$_bad" >&2
        return 1
    fi
    echo "check_readme_version: OK ($_n requirement(s) in $_readme agree with $_want)"
    return 0
}

# ---------------------------------------------------------------------------
# Self-test. Four directions, and the last two are the ones that pin the policy:
# without them the check passes the first two and nobody knows what it does with
# the others.
# ---------------------------------------------------------------------------
self_test() {
    _tmp="$(mktemp -d)"
    _fail=0

    printf 'version = "4.1.0"\n' > "$_tmp/Cargo.toml"

    _case() {
        _name="$1"
        _want_rc="$2"
        if check "$_tmp/README.md" "$_tmp/Cargo.toml" >/dev/null 2>&1; then
            _rc=0
        else
            _rc=1
        fi
        if [ "$_rc" = "$_want_rc" ]; then
            echo "  [ok]   $_name"
        else
            echo "  [FAIL] $_name (got rc=$_rc, wanted $_want_rc)"
            _fail=1
        fi
    }

    printf 'magi-core = "4.1"\n' > "$_tmp/README.md"
    _case "agreeing requirement                 " 0

    printf 'magi-core = "4.2"\n' > "$_tmp/README.md"
    _case "README AHEAD of the manifest         " 1

    printf 'magi-core = "4.0"\n' > "$_tmp/README.md"
    _case "README BEHIND the manifest           " 1

    printf 'magi_core = "4.1"\n' > "$_tmp/README.md"
    _case "zero matches is a FAILURE            " 1

    printf 'magi-core = "4.1"\nmagi-core = { version = "4.0" }\n' > "$_tmp/README.md"
    _case "two matches, one disagreeing         " 1

    printf 'magi-core = "4.1"\n# magi-core = { version = "4.1", features = ["ollama"] }\n' > "$_tmp/README.md"
    _case "two matches, both agreeing           " 0

    printf 'magi-core = "4.1"\nmagi-core = { features = ["ollama"], version = "3.9" }\n' > "$_tmp/README.md"
    _case "inline table, version NOT first     " 1

    printf 'magi-core = "4.1"\n[dependencies.magi-core]\nversion = "3.9"\n' > "$_tmp/README.md"
    _case "a shape it cannot parse is REFUSED  " 1

    # The same shape with TWO parseable requirements sharing a line. Counting
    # mentions by line made 2 extractions cover 2 mention-lines and the third
    # requirement went uncompared -- exit 0 over a stale 3.9.
    printf 'see `magi-core = "4.1"` or `magi-core = "4.1"` inline.\n[dependencies.magi-core]\nversion = "3.9"\n' > "$_tmp/README.md"
    _case "two on one line cannot mask a third " 1

    printf 'magi-core = "4.1"\n' > "$_tmp/README.md"
    printf 'name = "x"\n' > "$_tmp/Cargo.toml"
    _case "no version in the manifest is a FAIL " 1

    rm -rf "$_tmp"
    if [ "$_fail" = 0 ]; then
        echo "check_readme_version: self-test OK -- 10 cases"
        return 0
    fi
    echo "check_readme_version: SELF-TEST FAILED" >&2
    return 1
}

if [ "${1:-}" = "--self-test" ]; then
    self_test
else
    check "$README" "$MANIFEST"
fi
