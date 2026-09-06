#!/usr/bin/env sh
# Author: Julian Bolivar
# Version: 4.1.0
# Date: 2026-09-05
#
# RELEASE gate: published documentation must carry no unfinished sections.
#
# A milestone may ship a migration-guide section whose numbers another milestone derives, and mark
# the hole so the next reader knows it is a hole rather than an omission. That is honest. What is
# NOT honest is a marker that CLAIMS to block the release while nothing checks it -- the shape of
# guard this project keeps finding: one that reports success while guarding nothing. The marker in
# `docs/migration-v4.0.md` said "the release cannot be tagged while it is here" and no grep
# existed anywhere in the repository. This is that grep.
#
# It is deliberately NOT part of `ci/run_all_checks.sh`. That script is the PER-ROUND gate, run by
# every milestone; a marker left for a later milestone would make every round of the earlier one
# red for something it is not responsible for. The question this asks -- "is the documentation
# finished?" -- only has to be answered once, at the tag.
#
# THREE WAYS THIS SCRIPT COULD HAVE LIED, all found by review and all closed below. They are
# written out because the first version had all three and read as healthy:
#
#   1. It certified a directory it never opened. `grep -rn ... docs/ 2>/dev/null` on a missing or
#      renamed path prints nothing, the `if` is false, and the script announces that published
#      documentation is finished -- having looked at zero bytes. Every target is now asserted to
#      exist FIRST, and a missing one is a failure, not a pass.
#   2. It read grep's ERROR as good news. grep exits 0 when it matches, 1 when it does not and
#      2 when something went wrong; the `if` treated 1 and 2 alike, so an unreadable file was
#      indistinguishable from a clean one. Exit 2 is now its own failure.
#   3. It claimed more than it checked. The message said "published documentation" while the scan
#      covered only `docs/` -- but README and CHANGELOG ship to crates.io as well, and rustdoc
#      ships to docs.rs. The scan now covers what the sentence claims.
#
# Self-tested with `--self-test`, for the same reason its sibling `check_redaction.sh` is: the
# mechanism that reports success is the one that has to be attacked.

set -eu

MARKER='PENDING: MS'

# Everything that reaches a consumer: crates.io ships the package, docs.rs builds the rustdoc.
TARGETS='docs README.md CHANGELOG.md src'

# Resolve this script's own absolute path, no matter how it was invoked: given as an absolute
# path, given as a path relative to some directory, or found by searching $PATH. Needed because
# the self-test below re-invokes this same script from inside a throwaway directory, where a
# path relative to the ORIGINAL working directory is meaningless -- and the old form,
# "$OLDPWD/$0", only reconstructed the right path when $0 happened to be relative.
#
# `readlink -f` / `realpath` are not used: both are GNU extensions, absent from the BSD/macOS
# `readlink` this project does not target today but must not assume away -- and the operation
# needed here is "resolve against the right base directory", not "follow a symlink chain".
resolve_self() {
    case "$1" in
        /*)
            # Already absolute.
            printf '%s\n' "$1"
            ;;
        */*)
            # Relative, with a directory component: resolve against the directory this
            # process was started in, before anything below has a chance to cd away from it.
            resolve_dir="$(CDPATH= cd "$(dirname "$1")" 2>/dev/null && pwd)" || return 1
            printf '%s/%s\n' "$resolve_dir" "$(basename "$1")"
            ;;
        *)
            # No directory component at all. This has TWO sources, and assuming only the
            # second is a defect found by running it: `sh check_pending.sh` from inside
            # ci/ sets $0 to a bare name that is relative to the CURRENT directory, not
            # something on $PATH. Trying $PATH first made that invocation fail outright.
            # The current directory wins because `sh name` never searches $PATH for its
            # script argument, so that reading is the deterministic one; $PATH is the
            # fallback, for a copy installed as a command.
            if [ -r "./$1" ]; then
                printf '%s/%s
' "$(pwd)" "$1"
            else
                command -v "$1"
            fi
            ;;
    esac
}

# These two functions expose the defect above: they spawn six real child processes, one per way
# this script can be invoked, and check that each one's `--self-test` still succeeds.
#
# CHECK_PENDING_SELFTEST_NESTED marks a child so it skips this same sub-test; without it, every
# child would spawn five more, forever.
run_crossing() {
    label="$1"; dir="$2"; shift 2
    if out="$(cd "$dir" && "$@" 2>&1)"; then
        crossing_rc=0
    else
        crossing_rc=$?
    fi
    if [ "$crossing_rc" -eq 0 ] && printf '%s' "$out" | grep -q 'self-test OK'; then
        echo "crossings: PASS -- $label"
    else
        echo "crossings: FAIL -- $label (exit $crossing_rc)" >&2
        printf '%s\n' "$out" >&2
        frc=1
    fi
}

six_crossings_test() {
    frc=0
    ci_dir="$(dirname "$SELF")"
    root_dir="$(cd "$ci_dir/.." && pwd)"
    name="$(basename "$SELF")"

    # NOTHING IS WRITTEN INTO THE REPOSITORY. An earlier version created
    # `$root_dir/.check_pending_selftest_other`, and the round gate runs this every
    # round: the happy path cleaned up, but the EXIT trap was installed only after two
    # `mktemp -d`, a `cp` and a `chmod`, all under `set -eu` -- so a failure in that
    # window, or a Ctrl-C, left an untracked directory in the tree. That reddens the
    # clean-status check at finalisation, shows up in plan alignment as a path no task
    # produced, and widens the scoped test selector to the full suite.
    #
    # (d) needs a directory that is neither the copy's directory nor its parent,
    # reached by a SHORT relative path. A private sandbox gives exactly that without
    # touching the tree: the copy lives at `$sandbox/ci/$name`, (d) runs from
    # `$sandbox/other`, and "../ci/$name" resolves between them as it would in the
    # repository.
    sandbox="$(mktemp -d)"
    mkdir -p "$sandbox/ci" "$sandbox/other"
    cp "$SELF" "$sandbox/ci/$name"
    chmod +x "$sandbox/ci/$name"
    other_rel="$sandbox/other"
    # (c) needs a directory with no relation to the repo at all.
    other_abs="$(mktemp -d)"
    # (e) needs a directory on $PATH holding nothing else named "$name".
    bindir="$(mktemp -d)"
    cp "$SELF" "$bindir/$name"
    chmod +x "$bindir/$name"
    trap 'rm -rf "$sandbox" "$other_abs" "$bindir"' EXIT

    run_crossing "a: absolute path, from repo root" \
        "$root_dir" env CHECK_PENDING_SELFTEST_NESTED=1 sh "$SELF" --self-test
    run_crossing "b: relative path, from repo root" \
        "$root_dir" env CHECK_PENDING_SELFTEST_NESTED=1 sh "ci/$name" --self-test
    run_crossing "c: absolute path, from another directory" \
        "$other_abs" env CHECK_PENDING_SELFTEST_NESTED=1 sh "$SELF" --self-test
    run_crossing "d: relative path, from another directory" \
        "$other_rel" env CHECK_PENDING_SELFTEST_NESTED=1 sh "../ci/$name" --self-test
    # (e) MEASURED CAVEAT, written so this case cannot claim coverage it does not have:
    # when a script is found through $PATH the shell hands it an ALREADY-RESOLVED absolute
    # $0, so this crossing exercises the absolute branch, not the $PATH lookup. It is kept
    # because it pins the invocation form a user would actually type.
    run_crossing "e: invoked by name through PATH" \
        "$root_dir" env CHECK_PENDING_SELFTEST_NESTED=1 "PATH=$bindir:$PATH" "$name" --self-test
    # (f) is the crossing that actually reaches the no-slash branch, and it FAILED before
    # the fix in resolve_self: $0 arrives as a bare name relative to the current directory.
    run_crossing "f: bare name, from the script directory" \
        "$ci_dir" env CHECK_PENDING_SELFTEST_NESTED=1 sh "$name" --self-test

    rm -rf "$sandbox" "$other_abs" "$bindir"
    trap - EXIT
    return "$frc"
}

self_test() {
    rc=0

    SELF="$(resolve_self "$0")" || {
        echo "check_pending: FAIL -- could not resolve this script's own path from \$0='$0'" >&2
        exit 1
    }
    [ -r "$SELF" ] || {
        echo "check_pending: FAIL -- resolved self path '$SELF' is not a readable file" >&2
        exit 1
    }

    if [ "${CHECK_PENDING_SELFTEST_NESTED:-}" != "1" ]; then
        six_crossings_test || rc=1
    fi

    tmp="$(mktemp -d)"
    mkdir -p "$tmp/docs" "$tmp/src"
    printf 'clean
' > "$tmp/docs/guide.md"
    printf 'clean
' > "$tmp/README.md"
    printf 'clean
' > "$tmp/CHANGELOG.md"
    printf 'clean
' > "$tmp/src/lib.rs"

    # A clean tree must PASS -- otherwise the rule is a wall, not a guard.
    if ! (cd "$tmp" && sh "$SELF" >/dev/null 2>&1); then
        echo "self-test: a clean tree was rejected" >&2; rc=1
    fi

    # THE LITERAL IS ASSERTED, because every case below used to write its fixture from
    # $MARKER -- so marker and expectation moved together and no change to the constant
    # could redden this. Measured: replacing it with a string that appears nowhere left
    # the self-test saying OK and the guard reporting the published docs finished, with a
    # live `PENDING: MS` in one of them. This is the one guard between a pending marker
    # and an immutable crates.io, and its only evidence of life is this self-test, since
    # docs/ carries no marker in the ordinary case.
    #
    # Same correction 3.1.0 made to a test that asserted `starts_with(MAGE_LOCAL_PREFIX)`
    # and passed with the constant emptied: a check written from the thing it checks
    # agrees with whatever that thing says.
    # And the change is SCHEDULED, not hypothetical: MS1 recorded that a
    # `TODO(MS3)` marker in tracked source interacts with this guard, and that the
    # convention should be decided before the marker is written. The two strings
    # differ today so nothing is blocked -- but unifying them edits this constant,
    # and until now that edit would have passed in silence. It costs two places
    # now, which is the deliberation such a change deserves.
    if [ "$MARKER" != 'PENDING: MS' ]; then
        echo "self-test: MARKER is '$MARKER', expected the literal 'PENDING: MS'" >&2
        rc=1
    fi

    # Each published surface must be REACHED. A scan that skips one certifies what it never read,
    # which is defect 3 above -- and the only way to prove it is closed is one file at a time.
    # The fixture is a hard-coded literal, NOT "$MARKER", for the reason just above.
    for f in docs/guide.md README.md CHANGELOG.md src/lib.rs; do
        printf 'PENDING: MS9 here
' > "$tmp/$f"
        if (cd "$tmp" && sh "$SELF" >/dev/null 2>&1); then
            echo "self-test: a marker in $f was NOT detected" >&2; rc=1
        fi
        printf 'clean
' > "$tmp/$f"
    done

    # A MISSING target must fail rather than pass, which is defect 1.
    rm -rf "$tmp/docs"
    if (cd "$tmp" && sh "$SELF" >/dev/null 2>&1); then
        echo "self-test: a missing docs/ was treated as clean" >&2; rc=1
    fi

    rm -rf "$tmp"
    [ "$rc" -eq 0 ] && echo "check_pending: self-test OK"
    exit "$rc"
}

[ "${1:-}" = "--self-test" ] && self_test

echo "=== pending release markers ==="

for t in $TARGETS; do
    if [ ! -e "$t" ]; then
        echo "check_pending: FAIL -- '$t' does not exist, so nothing was scanned there." >&2
        echo "check_pending: a rule that certifies a path it never opened is worse than none." >&2
        exit 1
    fi
done

# `set +e` around grep because 1 (no match) is the SUCCESS case here and would abort under -e.
set +e
found="$(grep -rn "$MARKER" $TARGETS 2>&1)"
status=$?
set -e

case "$status" in
    0)
        echo "$found"
        echo ""
        echo "check_pending: FAIL -- published documentation still carries an unfinished section."
        echo "check_pending: crates.io is immutable and docs.rs builds per version, so a hole that"
        echo "check_pending: ships can only be closed by publishing again. Write the section first."
        exit 1
        ;;
    1)
        echo "check_pending: OK (no unfinished sections in published docs)"
        ;;
    *)
        echo "check_pending: FAIL -- grep exited $status, which is an ERROR and not 'no match'." >&2
        echo "$found" >&2
        exit 1
        ;;
esac
