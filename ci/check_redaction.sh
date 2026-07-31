#!/bin/bash
# Author: Julian Bolivar
# Version: 1.0.0
# Date: 2026-07-30
#
# Ratchet against the regression of a KNOWN leak path.
#
# NOT a proof: these checks are static and name-sensitive, so a novel form evades them. They stop
# what was already removed from coming back — the failure mode that actually happens. What they
# cannot see is covered by the human review checklist in the plan.
#
# Run `--self-test` to verify each pattern still matches what it claims to match: a check that
# silently stopped matching is worse than no check, because it reports success.
#
# # The maintenance cost is real, and accepted with a condition
#
# Grep and awk over Rust is fragile: every rule here has been wrong at least once, and four of them
# were wrong in the direction that reports success. The cost is paid deliberately, because the
# alternative on offer today is nothing — the leaks this guards are not expressible as a type, and
# the one that mattered most (`referer(false)`) is a call whose ABSENCE no test can observe.
#
# The condition: every rule carries a fixture pair, the self-test asserts the rule that fired by
# NAME, and a rule change is verified by injecting the defect it guards into the real tree — not by
# re-running the fixtures it was written against. Without those three, this file is theatre.
#
# If a Dylint lint or a sealed-trait formulation can express these invariants once the provider API
# settles, it should replace this wholesale. That is a better tool, not a different opinion.
set -euo pipefail

SRC="${SRC:-src}"
PROVIDERS="${PROVIDERS:-$SRC/providers}"
# Excluded from every provider-scoped pattern (1, 1d, 6) via `provider_files`. It is the module
# that legitimately interpolates ONE error — a URL parse error, whose variants carry no payload and
# therefore cannot echo the input — and it is held to the stricter rules 2/2b/3/4 instead.
ALLOW="provider_url.rs"

# Floor on the fixture count, so an emptied or mis-pathed directory cannot pass as a clean run.
MIN_FIXTURE_PAIRS=20

fail() { echo "check_redaction: $1" >&2; exit 1; }

# Deny-by-default over the directory, RECURSIVE: a provider that grows into a submodule
# (providers/gemini/mod.rs) must not fall outside the net silently.
# Scope, not exception: only files that actually speak HTTP can hold an error carrying a URL. A
# subprocess provider has no URL to leak, so including it would be noise — and a check that cries
# wolf is a check that gets silenced.
provider_files() {
    for f in $(find "$PROVIDERS" -name '*.rs' ! -name "$ALLOW" 2>/dev/null || true); do
        grep -q 'reqwest' "$f" 2>/dev/null && echo "$f"
    done
}

# Production half of a file: everything before `#[cfg(test)]`. Test assertions legitimately print
# errors — they are not a channel anything ships through.
#
# ANCHORED AT COLUMN 0, and matching `#[cfg(all(test, …))]` as well. The unanchored substring
# version was evadable by ordinary Rust: an indented `#[cfg(test)] use …;` or a test-only helper
# part-way down a file truncated the scan there and blinded every later pattern for the rest of the
# file — a `String` base_url, a raw error interpolation and an unconfigured client could all pass
# together. The mirror error was equally real: `#[cfg(all(test, feature = "ollama"))]` did not match
# at all, so that file's entire test module was scanned as production. The split was correct by
# luck in one file and by construction in none.
#
# SKIPS TEST MODULES; it does not truncate. Truncation cannot be right for a file with more than
# one test module — cutting at the first blinds everything below it, and cutting at the last leaves
# the earlier ones in the production half. `orchestrator.rs` has two, which is how a test ended up
# being scanned as production while the whole file below the first module went unchecked.
#
# A `#[cfg(test)]` that is NOT followed by a module with a BODY skips nothing — `#[cfg(test)] use
# serial_test::serial;` and `#[cfg(test)] mod tests;` both skip zero lines. The brace is required
# for exactly that reason: a bodyless `mod tests;` has nothing to skip, and without the brace the
# skip ran until the next unrelated column-0 `}`, blanking real production code — three rules at
# once. The visibility group also catches `pub(crate) mod tests { … }`, which the earlier form
# missed entirely.
#
# Skipped lines are emitted as BLANK rather than dropped, so `NR` stays aligned with real file line
# numbers. Pattern 5 and the `grep -n` rules report positions, and shifting them silently would
# make every future report point at the wrong line.
#
# KNOWN BLIND SPOT, stated rather than implied: the test-module detection is anchored at column 0,
# so a test module nested inside another module is NOT skipped and its body is scanned as
# production. That fails in the SAFE direction — false positives, which a fixture catches and a
# human notices — but a maintainer reading only the regex would assume otherwise.
#
# LINE COMMENTS ARE STRIPPED. Every rule here looks for a code shape, and a comment that mentions
# one is prose, not the thing. It matters most for the per-builder count: a comment naming
# `Client::builder` inflated the builder tally and failed a correct file, which is the way a check
# gets switched off. The `//` must follow whitespace or start the line, so a URL inside a string
# literal survives intact.
prod_only() {
    awk '
      /^#\[cfg[^)]*[^a-z_"]test[^a-z_-]/ { pending = 1; print ""; next }
      pending && /^(pub([(][^)]*[)])? )?mod [A-Za-z0-9_]+[ 	]*\{/ { skipping = 1; pending = 0; print ""; next }
      pending                       { pending = 0 }
      skipping && /^\}/             { skipping = 0; print ""; next }
      skipping                      { print ""; next }
                                    { sub(/(^|[ 	])\/\/.*$/, ""); print }
    ' "$1"
}

# Builds a minimal tree that passes EVERY check, so a fixture placed into it is the only thing
# that can make the run fail.
#
# Without a valid skeleton the harness is worse than useless: an earlier version copied each
# fixture over `error.rs` as well, so the variant-existence check fired first and every fixture
# was "rejected" — by the wrong rule. Three patterns were gutted in review and the self-test
# still reported OK.
skeleton() {
    local tmp="$1"
    mkdir -p "$tmp/providers"
    printf 'use reqwest as _;\nfn clean() {}\n' > "$tmp/providers/subject.rs"
    printf 'use reqwest as _;\nfn redacted(&self) -> String { String::new() }\n' \
        > "$tmp/providers/provider_url.rs"
    {
        printf 'pub enum ProviderError {\n'
        for v in Http Network Timeout Auth Process ResponseTooLarge RetryAbandoned External; do
            printf '    #[non_exhaustive]\n    %s {\n        f: u8,\n    },\n' "$v"
        done
        printf '}\nfn build() -> Self { Self::External { f: 0 } }\n'
    } > "$tmp/error.rs"
    printf 'fn f() {\n    match outcome {\n        A => 1,\n        B => 2,\n    };\n}\n' \
        > "$tmp/orchestrator.rs"
}

# Reads a `// KEY: value` header line from a fixture.
fixture_meta() { sed -n "s|^// $2: ||p" "$1" | head -1; }

self_test() {
    local rc=0 dir="ci/fixtures/redaction"

    # The harness's own vacuity check. With the directory missing or empty, every loop below
    # skips and the run reports `self-test OK` — a self-test that certifies nothing while
    # claiming health, which is precisely the failure mode it exists to detect one level down.
    # The floor is a floor, not the current count: it must not need editing for every new pair.
    # Counted with `find`, not `ls`: under `pipefail` a failing `ls` aborts the script with its
    # own status and no message, which is a failure nobody can act on.
    local pairs
    pairs="$(find "$dir" -name '*_bad.rs' 2>/dev/null | wc -l || true)"
    if [ "${pairs:-0}" -lt "$MIN_FIXTURE_PAIRS" ]; then
        echo "self-test: found $pairs fixture(s) in $dir, expected at least $MIN_FIXTURE_PAIRS" >&2
        echo "  a self-test with no fixtures reports success while testing nothing" >&2
        exit 1
    fi

    for bad in "$dir"/*_bad.rs; do
        [ -e "$bad" ] || continue
        local name target expect tmp out
        name="$(basename "$bad" _bad.rs)"
        target="$(fixture_meta "$bad" TARGET)"
        expect="$(fixture_meta "$bad" EXPECT)"
        if [ -z "$target" ] || [ -z "$expect" ]; then
            echo "self-test: $name lacks a TARGET or EXPECT header" >&2; rc=1; continue
        fi
        tmp="$(mktemp -d)"; skeleton "$tmp"
        # Strip the metadata lines: an EXPECT string quotes the rule it must trigger, so
        # leaving it in the scanned file can satisfy the very check it is testing.
        grep -v '^// \(TARGET\|EXPECT\):' "$bad" > "$tmp/$target"
        out="$(SRC="$tmp" PROVIDERS="$tmp/providers" bash "$0" 2>&1)" && {
            echo "self-test: $name was NOT rejected" >&2; rc=1
        }
        # The message must name the rule that fired. Exit status alone proved nothing: any
        # unrelated check could reject the fixture and the pattern under test stay dead.
        case "$out" in
            *"$expect"*) ;;
            *) echo "self-test: $name rejected by the WRONG rule" >&2
               echo "  expected to contain: $expect" >&2
               echo "  actual: $out" >&2
               rc=1 ;;
        esac
        rm -rf "$tmp"
    done

    for good in "$dir"/*_good.rs; do
        [ -e "$good" ] || continue
        local name target tmp out
        name="$(basename "$good" _good.rs)"
        target="$(fixture_meta "$good" TARGET)"
        if [ -z "$target" ]; then
            echo "self-test: $name lacks a TARGET header" >&2; rc=1; continue
        fi
        tmp="$(mktemp -d)"; skeleton "$tmp"
        grep -v '^// \(TARGET\|EXPECT\):' "$good" > "$tmp/$target"
        if ! out="$(SRC="$tmp" PROVIDERS="$tmp/providers" bash "$0" 2>&1)"; then
            echo "self-test: $name rejected a clean file: $out" >&2; rc=1
        fi
        rm -rf "$tmp"
    done

    [ "$rc" -eq 0 ] && echo "check_redaction: self-test OK"
    exit "$rc"
}

[ "${1:-}" = "--self-test" ] && self_test

# 1 — error interpolation, in its FOUR syntaxes: named Display, named Debug (plain and pretty),
#     the tracing sigils, and POSITIONAL — `format!("failed: {}", e)`, which is the form most
#     people reach for first and which the named-capture pattern never saw. Four spellings for one
#     operation is why the structural rules (1c, 1d) matter more than this list: they make the
#     shared mapper the only way to build a transport error at all, whatever the spelling.
for f in $(provider_files); do
    if prod_only "$f" | grep -nE '\{(e|err|error|source)(:#?\?)?\}|\b(e|err|error|source)\.to_string\(\)|= *[%?](e|err|error|source)\b|\{[0-9]*(:#?\?)?\}[^"]*"[^;]*[(,][[:space:]]*&?(e|err|error|source)[,);[:space:]]'; then
        fail "raw error interpolation in $f (compose from a redacted URL instead)"
    fi
done

# 1b — DROPPED, and the reason matters more than the rule did.
#
# The intent was a positive rule: "every map_err routes through the shared mapper". Line-wise grep
# cannot express it — a multi-line closure puts the mapper call on a different line than `map_err(`,
# so the check fired on correct code. A check that cries wolf gets silenced, and a silenced check is
# worse than an absent one.
#
# What replaced it is stronger anyway, because it is enforced by the compiler and by types:
#   * pattern 1 forbids interpolating an error at all in production provider code;
#   * pattern 1c forbids the `From` impl, so `?` cannot convert a client error implicitly;
#   * pattern 1d forbids constructing a transport variant, so the mapper is the ONLY way to make one;
#   * `describe_parse_error` accepts only a serde error, so the safe case cannot be handed a
#     network error by mistake.
# Together those make "the message was composed elsewhere" a property of the type system rather than
# of a regex.

# 1c — no implicit conversion. Without this impl, `?` on a client Result does not compile, so the
#      compiler itself forces the mapper. This is the check that closes the `?`/match evasion.
if grep -rn 'impl From<reqwest::Error>' "$SRC" 2>/dev/null; then
    fail "From<reqwest::Error> would bypass the mapper via `?`"
fi

# 1d — transport errors are built in ONE place. Verify the variant names still exist first, so a
#      rename makes this SHOUT instead of silently matching nothing.
#
#      The list matches the prohibition below EXACTLY. `Auth` used to be verified here while being
#      absent from the rule — it is legitimately constructed when mapping a status — so the
#      anti-drift check was guarding a name with nothing behind it.
if [ "${SKIP_EXISTENCE:-0}" != "1" ]; then
    for v in Network Timeout ResponseTooLarge; do
        grep -q "    $v {" "$SRC/error.rs" 2>/dev/null \
            || fail "variant $v not found in error.rs — update this script"
    done
fi
for f in $(provider_files); do
    if prod_only "$f" | grep -nE 'ProviderError::(Network|Timeout|ResponseTooLarge)[[:space:]]*\{'; then
        fail "transport error constructed in $f (it must come from the shared mapper)"
    fi
done

# 2 / 2b — nothing that carries the URL crosses the module boundary, and only `redacted` may
#          return a string. POSIX classes, not \s: the latter is a GNU extension.
if [ -f "$PROVIDERS/$ALLOW" ]; then
    # `( +async)?` is load-bearing: nearly every function in that module is async, so a pattern
    # anchored on `pub(crate) fn` alone sees almost none of them. A `pub(crate) async fn
    # leak_url(&self) -> reqwest::Url` passed both of these unnoticed until review.
    if grep -nE '^[[:space:]]*pub(\(crate\))?( +async)? +fn .*-> *&?(reqwest::)?(Url|Request|RequestBuilder|Response)' "$PROVIDERS/$ALLOW"; then
        fail "a client type escapes $ALLOW"
    fi
    # Two functions may return a `String`, and the distinction is what the rule is about:
    #   * `redacted`             — the ONE rendering of a URL;
    #   * `read_diagnostic_body` — a SERVER's response body, which is not a URL at all.
    # Anything else returning a string from this module is a second way to render the secret.
    if grep -nE '^[[:space:]]*pub(\(crate\))?( +async)? +fn [a-z_]+.*-> *(String|&str)' "$PROVIDERS/$ALLOW" \
        | grep -vE 'fn (redacted|read_diagnostic_body)\('; then
        fail "only redacted() may return a string from $ALLOW"
    fi
    # 3 / 4 — one definition of the redaction; no derived Debug or Serialize on the wrappers.
    if [ "$(grep -c 'fn redacted(' "$PROVIDERS/$ALLOW")" -ne 1 ]; then
        fail "redacted() must be defined exactly once"
    fi
    if grep -nE '#\[derive\([^)]*(Debug|Serialize)' "$PROVIDERS/$ALLOW"; then
        fail "a derived Debug or Serialize on a URL wrapper would print or persist the secret"
    fi
fi

# 5 — the compiler-forces-a-decision invariant stays on: no catch-all inside the outcome match.
#     A line-wise grep cannot express "inside a block", so this walks it.
#
# THROUGH `prod_only`, like every other file-scoped rule. This one alone scanned the whole file,
# which nobody noticed until a test legitimately matched on an outcome and had every right to a
# catch-all: a test asserting "this is NOT the Transport arm" must be able to say `_ => panic!`.
# The rule is about production dispatch, not about how tests read the enum.
#
# The terminator accepts BOTH closing forms. `};` alone matches only the
# `let (a, b) = match outcome { … };` shape; a `match outcome { … }` used as a statement closes
# with a bare `}`, so `inblock` was never cleared and leaked to end of file — every later `_ =>`
# in the file would have been blamed on a match it has nothing to do with.
if [ -f "$SRC/orchestrator.rs" ]; then
    prod_only "$SRC/orchestrator.rs" | awk '
      /match outcome \{/ { match($0, /^[ 	]*/); ind = RLENGTH; inblock = 1; next }
      inblock && /^[ 	]*\}[;]?[ 	]*$/ {
          match($0, /^[ 	]*/)
          if (RLENGTH <= ind) { inblock = 0 }
          next
      }
      inblock && /^[ 	]*_[ 	]*=>/ { print "catch-all arm at line " NR; bad = 1 }
      END { exit bad ? 1 : 0 }
    ' || fail "a catch-all arm would silence the outcome decision"
fi

# 6 — no provider stores the URL as a String: the invariant behind "the secret is not a String".
for f in $(provider_files); do
    if prod_only "$f" | grep -nE '^[[:space:]]*base_url:[[:space:]]*String'; then
        fail "$f stores base_url as String — it must hold the URL authority type"
    fi
done

# 7 — every client this crate builds must have Referer OFF.
#
# The only PRESENCE rule here, and it is presence for a reason. The others forbid something, so a
# test can catch the forbidden thing appearing. This one requires something, and no test can catch
# it disappearing: the contract test builds its OWN client to prove the technique works, so it
# keeps passing after every provider has quietly lost the call.
#
# What the default does, measured rather than assumed: on a redirect the client sends the ORIGINAL
# url — query string included — as `Referer` to the target origin. For an endpoint authenticated by
# a query parameter that hands the credential to a third party, and it bypasses every other defense
# in this file because it never passes through anything this crate renders.
for f in $(find "$SRC" -name '*.rs' 2>/dev/null || true); do
    prod="$(prod_only "$f")"
    # `Client::new()` is a DEFAULT client: Referer on, and no builder to turn it off. It is
    # rejected outright rather than asked for a call it has no way to make — and it slipped past
    # the rule below, which only ever looked for the builder.
    if echo "$prod" | grep -q 'Client::new()'; then
        fail "$f uses Client::new(), which cannot disable referer — build the client instead"
    fi
    # PER BUILDER, not per file. A file-level check is satisfied by one configured client while a
    # second one beside it goes bare — and a provider module holding a completions client and a
    # probe client is exactly the shape this crate has. Counting is enough here: every builder
    # must carry the call, so the counts have to match.
    builders="$(echo "$prod" | grep -c 'Client::builder' || true)"
    [ "${builders:-0}" -gt 0 ] || continue
    configured="$(echo "$prod" | grep -c 'referer(false)' || true)"
    [ "${configured:-0}" -ge "${builders:-0}" ] \
        || fail "$f builds $builders HTTP client(s) but only $configured disable referer — on a redirect the default leaks the full URL, query included, to the target origin"
done

# 8 — the asymmetry this release opens: `External` is CONSTRUCTIBLE from another crate, every
#     other variant is not.
#
# Only the POSITIVE half is testable: `examples/external_provider.rs` compiles, which proves the
# door exists. The negative half cannot be — a test asserting that `ProviderError::Http { .. }`
# fails to compile from outside would be testing `rustc`, not this crate. So it is enforced through
# its two causes, both of which are greppable.
#
# It lives in this file rather than its own because `Http.status` is what drives lineage
# condemnation: letting a third party build one would hand it a run-wide consequence, which is the
# same ownership question the rest of these checks defend.
if [ -f "$SRC/error.rs" ] && [ "${SKIP_EXISTENCE:-0}" != "1" ]; then
    # 8a — no variant may quietly lose the attribute. Listed by name so that DELETING a variant
    #      breaks this check too, instead of silently shrinking what is verified.
    for v in Http Network Timeout Auth Process ResponseTooLarge RetryAbandoned External; do
        grep -B1 "^    $v {" "$SRC/error.rs" | grep -q 'non_exhaustive' \
            || fail "variant $v lost #[non_exhaustive] — it would become constructible from any crate"
    done
    # 8b — exactly ONE door. `error.rs` holds the only public constructor, so whatever it builds is
    #      what the outside world can build.
    built="$(prod_only "$SRC/error.rs" | grep -oE 'Self::[A-Za-z]+ \{' | sort -u | tr '\n' ' ')"
    [ "$built" = "Self::External { " ] \
        || fail "error.rs must construct External and nothing else, found: ${built:-<none>}"
fi

echo "check_redaction: OK"
