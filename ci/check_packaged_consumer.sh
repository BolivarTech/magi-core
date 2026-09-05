#!/bin/sh
# Author: Julian Bolivar
# Version: 4.1.0
# Date: 2026-09-05
#
# An OUTSIDE consumer compiles against the PACKAGED source — before publishing.
#
# ## What this catches that nothing else does
#
# The measured case: `ProviderError` stopped being constructible from another crate
# (`E0639`) and reached a consumer EIGHT DAYS after the release, because inside
# `src/` the variants are always constructible and no test there could see it.
# `examples/external_provider.rs` exists for exactly that reason and says so in its
# own header — an example compiles as a SEPARATE crate, so `#[non_exhaustive]`
# applies to it as it applies to a real consumer.
#
# But the repo's own `cargo build --examples` compiles them against the WORKING
# TREE. This compiles them against the tarball `cargo package` produces, which is
# the artifact crates.io serves: the same source minus whatever `exclude`,
# `include` or an untracked file removes. A file that never made it into the
# package fails here and passes there.
#
# ## Why it is a local gate and not a post-publish job
#
# A job that verifies the package after `cargo publish` cannot withdraw anything —
# crates.io is immutable — so its red is a post-mortem, not a gate. This runs
# BEFORE the publish, on this machine, with no network beyond what cargo already
# needs and no backend at all.
#
# ## What it does NOT cover, stated so nobody reads it as more
#
#   * Whether docs.rs builds the published version. That exists only after
#     publishing and has no local anticipation; out of scope rather than deferred,
#     because a check that cannot stop anything is not a gate.
#   * The DEFAULT feature set. The examples are built under `--all-features` only,
#     while `run_all_checks.sh` builds the tree's examples under both — for the
#     reason written there, that an item behind a feature gate is the one a
#     default-features consumer never sees compiled. Not added here because this is
#     already the most expensive step in the gate; named so the gap is a decision.
#   * `tests/`, which as of 4.1.0 is NOT packaged at all (R-32). The gap this line
#     used to name -- packaged tests never being compiled -- is gone with them, and
#     a different one takes its place: a fixture a test needs now has to live under
#     `src/` to reach the tarball, and nothing here checks that it did. The
#     packaged crate simply carries no tests to run.
#   * BEHAVIOUR. The examples are COMPILED against the tarball and never run, so
#     what this proves is that the packaged source presents a usable API to an
#     outside crate -- which is the `E0639` class it exists for. The assertions
#     inside `external_provider` are executed by `run_all_checks.sh`, against the
#     TREE. A defect that compiles identically and behaves differently from the
#     package is outside both.
#
# Usage: sh ci/check_packaged_consumer.sh   (from the repo root)
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# STRUCTURED, and in pure shell. Two earlier shapes were rejected: `grep | sed`,
# whose backreference in the original plan had become a CONTROL BYTE rather than
# `\1` — it returned the empty string and the comparison downstream passed against
# nothing — and a `python` one-liner, which would make the LAST step of the
# publishing gate the only one needing an interpreter no workflow installs. Its
# absence would fail closed, but attributed to nothing.
#
# `cargo pkgid` prints `path+file:///…#magi-core@4.0.0`; the strip takes whatever
# follows the last `@` or `#`, which covers both that spelling and the older
# `…#4.0.0` one.
VERSION="$(cargo pkgid)"
VERSION="${VERSION##*[@#]}"

if [ -z "$VERSION" ]; then
  echo "check_packaged_consumer: could not read the crate version" >&2
  exit 1
fi

# `${CARGO_TARGET_DIR:-target}`, like `run_all_checks.sh` reads it. Hardcoding
# `target/` would send `cargo package` to one place and look for its output in
# another whenever that variable is set, and the guard below would then blame
# `cargo package` for something it did correctly — a red nobody can attribute,
# which this project rates worse than no gate.
TARGET="${CARGO_TARGET_DIR:-$ROOT/target}"
PKG_DIR="$TARGET/package/magi-core-$VERSION"

# `--allow-dirty` so the check runs mid-work, and the direction of its error is
# worth stating: locally it OVER-approximates, because an untracked file IS
# included in the package. So a green here can correspond to a package that, from
# a clean tree, is missing that file. What closes it is that the release path runs
# this same script on a fresh checkout, where `--allow-dirty` is a no-op and the
# answer is exact.
echo "=== packaging magi-core $VERSION ==="
cargo package --allow-dirty

if [ ! -d "$PKG_DIR" ]; then
  echo "check_packaged_consumer: expected $PKG_DIR after cargo package" >&2
  exit 1
fi

# THE STEP THAT KEEPS THIS CHECK FROM DISARMING ITSELF.
#
# `cargo build --examples` with no example targets prints a warning and exits 0 —
# verified by running it, not by reading the docs. So one `exclude` entry, one
# switch to an `include` list or one directory rename makes this script package the
# crate, compile NOTHING, and print that the examples compiled. The success line
# would then be a false statement, which is the false-negative class this project
# has paid for repeatedly.
#
# The witness is EVERY example the working tree declares, and the list is DERIVED
# rather than written down.
#
# Two earlier shapes were wrong, and the pair of them is the lesson. The first was
# deliberately list-free: it asked only that the artifact declare SOME example. That
# fails because cargo drops the `[[example]]` block of a file that did not reach the
# tarball, so excluding examples ONE AT A TIME slipped straight through — the script
# packaged the crate, compiled the survivors, and printed OK. The second named the
# single example whose property justified the check. That closed the hole for
# `external_provider.rs` and left it open for `decoupled_probe.rs`, which carries a
# second separate-crate property its own module doc states: that a probe can be
# declared apart from the provider serving completions, a coupling that cannot be
# exercised from inside `src/`, where a test double satisfies both bounds at once.
#
# Both mutations were found by RUNNING them, not by reading the guard. Naming files
# was the defect, not which file got named: a hand-kept list is wrong the moment
# someone adds an example, and wrong silently, which is the failure mode of every
# hand-maintained selection this project has been bitten by.
#
# So the tree is asked what it has. Every example present here must be declared by
# the packaged manifest; an example added tomorrow is covered with no edit, and any
# single one going missing fails loudly and by name. Both target shapes count, the
# flat `examples/x.rs` and the directory `examples/x/main.rs`.
# The packaged names are read from the `[[example]]` SECTIONS, not from the file
# at large. A bare `grep '^name = "x"'` over the whole manifest was wrong and the
# mutation that proves it is routine rather than exotic: `[package]`, `[lib]` and
# every one of the thirteen `[[test]]` blocks also carry a `name =` key, so an
# example named after an existing test satisfied the grep from the test's own
# block. Adding `examples/cross_milestone.rs` next to `tests/cross_milestone.rs`
# and excluding it from the package made cargo print `ignoring example
# cross_milestone` on stderr while this script printed OK — executed, not reasoned.
#
# That is the third time this guard has been holed by the same shape, so it is
# worth naming: each earlier version was correct for the tree in front of it and
# wrong for a tree someone would plausibly create next. Anonymous, then one name,
# then every name but matched too loosely.
PKG_EXAMPLES="$(awk '
  /^\[\[example\]\]/ { in_example = 1; next }
  /^\[/                 { in_example = 0 }
  in_example && /^name = "/ {
    line = $0
    sub(/^name = "/, "", line)
    sub(/"$/, "", line)
    print line
  }
' "$PKG_DIR/Cargo.toml")"

TREE_EXAMPLES=0
MISSING=''
for f in examples/*.rs examples/*/main.rs; do
  [ -e "$f" ] || continue
  case "$f" in
    examples/*/main.rs) n="$(basename "$(dirname "$f")")" ;;
    *) n="$(basename "$f" .rs)" ;;
  esac
  TREE_EXAMPLES=$((TREE_EXAMPLES + 1))
  printf '%s\n' "$PKG_EXAMPLES" | grep -Fqx "$n" || MISSING="$MISSING $n"
done

# Zero examples in the tree is itself a failure. Without this the loop would find
# nothing, `MISSING` would stay empty, and the check would pass having verified
# nothing — the same exit-0-on-an-empty-set shape that made `cargo build --examples`
# unsafe to trust in the first place.
if [ "$TREE_EXAMPLES" -eq 0 ]; then
  echo "check_packaged_consumer: the working tree declares NO examples, so this check" >&2
  echo "has nothing to compile as an outside crate and would report success having" >&2
  echo "verified nothing. Was examples/ renamed or removed?" >&2
  exit 1
fi

if [ -n "$MISSING" ]; then
  echo "check_packaged_consumer: the packaged manifest is missing example(s):$MISSING" >&2
  echo "They exist in the working tree, so the tree build compiles them and this gate" >&2
  echo "would have reported success while the artifact crates.io serves cannot build" >&2
  echo "them. Each example is here because it proves something only a SEPARATE crate" >&2
  echo "can prove. Did an 'exclude' or 'include' entry stop them reaching the package?" >&2
  exit 1
fi

# Its OWN target dir. Two builds sharing one relink the same binaries and produce
# link errors that read as code defects — a trap this project has already paid for
# twice.
echo "=== compiling the packaged examples as outside consumers ==="
CARGO_TARGET_DIR="$TARGET/packaged-consumer" \
  cargo build --manifest-path "$PKG_DIR/Cargo.toml" --examples --all-features

# Reports what happened rather than asserting the property: a count is checkable,
# a claim is not. It counts DECLARATIONS and says so — `cargo build --examples`
# silently skips a target whose `required-features` are unmet, so the day an
# example gains one, "compiled" would overstate what this number knows.
#
# The count is REPORTING, not a guard: the guard is the derived witness above, which
# already failed if any of them were missing. A count compared against nothing cannot
# fail, and a human diffing two logs to notice that 3 became 2 is not a gate.
# Counted off the same parsed list the guard used, so the number cannot describe a
# different set from the one that was checked. `|| true` because grep exits 1 on an
# empty list, which `set -e` would turn into a silent death with no message at all.
DECLARED="$(printf '%s\n' "$PKG_EXAMPLES" | grep -c . || true)"
echo "check_packaged_consumer: OK ($VERSION, $DECLARED packaged example(s) declared; cargo build --examples reported success)"
