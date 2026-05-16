# magi-core v0.4.0 — Python-Parity Gap Closure: TDD Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Follow CLAUDE.local.md §3 TDD discipline: every task is Red→Green→Refactor with `/verification-before-completion` between phases.

**Goal:** Cerrar 5 gaps de paridad con MAGI Python v2.2.8: bump de prompts, per-mode default model, single-shot retry on schema errors, `retried_agents` telemetry, Windows console UTF-8 hardening.

**Architecture:** Cambios aditivos sobre la base v0.3.1. Sin nuevas deps. Sin breaking API changes (campo nuevo `retried_agents` es skip-if-empty + default-on-deserialize). El retry layer es inline en `orchestrator.rs`; el helper `build_retry_prompt` se aloja en `user_prompt.rs` para cohesión semántica.

**Tech Stack:** Rust 1.91 (MSRV), tokio (async runtime), serde (JSON), regex, fastrand (existing). Test runner: cargo nextest. No new deps.

**Spec reference:** `sbtdd/spec-behavior.md` v1.0 — decisiones autónomas marcadas como **[D-N]** allí.

**Branch sugerida:** `v0_4_0` (a crear desde `main` post-aprobación).

**Target test count:** 359 (v0.3.1) + ~37 → ~396.

---

## Task ordering and dependencies

```
T00 (ADR + Migration doc — no code)
  ↓
T01 (Prompts v2.2.8)  ← independiente
T02 (default_model)   ← independiente
T03 (build_retry_prompt)  ← independiente
T04 (retried_agents field)  ← independiente
T05 (RoutingMockProvider)  ← independiente, test helper
  ↓
T06 (parse_and_validate + DispatchOutcome)
  ↓
T07 (dispatch_one_agent)  ← usa T03, T06
  ↓
T08 (wire into Magi::analyze)  ← usa T04, T05, T07
  ↓
T09 (basic_analysis: Windows hardening)  ← independiente del stack retry
T10 (basic_analysis: default model usage)  ← usa T02
  ↓
T11 (CHANGELOG + Cargo.toml bump a 0.4.0)
```

T01–T05 son paralelizables. T06–T08 son secuenciales (mismo archivo). T09–T10 son del example, no afectan src/. T11 cierra.

---

## Task T00 — ADR + Migration doc (pre-Red mandatorio)

**Files:**
- Create: `docs/adr/002-retry-on-schema-error.md`
- Create: `docs/migration-v0.4.md`

Este task **no ejecuta TDD** — es documentación de diseño previa al primer commit Red, mandatoria per spec §11.

- [ ] **T00.1: Crear ADR 002**

Contenido obligatorio (port directo del bloque preparado en la spec §11; sin placeholders):

```markdown
# ADR 002: Single-Shot Retry on Schema/Parse Errors

**Status:** Accepted
**Date:** 2026-05-15
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
1. The literal `MagiError` Display output (controlled by the crate).
2. A restatement of the 7 required JSON keys.
3. Instructions to emit valid JSON without truncation or extra text.

The text is a near-verbatim port of Python's `_build_retry_prompt`
(`run_magi.py:360-396`) to keep the LLM's self-correction behavior
aligned across implementations.

## Why single-shot (no exponential backoff)

Python uses single-shot. Two retries doubles cost without strong
evidence of recovery — if the first retry with explicit feedback fails,
a second retry is unlikely to recover and probably indicates the model
or the context is broken. Failing fast preserves quota.

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
   `MagiError` Display (crate-controlled).

## Implementation references

- `src/user_prompt.rs::build_retry_prompt` — corrective prompt builder.
- `src/orchestrator.rs::dispatch_one_agent` — per-agent retry FSM.
- `src/reporting.rs::MagiReport.retried_agents` — telemetry field.
- `sbtdd/spec-behavior.md` §7 BDD-03..BDD-08 + BDD-13..BDD-14.
```

- [ ] **T00.2: Crear `docs/migration-v0.4.md`** (texto preparado en spec §12 + nota de testing):

```markdown
# Migration guide: magi-core 0.3.x → 0.4.0

## Summary

v0.4.0 closes 5 gaps with the Python MAGI reference (v2.2.8):

1. Embedded agent prompts updated from MAGI v2.1.3 to v2.2.8 (adds
   explicit "7 keys required" enforcement).
2. New helper `default_model_for_mode(Mode) -> &'static str` in
   `provider.rs`.
3. Single-shot retry on schema/parse errors (transparent to API consumers).
4. New `retried_agents` field in `MagiReport` (skip-if-empty in JSON,
   default empty on deserialize — backward-compatible).
5. Windows console UTF-8 hardening in the `basic_analysis` example.

## API compatibility

- **Additive only.** No removed APIs, no renamed methods, no signature
  changes.
- `MagiReport` now derives `Deserialize` in addition to `Serialize` to
  support `#[serde(default)]` on the new field. v0.3.x JSON deserializes
  to v0.4.0 `MagiReport` without errors; the new field defaults to empty.

## Behavior changes

- **More resilient analyses.** Agents whose first response fails schema
  validation (e.g., missing one of the 7 required keys) are retried once
  with a corrective prompt. Previously these agents would go straight
  to `failed_agents` and (in the 3-agent case) trigger degraded mode.
- **New telemetry.** When a retry occurs, the agent name appears in
  `report.retried_agents` regardless of whether the retry succeeded. If
  the retry also fails, the agent ALSO appears in `failed_agents` with
  reason prefix `"retry-failed: "`.

## Consumer action items

- **None required for backward compatibility.** Existing v0.3.x callers
  continue to work without changes.
- **Optional:** Inspect `report.retried_agents` for resilience metrics
  (e.g., dashboard "% of analyses that needed retry").
- **Optional:** Use `default_model_for_mode` instead of hardcoding model
  strings:
  ```rust
  let alias = magi_core::default_model_for_mode(Mode::Analysis);
  let model_id = magi_core::resolve_claude_alias(alias);
  ```
- **Windows operators:** the `basic_analysis` example now sets the
  console output codepage to UTF-8 at startup; em-dash and other
  multibyte chars in the report no longer panic on cp1252 consoles.

## Test count

`cargo nextest run` test count moves from ~359 (v0.3.1) to ~396 (v0.4.0).
```

- [ ] **T00.3: Commit ADR + migration doc**

```bash
git add docs/adr/002-retry-on-schema-error.md docs/migration-v0.4.md
git commit -m "docs: add ADR 002 + v0.4 migration guide"
```

Expected: commit success, working tree clean.

---

## Task T01 — Bump prompts to MAGI@v2.2.8

**Files:**
- Modify: `src/prompts_md/melchior.md` (regenerate from MAGI@645932c7)
- Modify: `src/prompts_md/balthasar.md` (regenerate from MAGI@645932c7)
- Modify: `src/prompts_md/caspar.md` (regenerate from MAGI@645932c7)
- Modify: `tests/fixtures/magi_ref_prompts.sha256` (new SHAs + header)
- Modify: `tests/fixtures/gen_magi_ref_prompts.py` (`MAGI_REF_SHA` constant)

- [ ] **T01.1 (Red): Update fixture to v2.2.8 expected SHAs FIRST**

Edit `tests/fixtures/gen_magi_ref_prompts.py`: change `MAGI_REF_SHA` constant to `"645932c78da5327a0deee01f38b90849cda37d18"`, then run:

```bash
python tests/fixtures/gen_magi_ref_prompts.py
```

Expected: `tests/fixtures/magi_ref_prompts.sha256` is overwritten with v2.2.8 hashes + new header `# Generated from MAGI@645932c7... on YYYY-MM-DD`.

- [ ] **T01.2 (Red): Run fixture test, expect FAIL**

```bash
cargo nextest run test_prompts_match_python_reference_sha256
```

Expected: **FAIL** — the embedded prompts are still v2.1.3 bytes but the fixture now expects v2.2.8 hashes. This is the Red state.

- [ ] **T01.3 (Green): Replace embedded prompts with v2.2.8 content**

Run from Bash (cross-platform Python script to extract + write bytes literally):

```bash
python -c "
import subprocess
from pathlib import Path
SHA = '645932c78da5327a0deee01f38b90849cda37d18'
PY = Path(r'D:/jbolivarg/PythonProjects/MAGI')
OUT = Path(r'D:/jbolivarg/RustProjects/MAGI-Core/src/prompts_md')
for agent in ('melchior', 'balthasar', 'caspar'):
    src = f'{SHA}:skills/magi/agents/{agent}.md'
    data = subprocess.check_output(['git', '-C', str(PY), 'show', src])
    # Force LF line endings (mismo invariante del fixture v0.3)
    data = data.replace(b'\r\n', b'\n')
    (OUT / f'{agent}.md').write_bytes(data)
    print(f'wrote {agent}.md ({len(data)} bytes)')
"
```

Expected output: 3 lines `wrote X.md (NNNN bytes)` con tamaños ~4158/4249/4749 bytes.

- [ ] **T01.4 (Green): Verify fixture test now passes**

```bash
cargo nextest run test_prompts_match_python_reference_sha256
```

Expected: **PASS**.

- [ ] **T01.5 (Green): Verify full suite green**

```bash
cargo clippy --tests -- -D warnings
cargo nextest run
cargo fmt --check
```

Expected: zero warnings, all tests pass, no fmt diffs.

- [ ] **T01.6 (Refactor): Add doc-comment to MAGI_REF_SHA**

En `tests/fixtures/gen_magi_ref_prompts.py`, aumentar el docstring del constant:

```python
# Pin to MAGI@v2.2.8 (commit 645932c7). The v2.1.4 prompt update added
# explicit "must contain all seven top-level keys exactly" enforcement
# to each agent prompt. See docs/migration-v0.4.md for rationale.
MAGI_REF_SHA = "645932c78da5327a0deee01f38b90849cda37d18"
```

- [ ] **T01.7 (Commits): Uno por fase**

```bash
# Red — fixture updated, test failing as expected
git add tests/fixtures/
git commit -m "test: pin prompt fixture to MAGI@v2.2.8 SHA"
```

```bash
# Green — prompts regenerated, test passing
git add src/prompts_md/
git commit -m "feat: bump embedded prompts to MAGI@v2.2.8"
```

```bash
# Refactor — generator docstring polish
git add tests/fixtures/gen_magi_ref_prompts.py
git commit -m "refactor: document MAGI_REF_SHA pin rationale"
```

Tras cada commit ejecutar `/verification-before-completion` y actualizar `.claude/session-state.json` per §2.3.

---

## Task T02 — `default_model_for_mode` in `provider.rs`

**Files:**
- Modify: `src/provider.rs` (add public function + tests)
- Modify: `src/lib.rs` (re-export)

- [ ] **T02.1 (Red): Write the failing tests**

Append to `src/provider.rs` inside `#[cfg(test)] mod tests`:

```rust
#[test]
fn test_default_model_for_mode_code_review_is_opus() {
    assert_eq!(default_model_for_mode(Mode::CodeReview), "opus");
}

#[test]
fn test_default_model_for_mode_design_is_opus() {
    assert_eq!(default_model_for_mode(Mode::Design), "opus");
}

#[test]
fn test_default_model_for_mode_analysis_is_opus() {
    assert_eq!(default_model_for_mode(Mode::Analysis), "opus");
}
```

- [ ] **T02.2 (Red): Verify FAIL**

```bash
cargo nextest run test_default_model_for_mode
```

Expected: **FAIL** con compile error `cannot find function default_model_for_mode in this scope`.

- [ ] **T02.3 (Green): Implement the function**

Agregar a `src/provider.rs` después de `resolve_claude_alias`:

```rust
/// Resolves the default model short-name (`"opus"`, `"sonnet"`, `"haiku"`)
/// recommended for the given analysis mode.
///
/// Mirrors Python's `MODE_DEFAULT_MODELS` (MAGI@v2.2.8 `models.py:58-62`).
/// As of v0.4.0 all three modes default to `"opus"` per Python parity.
/// Pair with [`resolve_claude_alias`] to obtain the full model id:
///
/// ```
/// use magi_core::{Mode, default_model_for_mode, resolve_claude_alias};
/// let alias = default_model_for_mode(Mode::Analysis);
/// let model_id = resolve_claude_alias(alias);
/// assert_eq!(model_id, "claude-opus-4-7");
/// ```
///
/// # Arguments
///
/// * `mode` — The analysis mode whose default model alias to return.
///
/// # Returns
///
/// The short alias name (always `"opus"` in v0.4.0).
pub fn default_model_for_mode(mode: Mode) -> &'static str {
    match mode {
        Mode::CodeReview => "opus",
        Mode::Design => "opus",
        Mode::Analysis => "opus",
    }
}
```

- [ ] **T02.4 (Green): Re-export from `lib.rs` si aplica**

Revisar `src/lib.rs` por `pub use` de provider items. Si `resolve_claude_alias` está re-exportado, agregar `default_model_for_mode` al mismo grupo:

```rust
pub use provider::{
    CompletionConfig, LlmProvider, RetryProvider, default_model_for_mode,
    resolve_claude_alias,
};
```

- [ ] **T02.5 (Green): Verify PASS**

```bash
cargo nextest run test_default_model_for_mode
cargo clippy --tests -- -D warnings
cargo fmt --check
cargo doc --no-deps
```

Expected: 3 tests pass, zero warnings, zero doc warnings.

- [ ] **T02.6 (Refactor): None needed.**

- [ ] **T02.7 (Commits):**

```bash
git add src/provider.rs
git commit -m "test: add default_model_for_mode test stubs"
```

```bash
git add src/provider.rs src/lib.rs
git commit -m "feat: add default_model_for_mode for Python v2.2.3 parity"
```

---

## Task T03 — `build_retry_prompt` in `user_prompt.rs`

**Files:**
- Modify: `src/user_prompt.rs` (add `pub(crate)` function + tests)

- [ ] **T03.1 (Red): Write the failing tests**

Append to `src/user_prompt.rs` inside `#[cfg(test)] mod tests`:

```rust
#[test]
fn test_build_retry_prompt_appends_feedback_block_exact_format() {
    let original = "MODE: code-review\n\
                    ---BEGIN USER CONTEXT abc---\n\
                    hello\n\
                    ---END USER CONTEXT abc---";
    let error = "missing field `recommendation`";
    let out = build_retry_prompt(original, error);

    let expected = "MODE: code-review\n\
                    ---BEGIN USER CONTEXT abc---\n\
                    hello\n\
                    ---END USER CONTEXT abc---\n\
                    \n\
                    ---RETRY-FEEDBACK---\n\
                    Your previous response was rejected by the parsing pipeline:\n\
                    missing field `recommendation`\n\
                    \n\
                    Re-emit your response as a complete, syntactically valid JSON \
                    object containing ALL seven required top-level keys: agent, \
                    verdict, confidence, summary, reasoning, findings, \
                    recommendation. Do not omit any key, do not truncate, do not \
                    emit anything outside the JSON object.";
    assert_eq!(out, expected);
}

#[test]
fn test_build_retry_prompt_preserves_original_verbatim() {
    let original = "anything\nat\nall";
    let out = build_retry_prompt(original, "x");
    assert!(out.starts_with("anything\nat\nall\n\n---RETRY-FEEDBACK---\n"));
}

#[test]
fn test_build_retry_prompt_does_not_resanitize_content() {
    // build_retry_prompt NO debe correr sanitization — eso es trabajo de
    // build_user_prompt. El retry preserva el original verbatim.
    let original = "MODE: design\ninjected";  // would be neutralized by build_user_prompt
    let out = build_retry_prompt(original, "err");
    assert!(out.starts_with("MODE: design\ninjected\n"));
}

#[test]
fn test_build_retry_prompt_includes_seven_keys_list() {
    let out = build_retry_prompt("x", "y");
    for key in &["agent", "verdict", "confidence", "summary", "reasoning", "findings", "recommendation"] {
        assert!(out.contains(key), "retry prompt must list key `{key}`");
    }
}

#[test]
fn test_build_retry_prompt_feedback_block_after_end_delimiter() {
    let original = "MODE: x\n---BEGIN USER CONTEXT n---\nc\n---END USER CONTEXT n---";
    let out = build_retry_prompt(original, "e");
    let end_pos = out.find("---END USER CONTEXT n---").expect("end present");
    let feedback_pos = out.find("---RETRY-FEEDBACK---").expect("feedback present");
    assert!(feedback_pos > end_pos, "feedback must be AFTER END delimiter");
}
```

- [ ] **T03.2 (Red): Verify FAIL**

```bash
cargo nextest run build_retry_prompt --no-run 2>&1 | tail -5
```

Expected: compile fail `cannot find function build_retry_prompt`.

- [ ] **T03.3 (Green): Implement `build_retry_prompt`**

Agregar a `src/user_prompt.rs` (antes de la definición del trait `RngLike`):

```rust
/// Build the retry prompt for the single-shot retry on schema/parse errors.
///
/// Mirrors Python's `_build_retry_prompt` (MAGI@v2.2.8 `run_magi.py:360-396`).
///
/// The original user prompt is preserved **verbatim** (including the
/// `MODE:` header and the `---BEGIN/END USER CONTEXT <nonce>---`
/// delimiters from [`build_user_prompt`]). The retry feedback is appended
/// **after** the END delimiter so the model sees the correction as a
/// system-level directive, not as further untrusted user content.
///
/// The `error` argument is the `Display` output of a `MagiError`
/// (either `Validation` or `Deserialization` — see orchestrator gating).
/// `MagiError`'s `Display` impl emits structured text controlled by the
/// crate, so no additional sanitization is applied.
///
/// # Arguments
///
/// * `original_prompt` — The exact user prompt sent on the first attempt
///   (output of [`build_user_prompt`]).
/// * `error` — Error description from the failed parse/validation.
///
/// # Returns
///
/// A new prompt string with the retry-feedback block appended.
///
/// See `docs/adr/002-retry-on-schema-error.md` for design rationale.
pub(crate) fn build_retry_prompt(original_prompt: &str, error: &str) -> String {
    format!(
        "{original_prompt}\n\n\
         ---RETRY-FEEDBACK---\n\
         Your previous response was rejected by the parsing pipeline:\n\
         {error}\n\n\
         Re-emit your response as a complete, syntactically valid JSON \
         object containing ALL seven required top-level keys: agent, \
         verdict, confidence, summary, reasoning, findings, \
         recommendation. Do not omit any key, do not truncate, do not \
         emit anything outside the JSON object."
    )
}
```

- [ ] **T03.4 (Green): Verify PASS**

```bash
cargo nextest run build_retry_prompt
cargo clippy --tests -- -D warnings
cargo fmt --check
cargo doc --no-deps
```

Expected: 5 tests pass, zero warnings.

- [ ] **T03.5 (Refactor): None.**

- [ ] **T03.6 (Commits):**

```bash
git add src/user_prompt.rs
git commit -m "test: add build_retry_prompt format and behavior tests"
```

```bash
git add src/user_prompt.rs
git commit -m "feat: add build_retry_prompt for v2.2.0 schema-retry parity"
```

---

## Task T04 — `retried_agents` field in `MagiReport`

**Files:**
- Modify: `src/reporting.rs` (add field + Deserialize derive)
- Modify: `src/orchestrator.rs` (update MagiReport construction site)

- [ ] **T04.1 (Red): Write the failing tests**

Append a `src/reporting.rs` inside `#[cfg(test)] mod tests`. Buscar el helper `dummy_consensus()` o similar usado en tests existentes de `MagiReport`; reusarlo. Si no existe, declarar uno mínimo inline:

```rust
#[test]
fn test_magi_report_retried_agents_default_empty() {
    let report = MagiReport {
        agents: vec![],
        consensus: dummy_consensus(),
        banner: String::new(),
        report: String::new(),
        degraded: false,
        failed_agents: BTreeMap::new(),
        retried_agents: BTreeSet::new(),
    };
    assert!(report.retried_agents.is_empty());
}

#[test]
fn test_magi_report_skip_serializing_empty_retried_agents() {
    let report = MagiReport {
        agents: vec![],
        consensus: dummy_consensus(),
        banner: String::new(),
        report: String::new(),
        degraded: false,
        failed_agents: BTreeMap::new(),
        retried_agents: BTreeSet::new(),
    };
    let json = serde_json::to_string(&report).unwrap();
    assert!(!json.contains("retried_agents"), "empty retried_agents must be omitted");
}

#[test]
fn test_magi_report_serializes_non_empty_retried_agents_alphabetically() {
    let mut retried = BTreeSet::new();
    retried.insert(AgentName::Melchior);
    retried.insert(AgentName::Balthasar);
    retried.insert(AgentName::Caspar);
    let report = MagiReport {
        agents: vec![],
        consensus: dummy_consensus(),
        banner: String::new(),
        report: String::new(),
        degraded: false,
        failed_agents: BTreeMap::new(),
        retried_agents: retried,
    };
    let json = serde_json::to_string(&report).unwrap();
    assert!(json.contains(r#""retried_agents":["balthasar","caspar","melchior"]"#),
        "got: {json}");
}

#[test]
fn test_magi_report_deserialize_v03_json_defaults_retried_agents_empty() {
    // v0.3.1 JSON did not have the "retried_agents" key
    let v03_json = r#"{
        "agents": [],
        "consensus": {"verdict_label":"GO","score":1.0,"confidence":0.9,"agents":[],"findings":[],"conditions":[]},
        "banner": "",
        "report": "",
        "degraded": false,
        "failed_agents": {}
    }"#;
    let report: MagiReport = serde_json::from_str(v03_json).unwrap();
    assert!(report.retried_agents.is_empty());
}
```

Note: el shape exacto del `consensus` dummy depende del struct `ConsensusResult`. Adaptar al schema actual (revisar `src/consensus.rs::ConsensusResult` antes de escribir el JSON literal). Si la deserialización requiere más fields, completarlos con defaults.

- [ ] **T04.2 (Red): Verify FAIL**

```bash
cargo nextest run magi_report --no-run 2>&1 | tail -10
```

Expected: compile errors `no field 'retried_agents' on type MagiReport` y (para el test deserialize) `the trait Deserialize is not implemented for MagiReport`.

- [ ] **T04.3 (Green): Add field + `Deserialize` derive**

En `src/reporting.rs`, modificar el struct `MagiReport`:

```rust
use std::collections::BTreeSet;  // agregar si no está
// El use serde::{Deserialize, Serialize}; debe incluir Deserialize.

/// ...existing doc...
#[derive(Debug, Clone, Serialize, Deserialize)]  // added: Deserialize
pub struct MagiReport {
    pub agents: Vec<AgentOutput>,
    pub consensus: ConsensusResult,
    pub banner: String,
    pub report: String,
    pub degraded: bool,
    pub failed_agents: BTreeMap<AgentName, String>,
    /// Agents whose first attempt failed schema/parse validation and that
    /// were retried once. Included in JSON only if non-empty.
    /// Composes with `failed_agents` to derive recovery cohorts.
    /// See `docs/adr/002-retry-on-schema-error.md`.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub retried_agents: BTreeSet<AgentName>,
}
```

Verificar que `AgentName`, `ConsensusResult`, y `AgentOutput` deriven `Deserialize`. Si no, agregárselo (debería estar ya por consistency).

- [ ] **T04.4 (Green): Update existing constructors of MagiReport**

Compilar y resolver cada compile error:

```bash
cargo build --tests 2>&1 | grep "missing field" | head -10
```

En cada sitio reportado, agregar `retried_agents: BTreeSet::new(),` al struct literal. Esperables:
- `src/reporting.rs` dentro de `ReportBuilder::build` (si construye `MagiReport`).
- `src/orchestrator.rs` final return de `analyze()`.

- [ ] **T04.5 (Green): Verify PASS**

```bash
cargo nextest run magi_report
cargo nextest run  # full suite — no debe romper otros tests
cargo clippy --tests -- -D warnings
cargo fmt --check
cargo doc --no-deps
```

Expected: 4 tests nuevos pasan, suite completa verde.

- [ ] **T04.6 (Refactor): Verificar ergonomía de construcción**

Si 3+ sitios construyen `MagiReport` con `retried_agents: BTreeSet::new()`, considerar agregar `#[derive(Default)]` si todos los demás campos lo permiten. Si no aplica limpiamente, no añadirlo — explicar en commit message.

- [ ] **T04.7 (Commits):**

```bash
git add src/reporting.rs
git commit -m "test: add retried_agents field tests on MagiReport"
```

```bash
git add src/reporting.rs src/orchestrator.rs
git commit -m "feat: add retried_agents telemetry field to MagiReport"
```

---

## Task T05 — `RoutingMockProvider` test helper

**Files:**
- Create: `src/test_support.rs`
- Modify: `src/lib.rs` (gate-test module)

Test helper. NO es production code.

- [ ] **T05.1 (Red): Crear archivo vacío y tests que fallen al compilar**

Crear `src/test_support.rs` con header obligatorio + comment placeholder:

```rust
// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-05-15

//! Test-only support utilities. Gated `#[cfg(test)]` at module declaration.
//! NOT public API.
```

Agregar al final del archivo:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{CompletionConfig, LlmProvider};
    use crate::error::ProviderError;

    #[tokio::test]
    async fn test_routing_mock_provider_routes_by_agent_marker() {
        let mp = RoutingMockProvider::new()
            .with_agent_responses("melchior", vec![Ok("MEL_1".to_string()), Ok("MEL_2".to_string())])
            .with_agent_responses("balthasar", vec![Ok("BAL_1".to_string())]);

        let cfg = CompletionConfig::default();
        let mel_sys = "You are Melchior, the Scientist.";
        let bal_sys = "You are Balthasar, the Pragmatist.";

        let r1 = mp.complete(mel_sys, "x", &cfg).await.unwrap();
        let r2 = mp.complete(bal_sys, "x", &cfg).await.unwrap();
        let r3 = mp.complete(mel_sys, "x", &cfg).await.unwrap();
        assert_eq!(r1, "MEL_1");
        assert_eq!(r2, "BAL_1");
        assert_eq!(r3, "MEL_2");
    }

    #[tokio::test]
    async fn test_routing_mock_provider_returns_error_when_sequence_exhausted() {
        let mp = RoutingMockProvider::new()
            .with_agent_responses("caspar", vec![Ok("CAS_1".to_string())]);
        let cfg = CompletionConfig::default();
        let cas_sys = "You are Caspar, the Critic.";

        let _ = mp.complete(cas_sys, "x", &cfg).await.unwrap();
        let r = mp.complete(cas_sys, "x", &cfg).await;
        assert!(matches!(r, Err(ProviderError::Process { .. })));
    }

    #[tokio::test]
    async fn test_routing_mock_provider_can_inject_provider_errors() {
        let mp = RoutingMockProvider::new()
            .with_agent_responses("melchior", vec![
                Err(ProviderError::Timeout { message: "t".to_string() }),
                Ok("MEL_2".to_string()),
            ]);
        let cfg = CompletionConfig::default();
        let mel_sys = "You are Melchior, the Scientist.";

        let r1 = mp.complete(mel_sys, "x", &cfg).await;
        assert!(matches!(r1, Err(ProviderError::Timeout { .. })));
        let r2 = mp.complete(mel_sys, "x", &cfg).await.unwrap();
        assert_eq!(r2, "MEL_2");
    }
}
```

Registrar el módulo en `src/lib.rs`:

```rust
#[cfg(test)]
pub(crate) mod test_support;
```

- [ ] **T05.2 (Red): Verify FAIL**

```bash
cargo nextest run routing_mock_provider --no-run 2>&1 | tail -5
```

Expected: compile error `cannot find struct RoutingMockProvider`.

- [ ] **T05.3 (Green): Implement `RoutingMockProvider`**

En `src/test_support.rs` (antes del bloque de tests):

```rust
use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;

use crate::error::ProviderError;
use crate::provider::{CompletionConfig, LlmProvider};

/// Mock provider that routes `complete()` calls to per-agent response
/// sequences by detecting the agent's role title in the system prompt.
///
/// Detection markers (case-insensitive substring match against
/// `system_prompt`):
/// - `"melchior"` → routes to "melchior" sequence
/// - `"balthasar"` → routes to "balthasar" sequence
/// - `"caspar"` → routes to "caspar" sequence
///
/// If no marker matches or the matched sequence is exhausted, returns
/// `ProviderError::Process` for diagnostic visibility.
pub struct RoutingMockProvider {
    sequences: Mutex<HashMap<&'static str, Vec<Result<String, ProviderError>>>>,
}

impl RoutingMockProvider {
    pub fn new() -> Self {
        Self {
            sequences: Mutex::new(HashMap::new()),
        }
    }

    /// Set the response sequence for an agent. `agent_key` ∈ {"melchior",
    /// "balthasar", "caspar"}. Responses are consumed FIFO.
    pub fn with_agent_responses(
        self,
        agent_key: &'static str,
        responses: Vec<Result<String, ProviderError>>,
    ) -> Self {
        self.sequences.lock().unwrap().insert(agent_key, responses);
        self
    }

    fn detect_agent(system_prompt: &str) -> Option<&'static str> {
        let lower = system_prompt.to_lowercase();
        for key in &["melchior", "balthasar", "caspar"] {
            if lower.contains(key) {
                return Some(*key);
            }
        }
        None
    }
}

impl Default for RoutingMockProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmProvider for RoutingMockProvider {
    async fn complete(
        &self,
        system_prompt: &str,
        _user_prompt: &str,
        _config: &CompletionConfig,
    ) -> Result<String, ProviderError> {
        let key = Self::detect_agent(system_prompt).ok_or_else(|| {
            ProviderError::Process {
                exit_code: None,
                stderr: format!(
                    "RoutingMockProvider: no agent marker found in system prompt: {}",
                    system_prompt.chars().take(80).collect::<String>()
                ),
            }
        })?;

        let mut sequences = self.sequences.lock().unwrap();
        let seq = sequences.get_mut(key).ok_or_else(|| ProviderError::Process {
            exit_code: None,
            stderr: format!("RoutingMockProvider: no sequence registered for {key}"),
        })?;

        if seq.is_empty() {
            return Err(ProviderError::Process {
                exit_code: None,
                stderr: format!("RoutingMockProvider: sequence exhausted for {key}"),
            });
        }
        seq.remove(0)
    }

    fn name(&self) -> &str {
        "routing-mock"
    }

    fn model(&self) -> &str {
        "test"
    }
}
```

- [ ] **T05.4 (Green): Verify PASS**

```bash
cargo nextest run routing_mock_provider
cargo clippy --tests -- -D warnings
cargo fmt --check
```

Expected: 3 tests pass.

- [ ] **T05.5 (Verify markers match prompts)**

```bash
grep -l "Melchior\|Balthasar\|Caspar" src/prompts_md/*.md
```

Expected: las 3 files matchean. Si una no matchea (renombre en v2.2.8), ajustar `detect_agent` o documentar la divergencia.

- [ ] **T05.6 (Commits):**

```bash
git add src/test_support.rs src/lib.rs
git commit -m "test: add RoutingMockProvider tests for per-agent sequences"
```

```bash
git add src/test_support.rs
git commit -m "feat: add RoutingMockProvider test helper for retry tests"
```

---

## Task T06 — `parse_and_validate` + `DispatchOutcome` enum

**Files:**
- Modify: `src/orchestrator.rs`

Building blocks para T07. No se usan aún en `analyze`.

- [ ] **T06.1 (Red): Write the failing tests**

Append al `#[cfg(test)] mod tests` en `src/orchestrator.rs`:

```rust
#[test]
fn test_parse_and_validate_ok_for_valid_json() {
    let validator = Validator::new();
    let raw = r#"{"agent":"melchior","verdict":"approve","confidence":0.9,"summary":"s","reasoning":"r","findings":[],"recommendation":"rec"}"#;
    let out = parse_and_validate(raw, &validator).unwrap();
    assert_eq!(out.agent, AgentName::Melchior);
}

#[test]
fn test_parse_and_validate_returns_deserialization_for_bad_json() {
    let validator = Validator::new();
    let raw = "not json at all {{{";
    let err = parse_and_validate(raw, &validator).unwrap_err();
    assert!(matches!(err, MagiError::Deserialization(_)));
}

#[test]
fn test_parse_and_validate_returns_validation_for_out_of_range_confidence() {
    let validator = Validator::new();
    let raw = r#"{"agent":"melchior","verdict":"approve","confidence":1.5,"summary":"s","reasoning":"r","findings":[],"recommendation":"rec"}"#;
    let err = parse_and_validate(raw, &validator).unwrap_err();
    assert!(matches!(err, MagiError::Validation(_)));
}

#[test]
fn test_dispatch_outcome_failed_variants_construct() {
    let _a = DispatchOutcome::Failed { reason: "x".to_string(), retried: false };
    let _b = DispatchOutcome::Failed { reason: "y".to_string(), retried: true };
}
```

- [ ] **T06.2 (Red): Verify FAIL**

```bash
cargo nextest run parse_and_validate --no-run 2>&1 | tail -10
```

Expected: compile errors `cannot find function parse_and_validate` y `cannot find type DispatchOutcome`.

- [ ] **T06.3 (Green): Implement helpers**

En `src/orchestrator.rs`, agregar antes de `impl Magi`:

```rust
/// Internal: outcome of dispatching a single agent. Success path returns
/// `AgentOutput` directly via `Ok`; this enum represents failure paths
/// with telemetry (was a retry attempted?).
#[derive(Debug)]
pub(crate) enum DispatchOutcome {
    /// Agent failed (first attempt without retry, or after retry).
    Failed { reason: String, retried: bool },
    /// Reserved: retry succeeded path uses `Ok(AgentOutput)` from caller.
    /// Kept for future symmetry if telemetry split is needed.
    #[allow(dead_code)]
    RetriedAndOk(crate::schema::AgentOutput),
}

/// Internal: parse a raw agent response and validate against the
/// `Validator`. Returns parsed output or one of the two error variants
/// that trigger retry in `dispatch_one_agent`:
/// - `MagiError::Deserialization` from `parse_agent_response`
/// - `MagiError::Validation` from `validator.validate_mut`
pub(crate) fn parse_and_validate(
    raw: &str,
    validator: &Validator,
) -> Result<AgentOutput, MagiError> {
    let mut output = parse_agent_response(raw)?;
    validator.validate_mut(&mut output)?;
    Ok(output)
}
```

- [ ] **T06.4 (Green): Verify PASS**

```bash
cargo nextest run parse_and_validate
cargo nextest run dispatch_outcome
cargo clippy --tests -- -D warnings
```

Expected: 4 tests pass.

- [ ] **T06.5 (Refactor): Reuse `parse_and_validate` from `process_results`**

El existente `process_results` (orchestrator.rs ~514-528) duplica la lógica parse→validate→error. Reemplazar con:

```rust
match result {
    Ok(raw) => match parse_and_validate(&raw, &self.validator) {
        Ok(output) => successful.push(output),
        Err(e @ MagiError::Deserialization(_)) => {
            failed_agents.insert(name, format!("parse: {e}"));
        }
        Err(e @ MagiError::Validation(_)) => {
            failed_agents.insert(name, format!("validation: {e}"));
        }
        Err(e) => {
            failed_agents.insert(name, e.to_string());
        }
    },
    Err(e) => { failed_agents.insert(name, e.to_string()); }
}
```

`process_results` será removido en T08 — este refactor es interim para validar `parse_and_validate` ya en el camino caliente.

- [ ] **T06.6 (Verification):**

```bash
cargo nextest run
cargo clippy --tests -- -D warnings
cargo fmt --check
```

Expected: green.

- [ ] **T06.7 (Commits):**

```bash
git add src/orchestrator.rs
git commit -m "test: add parse_and_validate + DispatchOutcome stubs"
```

```bash
git add src/orchestrator.rs
git commit -m "feat: add parse_and_validate helper + DispatchOutcome enum"
```

```bash
git add src/orchestrator.rs
git commit -m "refactor: route process_results through parse_and_validate"
```

---

## Task T07 — `dispatch_one_agent` async function with retry

**Files:**
- Modify: `src/orchestrator.rs`

- [ ] **T07.1 (Red): Write the failing tests**

Append al test mod en `src/orchestrator.rs`. Las 5 pruebas cubren BDD-03..BDD-08 a nivel de unidad.

```rust
#[tokio::test]
async fn test_dispatch_one_agent_success_first_attempt_no_retry() {
    use crate::test_support::RoutingMockProvider;
    use std::sync::Arc;
    use std::time::Duration;

    let valid = r#"{"agent":"melchior","verdict":"approve","confidence":0.9,"summary":"s","reasoning":"r","findings":[],"recommendation":"rec"}"#;
    let provider = Arc::new(
        RoutingMockProvider::new()
            .with_agent_responses("melchior", vec![Ok(valid.to_string())])
    );
    let agent = Agent::new(AgentName::Melchior, provider as Arc<dyn LlmProvider>);
    let validator = Arc::new(Validator::new());
    let cfg = CompletionConfig::default();

    let (result, retried) = dispatch_one_agent(
        agent,
        "MODE: code-review\n---BEGIN USER CONTEXT n---\nx\n---END USER CONTEXT n---".to_string(),
        cfg,
        validator,
        Duration::from_secs(30),
    ).await;

    assert!(result.is_ok());
    assert!(!retried);
}

#[tokio::test]
async fn test_dispatch_one_agent_retries_on_validation_error_and_succeeds() {
    use crate::test_support::RoutingMockProvider;
    use std::sync::Arc;
    use std::time::Duration;

    let bad = r#"{"agent":"melchior"}"#;
    let good = r#"{"agent":"melchior","verdict":"approve","confidence":0.9,"summary":"s","reasoning":"r","findings":[],"recommendation":"rec"}"#;
    let provider = Arc::new(
        RoutingMockProvider::new()
            .with_agent_responses("melchior", vec![Ok(bad.to_string()), Ok(good.to_string())])
    );
    let agent = Agent::new(AgentName::Melchior, provider as Arc<dyn LlmProvider>);
    let validator = Arc::new(Validator::new());
    let cfg = CompletionConfig::default();

    let (result, retried) = dispatch_one_agent(
        agent,
        "MODE: code-review\n---BEGIN USER CONTEXT n---\nx\n---END USER CONTEXT n---".to_string(),
        cfg,
        validator,
        Duration::from_secs(30),
    ).await;

    assert!(result.is_ok());
    assert!(retried);
}

#[tokio::test]
async fn test_dispatch_one_agent_retries_on_deserialization_and_fails() {
    use crate::test_support::RoutingMockProvider;
    use std::sync::Arc;
    use std::time::Duration;

    let provider = Arc::new(
        RoutingMockProvider::new()
            .with_agent_responses("caspar", vec![
                Ok("not json {{{".to_string()),
                Ok("still not json".to_string()),
            ])
    );
    let agent = Agent::new(AgentName::Caspar, provider as Arc<dyn LlmProvider>);
    let validator = Arc::new(Validator::new());
    let cfg = CompletionConfig::default();

    let (result, retried) = dispatch_one_agent(
        agent,
        "MODE: design\n---BEGIN USER CONTEXT n---\nx\n---END USER CONTEXT n---".to_string(),
        cfg,
        validator,
        Duration::from_secs(30),
    ).await;

    assert!(result.is_err());
    let err_msg = match result {
        Err(DispatchOutcome::Failed { reason, .. }) => reason,
        _ => panic!("expected Failed"),
    };
    assert!(err_msg.starts_with("retry-failed: "), "got: {err_msg}");
    assert!(retried);
}

#[tokio::test]
async fn test_dispatch_one_agent_does_not_retry_on_provider_timeout() {
    use crate::test_support::RoutingMockProvider;
    use std::sync::Arc;
    use std::time::Duration;

    let provider = Arc::new(
        RoutingMockProvider::new()
            .with_agent_responses("balthasar", vec![
                Err(ProviderError::Timeout { message: "t".to_string() }),
                Ok("MUST NOT BE CALLED".to_string()),
            ])
    );
    let agent = Agent::new(AgentName::Balthasar, provider as Arc<dyn LlmProvider>);
    let validator = Arc::new(Validator::new());
    let cfg = CompletionConfig::default();

    let (result, retried) = dispatch_one_agent(
        agent,
        "MODE: code-review\n---BEGIN USER CONTEXT n---\nx\n---END USER CONTEXT n---".to_string(),
        cfg,
        validator,
        Duration::from_secs(30),
    ).await;

    assert!(result.is_err());
    let err_msg = match result {
        Err(DispatchOutcome::Failed { reason, .. }) => reason,
        _ => panic!("expected Failed"),
    };
    assert!(err_msg.to_lowercase().contains("timeout"));
    assert!(!retried, "provider errors must NOT trigger retry");
}

#[tokio::test]
async fn test_dispatch_one_agent_retry_then_provider_error_marks_retried() {
    // BDD-08: first attempt validation error → retry → provider error
    // retried=true must be preserved.
    use crate::test_support::RoutingMockProvider;
    use std::sync::Arc;
    use std::time::Duration;

    let provider = Arc::new(
        RoutingMockProvider::new()
            .with_agent_responses("caspar", vec![
                Ok("{}".to_string()),  // validation error
                Err(ProviderError::Timeout { message: "t2".to_string() }),
            ])
    );
    let agent = Agent::new(AgentName::Caspar, provider as Arc<dyn LlmProvider>);
    let validator = Arc::new(Validator::new());
    let cfg = CompletionConfig::default();

    let (result, retried) = dispatch_one_agent(
        agent,
        "MODE: x\n---BEGIN USER CONTEXT n---\nx\n---END USER CONTEXT n---".to_string(),
        cfg,
        validator,
        Duration::from_secs(30),
    ).await;

    assert!(result.is_err());
    let err_msg = match result {
        Err(DispatchOutcome::Failed { reason, .. }) => reason,
        _ => panic!("expected Failed"),
    };
    assert!(err_msg.starts_with("retry-failed: "));
    assert!(retried);
}
```

- [ ] **T07.2 (Red): Verify FAIL**

```bash
cargo nextest run dispatch_one_agent --no-run 2>&1 | tail -5
```

Expected: compile error `cannot find function dispatch_one_agent`.

- [ ] **T07.3 (Green): Implement `dispatch_one_agent`**

En `src/orchestrator.rs`, después de la definición de `DispatchOutcome` (T06):

```rust
use std::sync::Arc;
use std::time::Duration;

use crate::user_prompt::build_retry_prompt;

/// Dispatch a single agent with one-shot retry on schema/parse errors.
///
/// Returns `(Result<AgentOutput, DispatchOutcome>, bool)` where the bool
/// duplicates the `retried` flag for ergonomic destructuring at the
/// call site.
///
/// Retry trigger: `MagiError::Validation` or `MagiError::Deserialization`
/// from `parse_and_validate` on the first response. Provider errors and
/// timeouts skip retry.
///
/// See `docs/adr/002-retry-on-schema-error.md`.
pub(crate) async fn dispatch_one_agent(
    agent: Agent,
    user_prompt: String,
    config: CompletionConfig,
    validator: Arc<Validator>,
    timeout: Duration,
) -> (Result<AgentOutput, DispatchOutcome>, bool) {
    // First attempt
    let first_result =
        tokio::time::timeout(timeout, agent.execute(&user_prompt, &config)).await;

    let first_raw = match first_result {
        Ok(Ok(raw)) => raw,
        Ok(Err(provider_err)) => {
            return (
                Err(DispatchOutcome::Failed {
                    reason: MagiError::Provider(provider_err).to_string(),
                    retried: false,
                }),
                false,
            );
        }
        Err(_elapsed) => {
            return (
                Err(DispatchOutcome::Failed {
                    reason: format!("timeout: agent timed out after {timeout:?}"),
                    retried: false,
                }),
                false,
            );
        }
    };

    let first_err = match parse_and_validate(&first_raw, &validator) {
        Ok(output) => return (Ok(output), false),
        Err(e) => e,
    };

    let should_retry = matches!(
        first_err,
        MagiError::Validation(_) | MagiError::Deserialization(_)
    );
    if !should_retry {
        return (
            Err(DispatchOutcome::Failed {
                reason: first_err.to_string(),
                retried: false,
            }),
            false,
        );
    }

    let retry_prompt = build_retry_prompt(&user_prompt, &first_err.to_string());
    let second_result =
        tokio::time::timeout(timeout, agent.execute(&retry_prompt, &config)).await;

    let second_raw = match second_result {
        Ok(Ok(raw)) => raw,
        Ok(Err(provider_err)) => {
            return (
                Err(DispatchOutcome::Failed {
                    reason: format!("retry-failed: {}", MagiError::Provider(provider_err)),
                    retried: true,
                }),
                true,
            );
        }
        Err(_elapsed) => {
            return (
                Err(DispatchOutcome::Failed {
                    reason: format!("retry-failed: timeout after {timeout:?}"),
                    retried: true,
                }),
                true,
            );
        }
    };

    match parse_and_validate(&second_raw, &validator) {
        Ok(output) => (Ok(output), true),
        Err(e) => (
            Err(DispatchOutcome::Failed {
                reason: format!("retry-failed: {e}"),
                retried: true,
            }),
            true,
        ),
    }
}
```

- [ ] **T07.4 (Green): Verify PASS**

```bash
cargo nextest run dispatch_one_agent
cargo clippy --tests -- -D warnings
cargo fmt --check
```

Expected: 5 tests pass.

- [ ] **T07.5 (Refactor): None.**

- [ ] **T07.6 (Commits):**

```bash
git add src/orchestrator.rs
git commit -m "test: add dispatch_one_agent retry FSM tests"
```

```bash
git add src/orchestrator.rs
git commit -m "feat: implement dispatch_one_agent with single-shot retry"
```

---

## Task T08 — Wire `dispatch_one_agent` into `Magi::analyze`

**Files:**
- Modify: `src/orchestrator.rs`

- [ ] **T08.1 (Red): Write the failing integration tests**

```rust
#[tokio::test]
async fn test_analyze_populates_retried_agents_on_recovery() {
    use crate::test_support::RoutingMockProvider;
    use std::sync::Arc;

    let valid = |a: &str| format!(
        r#"{{"agent":"{a}","verdict":"approve","confidence":0.9,"summary":"s","reasoning":"r","findings":[],"recommendation":"rec"}}"#
    );

    let provider = Arc::new(
        RoutingMockProvider::new()
            .with_agent_responses("melchior", vec![Ok("{}".to_string()), Ok(valid("melchior"))])
            .with_agent_responses("balthasar", vec![Ok(valid("balthasar"))])
            .with_agent_responses("caspar", vec![Ok(valid("caspar"))])
    );
    let magi = Magi::new(provider as Arc<dyn LlmProvider>);
    let report = magi.analyze(&Mode::CodeReview, "fn main() {}").await.unwrap();

    assert!(report.failed_agents.is_empty(), "{:?}", report.failed_agents);
    assert_eq!(report.retried_agents.len(), 1);
    assert!(report.retried_agents.contains(&AgentName::Melchior));
    assert_eq!(report.agents.len(), 3);
}

#[tokio::test]
async fn test_analyze_retry_also_fails_lands_in_both_sets() {
    use crate::test_support::RoutingMockProvider;
    use std::sync::Arc;

    let valid = |a: &str| format!(
        r#"{{"agent":"{a}","verdict":"approve","confidence":0.9,"summary":"s","reasoning":"r","findings":[],"recommendation":"rec"}}"#
    );

    let provider = Arc::new(
        RoutingMockProvider::new()
            .with_agent_responses("caspar", vec![
                Ok("bad".to_string()),
                Ok("still bad".to_string()),
            ])
            .with_agent_responses("melchior", vec![Ok(valid("melchior"))])
            .with_agent_responses("balthasar", vec![Ok(valid("balthasar"))])
    );
    let magi = Magi::new(provider as Arc<dyn LlmProvider>);
    let report = magi.analyze(&Mode::CodeReview, "x").await.unwrap();

    assert_eq!(report.agents.len(), 2);
    assert!(report.failed_agents.contains_key(&AgentName::Caspar));
    assert!(report.failed_agents[&AgentName::Caspar].starts_with("retry-failed: "));
    assert!(report.retried_agents.contains(&AgentName::Caspar));
    assert!(report.degraded);
}

#[tokio::test]
async fn test_analyze_no_retry_on_timeout_keeps_retried_empty() {
    use crate::test_support::RoutingMockProvider;
    use std::sync::Arc;

    let valid = |a: &str| format!(
        r#"{{"agent":"{a}","verdict":"approve","confidence":0.9,"summary":"s","reasoning":"r","findings":[],"recommendation":"rec"}}"#
    );

    let provider = Arc::new(
        RoutingMockProvider::new()
            .with_agent_responses("balthasar", vec![
                Err(ProviderError::Timeout { message: "t".to_string() })
            ])
            .with_agent_responses("melchior", vec![Ok(valid("melchior"))])
            .with_agent_responses("caspar", vec![Ok(valid("caspar"))])
    );
    let magi = Magi::new(provider as Arc<dyn LlmProvider>);
    let report = magi.analyze(&Mode::CodeReview, "x").await.unwrap();

    assert_eq!(report.agents.len(), 2);
    assert!(report.failed_agents.contains_key(&AgentName::Balthasar));
    assert!(report.retried_agents.is_empty(), "no retry on timeout");
}
```

- [ ] **T08.2 (Red): Verify FAIL**

```bash
cargo nextest run test_analyze_populates_retried test_analyze_retry_also test_analyze_no_retry_on_timeout
```

Expected: tests fail (`retried_agents` no se popula porque `analyze` aún no usa el nuevo dispatch).

- [ ] **T08.3 (Green): Refactor `Magi::analyze`**

1. Reemplazar `launch_agents` (lines ~449-498) con `dispatch_with_retry`:

```rust
async fn dispatch_with_retry(
    &self,
    agents: Vec<Agent>,
    user_prompt: &str,
) -> Result<
    (Vec<AgentOutput>, BTreeMap<AgentName, String>, BTreeSet<AgentName>),
    MagiError,
> {
    let timeout = self.config.timeout;
    let completion = self.config.completion.clone();
    let validator = Arc::new(self.validator.clone());
    let mut handles = Vec::new();
    let mut abort_handles = Vec::new();

    for agent in agents {
        let name = agent.name();
        let user_prompt_cloned = user_prompt.to_string();
        let config = completion.clone();
        let validator = validator.clone();
        let handle = tokio::spawn(async move {
            dispatch_one_agent(agent, user_prompt_cloned, config, validator, timeout).await
        });
        abort_handles.push(handle.abort_handle());
        handles.push((name, handle));
    }

    let _guard = AbortGuard(abort_handles);

    let mut successful = Vec::new();
    let mut failed = BTreeMap::new();
    let mut retried = BTreeSet::new();
    for (name, handle) in handles {
        match handle.await {
            Ok((Ok(output), was_retried)) => {
                successful.push(output);
                if was_retried {
                    retried.insert(name);
                }
            }
            Ok((Err(DispatchOutcome::Failed { reason, retried: was_retried }), _)) => {
                failed.insert(name, reason);
                if was_retried {
                    retried.insert(name);
                }
            }
            Ok((Err(DispatchOutcome::RetriedAndOk(_)), _)) => {
                unreachable!("dispatch_one_agent returns RetriedAndOk via Ok variant");
            }
            Err(join_err) => {
                failed.insert(name, format!("panic: {join_err}"));
            }
        }
    }

    let min_agents = self.consensus_engine.min_agents();
    if successful.len() < min_agents {
        return Err(MagiError::InsufficientAgents {
            succeeded: successful.len(),
            required: min_agents,
        });
    }

    Ok((successful, failed, retried))
}
```

2. Modificar `analyze` para usar `dispatch_with_retry`:

```rust
// Cambio en analyze:
let (successful, failed_agents, retried_agents) = self
    .dispatch_with_retry(agents, &user_prompt)
    .await?;

// ...y al construir MagiReport:
let report = MagiReport {
    agents: successful,
    consensus,
    banner,
    report: markdown,
    degraded,
    failed_agents,
    retried_agents,  // NUEVO
};
```

3. Eliminar los métodos `launch_agents` y `process_results` ya inutilizados.

- [ ] **T08.4 (Green): Verify PASS**

```bash
cargo nextest run
cargo clippy --tests -- -D warnings
cargo fmt --check
cargo doc --no-deps
cargo audit
```

Expected: todos los tests pasan (existentes + 3 nuevos), zero warnings, zero advisories.

- [ ] **T08.5 (Refactor): Eliminar dead code post-remove**

```bash
cargo clippy --tests -- -D warnings
```

Si clippy reporta funciones privadas no usadas, eliminarlas. Candidatos: estructuras helper que `launch_agents`/`process_results` usaban en exclusiva.

- [ ] **T08.6 (Commits):**

```bash
git add src/orchestrator.rs
git commit -m "test: add analyze integration tests for retry telemetry"
```

```bash
git add src/orchestrator.rs
git commit -m "feat: wire dispatch_one_agent into Magi::analyze with retried_agents"
```

```bash
git add src/orchestrator.rs
git commit -m "refactor: remove obsolete launch_agents and process_results"
```

---

## Task T09 — Windows console UTF-8 hardening in `basic_analysis`

**Files:**
- Modify: `examples/basic_analysis.rs`

- [ ] **T09.1 (Setup): Inspect current example main**

```bash
head -60 examples/basic_analysis.rs
```

Identificar `fn main()` y el punto exacto donde insertar el setup.

- [ ] **T09.2 (Red): no es testeable unit-level**

Per spec §10.4, Windows FFI no es testeable unit-level. La verificación es manual o por smoke test en CI Windows. Documentar como comment en código.

- [ ] **T09.3 (Green): Add `setup_console_encoding`**

Agregar a `examples/basic_analysis.rs` antes de `fn main`:

```rust
#[cfg(windows)]
fn setup_console_encoding() {
    // SAFETY: SetConsoleOutputCP is a Win32 API that takes a single u32
    // by value and returns a BOOL (i32). It accesses no shared memory,
    // has no aliasing concerns, and is documented thread-safe by
    // Microsoft. Calling once at process start with CP_UTF8 (65001)
    // configures the console output codepage so subsequent `println!`
    // calls can emit UTF-8 (em dash, ellipsis, etc.) without panicking
    // on cp1252-default consoles.
    //
    // Return value ignored: a failed call means the console codepage
    // is already different (e.g. piped to a file with no console
    // attached). Falling back to whatever stdio is configured for is
    // acceptable behavior.
    const CP_UTF8: u32 = 65001;
    unsafe extern "system" {
        fn SetConsoleOutputCP(wCodePageID: u32) -> i32;
    }
    unsafe { SetConsoleOutputCP(CP_UTF8) };
}

#[cfg(not(windows))]
fn setup_console_encoding() {}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    setup_console_encoding();
    // ...existing body...
}
```

- [ ] **T09.4 (Green): Verify build green todas las plataformas**

```bash
cargo build --example basic_analysis --features claude-cli
cargo clippy --example basic_analysis --features claude-cli -- -D warnings
cargo fmt --check
```

Expected: zero warnings.

- [ ] **T09.5 (Refactor): None.**

- [ ] **T09.6 (Commits):**

```bash
git add examples/basic_analysis.rs
git commit -m "feat: harden basic_analysis example for Windows UTF-8 consoles"
```

(No Red commit — FFI no testeable unitariamente.)

---

## Task T10 — `basic_analysis` uses `default_model_for_mode`

**Files:**
- Modify: `examples/basic_analysis.rs`

- [ ] **T10.1 (Inspect): Find current model resolution**

```bash
grep -n "model\|--model\|opus\|sonnet" examples/basic_analysis.rs
```

- [ ] **T10.2 (Green): Default the model when arg missing**

Reemplazar el fallback actual (probablemente hardcoded `"opus"`) con una llamada:

```rust
use magi_core::{default_model_for_mode, resolve_claude_alias};

// En CLI parsing:
let mode: Mode = parse_mode(&args.mode)?;
let model_alias: &str = args.model.as_deref()
    .unwrap_or_else(|| default_model_for_mode(mode));
let model_id = resolve_claude_alias(model_alias);
```

Adaptar al pattern de argparse existente.

- [ ] **T10.3 (Green): Verify build green**

```bash
cargo build --example basic_analysis --features claude-cli
cargo clippy --example basic_analysis --features claude-cli -- -D warnings
```

- [ ] **T10.4 (Manual smoke):**

```bash
cargo run --example basic_analysis --features claude-cli -- --mode analysis --input ./README.md
```

(Si requiere claude CLI no disponible, skip. La compilación verde es suficiente.)

- [ ] **T10.5 (Commits):**

```bash
git add examples/basic_analysis.rs
git commit -m "feat: use default_model_for_mode in basic_analysis"
```

---

## Task T11 — CHANGELOG + version bump

**Files:**
- Modify: `Cargo.toml` (version 0.3.1 → 0.4.0)
- Modify: `CHANGELOG.md`

- [ ] **T11.1: Bump version**

En `Cargo.toml`:

```toml
[package]
name = "magi-core"
version = "0.4.0"
```

- [ ] **T11.2: Update CHANGELOG**

Prepend a `CHANGELOG.md` (usar el entry de v0.3.1 como template stylistic):

```markdown
## [0.4.0] - 2026-05-XX

### Added

- `default_model_for_mode(Mode) -> &'static str` in `provider.rs` (paridad with Python `MODE_DEFAULT_MODELS` v2.2.3).
- `retried_agents: BTreeSet<AgentName>` field on `MagiReport` — telemetry of agents whose first attempt failed schema/parse and were retried once.
- `MagiReport` now derives `Deserialize` (in addition to `Serialize`) to support backward-compatible deserialization of v0.3.x JSON.
- `examples/basic_analysis.rs` configures console UTF-8 codepage on Windows at startup.

### Changed

- Single-shot retry on `MagiError::Validation` and `MagiError::Deserialization` errors during `Magi::analyze`. Agents whose first response fails schema or JSON parsing are retried once with a corrective prompt (Python v2.2.0 + v2.2.4 parity).
- Embedded agent prompts bumped from `MAGI@v2.1.3` (commit 668f0e5e) to `MAGI@v2.2.8` (commit 645932c7). New prompts explicitly require all seven top-level JSON keys.

### Backward compatibility

- All v0.3.1 public APIs preserved; only additive changes. v0.3.x JSON deserializes to v0.4.0 `MagiReport` with `retried_agents = BTreeSet::new()` default.

### Documentation

- New ADR: `docs/adr/002-retry-on-schema-error.md`.
- New guide: `docs/migration-v0.4.md`.
```

- [ ] **T11.3: Full verification**

```bash
cargo build --release
cargo nextest run
cargo clippy --tests -- -D warnings
cargo fmt --check
cargo doc --no-deps
cargo audit
```

Expected: todos green.

- [ ] **T11.4: Pre-merge gates per CLAUDE.local.md §6**

```bash
# Loop 1: /requesting-code-review until clean-to-go
# Loop 2: /magi:magi until >= GO WITH CAVEATS
```

Aplicar las *Conditions for Approval* del MAGI gate si las hay.

- [ ] **T11.5: Final commit**

```bash
git add Cargo.toml CHANGELOG.md
git commit -m "chore: release v0.4.0"
```

(No Red phase — version bump y CHANGELOG no son testeables.)

---

## Self-review against spec

| Spec section | Task(s) |
|---|---|
| §1 Gap 1 — Bump prompts | T01 |
| §1 Gap 2 — Default model | T02, T10 |
| §1 Gap 3 — Retry layer | T03, T06, T07, T08 |
| §1 Gap 4 — `retried_agents` telemetry | T04, T08 |
| §1 Gap 5 — Windows hardening | T09 |
| §11 ADR mandatorio | T00 |

| Spec BDD | Task que lo implementa |
|---|---|
| BDD-01 (prompt SHA-256) | T01.4 |
| BDD-02 (default model 3 modos) | T02.1 |
| BDD-03 (retry exitoso schema) | T07.1 (test_..._retries_on_validation) + T08.1 |
| BDD-04 (retry exitoso parse) | T07.1 + T08.1 |
| BDD-05 (retry también falla) | T07.1 (test_..._retries_on_deserialization) + T08.1 |
| BDD-06 (no retry timeout) | T07.1 + T08.1 |
| BDD-07 (no retry HTTP) | T07.1 (cubre por extension del test timeout pattern) |
| BDD-08 (retry → provider error) | T07.1 (test_..._retry_then_provider_error) |
| BDD-09 (telemetry vacía skip) | T04.1 |
| BDD-10 (telemetry orden alfabético) | T04.1 |
| BDD-11 (backward compat deser) | T04.1 |
| BDD-12 (markdown no retried) | implícito — no se cambia `ReportFormatter` |
| BDD-13 (defensa preserved) | T07.1 + T03 (nonce mismo, feedback fuera) |
| BDD-14 (build_retry_prompt exact) | T03.1 |
| BDD-15 (Windows no panic) | T09 (smoke manual) |
| BDD-16 (example default model) | T10 |

**Placeholder scan:** sin "TBD", sin "implement later". Cada code block tiene contenido completo.

**Type consistency:** `DispatchOutcome` definido en T06, usado en T07 + T08. `parse_and_validate` definido T06, usado T07. `dispatch_one_agent` definido T07, llamado desde `dispatch_with_retry` en T08. `RoutingMockProvider` definido T05, usado en T07 + T08. `default_model_for_mode` definido T02, usado en T10. `build_retry_prompt` definido T03, usado en T07.

---

## Execution

**Plan complete and saved to `planning/claude-plan-tdd-org.md`.** Dos opciones de ejecución:

1. **Subagent-Driven (recomendado)** — `superpowers:subagent-driven-development`. Tareas T01–T05 paralelizables (5 subagentes); T06–T08 secuenciales (mismo archivo); T09–T11 paralelizables después. Two-stage review per task.

2. **Inline Execution** — `superpowers:executing-plans`. Sesión secuencial con checkpoints intermedios.

**Antes de ejecutar (CLAUDE.local.md §1):**

1. **Checkpoint 1** — revisión manual del usuario sobre este plan (`claude-plan-tdd-org.md`). El usuario puede rechazar y forzar regeneración.
2. **MAGI Gate** — `/magi:magi revisa @sbtdd/spec-behavior.md y @planning/claude-plan-tdd-org.md` iterando hasta veredicto >= `GO WITH CAVEATS`. El agente reescribe `claude-plan-tdd.md` aplicando los findings de MAGI.
3. **Aprobación final** de `claude-plan-tdd.md`.

Solo entonces se puede dispatchear T01..T11.
