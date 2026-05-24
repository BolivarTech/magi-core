# Changelog

All notable changes to `magi-core` are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-05-24

First stable release under SemVer. Closes parity with Python MAGI v3.0.0
(structured findings + agent finding-calibration prompts) and freezes the
public API. See `docs/migration-v1.0.md` for the upgrade guide.

### Added

- **`Category` enum** (`magi_core::schema::Category`) — controlled finding
  vocabulary (15 named slugs + `Other`), kebab-case serde with `#[serde(other)]`
  fallback. Parity with Python `finding_id.CATEGORY_SLUGS`.
- **`finding_id` module** (`magi_core::finding_id`) — `generate_finding_id`
  (stable SHA-256[:16] dedup key from `file`/`line`/`category`, cross-language
  parity with Python verified by golden vectors), `normalize_path`,
  `normalize_category`.
- **Structured findings** — `Finding` gains optional `file`, `line`, `category`
  fields (agent-reported, fail-soft deserialization). New `Finding::new` +
  `with_location` / `with_category` builders.
- **Id-aware consensus dedup** — co-located findings (file + line) merge by
  stable `finding_id`; unlocated findings merge by normalized title (unchanged).
  `DedupFinding` gains `file`/`line`/`category`/`id`.
- Agent prompts re-pinned to MAGI v3.0.0 (`62cf5801`): finding calibration
  (likelihood/downgrade, Caspar override) and optional `file`/`line`/`category`
  output fields. The 7 top-level keys are unchanged.

### Changed (breaking)

- **`#[non_exhaustive]`** on `Finding`, `AgentOutput`, `MagiReport`,
  `DedupFinding`, and `Category`. External crates can no longer use struct
  literals or exhaustive `match` on these types. Construct `Finding` via
  `Finding::new(...)`. Closed enums (`Verdict`, `Severity`, `AgentName`, `Mode`)
  remain exhaustively matchable.
- **`ClaudeRequest`, `ClaudeMessage`** (and `build_request_body`) dropped from
  `pub` to `pub(crate)` — HTTP request-shaping plumbing, never part of the
  analysis contract (were not re-exported from the prelude).

### Security

- `Finding.file` / `line` and `DedupFinding.id` are **agent-reported and NOT
  verified** against any source. The diff-grounded hallucination guard (Python
  MAGI v3.0.0 `finding_validation.py`) is deliberately a consumer concern, not a
  library feature (see ADR 004). Consumers building review tooling must validate
  located findings against their own diff before trusting them.

### Notes

- New runtime dependency: `sha2` (promoted from dev-dependency) for stable
  finding identity.
- API stability policy (ADR 005): 1.x minors add providers (Gemini, OpenAI) and
  fields additively; `2.0.0` is reserved for the next contract break.

## [0.6.0] - 2026-05-21

### Changed

- **Prose-wrapped agent JSON recovery** in `parse_agent_response` (Python
  MAGI v2.4.2 parity). When an agent wraps its verdict object in
  natural-language prose — before *and now after* the JSON — the parser
  recovers the embedded object instead of failing. The fast path (the
  whole string is the JSON, optionally fenced) is unchanged.
- **Fail closed on ambiguous recovery.** When two or more verdict-shaped
  objects are present (e.g. an agent quotes the schema example beside its
  real verdict), recovery returns no object so the agent fails closed and
  is retried — preventing a fabricated verdict from silently entering
  consensus. Selection is schema-aware via the `agent`/`verdict`
  discriminator keys, not by character span, so a large echoed tool-use
  document cannot shadow the real verdict.

### Security

- The recovery scan is bounded against oversized / adversarial input:
  input larger than 1 MB (`LENIENT_RECOVERY_MAX_BYTES`) skips recovery,
  and at most 2 000 brace positions are probed (`MAX_BRACE_PROBES`).
  Worst-case cost is the product of the probe cap and serde_json's
  recursion limit — both constants — so the scan stays O(1) in
  pathological input size, not O(n^2). Deeply nested input returns an
  error (serde recursion limit) rather than panicking.

### Backward compatibility

- **No public API change.** `parse_agent_response` and the new recovery
  helper/constants are private. Well-formed and preamble-wrapped outputs
  parse exactly as before. The only behavior changes are internal:
  trailing-prose output now succeeds, and ambiguous multi-verdict output
  now fails closed (previously one object was returned).

### Test count

`cargo nextest run --features test-utils` runs **393 tests** (up from
377 in v0.5.0). 16 new parser tests cover trailing prose (incl.
multi-byte UTF-8), fail-closed ambiguity (both orderings), the size and
probe-count bounds (including the exact byte-budget boundary),
truncated / partial objects, in-string brace echoes, and deeply-nested
no-panic.

### Pre-merge gates (CLAUDE.local.md §6)

- **Loop 1** `/requesting-code-review`: clean-to-go (1 iteration; 0
  critical, 0 important)
- **Loop 2** `/magi:magi`: STRONG GO unanimous — Melchior 90%,
  Balthasar 88%, Caspar 85%

## [0.5.0] - 2026-05-16

### Added

- **`MagiBuilder::with_complexity_gate(F)`** — caller-supplied predicate
  `Fn(&str, &Mode) -> bool + Send + Sync + 'static` evaluated by
  `Magi::analyze` after input-size validation but before LLM dispatch.
  When the predicate returns `false`, `analyze` short-circuits with
  `MagiError::SkippedByComplexityGate` and **zero LLM calls are made**.
  Useful for cost control (rate limiters, length thresholds, pre-flight
  triage). See `with_complexity_gate` docstring for evaluation order,
  panic/cost contracts, and composable patterns.
- **`MagiError::SkippedByComplexityGate { reason: String }`** new
  variant, marked `#[non_exhaustive]` so future structured fields
  (e.g., `content_len`, `mode`) can be added without breaking match
  patterns. The `reason` string is library-synthesized in the format
  `"complexity gate rejected: mode={mode}, content_len={N}"`. **This
  format is NOT part of the SemVer contract** — treat as human/log
  output only; count variant occurrences for structured logging.
- **Internal type alias `pub(crate) ComplexityGate`** —
  `Arc<dyn Fn(&str, &Mode) -> bool + Send + Sync>`. A `Result`-returning
  sibling alias may be added in v0.6+ if callers need predicate-supplied
  error context.

### Changed (breaking)

- **`MagiError` is now `#[non_exhaustive]`.** Downstream consumers that
  pattern-match exhaustively on `MagiError` MUST add a `_ => ...` arm.
  This closes the per-variant breaking-change pattern for all future
  releases — additions in v0.6+ will no longer require minor bumps.

### Performance

- Complexity gate path: when the predicate returns `false`, `analyze`
  returns immediately without instantiating the agent factory,
  generating a nonce, or calling any provider. The cost of a skipped
  call is the cost of the predicate plus one `format!` allocation for
  the synthesized reason.

### Documentation

- `with_complexity_gate` rustdoc enumerates the 3-step evaluation
  order (validate-first, then gate, then dispatch) with rationale
  for the order chosen (stateful predicates do not fire on oversize
  inputs).
- Variant doc on `SkippedByComplexityGate` documents the
  `#[non_exhaustive]` contract and instructs consumers to use
  `{ reason, .. }` rest pattern.

### Test count

`cargo nextest run --features test-utils` runs **377 tests** (up from
370 in v0.4.0). 7 new tests cover the gate's allow/block paths, the
content+mode propagation, the default no-gate v0.4.x backward compat,
the stateful rate-limiter use case, the synthesized reason format,
and the validate-first invariant (oversize inputs do not fire the
predicate's side effects).

### Backward compatibility

- All v0.4.x public APIs preserved. Default (no gate set) preserves
  v0.4.x behavior exactly — verified by a dedicated test using the
  `Magi::new` default-builder path.
- New `MagiError` variant means downstream exhaustive matchers must
  add a catch-all arm. Acceptable per project convention (v0.3 added
  `InvalidInput` similarly). Forward-compatible thanks to enum-level
  `#[non_exhaustive]` added in this release.

### Pre-merge gates (CLAUDE.local.md §6)

- **Loop 1** `/requesting-code-review`: clean-to-go (4 iterations)
- **Loop 2** `/magi:magi`: STRONG GO unanimous (2 iterations) — Melchior 92%, Balthasar 85%, Caspar 88%

## [0.4.0] - 2026-05-16

### Added

- **`default_model_for_mode(Mode) -> &'static str`** in `provider.rs`
  (Python v2.2.3 `MODE_DEFAULT_MODELS` parity). All three modes default
  to `"opus"` per Python v2.2.8. Pair with `resolve_claude_alias` to
  obtain the full model id. Re-exported from `prelude`.
- **`MagiReport.retried_agents: BTreeSet<AgentName>`** field — telemetry
  for agents whose first attempt failed schema/parse and were retried.
  Composes with `failed_agents` for two derived cohorts (recovered vs
  retry-also-failed). Serialized only when non-empty
  (`#[serde(skip_serializing_if)]`); default empty on deserialize.
- **`MagiReport` now derives `Deserialize`** (in addition to `Serialize`)
  to support backward-compatible loading of v0.3.x JSON.
- **`MagiBuilder::with_retry_disabled()`** opt-out for latency-sensitive
  deployments. When disabled, schema/parse errors go directly to
  `failed_agents` without a retry attempt (single-shot semantics).
- **`MagiConfig.retry_on_schema_error: bool`** (default `true`) gates
  the retry layer.
- **Cargo feature `test-utils`** exposing
  `magi_core::test_support::RoutingMockProvider` for downstream
  integration tests. Stable only within the v0.4.x line — see
  `docs/migration-v0.4.md`.
- **`examples/basic_analysis.rs`**: Windows console UTF-8 hardening
  (`setup_console_encoding` calls `SetConsoleOutputCP(CP_UTF8)` at
  startup). Failed calls surface a stderr warning. Compile-time guard
  test verifies the cfg-gating on both platforms.
- **`examples/basic_analysis.rs`**: when `--model` is omitted, uses
  `default_model_for_mode(mode)` (Python v2.2.3 parity).
- **ADR 002** `docs/adr/002-retry-on-schema-error.md` — retry mechanic,
  two-layer error sanitization, alternatives considered.
- **Migration guide** `docs/migration-v0.4.md`.

### Changed

- **Single-shot retry on `MagiError::Validation` and
  `MagiError::Deserialization`** during `Magi::analyze`. Agents whose
  first response fails schema or JSON parsing are retried once with a
  corrective prompt that preserves the original `BEGIN/END USER CONTEXT
  <nonce>` envelope verbatim and appends `---RETRY-FEEDBACK---` after
  the END delimiter. Python v2.2.0 + v2.2.4 parity. Provider errors
  (HTTP, network, timeout, auth, nested-session) skip retry — they're
  handled by the orthogonal `RetryProvider` layer.
- **Retry feedback error sanitization** (two-layer): `neutralize_headers`
  for line-start `MODE:` / `CONTEXT:` / `---BEGIN` / `---END` tokens,
  plus literal substring replace of `---RETRY-FEEDBACK---` (anywhere,
  not anchored — closes a regex gap where the trailing `---` lacks the
  expected separator). Prevents second-order injection via error
  strings.
- **Embedded agent prompts** bumped from `MAGI@v2.1.3` (commit
  `668f0e5e`) to `MAGI@v2.2.8` (commit `645932c7`). New prompts
  explicitly require the seven top-level JSON keys (`agent`, `verdict`,
  `confidence`, `summary`, `reasoning`, `findings`, `recommendation`).
- **`Magi.validator`** is now `Arc<Validator>` (was bare `Validator`)
  so the dispatch layer shares the compiled regexes across spawned
  tasks instead of deep-cloning per `analyze()` call.
- **`Agent::execute`** wraps the provider call in
  `CURRENT_AGENT_IDENTITY.scope(self.name, ...)` (a `pub(crate)`
  `tokio::task_local!`) so test-only providers can route responses
  per-agent without parsing the system prompt or polluting
  `CompletionConfig`. Production providers (Claude HTTP, Claude CLI)
  ignore the task-local — no observable behavior change.

### Backward compatibility

- All v0.3.1 public APIs preserved. v0.3.x JSON deserializes cleanly to
  v0.4.0 `MagiReport`; the new `retried_agents` field defaults to
  empty.
- `CompletionConfig` is unchanged from v0.3.1.

### Performance

- **Worst-case latency per agent doubles** when retry triggers (fresh
  `timeout` budget for each of the two attempts). If your application
  configures a custom timeout via `MagiBuilder::with_timeout(d)`, plan
  for 2×`d` as the effective ceiling per agent. Use
  `with_retry_disabled()` to restore v0.3.1 single-shot semantics.

### Documentation

- New ADR: `docs/adr/002-retry-on-schema-error.md`.
- New guide: `docs/migration-v0.4.md`.
- 19 new BDDs in `sbtdd/spec-behavior.md` (BDD-01..BDD-19) covering
  prompt SHA, default model, retry FSM (success / fail / no-retry on
  provider errors), telemetry serialization, backward-compat, anti
  injection invariants, AgentName Ord, Windows hardening.

### Test count

`cargo nextest run --features test-utils` runs **366 tests** (up from
324 in v0.3.1). The 42 new tests cover the retry FSM, retry
telemetry, the 2-layer error sanitization, the `test-utils` feature,
the AgentName Ord contract, and the v0.3.1 backward-compat fixture.

## [0.3.1] - 2026-04-19

### Fixed

- Align `opus` alias assertions in `ClaudeProvider` and `ClaudeCliProvider`
  test suites with the resolved model id `claude-opus-4-7`. The alias
  resolution itself was already correct in v0.3.0, but four test
  assertions and their accompanying docstrings still referenced the
  previous `claude-opus-4-6` value, causing the test suite to fail under
  `cargo nextest run --all-features`.

### Yanked

- **v0.3.0 is yanked.** It compiles and the runtime behavior matches
  v0.3.1, but its bundled test suite fails on `cargo test`. Consumers
  running the crate's tests (e.g., during dependency audits) see four
  unrelated failures. Upgrade to v0.3.1.

## [0.3.0] - 2026-04-18

### Changed (breaking)

- **Prompt architecture** consolidated from 9 mode-specific files to 3
  mode-agnostic prompts (one per agent). The `Mode` is now injected via
  the `user_prompt`, not the `system_prompt`. See
  `docs/migration-v0.3.md` and `sbtdd/spec-behavior.md` for the full
  change.
- **`MagiBuilder::with_custom_prompt(agent, mode, prompt)`** deprecated
  in favor of `with_custom_prompt_for_mode(agent, mode, prompt)`. A shim
  remains in place through v0.3.x; it will be removed in v0.4.0.
- **`Agent::new`** no longer takes a `Mode` parameter. The orchestrator
  resolves the system prompt via `lookup_prompt` and passes it to
  `Agent::execute` directly.
- **`user_prompt` format** changed. The payload sent to the LLM now
  follows the defense-in-depth pipeline from
  `docs/adr/001-prompt-injection-threat-model.md`:
  ```
  MODE: <mode>
  ---BEGIN USER CONTEXT <32-hex-nonce>---
  <sanitized content>
  ---END USER CONTEXT <32-hex-nonce>---
  ```
  Sanitization pipeline: `normalize_newlines` → `strip_invisibles` →
  `neutralize_headers` (3-layer defense-in-depth, order fixed).
  Consumers that inspect `user_prompt` via mocks must adjust their
  assertions.

### Added

- **`MagiBuilder::with_custom_prompt_for_mode`** — per-mode custom prompt
  override.
- **`MagiBuilder::with_custom_prompt_all_modes`** — mode-agnostic override
  (lookup order: per-mode → all-modes → embedded default).
- **`docs/adr/001-prompt-injection-threat-model.md`** — threat model and
  defense rationale for the prompt-injection hardening.
- **`MagiError::InvalidInput { reason }`** — returned from
  `build_user_prompt` when sanitized content contains the generated
  nonce (fail-closed; probability ~2^-128).
- **72 new unit tests** (pipeline + adversarial + integration + SHA-256
  parity). Total: 324.

### Security considerations (MAGI R3 W8)

The following limitations are **known and accepted** per the threat model
in `docs/adr/001-prompt-injection-threat-model.md` (Scope IS-NOT section):

- **Case-sensitive header matching.** `mode:`, `Mode:`, `MoDe:` are NOT
  neutralized by `neutralize_headers`. Only exact uppercase `MODE:`,
  `CONTEXT:`, `---BEGIN`, `---END` are matched. This preserves
  Python-MAGI parity. Consumers with stricter threat models must
  pre-filter input.
- **Non-ASCII whitespace.** U+00A0 (NBSP), U+3000 (Ideographic Space),
  and other non-ASCII whitespace characters before a header token are NOT
  absorbed by the regex — they may enable a bypass. Documented as an
  accepted gap in ADR 001; `INVISIBLE_AND_SEPARATOR_RE` omits them.
  Consumers must pre-filter if needed.
- **Nonce entropy ~64 bits.** `fastrand` has an effective state size of
  ~64 bits (not 128). The effective nonce collision probability is
  ~2^-64 per call rather than the theoretical 2^-128. This is acceptable
  per the threat model. A `pub(crate) with_rng_source` escape hatch is
  available for test injection.

### Dependencies

- New: `fastrand = "~2"` (non-cryptographic RNG for per-request nonce).
- New dev-dep: `sha2 = "0.10"` (fixture SHA-256 verification).

### Not included (deferred beyond v0.3.0)

- Verbose-markdown opt-in mode (restoring detail/reasoning paragraphs
  in rendered markdown). Deferred to v0.4+.
- Public `pub trait RngLike` — currently `pub(crate)`. Promote
  additively if a consumer requests it.

## [0.2.0] - 2026-04-18

### Changed (breaking)

- **Claude `opus` alias** now resolves to `claude-opus-4-7` (was `claude-opus-4-6`).
- **`Condition.condition`** is now sourced from `AgentOutput.summary` instead of
  `AgentOutput.recommendation`. Conditions are intended as short one-line blocking
  statements; full recommendations remain in the separate `recommendations` map.
- **`Validator`**: new `validate_mut(&self, &mut AgentOutput) -> Result<(), MagiError>`
  method. The orchestrator pipeline switched to use it so parsed agent outputs now
  flow through consensus with titles already cleaned in-place.
- **Consensus deduplication** no longer collapses interior whitespace. Titles
  differing by internal spacing (e.g., `"SQL injection"` vs `"SQL  injection"`)
  are now treated as distinct findings — aligned with Python MAGI 2.1.3. Dedup
  key uses NFKC normalization + full Unicode case-folding (`caseless` crate)
  instead of `to_lowercase()`.
- **`MagiConfig::max_input_len` default** raised from 1 MB (`1_048_576`) to
  4 MB (`4 * 1024 * 1024`). Consumers exposing the library to untrusted input
  should lower it via `MagiBuilder::with_max_input_len`. Full 10 MB alignment
  with Python is deferred to v0.3.0 pending allocation audit.
- **Report output (markdown)** changes:
  - `## Consensus Summary` section removed. Consumers parsing the rendered
    markdown should read `consensus.majority_summary` from the JSON instead.
  - Dissent section renders one line per dissenter with the `summary` field
    only (no `reasoning` paragraph). The `reasoning` field remains in JSON
    output.
  - Findings section renders one line per finding with fixed-width marker (5)
    and severity (14) columns. No indented detail paragraph; detail remains
    in JSON.
  - `GO WITH CAVEATS` consensus label now includes split count:
    `GO WITH CAVEATS (2-1)`.
  - `majority_summary` entries prefixed with agent display name:
    `"Melchior: <summary> | Balthasar: <summary>"`.
- **Banner rendering**: agent labels now column-aligned to the longest label
  so verdicts start at the same column. Labels that exceed the inner width (50)
  are truncated with `"..."` while preserving the verdict suffix.

### Security considerations

- **`max_input_len` default raised from 1 MB to 4 MB.** Consumers that expose
  `analyze()` to untrusted input should explicitly lower this via
  `MagiBuilder::with_max_input_len(1_048_576)` or similar. See `docs/migration-v0.2.md`
  for the allocation-envelope analysis (peak ≈ 5× content size during the 3-agent
  parallel dispatch; 4 MB default produces ~20 MB peak).
- **`Validator::validate_mut` silently rewrites `Finding.title` in place.** The
  orchestrator pipeline now uses `validate_mut`, so `MagiReport.agents[i].findings[j].title`
  reflects the *cleaned* form (NFKC-ready, invisible-char-stripped) rather than
  the raw LLM output. Consumers that need the raw form must preserve it before
  passing to `Magi::analyze`.

### Added

- **`clean_title`** public function in `validate` module: strips invisible
  Unicode characters and normalizes control whitespace (tabs, newlines, etc.)
  to a single space, matching Python MAGI 2.1.3 semantics.
- **`ReportConfig::new_checked`** constructor that validates ASCII on all
  `agent_titles` values, returning `Result<Self, ReportError>`.
- **`ReportError`** enum for structured reporting errors
  (`NonAsciiTitle { agent, field, value }` variant).
- **`BANNER_WIDTH`** and **`BANNER_INNER`** public constants on `reporting`
  module.
- **`DEFAULT_MAX_INPUT_LEN`** public constant on `orchestrator` module.
- **78 new unit tests** covering zero-width handling, NFKC+casefold, banner
  alignment, fit_content edge cases, dedup ordering, and more. Total test
  count: 250 (up from 172).

### Deprecated

- **`Finding::stripped_title`** is now `#[deprecated(since = "0.2.0")]`. The
  method still exists as a shim over `validate::clean_title`, but with a
  **different character coverage** than v0.1.x — it now strips the Python
  MAGI `_ZERO_WIDTH_RE` set (U+200B-U+200F, U+2028-U+202F, U+2060-U+206F,
  U+FEFF, U+00AD) instead of the v0.1.x `ZERO_WIDTH_PATTERN` set (which
  covered Arabic/Syriac/Mongolian format marks). See
  `docs/migration-v0.2.md` for the full comparison. The method will be
  removed in v0.3.0.

### Dependencies

- New: `unicode-normalization = "~0.1.24"` (NFKC for dedup key).
- New: `caseless = "~0.2.2"` (full Unicode case-folding for dedup key,
  equivalent to Python `str.casefold()`).

### Not included (deferred to v0.3.0)

- **Prompt architecture consolidation** (9 prompt files → 3 mode-agnostic +
  prompt-injection hardening). Tracked in
  `planning/claude-plan-tdd-v0.3-prompts.md`.

## [0.1.2] - 2026-04-05

- Initial public release. 172 tests. MAGI review STRONG GO (unanimous, round 3).
