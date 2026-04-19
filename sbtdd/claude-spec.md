# magi-core: Complete Specification

> Synthesized from: `sbtdd/spec-behavior.md` (SDD+BDD spec, MAGI STRONG GO),
> `sbtdd/claude-research.md` (codebase + best practices research),
> `sbtdd/claude-interview.md` (implementation interview).

---

## 1. Overview

**magi-core** is a Rust crate (`lib`) that provides an LLM-agnostic multi-perspective
analysis system. It launches three independent sub-agents (Melchior, Balthasar, Caspar)
in parallel against any LLM provider, collects structured JSON verdicts, computes
weighted consensus, and generates a unified report with deduplicated findings,
dissenting opinions, and recommendations.

The system is a conceptual port of the Python MAGI plugin for Claude Code, redesigned
to be provider-agnostic via the `LlmProvider` trait.

### Key Design Decisions

| Decision | Choice | Source |
|----------|--------|--------|
| Async trait dispatch | `async-trait` crate (native async traits don't support `dyn` as of Rust 1.85) | Research |
| Provider sharing | `Arc<dyn LlmProvider>` (Clone is cheap, Send+Sync for tokio::spawn) | Spec + Research |
| Builder pattern | Consuming `mut self`, reqwest/tokio style | Spec + Interview |
| Feature flags | Separate: `claude-api`, `claude-cli`, `gemini-api`, `gemini-cli`, `openai-api` | Interview |
| Logging | `tracing` crate for structured logging | Interview |
| Testing mocks | `mockall` crate for auto-generated mocks from traits | Interview |
| Retry policy | Built-in, configurable via `RetryConfig` (default: 3 retries, 1s delay) | Interview |
| System prompts | `.md` files compiled via `include_str!`, runtime override via `with_custom_prompt()` / `from_directory()` | Interview |
| API style | Prelude module, reqwest/tokio conventions | Interview |
| MSRV | Latest stable (1.85+), edition 2024 | Interview |
| Portability | Only platforms with full tokio (Linux, macOS, Windows) | Interview |
| Distribution | Lib crate + example CLI in `examples/` | Interview |

### Version Scope

| Version | Providers |
|---------|-----------|
| v1.0 (MVP) | `ClaudeProvider` (HTTP) + `ClaudeCliProvider` (CLI) |
| v1.1 | `GeminiProvider` (HTTP) + `GeminiCliProvider` (CLI) |
| v1.2 | `OpenAiProvider` (HTTP) |

---

## 2. Architecture

### Module Structure

```
magi-core/
  src/
    lib.rs              # Crate root, re-exports, prelude
    prelude.rs          # pub use of common types
    schema.rs           # RF-01: Domain types (Verdict, Severity, Mode, etc.)
    validate.rs         # RF-02: Validator with ValidationLimits
    consensus.rs        # RF-03: ConsensusEngine, ConsensusResult
    reporting.rs        # RF-04: ReportFormatter, banner + markdown
    provider.rs         # RF-05: LlmProvider trait, CompletionConfig
    agent.rs            # RF-06: Agent, AgentFactory
    orchestrator.rs     # RF-07: Magi, MagiBuilder, MagiConfig
    error.rs            # RF-08: MagiError, ProviderError
    report.rs           # RF-09: MagiReport
    providers/
      mod.rs
      claude.rs         # ClaudeProvider (feature: claude-api)
      claude_cli.rs     # ClaudeCliProvider (feature: claude-cli)
      gemini.rs         # GeminiProvider (feature: gemini-api, v1.1)
      gemini_cli.rs     # GeminiCliProvider (feature: gemini-cli, v1.1)
      openai.rs         # OpenAiProvider (feature: openai-api, v1.2)
    prompts/
      melchior.rs       # include_str! from .md files
      balthasar.rs
      caspar.rs
    prompts_md/         # Raw .md prompt files compiled into binary
      melchior_code_review.md
      melchior_design.md
      melchior_analysis.md
      balthasar_code_review.md
      balthasar_design.md
      balthasar_analysis.md
      caspar_code_review.md
      caspar_design.md
      caspar_analysis.md
  examples/
    basic_analysis.rs   # Example CLI usage
```

### Dependency Graph (Low Coupling)

```
schema ← validate ← consensus
                   ← reporting
provider ← agent ← orchestrator → consensus
                                → validate
                                → reporting
error ← (all modules)
```

Pure logic modules (schema, validate, consensus, reporting) have NO async or network dependencies.

### Dependencies

```toml
[dependencies]
tokio = { version = "1", features = ["process", "sync", "time", "macros", "rt-multi-thread"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
async-trait = "0.1"
thiserror = "2"
regex = "1"
tracing = "0.1"

# Optional provider dependencies
reqwest = { version = "0.12", features = ["json"], optional = true }

[features]
default = []
claude-api = ["reqwest"]
claude-cli = []
gemini-api = ["reqwest"]
gemini-cli = []
openai-api = ["reqwest"]

[dev-dependencies]
mockall = "0.13"
tokio = { version = "1", features = ["test-util"] }
```

---

## 3. Functional Requirements Summary

### RF-01: Domain Types (schema)

Enums with encapsulated behavior: `Verdict` (weight, effective, Display), `Severity` (Ord, icon, Display), `Mode` (Display), `AgentName` (title, display_name, Ord for deterministic tiebreak).

Structs: `Finding` (stripped_title), `AgentOutput` (is_approving, is_dissenting, effective_verdict).

All types: Serialize/Deserialize, Clone, Debug, PartialEq.

### RF-02: Validation (validate)

`Validator` with `ValidationLimits` (#[non_exhaustive]), precompiled `Regex` for zero-width char stripping. Validates confidence range, text field lengths, finding counts/titles/details.

### RF-03: Consensus (consensus)

`ConsensusEngine` with `ConsensusConfig` (#[non_exhaustive]). Stateless `determine(&self)` returns `ConsensusResult` with score, agent_count, votes, findings, dissent. Epsilon-aware float classification. Degraded mode caps STRONG labels.

### RF-04: Reporting (reporting)

`ReportFormatter` with `ReportConfig` (#[non_exhaustive]). Fixed-width 52-char ASCII banner. Markdown report with sections: Consensus Summary, Key Findings, Dissenting Opinion, Conditions for Approval, Recommended Actions. Replicates Python MAGI output format exactly.

### RF-05: LlmProvider Trait (provider)

`#[async_trait] trait LlmProvider: Send + Sync` with `complete()`, `name()`, `model()`. `CompletionConfig` (#[non_exhaustive]) with max_tokens + temperature (no timeout field).

### RF-06: Agents (agent)

`Agent` with name, mode, system_prompt, `Arc<dyn LlmProvider>`. `AgentFactory` with default_provider, per-agent overrides, custom prompts. System prompts from `include_str!` .md files.

### RF-07: Orchestrator (orchestrator)

`Magi` as main entry point. `MagiBuilder` with consuming method chaining. `MagiConfig` (#[non_exhaustive]) with timeout, max_input_len, completion, retry. `analyze(&self)` orchestrates full flow: validate input -> create agents -> launch parallel -> deserialize -> validate -> consensus -> report.

### RF-08: Errors (error)

`MagiError` enum: Validation, Provider, InsufficientAgents, Deserialization, InputTooLarge, Io. `ProviderError` enum: Http, Network, Timeout, Auth, Process, NestedSession. Both with thiserror derives. No panic! anywhere.

### RF-09: MagiReport (report)

Struct with agents, consensus, banner, report, degraded, failed_agents. Serialize for JSON export matching Python original format.

### RF-10: CLI Providers (cli-provider)

`ClaudeCliProvider`: model alias whitelist + pass-through, CLAUDECODE env check, stdin piping, double-nested JSON parsing, code fence stripping. `GeminiCliProvider` (v1.1): similar pattern for `gemini` CLI.

---

## 4. Interview Additions (Beyond Original Spec)

### 4.1 Retry Policy (NEW)

The original spec says "No debe hacer retry automatico a nivel de orquestador". The interview adds a **configurable** retry mechanism at the **provider level**:

```rust
#[non_exhaustive]
pub struct RetryConfig {
    pub max_retries: u32,      // default: 3
    pub base_delay: Duration,  // default: 1s
    pub retryable: fn(&ProviderError) -> bool, // default: 500, 429, Timeout
}
```

This is added to `MagiConfig` as an optional field. The retry wraps individual `provider.complete()` calls, not the full orchestration. This reconciles with the spec's prohibition on orchestrator-level retry.

### 4.2 Tracing (NEW)

The original spec says "No debe hacer logging directo". The interview selects `tracing` crate. Resolution: use `tracing` for structured spans/events, which is opt-in (consumers choose their subscriber). No direct stdout/stderr output. This is compatible with the spec's intent.

### 4.3 Feature Flag Separation (REFINED)

Original spec used `features = ["claude", "openai", "gemini"]`. Interview refines to separate HTTP and CLI: `claude-api`, `claude-cli`, `gemini-api`, `gemini-cli`, `openai-api`.

### 4.4 Prelude Module (NEW)

```rust
pub mod prelude {
    pub use crate::schema::{Verdict, Severity, Mode, AgentName, Finding, AgentOutput};
    pub use crate::orchestrator::{Magi, MagiBuilder, MagiConfig};
    pub use crate::provider::{LlmProvider, CompletionConfig};
    pub use crate::report::MagiReport;
    pub use crate::consensus::ConsensusResult;
    pub use crate::error::{MagiError, ProviderError};
}
```

### 4.5 System Prompts as include_str! (REFINED)

Default prompts stored as `.md` files in `src/prompts_md/`, compiled into binary via `include_str!`. Modifying a prompt only requires editing the .md file, not Rust source. Runtime overrides via `AgentFactory::with_custom_prompt()` and `from_directory()` remain.

---

## 5. BDD Scenarios (33 Total)

All 33 scenarios from `sbtdd/spec-behavior.md` remain unchanged. Key coverage:

| Category | Scenarios | Numbers |
|----------|-----------|---------|
| Unanimous consensus | 2 | 1, 4 |
| Mixed consensus | 2 | 2, 3 |
| Tie/edge cases | 1 | 5 |
| Degradation (1 fail) | 2 | 6, 7 |
| Degradation (2-3 fail) | 2 | 8, 9 |
| Validation errors | 3 | 10, 11, 12 |
| Finding dedup | 1 | 13 |
| JSON parse failure | 1 | 14 |
| Banner/report format | 2 | 15, 16 |
| Provider-specific | 3 | 17, 25, 26 |
| CLI providers | 6 | 18-23 |
| Builder pattern | 2 | 28, 29 |
| Agent modes | 1 | 30 |
| Error paths | 3 | 24, 31, 32 |
| Degraded label caps | 1 | 33 |

---

## 6. Constraints

All constraints from the original spec remain in force:

- **No panic!** in library code (only in `#[cfg(test)]`)
- **No unsafe**
- **No binary** (lib only, examples in `examples/`)
- **No hardcoded secrets**
- **No mutable state** between `analyze()` calls
- **No filesystem dependency** except optional prompt loading
- **Feature-gated providers** (nothing enabled by default)
- **CLAUDECODE env var detection** in CLI providers
- **OOP paradigm**: struct + impl + trait, composition over inheritance
- **File headers**: `// Author: Julian Bolivar`, `// Version: 1.0.0`, `// Date: YYYY-MM-DD`
- **Rustdoc** on all public APIs
- **clippy + rustfmt** enforced

---

## 7. Testing Strategy

- **Framework**: `#[test]` + cargo nextest + TDD-Guard
- **Mocking**: `mockall` for `LlmProvider` trait mocks
- **TDD**: Strict Red-Green-Refactor cycle
- **Coverage**: All 33 BDD scenarios + unit tests for each RF module
- **Test naming**: Behavior-descriptive (e.g., `test_consensus_caps_strong_label_in_degraded_mode`)
