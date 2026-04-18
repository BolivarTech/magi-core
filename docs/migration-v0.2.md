# Migration Guide: magi-core 0.1.x → 0.2.0

This release adopts algorithmic and output-format equivalence with the Python
MAGI v2.1.3 reference implementation. All breaking changes are listed below
with recommended migration actions.

## Dependencies

Two new dependencies have been added. Both are small, without `unsafe`, and
pinned with tilde constraints:

- `unicode-normalization = "~0.1.24"`
- `caseless = "~0.2.2"`

No action required from consumers — they are transitive through `magi-core`.

## Opus model alias

The `opus` short name now resolves to `claude-opus-4-7` (was `claude-opus-4-6`).

If you hard-coded `"claude-opus-4-6"` in your integration, update to
`"claude-opus-4-7"` or use the short name `"opus"` and let the library resolve.

## `Finding::stripped_title` → `validate::clean_title`

The method is now deprecated. Callers should switch to the free function
`magi_core::validate::clean_title(&finding.title)`, which applies a stricter
cleaning pipeline (control-whitespace normalization, invisible-character
removal, trim).

**Note:** `clean_title` is a strict superset of the old `stripped_title`
behavior for zero-width stripping, plus it also collapses control whitespace
to spaces and trims edges. If your code relied on the partial behavior,
review carefully.

The deprecated method will be removed in v0.3.0.

## `Condition.condition` now sourced from `summary`

In `ConsensusResult.conditions[].condition`, the text is now the conditional
agent's `summary` field (a short one-liner) instead of the `recommendation`
field (the full suggested action).

**Why:** conditions are meant as blocking one-liners; recommendations stay in
`recommendations: Map<AgentName, String>` for full context.

**Action:** if you rendered `Condition.condition` as a long action, it will
now be a short summary. Your UI may need layout adjustments. If you need the
full action, read from `recommendations[&agent]` instead.

## Report markdown: `## Consensus Summary` removed

The section between the banner and `## Key Findings` no longer appears.
Consumers parsing markdown should read `consensus.majority_summary` from the
JSON output instead.

## Report markdown: dissent one-line, no reasoning paragraph

`## Dissenting Opinion` now emits one line per dissenter with `summary` only.
The `reasoning` field remains in JSON; update markdown parsers accordingly.

## Report markdown: findings one-line, no detail paragraph

`## Key Findings` now renders one line per finding with fixed-width marker
and severity columns. The `detail` field is no longer rendered in markdown
but remains in JSON.

## `GO WITH CAVEATS` now includes split count

The consensus label `"GO WITH CAVEATS"` is now `"GO WITH CAVEATS (N-M)"`
where N = approve+conditional count and M = reject count.

## `majority_summary` prefixed with agent display name

Old: `"Approve summary | Another summary"`
New: `"Melchior: Approve summary | Balthasar: Another summary"`

## Dedup no longer collapses interior whitespace

Findings with titles differing only in interior spacing are now treated as
distinct. Agents that produce inconsistent whitespace in titles may see
duplicates where they didn't before. Consider normalizing titles upstream
or rely on the `clean_title` pipeline.

## `max_input_len` default raised to 4 MB

Consumers exposing the library to untrusted input should explicitly lower
the limit via `MagiBuilder::with_max_input_len(1_048_576)`. The default
(4 MB) is a compromise between Python's 10 MB and the previous 1 MB.

A full 10 MB alignment with Python is deferred to v0.3.0.

## New: `ReportConfig::new_checked`

If you construct `ReportConfig` with custom `agent_titles`, prefer
`ReportConfig::new_checked(52, agent_titles)?` which validates ASCII. The
infallible `ReportConfig::default()` is unchanged.

## What's not changing in v0.2.0 (v0.3.0 roadmap)

- **Prompt architecture**: the 9 mode-specific prompt files and the 3-arg
  `MagiBuilder::with_custom_prompt(AgentName, Mode, String)` API are
  unchanged in v0.2.0. They will be reorganized in v0.3.0 along with
  prompt-injection hardening.

## Upgrade checklist

1. Update your `Cargo.toml`: `magi-core = "0.2"`.
2. Check for direct use of `Finding::stripped_title` — replace with
   `clean_title`.
3. Review your markdown rendering pipeline for dropped `## Consensus Summary`.
4. Review `Condition.condition` consumers for the summary/recommendation shift.
5. If exposing to untrusted input, explicitly set `max_input_len`.
6. Run your test suite.
