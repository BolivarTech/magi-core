#!/bin/sh
# Author: Julian Bolivar
# Version: 1.0.0
# Date: 2026-09-13
#
# No caller of the deprecated `OpenAiCompatibleProvider` constructors may return.
#
# `OpenAiCompatibleProvider::new` and `::with_timeout` are deprecated in favour of
# `::with_dialect`, and every call site in this repository -- crate, tests, examples and the
# smoke harness -- was migrated so that the next major release can DELETE the two instead of
# migrating again. That only stays true if nothing reintroduces a caller, and the compiler
# alone cannot promise it: `#[deprecated]` is a warning, the gate turns warnings into errors,
# and a single `#[allow(deprecated)]` written beside a new test silences both. Catching that
# `allow` is this check's one job.
#
# ## The two rules
#
# A. `OpenAiCompatibleProvider::new(` and `OpenAiCompatibleProvider::with_timeout(` -- the
#    QUALIFIED spelling, which is what keeps the rule narrow: a bare `new(` would match every
#    constructor in the tree -- must not appear anywhere under `src/`, `tests/`, `examples/`
#    and `smoke/src/`. Rustdoc examples are scanned like code, because a doctest compiles.
# B. Inside `src/providers/openai_compat.rs` the SAME pair spelled `Self::` must appear
#    EXACTLY once: `new` delegates to `with_timeout` under an `#[allow(deprecated)]` whose
#    reason is written next to it. A count, not an exclusion, on purpose -- an exclusion
#    would stay green if that delegation were deleted or if a second one crept in, and the
#    count fails in both directions. Rule B is scoped to that one file: `Self::new(` in any
#    other file constructs some other type and is none of this check's business (measured:
#    widening the scope turns 32 hits into 44, none of them a compat provider).
#
# ## What it does not see, and why that is acceptable
#
# This is a grep, not a type check. A caller reached through an alias
# (`use ... as P; P::new(..)`) or through a variable is invisible to it, and the self-test
# PINS that blind spot with a fixture rather than pretending it is closed. It is acceptable
# because the compiler already covers every spelling: `#[deprecated]` fires on the aliased
# call too, and with `-D warnings` that is a build error. What the compiler cannot see is
# an `#[allow(deprecated)]` placed on the QUALIFIED spelling, and for that the qualified
# pattern is enough. Prose that quotes the call form with its parenthesis fires the rule;
# reword the prose.
#
# ## Discipline inherited from the siblings
#
# An EMPTY scan set is a FAILURE: with no `.rs` file under the roots there is nothing to
# guard, and reporting OK over zero bytes is the vacuous success this file exists to refuse.
# A missing root fails for the same reason. `--self-test` runs entirely inside `mktemp -d`,
# writes one fixture per rule -- an offender and a clean sibling -- and asserts WHICH rule
# fired by name; exit status alone would also accept a guard failing for an unrelated reason.
set -eu

ROOT="${ROOT:-.}"
ROOTS="src tests examples smoke/src"
COMPAT_FILE="src/providers/openai_compat.rs"
# Spelled with character classes, never `\b`: a word boundary escaped one layer too many once
# became a literal backspace in a sibling rule, and a rule that matches nothing reports success.
QUALIFIED='OpenAiCompatibleProvider::(new|with_timeout)[[:space:]]*\('
SELF_CALL='(^|[^A-Za-z0-9_])Self::(new|with_timeout)[[:space:]]*\('
EXPECTED_SELF_CALLS=1

fail() { echo "check_no_deprecated_ctors: $1" >&2; exit 1; }

# Resolve this script's own absolute path however it was invoked, BEFORE anything can change
# the working directory: the self-test re-invokes it against a throwaway tree, where a path
# relative to the original directory means nothing.
resolve_self() {
    case "$1" in
        /*) printf '%s\n' "$1" ;;
        */*)
            resolve_dir="$(CDPATH= cd "$(dirname "$1")" 2>/dev/null && pwd)" || return 1
            printf '%s/%s\n' "$resolve_dir" "$(basename "$1")"
            ;;
        *)
            if [ -r "./$1" ]; then
                printf '%s/%s\n' "$(pwd)" "$1"
            else
                command -v "$1"
            fi
            ;;
    esac
}
SELF="$(resolve_self "$0")" || fail "cannot resolve my own path from '$0'"

scan() {
    scan_root="$1"
    rc=0
    files=0
    for sub in $ROOTS; do
        dir="$scan_root/$sub"
        [ -d "$dir" ] || fail "$dir not found -- update this script"
        # `/dev/null` as a second operand keeps grep printing `path:line:` even when `find`
        # hands it a single file; `-n` alone would drop the path in that case.
        hits="$(find "$dir" -name '*.rs' -type f -exec grep -nE "$QUALIFIED" /dev/null {} + 2>/dev/null || true)"
        if [ -n "$hits" ]; then
            printf '%s\n' "$hits" | while IFS= read -r hit; do
                echo "check_no_deprecated_ctors: [deprecated-ctor] $hit -- migrate to OpenAiCompatibleProvider::with_dialect(base_url, model, api_key, dialect, timeout)" >&2
            done
            rc=1
        fi
        n="$(find "$dir" -name '*.rs' -type f | wc -l)"
        files=$((files + n))
    done
    if [ "$files" -eq 0 ]; then
        fail "[empty-scan] no .rs file under $ROOTS; nothing to guard is not a pass"
    fi

    compat="$scan_root/$COMPAT_FILE"
    if [ -f "$compat" ]; then
        self_calls="$(grep -cE "$SELF_CALL" "$compat" || true)"
    else
        self_calls=0
    fi
    if [ "$self_calls" -ne "$EXPECTED_SELF_CALLS" ]; then
        echo "check_no_deprecated_ctors: [self-delegation] $COMPAT_FILE: expected exactly $EXPECTED_SELF_CALLS Self::(new|with_timeout)( call -- new delegating to with_timeout under its #[allow(deprecated)] -- found $self_calls" >&2
        rc=1
    fi
    return "$rc"
}

self_test() {
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    fails=0

    # A tree with every root present and the one legitimate `Self::` delegation in place.
    # Each fixture below is this skeleton plus one variation.
    skeleton() {
        mkdir -p "$1/src/providers" "$1/tests" "$1/examples" "$1/smoke/src"
        cat > "$1/src/providers/openai_compat.rs" <<'EOF'
impl OpenAiCompatibleProvider {
    #[allow(deprecated)]
    pub fn new(base_url: String) -> Result<Self, ProviderError> {
        Self::with_timeout(base_url, DEFAULT_CLIENT_TIMEOUT)
    }
    pub fn with_timeout(base_url: String, timeout: Duration) -> Result<Self, ProviderError> {
        Self::with_dialect(base_url, Dialect::default(), timeout)
    }
}
EOF
        cat > "$1/tests/migrated.rs" <<'EOF'
let p = OpenAiCompatibleProvider::with_dialect(url, "m", None, Dialect::MaxTokens, timeout);
EOF
    }

    # Expects the run over `$1` to FAIL and its output to name rule `$2`.
    expect_reject() {
        if out="$(ROOT="$1" sh "$SELF" 2>&1)"; then
            echo "self-test: $3 was NOT rejected" >&2; fails=$((fails + 1)); return
        fi
        case "$out" in
            *"$2"*) ;;
            *) echo "self-test: $3 rejected by the WRONG rule" >&2
               echo "  expected to contain: $2" >&2
               echo "  actual: $out" >&2
               fails=$((fails + 1)) ;;
        esac
    }

    # Expects the run over `$1` to pass.
    expect_accept() {
        if ! out="$(ROOT="$1" sh "$SELF" 2>&1)"; then
            echo "self-test: $2 was rejected: $out" >&2; fails=$((fails + 1))
        fi
    }

    # 1. The clean tree: migrated callers and the one delegation. The negative half of every
    #    pair below; without it a guard that always fires would pass its self-test.
    skeleton "$tmp/clean"
    expect_accept "$tmp/clean" "clean tree"

    # 2. Rule A: a qualified call to a deprecated constructor, in a test -- the shape a new
    #    `#[allow(deprecated)]` would wear.
    skeleton "$tmp/qualified"
    cat > "$tmp/qualified/tests/regressed.rs" <<'EOF'
#[allow(deprecated)]
let p = OpenAiCompatibleProvider::new(url, "m", None).expect("constructs");
EOF
    expect_reject "$tmp/qualified" "[deprecated-ctor]" "qualified new()"

    # 2b. Rule A, the other constructor, in the smoke harness root -- a separate workspace the
    #     root cargo commands never reach, so the scan must include it explicitly.
    skeleton "$tmp/qualified-timeout"
    cat > "$tmp/qualified-timeout/smoke/src/runner.rs" <<'EOF'
let p = OpenAiCompatibleProvider::with_timeout(url, "m", None, Duration::from_secs(1));
EOF
    expect_reject "$tmp/qualified-timeout" "[deprecated-ctor]" "qualified with_timeout() in smoke"

    # 2c. Rule A in a rustdoc example: a doctest compiles, so it is a caller like any other.
    skeleton "$tmp/doctest"
    cat > "$tmp/doctest/src/lib.rs" <<'EOF'
/// ```
/// let p = OpenAiCompatibleProvider::new("http://h/v1", "m", None);
/// ```
pub fn documented() {}
EOF
    expect_reject "$tmp/doctest" "[deprecated-ctor]" "qualified new() in a doctest"

    # 3. Rule B: a second `Self::new(` inside the compat file -- one delegation too many.
    skeleton "$tmp/second-self"
    cat >> "$tmp/second-self/src/providers/openai_compat.rs" <<'EOF'
fn second() -> Result<OpenAiCompatibleProvider, ProviderError> {
    Self::new("http://h/v1".to_string())
}
EOF
    expect_reject "$tmp/second-self" "[self-delegation]" "second Self:: call in the compat file"

    # 3b. Rule B, the other direction: the expected delegation is gone. A per-site exclusion
    #     would stay green here; the count does not.
    skeleton "$tmp/no-self"
    cat > "$tmp/no-self/src/providers/openai_compat.rs" <<'EOF'
impl OpenAiCompatibleProvider {
    pub fn new(base_url: String) -> Result<Self, ProviderError> {
        Self::with_dialect(base_url, Dialect::default(), DEFAULT_CLIENT_TIMEOUT)
    }
}
EOF
    expect_reject "$tmp/no-self" "[self-delegation]" "missing Self:: delegation"

    # 4. `Self::new(` in ANY OTHER file constructs some other type and must NOT fire. This is
    #    the fixture that keeps rule B scoped: widening it once produced 44 hits, not 32.
    skeleton "$tmp/other-self"
    cat > "$tmp/other-self/src/reporting.rs" <<'EOF'
impl ReportConfig {
    pub fn default() -> Self {
        Self::new(52)
    }
}
EOF
    expect_accept "$tmp/other-self" "Self::new in an unrelated file"

    # 5. THE BLIND SPOT, PINNED: an aliased call is invisible to this grep, and the self-test
    #    asserts that it is. A red here would be the wrong result -- the check exists to catch
    #    a new `#[allow(deprecated)]` on the QUALIFIED spelling, not to redo the type check
    #    the compiler already performs (which DOES fire on this alias). The day someone
    #    believes they closed this edge, this fixture says whether they did.
    skeleton "$tmp/alias"
    cat > "$tmp/alias/tests/aliased.rs" <<'EOF'
use magi_core::providers::openai_compat::OpenAiCompatibleProvider as P;
#[allow(deprecated)]
let p = P::new(url, "m", None).expect("constructs");
EOF
    expect_accept "$tmp/alias" "aliased call (documented blind spot)"

    # 6. An empty scan set is a failure, not a pass.
    mkdir -p "$tmp/none/src" "$tmp/none/tests" "$tmp/none/examples" "$tmp/none/smoke/src"
    expect_reject "$tmp/none" "[empty-scan]" "empty scan set"

    # 7. A missing root is a failure too, for the same reason.
    skeleton "$tmp/missing-root"
    rm -r "$tmp/missing-root/smoke"
    expect_reject "$tmp/missing-root" "not found" "missing root"

    if [ "$fails" -eq 0 ]; then
        echo "check_no_deprecated_ctors: self-test OK -- 10 cases"
        exit 0
    fi
    exit 1
}

[ "${1:-}" = "--self-test" ] && self_test

scan "$ROOT"
echo "check_no_deprecated_ctors: OK"
