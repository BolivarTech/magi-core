# Graph Report - .  (2026-05-29)

## Corpus Check
- 32 files · ~58,295 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 876 nodes · 1848 edges · 35 communities (26 shown, 9 thin omitted)
- Extraction: 99% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 27 edges (avg confidence: 0.82)
- Token cost: 0 input · 260,378 output

## Community Hubs (Navigation)
- [[_COMMUNITY_Orchestrator Dispatch|Orchestrator Dispatch]]
- [[_COMMUNITY_Report Formatting|Report Formatting]]
- [[_COMMUNITY_Consensus Engine|Consensus Engine]]
- [[_COMMUNITY_Title Validation|Title Validation]]
- [[_COMMUNITY_ProviderConsensus Cross-Cut|Provider/Consensus Cross-Cut]]
- [[_COMMUNITY_Agent & Factory|Agent & Factory]]
- [[_COMMUNITY_LlmProvider Trait & Retry|LlmProvider Trait & Retry]]
- [[_COMMUNITY_Schema Serde Tests|Schema Serde Tests]]
- [[_COMMUNITY_OpenAI-Compatible Provider|OpenAI-Compatible Provider]]
- [[_COMMUNITY_Claude HTTP Provider|Claude HTTP Provider]]
- [[_COMMUNITY_Prompt Header Neutralization|Prompt Header Neutralization]]
- [[_COMMUNITY_Claude CLI Provider|Claude CLI Provider]]
- [[_COMMUNITY_Report JSON Fixture|Report JSON Fixture]]
- [[_COMMUNITY_User Prompt Builder|User Prompt Builder]]
- [[_COMMUNITY_MAGI Domain Docs & Prompts|MAGI Domain Docs & Prompts]]
- [[_COMMUNITY_Python Reference Fixtures|Python Reference Fixtures]]
- [[_COMMUNITY_Error Types|Error Types]]
- [[_COMMUNITY_Finding Type|Finding Type]]
- [[_COMMUNITY_Embedded Prompt Loading|Embedded Prompt Loading]]
- [[_COMMUNITY_Routing Mock Provider|Routing Mock Provider]]
- [[_COMMUNITY_Retry Prompt Builder|Retry Prompt Builder]]
- [[_COMMUNITY_Finding ID Generation|Finding ID Generation]]
- [[_COMMUNITY_AgentName & Severity|AgentName & Severity]]
- [[_COMMUNITY_Basic Analysis Example|Basic Analysis Example]]
- [[_COMMUNITY_Verdict & AgentOutput|Verdict & AgentOutput]]
- [[_COMMUNITY_Prompt Sanitization Helpers|Prompt Sanitization Helpers]]
- [[_COMMUNITY_AgentOutput Dissent Tests|AgentOutput Dissent Tests]]
- [[_COMMUNITY_RNG Test Helpers|RNG Test Helpers]]
- [[_COMMUNITY_CICD Workflows|CI/CD Workflows]]
- [[_COMMUNITY_Mode Enum|Mode Enum]]
- [[_COMMUNITY_Community 33|Community 33]]
- [[_COMMUNITY_Community 34|Community 34]]

## God Nodes (most connected - your core abstractions)
1. `make_consensus()` - 30 edges
2. `make_agent()` - 29 edges
3. `build_user_prompt()` - 23 edges
4. `parse_agent_response()` - 22 edges
5. `make_output()` - 18 edges
6. `mock_agent_json()` - 18 edges
7. `MagiBuilder` - 17 edges
8. `dispatch_one_agent()` - 15 edges
9. `ReportFormatter` - 15 edges
10. `build_retry_prompt()` - 15 edges

## Surprising Connections (you probably didn't know these)
- `7-key agent JSON output schema` --shares_data_with--> `magi_report_v0_3_1.json fixture`  [INFERRED]
  src/prompts_md/melchior.md → tests/fixtures/magi_report_v0_3_1.json
- `main()` --calls--> `default_model_for_mode()`  [INFERRED]
  examples/basic_analysis.rs → src/provider.rs
- `prompts_md README (byte-parity exemption)` --references--> `MAGI_REF_SHA pin (Python MAGI v3.0.0)`  [EXTRACTED]
  src/prompts_md/README.md → tests/fixtures/_magi_ref.py
- `basic_analysis example` --calls--> `Magi orchestrator`  [INFERRED]
  examples/basic_analysis.rs → src/orchestrator.rs
- `MAGI System Technical Documentation` --conceptually_related_to--> `Melchior — The Scientist (prompt)`  [INFERRED]
  docs/MAGI-System-Documentation.md → src/prompts_md/melchior.md

## Hyperedges (group relationships)
- **Magi analyze pipeline** — orchestrator_analyze, agent_AgentFactory, user_prompt_build_user_prompt, orchestrator_dispatch_with_retry, consensus_ConsensusEngine, reporting_ReportFormatter [EXTRACTED 0.90]
- **Finding deduplication keying flow** — consensus_deduplicate_findings, consensus_finding_key, finding_id_generate_finding_id, consensus_dedup_key, validate_clean_title [EXTRACTED 0.85]
- **Agent dispatch, parse and validate** — orchestrator_dispatch_one_agent, orchestrator_parse_and_validate, orchestrator_parse_agent_response, orchestrator_embedded_verdict_object, validate_Validator [EXTRACTED 0.85]
- **Three MAGI agent prompts (multi-perspective consensus)** — prompts_melchior, prompts_balthasar, prompts_caspar [EXTRACTED 1.00]
- **LlmProvider implementors** — claude_ClaudeProvider, claude_cli_ClaudeCliProvider, openai_compat_OpenAiCompatibleProvider, provider_LlmProvider [EXTRACTED 1.00]
- **MAGI prompt extraction/hashing fixture pipeline** — magi_ref_source_of_truth, magi_ref_extract_prompts, magi_ref_gen_prompts [EXTRACTED 1.00]

## Communities (35 total, 9 thin omitted)

### Community 0 - "Orchestrator Dispatch"
Cohesion: 0.05
Nodes (80): AbortGuard, CapturingMockProvider, dispatch_one_agent(), embedded_verdict_object(), Magi, MagiBuilder, MagiConfig, mock_agent_json() (+72 more)

### Community 1 - "Report Formatting"
Cohesion: 0.08
Nodes (55): fit_content(), MagiReport, make_agent(), make_consensus(), ReportConfig, ReportError, ReportFormatter, test_agent_display_fallback_to_agent_name_methods() (+47 more)

### Community 2 - "Consensus Engine"
Cohesion: 0.10
Nodes (61): Condition, ConsensusConfig, ConsensusEngine, ConsensusResult, dedup_key(), DedupFinding, DedupKey, Dissent (+53 more)

### Community 3 - "Title Validation"
Cohesion: 0.06
Nodes (35): clean_title(), finding_with_title(), output_with_confidence(), output_with_findings(), test_clean_title_is_idempotent(), test_title_length_checked_after_strip_zero_width(), test_validate_accepts_confidence_at_boundaries(), test_validate_accepts_finding_with_normal_title() (+27 more)

### Community 4 - "Provider/Consensus Cross-Cut"
Cohesion: 0.05
Nodes (59): Agent, AgentFactory, CURRENT_AGENT_IDENTITY task-local, ClaudeProvider (HTTP), ClaudeRequest (HTTP body), ClaudeProvider::build_request_body, ClaudeCliProvider (subprocess), ClaudeCliProvider::build_args (+51 more)

### Community 5 - "Agent & Factory"
Cohesion: 0.10
Nodes (19): Agent, AgentFactory, MockProvider, test_agent_accessors(), test_agent_execute_delegates_to_provider(), test_agent_factory_creates_agents_in_order(), test_agent_factory_creates_three_agents(), test_agent_factory_creates_three_agents_for_all_modes() (+11 more)

### Community 6 - "LlmProvider Trait & Retry"
Cohesion: 0.12
Nodes (26): CompletionConfig, default_model_for_mode(), is_retryable(), LlmProvider, MockProvider, resolve_claude_alias(), RetryProvider, test_completion_config_default_values() (+18 more)

### Community 8 - "OpenAI-Compatible Provider"
Cohesion: 0.09
Nodes (20): OpenAiChoice, OpenAiCompatibleProvider, OpenAiMessage, OpenAiRequest, OpenAiRespMessage, OpenAiResponse, test_auth_header_none_when_key_absent(), test_auth_header_some_when_key_present() (+12 more)

### Community 9 - "Claude HTTP Provider"
Cohesion: 0.11
Nodes (22): ClaudeMessage, ClaudeProvider, ClaudeRequest, ClaudeResponse, ContentBlock, test_build_request_body_contains_all_required_fields(), test_claude_provider_model_returns_configured_model(), test_claude_provider_name_returns_claude() (+14 more)

### Community 11 - "Claude CLI Provider"
Cohesion: 0.15
Nodes (23): ClaudeCliProvider, CliOutput, parse_cli_output(), strip_code_fences(), test_build_args_includes_required_cli_flags(), test_new_claude_prefix_passes_through(), test_new_haiku_maps_to_claude_haiku_model(), test_new_invalid_model_returns_auth_error() (+15 more)

### Community 12 - "Report JSON Fixture"
Cohesion: 0.09
Nodes (22): agents, banner, consensus, agent_count, conditions, confidence, consensus, consensus_verdict (+14 more)

### Community 13 - "User Prompt Builder"
Cohesion: 0.18
Nodes (20): build_user_prompt(), fixed_nonce(), test_build_user_prompt_accepts_empty_content(), test_build_user_prompt_all_5_unicode_separators_positive_neutralization(), test_build_user_prompt_benign_content_canonical_format(), test_build_user_prompt_case_variant_headers_not_neutralized(), test_build_user_prompt_does_not_neutralize_wide_keywords(), test_build_user_prompt_leading_whitespace_does_not_bypass_neutralization() (+12 more)

### Community 14 - "MAGI Domain Docs & Prompts"
Cohesion: 0.13
Nodes (16): Voting rules + confidence formula, Evangelion MAGI origin (Naoko Akagi), MAGI System Technical Documentation, Structured disagreement rationale, Why three perspectives (not 2 or 5), magi_report_v0_3_1.json fixture, MAGI_REF_SHA pin (Python MAGI v3.0.0), _magi_ref.py (single source of truth) (+8 more)

### Community 15 - "Python Reference Fixtures"
Cohesion: 0.12
Nodes (14): bool, bytes, main(), main(), MAGI R1 W4: pre-write check that the pinned SHA exists in the repo     before r, verify_sha_exists(), Read a file's bytes at a specific ref via `git show`, no checkout., read_blob() (+6 more)

### Community 17 - "Finding Type"
Cohesion: 0.16
Nodes (10): Finding, test_agent_name_btreeset_orders_alphabetically(), test_agent_name_usable_as_btreemap_key(), test_finding_new_constructs_three_field_finding(), test_finding_new_defaults_optional_fields(), test_finding_serde_roundtrip(), test_finding_serializes_file_line_null_category_always(), test_finding_stripped_title_preserves_normal_text() (+2 more)

### Community 18 - "Embedded Prompt Loading"
Cohesion: 0.24
Nodes (7): balthasar_prompt(), caspar_prompt(), embedded_prompt_for(), lookup_prompt(), melchior_prompt(), test_prompts_match_python_reference_sha256(), test_lookup_prompt_falls_back_to_embedded_default_when_no_override()

### Community 19 - "Routing Mock Provider"
Cohesion: 0.37
Nodes (5): RoutingMockProvider, test_routing_mock_provider_can_inject_provider_errors(), test_routing_mock_provider_exhausted_sequence_errors(), test_routing_mock_provider_fails_when_no_task_local_scope(), test_routing_mock_provider_routes_by_task_local_identity()

### Community 20 - "Retry Prompt Builder"
Cohesion: 0.15
Nodes (13): build_retry_prompt(), test_build_retry_prompt_appends_feedback_block_exact_format(), test_build_retry_prompt_does_not_neutralize_midline_tokens(), test_build_retry_prompt_does_not_resanitize_content(), test_build_retry_prompt_feedback_block_after_end_delimiter(), test_build_retry_prompt_includes_seven_keys_list(), test_build_retry_prompt_neutralizes_dash_variant_retry_markers(), test_build_retry_prompt_neutralizes_injected_retry_feedback_marker() (+5 more)

### Community 22 - "Finding ID Generation"
Cohesion: 0.22
Nodes (4): de_category(), generate_finding_id(), normalize_category(), test_generate_finding_id_shape()

### Community 24 - "Basic Analysis Example"
Cohesion: 0.39
Nodes (8): create_provider(), main(), parse_mode(), print_usage(), ProviderArgs, read_input(), setup_console_encoding(), test_setup_console_encoding_runs_without_panic()

### Community 26 - "Prompt Sanitization Helpers"
Cohesion: 0.25
Nodes (8): neutralize_headers(), normalize_newlines(), sanitize_error_for_retry_feedback(), strip_invisibles(), test_neutralize_headers_preserves_unmatched_lines_borrowed(), test_normalize_newlines_handles_empty_string(), test_normalize_newlines_preserves_existing_lf_borrows(), test_strip_invisibles_preserves_regular_text()

### Community 27 - "AgentOutput Dissent Tests"
Cohesion: 0.33
Nodes (6): make_output(), test_agent_output_conditional_is_not_dissenting_from_approve_majority(), test_agent_output_effective_verdict_maps_conditional_to_approve(), test_agent_output_empty_findings_valid(), test_agent_output_is_dissenting_when_verdict_differs_from_majority(), test_agent_output_is_not_dissenting_when_verdict_matches_majority()

### Community 28 - "RNG Test Helpers"
Cohesion: 0.50
Nodes (3): FixedRng, test_fastrand_source_returns_distinct_values_across_calls(), test_fixed_rng_panics_when_exhausted()

## Knowledge Gaps
- **63 isolated node(s):** `ProviderArgs`, `ConsensusResult`, `DedupFinding`, `Dissent`, `Condition` (+58 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **9 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `dedup_key()` connect `Consensus Engine` to `Title Validation`?**
  _High betweenness centrality (0.015) - this node is a cross-community bridge._
- **Why does `clean_title()` connect `Title Validation` to `Consensus Engine`?**
  _High betweenness centrality (0.015) - this node is a cross-community bridge._
- **Why does `dispatch_one_agent()` connect `Orchestrator Dispatch` to `Retry Prompt Builder`?**
  _High betweenness centrality (0.013) - this node is a cross-community bridge._
- **What connects `ProviderArgs`, `ConsensusResult`, `DedupFinding` to the rest of the system?**
  _72 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Orchestrator Dispatch` be split into smaller, more focused modules?**
  _Cohesion score 0.05172413793103448 - nodes in this community are weakly interconnected._
- **Should `Report Formatting` be split into smaller, more focused modules?**
  _Cohesion score 0.07816455696202532 - nodes in this community are weakly interconnected._
- **Should `Consensus Engine` be split into smaller, more focused modules?**
  _Cohesion score 0.09523809523809523 - nodes in this community are weakly interconnected._