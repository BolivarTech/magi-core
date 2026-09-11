#!/bin/sh
# Author: Julian Bolivar
# Version: 1.0.0
# Date: 2026-09-11
#
# The emit-side half of the one-shot warning gate.
#
# `Magi::warn_once` makes the cadence of a builder warning structural on the RENDER side: a
# `WarnOnce` variant cannot exist without naming its bit and its message. It does nothing on
# the EMIT side -- a bare `tracing::warn!` written beside the gate bypasses it entirely, and
# that is literally what R-18 was: two warnings latched, a third written without a flag, and
# nothing that could notice. This check is the only protection against a recurrence, because
# the lint that would replace it (`clippy::disallowed_macros`) is deferred past 4.1.0.
#
# ## The rule
#
# In every module that CALLS `warn_once` -- discovered by grep, never listed, so a second
# module adopting the gate is covered the day it does -- a direct `warn!(` invocation is
# rejected unless the line immediately before it carries the exemption marker
#
#     // warn-once-exempt: <reason>
#
# with a NON-EMPTY reason. Both halves are required. A bare marker is an exception without a
# justification, which is how an allow-list grows until it guards nothing. Comment lines are
# not scanned, so rustdoc that MENTIONS the macro does not trip the rule.
#
# ## What it does not see
#
# This is a grep, with the fragility this project already knows from `check_redaction.sh`. It
# looks for the macro NAME followed by `(`; a `warn!` reached through an alias, a wrapper
# macro, or `tracing::event!(Level::WARN, ..)` is invisible to it. Named here rather than
# left implied: a guard that looks wider than it is reads as coverage that does not exist.
#
# ## Discipline inherited from the siblings
#
# An EMPTY scan set is a FAILURE (R-29): with no module calling `warn_once` there is nothing
# to guard, and reporting OK over zero bytes is the vacuous success this file exists to
# refuse one level down. `--self-test` runs entirely inside `mktemp -d`, writes fixtures for
# each rule, and asserts WHICH rule fired by name -- exit status alone would also accept a
# guard failing for an unrelated reason.
set -eu

SRC="${SRC:-src}"
MARKER='// warn-once-exempt:'

fail() { echo "check_warn_once: $1" >&2; exit 1; }

# Resolve this script's own absolute path however it was invoked, BEFORE anything can change
# the working directory: the self-test re-invokes it from inside a throwaway tree, where a
# path relative to the original directory means nothing. Same shape as `check_pending.sh`,
# and for the same reason -- `"$OLDPWD/$0"` only worked when `$0` happened to be relative.
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

# The scan: every module that calls the gate, then every direct `warn!(` in it.
#
# The method-call form (`.warn_once(`) is what discovers a module, so the file that merely
# DEFINES `fn warn_once` is not pulled in by its definition -- it is pulled in the moment it
# calls the gate, which the orchestrator does.
#
# The macro pattern is spelled with a character class, not `\b`: a word boundary escaped one
# layer too many once became a literal backspace in a sibling rule, and a rule that matches
# nothing reports success. Comment lines are skipped so prose naming the macro is not a hit.
scan() {
    scan_root="$1"
    [ -d "$scan_root" ] || fail "$scan_root not found -- update this script"
    modules="$(find "$scan_root" -name '*.rs' -type f -exec grep -lE '\.warn_once\(' {} + 2>/dev/null || true)"
    if [ -z "$modules" ]; then
        fail "[empty-scan] no module under $scan_root calls warn_once; nothing to guard is not a pass"
    fi
    rc=0
    for module in $modules; do
        # Line numbers of every non-comment line invoking the macro.
        hits="$(grep -nE '(^|[^A-Za-z0-9_])warn!\(' "$module" | grep -vE '^[0-9]+:[[:space:]]*//' | cut -d: -f1 || true)"
        for line in $hits; do
            prev=$((line - 1))
            before="$(sed -n "${prev}p" "$module")"
            case "$before" in
                *"$MARKER"*)
                    reason="${before##*"$MARKER"}"
                    # Strip surrounding whitespace; what is left must be a reason.
                    reason="$(printf '%s' "$reason" | sed 's/^[[:space:]]*//; s/[[:space:]]*$//')"
                    if [ -z "$reason" ]; then
                        echo "check_warn_once: [empty-reason] $module:$line: exemption marker without a reason" >&2
                        rc=1
                    fi
                    ;;
                *)
                    echo "check_warn_once: [direct-warn] $module:$line: warn! outside the warn_once gate (latch it through WarnOnce, or annotate the line above with '$MARKER <reason>')" >&2
                    rc=1
                    ;;
            esac
        done
    done
    return "$rc"
}

self_test() {
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    fails=0

    # A module that uses the gate. Each fixture below is this skeleton plus one variation.
    skeleton() {
        cat > "$1" <<'EOF'
impl Magi {
    fn run(&self) {
        self.warn_once(WarnOnce::InertGuard { candidates: 1 });
    }
}
EOF
    }

    # Expects the run over `$1` to FAIL and its output to name rule `$2`.
    expect_reject() {
        if out="$(SRC="$1" sh "$SELF" 2>&1)"; then
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
        if ! out="$(SRC="$1" sh "$SELF" 2>&1)"; then
            echo "self-test: $2 was rejected: $out" >&2; fails=$((fails + 1))
        fi
    }

    # 1. A direct warn! in a module that uses the gate -- the R-18 shape.
    mkdir -p "$tmp/direct"; skeleton "$tmp/direct/a.rs"
    cat >> "$tmp/direct/a.rs" <<'EOF'
fn bypass() {
    tracing::warn!("said on every call");
}
EOF
    expect_reject "$tmp/direct" "[direct-warn]" "direct warn"

    # 2. The same, annotated with a reason: the legitimate exception.
    mkdir -p "$tmp/exempt"; skeleton "$tmp/exempt/a.rs"
    cat >> "$tmp/exempt/a.rs" <<'EOF'
fn per_call() {
    // warn-once-exempt: per-call condition, the input differs on every call
    tracing::warn!("input exceeds the threshold");
}
EOF
    expect_accept "$tmp/exempt" "annotated warn"

    # 3. A marker with nothing after it is a bypass wearing the marker's clothes.
    mkdir -p "$tmp/empty"; skeleton "$tmp/empty/a.rs"
    cat >> "$tmp/empty/a.rs" <<'EOF'
fn unjustified() {
    // warn-once-exempt:
    tracing::warn!("no reason given");
}
EOF
    expect_reject "$tmp/empty" "[empty-reason]" "empty reason"

    # 4. The marker must be on the line IMMEDIATELY before: one line further up is a comment
    #    about something else, and the warn! it precedes is unguarded.
    mkdir -p "$tmp/far"; skeleton "$tmp/far/a.rs"
    cat >> "$tmp/far/a.rs" <<'EOF'
fn displaced() {
    // warn-once-exempt: this reason belongs to a line that is not the next one
    let x = 1;
    tracing::warn!(x, "unguarded");
}
EOF
    expect_reject "$tmp/far" "[direct-warn]" "displaced marker"

    # 5. A bare `warn!(` after `use tracing::warn;` is the same bypass in a shorter spelling.
    mkdir -p "$tmp/bare"; skeleton "$tmp/bare/a.rs"
    cat >> "$tmp/bare/a.rs" <<'EOF'
use tracing::warn;
fn short() {
    warn!("bare spelling");
}
EOF
    expect_reject "$tmp/bare" "[direct-warn]" "bare warn!"

    # 6. A module that does NOT use the gate is out of scope: its warnings are per-event by
    #    design and the rule does not reach them. Prose naming the macro is not a hit either.
    mkdir -p "$tmp/scope"; skeleton "$tmp/scope/a.rs"
    cat > "$tmp/scope/b.rs" <<'EOF'
/// Emits a `tracing::warn!(..)` on every retry, deliberately.
fn other_module() {
    tracing::warn!("per-event, no gate here");
}
EOF
    expect_accept "$tmp/scope" "warn in a module outside the gate"

    # 7. An empty scan set is a failure, not a pass (R-29).
    mkdir -p "$tmp/none"
    cat > "$tmp/none/a.rs" <<'EOF'
fn nothing_here() {}
EOF
    expect_reject "$tmp/none" "[empty-scan]" "empty scan set"

    # 8. A missing root is a failure too, for the same reason.
    expect_reject "$tmp/does-not-exist" "not found" "missing root"

    if [ "$fails" -eq 0 ]; then
        echo "check_warn_once: self-test OK"
        exit 0
    fi
    exit 1
}

[ "${1:-}" = "--self-test" ] && self_test

scan "$SRC"
echo "check_warn_once: OK"
