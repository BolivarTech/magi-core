# magi-core

[![Crates.io](https://img.shields.io/crates/v/magi-core.svg)](https://crates.io/crates/magi-core)
[![Documentation](https://docs.rs/magi-core/badge.svg)](https://docs.rs/magi-core)
[![License](https://img.shields.io/crates/l/magi-core.svg)](https://github.com/BolivarTech/magi-core#license)
[![CI](https://github.com/BolivarTech/magi-core/actions/workflows/ci.yml/badge.svg)](https://github.com/BolivarTech/magi-core/actions)
[![Release](https://github.com/BolivarTech/magi-core/actions/workflows/release.yml/badge.svg)](https://github.com/BolivarTech/magi-core/actions/workflows/release.yml)

LLM-agnostic multi-perspective analysis system in Rust, inspired by the MAGI
supercomputers from [Neon Genesis Evangelion](https://en.wikipedia.org/wiki/Neon_Genesis_Evangelion).

Three independent agents analyze content from complementary perspectives, then a
consensus engine synthesizes their verdicts into a unified report.

> *"No single perspective is sufficient for good decision-making under uncertainty."*
> See [docs/MAGI-System-Documentation.md](docs/MAGI-System-Documentation.md) for the
> complete origin story, design philosophy, and Evangelion correspondence table.

| Agent        | Codename   | Perspective                  |
|--------------|------------|------------------------------|
| **Melchior** | Scientist  | Technical rigor & correctness |
| **Balthasar**| Pragmatist | Practicality & maintainability |
| **Caspar**   | Critic     | Risk, edge cases & failure modes |

## Features

- **LLM-agnostic** — bring your own provider via the `LlmProvider` trait
- **Parallel execution** — agents run concurrently via `tokio::spawn` with `AbortGuard` cancellation
- **Graceful degradation** — if one agent fails, the remaining two still produce a result
- **Weighted consensus** — approve (+1), conditional (+0.5), reject (-1) scoring with epsilon-aware classification
- **Structured findings** *(v1.0)* — `Finding` carries optional `file`/`line`/`category` (typed `Category` enum: 15 slugs + `Other`); the `finding_id` module exposes a stable SHA-256 dedup key with verified cross-language parity. Locations are agent-reported and **unverified** — validate against your own diff
- **Finding deduplication** — co-located findings (`file` + `line`) merge by a stable `finding_id`; unlocated findings merge by NFKC + full Unicode case-folded title. Severity is promoted to the highest seen across agents
- **Retry on schema errors** *(v0.4)* — single-shot retry with feedback prompt when an agent returns malformed JSON or fails schema validation. Opt-out via `with_retry_disabled()`. Telemetry surfaces via `MagiReport.retried_agents`.
- **Retry with backoff** — opt-in `RetryProvider` wrapper with exponential backoff for HTTP/network transient errors (orthogonal to the schema-retry layer)
- **Cost control via complexity gate** *(v0.5)* — caller-supplied predicate (`Fn(&str, &Mode) -> bool`) short-circuits `analyze` before any LLM dispatch. Composable patterns include length thresholds, rate limiters via atomic counters, and pre-flight cheap-model triage. See [Cost control](#cost-control-with-complexity-gate).
- **Prompt-injection hardening** — 3-layer sanitization pipeline (normalize newlines → strip invisibles → neutralize headers) + 128-bit per-request nonce with fail-closed collision detection. Retry-feedback envelope has a parallel 4-layer defense covering Unicode-confusable dash variants.
- **Byte-for-byte parity with MAGI Python reference** — 3 mode-agnostic prompts pinned to `MAGI@v3.0.0` (finding calibration), verified via SHA-256 fixture in CI
- **Feature-gated providers** — `claude-api` (HTTP), `claude-cli` (subprocess), and `openai-compat` (OpenAI Chat Completions — OpenAI cloud + Ollama/LocalAI/vLLM/LM Studio/llama.cpp-server) ship as optional features
- **Optional test helpers** — `test-utils` feature exposes `RoutingMockProvider` for downstream integration tests
- **No `unsafe` in production library code** — the only `unsafe` is in `#[cfg(test)]` env-var helpers and the `basic_analysis` example (edition-2024 `set_var` / Windows console APIs)

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
magi-core = "1.0"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }

# Enable one or both built-in providers:
# magi-core = { version = "1.0", features = ["claude-cli"] }
# magi-core = { version = "1.0", features = ["claude-api"] }
```

### Basic Usage

```rust
use magi_core::prelude::*;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), MagiError> {
    // Bring your own LlmProvider implementation
    let provider: Arc<dyn LlmProvider> = /* your provider */;

    let magi = Magi::new(provider);
    let report = magi.analyze(&Mode::CodeReview, "fn main() {}").await?;

    println!("{}", report.report);
    Ok(())
}
```

### With Builder

```rust
use magi_core::prelude::*;
use std::sync::Arc;
use std::time::Duration;

# async fn example() -> Result<(), MagiError> {
let default_provider: Arc<dyn LlmProvider> = /* ... */;
let caspar_provider: Arc<dyn LlmProvider> = /* ... */;

let magi = Magi::builder(default_provider)
    .with_provider(AgentName::Caspar, caspar_provider)
    .with_timeout(Duration::from_secs(60))
    .with_consensus_config(ConsensusConfig { min_agents: 2, epsilon: 1e-9 })
    .build()?;

let report = magi.analyze(&Mode::Design, "Propose a caching layer").await?;
println!("{}", report.report);
# Ok(())
# }
```

### Custom System Prompts

Two override modes resolve in order: per-mode → all-modes → embedded default.

```rust
use magi_core::prelude::*;
use std::sync::Arc;

# async fn example() -> Result<(), MagiError> {
let provider: Arc<dyn LlmProvider> = /* ... */;

let magi = Magi::builder(provider)
    // Only used when mode == CodeReview
    .with_custom_prompt_for_mode(
        AgentName::Melchior,
        Mode::CodeReview,
        "You are a code review specialist...".to_string(),
    )
    // Fallback for all other modes
    .with_custom_prompt_all_modes(
        AgentName::Melchior,
        "You are Melchior, the scientist...".to_string(),
    )
    .build()?;
# Ok(())
# }
```

> **Migrating from v0.2:** `with_custom_prompt(agent, mode, prompt)` is
> deprecated. Use `with_custom_prompt_for_mode` (per-mode) or
> `with_custom_prompt_all_modes` (mode-agnostic).

### Cost Control with Complexity Gate

*(v0.5+)* `analyze` always costs 3 Claude calls. To avoid spending on
trivial inputs, install a caller-supplied predicate via
`MagiBuilder::with_complexity_gate`. When it returns `false`, `analyze`
returns `MagiError::SkippedByComplexityGate` with **zero LLM dispatch**.

```rust
use magi_core::prelude::*;
use std::sync::Arc;

# async fn example() -> Result<(), MagiError> {
let provider: Arc<dyn LlmProvider> = /* ... */;

let magi = Magi::builder(provider)
    .with_complexity_gate(|content, mode| match mode {
        Mode::CodeReview => content.len() >= 200,
        Mode::Design => content.len() >= 500,
        Mode::Analysis => !content.trim().is_empty(),
    })
    .build()?;

match magi.analyze(&Mode::CodeReview, "fn main() {}").await {
    Ok(report) => println!("{}", report.report),
    Err(MagiError::SkippedByComplexityGate { reason, .. }) => {
        eprintln!("skipped: {reason}");
    }
    Err(other) => return Err(other),
}
# Ok(())
# }
```

**Evaluation order:** `analyze` checks (1) `max_input_len` validation,
(2) the gate, (3) agent factory + nonce + dispatch. Validate-first
means stateful predicates (rate limiters, cache counters) do NOT
fire on oversize inputs.

**Predicate contract:** `Fn(&str, &Mode) -> bool + Send + Sync + 'static`.
Must be cheap (microseconds, not milliseconds) because it runs
synchronously on the calling task's executor. Long-running classification
should be offloaded to a separate task. Predicate panics propagate
uncaught — wrap your predicate body in defensive code if its inputs are
not under your control.

Composable patterns: length thresholds per mode, rate limiters via
`Arc<AtomicUsize>`, pre-flight cheap-LLM triage via `pollster::block_on`,
or a stateful classifier shared across `Magi` instances.

**Disabling the gate (v0.4-equivalent behavior):** simply don't call
`with_complexity_gate`. The default state is "no gate set" and `analyze`
proceeds to dispatch unconditionally — byte-equivalent to v0.4.x.

```rust
use magi_core::prelude::*;
use std::sync::Arc;

# async fn example() -> Result<(), MagiError> {
let provider: Arc<dyn LlmProvider> = /* ... */;

// No `.with_complexity_gate(...)` call → v0.4.x behavior preserved.
let magi = Magi::builder(provider).build()?;

let report = magi.analyze(&Mode::CodeReview, "fn main() {}").await?;
// `MagiError::SkippedByComplexityGate` will NEVER be returned when no
// gate is set — the variant only fires when a predicate is installed
// and returns `false`.
println!("{}", report.report);
# Ok(())
# }
```

The shorter `Magi::new(provider)` constructor also creates a Magi
without a gate. Choose `Magi::new` for the simplest path; choose
`Magi::builder(...).build()` only when you need to configure timeouts,
custom prompts, providers per agent, or the complexity gate itself.

### Using the Built-in Claude CLI Provider

```rust
use magi_core::prelude::*;
use std::sync::Arc;

# #[cfg(feature = "claude-cli")]
# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let provider = Arc::new(ClaudeCliProvider::new("sonnet")?);
let magi = Magi::new(provider);

let report = magi.analyze(&Mode::Analysis, "Should we use microservices?").await?;
println!("{}", report.banner);
# Ok(())
# }
```

## Architecture

```
                    +-----------+
                    |   Magi    |  Orchestrator
                    +-----+-----+
                          |
              +-----------+-----------+
              |           |           |
         +----+----+ +----+----+ +----+----+
         | Melchior| |Balthasar| |  Caspar |  Agents (parallel)
         +----+----+ +----+----+ +----+----+
              |           |           |
              +-----+-----+-----+-----+
                    |                 |
              +-----+-----+   +------+------+
              |  Validator |   |  Consensus  |
              +-----------+   +------+------+
                                     |
                              +------+------+
                              |  Reporting  |
                              +-------------+
```

### Module Dependency Graph

```
error         (foundation — no internal deps)
schema        (domain types: Verdict, Severity, Mode, AgentName, Category, Finding, AgentOutput)
finding_id    (stable SHA-256 finding identity + fail-soft file/line/category deserializers)
validate      (field validation with regex zero-width stripping, NFKC + casefold)
consensus     (weighted scoring, classification, finding dedup)
reporting     (ASCII banner + markdown report generation)
provider      (LlmProvider trait, CompletionConfig, RetryProvider)
prompts       (3 mode-agnostic prompts embedded via include_str!, lookup helper)
prompts_md/   (byte-for-byte Python reference: melchior.md, balthasar.md, caspar.md)
user_prompt   (sanitization pipeline + nonce-delimited payload construction)
agent         (Agent struct, AgentFactory — no Mode parameter as of v0.3)
orchestrator  (Magi, MagiBuilder — composes everything)
providers/
  claude          [feature: claude-api]      — HTTP via reqwest
  claude_cli      [feature: claude-cli]      — subprocess via tokio::process
  openai_compat   [feature: openai-compat]   — OpenAI Chat Completions HTTP (OpenAI + Ollama/LocalAI/vLLM/LM Studio)
```

### Prompt Injection Defense

User content is treated as untrusted. Before being sent to the LLM, every
call to `Magi::analyze` wraps the content in a delimited payload:

```
MODE: <mode>
---BEGIN USER CONTEXT <32-hex-nonce>---
<sanitized content>
---END USER CONTEXT <32-hex-nonce>---
```

The sanitization pipeline runs in a fixed order:

1. `normalize_newlines` — converts Unicode line terminators (`\r\n`, `\r`,
   U+0085, U+000B, U+000C, U+2028, U+2029) to `\n`.
2. `strip_invisibles` — removes zero-width and bidi formatting characters.
3. `neutralize_headers` — prefixes any line starting with `MODE`, `CONTEXT`,
   `---BEGIN`, or `---END` (with optional leading ASCII whitespace) with two
   spaces so it cannot be parsed as a delimiter.

Each request uses a fresh 128-bit nonce. If the sanitized content happens
to contain the generated nonce, `analyze` fails closed with
`MagiError::InvalidInput`. Accepted limitations include case-sensitive
matching, non-ASCII whitespace, and ~64-bit effective nonce entropy from
`fastrand`.

## Consensus Labels

| Score | Condition             | Label                        |
|-------|-----------------------|------------------------------|
| 1.0   | Unanimous approve     | **STRONG GO**                |
| > 0   | Has conditionals      | **GO WITH CAVEATS (N-M)**    |
| > 0   | No conditionals       | **GO (N-M)**                 |
| 0     | Tie                   | **HOLD -- TIE**              |
| < 0   | Mixed                 | **HOLD (N-M)**               |
| -1.0  | Unanimous reject      | **STRONG NO-GO**             |

`(N-M)` is the effective split: approves and conditionals on the "go" side,
rejects on the "no" side. In degraded mode (2/3 agents), STRONG labels are
capped to their regular counterparts.

## Implementing a Custom Provider

```rust
use magi_core::prelude::*;
use async_trait::async_trait;

struct MyProvider;

#[async_trait]
impl LlmProvider for MyProvider {
    async fn complete(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        config: &CompletionConfig,
    ) -> Result<String, ProviderError> {
        // Call your LLM backend here
        todo!()
    }

    fn name(&self) -> &str { "my-provider" }
    fn model(&self) -> &str { "my-model-v1" }
}
```

The agent expects a JSON response matching the `AgentOutput` schema:

```json
{
  "agent": "melchior",
  "verdict": "approve",
  "confidence": 0.85,
  "summary": "One-line verdict summary",
  "reasoning": "Detailed analysis (2-5 paragraphs)",
  "findings": [
    { "severity": "warning", "title": "Short title", "detail": "Explanation",
      "file": "src/db.rs", "line": 42, "category": "logic-error" }
  ],
  "recommendation": "What this agent recommends"
}
```

`findings[].file`, `line`, and `category` are **optional** (typically present in
code-review). Omit them or use `null` in design/analysis. Unknown `category`
values fall back to `"other"`; a malformed `file`/`line` fails soft to absent
(never a deserialization error). These locations are agent-reported and
**unverified** — validate against your own diff.

## Feature Flags

| Feature          | Default | Description                          |
|------------------|---------|--------------------------------------|
| `claude-api`     | off     | HTTP provider via `reqwest`          |
| `claude-cli`     | off     | Subprocess provider via `tokio::process` |
| `openai-compat`  | off     | OpenAI Chat Completions HTTP provider (`OpenAiCompatibleProvider`) — OpenAI cloud + Ollama/LocalAI/vLLM/LM Studio/llama.cpp-server via a configurable `base_url`. |
| `test-utils`     | off     | Exposes `magi_core::test_support::RoutingMockProvider` for downstream integration tests. Stable within the 1.x line. |

The core library (orchestrator, consensus, reporting, validation) compiles with
no optional features enabled.

## Example

```bash
# Run the included example with the CLI provider
cargo run --example basic_analysis --features claude-cli -- \
  --input "fn fibonacci(n: u32) -> u32 { match n { 0 => 0, 1 => 1, _ => fibonacci(n-1) + fibonacci(n-2) } }"

# JSON output
cargo run --example basic_analysis --features claude-cli -- \
  --json --input "fn fibonacci(n: u32) -> u32 { match n { 0 => 0, 1 => 1, _ => fibonacci(n-1) + fibonacci(n-2) } }"
```

## Minimum Supported Rust Version

This crate uses Rust edition 2024 and requires **Rust 1.91+**.

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for the full version history.

## License

Licensed under either of

- [Apache License, Version 2.0](http://www.apache.org/licenses/LICENSE-2.0)
- [MIT License](http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
