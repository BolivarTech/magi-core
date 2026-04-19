# Implementation Plan: magi-core

## 1. What We're Building

**magi-core** is a Rust library crate that provides multi-perspective analysis using
three independent LLM agents (Melchior/Scientist, Balthasar/Pragmatist, Caspar/Critic).
Each agent analyzes the same content from a different perspective, then a consensus
engine synthesizes their verdicts into a unified report.

The crate is LLM-agnostic: a `LlmProvider` trait abstracts the backend, allowing
users to plug in any LLM (Claude, Gemini, OpenAI, local models) via HTTP APIs or
CLI subprocesses. Each agent can use a different provider.

**Target users**: Rust developers who want automated multi-perspective code review,
design evaluation, or general analysis powered by LLMs.

**API style**: reqwest/tokio conventions. Builder pattern, sensible defaults, strongly
typed. A simple use case is 3 lines of code:

```rust
let magi = Magi::new(Arc::new(provider));
let report = magi.analyze(&Mode::CodeReview, content).await?;
println!("{}", report.report);
```

---

## 2. Architecture Overview

### Module Dependency Graph

```
error.rs  ← (all modules depend on this)
    ↑
schema.rs ← validate.rs ← consensus.rs
                         ← reporting.rs
    ↑
provider.rs ← agent.rs ← orchestrator.rs → consensus.rs
                                          → validate.rs
                                          → reporting.rs
    ↑
providers/  (feature-gated, each behind its own feature flag)
```

**Key principle**: Pure logic modules (schema, validate, consensus, reporting) have
zero async or network dependencies. Only `provider.rs`, `agent.rs`, `orchestrator.rs`,
and the `providers/` directory use async/tokio.

### Directory Structure

```
magi-core/
  src/
    lib.rs                    # Crate root, module declarations, re-exports
    prelude.rs                # Common types for `use magi_core::prelude::*`
    error.rs                  # MagiError + ProviderError enums
    schema.rs                 # Verdict, Severity, Mode, AgentName, Finding, AgentOutput
    validate.rs               # Validator + ValidationLimits
    consensus.rs              # ConsensusEngine + ConsensusConfig + ConsensusResult
    reporting.rs              # ReportFormatter + ReportConfig + MagiReport
    provider.rs               # LlmProvider trait + CompletionConfig + RetryProvider
    agent.rs                  # Agent + AgentFactory
    orchestrator.rs           # Magi + MagiBuilder + MagiConfig
    providers/
      mod.rs                  # Conditional re-exports based on features
      claude.rs               # ClaudeProvider (feature: claude-api)
      claude_cli.rs           # ClaudeCliProvider (feature: claude-cli)
      gemini.rs               # v1.1
      gemini_cli.rs           # v1.1
      openai.rs               # v1.2
    prompts/
      mod.rs                  # Module declarations
      melchior.rs             # include_str! from .md files, per-mode constants
      balthasar.rs
      caspar.rs
    prompts_md/               # Raw .md files compiled into binary
      melchior_code_review.md
      melchior_design.md
      melchior_analysis.md
      balthasar_code_review.md
      ... (9 files total, 3 agents x 3 modes)
  examples/
    basic_analysis.rs         # Example: ClaudeCliProvider by default, CLI args for other providers
  Cargo.toml
```

### Dependencies

| Crate | Version | Justification | Features |
|-------|---------|---------------|----------|
| tokio | 1 | Async runtime for parallel agents, subprocess management, timeouts | process, sync, time, macros, rt-multi-thread |
| serde | 1 | Serialize/Deserialize for all domain types and JSON I/O | derive |
| serde_json | 1 | JSON parsing of LLM responses and report serialization | — |
| async-trait | 0.1 | Required for `dyn LlmProvider` — native async traits don't support dyn dispatch | — |
| thiserror | 2 | Derive `Error` + auto-generated `Display` via `#[error("...")]` for MagiError/ProviderError | — |
| regex | 1 | Precompiled pattern for zero-width Unicode char stripping in Validator | — |
| tracing | 0.1 | Structured logging — zero-cost no-op without subscriber (spec-compliant: not "direct logging") | — |
| reqwest | 0.12 | HTTP client for API providers (optional, feature-gated) | json |
| mockall | 0.13 | (dev-only) Auto-generate mocks from LlmProvider trait | — |

**Feature flags** (nothing enabled by default):
- `claude-api` — enables `ClaudeProvider` (pulls `reqwest`)
- `claude-cli` — enables `ClaudeCliProvider` (no extra deps)
- `gemini-api` — enables `GeminiProvider` (v1.1, pulls `reqwest`)
- `gemini-cli` — enables `GeminiCliProvider` (v1.1)
- `openai-api` — enables `OpenAiProvider` (v1.2, pulls `reqwest`)

---

## 3. Implementation Sections

### Section 1: Foundation — Error Types and Domain Schema

**Files**: `error.rs`, `schema.rs`

**Why first**: Every other module depends on these types. They're pure data with no
external dependencies, making them ideal to start with and test independently.

#### error.rs

Two error enums with `thiserror` derives:

- **`ProviderError`**: Http (status + body), Network, Timeout, Auth, Process (exit_code + stderr), NestedSession. `Display` generated by `thiserror`'s `#[error("...")]` attributes (not manual impl).
- **`MagiError`**: Validation(String), Provider(ProviderError), InsufficientAgents { succeeded, required }, Deserialization(String), InputTooLarge { size, max }, Io(std::io::Error). `Display` via `thiserror`. Implement `From<ProviderError>`, `From<serde_json::Error>`, `From<std::io::Error>`.

No `panic!` anywhere. All error paths return `Result`.

#### schema.rs

**Enums with encapsulated behavior:**

- `Verdict` — Approve, Reject, Conditional. Methods: `weight() -> f64` (+1.0, -1.0, +0.5), `effective() -> Verdict` (Conditional maps to Approve for majority counting), `Display` (uppercase).
- `Severity` — Critical, Warning, Info. Implements `Ord` (Critical > Warning > Info), `icon() -> &str` ("[!!!]", "[!!]", "[i]"), `Display`.
- `Mode` — CodeReview, Design, Analysis. `Display` outputs "code-review", "design", "analysis".
- `AgentName` — Melchior, Balthasar, Caspar. Methods: `title() -> &str`, `display_name() -> &str`, `Ord` (alphabetical for deterministic tiebreak).

**Structs:**

- `Finding` — severity, title, detail. Method: `stripped_title()` using regex to remove zero-width Unicode.
- `AgentOutput` — agent, verdict, confidence, summary, reasoning, findings, recommendation. Methods: `is_approving()`, `is_dissenting(majority)`, `effective_verdict()`.

All types derive: Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash. Enums used as map keys (`AgentName`) additionally need `Ord` and `PartialOrd` (already specified for `AgentName` and `Severity`).

**Serde customization**: Verdict and Severity serialize as lowercase strings ("approve", "critical"). AgentName serializes as lowercase. Use `#[serde(rename_all = "lowercase")]` or custom impl.

---

### Section 2: Validation

**File**: `validate.rs`

**Depends on**: schema.rs, error.rs

#### ValidationLimits

`#[non_exhaustive]` config struct with fields: max_findings (100), max_title_len (500), max_detail_len (10_000), max_text_len (50_000), confidence_min (0.0), confidence_max (1.0). Implements `Default`.

#### Validator

Holds `limits: ValidationLimits` and `zero_width_pattern: Regex` (precompiled once in constructor for Unicode category Cf characters).

Methods:
- `new()` — default limits, compile regex once
- `with_limits(limits)` — custom limits
- `validate(&self, output: &AgentOutput) -> Result<(), MagiError>` — calls sub-validators in order: confidence, summary, reasoning, recommendation, findings
- Private methods: `validate_confidence`, `validate_text_field`, `validate_findings`, `validate_finding`, `strip_zero_width`

Each validation failure returns `MagiError::Validation` with a descriptive message including the field name for diagnostics.

---

### Section 3: Consensus Engine

**File**: `consensus.rs`

**Depends on**: schema.rs, error.rs

This is the core algorithm. The engine is **stateless** — `determine(&self)` takes agent outputs and returns everything in `ConsensusResult`. No mutable state between calls.

#### ConsensusConfig

`#[non_exhaustive]` with `min_agents: usize` (default 2, enforced >= 1 to prevent division by zero), `epsilon: f64` (default 1e-9).

#### ConsensusEngine

Holds `config: ConsensusConfig`.

**Main method**: `determine(&self, agents: &[AgentOutput]) -> Result<ConsensusResult, MagiError>`

Algorithm:
1. Verify at least `config.min_agents` agents (else InsufficientAgents error)
2. Reject duplicate agent names (Validation error)
3. Compute normalized score: `sum(v.weight()) / len`
4. Determine majority via `effective()` mapping (Conditional->Approve). Binary {Approve, Reject} — no 3-way split possible. Ties broken by `AgentName::cmp()`.
5. Classify score to label + verdict using epsilon-aware float comparison
6. Compute confidence: `base = sum(majority_conf) / num_agents`, `weight_factor = (abs(score) + 1) / 2`, `confidence = base * weight_factor`, clamped [0.0, 1.0], rounded 2 decimals
7. Deduplicate findings: group by title (case-insensitive), promote severity, keep detail from highest-severity finding (or first agent on tie: Melchior > Balthasar > Caspar), track sources
8. Identify majority/dissent sides
9. Build ConsensusResult

**Classification rules** (all comparisons use epsilon):
- `|score - 1.0| < eps` → "STRONG GO" / Approve
- `|score - (-1.0)| < eps` → "STRONG NO-GO" / Reject
- `score > eps` with conditionals → "GO WITH CAVEATS" / Approve
- `score > eps` no conditionals → "GO (N-M)" / Approve
- `|score| < eps` → "HOLD -- TIE" / Reject
- `score < -eps` → "HOLD (N-M)" / Reject

**Degraded mode cap**: when `agent_count < 3`, STRONG GO → GO (N-0), STRONG NO-GO → HOLD (N-0).

#### ConsensusResult

Struct with: consensus (String label), consensus_verdict, confidence, score, agent_count, votes (HashMap), majority_summary, dissent (Vec<Dissent>), findings (Vec<DedupFinding>), conditions (Vec<Condition>), recommendations (HashMap).

Supporting structs: `DedupFinding` (extends Finding with sources), `Dissent` (agent, summary, reasoning), `Condition` (agent, condition).

---

### Section 4: Reporting

**File**: `reporting.rs`

**Depends on**: schema.rs, consensus.rs

#### ReportConfig

`#[non_exhaustive]` with `banner_width: usize` (52), `agent_titles: BTreeMap<AgentName, (String, String)>` (display_name, title).

#### ReportFormatter

Holds `config: ReportConfig` and `banner_inner: usize` (calculated as `banner_width - 2`).

**Public methods:**
- `format_banner(&self, agents, consensus)` — ASCII verdict box, fixed width 52 chars
- `format_init_banner(&self, mode, model, timeout_secs)` — pre-analysis initialization box
- `format_report(&self, agents, consensus)` — full markdown report (banner + all sections)

**Banner format** (exactly 52 chars wide per line):
```
+==================================================+
|          MAGI SYSTEM -- VERDICT                  |
+==================================================+
|  Melchior (Scientist):  APPROVE (90%)            |
|  Balthasar (Pragmatist):  CONDITIONAL (85%)      |
|  Caspar (Critic):  REJECT (78%)                  |
+==================================================+
|  CONSENSUS: GO WITH CAVEATS                      |
+==================================================+
```

**Report sections** (in order):
1. Banner
2. `## Consensus Summary` — majority summaries joined by " | "
3. `## Key Findings` (if any) — `{icon} **[{SEVERITY}]** {title} _(from {sources})_`
4. `## Dissenting Opinion` (if any) — agent name + summary + full reasoning
5. `## Conditions for Approval` (if any) — bulleted list
6. `## Recommended Actions` — per-agent recommendations

---

### Section 5: LlmProvider Trait and CompletionConfig

**File**: `provider.rs`

**Depends on**: error.rs

#### LlmProvider Trait

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, system_prompt: &str, user_prompt: &str, config: &CompletionConfig) -> Result<String, ProviderError>;
    fn name(&self) -> &str;
    fn model(&self) -> &str;
}
```

Uses `async-trait` crate because native async traits don't support `dyn Trait` as of Rust 1.85. The trait requires `Send + Sync` for `Arc<dyn LlmProvider>` usage with `tokio::spawn`.

#### CompletionConfig

`#[non_exhaustive]` with `max_tokens: u32` (default 4096), `temperature: f64` (default 0.0). No timeout field — timeout is managed by `MagiConfig.timeout` via `tokio::time::timeout`.

---

### Section 6: Agents and AgentFactory

**File**: `agent.rs`

**Depends on**: schema.rs, provider.rs, error.rs, prompts/

#### Agent

Each agent is an autonomous unit with its own provider. Fields: `name: AgentName`, `mode: Mode`, `system_prompt: String`, `provider: Arc<dyn LlmProvider>`.

Constructors:
- `new(name, mode, provider)` — auto-generates system prompt from `include_str!` prompts
- `with_custom_prompt(name, mode, provider, prompt)` — user-provided prompt
- `from_file(name, mode, provider, path)` — loads prompt from filesystem

`execute(&self, user_prompt, config) -> Result<String, ProviderError>` delegates to `self.provider.complete()`.

Accessor methods: `name()`, `mode()`, `system_prompt()`, `provider_name()`, `provider_model()`, `display_name()`, `title()`.

#### AgentFactory

Creates sets of 3 agents. Fields: `default_provider: Arc<dyn LlmProvider>`, `agent_providers: BTreeMap<AgentName, Arc<dyn LlmProvider>>`, `custom_prompts: BTreeMap<AgentName, String>`.

Builder-style methods: `with_provider(name, provider)`, `with_custom_prompt(name, prompt)`, `from_directory(dir)`.

`create_agents(&self, mode) -> Vec<Agent>` creates all 3 agents, using per-agent provider override or default, and custom prompts where registered.

#### System Prompts (prompts/ module)

9 markdown files in `src/prompts_md/` (3 agents x 3 modes), compiled via `include_str!`. Each prompt module (melchior.rs, balthasar.rs, caspar.rs) exposes `fn prompt_for_mode(mode: &Mode) -> &'static str`.

Prompt structure follows the Python original:
- Agent identity and analytical lens
- Input format (MODE + CONTEXT)
- Focus areas per mode
- Personality traits
- Constraints (English, JSON-only output, field length guidelines)
- Exact JSON schema to produce

---

### Section 7: Orchestrator — Magi, MagiBuilder, MagiConfig

**File**: `orchestrator.rs`

**Depends on**: all other modules

This is the main entry point. `Magi` composes all components.

#### MagiConfig

`#[non_exhaustive]` with: `timeout: Duration` (300s), `max_input_len: usize` (1_048_576 = 1MB, measured in bytes — UTF-8 encoded), `completion: CompletionConfig`. No retry field — retry is the user's responsibility via `RetryProvider` wrapper (see Section 9).

#### MagiBuilder

Consuming builder (`mut self` methods). Required field: `default_provider` (passed to constructor). Optional: per-agent providers, custom prompts, prompts_dir, config, completion override, validation_limits, consensus_config, report_config.

`build(self) -> Result<Magi, MagiError>` — returns Result for future-proofing (e.g., `prompts_dir` failures, config validation). Currently succeeds if required provider is present. Assembles: AgentFactory, Validator, ConsensusEngine, ReportFormatter from accumulated config.

#### Magi

Fields: config, agent_factory, validator, consensus_engine, formatter.

**`analyze(&self, mode, content) -> Result<MagiReport, MagiError>`** orchestrates:

1. Validate `content.len() <= config.max_input_len` (InputTooLarge if exceeded)
2. `agent_factory.create_agents(mode)` — 3 agents, each with own provider
3. `formatter.format_init_banner(mode, ...)` — tracing event with init banner
4. `build_prompt(mode, content)` — "MODE: {mode}\nCONTEXT:\n{content}"
5. `launch_agents(agents, prompt)` — uses `tokio::task::JoinSet` (not raw tokio::spawn) so that dropping the `analyze` future automatically aborts all spawned agent tasks, preventing resource leaks and wasted LLM API quota. Each task is wrapped in `tokio::time::timeout(config.timeout)`.
6. `parse_agent_response(raw)` — strips code fences, finds JSON object boundaries (first `{` to last `}`), then deserializes to `AgentOutput` via `serde_json::from_str`. Handles common LLM output quirks (markdown fences, preamble text before JSON, extra whitespace). Serde ignores unknown fields by default.
7. `validator.validate(&output)` per successful agent
8. `process_results(results)` — separate successes/failures, check minimum agents
9. `consensus_engine.determine(&successful)` — compute consensus
10. `formatter.format_report(&successful, &consensus)` — generate markdown
11. Construct and return `MagiReport`

**Degradation**: 2/3 OK → continue with `degraded: true`. <2 OK → `MagiError::InsufficientAgents`.

---

### Section 8: MagiReport

**File**: `reporting.rs` (merged with ReportFormatter — one module for all report types)

**Depends on**: schema.rs, consensus.rs

Simple struct: `agents: Vec<AgentOutput>`, `consensus: ConsensusResult`, `banner: String`, `report: String`, `degraded: bool`, `failed_agents: Vec<AgentName>`.

Derives: Serialize, Clone, Debug. JSON output matches the Python original format exactly (lowercase agent names, confidence rounded to 2 decimals, etc.).

---

### Section 9: RetryProvider (Opt-in Wrapper)

**File**: `provider.rs` (alongside LlmProvider trait)

**Depends on**: error.rs

The spec prohibits automatic retry at the orchestrator level ("No debe hacer retry automatico a nivel de orquestador"). Instead, retry is exposed as an **opt-in wrapper** that users compose with any `LlmProvider`.

#### RetryProvider

A struct that wraps any `Arc<dyn LlmProvider>` and adds retry logic. Implements `LlmProvider` itself, so it's transparent to the orchestrator.

Fields: `inner: Arc<dyn LlmProvider>`, `max_retries: u32` (default 3), `base_delay: Duration` (default 1s).

The `complete()` implementation retries on transient errors (HTTP 500, 429, Timeout, Network). Auth and Process errors are NOT retryable. Uses `tokio::time::sleep` between retries. Returns the last error after all retries exhausted.

Usage:
```rust
let provider = Arc::new(RetryProvider::new(Arc::new(ClaudeProvider::new(key, model))));
let magi = Magi::new(provider); // retry is transparent
```

This design keeps the orchestrator simple (spec-compliant) while providing retry as a user-controlled composition pattern.

---

### Section 10: ClaudeProvider (HTTP API)

**File**: `providers/claude.rs`

**Feature**: `claude-api`

**Depends on**: provider.rs, error.rs, reqwest

Struct with: `client: reqwest::Client` (reused, connection pooling), `api_key: String`, `model: String`.

Constructor: `new(api_key, model)` — creates a `reqwest::Client` with default timeouts and `x-api-key` header set via `set_sensitive(true)`.

`complete()` implementation:
- POST to `https://api.anthropic.com/v1/messages`
- Headers: `x-api-key`, `anthropic-version: 2023-06-01`, `content-type: application/json`
- Body: `{"model": model, "max_tokens": config.max_tokens, "temperature": config.temperature, "system": system_prompt, "messages": [{"role": "user", "content": user_prompt}]}`
- Check response status, map non-2xx to `ProviderError::Http`
- Parse response, extract content text block
- Return the text string

---

### Section 11: ClaudeCliProvider (CLI Subprocess)

**File**: `providers/claude_cli.rs`

**Feature**: `claude-cli`

**Depends on**: provider.rs, error.rs, tokio::process

Struct with: `model: String`, `model_id: String`.

Constructor: `new(model) -> Result<Self, ProviderError>`
- Model alias whitelist: "opus" → "claude-opus-4-6", "sonnet" → "claude-sonnet-4-6", "haiku" → "claude-haiku-4-5-20251001"
- Pass-through: any string containing "claude-" accepted as-is
- Reject unknown strings with `ProviderError::Auth`
- Check `CLAUDECODE` env var → `ProviderError::NestedSession` if present (fail-fast)

`complete()` implementation:
- `tokio::process::Command::new("claude")` with args: `["--print", "--output-format", "json", "--model", model_id, "--system-prompt", system_prompt]`
- All stdio piped, `kill_on_drop(true)`
- Write user prompt to stdin, then `drop(stdin)` to signal EOF
- Wrap `child.wait_with_output()` in `tokio::time::timeout`
- On timeout: kill child, return `ProviderError::Timeout`
- Check exit status: non-zero → `ProviderError::Process` with stderr
- Parse double-nested JSON: outer envelope `CliOutput { is_error, result }`, check is_error, extract `result` string, strip code fences, return

**CliOutput** helper struct (private, Deserialize): handles the outer JSON envelope from Claude CLI.

**Security**: `tokio::process::Command::new("claude")` bypasses the shell (calls executable directly), preventing shell injection even with user-controlled prompts. Document this mitigation in Rustdoc.

**Windows limitation**: `child.kill()` uses TerminateProcess which doesn't propagate to grandchild processes. Document this as a known limitation.

---

### Section 12: Prelude and Crate Root

**Files**: `lib.rs`, `prelude.rs`

`prelude.rs` re-exports the most commonly needed types so users can do `use magi_core::prelude::*`.

`lib.rs` declares all modules, conditionally compiles provider modules based on features, and re-exports key types at crate root level.

---

## 4. Build Sequence

Implementation should follow this order based on the dependency graph:

| Phase | Modules | Rationale |
|-------|---------|-----------|
| 1 | error.rs | Foundation — every module depends on error types |
| 2 | schema.rs | Core domain types, pure data, no dependencies except error |
| 3 | validate.rs | Depends only on schema + error, tests regex and limits |
| 4 | consensus.rs | Core algorithm, depends on schema + error |
| 5 | reporting.rs (+ MagiReport) | Formatting + report struct, depends on schema + consensus |
| 6 | provider.rs (+ RetryProvider) | Trait definition + CompletionConfig + opt-in retry wrapper |
| 7 | agent.rs + prompts/ | Agent struct + factory + prompt files, depends on schema + provider |
| 8 | orchestrator.rs | Main entry point, composes everything, uses JoinSet |
| 9 | providers/claude.rs | HTTP provider (feature-gated) |
| 10 | providers/claude_cli.rs | CLI provider (feature-gated) |
| 11 | prelude.rs + lib.rs | Final wiring, re-exports |
| 12 | examples/basic_analysis.rs | Example: defaults to ClaudeCliProvider, supports CLI args for provider selection |

Phases 1-5 are pure logic (no async). Phases 6-8 introduce async. Phases 9-10 add external dependencies. Phases 11-12 are wiring and docs.

---

## 5. Testing Strategy

### Per-Module Tests

Each module has `#[cfg(test)] mod tests` with unit tests covering:
- All public methods
- Edge cases from BDD scenarios
- Error paths

### Mock Strategy

Use `mockall` to auto-generate `MockLlmProvider` from the `LlmProvider` trait. Mock providers return predefined JSON strings for each agent. This enables testing the full orchestration flow without any LLM or network calls.

### BDD Scenario Mapping

The 33 BDD scenarios from the spec map to specific test functions:

| Scenario | Module | Test Focus |
|----------|--------|------------|
| 1-5 | consensus.rs | Consensus classification (unanimous, mixed, tie) |
| 6-9 | orchestrator.rs | Degradation and insufficient agents |
| 10-12 | validate.rs | Validation error paths |
| 13 | consensus.rs | Finding deduplication |
| 14 | orchestrator.rs | JSON parse failure handling |
| 15-16 | reporting.rs | Banner width and report sections |
| 17, 25 | providers/ | Provider-specific behavior (v1.2) |
| 18-23 | providers/claude_cli.rs | CLI provider behavior |
| 24, 31 | providers/, agent.rs | Error paths |
| 26-27 | agent.rs | Mixed providers |
| 28-29 | orchestrator.rs | Builder pattern |
| 30 | agent.rs | Mode-specific prompts |
| 32 | orchestrator.rs | InputTooLarge |
| 33 | consensus.rs + orchestrator.rs | Degraded label cap |

### TDD Enforcement

Every module follows Red-Green-Refactor:
1. **Red**: Write failing tests from BDD scenarios + edge cases
2. **Green**: Minimum implementation to pass
3. **Refactor**: Clean up, add docs, extract helpers

TDD-Guard hooks enforce this at the tooling level.

---

## 6. Key Decisions and Trade-offs

| Decision | Choice | Why |
|----------|--------|-----|
| `async-trait` over native | Native doesn't support `dyn Trait` yet | Required for `Arc<dyn LlmProvider>` |
| `include_str!` for prompts | Prompts are `.md` files compiled into binary | Easy maintenance without touching Rust source |
| Stateless ConsensusEngine | `determine(&self)` returns all state in result | Thread-safe, no mutable state, each `analyze()` independent |
| Separate feature flags | `claude-api` vs `claude-cli` | Users may want CLI for dev without pulling reqwest |
| RetryProvider wrapper (not orchestrator) | Opt-in composition, user wraps their provider | Spec-compliant: "No retry automatico a nivel de orquestador" |
| `tracing` for logging | Zero-cost when unused, requires subscriber | Not "direct logging" — compatible with spec constraint |
| `JoinSet` for agent tasks | Dropped JoinSet auto-aborts spawned tasks | Cancellation safety — prevents resource leaks |
| `BTreeMap` over `HashMap` | Deterministic key ordering in serialized JSON | Reproducible output, matches Python dict insertion order |
| `build() -> Result` | Future-proof builder return type | Allows fallible config validation in future versions |
| Fixed 52-char banner | Matches Python original exactly | Visual parity with existing MAGI output |
| epsilon-aware floats | All score comparisons use 1e-9 epsilon | Prevents floating-point edge cases in classification |
| `#[non_exhaustive]` on configs | All public config structs | Adding fields in future versions won't break downstream |
| Strict validation (no clamping) | Invalid LLM output is rejected, not "fixed" | Library should not mask LLM problems; user gets clear errors |
| `max_input_len` in bytes | `str::len()` is UTF-8 bytes, not chars | Natural Rust behavior, correlates with LLM token estimation |

---

## 7. Constraints Checklist

- [ ] No `panic!`, `unwrap()`, `expect()`, `unreachable!()`, `todo!()` outside `#[cfg(test)]`
- [ ] No `unsafe`
- [ ] All public APIs have Rustdoc with `///`
- [ ] File headers: `// Author: Julian Bolivar`, `// Version: 1.0.0`, `// Date: YYYY-MM-DD`
- [ ] `rustfmt` + `clippy --tests -- -D warnings` clean
- [ ] All error paths return `Result<T, MagiError>` or `Result<T, ProviderError>`
- [ ] OOP: struct + impl + trait, real state in structs, SRP per method
- [ ] Named constants for all magic numbers (weights, banner width, limits)
- [ ] Features: nothing enabled by default
