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

# A missing tree is a FAILURE, not a clean run. Every provider-scoped rule iterates a `find`, so a
# wrong path or a moved directory made all of them pass over nothing and the script reported OK —
# the same vacuous success the fixture floor exists to prevent, one level up.
[ -d "$SRC" ] || fail "$SRC not found — update this script"
[ -d "$PROVIDERS" ] || fail "$PROVIDERS not found — update this script"

# A SECOND exception, and the reason it is safe is CHECKED rather than asserted. The subprocess
# provider composes error text from `std::io::Error` and from a parse failure, neither of which can
# carry a URL, because that provider has none — it spawns a program. Excluding it on that basis
# alone would rot the moment it grew one, so the rule below fails loudly if it ever names an HTTP
# client or the URL type. The exception is then a condition, not a memory.
NO_URL="claude_cli.rs"

# Deny-by-default over the directory, RECURSIVE: EVERY `.rs` under it except the two allowlisted by
# name, so a provider that grows into `providers/gemini/mod.rs` is covered the day it appears
# rather than the day someone remembers to add it.
#
# An earlier version also required the file to mention `reqwest`, which put an allow-by-default
# filter INSIDE a deny-by-default check: a provider file that interpolated an error without naming
# that crate was never scanned at all, and one such file was already here. The filter bought
# nothing — the rules match code shapes, so a file with none of them passes in microseconds.
provider_files() {
    find "$PROVIDERS" -name '*.rs' ! -name "$ALLOW" ! -name "$NO_URL" 2>/dev/null || true
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
# KNOWN BLIND SPOTS, stated rather than implied. All three fail in the SAFE direction — false
# positives, which are loud — but a maintainer who hits one should know it is a documented limit
# rather than a bug to chase:
#
#   * a test module NESTED inside another module is not skipped (the detection is anchored at
#     column 0), so its body is scanned as production;
#   * BLOCK comments (`/* … */`) are not stripped, only line comments;
#   * a string literal spanning lines can leave its contents visible to the line-wise rules.
#
# Attributes, blank lines and COMMENTS between the gate and the `mod` do NOT cancel it — all three
# were found the same way, one per review round, which is the argument for the category over the
# list: what cancels the gate is a line of CODE, not any of the things that decorate one.
#
# HANDLED, each verified against the regex rather than assumed: `#[cfg(test)]`, the compound in
# either argument order, a compound with a NESTED paren before the token
# (`all(not(feature = "y"), test)`), and `any(test, doc)`. Correctly NOT treated as test modules:
# `#[cfg(test_helper)]`, every `#[cfg(feature = "…test…")]` spelling (the token is blanked with
# the literal, so `test-utils`, `my_test`, `a-test` and `integration.test` are all inert), and
# `#[cfg(not(test))]` — the last of which is production code, and skipping it was a real leak.
#
# LINE COMMENTS ARE STRIPPED. Every rule here looks for a code shape, and a comment that mentions
# one is prose, not the thing. It matters most for the per-builder count: a comment naming
# `Client::builder` inflated the builder tally and failed a correct file, which is the way a check
# gets switched off. The `//` must follow whitespace or start the line, so a URL inside a string
# literal survives intact.
prod_only() {
    awk '
      # Strips a line comment WITHOUT touching a `//` inside a string literal. The earlier
      # whitespace-guard was not enough: `let s = "a // b";` has a space before it too, so the
      # literal was cut and the rest of the line vanished from every rule. Walking the quotes is
      # the only way to tell the two apart, and a corrupted line is a false NEGATIVE — the
      # direction that hides a leak.
      #
      # A RAW string (`r#"…"#`) defeats the quote walk: its `"` characters are ordinary, and
      # `\` does not escape, so tracking them inverts `inq` and can cut a line that had no
      # comment at all — a false NEGATIVE. Such a line is therefore left ALONE. That is the
      # safe direction: an unstripped comment can only add a false positive.
      function strip_comment(line,   i, c, inq, esc) {
          if (index(line, "r\"") || index(line, "r#\"") || index(line, "\"#")) { return line }
          for (i = 1; i <= length(line); i++) {
              c = substr(line, i, 1)
              if (esc)            { esc = 0; continue }
              if (c == "\\")     { esc = 1; continue }
              if (c == "\"")      { inq = !inq; continue }
              if (!inq && c == "/" && substr(line, i + 1, 1) == "/") {
                  return substr(line, 1, i - 1)
              }
          }
          return line
      }
      # `[^)]*` could not cross a nested `)`, so `#[cfg(all(not(feature = "y"), test))]` was not
      # recognised and that module was scanned as production. `.*` crosses it; the optional group
      # is what keeps the bare `#[cfg(test)]` matching, since there the `(` IS the separator.
      #
      # `not(test)` is EXCLUDED, and that one was wrong in the unsafe direction from the start:
      # it marks code compiled only OUTSIDE tests — production by definition — and skipping it
      # hid real production lines from every rule below.
      # Matched against a copy with STRING LITERALS BLANKED, because the token only means "test
      # module" when it is a cfg predicate, never when it is part of a feature NAME. Excluding the
      # quote character alone was not enough: `#[cfg(feature = "a-test")]` and
      # `#[cfg(feature = "integration.test")]` both put a permitted character before the token, so
      # a production feature-gated module was skipped entirely — a false negative, and the class
      # rather than the instance is what gets closed here.
      { probe = $0; gsub(/"[^"]*"/, "@", probe) }
      probe ~ /^#\[cfg\((.*[^a-z_"])?test[^a-z_-]/ &&
      probe !~ /not[[:space:]]*\([[:space:]]*test[^a-z_-]/ { pending = 1; print ""; next }
      pending && /^(pub([(][^)]*[)])? )?mod [A-Za-z0-9_]+[ 	]*\{/ { skipping = 1; pending = 0; print ""; next }
      # A FURTHER attribute or a blank line between the cfg and the `mod` does NOT cancel it.
      # Clearing on any non-`mod` line meant `#[cfg(test)]` followed by `#[allow(…)]` left the
      # module unrecognised, so its whole body was scanned as production — a false positive, and
      # one that arrives with no clue as to why, since the offending line is a test.
      pending && /^([[:space:]]*$|#\[|\/\/)/ { print ""; next }
      pending                       { pending = 0 }
      skipping && /^\}/             { skipping = 0; print ""; next }
      skipping                      { print ""; next }
                                    { print strip_comment($0) }
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
#
#     The tracing sigil has TWO spellings and only one was covered. `warn!(err = %e, …)` names the
#     field; `warn!(%e, …)` lets the sigil name it, which is the shorter form and therefore the
#     likelier one. Requiring the `=` matched the verbose spelling and missed the terse one — a
#     false negative, in a crate that already logs with `tracing`.
for f in $(provider_files); do
    if prod_only "$f" | grep -nE '\{(e|err|error|source)(:#?\?)?\}|\b(e|err|error|source)\.to_string\(\)|[=(,][[:space:]]*[%?](e|err|error|source)\b|\{[0-9]*(:#?\?)?\}[^"]*"[^;]*[(,][[:space:]]*&?(e|err|error|source)[,);[:space:]]'; then
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
#
#      ANCHORED at the start of a line, so the sentence in a comment explaining why this impl is
#      forbidden does not fail the build. That is not hypothetical here: this file's own rationale
#      names the impl, and an earlier crate-wide check had to be re-anchored to a definition for
#      exactly the same reason — documenting an invariant should never break it.
#
#      Routed through `prod_only` like every other rule, which is both consistency and correctness:
#      an impl written inside a test module exists only in test builds, so it cannot be the `?` a
#      release binary takes. Flagging it would be a false positive on code that is already safe.
for f in $(find "$SRC" -name '*.rs' 2>/dev/null || true); do
    if prod_only "$f" | grep -nE '^[[:space:]]*impl( +[^ ]+)? +From<reqwest::Error>'; then
        fail "From<reqwest::Error> in $f would bypass the mapper via \`?\`"
    fi
done

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
# 2c — the no-URL exception must STAY a no-URL file. The subprocess provider is skipped by rule 1
#      because nothing it interpolates can carry a URL. The day it gains an HTTP client or holds a
#      `ProviderUrl`, that premise is false and its exemption has to go — so the premise is checked
#      instead of remembered. An exception nobody re-examines is how a covered file goes quiet.
if [ -f "$PROVIDERS/$NO_URL" ]; then
    if prod_only "$PROVIDERS/$NO_URL" | grep -nE '(^|[^A-Za-z_])(reqwest|ProviderUrl)([^A-Za-z0-9_]|$)'; then
        fail "$NO_URL is exempt from rule 1 only because it has no URL; it now does — remove it from the exemption and compose its errors through the shared mapper"
    fi
fi

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
#     COVERS THE ERROR CLASSIFIERS TOO, not just the outcome dispatch. Watching one of the two
#     was a gap found by probing rather than by reading: a `_ =>` restored to the function that
#     maps a provider error onto an outcome passed cleanly, and that function is where a new error
#     variant would silently inherit the run-wide consequence — the exact regression this file was
#     extended to prevent, one release earlier, in the review that introduced the fix.
#
#     A nested `match` inside an arm is NOT a false positive: the terminator is indent-aware, so a
#     deeper catch-all belongs to the inner match and is ignored. Verified by injecting one.
if [ -f "$SRC/orchestrator.rs" ]; then
    prod_only "$SRC/orchestrator.rs" | awk '
      # A STACK, not a single depth. A `match` on the same subject nested inside an arm of another
      # overwrote the outer one, so when the inner closed the walker considered itself out of the
      # block entirely — and every remaining arm of the OUTER match went unguarded, a catch-all
      # among them. Pushing and popping keeps the outer alive underneath.
      /match (outcome|err) \{/ {
          seen++
          match($0, /^[ 	]*/); depth++; stack[depth] = RLENGTH; ind = RLENGTH; next
      }
      depth > 0 && /^[ 	]*\}[;]?[ 	]*$/ {
          match($0, /^[ 	]*/)
          if (RLENGTH <= ind) { depth--; ind = depth > 0 ? stack[depth] : -1 }
          next
      }
      depth > 0 && /^[ 	]*_[ 	]*=>/ {
          # AT THE ARM LEVEL of the innermost WATCHED match, which is deliberate and is not the
          # same thing as "any nested catch-all is fine":
          #
          #   * a match on an UNWATCHED subject nested in an arm keeps its own catch-all — its
          #     arms sit deeper than those of the watched match, so they are ignored. A fixture
          #     pins it.
          #   * a match on a WATCHED subject nested in an arm is itself watched, and its catch-all
          #     IS flagged. That is the intent: the rule is about the subject, not the nesting, and
          #     a classifier does not stop needing exhaustiveness by being written inside another.
          #
          # Flagging every catch-all inside the block, watched subject or not, made the rule fire on
          # correct code the moment it was widened to a second subject.
          match($0, /^[ 	]*/)
          if (RLENGTH <= ind + 4) { print "catch-all arm at line " NR; bad = 1 }
          next
      }
      # A CHECK THAT WATCHED NOTHING MUST NOT REPORT SUCCESS. The subjects are matched by NAME, so
      # renaming them in Rust turns this rule into a silent no-op — it would keep passing while
      # guarding nothing, which is the failure mode the whole file exists to avoid. Seeing zero is
      # therefore an error, not a clean run.
      #
      # Its limit, stated rather than implied: this catches the file being emptied, moved, or every
      # subject renamed. Renaming ONE of several classifiers still leaves the others visible and
      # slips through. A count cannot fix that without hard-coding how many there should be, which
      # is a number that goes stale on its own. The rest of that gap is carried by the note beside
      # the match expressions in the Rust source, where the person doing the renaming is reading.
      END {
          if (seen == 0) {
              print "no classifying match found — this rule watches subjects BY NAME, so a rename turned it into a no-op"
              exit 1
          }
          exit bad ? 1 : 0
      }
    ' || fail "a catch-all arm would silence the outcome decision, or the subjects this rule watches by name were renamed"
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
    # Every spelling that yields a DEFAULT client: Referer on, and no builder to turn it off. They
    # are rejected outright rather than asked for a call they have no way to make.
    #
    # `Client::default()` is the same client as `Client::new()` by another name, and listing only
    # the latter left the shorter spelling as a silent bypass of this whole rule — the same
    # one-alternative-of-several gap that pattern 1 had, in the check that guards the leak nothing
    # else can see.
    if echo "$prod" | grep -qE 'Client::(new|default)\(\)'; then
        fail "$f builds a default HTTP client, which cannot disable referer — use the builder"
    fi
    # PER BUILDER, not per file. A file-level check is satisfied by one configured client while a
    # second one beside it goes bare — and a provider module holding a completions client and a
    # probe client is exactly the shape this crate has. Counting is enough here: every builder
    # must carry the call, so the counts have to match.
    # Both spellings of "start a builder" count, for the same reason: `ClientBuilder::new()` is
    # `Client::builder()` written the long way, and a tally that saw only one of them would let a
    # second, unconfigured client sit beside a configured one without changing the count.
    builders="$(echo "$prod" | grep -cE 'Client::builder|ClientBuilder::new' || true)"
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
    #
    #      BOTH SPELLINGS. Matching only `Self::` left the fully-qualified `ProviderError::Http {}`
    #      invisible — and inside `error.rs` the two are interchangeable, so the door the check
    #      guards had an unwatched second leaf.
    #      The brace may follow WITHOUT a space — `Self::Http{ .. }` is ordinary Rust, and requiring
    #      the space let it through. The trailing `|| true` matters just as much: under `pipefail`
    #      a grep that matches nothing kills the script, so a file constructing NOTHING exited
    #      silently instead of reaching the message written for that case.
    #      Only the part of a line that can CONSTRUCT is read: everything after the last `=>`, or
    #      the whole line when there is none. `Self::Http { .. }` on the left of an arrow is a
    #      PATTERN — it destructures, it does not build — and counting it made a plain `match` over
    #      the enum look like a second door. `X => Self::External { .. }` still counts, because
    #      what follows the arrow is a construction.
    built="$(prod_only "$SRC/error.rs" \
        | awk '{ while (match($0, /=>/)) { $0 = substr($0, RSTART + 2) } print }' \
        | grep -oE '(Self|ProviderError)::[A-Za-z]+[[:space:]]*\{' \
        | sed -e 's/^ProviderError::/Self::/' -e 's/[[:space:]]*{$/ {/' \
        | sort -u | tr '\n' ' ' || true)"
    [ "$built" = "Self::External { " ] \
        || fail "error.rs must construct External and nothing else, found: ${built:-<none>}"
fi

echo "check_redaction: OK"
