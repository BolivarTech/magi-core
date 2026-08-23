#!/usr/bin/env sh
# Author: Julian Bolivar
# Version: 4.0.0
# Date: 2026-08-23
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

self_test() {
    rc=0
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
    if ! (cd "$tmp" && sh "$OLDPWD/$0" >/dev/null 2>&1); then
        echo "self-test: a clean tree was rejected" >&2; rc=1
    fi

    # Each published surface must be REACHED. A scan that skips one certifies what it never read,
    # which is defect 3 above -- and the only way to prove it is closed is one file at a time.
    for f in docs/guide.md README.md CHANGELOG.md src/lib.rs; do
        printf '%s here
' "$MARKER" > "$tmp/$f"
        if (cd "$tmp" && sh "$OLDPWD/$0" >/dev/null 2>&1); then
            echo "self-test: a marker in $f was NOT detected" >&2; rc=1
        fi
        printf 'clean
' > "$tmp/$f"
    done

    # A MISSING target must fail rather than pass, which is defect 1.
    rm -rf "$tmp/docs"
    if (cd "$tmp" && sh "$OLDPWD/$0" >/dev/null 2>&1); then
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
