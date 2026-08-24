#!/bin/bash
# Author: Julian Bolivar
# Version: 4.0.0
# Date: 2026-08-23
#
# The full gate, in ONE place.
#
# # Why this file exists
#
# The pull-request workflow and the release workflow ran the same sequence, written out twice. The
# two had already drifted once — the release path was missing checks the PR path had — which is the
# worst possible direction, because the run that publishes is the one running fewer checks, and
# nothing about that is visible until something ships that should not have.
#
# Duplicated lists diverge; that is not a prediction, it is what happened. With one script, adding a
# check adds it to both paths, and a check can only be skipped on the release path by deleting it
# from the PR path too — which a reviewer sees.
#
# # Both feature sets, deliberately
#
# `--all-features` alone does not COMPILE the parts of the suite gated behind default features, and
# the default set does not compile the feature-gated integration tests. A break confined to either
# is invisible to the other, and that is not hypothetical: eight tests sat red through an entire
# review loop because only one set was being run.
#
# Examples are built under BOTH sets for the same reason. The proof that this crate's error type is
# constructible from outside lives ONLY in an example — inside the crate the variants are always
# constructible, so an in-crate test would pass while the published API stayed broken.
#
# Not included: `cargo audit`. The reason USED to be "it needs a generated lockfile and network
# access", and that stopped discriminating the moment the packaged-consumer step below was added:
# `cargo package` resolves its own lockfile and touches the registry index, so this script is no
# longer offline. The real reason it stays a separate job is that it depends on the RustSec
# advisory DATABASE — a third-party service whose outage would turn this gate red for a reason
# that has nothing to do with the tree, which is the ambiguous-red this project refuses to build
# into its own gate. Everything here answers a question about THIS source.
#
# # THE ORDER OF THE STEPS BELOW IS LOAD-BEARING — do not sort or regroup them
#
#   0. "Doctests run LAST" holds among the steps that SHARE a gate target dir, which is what the
#      ordering below is about. The packaged-consumer step runs after them and is exempt: it works
#      out of `${CARGO_TARGET_DIR:-$ROOT/target}` and its `packaged-consumer` subdirectory,
#      touching neither `gate-all` nor `gate-default`, so it cannot contend with anything
#      ordered here. Said explicitly because this block is what the next person reasons from
#      when they reorder something.
#
#   1. Examples are built BEFORE the test runs. After them, linking failed on Windows against the
#      example's own `.pdb`, because the harness had just written dozens of binaries into the same
#      directory and had not released the handles.
#   2. Doctests run LAST among the cargo steps, for the mirror reason: run earlier, they still held
#      handles when the next build tried to link.
#
# Both surfaced as linker errors naming a source file, which points the reader at code that is
# fine. Neither is a correctness constraint on the crate — they are constraints on this script.
set -euo pipefail

step() { printf '\n=== %s ===\n' "$1"; }

# ONE TARGET DIRECTORY PER FEATURE SET, and this is not a performance tweak - it is what makes the
# gate trustworthy.
#
# Sharing one `target/` between the two sets means every switch RELINKS the same binary paths, and
# on Windows that collided with handles the previous step had not released: LNK1104, cannot open
# the example's own .exe. The result was a gate that went red on contention rather than on a
# defect, which is worse than no gate - a failure nobody can attribute is a failure everyone learns
# to rationalise past. This crate has been here before, and isolating the feature sets is what made
# that verification trustworthy then too.
#
# It is also simply correct: the two sets are different builds, and giving them separate output
# directories stops them invalidating each other's artefacts on every alternation.
ALL_DIR="${CARGO_TARGET_DIR:-target}/gate-all"
DEF_DIR="${CARGO_TARGET_DIR:-target}/gate-default"

# Announced rather than discovered: without this the first failure is `no such subcommand`, from a
# step whose real subject is the crate. Checked here so the message names the missing tool.
command -v cargo-nextest >/dev/null 2>&1     || { echo "cargo-nextest is required: cargo install cargo-nextest" >&2; exit 1; }

step "format"
cargo fmt --check

step "clippy (all features)"
CARGO_TARGET_DIR="$ALL_DIR" cargo clippy --all-targets --all-features -- -D warnings

step "clippy (default features)"
CARGO_TARGET_DIR="$DEF_DIR" cargo clippy --all-targets -- -D warnings

# BEFORE the test runs, and the order is load-bearing on Windows. Run after them, these linked
# binaries failed with LNK1104/1201 — "cannot open file" against their own `.pdb` — because the
# test harness had just written dozens of executables into the same directory and the handles had
# not been released. Both built fine in isolation and only failed in sequence, which is the shape
# of an ordering problem wearing a linker error's clothes.
step "examples (all features)"
CARGO_TARGET_DIR="$ALL_DIR" cargo build --all-features --examples

step "examples (default features)"
CARGO_TARGET_DIR="$DEF_DIR" cargo build --examples

# BUILDING an example is not running it. `external_provider` asserts that an outside
# implementor's telemetry reports NOT MEASURED rather than zeros, and an assert that never
# executes guards nothing — the same shape as the edge-case test that sat behind
# `not(debug_assertions)` and therefore never ran in CI.
step "external provider example (behaviour, not just compilation)"
CARGO_TARGET_DIR="$ALL_DIR" cargo run --all-features --example external_provider

step "tests (all features)"
CARGO_TARGET_DIR="$ALL_DIR" cargo nextest run --all-features

# A SINGLE-feature configuration, which neither `--all-features` nor the default set compiles.
# `ollama` implies `openai-compat`, so `--all-features` always brings both and a consumer who
# enables only the OpenAI-compatible provider was building a combination no gate had ever seen.
# `check` rather than a full test run: the risk here is that the code does not COMPILE without
# its siblings' items in scope, and the behaviour is already covered by the two full runs.
step "openai-compat alone (compiles without its siblings)"
CARGO_TARGET_DIR="$DEF_DIR" cargo check --no-default-features --features openai-compat --all-targets

step "tests (default features)"
CARGO_TARGET_DIR="$DEF_DIR" cargo nextest run

step "docs (all features)"
CARGO_TARGET_DIR="$ALL_DIR" RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps

# BOTH feature sets, for the reason already written for doctests below: an item behind a feature
# gate is the one a default-features consumer never sees. Until this line the gate built docs
# under `--all-features` ONLY, and was therefore structurally unable to see the configuration
# docs.rs builds — which is no `default` at all, since this crate declares none. Five intra-doc
# links in `provider.rs`, a file that renders under every feature set, dangled on the default set
# and nothing could report it. Two of the five had just been ADDED by a fix.
#
# `Cargo.toml` now carries `[package.metadata.docs.rs] all-features = true`, so the published
# page resolves them either way. This step exists because that metadata is a promise about a
# service we cannot run locally, and the promise is worth nothing if the docs only build under
# one set.
step "docs (default features)"
CARGO_TARGET_DIR="$DEF_DIR" RUSTDOCFLAGS="-D warnings" cargo doc --no-deps

# LAST among the cargo steps, and the order is load-bearing on Windows. Running this before a
# `cargo build` made the build fail to LINK — error 1104/1201, "cannot open file" — because the
# doctest harness still held handles on artefacts in `target/`. Both examples built fine in
# isolation and failed only in sequence, which is the shape of an ordering bug rather than a code
# one: it reports a linker failure, so the first instinct is to look at the code it names.
step "doctests (all features)"
CARGO_TARGET_DIR="$ALL_DIR" cargo test --doc --all-features

# Both sets here too, for the same reason as everything else: a doctest on an item behind a feature
# gate is the one a default-features consumer never sees compiled, and a doctest that only exists
# under `--all-features` is a promise made to the smaller audience without being checked for them.
step "doctests (default features)"
CARGO_TARGET_DIR="$DEF_DIR" cargo test --doc

step "verdict-search rule"
bash ci/check_r0.sh

step "prose artifacts (self-test)"
sh ci/check_prose_artifacts.sh --self-test

step "prose artifacts"
sh ci/check_prose_artifacts.sh

step "redaction rule (self-test)"
bash ci/check_redaction.sh --self-test

step "redaction rule"
bash ci/check_redaction.sh

step "calibration seal"
bash ci/check_calibration.sh

# LAST, and deliberately so: it packages the crate and compiles the examples as
# outside consumers against that tarball, which costs a full dependency build in
# its own target dir, and `cargo package` runs a verification build of the library
# BEFORE that, so the step is roughly two full builds rather than one. Everything
# cheaper has already spoken by the time it runs.
step "packaged consumer (an outside crate compiles against the tarball)"
sh ci/check_packaged_consumer.sh

printf '\nall checks passed\n'
