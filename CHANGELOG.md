# Changelog

All notable changes to `magi-core` are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [3.0.0] - 2026-07-29

**The verdict sentinel.** An agent's verdict is now read only from between two marker
lines, each alone on its own line, exactly once:

```
<MAGI_VERDICT>
{ ...the 7-key JSON object... }
</MAGI_VERDICT>
```

Text outside the markers is never parsed. This replaces a parser that *searched* the raw
response for a JSON object shaped like a verdict — a fast path that deserialized the whole
response, plus a scan over candidate objects that selected the one carrying the verdict's
discriminator keys, bounded by a size limit and a probe cap. Searching means choosing, and
choosing means some input is chosen wrong; the worst case was a **fabricated verdict in the
adversarial seat** — an `approve`/`conditional` no model ever formed, entering consensus as
if it were an opinion. That code is deleted outright: no flag, no environment variable, no
Cargo feature restores it. A second, unplanned win: "thinking" models that restate their
schema while reasoning used to produce two verdict-shaped objects, fail closed, and drop a
mage that had actually answered — reasoning now lives outside the markers and can no longer
compete with the verdict.

### BREAKING

- **`MagiBuilder::build()` now rejects any custom prompt that does not carry the marker
  block.** Every resolvable prompt is checked — the three built-in ones and every override,
  including prompts loaded via `with_prompts_dir` — and the check runs **before any provider
  is contacted**; no request is sent for a prompt that fails it. The new
  `MagiError::PromptContract` variant reports which prompt (agent and mode, or an
  unassigned one) and which rule it broke. **Only consumers using `with_custom_prompt*` /
  `with_prompts_dir` are affected** — the built-in prompts already satisfy the check, so a
  consumer that never overrides a prompt sees no change. No signature moved and no item was
  removed, so nothing breaks at the type level; what breaks is *behaviour* — a configuration
  that used to build now returns `Err` — and that is a break under plain SemVer regardless
  of what the type system says, hence a major and not a minor.

  **The fix:** start from `prompts::caspar_prompt()` (now public) and edit it, or copy its
  `## Output format` section into your own prompt verbatim, then check the result with the
  also-public `prompts::validate_prompt` — the exact function `build()` runs. There is
  deliberately no automatic fixer: appending the marker section to a legacy prompt produces
  one that contradicts itself (one half forbids text outside the JSON, the other invites the
  model to reason freely before the markers), and that prompt passes the check while
  performing worse than either half on its own. Migrating means *removing* the old
  instruction, not layering a new one over it.

  If your provider forces `response_format` / structured JSON output, that is a dead end the
  sentinel cannot help with — a model forced to emit raw JSON cannot wrap it in markers.
  Staying on `2.x` is not a fix either: that is the version with the fabrication hole still
  open. The provider has to stop forcing structured output.

### Added

- **`verdict_markers` module** (public, narrow): `VERDICT_OPEN` / `VERDICT_CLOSE`,
  `extract`, `VerdictExtractionError`, `ExtractionFailureCause`. Pure, allocation-light,
  never panics — every failure is a typed `Err`. No new dependencies.
- **`prompts` module is now public**: the three prompt accessors plus `validate_prompt` and
  `validate_prompt_for`. It had to be, or the new build-time check would be a wall with no
  door. Internal resolution helpers stay crate-private.
- **Retry feedback is now selected by the typed failure cause** — seven distinct
  instructions instead of one generic paragraph. A model told it wrote the markers twice can
  fix that; a model told only "re-emit valid JSON" cannot. The old generic instruction also
  told models to emit nothing outside the JSON, which now contradicts the contract.
- **`MagiReport::extraction_failures: BTreeMap<AgentName, Vec<ExtractionFailure>>`** —
  always present and seeded for every dispatched agent, so an empty `Vec` is a positive
  certificate that the seat adhered on every attempt. Each record carries the **model** that
  produced the rejected output, the attempt number, and the cause: with per-agent rotation
  the actionable question is *which model* fails to adhere, not *which seat* happened to
  suffer. A `MagiReport` produced by 2.2.0 still deserializes — the field defaults to empty.
- A `## Extraction Failures` section in the human-readable report, emitted only when at
  least one attempt failed. A clean run's report text is unchanged.
- **Post-validation checks:** an echoed worked example is rejected (`EchoedExample`), and a
  verdict claiming to come from another mage is rejected (`AgentIdentity`). When both would
  fire, the echo is reported — it names the root cause, and its feedback subsumes the other.
- CI now mechanically enforces the "never search outside the markers" rule
  (`ci/check_r0.sh`), so a future change cannot quietly reintroduce the deleted scan.

### Changed

- The three embedded system prompts are re-pinned to the reference implementation's
  sentinel-era prompts, applied verbatim with no local divergence. The worked example moved
  outside the marker block and a non-JSON placeholder took its place inside — which is what
  makes an echo harmless rather than merely less severe.

## [2.2.0] - 2026-07-27

Hardening of invisible-character stripping and of header neutralization. The set
of characters removed from untrusted content and from finding titles is now
defined **by Unicode category** instead of by a hand-written list, and the header
neutralizer no longer depends on that set at all.

**Why a minor and not a patch.** No signature moved and no item was removed, so
this began as a `2.1.1` patch. It ships as a minor under a policy this release
adopts: **a change to the observable output of a public item is a contract
change, and ships as a minor at minimum** — even when the new behavior conforms
*better* to that item's documented contract, as it does here. "It's a bug fix"
does not make an output change a patch, because a consumer's expectations are
set by the behavior they observed, not by our reading of our own docs. A
security motivation is a reason to ship the change, not a reason to hide it in a
patch.

### Fixed

- **Unicode tag characters are now stripped from untrusted content.**
  `U+E0001` and `U+E0020`–`U+E007F` (category `Cf`) encode readable ASCII that is
  invisible to a human reviewing the content, yet many LLM tokenizers emit them
  as text — the classic covert channel for prompt injection. They were absent
  from the enumerated set and survived sanitization intact.
- **`U+180E` MONGOLIAN VOWEL SEPARATOR could bypass header neutralization.**
  It was reclassified `Zs` → `Cf` in Unicode 6.3, so it matched neither the
  strip set nor `\s`. A `MODE<U+180E>:` line in user content therefore passed
  through unprefixed while a model could still read it as a real header.
- Both gaps also let the affected characters reach finding-title dedup, so two
  titles differing only by an invisible were treated as distinct.

- **The header-neutralization bypass is closed at its cause, on both flanks.**
  A blank-rendering character placed next to a reserved keyword used to make the
  neutralizer's pattern fail while a model still read the line as a header.
  Stripping cannot fix this — it removes only the code points someone
  remembered to enumerate, so the next unassigned one reopens the hole. Both
  flanks now require a **non-letter**: the leading group went from `[\t ]*` to
  `[^A-Za-z\r\n]*` and the trailing group from `(\s|:|$)` to `([^A-Za-z]|$)`.
  A bypass would need a character that is invisible *and* an ASCII letter,
  which is a contradiction, so the defense also covers code points that do not
  exist yet. This closes the documented `U+00A0` limitation (MAGI R3 W7).
  Word-continuation discrimination is unchanged — `MODESTY`, `CONTEXTUAL`,
  `---BEGINNING` and `MODEL:` are still not treated as headers.
- The neutralizer now **deliberately over-neutralizes**: any line whose first
  letters are a reserved keyword flanked by non-letters gets two spaces
  inserted immediately before the keyword. Besides `MODE_SELECT`,
  `CONTEXT-free`, `MODE1:` and `MODEÉ`, this reaches shapes common in reviewed
  content — `- MODE: x` becomes `-   MODE: x`, and likewise `| MODE | v |`,
  `> MODE: x`, `"MODE":`, `### MODE`, `2. CONTEXT switch`. Cosmetic on a
  content line, and the intended trade against a sanitizer that can be walked
  past.

### Changed

- The stripping pattern is now `[\p{Cf}\u{2028}\u{2029}\u{2065}\u{202F}]` —
  the exhaustive `Cf` category plus four code points that are deliberately in
  scope but are **not** `Cf`: `U+2028`/`U+2029` (`Zl`/`Zp`), `U+202F` (`Zs`) and
  `U+2065` (`Cn`, unassigned). A category does not age; the list had.
- **Observable behavior of the public `validate::clean_title` changes**: it now
  removes 143 additional invisible code points — tag characters, Arabic and
  Syriac format marks, Egyptian hieroglyph joiners, and musical beam/slur
  controls among them. Two finding titles that differ only by such a character
  now deduplicate together, where previously they did not; only **unlocated**
  findings are affected, since a located finding deduplicates by
  `(file, line, category)` and never routes its title through this path.
  No previously stripped code point stopped being stripped — the new pattern is
  a strict superset, pinned by a non-regression test over the old set.
- `Mn` (nonspacing marks) is **deliberately excluded** — stripping it would
  destroy combining accents in legitimate text. `U+00A0` NBSP remains untouched.
- Diverges from the Python reference, which still enumerates the set; the
  reference is to follow.

## [2.1.0] - 2026-07-27

Per-agent lineage **rotation**: a dead model rotates to another lineage instead
of degrading the run. Fully additive — a consumer that declares no fallbacks
compiles and behaves exactly as 2.0.x.

### Added

- **Per-agent rotation.** Declare a per-mage primary `Lineage`
  (`MagiBuilder::with_agent` / `with_probing_agent`) and a shared, run-wide
  `FallbackPool` (`with_fallback_pool`, `max_rotations`). On a surfaced transport
  error (after the MS1 `RetryProvider` exhausts) or a schema failure surviving its
  corrective retry, the mage rotates to the next eligible lineage. A transport
  failure condemns the lineage run-wide; a schema failure is mage-local; a panic
  never rotates.
- **`MagiError::EndpointDown { lineages }`** — a new variant (permitted by the
  existing `#[non_exhaustive]`): two DISTINCT connection-level (`Network`)
  failures fast-fail the run **before** consensus. `Http 5xx` / `Timeout` /
  `RetryAbandoned` condemn a lineage but do NOT count toward this threshold.
- **Rotation telemetry on `MagiReport`.** A new **always-present** JSON key,
  `rotations: BTreeMap<AgentName, AgentRotation>`, is populated for every agent
  (successful and failed) on every run — strict/exhaustive JSON consumers should
  expect it. Each `AgentRotation` carries `model_configured`, `model_used`, the
  ordered `chain` of `RotationEvent` hops (empty when the mage did not rotate),
  and `ran_unmeasured`. When some mage rotated, the human report gains a
  `## Model Rotations` section (`⟲ <agent> rotated: <from> → <to> (<kind>)`); with
  no rotation the text output is byte-identical to 2.0.x.
- **`ProviderProbe` trait + `OllamaProvider` (feature `ollama`).** An optional
  capability trait (context `window` + weights `digest`), separate from
  `LlmProvider`. `OllamaProvider` reuses the OpenAI-compatible completions path
  and adds the native probe: **window** from `POST /api/show`, **digest** from
  `GET /api/tags`. A concurrent, timeout-bounded preflight caches the results so
  rotation eligibility reads them with zero I/O. The digest verify is **fail-open**
  — only two lineages resolving to the SAME digest are rejected; an unresolvable
  digest is trusted by the declared lineage.

### Compatibility

- **Default (no fallbacks declared) preserves 2.0.x behavior** — same dispatch
  FSM, same failure strings, no registry, no endpoint-down. No regression.
- No breaking changes; no new runtime dependencies (`tracing`/`reqwest` already
  present). The `ollama` feature enables `openai-compat`.

## [2.0.0] - 2026-07-25

Backoff/retry hardening plus a deliberate audit of the public API's
extensibility, detailed in the sections below.

### BREAKING

- **`ProviderError` is `#[non_exhaustive]`** (the enum and its struct-like
  variants). External `match` needs a `_ =>` arm and `..` when destructuring.
  This is what lets `Http` carry the server's `Retry-After`, impossible in 1.x.
- **`ConsensusResult`, `Dissent`, `Condition` are `#[non_exhaustive]`.** Reading
  them is unchanged; struct-literal construction from another crate is not.
- **`RetryProvider` no longer exposes public fields.** The removed `pub
  max_retries` / `pub base_delay` are now set through an immutable `RetryConfig`
  (`with_config(inner, RetryConfig)`); read them via `provider.config()`.
- **A 300 s total client timeout where there was none before.** A `complete()`
  call that today takes 400 s and works will, after the upgrade, **fail with
  `ProviderError::Timeout`**. This is the only plausible regression — raise it
  with the provider's `with_timeout` constructor. See the migration guide.
- **A `Retry-After` in date form (or otherwise uninterpretable) now ABANDONS the
  retry** (`ProviderError::RetryAbandoned`) instead of being silently ignored.
- **`ClaudeProvider::map_status_to_error` gains `retry_after_raw` / `received_at`
  parameters** (its visibility stays `pub`).

### Added

- `backoff` module: capped exponential backoff, full jitter, and a
  delta-seconds `Retry-After` parser (total functions — never panic).
- `RetryConfig` with `cap`, `retry_after_cap`, `operation_budget`, and
  `flat_classes` (flat vs exponential backoff per failure class).
- `operation_budget` (default 10 min) bounding the total retry time, and typed
  abandonment (`AbandonReason`) that says *why* retrying stopped.
- `with_timeout` constructors on both HTTP providers; `DEFAULT_CLIENT_TIMEOUT`.
- `tracing` (new runtime dependency) to announce dangerous configurations and
  budget exhaustion — no-op without a subscriber.

### Changed

- `is_retryable` now also treats 408/502/503/504 as transient (local server
  cold-start).
- Waits are no longer the deterministic 1s/2s/4s progression — they carry full
  jitter, so a test asserting exact delays will need updating.

**Worst-case latency with the defaults: ~15 minutes** per `complete()` call
(10 min `operation_budget` + one 5 min timeout). Wrap the call in
`tokio::time::timeout` if you need a harder bound.

## [1.1.1] - 2026-07-17

### Fixed

- **Fabrication-echo hardening (F0).** The worked example embedded in each of the
  three agent prompts carried `"verdict": "approve"`; a model echoing the example
  verbatim could fabricate a clean `approve` in the adversarial (Caspar) seat —
  the worst silent failure the consensus can produce. The example now uses
  `"conditional"` (an echo surfaces as `GO WITH CAVEATS`, visible), matching the
  Python MAGI plugin's prompts from v5.1.0 onward. Pinned by a
  whitespace-normalized property test. This degrades the residual's severity; the
  durable fix (verdict sentinel) is scheduled as its own release.

### Changed

- Prompt re-pin tooling: local divergences from the pinned Python reference are
  now declared once (`tests/fixtures/_magi_ref.py::DIVERGENCES`) and applied
  automatically by both the extractor and the hash-fixture generator, with a
  fail-loud occurrence check — re-pinning is a single command again. The parity
  test was renamed to `test_prompts_match_pinned_reference_sha256`.
- `clippy` 1.97 `unnecessary_sort_by` lint fixed in the consensus engine
  (behavior-preserving; stable severity ordering retained).

## [1.1.0] - 2026-05-25

### Added

- **`OpenAiCompatibleProvider`** (feature `openai-compat`): one provider for OpenAI
  cloud and any OpenAI-compatible local server (Ollama, LocalAI, vLLM, LM Studio,
  llama.cpp-server) via a configurable `base_url`. Pass-through model, optional
  bearer auth, errors mapped to existing `ProviderError` variants. Re-exported in
  the prelude. `basic_analysis` gains `--provider openai-compat --base-url`.

### Notes

- The provider uses `max_tokens` (broad compat with local servers); OpenAI
  reasoning models (o1/o3) that require `max_completion_tokens` are not supported.
- The `reqwest::Client` sets no internal timeout; bound request duration via the
  orchestrator's per-agent timeout.

## [1.0.1] - 2026-05-25

### Fixed

- **`finding_key` rejects a non-positive `line`.** A finding built with
  `Finding::new(...).with_location(file, 0)` now deduplicates by title (no stable
  `id`) instead of producing an `id` over an invalid 1-based line. This aligns the
  `with_location` builder path with `de_opt_line`, which already maps non-positive
  lines to `None`. Only affects the degenerate `line == 0` case set directly via
  the builder; the deserialize path was already correct.

### Internal

- Deduplicated `MAGI_REF_SHA` (and `MAGI_PATH`, the agent set, and the `git show`
  blob reader) into a single `tests/fixtures/_magi_ref.py` imported by both
  `gen_` and `extract_magi_ref_prompts.py` — re-pinning the reference prompts now
  edits one file. No effect on the published crate.

## [1.0.0] - 2026-05-24

First stable release under SemVer. Closes parity with Python MAGI v3.0.0
(structured findings + agent finding-calibration prompts) and freezes the
public API.

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
  library feature. Consumers building review tooling must validate
  located findings against their own diff before trusting them.

### Notes

- New runtime dependency: `sha2` (promoted from dev-dependency) for stable
  finding identity.
- API stability policy: 1.x minors add providers and fields additively;
  `2.0.0` is reserved for the next contract break.

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
  integration tests. Stable only within the v0.4.x line.
- **`examples/basic_analysis.rs`**: Windows console UTF-8 hardening
  (`setup_console_encoding` calls `SetConsoleOutputCP(CP_UTF8)` at
  startup). Failed calls surface a stderr warning. Compile-time guard
  test verifies the cfg-gating on both platforms.
- **`examples/basic_analysis.rs`**: when `--model` is omitted, uses
  `default_model_for_mode(mode)` (Python v2.2.3 parity).

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

- 19 new BDDs (BDD-01..BDD-19) covering
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
  the `user_prompt`, not the `system_prompt`.
- **`MagiBuilder::with_custom_prompt(agent, mode, prompt)`** deprecated
  in favor of `with_custom_prompt_for_mode(agent, mode, prompt)`. A shim
  remains in place through v0.3.x; it will be removed in v0.4.0.
- **`Agent::new`** no longer takes a `Mode` parameter. The orchestrator
  resolves the system prompt via `lookup_prompt` and passes it to
  `Agent::execute` directly.
- **`user_prompt` format** changed. The payload sent to the LLM now
  follows the defense-in-depth pipeline:
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
- **`MagiError::InvalidInput { reason }`** — returned from
  `build_user_prompt` when sanitized content contains the generated
  nonce (fail-closed; probability ~2^-128).
- **72 new unit tests** (pipeline + adversarial + integration + SHA-256
  parity). Total: 324.

### Security considerations (MAGI R3 W8)

The following limitations are **known and accepted** per the threat model
(Scope IS-NOT section):

- **Case-sensitive header matching.** `mode:`, `Mode:`, `MoDe:` are NOT
  neutralized by `neutralize_headers`. Only exact uppercase `MODE:`,
  `CONTEXT:`, `---BEGIN`, `---END` are matched. This preserves
  Python-MAGI parity. Consumers with stricter threat models must
  pre-filter input.
- **Non-ASCII whitespace.** U+00A0 (NBSP), U+3000 (Ideographic Space),
  and other non-ASCII whitespace characters before a header token are NOT
  absorbed by the regex — they may enable a bypass. Documented as an
  accepted gap; `INVISIBLE_AND_SEPARATOR_RE` omits them.
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
  `MagiBuilder::with_max_input_len(1_048_576)` or similar. The allocation
  envelope is peak ≈ 5× content size during the 3-agent parallel dispatch;
  4 MB default produces ~20 MB peak.
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
  covered Arabic/Syriac/Mongolian format marks). The method will be
  removed in v0.3.0.

### Dependencies

- New: `unicode-normalization = "~0.1.24"` (NFKC for dedup key).
- New: `caseless = "~0.2.2"` (full Unicode case-folding for dedup key,
  equivalent to Python `str.casefold()`).

### Not included (deferred to v0.3.0)

- **Prompt architecture consolidation** (9 prompt files → 3 mode-agnostic +
  prompt-injection hardening).

## [0.1.2] - 2026-04-05

- Initial public release. 172 tests. MAGI review STRONG GO (unanimous, round 3).
