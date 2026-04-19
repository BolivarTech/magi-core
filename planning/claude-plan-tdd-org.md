# TDD Plan: magi-core

> Companion to `claude-plan.md`. Defines test stubs for each section.
> Tests must be written BEFORE implementation (Red-Green-Refactor).

**Testing setup**: Rust built-in `#[test]`, `cargo nextest`, TDD-Guard enforcement.
**Mocking**: `mockall` for `LlmProvider` trait.
**Naming**: Behavior-descriptive, e.g. `test_consensus_caps_strong_label_in_degraded_mode`.

---

## Section 1: Foundation — Error Types and Domain Schema

### error.rs

```rust
// Test: MagiError::Validation contains descriptive message
// Test: MagiError::InsufficientAgents formats succeeded and required in Display
// Test: MagiError::InputTooLarge formats size and max in Display
// Test: ProviderError::Http contains status code and body in Display
// Test: ProviderError::Process includes exit_code and stderr
// Test: From<ProviderError> for MagiError wraps correctly
// Test: From<serde_json::Error> for MagiError produces Deserialization variant
// Test: From<std::io::Error> for MagiError produces Io variant
```

### schema.rs — Verdict

```rust
// Test: Verdict::Approve weight is 1.0
// Test: Verdict::Reject weight is -1.0
// Test: Verdict::Conditional weight is 0.5
// Test: Verdict::Conditional effective maps to Approve
// Test: Verdict::Approve effective maps to Approve (identity)
// Test: Verdict::Reject effective maps to Reject (identity)
// Test: Verdict Display outputs "APPROVE", "REJECT", "CONDITIONAL"
// Test: Verdict serializes as lowercase ("approve", "reject", "conditional")
// Test: Verdict deserializes from lowercase strings
```

### schema.rs — Severity

```rust
// Test: Severity ordering Critical > Warning > Info
// Test: Severity icon returns "[!!!]", "[!!]", "[i]"
// Test: Severity Display outputs "CRITICAL", "WARNING", "INFO"
// Test: Severity serializes as lowercase
```

### schema.rs — Mode

```rust
// Test: Mode Display outputs "code-review", "design", "analysis"
// Test: Mode serializes as lowercase with hyphens
```

### schema.rs — AgentName

```rust
// Test: AgentName title returns "Scientist", "Pragmatist", "Critic"
// Test: AgentName display_name returns "Melchior", "Balthasar", "Caspar"
// Test: AgentName Ord follows alphabetical (Balthasar < Caspar < Melchior)
// Test: AgentName serializes as lowercase
// Test: AgentName implements Eq and Hash (usable as BTreeMap key)
```

### schema.rs — Finding

```rust
// Test: Finding stripped_title removes zero-width characters (U+200B, U+FEFF, U+200C)
// Test: Finding stripped_title preserves normal text
// Test: Finding serializes/deserializes roundtrip
```

### schema.rs — AgentOutput

```rust
// Test: AgentOutput is_approving true for Approve
// Test: AgentOutput is_approving true for Conditional
// Test: AgentOutput is_approving false for Reject
// Test: AgentOutput is_dissenting true when effective verdict differs from majority
// Test: AgentOutput is_dissenting false when effective verdict matches majority
// Test: AgentOutput effective_verdict maps Conditional to Approve
// Test: AgentOutput serializes/deserializes roundtrip with all fields
```

---

## Section 2: Validation

### validate.rs

```rust
// Test: Validator::new creates with default limits and compiled regex
// Test: Validator::with_limits uses custom limits

// BDD Scenario 10: confidence out of range
// Test: validate rejects confidence > 1.0 with MagiError::Validation
// Test: validate rejects confidence < 0.0 with MagiError::Validation
// Test: validate accepts confidence at boundaries (0.0 and 1.0)

// BDD Scenario 11: empty title after strip zero-width
// Test: validate rejects finding with title of only zero-width chars
// Test: validate accepts finding with normal title

// BDD Scenario 12: text field exceeds max_text_len
// Test: validate rejects reasoning exceeding MAX_TEXT_LEN
// Test: validate rejects summary exceeding MAX_TEXT_LEN
// Test: validate rejects recommendation exceeding MAX_TEXT_LEN

// Test: validate rejects findings count exceeding max_findings
// Test: validate rejects finding title exceeding max_title_len
// Test: validate rejects finding detail exceeding max_detail_len
// Test: validate accepts valid AgentOutput with all fields within limits
// Test: validate calls sub-validators in order (confidence, summary, reasoning, recommendation, findings)
// Test: strip_zero_width removes Unicode category Cf characters
```

---

## Section 3: Consensus Engine

### consensus.rs — ConsensusEngine

```rust
// BDD Scenario 1: unanimous approve
// Test: 3 approve agents → score=1.0, label="STRONG GO", verdict=Approve, confidence≈0.9

// BDD Scenario 2: mixed 2 approve + 1 reject
// Test: 2 approve + 1 reject → score=0.333, label="GO (2-1)", verdict=Approve, dissent contains rejector

// BDD Scenario 3: approve + conditional + reject
// Test: approve + conditional + reject → score=0.166, label="GO WITH CAVEATS", conditions present

// BDD Scenario 4: unanimous reject
// Test: 3 reject → score=-1.0, label="STRONG NO-GO", verdict=Reject

// BDD Scenario 5: tie with 2 agents (synthetic)
// Test: 1 approve + 1 reject (2 agents) → score=0, label="HOLD -- TIE", verdict=Reject

// BDD Scenario 13: finding deduplication
// Test: same title different case → merged, severity promoted to highest
// Test: merged finding sources include both agents
// Test: detail preserved from highest-severity finding
// Test: on same severity, detail from first agent (Melchior > Balthasar > Caspar)

// BDD Scenario 33: degraded mode caps STRONG labels
// Test: 2 approve agents (degraded) → label="GO (2-0)" not "STRONG GO"
// Test: 2 reject agents (degraded) → label="HOLD (2-0)" not "STRONG NO-GO"

// Test: determine rejects fewer than min_agents with InsufficientAgents
// Test: determine rejects duplicate agent names with Validation error
// Test: epsilon-aware classification near boundaries (score ≈ 0, ≈ 1.0, ≈ -1.0)
// Test: confidence formula: base_confidence * weight_factor, clamped [0,1], rounded 2 decimals
// Test: majority_summary joins majority agent summaries with " | "
// Test: conditions extracted from agents with Conditional verdict
// Test: recommendations map includes all agents
// Test: ConsensusConfig enforces min_agents >= 1
```

---

## Section 4: Reporting

### reporting.rs — ReportFormatter

```rust
// BDD Scenario 15: banner width
// Test: all banner lines are exactly 52 characters wide
// Test: banner with long agent names still fits 52 chars (padding/truncation)

// BDD Scenario 16: report contains all sections
// Test: report with mixed consensus contains all 5 markdown headers
// Test: report without dissent omits "## Dissenting Opinion"
// Test: report without conditions omits "## Conditions for Approval"
// Test: report without findings omits "## Key Findings"

// Test: format_banner generates correct ASCII art structure
// Test: format_init_banner shows mode, model, timeout
// Test: format_separator is "+" + "=" * 50 + "+"
// Test: format_agent_line shows "Name (Title):  VERDICT (NN%)" format
// Test: format_findings shows icon + severity + title + sources + detail
// Test: format_dissent shows agent name, summary, full reasoning
// Test: format_conditions shows bulleted list with agent names
// Test: format_recommendations shows per-agent recommendations
// Test: agent_display falls back to AgentName methods when not in config
```

### reporting.rs — MagiReport

```rust
// Test: MagiReport serializes to JSON matching Python original format
// Test: degraded=false when all 3 agents succeed
// Test: degraded=true with failed_agents populated when agent fails
// Test: agent names in JSON are lowercase
// Test: consensus.confidence is rounded to 2 decimals
```

---

## Section 5: LlmProvider Trait

### provider.rs

```rust
// Test: CompletionConfig::default has max_tokens=4096, temperature=0.0
// Test: CompletionConfig is #[non_exhaustive] (compile-time, structural)

// Test: RetryProvider wraps inner provider and delegates name()/model()
// Test: RetryProvider retries on ProviderError::Timeout up to max_retries
// Test: RetryProvider retries on ProviderError::Http with status 500
// Test: RetryProvider retries on ProviderError::Http with status 429
// Test: RetryProvider does NOT retry on ProviderError::Auth
// Test: RetryProvider does NOT retry on ProviderError::Process
// Test: RetryProvider returns last error after exhausting retries
// Test: RetryProvider returns success on first successful retry
// Test: RetryProvider default config: 3 retries, 1s delay
```

---

## Section 6: Agents and AgentFactory

### agent.rs

```rust
// BDD Scenario 26: agents with different providers
// Test: each agent uses its own provider (verify mock receives exactly 1 call)

// BDD Scenario 27: factory with default and override
// Test: factory default provider used for Melchior and Balthasar, override for Caspar

// BDD Scenario 30: modes generate different prompts
// Test: CodeReview, Design, Analysis produce distinct system prompts per agent

// BDD Scenario 31: from_directory with nonexistent path
// Test: from_directory returns MagiError::Io for nonexistent directory

// Test: Agent::new generates system prompt from include_str! prompts
// Test: Agent::with_custom_prompt uses provided prompt
// Test: Agent::execute delegates to provider.complete with system prompt
// Test: Agent accessors (name, mode, system_prompt, provider_name, etc.)
// Test: AgentFactory::new creates 3 agents sharing default provider
// Test: AgentFactory::with_provider overrides provider for specific agent
// Test: AgentFactory::with_custom_prompt overrides prompt for specific agent
// Test: AgentFactory::create_agents returns exactly 3 agents for any mode
```

---

## Section 7: Orchestrator

### orchestrator.rs

```rust
// BDD Scenario 1: successful analysis with 3 unanimous agents
// Test: analyze returns MagiReport with 3 outputs, consensus, banner, report, degraded=false

// BDD Scenario 6: degradation - 1 agent timeout
// Test: 2 succeed + 1 timeout → Ok(MagiReport), degraded=true, failed_agents=[Caspar]

// BDD Scenario 7: degradation - 1 agent invalid JSON
// Test: 2 succeed + 1 bad JSON → Ok(MagiReport), degraded=true

// BDD Scenario 8: 2 agents fail
// Test: 1 succeed + 2 fail → Err(InsufficientAgents { succeeded: 1, required: 2 })

// BDD Scenario 9: all agents fail
// Test: 0 succeed → Err(InsufficientAgents { succeeded: 0, required: 2 })

// BDD Scenario 14: LLM returns non-JSON
// Test: agent returns plain text → treated as failed, system continues with remaining

// BDD Scenario 28: Magi::new with single provider
// Test: new creates Magi with 3 agents sharing same provider, all defaults

// BDD Scenario 29: builder with mixed providers and custom config
// Test: builder sets per-agent providers and custom timeout

// BDD Scenario 32: input too large
// Test: content exceeding max_input_len → Err(InputTooLarge) without launching agents

// Test: MagiConfig::default has timeout=300s, max_input_len=1MB
// Test: build_prompt formats "MODE: {mode}\nCONTEXT:\n{content}"
// Test: parse_agent_response strips code fences from JSON
// Test: parse_agent_response finds JSON object in preamble text
// Test: parse_agent_response fails on completely invalid input
// Test: launch_agents uses JoinSet (cancellation safety — dropped JoinSet aborts tasks)
// Test: MagiBuilder::build returns Result
```

---

## Section 8: MagiReport (in reporting.rs)

Covered by Section 4 tests above (MagiReport stubs).

---

## Section 9: RetryProvider

Covered by Section 5 tests above (RetryProvider stubs).

---

## Section 10: ClaudeProvider (HTTP API)

### providers/claude.rs

```rust
// Test: ClaudeProvider::new creates provider with api_key and model
// Test: provider.name() returns "claude"
// Test: provider.model() returns the configured model string
// Test: complete sends POST to /v1/messages with correct headers (x-api-key, anthropic-version)
// Test: complete maps non-2xx response to ProviderError::Http
// Test: complete extracts text content from Claude response format
// Test: reqwest::Client is reused (not created per call)
```

---

## Section 11: ClaudeCliProvider (CLI Subprocess)

### providers/claude_cli.rs

```rust
// BDD Scenario 18: launches 3 subprocesses in parallel
// Test: build_args includes --print, --output-format json, --model, --system-prompt

// BDD Scenario 19: parses double-nested JSON
// Test: parse_cli_output extracts inner JSON from {"result": "..."} envelope

// BDD Scenario 20: detects error in CLI response
// Test: is_error=true returns ProviderError::Process

// BDD Scenario 21: strips code fences
// Test: extract_json removes ```json ... ``` wrapping

// BDD Scenario 22: handles timeout
// Test: timeout triggers child.kill() and ProviderError::Timeout

// BDD Scenario 23: detects nested session
// Test: CLAUDECODE env var present → Err(ProviderError::NestedSession) in constructor

// Test: new("sonnet") maps to "claude-sonnet-4-6"
// Test: new("opus") maps to "claude-opus-4-6"
// Test: new("haiku") maps to "claude-haiku-4-5-20251001"
// Test: new("claude-custom-model") passes through (contains "claude-")
// Test: new("invalid") returns ProviderError::Auth
// Test: provider.name() returns "claude-cli"
// Test: provider.model() returns the resolved model_id
// Test: complete sends user prompt via stdin, not as CLI arg
```

---

## Section 12: Prelude and Crate Root

```rust
// Test: prelude re-exports Magi, Mode, MagiReport, LlmProvider, CompletionConfig, etc.
// Test: feature flags conditionally compile provider modules
// Test: crate compiles with no features enabled (core only)
// Test: crate compiles with claude-api feature
// Test: crate compiles with claude-cli feature
```
