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
block="$(awk -v c="pub(crate) const $CONST" '
  index($0, c) { print buf; exit }
  /^\/\/\// { buf = buf "\n" $0; next }
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
models="$(echo "$block"     | grep -oE '`[A-Za-z0-9]+[A-Za-z0-9.:_-]*`'     | grep -E '[.:-]'     | grep -vE '`[A-Z_]+`'     | sort -u | wc -l)"
[ "$models" -ge "$MIN_MODELS" ] \
    || fail "$CONST's rustdoc names $models model(s); at least $MIN_MODELS are required, since one model says nothing about the rest"

echo "check_calibration: OK ($models models named, date present)"
