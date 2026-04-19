# Section 07: Agents and AgentFactory (`agent.rs` + `prompts/`)

## Overview

This section implements the `Agent` struct and `AgentFactory`, which create and manage the three MAGI agents (Melchior/Scientist, Balthasar/Pragmatist, Caspar/Critic). Each agent is an autonomous unit with its own LLM provider and system prompt. The `AgentFactory` creates sets of 3 agents with support for default and per-agent provider overrides, custom prompts, and filesystem-loaded prompts. System prompts are compiled into the binary from markdown files using `include_str!`.

## Dependencies

- **External crates**:
  - `async-trait = "0.1"` (already added in Section 06)
  - `tokio` (already in dependencies)
- **Internal sections**:
  - Section 01 (`error.rs`) -- `MagiError::Io` for filesystem prompt loading errors
  - Section 02 (`schema.rs`) -- `AgentName`, `Mode` enums for agent identity and mode selection
  - Section 06 (`provider.rs`) -- `LlmProvider` trait, `CompletionConfig` for the provider interface
- **Standard library**: `std::sync::Arc`, `std::path::Path`, `std::fs`, `std::collections::BTreeMap`

## Files to Create or Modify

| File | Action |
|------|--------|
| `magi-core/src/agent.rs` | Create -- contains `Agent` and `AgentFactory` |
| `magi-core/src/prompts/mod.rs` | Create -- module declarations for prompt submodules |
| `magi-core/src/prompts/melchior.rs` | Create -- `include_str!` constants and `prompt_for_mode` |
| `magi-core/src/prompts/balthasar.rs` | Create -- `include_str!` constants and `prompt_for_mode` |
| `magi-core/src/prompts/caspar.rs` | Create -- `include_str!` constants and `prompt_for_mode` |
| `magi-core/src/prompts_md/melchior_code_review.md` | Create -- system prompt markdown |
| `magi-core/src/prompts_md/melchior_design.md` | Create -- system prompt markdown |
| `magi-core/src/prompts_md/melchior_analysis.md` | Create -- system prompt markdown |
| `magi-core/src/prompts_md/balthasar_code_review.md` | Create -- system prompt markdown |
| `magi-core/src/prompts_md/balthasar_design.md` | Create -- system prompt markdown |
| `magi-core/src/prompts_md/balthasar_analysis.md` | Create -- system prompt markdown |
| `magi-core/src/prompts_md/caspar_code_review.md` | Create -- system prompt markdown |
| `magi-core/src/prompts_md/caspar_design.md` | Create -- system prompt markdown |
| `magi-core/src/prompts_md/caspar_analysis.md` | Create -- system prompt markdown |
| `magi-core/src/lib.rs` | Add `pub mod agent;` and `mod prompts;` |

## File Header

Every new source file in this project must start with:

```rust
// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05
```

## Tests (Write First -- Red Phase)

All tests go in `src/agent.rs` inside a `#[cfg(test)] mod tests` block. Use a manual mock or `mockall`-generated mock for `LlmProvider`.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::*;
    use std::sync::Arc;

    // Helper: create a mock LlmProvider that records calls
    // struct MockProvider { name: String, model: String, ... }

    // -- BDD Scenario 26: agents with different providers --

    /// Each agent uses its own provider (verify mock receives exactly 1 call).
    #[tokio::test]
    async fn test_each_agent_uses_its_own_provider() {
        // Create 3 distinct mock providers
        // Create AgentFactory with per-agent overrides
        // Create agents, execute each
        // Assert each mock received exactly 1 call
    }

    // -- BDD Scenario 27: factory with default and override --

    /// Factory uses default provider for agents without override, override for Caspar.
    #[tokio::test]
    async fn test_factory_default_and_override_providers() {
        // Create default mock provider
        // Create Caspar override mock provider
        // AgentFactory with default, then with_provider(Caspar, override)
        // Create agents
        // Assert Melchior and Balthasar use default provider name
        // Assert Caspar uses override provider name
    }

    // -- BDD Scenario 30: modes generate different prompts --

    /// CodeReview, Design, Analysis produce distinct system prompts per agent.
    #[test]
    fn test_different_modes_produce_distinct_prompts() {
        // Create mock provider
        // Create Agent::new(Melchior, Mode::CodeReview, provider)
        // Create Agent::new(Melchior, Mode::Design, provider)
        // Create Agent::new(Melchior, Mode::Analysis, provider)
        // Assert all three system_prompt() values are different
    }

    // -- BDD Scenario 31: from_directory with nonexistent path --

    /// from_directory returns MagiError::Io for nonexistent directory.
    #[test]
    fn test_from_directory_returns_io_error_for_nonexistent_path() {
        // Call AgentFactory::from_directory with "/nonexistent/path")
        // Assert Err matches MagiError::Io(_)
    }

    // -- Agent construction and accessors --

    /// Agent::new generates system prompt from include_str! prompts.
    #[test]
    fn test_agent_new_generates_system_prompt() {
        // Create Agent::new(Melchior, CodeReview, mock)
        // Assert system_prompt() is non-empty
    }

    /// Agent::with_custom_prompt uses provided prompt.
    #[test]
    fn test_agent_with_custom_prompt_uses_provided_prompt() {
        // Create Agent::with_custom_prompt(Melchior, CodeReview, mock, "Custom prompt")
        // Assert system_prompt() == "Custom prompt"
    }

    /// Agent::execute delegates to provider.complete with system prompt.
    #[tokio::test]
    async fn test_agent_execute_delegates_to_provider() {
        // Create mock that returns "response text"
        // Create agent, call execute("user input", &config)
        // Assert result == Ok("response text")
        // Assert mock was called with agent's system_prompt and "user input"
    }

    /// Agent accessors return correct values.
    #[test]
    fn test_agent_accessors() {
        // Create Agent::new(Balthasar, Design, mock)
        // Assert name() == AgentName::Balthasar
        // Assert mode() == Mode::Design
        // Assert provider_name() == mock.name()
        // Assert provider_model() == mock.model()
        // Assert display_name() == "Balthasar"
        // Assert title() == "Pragmatist"
    }

    // -- AgentFactory tests --

    /// AgentFactory::new creates 3 agents sharing default provider.
    #[test]
    fn test_agent_factory_creates_three_agents() {
        // Create factory with default mock
        // Call create_agents(Mode::CodeReview)
        // Assert exactly 3 agents returned
        // Assert agents are Melchior, Balthasar, Caspar (in some order)
    }

    /// AgentFactory::with_provider overrides provider for specific agent.
    #[test]
    fn test_agent_factory_with_provider_overrides_specific_agent() {
        // Create factory with default, add override for Caspar
        // Create agents
        // Assert Caspar has different provider_name than others
    }

    /// AgentFactory::with_custom_prompt overrides prompt for specific agent.
    #[test]
    fn test_agent_factory_with_custom_prompt_overrides_prompt() {
        // Create factory, add custom prompt for Melchior
        // Create agents
        // Assert Melchior's system_prompt matches custom text
        // Assert others have default prompts
    }

    /// AgentFactory::create_agents returns exactly 3 agents for any mode.
    #[test]
    fn test_agent_factory_creates_three_agents_for_all_modes() {
        // Create factory
        // For each mode (CodeReview, Design, Analysis):
        //   Assert create_agents returns exactly 3 agents
    }
}
```

## Implementation Details (Green Phase)

### System Prompts Module (`prompts/`)

The `prompts/` module provides compiled-in system prompts for each agent-mode combination. There are 9 markdown files (3 agents x 3 modes) in `src/prompts_md/`, embedded via `include_str!`.

#### `prompts/mod.rs`

Declares the three submodules:

```rust
pub mod melchior;
pub mod balthasar;
pub mod caspar;
```

#### `prompts/melchior.rs`, `prompts/balthasar.rs`, `prompts/caspar.rs`

Each file follows the same pattern:

```rust
use crate::schema::Mode;

const CODE_REVIEW: &str = include_str!("../prompts_md/melchior_code_review.md");
const DESIGN: &str = include_str!("../prompts_md/melchior_design.md");
const ANALYSIS: &str = include_str!("../prompts_md/melchior_analysis.md");

/// Returns the system prompt for this agent in the given mode.
pub fn prompt_for_mode(mode: &Mode) -> &'static str {
    match mode {
        Mode::CodeReview => CODE_REVIEW,
        Mode::Design => DESIGN,
        Mode::Analysis => ANALYSIS,
    }
}
```

#### Prompt Markdown Files (`prompts_md/`)

9 markdown files. Each prompt follows a structure ported from the Python original:

- Agent identity and analytical lens (e.g., Melchior is the Scientist -- methodical, evidence-based)
- Input format: the agent receives `MODE: {mode}\nCONTEXT:\n{content}`
- Focus areas specific to the mode (e.g., code review focuses on bugs, security, performance)
- Personality traits and analytical biases
- Constraints: respond in English, output valid JSON only, follow field length guidelines
- Exact JSON schema the agent must produce (matching `AgentOutput` struct)

The JSON schema in each prompt must match the `AgentOutput` struct fields: `agent`, `verdict`, `confidence`, `summary`, `reasoning`, `findings` (array of `{severity, title, detail}`), `recommendation`.

### `Agent` Struct

Each agent is an autonomous unit that combines an identity, a mode-specific system prompt, and an LLM provider.

- **Fields**:
  - `name: AgentName` -- which MAGI agent this is
  - `mode: Mode` -- the analysis mode
  - `system_prompt: String` -- the full system prompt (from `include_str!` or custom)
  - `provider: Arc<dyn LlmProvider>` -- the LLM backend for this agent

- **Constructors**:
  - `new(name: AgentName, mode: Mode, provider: Arc<dyn LlmProvider>) -> Self` -- auto-generates system prompt by dispatching to the appropriate `prompts::{agent}::prompt_for_mode(&mode)` based on `name`
  - `with_custom_prompt(name: AgentName, mode: Mode, provider: Arc<dyn LlmProvider>, prompt: String) -> Self` -- uses the provided prompt string as-is
  - `from_file(name: AgentName, mode: Mode, provider: Arc<dyn LlmProvider>, path: &Path) -> Result<Self, MagiError>` -- reads prompt from a filesystem path, returns `MagiError::Io` on failure (uses `std::fs::read_to_string` which returns `io::Error`, converted via `From<io::Error> for MagiError`)

- **Methods**:
  - `execute(&self, user_prompt: &str, config: &CompletionConfig) -> Result<String, ProviderError>` -- delegates to `self.provider.complete(self.system_prompt, user_prompt, config)`. Returns the raw LLM response string (parsing happens in the orchestrator).
  - `name(&self) -> AgentName` -- returns the agent's name enum
  - `mode(&self) -> Mode` -- returns the analysis mode
  - `system_prompt(&self) -> &str` -- returns the system prompt
  - `provider_name(&self) -> &str` -- delegates to `self.provider.name()`
  - `provider_model(&self) -> &str` -- delegates to `self.provider.model()`
  - `display_name(&self) -> &str` -- delegates to `self.name.display_name()`
  - `title(&self) -> &str` -- delegates to `self.name.title()`

### `AgentFactory` Struct

Creates sets of 3 agents with support for provider overrides and custom prompts.

- **Fields**:
  - `default_provider: Arc<dyn LlmProvider>` -- used for agents without a specific override
  - `agent_providers: BTreeMap<AgentName, Arc<dyn LlmProvider>>` -- per-agent provider overrides
  - `custom_prompts: BTreeMap<AgentName, String>` -- per-agent custom prompt overrides

- **Constructor**:
  - `new(default_provider: Arc<dyn LlmProvider>) -> Self` -- creates factory with default provider, empty overrides

- **Builder-style methods** (take `&mut self` or consume and return `Self`):
  - `with_provider(mut self, name: AgentName, provider: Arc<dyn LlmProvider>) -> Self` -- registers a provider override for a specific agent
  - `with_custom_prompt(mut self, name: AgentName, prompt: String) -> Self` -- registers a custom prompt for a specific agent
  - `from_directory(mut self, dir: &Path) -> Result<Self, MagiError>` -- loads custom prompts from a directory. Expected filenames: `{agent}_{mode}.md` (e.g., `melchior_code_review.md`). Only loads files that exist; missing files use the default `include_str!` prompts. Returns `MagiError::Io` if the directory itself does not exist.

- **Agent creation**:
  - `create_agents(&self, mode: Mode) -> Vec<Agent>` -- creates exactly 3 agents (Melchior, Balthasar, Caspar). For each agent:
    1. Select provider: `agent_providers.get(&name)` or fall back to `default_provider`
    2. Select prompt: if `custom_prompts` has entry for this agent, use `Agent::with_custom_prompt`; otherwise use `Agent::new` (which auto-selects the `include_str!` prompt for the mode)
    3. Return agents in a fixed order: `[Melchior, Balthasar, Caspar]`

### Prompt Selection Logic

When creating an agent via `Agent::new`, the system prompt is selected by agent name:

```rust
let prompt = match name {
    AgentName::Melchior => prompts::melchior::prompt_for_mode(&mode),
    AgentName::Balthasar => prompts::balthasar::prompt_for_mode(&mode),
    AgentName::Caspar => prompts::caspar::prompt_for_mode(&mode),
};
```

### `lib.rs` Module Declarations

Add to `src/lib.rs`:
- `pub mod agent;`
- `mod prompts;` (private -- only used internally by `agent.rs`)

## Constraints Checklist

- No `panic!`, `unwrap()`, `expect()` outside `#[cfg(test)]`
- No `unsafe`
- All public types and methods have `///` Rustdoc
- `Arc<dyn LlmProvider>` used for thread-safe shared ownership across agents
- `BTreeMap` for `agent_providers` and `custom_prompts` for deterministic ordering
- `include_str!` paths are relative to the source file location
- `from_file` and `from_directory` use `?` with `std::io::Error` → `MagiError::Io` conversion
- `rustfmt` and `clippy --tests -- -D warnings` clean
- File header present on all new `.rs` files

## Refactor Phase Notes

After Green phase passes all tests:

- Verify all 9 prompt markdown files are well-structured and match the Python original's intent
- Ensure `AgentFactory::create_agents` always returns agents in deterministic order
- Add Rustdoc `///` on all public types, constructors, and methods
- Confirm `cargo doc --no-deps` generates clean documentation
- Consider whether `from_directory` should log warnings for missing prompt files (via `tracing`)
- Verify that `Agent::execute` does not add any logic beyond delegation -- parsing belongs in the orchestrator
