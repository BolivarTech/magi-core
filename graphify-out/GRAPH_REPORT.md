# Graph Report - MAGI-Core  (2026-07-21)

## Corpus Check
- 30 files · ~59,203 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 905 nodes · 1951 edges · 39 communities (38 shown, 1 thin omitted)
- Extraction: 99% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 17 edges (avg confidence: 0.79)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `cd0a5a37`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- orchestrator.rs
- reporting.rs
- consensus.rs
- validate.rs
- Finding
- .new
- provider.rs
- openai_compat.rs
- claude.rs
- user_prompt.rs
- claude_cli.rs
- consensus
- Balthasar — The Pragmatist
- MAGI System Technical Documentation
- Caspar — The Critic
- Melchior — The Scientist
- .new
- mod.rs
- RoutingMockProvider
- MAGI System Technical Documentation
- finding_id.rs
- .cmp
- basic_analysis.rs
- AgentOutput
- magi_report_v0_3_1.json fixture
- make_output
- [0.5.0] - 2026-05-16
- Release workflow (publish to crates.io)
- [0.2.0] - 2026-04-18
- [0.4.0] - 2026-05-16
- [0.3.0] - 2026-04-18
- [0.6.0] - 2026-05-21
- [1.0.0] - 2026-05-24
- [0.3.1] - 2026-04-19
- [1.0.1] - 2026-05-25
- [1.1.1] - 2026-07-17

## God Nodes (most connected - your core abstractions)
1. `make_consensus()` - 30 edges
2. `make_agent()` - 29 edges
3. `build_user_prompt (injection defense)` - 26 edges
4. `parse_agent_response` - 23 edges
5. `make_output()` - 21 edges
6. `MagiBuilder` - 19 edges
7. `dispatch_one_agent (retry FSM)` - 19 edges
8. `ReportFormatter` - 19 edges
9. `mock_agent_json()` - 18 edges
10. `OpenAiCompatibleProvider` - 17 edges

## Surprising Connections (you probably didn't know these)
- `main()` --calls--> `default_model_for_mode()`  [INFERRED]
  examples/basic_analysis.rs → src/provider.rs
- `basic_analysis example` --calls--> `Magi orchestrator`  [INFERRED]
  examples/basic_analysis.rs → src/orchestrator.rs
- `RetryProvider` --semantically_similar_to--> `dispatch_one_agent (retry FSM)`  [INFERRED] [semantically similar]
  src/provider.rs → src/orchestrator.rs
- `test_lookup_prompt_falls_back_to_embedded_default_when_no_override()` --calls--> `lookup_prompt()`  [INFERRED]
  src/orchestrator.rs → src/prompts/mod.rs
- `dispatch_one_agent (retry FSM)` --calls--> `build_retry_prompt()`  [INFERRED]
  src/orchestrator.rs → src/user_prompt.rs

## Import Cycles
- None detected.

## Hyperedges (group relationships)
- **Magi analyze pipeline** — src_orchestrator_analyze, src_agent_agentfactory, src_user_prompt_build_user_prompt, src_orchestrator_dispatch_with_retry, src_consensus_consensusengine, src_reporting_reportformatter [EXTRACTED 0.90]
- **Agent dispatch, parse and validate** — src_orchestrator_dispatch_one_agent, src_orchestrator_parse_and_validate, src_orchestrator_parse_agent_response, src_orchestrator_embedded_verdict_object, src_validate_validator [EXTRACTED 0.85]
- **LlmProvider implementors** — src_providers_claude_claudeprovider, src_providers_claude_cli_claudecliprovider, src_providers_openai_compat_openaicompatibleprovider, src_provider_llmprovider [EXTRACTED 1.00]

## Communities (39 total, 1 thin omitted)

### Community 0 - "orchestrator.rs"
Cohesion: 0.05
Nodes (84): basic_analysis example, AbortGuard (RAII task abort), CapturingMockProvider, dispatch_one_agent (retry FSM), dispatch_with_retry, embedded_verdict_object (lenient recovery), Magi orchestrator, MagiBuilder (+76 more)

### Community 1 - "reporting.rs"
Cohesion: 0.08
Nodes (54): fit_content (ASCII banner truncation), make_agent(), make_consensus(), ReportConfig, ReportError, ReportFormatter, test_agent_display_fallback_to_agent_name_methods(), test_agent_line_format() (+46 more)

### Community 2 - "consensus.rs"
Cohesion: 0.08
Nodes (77): AgentName, AgentOutput, BTreeMap, Category, Default, Finding, MagiError, Mode (+69 more)

### Community 3 - "validate.rs"
Cohesion: 0.07
Nodes (35): clean_title, finding_with_title(), output_with_confidence(), output_with_findings(), test_clean_title_is_idempotent(), test_title_length_checked_after_strip_zero_width(), test_validate_accepts_confidence_at_boundaries(), test_validate_accepts_finding_with_normal_title() (+27 more)

### Community 4 - "Finding"
Cohesion: 0.40
Nodes (3): de_opt_line (fail-soft line), Finding, test_finding_with_location_and_category()

### Community 5 - ".new"
Cohesion: 0.12
Nodes (19): Agent, AgentFactory, MockProvider, test_agent_accessors(), test_agent_execute_delegates_to_provider(), test_agent_factory_creates_agents_in_order(), test_agent_factory_creates_three_agents(), test_agent_factory_creates_three_agents_for_all_modes() (+11 more)

### Community 6 - "provider.rs"
Cohesion: 0.15
Nodes (26): CompletionConfig, default_model_for_mode(), is_retryable, LlmProvider trait, MockProvider, resolve_claude_alias, RetryProvider, test_completion_config_default_values() (+18 more)

### Community 8 - "openai_compat.rs"
Cohesion: 0.08
Nodes (23): OpenAiCompatibleProvider::auth_header, OpenAiCompatibleProvider::build_request_body, OpenAiCompatibleProvider::endpoint_url, OpenAiChoice, OpenAiCompatibleProvider, OpenAiMessage, OpenAiRequest, OpenAiRespMessage (+15 more)

### Community 9 - "claude.rs"
Cohesion: 0.06
Nodes (29): MagiError, ProviderError, ClaudeProvider::build_request_body, ClaudeMessage, ClaudeProvider (HTTP), ClaudeRequest (HTTP body), ClaudeResponse, ContentBlock (+21 more)

### Community 10 - "user_prompt.rs"
Cohesion: 0.05
Nodes (46): build_retry_prompt(), build_user_prompt (injection defense), FastrandSource, fixed_nonce(), FixedRng, neutralize_headers, normalize_newlines(), RngLike (+38 more)

### Community 11 - "claude_cli.rs"
Cohesion: 0.15
Nodes (24): ClaudeCliProvider::build_args, ClaudeCliProvider (subprocess), CliOutput, parse_cli_output, strip_code_fences, test_build_args_includes_required_cli_flags(), test_new_claude_prefix_passes_through(), test_new_haiku_maps_to_claude_haiku_model() (+16 more)

### Community 12 - "consensus"
Cohesion: 0.12
Nodes (19): agents, banner, agent_count, conditions, confidence, consensus, consensus_verdict, dissent (+11 more)

### Community 13 - "Balthasar — The Pragmatist"
Cohesion: 0.17
Nodes (11): Balthasar — The Pragmatist, Constraints, Finding calibration (code-review mode only), In analysis mode, In code review mode, In design mode, Input format, Output format (+3 more)

### Community 14 - "MAGI System Technical Documentation"
Cohesion: 0.12
Nodes (16): bool, bytes, main(), int, main(), int, Path, str (+8 more)

### Community 15 - "Caspar — The Critic"
Cohesion: 0.17
Nodes (11): Caspar — The Critic, Constraints, Finding calibration (code-review mode only), In analysis mode, In code review mode, In design mode, Input format, Output format (+3 more)

### Community 16 - "Melchior — The Scientist"
Cohesion: 0.17
Nodes (11): Constraints, Finding calibration (code-review mode only), In analysis mode, In code review mode, In design mode, Input format, Melchior — The Scientist, Output format (+3 more)

### Community 17 - ".new"
Cohesion: 0.22
Nodes (8): test_agent_name_btreeset_orders_alphabetically(), test_agent_name_usable_as_btreemap_key(), test_finding_new_constructs_three_field_finding(), test_finding_new_defaults_optional_fields(), test_finding_serde_roundtrip(), test_finding_serializes_file_line_null_category_always(), test_finding_stripped_title_preserves_normal_text(), test_finding_stripped_title_removes_zero_width_characters()

### Community 18 - "mod.rs"
Cohesion: 0.26
Nodes (5): balthasar_prompt(), caspar_prompt(), embedded_prompt_for(), melchior_prompt(), test_prompts_match_pinned_reference_sha256()

### Community 19 - "RoutingMockProvider"
Cohesion: 0.33
Nodes (6): CURRENT_AGENT_IDENTITY task-local, RoutingMockProvider, test_routing_mock_provider_can_inject_provider_errors(), test_routing_mock_provider_exhausted_sequence_errors(), test_routing_mock_provider_fails_when_no_task_local_scope(), test_routing_mock_provider_routes_by_task_local_identity()

### Community 20 - "MAGI System Technical Documentation"
Cohesion: 0.20
Nodes (9): Voting rules + confidence formula, Evangelion MAGI origin (Naoko Akagi), MAGI System Technical Documentation, Structured disagreement rationale, Why three perspectives (not 2 or 5), Exemption from CLAUDE.local.md §0.2 file-header rule, Local divergence from the pinned reference (F0, 2026-07-16), Regeneration (+1 more)

### Community 22 - "finding_id.rs"
Cohesion: 0.24
Nodes (6): de_category(), generate_finding_id (SHA-256), normalize_category(), normalize_path, test_generate_finding_id_shape(), Category

### Community 23 - ".cmp"
Cohesion: 0.33
Nodes (3): AgentName, Mode, Severity

### Community 24 - "basic_analysis.rs"
Cohesion: 0.39
Nodes (8): create_provider(), main(), parse_mode(), print_usage(), ProviderArgs, read_input(), setup_console_encoding(), test_setup_console_encoding_runs_without_panic()

### Community 25 - "AgentOutput"
Cohesion: 0.28
Nodes (4): Magi::analyze, MagiReport, AgentOutput, Verdict

### Community 27 - "make_output"
Cohesion: 0.33
Nodes (6): make_output(), test_agent_output_conditional_is_not_dissenting_from_approve_majority(), test_agent_output_effective_verdict_maps_conditional_to_approve(), test_agent_output_empty_findings_valid(), test_agent_output_is_dissenting_when_verdict_differs_from_majority(), test_agent_output_is_not_dissenting_when_verdict_matches_majority()

### Community 28 - "[0.5.0] - 2026-05-16"
Cohesion: 0.25
Nodes (8): [0.5.0] - 2026-05-16, Added, Backward compatibility, Changed (breaking), Documentation, Performance, Pre-merge gates (CLAUDE.local.md §6), Test count

### Community 29 - "Release workflow (publish to crates.io)"
Cohesion: 0.25
Nodes (7): [0.1.2] - 2026-04-05, [1.1.0] - 2026-05-25, Added, Changelog, Notes, CI workflow (test/clippy/fmt/audit/doc), Release workflow (publish to crates.io)

### Community 30 - "[0.2.0] - 2026-04-18"
Cohesion: 0.29
Nodes (7): [0.2.0] - 2026-04-18, Added, Changed (breaking), Dependencies, Deprecated, Not included (deferred to v0.3.0), Security considerations

### Community 32 - "[0.4.0] - 2026-05-16"
Cohesion: 0.29
Nodes (7): [0.4.0] - 2026-05-16, Added, Backward compatibility, Changed, Documentation, Performance, Test count

### Community 33 - "[0.3.0] - 2026-04-18"
Cohesion: 0.33
Nodes (6): [0.3.0] - 2026-04-18, Added, Changed (breaking), Dependencies, Not included (deferred beyond v0.3.0), Security considerations (MAGI R3 W8)

### Community 34 - "[0.6.0] - 2026-05-21"
Cohesion: 0.33
Nodes (6): [0.6.0] - 2026-05-21, Backward compatibility, Changed, Pre-merge gates (CLAUDE.local.md §6), Security, Test count

### Community 35 - "[1.0.0] - 2026-05-24"
Cohesion: 0.40
Nodes (5): [1.0.0] - 2026-05-24, Added, Changed (breaking), Notes, Security

### Community 36 - "[0.3.1] - 2026-04-19"
Cohesion: 0.67
Nodes (3): [0.3.1] - 2026-04-19, Fixed, Yanked

### Community 37 - "[1.0.1] - 2026-05-25"
Cohesion: 0.67
Nodes (3): [1.0.1] - 2026-05-25, Fixed, Internal

### Community 38 - "[1.1.1] - 2026-07-17"
Cohesion: 0.67
Nodes (3): [1.1.1] - 2026-07-17, Changed, Fixed

## Knowledge Gaps
- **112 isolated node(s):** `Fixed`, `Changed`, `Added`, `Notes`, `Fixed` (+107 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **1 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Magi::analyze` connect `AgentOutput` to `orchestrator.rs`, `reporting.rs`, `consensus.rs`, `.new`, `user_prompt.rs`?**
  _High betweenness centrality (0.216) - this node is a cross-community bridge._
- **Why does `dispatch_one_agent (retry FSM)` connect `orchestrator.rs` to `user_prompt.rs`, `.new`, `provider.rs`?**
  _High betweenness centrality (0.192) - this node is a cross-community bridge._
- **Why does `LlmProvider trait` connect `provider.rs` to `orchestrator.rs`, `.new`, `openai_compat.rs`, `claude.rs`, `claude_cli.rs`, `RoutingMockProvider`?**
  _High betweenness centrality (0.185) - this node is a cross-community bridge._
- **Are the 3 inferred relationships involving `build_user_prompt (injection defense)` (e.g. with `dispatch_one_agent (retry FSM)` and `.analyze()`) actually correct?**
  _`build_user_prompt (injection defense)` has 3 INFERRED edges - model-reasoned connections that need verification._
- **What connects `Fixed`, `Changed`, `Added` to the rest of the system?**
  _117 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `orchestrator.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.05292353823088456 - nodes in this community are weakly interconnected._
- **Should `reporting.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.08191808191808192 - nodes in this community are weakly interconnected._