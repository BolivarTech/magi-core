#!/bin/bash
# Author: Julian Bolivar
# Version: 1.0.0
# Date: 2026-07-30
#
# Seal on the response-body cap's calibration.
#
# `BYTES_PER_TOKEN_CEILING` is the factor that turns `max_tokens` into a byte budget. A number like
# that is easy to pick by intuition and then describe as if it had been measured, so this asserts
# that its rustdoc actually carries evidence: a date and at least three named models.
#
# WHAT THIS DOES NOT DO: it cannot check that the measurement is correct, or recent, or that the
# models named are the ones in use. It converts "the calibration was done" from an assertion into
# an artefact — no more than that. A stale but well-formed block passes.
set -euo pipefail

SRC="${SRC:-src}"
FILE="${FILE:-$SRC/providers/provider_url.rs}"
CONST="BYTES_PER_TOKEN_CEILING"
MIN_MODELS=3

fail() { echo "check_calibration: $1" >&2; exit 1; }

[ -f "$FILE" ] || fail "$FILE not found — update this script"

# The rustdoc block is everything from the start of the doc comment down to the definition.
#
# Attributes and blank lines between the doc and the definition do NOT reset it. An earlier version
# cleared the buffer on any non-`///` line, so a `#[cfg(...)]` or a stray blank between the evidence
# and the constant made the whole block invisible and the seal reported "no rustdoc" — failing for
# a reason that has nothing to do with the calibration, which is how a check earns a reputation for
# crying wolf and then gets removed.
block="$(awk -v c="pub(crate) const $CONST" '
  index($0, c)       { print buf; exit }
  /^[[:space:]]*\/\/\// { buf = buf "\n" $0; next }
  /^[[:space:]]*$/   { next }
  /^[[:space:]]*#\[/ { next }
  { buf = "" }
' "$FILE")"

[ -n "$block" ] || fail "no rustdoc found on $CONST"

echo "$block" | grep -qE '[0-9]{4}-[0-9]{2}-[0-9]{2}' \
    || fail "$CONST's rustdoc carries no measurement date — a number without a date is an intuition"

# Model rows are the backticked identifiers in the evidence table. Counting distinct ones keeps a
# single model repeated across rows from passing as three.
#
# A model name must contain a `.`, `-` or `:` — every family in use is versioned or tagged
# (`qwen3.5:397b`, `gpt-oss:120b`, `nemotron-3-super`). Without that requirement the count picked up
# ordinary code identifiers from the same rustdoc — `max_tokens`, the constant's own name — so a
# block naming ONE model could pass a floor of three. It counted backticks, not evidence.
# The `|| true` is load-bearing under `pipefail`: with no matches a grep exits non-zero and takes
# the whole script with it, so a rustdoc block naming ZERO models died here instead of reaching the
# message written for exactly that case. The seal would then have failed for the right reason with
# the wrong explanation — and an unexplained failure is how a check gets deleted.
# A DIGIT is required as well as a separator. The separator alone let ordinary hyphenated
# identifiers through — `magi-core`, `provider_url.rs`, `check_redaction.sh` — so three of those in
# a rustdoc satisfied a floor meant to require three measured models. Every model in use carries a
# version or a parameter count (`qwen3.5:397b`, `gpt-oss:120b`, `nemotron-3-super`), and none of the
# identifiers that were slipping through does.
models="$(echo "$block"     | grep -oE '`[A-Za-z0-9]+[A-Za-z0-9.:_-]*`'     | grep -E '[.:-]'     | grep -E '[0-9]'     | grep -vE '`[A-Z_]+`'     | sort -u | wc -l || true)"
models="${models:-0}"
[ "$models" -ge "$MIN_MODELS" ] \
    || fail "$CONST's rustdoc names $models model(s); at least $MIN_MODELS are required, since one model says nothing about the rest"

echo "check_calibration: OK ($models models named, date present)"
