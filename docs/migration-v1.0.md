# Migration guide: magi-core 0.6.x → 1.0.0

v1.0.0 is the first stable release under SemVer. From this point forward, any
break of the public contract requires `2.0.0`. This document covers every
consumer-visible change introduced in the 1.0 changeset.

## Summary of changes

1. **Structured findings** — `Finding` gains optional `file`, `line`, and
   `category` fields. Construction is now via `Finding::new(...).with_location(...).with_category(...)`.
2. **`#[non_exhaustive]`** — `Finding`, `AgentOutput`, `MagiReport`,
   `DedupFinding`, and `Category` are now non-exhaustive. Struct literals
   and exhaustive `match` arms no longer compile for these types.
3. **`finding_id` module** — new `magi_core::finding_id::generate_finding_id`
   computes a stable 16-hex dedup key from `(file, line, category)`.
4. **Visibility cleanup** — `ClaudeRequest` and `ClaudeMessage` dropped from
   `pub` to `pub(crate)`. They were never part of the analysis contract.
5. **No public constructors for output types** — `AgentOutput`, `DedupFinding`,
   and `MagiReport` are obtained from the API or deserialization, not constructed
   by consumers.

---

## 1. Structured findings

### What changed

`Finding` in `src/schema.rs` gains three optional fields:

| Field | Type | Source |
|-------|------|--------|
| `file` | `Option<String>` | Agent-reported; unverified |
| `line` | `Option<u32>` | Agent-reported; unverified |
| `category` | `Category` | Agent-reported; defaults to `Category::Other` |

`DedupFinding` in `src/consensus.rs` gains a matching `id` field:

| Field | Type | Source |
|-------|------|--------|
| `id` | `Option<String>` | SHA-256[:16] of `(file, line, category)` when all three present |
| `file` | `Option<String>` | Forwarded from the winning `Finding` |
| `line` | `Option<u32>` | Forwarded from the winning `Finding` |
| `category` | `Category` | Forwarded from the winning `Finding` |

### Backward-compatible deserialization

A 0.6.x JSON report (without `file`, `line`, `category`, or `id`) deserializes
correctly under v1.0.0:

```json
{
  "severity": "warning",
  "title": "SQL injection",
  "detail": "User input concatenated into query string."
}
```

Missing optional fields default to `None` or `Category::Other`. A `#[serde(default)]`
attribute on each optional field ensures this. Malformed optional values
(e.g., `"line": "not-a-number"`) degrade silently to `None`; they never
return a deserialization error (fail-soft, Python parity).

### Construction changes

`Finding` is now `#[non_exhaustive]`. **Struct literals no longer compile for
external crates.** Use the provided constructors instead:

```rust
// v0.6.x struct literal (no longer compiles)
let f = Finding { severity: Severity::Warning, title: "...".into(), detail: "...".into() };

// v1.0.0 constructor
use magi_core::prelude::*;

let f = Finding::new(Severity::Warning, "SQL injection", "User input concatenated into query.");

// With optional location
let f = Finding::new(Severity::Warning, "SQL injection", "User input concatenated.")
    .with_location("src/db.rs", 42)
    .with_category(Category::SecurityVulnerability);
```

### Warning: unverified locations

`Finding.file`, `Finding.line`, and `DedupFinding.id` are **agent-reported
and NOT verified** against any source artifact. A hallucinated `file`/`line`
yields a stable, authoritative-looking `id`.

**Consumers building code-review tooling MUST validate located findings
against their own diff (or source tree) before trusting them.** See
[ADR 004](adr/004-diff-grounded-finding-validation-is-consumer-concern.md)
for the rationale and a description of the recommended guard pattern.

---

## 2. `#[non_exhaustive]` on extensible types

The following types are now `#[non_exhaustive]`:

| Type | Effect |
|------|--------|
| `Finding` | Struct literals forbidden; use `Finding::new(...)` |
| `AgentOutput` | Struct literals forbidden; obtained via API output or deserialization |
| `MagiReport` | Struct literals forbidden; obtained via `Magi::analyze` or deserialization |
| `DedupFinding` | Struct literals forbidden; obtained from `ConsensusResult.findings` |
| `Category` | Exhaustive enum matching requires a `_ =>` arm |

**Closed enums are unchanged.** `Verdict`, `Severity`, `AgentName`, and `Mode`
are NOT `#[non_exhaustive]` and remain exhaustively matchable without a
catch-all arm.

### Matching `Category`

```rust
// v1.0.0 — Category is non_exhaustive
match finding.category {
    Category::SecurityVulnerability => { /* ... */ }
    Category::LogicError           => { /* ... */ }
    Category::PerformanceIssue     => { /* ... */ }
    Category::MaintainabilityIssue => { /* ... */ }
    Category::Other                => { /* ... */ }
    _ => { /* future slugs */ }   // required
}

// Closed enums (unchanged) — no catch-all needed
match report.consensus.verdict {
    Verdict::Approve   => { /* ... */ }
    Verdict::Reject    => { /* ... */ }
    Verdict::Hold      => { /* ... */ }
}
```

---

## 3. No public constructors for output types

`AgentOutput`, `DedupFinding`, and `MagiReport` do not expose public
constructors. They are obtained either as outputs of the library API or
through deserialization. This is intentional (see
[ADR 005 §2](adr/005-api-stability-1.0.md)) — constructing these types
externally would couple consumers to internal structure that may gain fields
in future minor releases.

If you need to build a `MagiReport` for testing, deserialize from JSON:

```rust
let json = include_str!("fixtures/sample_report.json");
let report: magi_core::MagiReport = serde_json::from_str(json).unwrap();
```

---

## 4. `finding_id` module

New module `magi_core::finding_id` exposes one function:

```rust
pub fn generate_finding_id(file: &str, line: u32, category: Category) -> String
```

Returns the first 16 hex characters of `SHA-256("{file}:{line}:{category_slug}")`.
This is a **dedup key** with Python parity (MAGI@62cf5801 `finding_id.py`),
not a cross-system primary key. Use it to deduplicate findings across multiple
analysis runs on the same codebase, or to correlate with Python MAGI output.

```rust
use magi_core::finding_id::generate_finding_id;
use magi_core::schema::Category;

let id = generate_finding_id("src/db.rs", 42, Category::SecurityVulnerability);
assert_eq!(id.len(), 16);  // always 16 hex chars
```

Collision probability within a single report (tens of findings) is negligible.
Do not treat this id as a stable database primary key across codebases or
schema versions.

---

## 5. Visibility: `ClaudeRequest` and `ClaudeMessage`

`ClaudeRequest` and `ClaudeMessage` in `src/providers/claude.rs` were `pub`
in v0.6.x but were never re-exported from `magi_core::prelude` and were not
part of the analysis contract. They are now `pub(crate)`.

**Consumer impact:** none, unless you accessed these types via the full path
`magi_core::providers::claude::ClaudeRequest`. If you did, remove that usage;
the types are HTTP request-shaping plumbing that consumers should not rely on.

---

## 6. SemVer policy for the 1.x line

From 1.0.0 onward:

- **Minor releases (1.1, 1.2, …):** additive only. New feature-gated providers
  (`GeminiProvider` in 1.1, `OpenAIProvider` in 1.2), new optional fields on
  `#[non_exhaustive]` types, new `Category` slugs, new builder methods.
  Your existing code keeps compiling.
- **`2.0.0`:** reserved for the next break of the 1.0 contract (e.g., changing
  `LlmProvider::complete`'s signature, removing a public item, breaking
  `non_exhaustive` guarantees). Adding a provider does not require a major bump.

See [ADR 005](adr/005-api-stability-1.0.md) for the full policy.

---

## Consumer action checklist

- [ ] **Replace struct literals for `Finding`** with `Finding::new(...).with_location(...).with_category(...)`.
      The compiler will flag them with `error[E0639]: cannot create non-exhaustive struct using struct expression`.
- [ ] **Add `_ =>` arms to `Category` matches.** The compiler will flag
      exhaustive matches with `error[E0004]: non-exhaustive patterns`.
- [ ] **No changes needed for `Verdict`, `Severity`, `AgentName`, `Mode` matches.**
      These enums remain exhaustively matchable.
- [ ] **No changes needed for `MagiBuilder` or `Magi::analyze` call sites.**
      The builder API is unchanged.
- [ ] **Optional:** Inspect `DedupFinding.id`, `DedupFinding.file`, `DedupFinding.line`,
      `DedupFinding.category` on findings you render in a code-review tool.
      If you display or act on locations, validate them against your diff first
      (see ADR 004).
- [ ] **Optional:** Use `magi_core::finding_id::generate_finding_id` to
      stable-id findings for cross-run deduplication.

---

## Verification

```bash
cargo nextest run --features test-utils
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check
cargo build --release
cargo doc --no-deps
cargo audit
```

All commands must pass clean.
