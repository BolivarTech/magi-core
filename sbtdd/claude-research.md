# Research Findings for magi-core

## Part 1: Codebase Analysis

---

### 1. Python MAGI Plugin (`D:\jbolivarg\PythonProjects\MAGI`)

#### Agent System Architecture

**File Structure:**
- System prompts in `/skills/magi/agents/` (melchior.md, balthasar.md, caspar.md)
- Orchestrator in `/skills/magi/scripts/run_magi.py`
- Validation in `/skills/magi/scripts/validate.py`
- Synthesis engine in `/skills/magi/scripts/synthesize.py` and `/skills/magi/scripts/consensus.py`

**Agent Personalities & Modes:**
- **Melchior (Scientist)**: Technical rigor, correctness, algorithm analysis
- **Balthasar (Pragmatist)**: Practicality, maintainability, real-world impact
- **Caspar (Critic)**: Risk identification, edge cases, failure modes

Each agent responds to three modes: `code-review`, `design`, `analysis`

**Agent System Prompt Pattern:**
```
# [Agent Name] -- [Archetype]

You are [Agent Name], one of three MAGI analysis agents. Your lens is [focus area].

## Your role
[2-3 sentences on analytical perspective]

## Input format
MODE: {code-review|design|analysis}
CONTEXT: [user-provided content]

## What you focus on
### In code review mode / design mode / analysis mode
[Bulleted criteria specific to this agent's perspective]

## Your personality
[4-5 bullet points defining analytical approach]

## Constraints
- Always respond in English
- reasoning field: 2-5 focused paragraphs (200-500 words)
- findings array: 1-7 items
- confidence ranges defined
- Express personality through JSON field values, NOT text outside JSON

## Output format
Respond with ONLY a JSON object. No markdown fences, no preamble.
```

#### CLI Provider Implementation

**File:** `/skills/magi/scripts/run_magi.py`

**Subprocess Launching Pattern:**
```python
proc = await asyncio.create_subprocess_exec(
    "claude",
    "-p",                        # Print mode
    "--output-format", "json",
    "--model", model_id,
    "--system-prompt-file", system_prompt_file,
    "-",                         # Stdin placeholder
    stdin=asyncio.subprocess.PIPE,
    stdout=asyncio.subprocess.PIPE,
    stderr=asyncio.subprocess.PIPE,
)

try:
    stdout, stderr = await asyncio.wait_for(
        proc.communicate(input=prompt.encode("utf-8")), timeout=timeout
    )
except asyncio.TimeoutError:
    proc.kill()
    raise TimeoutError(...) from None
```

**Key patterns:**
- Prompt passed via stdin to avoid OS command-line length limits (~32KB on Windows)
- Timeout handling: `asyncio.wait_for()` with catch-and-kill-on-timeout
- Stderr captured as debug artifact
- Return code validation: raises RuntimeError if exit code != 0

#### JSON Output Parsing

**File:** `/skills/magi/scripts/parse_agent_output.py`

Handles multiple Claude CLI output shapes:
1. `{"result": "..."}` - Direct string
2. `{"content": [{"type": "text", "text": "..."}]}` - Content block format
3. Plain string

Code fence stripping via regex.

#### Agent Output JSON Schema

**File:** `/skills/magi/scripts/validate.py`

**Required Fields:**
- `agent`: str (must be in {"melchior", "balthasar", "caspar"})
- `verdict`: str (must be in {"approve", "reject", "conditional"})
- `confidence`: float (0.0 <= confidence <= 1.0)
- `summary`: str (max 50,000 chars)
- `reasoning`: str (max 50,000 chars)
- `findings`: list[dict] (max 100 items per agent)
- `recommendation`: str (max 50,000 chars)

**Finding Object:** severity, title (max 500), detail (max 10,000)

**Validation Logic:**
- Rejects booleans disguised as floats
- Strips zero-width Unicode characters (category Cf) from finding titles
- Max file size per agent output: 10 MB

#### Reporting Format

**File:** `/skills/magi/scripts/reporting.py`

Banner: ASCII art, fixed width 52 chars (inner=50, `|` borders).
Report sections (markdown): Banner, Consensus Summary, Key Findings, Dissenting Opinion, Conditions for Approval, Recommended Actions.

Agent titles: `{"melchior": ("Melchior", "Scientist"), "balthasar": ("Balthasar", "Pragmatist"), "caspar": ("Caspar", "Critic")}`

#### Consensus & Synthesis Logic

**File:** `/skills/magi/scripts/consensus.py`

Weights: approve=1, conditional=0.5, reject=-1
Score normalized to [-1.0, 1.0]

Classification:
- 1.0 -> "STRONG GO" (approve)
- -1.0 -> "STRONG NO-GO" (reject)
- positive with conditions -> "GO WITH CAVEATS" (conditional)
- positive no conditions -> "GO (N-M)" (approve)
- 0.0 -> "HOLD -- TIE" (reject)
- negative -> "HOLD (N-M)" (reject)

Finding deduplication: groups by title (case-insensitive), keeps highest severity, tracks sources.

Confidence: `base_confidence = sum(majority_conf) / num_agents * weight_factor` where `weight_factor = (abs(score) + 1) / 2`

#### Parallel Orchestration

Uses `asyncio.gather(..., return_exceptions=True)`. Minimum 2/3 agents required.
Degraded flag set when agents fail. Cleanup of old temp runs (max 5).

---

### 2. PR-AI-Reviewer (`D:\jbolivarg\RustProjects\PR-AI-Reviewer`)

#### Subprocess Execution with Tokio

**File:** `/src/backend/claude_code.rs`

```rust
let mut child = tokio::process::Command::new("claude")
    .args(&args)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .map_err(ReviewError::Io)?;

// Pipe prompt via stdin
if let Some(mut stdin) = child.stdin.take() {
    stdin.write_all(prompt.as_bytes()).await.map_err(ReviewError::Io)?;
}

let output = child.wait_with_output().await.map_err(ReviewError::Io)?;
```

CLI args: `["--print", "--output-format", "json", "--model", model, "--system-prompt", prompt]`

#### Double-Nested JSON Parsing

Outer envelope: `{"type": "result", "subtype": "success", "is_error": false, "result": "<escaped JSON string>"}`

Two-layer parsing:
1. Parse outer JSON to `CliOutput` struct
2. Extract inner JSON from `result` string field
3. Strip code fences if present
4. Parse inner JSON to domain struct

Custom deserializers: `null_as_default<T>` and `string_or_vec_findings`

#### Parallel Execution with JoinSet

Uses `tokio::task::JoinSet` with `Arc<Semaphore>` for bounded concurrency:

```rust
let semaphore = Arc::new(Semaphore::new(self.config.max_concurrent));
let mut join_set = JoinSet::new();

for request in requests {
    let backend = backend.clone();
    let semaphore = semaphore.clone();
    join_set.spawn(async move {
        let _permit = semaphore.acquire().await?;
        backend.review(&request).await
    });
}
```

Supports Sequential, Hybrid, and Full parallel modes.

#### Error Handling

Unified `ReviewError` enum: GitDiff, Api, Parse, Config, AzureDevOps, Io.
Implements `Display`, `Error`, `From<std::io::Error>`, `From<serde_json::Error>`, `From<reqwest::Error>`.

#### Notable: No CLAUDECODE Detection

PR-AI-Reviewer does NOT check for CLAUDECODE env var. This is something magi-core needs to add per spec RF-08 (ProviderError::NestedSession).

---

### 3. Current MAGI Rust Project (`D:\jbolivarg\RustProjects\MAGI`)

**Cargo.toml:** magi-core v0.1.0, edition 2024, empty dependencies
**src/lib.rs:** Placeholder `add()` function with basic test
**Status:** Bare scaffold ready for implementation

---

## Part 2: Rust Best Practices (2024-2025)

---

### 1. Async Trait Patterns

#### Native vs async-trait

Native `async fn` in traits (Rust 1.75+) works for **static dispatch only**. Does NOT work with `dyn Trait` / trait objects as of Rust 1.85.

**For magi-core**: Since `LlmProvider` needs `Arc<dyn LlmProvider>` for dynamic dispatch across tokio::spawn tasks, **`async-trait` is required**.

```rust
use async_trait::async_trait;

#[async_trait]
trait LlmProvider: Send + Sync {
    async fn complete(&self, prompt: &str) -> Result<String, Error>;
}

// Works with Arc<dyn LlmProvider> + tokio::spawn
async fn spawn_work(provider: Arc<dyn LlmProvider>) {
    let p = provider.clone();
    tokio::spawn(async move {
        let result = p.complete("Hello").await.unwrap();
    });
}
```

**Key rules for tokio::spawn compatibility:**
- Trait must have `: Send + Sync` supertraits
- Future must be `Send` (default in async-trait)
- Use `Arc<dyn Trait>` (not Rc)
- All data captured across `.await` points must be Send

#### Edition 2024 Impact

- `impl Trait` lifetime captures: captures all in-scope lifetimes by default
- No change to the `dyn` + async story

**Crates:** `async-trait` 0.1.x (stable, 0.1.83+)

**References:**
- https://blog.rust-lang.org/2023/12/21/async-fn-rpit-in-traits.html
- https://docs.rs/async-trait/latest/async_trait/
- https://doc.rust-lang.org/edition-guide/rust-2024/

---

### 2. Builder Pattern

#### Recommended: Consuming Builder (`mut self`)

```rust
pub struct MagiBuilder {
    provider: Option<Arc<dyn LlmProvider>>,
    config: Option<MagiConfig>,
}

impl MagiBuilder {
    pub fn provider(mut self, name: AgentName, p: Arc<dyn LlmProvider>) -> Self {
        self
    }

    pub fn config(mut self, config: MagiConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn build(self) -> Magi {
        // construct from accumulated state
    }
}
```

**Best practices:**
- Use `impl Into<String>` for string parameters
- `build()` returns `Result<T, E>` when validation needed
- `#[must_use]` on builder methods
- For simple structs (2-3 fields), just use `new()` directly

**For magi-core**: Consuming builder fits well. `Magi::builder(provider)` starts the chain, `.build()` is infallible since provider is required upfront.

**References:**
- https://rust-unofficial.github.io/patterns/patterns/creational/builder.html
- https://docs.rs/bon/latest/bon/

---

### 3. Tokio Subprocess Management

#### Key Pattern: Timeout + Kill

```rust
let mut child = Command::new("claude")
    .args(&args)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true)  // ALWAYS set this
    .spawn()?;

// Write prompt via stdin
let mut stdin = child.stdin.take().expect("stdin piped");
stdin.write_all(prompt.as_bytes()).await?;
drop(stdin);  // Close stdin to signal EOF

// Timeout wrapper
match timeout(duration, child.wait_with_output()).await {
    Ok(Ok(output)) => { /* process output */ }
    Ok(Err(e)) => { /* IO error */ }
    Err(_) => {
        let _ = child.kill().await;
        Err(ProviderError::Timeout)
    }
}
```

**Critical rules:**
- Always `kill_on_drop(true)` to prevent orphaned processes
- Always `drop(stdin)` after writing to signal EOF
- Use `tokio::join!` when reading stdout + stderr simultaneously (prevents deadlock)
- Use `String::from_utf8_lossy()` for graceful invalid UTF-8 handling

**Crate:** `tokio = { version = "1", features = ["process", "io-util", "time", "macros", "rt-multi-thread"] }`

**References:**
- https://docs.rs/tokio/latest/tokio/process/index.html
- https://docs.rs/tokio/latest/tokio/time/fn.timeout.html

---

### 4. Reqwest HTTP Client Patterns

#### Client Reuse (Critical)

Create ONE `reqwest::Client` and reuse it. `Client` is cheaply cloneable (Arc internally).

```rust
pub struct ClaudeProvider {
    client: Client,  // Clone is cheap
    api_key: String,
    model: String,
}
```

#### Authentication Headers

```rust
let mut auth_value = HeaderValue::from_str(&format!("Bearer {}", api_key))?;
auth_value.set_sensitive(true);  // Prevents logging
headers.insert(AUTHORIZATION, auth_value);
```

For Claude API: use `x-api-key` header instead of `Authorization: Bearer`.

#### Error Handling

Always check status before parsing JSON:

```rust
let response = client.post(url).json(&body).send().await?;
let status = response.status();
if !status.is_success() {
    let body = response.text().await.unwrap_or_default();
    return Err(ProviderError::Http { status: status.as_u16(), body });
}
response.json::<T>().await.map_err(...)
```

#### Timeouts

```rust
Client::builder()
    .connect_timeout(Duration::from_secs(10))
    .timeout(Duration::from_secs(60))
    .pool_idle_timeout(Duration::from_secs(90))
    .build()?;
```

**Crate:** `reqwest = { version = "0.12", features = ["json"] }`

**References:**
- https://docs.rs/reqwest/latest/reqwest/
- https://docs.rs/reqwest/latest/reqwest/struct.Client.html

---

## Summary: Key Implementation Patterns for magi-core

| Area | Pattern | Source |
|------|---------|--------|
| Async traits | `async-trait` crate for `dyn LlmProvider` | Best practices |
| Provider sharing | `Arc<dyn LlmProvider>` with Clone for tokio::spawn | PR-AI-Reviewer |
| CLI subprocess | tokio::process + stdin pipe + kill_on_drop + timeout | PR-AI-Reviewer + Python MAGI |
| JSON parsing | Double-nested (CLI envelope + inner JSON) + code fence strip | PR-AI-Reviewer |
| HTTP client | reqwest::Client reuse, per-provider headers | Best practices |
| Parallel execution | tokio::task::JoinSet or tokio::spawn + join | PR-AI-Reviewer |
| Consensus | Weighted voting, epsilon-aware float comparison | Python MAGI |
| Validation | Regex precompiled, zero-width char strip, size limits | Python MAGI |
| Error handling | Unified enum with thiserror, domain-specific variants | PR-AI-Reviewer |
| Reporting | Fixed-width ASCII banner + markdown sections | Python MAGI |
| Builder | Consuming `mut self`, `impl Into<String>` | Best practices |
| Nested session | Check CLAUDECODE env var in CLI provider constructor | Spec RF-08 (new) |

## Testing Setup

- **Test runner:** cargo nextest
- **TDD guard:** tdd-guard + tdd-guard-rust
- **Test command:** `python run-tests.py` (pipes nextest output through tdd-guard-rust)
- **Hooks:** PreToolUse, PostToolUse, SessionStart, UserPromptSubmit all configured in `.claude/settings.json`
