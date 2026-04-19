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
- **Finding deduplication** — NFKC + full Unicode case-folding merges duplicates across agents, promotes severity
- **Retry with backoff** — opt-in `RetryProvider` wrapper with exponential backoff
- **Prompt-injection hardening** — 3-layer sanitization pipeline (normalize newlines → strip invisibles → neutralize headers) + 128-bit per-request nonce with fail-closed collision detection. See [`docs/adr/001-prompt-injection-threat-model.md`](docs/adr/001-prompt-injection-threat-model.md).
- **Byte-for-byte parity with MAGI Python reference** — 3 mode-agnostic prompts verified via SHA-256 fixture in CI
- **Feature-gated providers** — `claude-api` (HTTP) and `claude-cli` (subprocess) ship as optional features
- **Zero unsafe in library code** — `#![forbid(unsafe_code)]` safe by design

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
magi-core = "0.3"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }

# Enable one or both built-in providers:
# magi-core = { version = "0.3", features = ["claude-cli"] }
# magi-core = { version = "0.3", features = ["claude-api"] }
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
> `with_custom_prompt_all_modes` (mode-agnostic). See
> [`docs/migration-v0.3.md`](docs/migration-v0.3.md).

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
schema        (domain types: Verdict, Severity, Mode, AgentName, Finding, AgentOutput)
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
  claude      [feature: claude-api]  — HTTP via reqwest
  claude_cli  [feature: claude-cli]  — subprocess via tokio::process
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
`MagiError::InvalidInput`. See
[`docs/adr/001-prompt-injection-threat-model.md`](docs/adr/001-prompt-injection-threat-model.md)
for the full threat model, scope, and accepted limitations
(case-sensitive matching, non-ASCII whitespace, ~64-bit effective nonce
entropy from `fastrand`).

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
    { "severity": "warning", "title": "Short title", "detail": "Explanation" }
  ],
  "recommendation": "What this agent recommends"
}
```

## Feature Flags

| Feature       | Default | Description                          |
|---------------|---------|--------------------------------------|
| `claude-api`  | off     | HTTP provider via `reqwest`          |
| `claude-cli`  | off     | Subprocess provider via `tokio::process` |

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

See [CHANGELOG.md](CHANGELOG.md) for the full version history. Migration
guides for breaking releases:

- [`docs/migration-v0.3.md`](docs/migration-v0.3.md) — prompt architecture + defense-in-depth
- [`docs/migration-v0.2.md`](docs/migration-v0.2.md) — consensus/report/validation parity with Python MAGI

## License

Licensed under either of

- [Apache License, Version 2.0](http://www.apache.org/licenses/LICENSE-2.0)
- [MIT License](http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
