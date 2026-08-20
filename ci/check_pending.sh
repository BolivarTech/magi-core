#!/usr/bin/env sh
# Author: Julian Bolivar
# Version: 1.0.0
# Date: 2026-08-20
#
# RELEASE gate: published documentation must carry no unfinished sections.
#
# A milestone may ship a migration-guide section whose numbers another milestone derives, and mark
# the hole so the next reader knows it is a hole rather than an omission. That is honest. What is
# NOT honest is a marker that CLAIMS to block the release while nothing checks it — the shape of
# guard this project keeps finding: one that reports success while guarding nothing. The marker in
# `docs/migration-v4.0.md` said "the release cannot be tagged while it is here" and no grep
# existed anywhere in the repository. This is that grep.
#
# It is deliberately NOT part of `ci/run_all_checks.sh`. That script is the PER-ROUND gate, run by
# every milestone; a marker left for a later milestone would make every round of the earlier one
# red for something it is not responsible for. The question this asks — "is the documentation
# finished?" — only has to be answered once, at the tag.

set -eu

echo "=== pending release markers ==="

if grep -rn "PENDING: MS" docs/ 2>/dev/null; then
    echo ""
    echo "check_pending: FAIL — published documentation still carries an unfinished section."
    echo "check_pending: crates.io is immutable and docs.rs builds per version, so a hole that"
    echo "check_pending: ships can only be closed by publishing again. Write the section first."
    exit 1
fi

echo "check_pending: OK (no unfinished sections in published docs)"
