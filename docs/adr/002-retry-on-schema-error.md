# ADR 002: Single-Shot Retry on Schema/Parse Errors

**Status:** Accepted
**Date:** 2026-05-16
**Related:** `sbtdd/spec-behavior.md` §3.4, §7 BDD-13, `docs/adr/001-prompt-injection-threat-model.md`

## Context

Python MAGI v2.2.0 introduced a single-shot retry when an agent's first
response fails schema validation (`pydantic.ValidationError`); v2.2.4
extended the scope to JSON parse errors (`json.JSONDecodeError`). The
mechanic is at `run_magi.py:530-549`. `magi-core` v0.3.1 has no retry
on schema errors — agents that fail parse/validation land directly in
`failed_agents`. This ADR records the design decisions for porting
that mechanic to Rust in v0.4.0.

## Decision

Add a single-shot retry layer to `Magi::analyze` that triggers ONLY on
`MagiError::Validation` or `MagiError::Deserialization` from the first
response. All other error categories (provider, timeout, auth, network,
process) skip retry and go directly to `failed_agents`.

The retry uses a feedback-augmented prompt (`build_retry_prompt`) that
preserves the original user prompt verbatim (including the v0.3
`---BEGIN/END USER CONTEXT <nonce>---` delimiters) and appends a
`---RETRY-FEEDBACK---` block describing the validation error and
restating the 7-key schema contract.

A new field `retried_agents: BTreeSet<AgentName>` is added to
`MagiReport`, populated for every agent whose first attempt failed
schema/parse, regardless of whether the retry succeeded. Composes with
`failed_agents` to derive cohorts downstream (retry recovered vs retry
also failed).

## Mechanic of the corrective prompt

The retry feedback block is placed AFTER the `---END USER CONTEXT---`
delimiter, not inside it. Rationale: anything inside the BEGIN/END
block is by contract untrusted user content. Putting the feedback there
would muddle the contract — an attacker controlling `content` could
already have placed a fake `---RETRY-FEEDBACK---` inside. Outside the
delimiters, the feedback is unambiguously a system-level message.

The feedback contains:
1. The literal `MagiError` Display output (controlled by the crate)
   passed through `sanitize_error_for_retry_feedback` (see below).
2. A restatement of the 7 required JSON keys.
3. Instructions to emit valid JSON without truncation or extra text.

The text is a near-verbatim port of Python's `_build_retry_prompt`
(`run_magi.py:360-396`) to keep the LLM's self-correction behavior
aligned across implementations.

## Mitigación de inyección de segundo orden (MAGI R1 C1 + R2 C1)

Although the `error` argument is the `Display` output of a `MagiError`
variant we control, both the MAGI R1 review and R2 review surfaced
distinct risk paths:

- **R1 C1/I5** — A parser/validator error string could theoretically
  echo fragments of the LLM's adversarial first output. If those
  fragments contain line-start tokens like `MODE:`, `CONTEXT:`,
  `---BEGIN`, or `---END`, the rendered feedback block would carry
  re-injectable structural tokens.
- **R2 C1** — Even after applying R1 C1's defense via `neutralize_headers`,
  the regex `(?m)^([\t ]*)(MODE|CONTEXT|---BEGIN|---END)(\s|:|$)`
  requires a separator after the keyword. The string `---RETRY-FEEDBACK---`
  ends with `---` (no separator), so the regex never matches it. An
  adversarial error containing `---RETRY-FEEDBACK---` could
  prematurely close the feedback envelope.

**Mitigation: two-layer sanitization** in
`sanitize_error_for_retry_feedback`:
1. `neutralize_headers(error)` — line-start tokens.
2. Literal substring replace `---RETRY-FEEDBACK---` → `  ---RETRY-FEEDBACK---`
   (anywhere in the string).

Both layers run on the error string before insertion into the feedback
block. The cost is one extra `replace` per retry — negligible.

## Why single-shot (no exponential backoff)

Python uses single-shot. Two retries doubles cost without strong
evidence of recovery — if the first retry with explicit feedback fails,
a second retry is unlikely to recover and probably indicates the model
or the context is broken. Failing fast preserves quota.

## Operator opt-out (MAGI R2 W6)

`MagiBuilder::with_retry_disabled()` lets latency-sensitive consumers
turn off the retry layer entirely. When disabled:
- `dispatch_one_agent` returns the first attempt's result directly even
  on `Validation`/`Deserialization` errors.
- `retried_agents` is always empty in the resulting report.
- Worst-case latency per agent matches v0.3.1 (single timeout budget).

Default: retry enabled.

## Telemetry contract

`retried_agents` is serialized to JSON only when non-empty
(`#[serde(skip_serializing_if = "BTreeSet::is_empty")]`). It is NOT
rendered in the markdown report — paridad with Python which keeps it
JSON-only.

## Interaction with v0.3 prompt-injection defense

The retry preserves the original user_prompt (with its sanitization,
delimiters, and nonce) verbatim. The retry does NOT re-call
`build_user_prompt`, so no new nonce is generated. This means a single
analyze() call produces at most one nonce per agent, even when the
retry runs. See BDD-13 for the test that locks this property.

## Alternatives considered

1. **Multiple retries with exponential backoff.** Rejected: cost
   doubles per retry; recovery rate beyond 1 retry is anecdotally
   low; Python parity weighs against.
2. **Retry on ALL `MagiError` variants.** Rejected: provider errors
   (HTTP 5xx, network, auth) are infrastructure-level and have their
   own retry layer (`RetryProvider`). Mixing them dilutes both layers.
3. **Recompute nonce + sanitization on retry.** Rejected: changing
   the user_prompt between attempts means the model gets a different
   context, making the corrective feedback less effective.
4. **Include the agent's first (bad) output in the feedback.** Rejected:
   the first output is adversarial — echoing it back to the model is a
   second-order injection vector if the bad output contains
   `---RETRY-FEEDBACK---` or similar. The feedback only carries the
   `MagiError` Display (crate-controlled), AND that Display is itself
   passed through `sanitize_error_for_retry_feedback` for defense in
   depth.

## Implementation references

- `src/user_prompt.rs::build_retry_prompt` — corrective prompt builder.
- `src/user_prompt.rs::sanitize_error_for_retry_feedback` — two-layer
  sanitization of the error string.
- `src/orchestrator.rs::dispatch_one_agent` — per-agent retry FSM.
- `src/orchestrator.rs::MagiBuilder::with_retry_disabled` — opt-out.
- `src/reporting.rs::MagiReport.retried_agents` — telemetry field.
- `sbtdd/spec-behavior.md` §7 BDD-03..BDD-08 + BDD-13..BDD-14 + BDD-17..BDD-19.
