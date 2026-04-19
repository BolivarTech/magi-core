<!-- PROJECT_CONFIG
runtime: rust-cargo
test_command: cargo nextest run
END_PROJECT_CONFIG -->

<!-- SECTION_MANIFEST
section-01-errors
section-02-schema
section-03-validation
section-04-consensus
section-05-reporting
section-06-provider
section-07-agents
section-08-orchestrator
section-09-claude-api
section-10-claude-cli
section-11-prelude
section-12-example
END_MANIFEST -->

# Implementation Sections Index

## Dependency Graph

| Section | Depends On | Blocks | Parallelizable |
|---------|------------|--------|----------------|
| section-01-errors | - | all | Yes |
| section-02-schema | 01 | 03, 04, 05, 07 | Yes |
| section-03-validation | 01, 02 | 08 | Yes |
| section-04-consensus | 01, 02 | 05, 08 | Yes |
| section-05-reporting | 01, 02, 04 | 08 | No |
| section-06-provider | 01 | 07, 09, 10 | Yes |
| section-07-agents | 01, 02, 06 | 08 | No |
| section-08-orchestrator | 01-07 | 11, 12 | No |
| section-09-claude-api | 01, 06 | 12 | Yes |
| section-10-claude-cli | 01, 06 | 12 | Yes |
| section-11-prelude | 01-10 | 12 | No |
| section-12-example | 01-11 | - | No |

## Execution Order

1. section-01-errors (no dependencies)
2. section-02-schema, section-06-provider (parallel after 01)
3. section-03-validation, section-04-consensus (parallel after 02)
4. section-05-reporting (after 04)
5. section-07-agents (after 02, 06)
6. section-08-orchestrator (after all core modules)
7. section-09-claude-api, section-10-claude-cli (parallel after 06)
8. section-11-prelude (after all modules)
9. section-12-example (final)

## Section Summaries

### section-01-errors
`error.rs` — MagiError and ProviderError enums with thiserror. Foundation for all other modules.

### section-02-schema
`schema.rs` — Verdict, Severity, Mode, AgentName enums with encapsulated behavior. Finding and AgentOutput structs. All derive Serialize/Deserialize/Clone/Debug/PartialEq/Eq/Hash.

### section-03-validation
`validate.rs` — Validator struct with ValidationLimits and precompiled Regex. Validates AgentOutput fields (confidence, text lengths, findings).

### section-04-consensus
`consensus.rs` — ConsensusEngine with ConsensusConfig. Stateless determine() method. Score computation, epsilon-aware classification, finding deduplication, majority/dissent identification. ConsensusResult, DedupFinding, Dissent, Condition structs.

### section-05-reporting
`reporting.rs` — ReportFormatter with ReportConfig. Fixed-width 52-char ASCII banner. Markdown report generation. MagiReport struct.

### section-06-provider
`provider.rs` — LlmProvider async trait (Send + Sync). CompletionConfig. RetryProvider opt-in wrapper.

### section-07-agents
`agent.rs` + `prompts/` — Agent struct with Arc<dyn LlmProvider>. AgentFactory with default and per-agent providers. System prompts from include_str! .md files (9 files: 3 agents x 3 modes).

### section-08-orchestrator
`orchestrator.rs` — Magi struct as main entry point. MagiBuilder with consuming method chaining. MagiConfig. analyze() method orchestrating full flow with JoinSet. parse_agent_response for LLM output robustness.

### section-09-claude-api
`providers/claude.rs` — ClaudeProvider implementing LlmProvider via reqwest HTTP client. Claude Messages API integration. Feature-gated: claude-api.

### section-10-claude-cli
`providers/claude_cli.rs` — ClaudeCliProvider implementing LlmProvider via tokio::process. Model alias whitelist, CLAUDECODE env check, double-nested JSON parsing, code fence stripping. Feature-gated: claude-cli.

### section-11-prelude
`prelude.rs` + `lib.rs` — Prelude module re-exporting common types. Crate root with module declarations and feature-gated conditional compilation.

### section-12-example
`examples/basic_analysis.rs` — Example binary using ClaudeCliProvider by default with CLI arg support for selecting other providers.
