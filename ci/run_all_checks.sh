#!/bin/bash
# Author: Julian Bolivar
# Version: 1.0.0
# Date: 2026-07-31
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
# Not included: `cargo audit`, which needs a generated lockfile and network access, so it stays a
# separate job in each workflow.
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

step "tests (all features)"
CARGO_TARGET_DIR="$ALL_DIR" cargo nextest run --all-features

step "tests (default features)"
CARGO_TARGET_DIR="$DEF_DIR" cargo nextest run

step "docs"
CARGO_TARGET_DIR="$ALL_DIR" RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps

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
sh ci/check_r0.sh

step "redaction rule (self-test)"
bash ci/check_redaction.sh --self-test

step "redaction rule"
bash ci/check_redaction.sh

step "calibration seal"
bash ci/check_calibration.sh

printf '\nall checks passed\n'
