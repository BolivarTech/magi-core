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
# A name counts when it STARTS WITH A LETTER and CONTAINS A DIGIT. Model names carry a version or a
# parameter count — `qwen3.5:397b`, `gpt-oss:120b`, `gemma4`, `o1` — and the identifiers that were
# being counted as models do not: `magi-core`, `provider_url.rs`, `check_redaction.sh`, `max_tokens`.
#
# An earlier form ALSO demanded a separator, which excluded `gemma4` — a model actually in the pool
# — and would have excluded a name like `o1` outright. Requiring the separator was a proxy for
# "looks versioned"; the digit is the property that was meant. The letter start keeps a bare version
# number such as `0.13` from being read as a model.
#
# The two exclusions are written as separate anchored greps rather than one alternation. The
# combined form produced the same results — verified against every name in the block — but a
# reviewer read its backtick placement as a bug, and a check whose correctness has to be argued is
# one that gets edited by someone who is not sure. Legibility is part of the job here.
models="$(echo "$block"     | grep -oE '`[A-Za-z][A-Za-z0-9.:_-]*`'     | grep -E '[0-9]'     | grep -vE '^`v[0-9]'     | grep -vE '^`[A-Z_]+`$'     | sort -u | wc -l || true)"
models="${models:-0}"
[ "$models" -ge "$MIN_MODELS" ] \
    || fail "$CONST's rustdoc names $models model(s); at least $MIN_MODELS are required, since one model says nothing about the rest. A name is counted when it is in backticks, starts with a letter and contains a digit — every model in use carries a version or a parameter count, and the identifiers that were being miscounted carry neither. If a real model name has no digit at all, widen the pattern rather than padding the block"

echo "check_calibration: OK ($models models named, date present)"
