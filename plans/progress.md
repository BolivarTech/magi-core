# Ralph Progress Log

Started: 2026-04-05
Project: magi-core
Methodology: SBTDD (Spec + Behavior + Test Driven Development)

## Codebase Patterns

- Rust crate: magi-core
- Error type: MagiError (thiserror)
- Test runner: cargo nextest via python run-tests.py
- Spec: sbtdd/spec-behavior.md
- Plan: planning/claude-plan-tdd.md
- 12 tasks total in prd.json

## Key Files

- src/lib.rs — crate root (currently empty scaffold)
- sbtdd/spec-behavior.md — SDD + BDD specification
- planning/sections/ — per-task TDD stubs

---

## 2026-04-05 - Session Notes

### Setup

**What was done:**
- Installed Ralph loops (Paso 10 of SBTDD autonomous flow)
- Created commands, hooks, scripts, guardrails, progress log

**Files created:**
- .claude/commands/ralph-loop.md
- .claude/commands/ralph-cancel.md
- .claude/commands/ralph-planner.md
- .claude/hooks/stop-hook.sh
- scripts/ralph/ralph.sh
- scripts/ralph/ralph-stop.sh
- scripts/ralph/ralph-status.sh
- scripts/ralph/ralph-tail.sh
- plans/guardrails.md
- plans/progress.md

**Learnings:**
- prd.json uses .tasks[] with .status field (not .features[] with .passes)
- Verification command: python run-tests.py
- TDD-Guard hooks already configured in .claude/settings.json

---

### Task 001: Error types foundation (COMPLETED)

**What was done:**
- Created `src/error.rs` with `ProviderError` (6 variants) and `MagiError` (6 variants)
- `ProviderError`: Http, Network, Timeout, Auth, Process, NestedSession — all with `thiserror::Error` derives
- `MagiError`: Validation, Provider, InsufficientAgents, Deserialization, InputTooLarge, Io
- `From` impls: `ProviderError` (#[from]), `std::io::Error` (#[from]), `serde_json::Error` (manual impl converting to string)
- Updated `src/lib.rs` to declare `pub mod error;`
- 13 tests: Display assertions for all variants + From conversion tests

**Key decisions:**
- `MagiError` cannot derive `PartialEq` because `std::io::Error` doesn't implement it — tests use `matches!` macro
- `serde_json::Error` → `MagiError::Deserialization(String)` via manual `From` impl (can't use `#[from]` since variant stores String, not the error)
- `ProviderError` derives `Clone` (needed by downstream); `MagiError` does not (io::Error is not Clone)

**Files modified:**
- src/error.rs (created)
- src/lib.rs (replaced scaffold with `pub mod error;`)

**Verification:** 13/13 tests pass, clippy clean, fmt clean, release build clean, docs clean

---

### Task 002: Domain schema types (COMPLETED)

**What was done:**
- Created `src/schema.rs` with 4 enums and 2 structs:
  - `Verdict`: Approve/Reject/Conditional — `weight()`, `effective()`, Display uppercase, serde lowercase
  - `Severity`: Critical/Warning/Info — `icon()`, manual `Ord` (Critical > Warning > Info), serde lowercase
  - `Mode`: CodeReview/Design/Analysis — Display kebab-case, serde kebab-case
  - `AgentName`: Melchior/Balthasar/Caspar — `title()`, `display_name()`, manual `Ord` alphabetical, serde lowercase
  - `Finding`: severity/title/detail — `stripped_title()` removes Unicode Cf chars via regex
  - `AgentOutput`: agent/verdict/confidence/summary/reasoning/findings/recommendation — `is_approving()`, `is_dissenting()`, `effective_verdict()`
- Added `regex = "1"` to Cargo.toml
- Updated `src/lib.rs` with `pub mod schema;`
- 39 tests covering all behaviors, serde roundtrips, ordering, deserialization rejection

**Key decisions:**
- `AgentOutput` does NOT use `#[serde(deny_unknown_fields)]` — spec requires ignoring unknown fields
- `AgentOutput` derives `PartialEq` but not `Eq`/`Hash` because it contains `f64` (confidence)
- `Severity::Ord` implemented manually (rank function) because derived Ord uses discriminant order which would give wrong direction
- `AgentName::Ord` implemented via `display_name()` string comparison for guaranteed alphabetical order
- `Finding::stripped_title()` compiles regex per call (spec says Validator in section 03 will precompile and reuse)

**Files modified:**
- src/schema.rs (created)
- src/lib.rs (added `pub mod schema;`)
- Cargo.toml (added `regex = "1"`)

**Verification:** 52/52 tests pass, clippy clean, fmt clean, release build clean, docs clean

---

### Task 003: Validation (COMPLETED)

**What was done:**
- Created `src/validate.rs` with `ValidationLimits` and `Validator` structs
- `ValidationLimits`: `#[non_exhaustive]`, Debug/Clone, Default with spec values (100, 500, 10_000, 50_000, 0.0, 1.0)
- `Validator`: holds `ValidationLimits` + precompiled `Regex` for Unicode Cf stripping
  - `new()` / `with_limits()` constructors
  - `validate(&self, &AgentOutput) -> Result<(), MagiError>` — fail-fast validation in order: confidence, summary, reasoning, recommendation, findings
  - `validate_confidence` — uses `!(val >= min && val <= max)` pattern to naturally reject NaN/Infinity
  - `validate_text_field` — generic check with field name in error message
  - `validate_findings` — count check + per-finding validation
  - `validate_finding` — strips zero-width chars first, then checks emptiness and length
  - `strip_zero_width` — uses precompiled regex (not per-call like `Finding::stripped_title()`)
- Updated `src/lib.rs` with `pub mod validate;`
- 21 tests covering: constructors, BDD-10 (confidence range), BDD-11 (zero-width title), BDD-12 (text length), NaN/Infinity, findings count/title/detail limits, validation order, title length after strip

**Key decisions:**
- `Validator` implements `Default` (delegates to `new()`) to satisfy clippy
- Confidence validation uses `!(a >= b && a <= c)` instead of `a < b || a > c` to catch NaN naturally (NaN comparisons always return false)
- Title length is checked on the stripped version, not the raw string (per acceptance criteria)
- Error messages include field names and limit values for diagnostics

**Files modified:**
- src/validate.rs (created)
- src/lib.rs (added `pub mod validate;`)

**Verification:** 73/73 tests pass, clippy clean, fmt clean, release build clean, docs clean

---

### Task 004: Consensus engine (COMPLETED)

**What was done:**
- Created `src/consensus.rs` with `ConsensusConfig`, `ConsensusEngine`, `ConsensusResult`, and supporting structs (`DedupFinding`, `Dissent`, `Condition`)
- `ConsensusConfig`: `#[non_exhaustive]`, Debug/Clone, Default (min_agents=2, epsilon=1e-9), min_agents clamped to 1 if 0
- `ConsensusEngine`: stateless `determine(&self, &[AgentOutput])` method implementing the full consensus algorithm:
  1. Input validation (count, duplicates)
  2. Normalized score computation from verdict weights
  3. Majority verdict determination with alphabetical tiebreaking
  4. Epsilon-aware classification into labels (STRONG GO, GO (n-m), GO WITH CAVEATS, HOLD -- TIE, HOLD (n-m), STRONG NO-GO)
  5. Degraded mode capping (< 3 agents: STRONG GO → GO, STRONG NO-GO → HOLD)
  6. Confidence formula: base * weight_factor, clamped [0,1], rounded 2 decimals
  7. Finding deduplication by case-insensitive stripped title, severity promotion, detail from highest severity (or first agent by Ord on tie)
  8. Dissent tracking, condition extraction, votes map, majority summary, recommendations map
- 25 tests covering all BDD scenarios (1,2,3,4,5,13,33), error cases, score/confidence calculations, deduplication edge cases

**Key decisions:**
- `ConsensusEngine` implements `Default` (delegates to `new(ConsensusConfig::default())`)
- Classification is extracted into private `classify()` helper for readability
- Deduplication is extracted into private `deduplicate_findings()` helper
- Agent-finding pairs are sorted by `AgentName::Ord` before grouping to ensure deterministic tiebreaking
- Findings in result sorted by severity descending (Critical first)
- `HashSet` used internally for duplicate detection; `BTreeMap` for all output maps
- `majority_verdict` variable tracks the binary majority for dissent/summary filtering; `consensus_verdict` from classification may differ (e.g. HOLD -- TIE uses Reject)

**Files modified:**
- src/consensus.rs (created)
- src/lib.rs (added `pub mod consensus;`)

**Verification:** 98/98 tests pass, clippy clean, fmt clean, release build clean, docs clean

---

### Task 005: Reporting and MagiReport (COMPLETED)

**What was done:**
- Created `src/reporting.rs` with `ReportConfig`, `ReportFormatter`, and `MagiReport`
- `ReportConfig`: `#[non_exhaustive]`, Debug/Clone, Default (banner_width=52, standard MAGI agent titles in BTreeMap)
- `ReportFormatter`: holds `config` + `banner_inner` (config.banner_width - 2)
  - `new()` / `with_config()` constructors
  - `format_banner()` — fixed-width 52-char ASCII verdict box with agent lines and consensus label
  - `format_init_banner()` — pre-analysis initialization box with mode/model/timeout
  - `format_report()` — full markdown report: banner + consensus summary + optional findings/dissent/conditions + recommendations
  - Private helpers: `format_separator()`, `format_line()` (with truncation), `agent_display()` (config lookup with fallback to AgentName methods)
  - Section methods: `format_consensus_summary()`, `format_findings()`, `format_dissent()`, `format_conditions()`, `format_recommendations()`
- `MagiReport`: Debug/Clone/Serialize with agents, consensus, banner, report, degraded, failed_agents
- Updated `src/lib.rs` with `pub mod reporting;`
- 20 tests covering: BDD-15 (banner width 52 chars), BDD-16 (all 5 markdown headers), optional section omission, banner structure, init banner, separator format, agent line format, findings/dissent/conditions/recommendations formatting, agent display fallback, MagiReport serialization, degraded flag, JSON lowercase agent names, confidence passthrough

**Key decisions:**
- Banner uses `std::fmt::Write` for string building (no allocation overhead from concatenation)
- `format_line()` truncates content that exceeds banner_inner width (preserves 52-char invariant)
- Optional sections (findings, dissent, conditions) are entirely omitted when data is empty
- `agent_display()` falls back to `AgentName::display_name()` and `AgentName::title()` when not in config
- Confidence rounding is NOT done by MagiReport — it's the consensus engine's responsibility (already handled in task 004)
- `MagiReport` only derives `Serialize` (not `Deserialize`) — it's output-only

**Files modified:**
- src/reporting.rs (created)
- src/lib.rs (added `pub mod reporting;`)

**Verification:** 118/118 tests pass, clippy clean, fmt clean, release build clean, docs clean

---

### Task 006: LlmProvider trait and RetryProvider (COMPLETED)

**What was done:**
- Created `src/provider.rs` with `LlmProvider` async trait, `CompletionConfig`, and `RetryProvider`
- `LlmProvider`: async trait (via `async-trait`) with `Send + Sync` bounds for `Arc<dyn LlmProvider>` usage with `tokio::spawn`
  - Methods: `complete()`, `name()`, `model()`
- `CompletionConfig`: `#[non_exhaustive]`, Debug/Clone, Default (max_tokens=4096, temperature=0.0)
- `RetryProvider`: wraps `Arc<dyn LlmProvider>` with configurable retry logic
  - `new()` with defaults (3 retries, 1s delay), `with_config()` for custom settings
  - Implements `LlmProvider` itself — transparent to consumers
  - Retries on: Timeout, Network, Http 500, Http 429
  - Does NOT retry on: Auth, Process, NestedSession, Http 4xx (except 429)
  - Returns last error after exhausting retries
- Added `async-trait = "0.1"` and `tokio = { version = "1", features = ["time"] }` to Cargo.toml
- Added `tokio` dev-dependency with `macros` + `rt-multi-thread` features for `#[tokio::test]`
- Updated `src/lib.rs` with `pub mod provider;`
- 15 tests: CompletionConfig defaults, RetryProvider delegation (name/model), retry on transient errors (Timeout, Http 500, Http 429, Network), no-retry on non-transient errors (Auth, Process, NestedSession, Http 4xx), exhaustion, success on first retry, default config values

**Key decisions:**
- `is_retryable()` is a private standalone function (not a method) — simple match on ProviderError variants
- Guard condition for Http status moved to match arm body (Rust `matches!` macro has issues with guards across alternation arms)
- `RetryProvider` loop uses `0..=max_retries` for `1 + max_retries` total attempts
- `last_error` tracked but not used in practice since the loop always returns from within
- `expect()` on `last_error` is unreachable dead code (loop always returns Err before exiting)

**Files modified:**
- src/provider.rs (created)
- src/lib.rs (added `pub mod provider;`)
- Cargo.toml (added async-trait, tokio dependencies)

**Verification:** 133/133 tests pass, clippy clean, fmt clean, release build clean, docs clean

---

### Task 007: Agents and AgentFactory (COMPLETED)

**What was done:**
- Created `src/agent.rs` with `Agent` and `AgentFactory` structs
- Created `src/prompts/` module with `mod.rs`, `melchior.rs`, `balthasar.rs`, `caspar.rs`
- Created 9 system prompt markdown files in `src/prompts_md/` (3 agents x 3 modes)
- `Agent`: three constructors (`new`, `with_custom_prompt`, `from_file`), `execute` async method, 7 accessor methods
- `AgentFactory`: `new`, `with_provider`, `with_custom_prompt`, `from_directory`, `create_agents`
- `create_agents` always returns `[Melchior, Balthasar, Caspar]` in fixed order
- Updated `src/lib.rs` with `pub mod agent;` and `mod prompts;`
- 15 tests covering: BDD-26 (each agent uses own provider), BDD-27 (default + override), BDD-30 (distinct prompts per mode), BDD-31 (from_directory io error), from_file io error, constructors, accessors, factory creation, ordering, all modes, JSON+English constraints in prompts

**Key decisions:**
- `prompts` module is `mod prompts;` (private) — only used internally by `agent.rs`
- `Mode` passed by value (it's `Copy`) rather than by reference as in spec — cleaner API
- `from_directory` verifies directory existence first via `read_dir`, then scans for individual files
- `MockProvider` uses `AtomicUsize` for thread-safe call counting in async tests
- Prompt markdown files define: agent identity, perspective, mode-specific focus areas, constraints (English, JSON-only), exact JSON schema

**Files created:**
- src/agent.rs
- src/prompts/mod.rs, melchior.rs, balthasar.rs, caspar.rs
- src/prompts_md/ (9 markdown files)

**Files modified:**
- src/lib.rs (added `pub mod agent;`, `mod prompts;`)

**Verification:** 148/148 tests pass, clippy clean, fmt clean, release build clean, docs clean

---

### Task 008: Orchestrator (COMPLETED)

**What was done:**
- Created `src/orchestrator.rs` with `MagiConfig`, `MagiBuilder`, `Magi`, `build_prompt()`, and `parse_agent_response()`
- `MagiConfig`: `#[non_exhaustive]`, Debug/Clone, Default (timeout=300s, max_input_len=1MB, completion=default)
- `MagiBuilder`: consuming builder pattern with all configuration options (providers, prompts, timeouts, limits, configs)
  - `build()` returns `Result<Magi, MagiError>` (can fail if `prompts_dir` is set and unreadable)
- `Magi`: main entry point composing `AgentFactory`, `Validator`, `ConsensusEngine`, `ReportFormatter`
  - `new()` convenience constructor using all defaults (safe unwrap — no I/O)
  - `builder()` returns a `MagiBuilder`
  - `analyze()` full orchestration: input validation → create agents → build prompt → launch parallel → parse → validate → consensus → report
  - `launch_agents()` uses `tokio::task::JoinSet` for cancellation safety — dropping the JoinSet aborts all spawned tasks
  - Each agent task wrapped in `tokio::time::timeout`
  - `process_results()` separates successes/failures, enforces min_agents=2 threshold
- `parse_agent_response()`: strips code fences, finds JSON boundaries (first `{` to last `}`), deserializes AgentOutput
- `build_prompt()`: formats `"MODE: {mode}\nCONTEXT:\n{content}"`
- Updated `src/lib.rs` with `pub mod orchestrator;`
- Updated `Cargo.toml` tokio features: added `rt` for JoinSet
- 15 tests: BDD-01 (unanimous approve), BDD-06 (1 timeout degraded), BDD-07 (bad JSON degraded), BDD-08 (2 fail insufficient), BDD-09 (all fail), BDD-14 (plain text as failure), BDD-28 (new with defaults), BDD-29 (builder with overrides), BDD-32 (input too large), config defaults, build_prompt format, parse code fences, parse preamble JSON, parse invalid fails, builder build ok

**Key decisions:**
- `launch_agents()` takes ownership of `Vec<Agent>` (not borrowed) so each Agent can be moved into a spawned `'static` future — Agent is `Send` because all fields are Send
- JoinSet used instead of `join_all` for automatic cancellation safety per spec
- MockProvider uses `Vec<Result<String, ProviderError>>` with cycling index to return different responses per agent call — enables testing mixed success/failure scenarios
- `process_results` enforces `min_agents=2` hardcoded (matches ConsensusConfig default) — the consensus engine also validates this independently
- JoinSet task panics are caught and mapped to `ProviderError::Process` with a fallback agent name

**Files modified:**
- src/orchestrator.rs (created)
- src/lib.rs (added `pub mod orchestrator;`)
- Cargo.toml (added `rt` feature to tokio)

**Verification:** 163/163 tests pass, clippy clean, fmt clean, release build clean, docs clean

---

### Task 009: ClaudeProvider HTTP API (COMPLETED)

**What was done:**
- Created `src/providers/` module with `mod.rs` and `claude.rs`
- `ClaudeProvider`: implements `LlmProvider` via `reqwest` HTTP client to Claude Messages API
  - `new(api_key, model)` constructor — creates `reqwest::Client` once for connection pooling
  - `complete()` sends POST to `/v1/messages` with `x-api-key`, `anthropic-version` headers
  - `build_request_body()` — constructs `ClaudeRequest` with model, max_tokens, temperature, system, messages
  - `parse_response()` — extracts first text content block from Claude API response JSON
  - `map_status_to_error()` — maps 401/403 to `ProviderError::Auth`, others to `ProviderError::Http`
  - Custom `Debug` impl that redacts API key (`[REDACTED]`)
- Feature-gated behind `claude-api` feature flag in Cargo.toml
- Added `reqwest = { version = "0.12", features = ["json"], optional = true }` to dependencies
- Updated `src/lib.rs` with `pub mod providers;`
- 14 tests covering: construction, accessors, request body building, response parsing (valid, multiple blocks, empty, invalid JSON), error mapping (401, 403, 429, 500), client reuse, Debug redaction

**Key decisions:**
- Helper structs (`ClaudeRequest`, `ClaudeMessage`, `ClaudeResponse`, `ContentBlock`) are defined in the module — request structs are `pub` for testability, response structs are private
- `parse_response` and `map_status_to_error` are associated functions (not methods) for easy unit testing without HTTP mocking
- `name()` and `model()` exist as both inherent methods and trait impl methods (trait requires them)
- `ClaudeResponse` uses `#[serde(rename = "type")]` for the `type` field in `ContentBlock` since `type` is a Rust keyword
- No HTTP integration tests — all tests verify construction, serialization, and parsing logic only

**Files created:**
- src/providers/mod.rs
- src/providers/claude.rs

**Files modified:**
- src/lib.rs (added `pub mod providers;`)
- Cargo.toml (added `reqwest` optional dep, `[features]` section with `claude-api`)

**Verification:** 177/177 tests pass (163 base + 14 ClaudeProvider), clippy clean, fmt clean, release build clean, docs clean

---
