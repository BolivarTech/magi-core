# ADR 005: API Stability for the 1.0 Release

**Status:** Accepted
**Date:** 2026-05-24
**Related:** magi-core v1.0.0 release, [[ADR 004]], structured findings (`Finding`, `Category`, `DedupFinding`)

## Context

v1.0.0 is the point at which `magi-core` commits to a stable public API under
SemVer: from 1.0 onward, any break of the public contract requires a `2.0`.

The 1.0 changeset itself contains the only *breaking* change in the plan — adding
the optional `file`/`line`/`category` fields to `Finding` (and `file`/`line`/
`category`/`id` to `DedupFinding`). Adding a field to a struct with public fields
breaks struct-literal construction and exhaustive pattern matching for external
crates. The cleanest place to absorb that break is the 1.0 baseline itself: there
is no frozen contract *before* 1.0 to break.

This ADR records the decisions that make the 1.0 API extensible without forcing a
premature `2.0`.

## Decision

### 1. `#[non_exhaustive]` on extensible types only

Applied to the structs that may gain fields and the one open enum:

- `Finding`, `AgentOutput`, `MagiReport`, `DedupFinding` (structs that may gain
  fields in future minors).
- `Category` (an open vocabulary that may gain slugs).

**Not** applied to the closed enums — `Verdict`, `Severity`, `AgentName`,
`Mode`. These are small, conceptually closed sets that are not expected to grow,
and consumers legitimately want to `match` them **exhaustively** (without a
forced `_ =>` arm). Marking them `non_exhaustive` would impose friction with no
benefit.

`#[non_exhaustive]` **must** ship in 1.0 — adding it after 1.0 is itself a
breaking change.

### 2. Construction policy (resolves RF-E2)

- **`Finding`** exposes a constructor: `Finding::new(severity, title, detail)`
  plus the chainable `with_location(file, line)` and `with_category(category)`.
  Required because `non_exhaustive` forbids external struct-literal construction.

- **`AgentOutput`, `DedupFinding`, `MagiReport`** do **not** expose a public
  constructor. They are obtained by **deserialization** (agent JSON, persisted
  reports) or as **outputs of the API** (`ConsensusEngine` / `Magi::analyze`),
  never constructed by a consumer. If a concrete future consumer needs to build
  one, a constructor is added then — an additive (minor) change.

### 3. Visibility cleanup

`ClaudeRequest` and `ClaudeMessage` drop from `pub` to `pub(crate)`: they are
HTTP request-shaping plumbing, not part of the analysis contract. (Backlog item
from the v0.2 API review.)

### 4. SemVer policy for the 1.x line

- New providers — `GeminiProvider` (v1.1), `OpenAIProvider` (v1.2) — are
  **additive**, feature-gated, and ship as **minor** releases.
- `2.0.0` is **reserved** for the next break of the 1.0 contract (e.g. changing
  `LlmProvider`'s signature, removing a public item). Adding a provider does not
  consume the major.

## Notes

- **`finding_id` width.** Identity is `SHA-256(...)[..16]` (64 bits), for parity
  with Python (`finding_id.py`). This is **not** a cross-system primary key; it
  is a dedup key whose collision space is ample for the intra-report population
  (tens of findings). Widening it later would diverge from Python and is out of
  scope.

- **Fail-soft erases malformed-field signal.** The deserialize-time fail-soft for
  `file`/`line`/`category` silently degrades a malformed optional value to
  `None`/`Other` (parity with Python `validate.py`). No telemetry is emitted for
  the degradation. This is acceptable because the fields are optional and, per
  [[ADR 004]], **unverified** regardless; surfacing per-field parse telemetry is
  a consumer concern.

## Consequences

- External consumers construct `Finding` via `new().with_*`; they cannot use
  struct literals on the `non_exhaustive` types, and cannot `match` them
  exhaustively (must use `_` or field access).
- The closed enums (`Verdict`, `Severity`, `AgentName`, `Mode`) remain
  exhaustively matchable — existing consumer `match`es keep compiling.
- Future field/variant additions to the `non_exhaustive` types, and future
  providers, are **minor** releases. The crate can evolve through the 1.x line
  without a `2.0` until the contract itself is broken.

## Implementation references

- `src/schema.rs` — `#[non_exhaustive]` on `Finding`/`AgentOutput`/`Category`;
  `Finding::new`/`with_location`/`with_category`.
- `src/consensus.rs` — `#[non_exhaustive]` on `DedupFinding`.
- `src/reporting.rs` — `#[non_exhaustive]` on `MagiReport`.
- `src/provider.rs` / `src/providers/claude.rs` — `pub(crate)` on
  `ClaudeRequest`/`ClaudeMessage`.
- `docs/migration-v1.0.md` — consumer-facing migration guide.
