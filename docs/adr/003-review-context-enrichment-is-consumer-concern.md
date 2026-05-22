# ADR 003: Review-Context Enrichment Is a Consumer Concern

**Status:** Accepted
**Date:** 2026-05-22
**Related:** Python MAGI v2.5.0 (`skills/magi/scripts/review_context.py`), `docs/adr/002-retry-on-schema-error.md`, v0.6.0 parser hardening

## Context

Python MAGI v2.5.0 added **code-review context enrichment**: a new module
`review_context.py` (538 lines) wired into the runner `run_magi.py`. When
analysis mode is `code-review`, the runner:

1. Locates the git repo root and requires a clean working tree (so all
   reads come from one coherent source == HEAD).
2. Obtains the diff (the input itself if it already looks like a diff,
   otherwise `git diff <base>...HEAD`).
3. Extracts the touched files and reads their full content from the
   working tree, behind guards: path-traversal containment (`realpath`
   inside repo root), binary/NUL skip, per-file size cap, file-count cap.
4. Verifies coherence (every added diff line must appear in the file).
5. Resolves cross-file symbol definitions for identifiers in the added
   lines via a single batched `git grep` for `def`/`class`, bounded by
   total and per-name caps, returning fixed-size excerpt windows.
6. Assembles an enriched bundle within a hard character budget (input
   always kept; touched files smallest-first; symbol defs dropped first
   when over budget).
7. Is fail-safe: enrichment never raises into the orchestrator — any error
   yields the original content unchanged with an explanatory note.

The question for `magi-core` (the Rust crate) was whether to port this.

## Decision

**`magi-core` will NOT include review-context enrichment in the library.**
Assembling the content that gets analyzed — including any git/filesystem
context gathering — is the **responsibility of the consumer**, not of this
crate.

This is a scope boundary, not a deferral of effort. The decision holds
regardless of implementation cost.

## Rationale

1. **The library contract is content-in, analysis-out.** `Magi::analyze(mode,
   content)` takes a caller-supplied string and returns a multi-perspective
   verdict. *How* that string is assembled — a raw diff, an enriched bundle
   of touched files plus referenced symbols, or anything else — is outside
   the contract. Enrichment is a transformation of the *input the consumer
   chooses to pass*, so it belongs in the consumer.

2. **The core is deliberately pure and side-effect-free.** `magi-core` does
   no filesystem or git I/O and is language-agnostic. Adding enrichment to
   the core would force git subprocess execution, arbitrary file reads, and
   their security surface (path traversal, untrusted-repo reads) onto every
   downstream embedder — even those that never review code. That contradicts
   the library's design.

3. **Faithful parity keeps it out of the crate.** In Python the enrichment
   lives in the *runner* (`run_magi.py`), not in the analysis core. The Rust
   analog of that runner is the consumer's application (or, for demonstration
   only, `examples/basic_analysis.rs`) — not the crate API. Mirroring Python
   faithfully therefore means keeping enrichment out of the library.

4. **The enrichment value is realized by a review tool, not by a library or
   its demo.** Richer agent context improves real reviews run through a tool
   like Python's `/magi:magi` skill. `magi-core` ships a library plus a
   teaching example; neither is a review workflow anyone runs against a real
   repository, so embedding enrichment there delivers little while obscuring
   the example's purpose (demonstrating the API).

5. **The symbol-resolution half is language-specific anyway.** Python's
   resolver keys off Python syntax (`def`/`class`, `keyword.kwlist`,
   `keyword.softkwlist`, Python builtins). A faithful port to a
   language-agnostic crate is impossible without committing the crate to a
   particular language's grammar — another reason this concern sits above the
   library, where the consumer knows the language under review.

## Consequences

- The library stays pure: no git/filesystem dependency, no subprocess, no
  path-traversal or untrusted-read surface added to the core or to any
  consumer that does not opt in.
- Consumers building a code-review tool on `magi-core` assemble their own
  review context (diff + files + symbols) and pass it as `content`. This is
  the same division of labor Python uses between `run_magi.py` (gathers
  context) and the analysis core.
- Users who want enriched MAGI reviews of a repository today already have
  them via the Python `/magi:magi` plugin, which performs this enrichment in
  its runner. The Rust crate is the embeddable analysis engine, not a daily
  review tool.

## Alternatives considered

1. **Port into `examples/basic_analysis.rs`.** Rejected. The example exists
   to teach the API; ~300 lines of git/filesystem/symbol-resolution
   infrastructure would more than double it and bury that lesson, while
   delivering no real value (a demo is not a review workflow). It would also
   place security-sensitive code (subprocess, arbitrary file reads,
   path-traversal guards) in demo-grade code, and the logic would be
   example-local and non-reusable.

2. **Feature-gated module (e.g. `review-context`, off by default).** Not
   adopted now, but this is the **only acceptable way to expose enrichment
   from the crate** if a concrete downstream consumer ever requests it. It
   keeps default builds pure (like `claude-api` / `claude-cli`), makes the
   logic a properly testable and reusable module, and confines the git/fs
   surface behind an explicit opt-in. Revisit under SBTDD (`/brainstorming`
   first — this has real design space) if such a consumer appears.

3. **Add to the library core.** Rejected outright — see Rationale §2.

## Implementation references

- Python source not ported: `skills/magi/scripts/review_context.py`,
  `run_magi.py::_maybe_enrich` (Python MAGI v2.5.0).
- Library boundary upheld: `src/orchestrator.rs::Magi::analyze` (content-in,
  analysis-out); no `std::fs` / `std::process` usage in the analysis core.
- Consumer-side analog for demonstration only: `examples/basic_analysis.rs`.
