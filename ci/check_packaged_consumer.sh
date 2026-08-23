#!/bin/sh
# Author: Julian Bolivar
# Version: 1.0.0
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
# Whether docs.rs builds the published version. That exists only after publishing
# and has no local anticipation; it is out of scope rather than deferred, because a
# check that cannot stop anything is not a gate.
#
# Usage: sh ci/check_packaged_consumer.sh   (from the repo root)
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# STRUCTURED read, not `grep | sed`. A previous version of this idea in the plan
# carried a backreference that had become a CONTROL BYTE rather than `\1`, so it
# returned the empty string and the comparison downstream passed against nothing.
VERSION="$(cargo metadata --no-deps --format-version 1 \
  | python -c "import json,sys; print(json.load(sys.stdin)['packages'][0]['version'])")"

if [ -z "$VERSION" ]; then
  echo "check_packaged_consumer: could not read the crate version" >&2
  exit 1
fi

PKG_DIR="$ROOT/target/package/magi-core-$VERSION"

# `--allow-dirty` on purpose: the subject is what the MANIFEST would package, and
# during development the tree carries uncommitted edits that belong in the answer.
# The release itself packages from a clean tree, which is strictly narrower.
echo "=== packaging magi-core $VERSION ==="
cargo package --allow-dirty

if [ ! -d "$PKG_DIR" ]; then
  echo "check_packaged_consumer: expected $PKG_DIR after cargo package" >&2
  exit 1
fi

# Its OWN target dir. Two builds sharing one `target/` relink the same binaries and
# produce link errors that read as code defects — a trap this project has already
# paid for twice.
echo "=== compiling the packaged examples as outside consumers ==="
CARGO_TARGET_DIR="$ROOT/target/packaged-consumer" \
  cargo build --manifest-path "$PKG_DIR/Cargo.toml" --examples --all-features

echo "check_packaged_consumer: OK ($VERSION, examples compile against the packaged source)"
