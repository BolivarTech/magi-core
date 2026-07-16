# Graph Report - MAGI-Core  (2026-07-16)

## Corpus Check
- 30 files · ~58,295 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 809 nodes · 1839 edges · 26 communities (22 shown, 4 thin omitted)
- Extraction: 99% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 25 edges (avg confidence: 0.82)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `7da50ff1`
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
- MAGI System Technical Documentation
- .new
- mod.rs
- RoutingMockProvider
- finding_id.rs
- .cmp
- basic_analysis.rs
- AgentOutput
- make_output
- Release workflow (publish to crates.io)
- Mode

## God Nodes (most connected - your core abstractions)
1. `make_consensus()` - 30 edges
2. `make_agent()` - 29 edges
3. `build_user_prompt (injection defense)` - 26 edges
4. `parse_agent_response` - 23 edges
5. `MagiBuilder` - 19 edges
6. `dispatch_one_agent (retry FSM)` - 19 edges
7. `ReportFormatter` - 19 edges
8. `make_output()` - 18 edges
9. `mock_agent_json()` - 18 edges
10. `OpenAiCompatibleProvider` - 17 edges

## Surprising Connections (you probably didn't know these)
- `7-key agent JSON output schema` --shares_data_with--> `magi_report_v0_3_1.json fixture`  [INFERRED]
  src/prompts_md/melchior.md → tests/fixtures/magi_report_v0_3_1.json
- `main()` --calls--> `default_model_for_mode()`  [INFERRED]
  examples/basic_analysis.rs → src/provider.rs
- `basic_analysis example` --calls--> `Magi orchestrator`  [INFERRED]
  examples/basic_analysis.rs → src/orchestrator.rs
- `prompts_md README (byte-parity exemption)` --references--> `MAGI_REF_SHA pin (Python MAGI v3.0.0)`  [EXTRACTED]
  src/prompts_md/README.md → tests/fixtures/_magi_ref.py
- `RetryProvider` --semantically_similar_to--> `dispatch_one_agent (retry FSM)`  [INFERRED] [semantically similar]
  src/provider.rs → src/orchestrator.rs

## Import Cycles
- None detected.

## Hyperedges (group relationships)
- **Magi analyze pipeline** — src_orchestrator_analyze, src_agent_agentfactory, src_user_prompt_build_user_prompt, src_orchestrator_dispatch_with_retry, src_consensus_consensusengine, src_reporting_reportformatter [EXTRACTED 0.90]
- **Finding deduplication keying flow** — src_consensus_deduplicate_findings, src_consensus_finding_key, src_finding_id_generate_finding_id, src_consensus_dedup_key, src_validate_clean_title [EXTRACTED 0.85]
- **Agent dispatch, parse and validate** — src_orchestrator_dispatch_one_agent, src_orchestrator_parse_and_validate, src_orchestrator_parse_agent_response, src_orchestrator_embedded_verdict_object, src_validate_validator [EXTRACTED 0.85]
- **Three MAGI agent prompts (multi-perspective consensus)** — prompts_melchior, prompts_balthasar, prompts_caspar [EXTRACTED 1.00]
- **LlmProvider implementors** — src_providers_claude_claudeprovider, src_providers_claude_cli_claudecliprovider, src_providers_openai_compat_openaicompatibleprovider, src_provider_llmprovider [EXTRACTED 1.00]
- **MAGI prompt extraction/hashing fixture pipeline** — tests_fixtures_magi_ref_source_of_truth, magi_ref_extract_prompts, magi_ref_gen_prompts [EXTRACTED 1.00]

## Communities (26 total, 4 thin omitted)

### Community 0 - "orchestrator.rs"
Cohesion: 0.05
Nodes (85): basic_analysis example, AbortGuard (RAII task abort), Magi::analyze, CapturingMockProvider, dispatch_one_agent (retry FSM), dispatch_with_retry, embedded_verdict_object (lenient recovery), Magi orchestrator (+77 more)

### Community 1 - "reporting.rs"
Cohesion: 0.08
Nodes (54): fit_content (ASCII banner truncation), make_agent(), make_consensus(), ReportConfig, ReportError, ReportFormatter, test_agent_display_fallback_to_agent_name_methods(), test_agent_line_format() (+46 more)

### Community 2 - "consensus.rs"
Cohesion: 0.09
Nodes (63): classify (score to label), Condition, ConsensusConfig, ConsensusEngine, ConsensusResult, dedup_key (title normalization), DedupFinding, DedupKey (+55 more)

### Community 3 - "validate.rs"
Cohesion: 0.07
Nodes (35): clean_title, finding_with_title(), output_with_confidence(), output_with_findings(), test_clean_title_is_idempotent(), test_title_length_checked_after_strip_zero_width(), test_validate_accepts_confidence_at_boundaries(), test_validate_accepts_finding_with_normal_title() (+27 more)

### Community 4 - "Finding"
Cohesion: 0.40
Nodes (3): de_opt_line (fail-soft line), Finding, test_finding_with_location_and_category()

### Community 5 - ".new"
Cohesion: 0.11
Nodes (22): embedded_prompt_for, lookup_prompt, Agent, AgentFactory, CURRENT_AGENT_IDENTITY task-local, MockProvider, test_agent_accessors(), test_agent_execute_delegates_to_provider() (+14 more)

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

### Community 14 - "MAGI System Technical Documentation"
Cohesion: 0.07
Nodes (30): bool, bytes, Voting rules + confidence formula, Evangelion MAGI origin (Naoko Akagi), MAGI System Technical Documentation, Structured disagreement rationale, Why three perspectives (not 2 or 5), magi_report_v0_3_1.json fixture (+22 more)

### Community 17 - ".new"
Cohesion: 0.22
Nodes (8): test_agent_name_btreeset_orders_alphabetically(), test_agent_name_usable_as_btreemap_key(), test_finding_new_constructs_three_field_finding(), test_finding_new_defaults_optional_fields(), test_finding_serde_roundtrip(), test_finding_serializes_file_line_null_category_always(), test_finding_stripped_title_preserves_normal_text(), test_finding_stripped_title_removes_zero_width_characters()

### Community 18 - "mod.rs"
Cohesion: 0.24
Nodes (7): test_lookup_prompt_falls_back_to_embedded_default_when_no_override(), balthasar_prompt(), caspar_prompt(), embedded_prompt_for(), lookup_prompt(), melchior_prompt(), test_prompts_match_python_reference_sha256()

### Community 19 - "RoutingMockProvider"
Cohesion: 0.37
Nodes (5): RoutingMockProvider, test_routing_mock_provider_can_inject_provider_errors(), test_routing_mock_provider_exhausted_sequence_errors(), test_routing_mock_provider_fails_when_no_task_local_scope(), test_routing_mock_provider_routes_by_task_local_identity()

### Community 22 - "finding_id.rs"
Cohesion: 0.24
Nodes (6): de_category(), generate_finding_id (SHA-256), normalize_category(), normalize_path, test_generate_finding_id_shape(), Category

### Community 24 - "basic_analysis.rs"
Cohesion: 0.39
Nodes (8): create_provider(), main(), parse_mode(), print_usage(), ProviderArgs, read_input(), setup_console_encoding(), test_setup_console_encoding_runs_without_panic()

### Community 27 - "make_output"
Cohesion: 0.33
Nodes (6): make_output(), test_agent_output_conditional_is_not_dissenting_from_approve_majority(), test_agent_output_effective_verdict_maps_conditional_to_approve(), test_agent_output_empty_findings_valid(), test_agent_output_is_dissenting_when_verdict_differs_from_majority(), test_agent_output_is_not_dissenting_when_verdict_matches_majority()

## Knowledge Gaps
- **44 isolated node(s):** `ProviderArgs`, `Dissent`, `Condition`, `DedupKey`, `RngLike` (+39 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **4 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `ConsensusEngine` connect `consensus.rs` to `orchestrator.rs`, `claude.rs`, `AgentOutput`?**
  _High betweenness centrality (0.193) - this node is a cross-community bridge._
- **Why does `Magi::analyze` connect `orchestrator.rs` to `reporting.rs`, `consensus.rs`, `user_prompt.rs`, `.new`?**
  _High betweenness centrality (0.182) - this node is a cross-community bridge._
- **Why does `dispatch_one_agent (retry FSM)` connect `orchestrator.rs` to `user_prompt.rs`, `.new`, `provider.rs`?**
  _High betweenness centrality (0.179) - this node is a cross-community bridge._
- **Are the 3 inferred relationships involving `build_user_prompt (injection defense)` (e.g. with `dispatch_one_agent (retry FSM)` and `.analyze()`) actually correct?**
  _`build_user_prompt (injection defense)` has 3 INFERRED edges - model-reasoned connections that need verification._
- **What connects `ProviderArgs`, `Dissent`, `Condition` to the rest of the system?**
  _51 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `orchestrator.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.052166224580017684 - nodes in this community are weakly interconnected._
- **Should `reporting.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.08191808191808192 - nodes in this community are weakly interconnected._