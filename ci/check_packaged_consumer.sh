#!/bin/sh
# Author: Julian Bolivar
# Version: 4.0.0
# Date: 2026-08-23
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
#   * The packaged `tests/`. They are never compiled, so a fixture that failed to
#     reach the tarball passes this check.
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
# The packaged manifest is the witness, and it is list-free: it does not name which
# examples must exist, only that the artifact declares some.
if ! grep -q '^\[\[example\]\]' "$PKG_DIR/Cargo.toml"; then
  echo "check_packaged_consumer: the packaged manifest declares NO examples, so this" >&2
  echo "check would have compiled nothing and reported success. Did an 'exclude' or" >&2
  echo "'include' entry stop examples/ from reaching the package?" >&2
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
DECLARED="$(grep -c '^\[\[example\]\]' "$PKG_DIR/Cargo.toml")"
echo "check_packaged_consumer: OK ($VERSION, $DECLARED packaged example(s) declared and built)"
