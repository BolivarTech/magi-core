# ADR 004: Diff-Grounded Finding Validation Is a Consumer Concern

**Status:** Accepted
**Date:** 2026-05-24
**Related:** [[ADR 003]] (`docs/adr/003-review-context-enrichment-is-consumer-concern.md`), Python MAGI v3.0.0 (`skills/magi/scripts/finding_validation.py`), magi-core v1.0.0 structured findings (`Finding::file/line/category`, `finding_id`)

## Context

Python MAGI v3.0.0 added a **diff-grounded hallucination guard**: a new module
`finding_validation.py` ("Diff-grounded validation of MAGI findings —
code-review only. Pure stdlib and total — never raises into the orchestrator.").
When analysis mode is `code-review`, after the agents return findings, the runner
(`run_magi.py`) validates each *located* finding against the actual diff:

1. Walks the unified diff once (`_iter_diff_events`) — the single source of truth
   for `extract_touched_files`, `added_lines_by_file`, and `parse_diff_ranges`.
2. Drops findings whose `file` is not among the diff's touched files, or whose
   `line` does not fall within a changed range (with `LINE_RANGE_MARGIN = 3` to
   absorb LLM line-counting fuzz).
3. Annotates survivors and surfaces the dropped titles in the report summary.

The guard's purpose is to stop a fabricated finding (one that cites a file/line
the change never touched) from entering consensus with authoritative weight.

The question for `magi-core` (the Rust crate) was whether to port this guard.

## Decision

**`magi-core` will NOT include the diff-grounded finding guard in the library.**
Validating findings against the reviewed diff is the **responsibility of the
consumer**, not of this crate.

This is a scope boundary, not a deferral of effort. The decision holds
regardless of implementation cost.

## Rationale

1. **It extends [[ADR 003]].** The library contract is *content-in,
   analysis-out*: `Magi::analyze(mode, content)` takes a caller-supplied string
   and returns a multi-perspective verdict. The guard requires knowing that the
   `content` *is a unified diff* and parsing its structure (touched files, added
   line ranges). The language-agnostic core does not and must not assume that.

2. **The core is deliberately pure and side-effect-free.** Porting the guard
   would pull diff-parsing (and, in the full Python flow, the surrounding git/fs
   context) into a crate that does no I/O. Same purity argument as ADR 003 §2.

3. **The guard is diff- and language-aware; the core is neither.** Which lines a
   change "touched" is a property of the diff format, not of the analysis engine.
   The consumer that produced the diff is the layer that can validate against it.

4. **What the core DOES provide (enabling the guard consumer-side).** v1.0.0 adds
   structured, optional `file`/`line`/`category` fields to `Finding` and a stable
   `finding_id` (`finding_id::generate_finding_id`). These are **data**: they let
   a consumer implement the guard without re-deriving the schema or recomputing
   identity. The core carries the structured location; the consumer validates it.

## Risk: unverified locations carry authoritative ids

Because the guard is excluded, the core computes `finding_id` from the
**agent-reported** `file`/`line` **without validating them against any source**.
A hallucinated `file`/`line` is carried verbatim and yields a stable,
authoritative-looking 16-hex id. This is a deliberate consequence of the scope
boundary, and it is a **trap for downstream consumers** if left implicit.

Mitigation (made visible at the API surface, not only here):

- `Finding::file`, `Finding::line`, and the deduped `id` carry doc-comments
  stating they are **agent-reported and unverified**; a consumer must run a diff
  guard before trusting them.
- `docs/migration-v1.0.md` documents the same in its "structured findings"
  section.
- Consumers building a code-review tool on `magi-core` are expected to validate
  located findings against their own diff (the very guard described above).

## Consequences

- The library stays pure: no diff-parsing, no git/fs, no new surface in the core
  or in consumers that do not opt in.
- Consumers building a code-review tool implement the guard themselves, fed by
  the structured `file`/`line`/`category` + `finding_id` the core now provides.
- Users who want guarded MAGI reviews of a repository today have them via the
  Python `/magi:magi` plugin, whose runner performs this validation.

## Alternatives considered

1. **Port into `examples/basic_analysis.rs`.** Rejected — same reasoning as
   ADR 003: the example teaches the API; diff-parsing infrastructure would bury
   that lesson and place diff/string-handling logic in demo-grade code.

2. **Feature-gated module (e.g. `finding-guard`, off by default).** Not adopted
   now, but this is the **only acceptable way to expose the guard from the
   crate** if a concrete downstream consumer requests it. It keeps default
   builds pure, makes the logic a testable/reusable module, and confines the
   diff-parsing behind an explicit opt-in. Revisit under SBTDD (`/brainstorming`
   first) if such a consumer appears. Parallel to ADR 003 alternative #2.

3. **Add to the library core.** Rejected outright — see Rationale §2.

## Implementation references

- Python source not ported: `skills/magi/scripts/finding_validation.py`,
  `run_magi.py::_validate_findings` wiring (Python MAGI v3.0.0, `MAGI@62cf5801`).
- Core provides (the data half): `src/schema.rs::Finding::{file,line,category}`,
  `src/finding_id.rs::generate_finding_id`.
- Library boundary upheld: `src/orchestrator.rs::Magi::analyze` (content-in,
  analysis-out); no `std::fs` / `std::process` / diff-parsing in the core.
