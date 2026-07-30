#!/usr/bin/env sh
# Author: Julian Bolivar
# Version: 1.0.0
# Date: 2026-07-29
#
# Purpose: the crate deliberately removed a heuristic that used to GUESS which JSON
# object in a model's response was the verdict. That guessing produced fabricated
# verdicts. The rule is now "extract between markers, never search", and this script is
# what stops the deleted code from being quietly revived by someone with good intentions
# a year from now. It is a ratchet against a KNOWN regression, not a proof of
# correctness: a brand-new heuristic under a new name would pass both checks, and the
# only control against that is a human reviewer who knows the rule.
#
# Run from the repository root. Exits non-zero on any violation.
set -eu

EXPECTED_LOCATOR_DEFINITIONS=1

violation=0

# Check 1 — banned symbols.
#
# (a) The scope is `-- src`, the DIRECTORY, which recurses. Deliberately not a glob like
#     'src/*': whether such a glob crosses a `/` depends on git's pathspec behaviour, and
#     `src/providers/` exists — an ambiguous pathspec could silently skip a whole
#     subtree, and a gate that quietly checks less than it claims is worse than none.
#
# (b) Comments are NOT excluded, and that is deliberate: the list holds only distinctive
#     identifiers, never common words, so there are no false positives by construction.
#     The consequence is accepted — a comment may not NAME a deleted symbol, it must
#     describe it. (`lenient` alone is NOT on the list for exactly this reason: it occurs
#     in legitimate prose about permissive fence handling, and a gate that breaks the
#     build for discussing the topic is a badly built gate.)
if git grep -nE 'embedded_verdict_object|VERDICT_KEYS|LENIENT_RECOVERY_MAX_BYTES|MAX_BRACE_PROBES|with_lenient_parsing|fallback_parse' -- src; then
    echo "check_r0: banned heuristic symbols found under src (listed above)"
    violation=1
fi

# Check 2 — exactly ONE definition of the marker-delimited block locator. Two would be
# the original defect back: one of them would drift and see less than the other, and the
# guard that simulates the parser would stop being a faithful simulation.
#
# Counts DEFINITIONS (the `fn ` prefix), not call sites. `git grep -c` exits non-zero
# when there are no matches, so it is guarded with `|| true`: under `set -e` an
# unguarded command substitution would abort the script instead of reporting `found 0`,
# turning the most important failure into an unreadable one.
matches=$(git grep -c 'fn locate_block' -- src || true)
n=$(printf '%s\n' "$matches" | awk -F: '{s+=$2} END {print s+0}')
if [ "$n" -ne "$EXPECTED_LOCATOR_DEFINITIONS" ]; then
    echo "check_r0: expected exactly ${EXPECTED_LOCATOR_DEFINITIONS} definition of \
'fn locate_block' under src, found ${n}"
    violation=1
fi

if [ "$violation" -ne 0 ]; then
    exit 1
fi

echo "check_r0: clean"
