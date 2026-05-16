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
- `CompletionConfig` is unchanged from v0.3.1 — no new fields. Test-only
  routing of `RoutingMockProvider` is achieved via an internal
  `tokio::task_local!` (not exposed in public API).

## Behavior changes

- **More resilient analyses.** Agents whose first response fails schema
  validation (e.g., missing one of the 7 required keys) are retried once
  with a corrective prompt. Previously these agents would go straight
  to `failed_agents` and (in the 3-agent case) trigger degraded mode.
- **New telemetry.** When a retry occurs, the agent name appears in
  `report.retried_agents` regardless of whether the retry succeeded. If
  the retry also fails, the agent ALSO appears in `failed_agents` with
  reason prefix `"retry-failed: "`.

## Retry semantics caveat (MAGI R3 W3)

The retry preserves the **original user prompt verbatim** — including
the BEGIN/END USER CONTEXT delimiters and the same nonce. Same `content`
goes back to the LLM with only a `---RETRY-FEEDBACK---` block appended
after the END delimiter. This is intentional ([D-17] in
`sbtdd/spec-behavior.md`) for Python parity: a fresh nonce and
re-sanitization would change the LLM's perception of the framing and
weaken the corrective feedback's effectiveness.

**Implication for prompt-injection-induced failures:** if the original
`content` contains an injection that successfully evaded the v0.3
sanitization layer (`build_user_prompt`), the retry will resend the
same content. The model will see the same adversarial payload twice.
This is acceptable because:
1. The v0.3 sanitization (normalize → strip → neutralize → nonce-fail-closed)
   is the primary defense; a payload that passes those layers is, by
   construction, structurally neutralized.
2. The retry-feedback block has its own independent 4-layer sanitization
   (`sanitize_error_for_retry_feedback`) covering the corrective message
   itself, so the retry envelope cannot be hijacked even by error
   strings that echo adversarial content.
3. Burning a second API call on the same payload is preferable to
   weakening the structural framing that the LLM has learned to trust.

If your threat model requires per-retry re-sanitization, use
`MagiBuilder::with_retry_disabled()` to skip the retry layer entirely.

## Performance impact

**Worst-case latency doubles when an agent triggers retry.** The retry
attempt uses a fresh `timeout` budget identical to the first attempt.
If your application configures a custom timeout via
`MagiBuilder::with_timeout(d)`, plan for 2×`d` as the effective ceiling
per agent.

If your deployment is latency-sensitive and cannot tolerate the 2×
worst-case, use the opt-out:

```rust
use magi_core::MagiBuilder;
let magi = MagiBuilder::new(provider)
    .with_retry_disabled()
    .build();
```

When disabled:
- Agents that fail schema/parse on the first attempt go directly to
  `failed_agents` (same v0.3.1 behavior).
- `retried_agents` is always empty in the resulting `MagiReport`.
- Per-agent worst-case latency equals `timeout` (not 2×`timeout`).

## Consumer action items

- **None required for backward compatibility.** Existing v0.3.x callers
  continue to work without changes.
- **Optional:** Inspect `report.retried_agents` for resilience metrics
  (e.g., dashboard "% of analyses that needed retry"). Composes with
  `failed_agents` for two cohorts: `retried - failed.keys()` is
  "retry recovered"; `retried ∩ failed.keys()` is "retry also failed".
- **Optional:** Use `default_model_for_mode` instead of hardcoding model
  strings:
  ```rust
  let alias = magi_core::default_model_for_mode(Mode::Analysis);
  let model_id = magi_core::resolve_claude_alias(alias);
  ```
- **Optional:** Use `MagiBuilder::with_retry_disabled()` if 2× worst-case
  latency is unacceptable for your deployment.
- **Windows operators:** the `basic_analysis` example now sets the
  console output codepage to UTF-8 at startup; em-dash and other
  multibyte chars in the report no longer panic on cp1252 consoles.
  Library consumers building their own binaries should replicate the
  pattern in `examples/basic_analysis.rs::setup_console_encoding`.

## `test-utils` feature flag stability

v0.4.0 introduces a new cargo feature `test-utils` that exposes
`magi_core::test_support::RoutingMockProvider` for integration tests.

The feature is **stable only within the v0.4.x line**. Future minor or
major versions (v0.5+) may rename, restructure, or remove this feature.
Consumers building production code on top of `magi_core::test_support`
should not assume long-term API stability — the module's primary
purpose is in-tree testing.

To use the routing mock from a downstream test:

```toml
# Consumer's Cargo.toml
[dev-dependencies]
magi-core = { version = "0.4", features = ["test-utils"] }
```

```rust
// Consumer's test
use magi_core::test_support::RoutingMockProvider;
use magi_core::Magi;
use std::sync::Arc;
// ...
```

## Test count

`cargo nextest run` test count moves from ~359 (v0.3.1) to ~411
(v0.4.0). Integration tests under `tests/` require
`--features test-utils` to access `RoutingMockProvider`.

## Verification

```bash
cargo nextest run --features test-utils
cargo clippy --tests --features test-utils -- -D warnings
cargo fmt --check
cargo doc --no-deps
cargo audit
```

All commands must pass clean.
