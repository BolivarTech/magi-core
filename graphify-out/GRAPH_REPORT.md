# Graph Report - MAGI-Core  (2026-07-30)

## Corpus Check
- 42 files · ~102,693 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 1565 nodes · 3971 edges · 108 communities (73 shown, 35 thin omitted)
- Extraction: 99% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 25 edges (avg confidence: 0.79)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `d3a28fce`
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
- schema.rs
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
- prelude.rs
- finding_id.rs
- .cmp
- LlmProvider
- magi_report_v0_3_1.json fixture
- [0.5.0] - 2026-05-16
- Release workflow (publish to crates.io)
- [0.2.0] - 2026-04-18
- lib.rs
- [0.4.0] - 2026-05-16
- [0.3.0] - 2026-04-18
- [0.6.0] - 2026-05-21
- [1.0.0] - 2026-05-24
- [0.3.1] - 2026-04-19
- RoutingMockProvider
- [1.1.1] - 2026-07-17
- [1.1.1] - 2026-07-17
- error.rs
- magi-core
- normalize_newlines
- MAGI System — Complete Technical Documentation
- FallbackPool
- String
- Quick Start
- Lineage
- .new
- FixedRng
- .validate
- LineageRegistry
- backoff.rs
- bool
- bytes
- Voting rules + confidence formula
- Evangelion MAGI origin (Naoko Akagi)
- MAGI System Technical Documentation
- Structured disagreement rationale
- Why three perspectives (not 2 or 5)
- basic_analysis example
- .with_limits
- 5. Data Schema and Consensus Protocol
- AgentName
- 2. Translation to the Software Engineering Domain
- ClaudeCliProvider::build_args
- Self
- mock_server.rs
- MockProvider
- int
- int
- str
- Result
- banner
- agent_count
- conditions
- confidence
- consensus
- consensus_verdict
- dissent
- findings
- majority_summary
- recommendations
- score
- votes
- degraded
- failed_agents
- caspar
- balthasar
- melchior
- report
- Self
- ClaudeProvider
- String
- Vec
- String
- Vec
- BTreeMap
- [1.1.1] - 2026-07-17
- [2.1.0] - 2026-07-27
- [2.2.0] - 2026-07-27
- ProviderError
- Vec

## God Nodes (most connected - your core abstractions)
1. `AgentName` - 68 edges
2. `ProviderError` - 66 edges
3. `LlmProvider` - 45 edges
4. `MagiBuilder` - 41 edges
5. `MagiError` - 37 edges
6. `make_consensus()` - 33 edges
7. `Lineage` - 33 edges
8. `make_agent()` - 32 edges
9. `Magi` - 31 edges
10. `AgentOutput` - 31 edges

## Surprising Connections (you probably didn't know these)
- `main()` --calls--> `default_model_for_mode()`  [INFERRED]
  examples/basic_analysis.rs → src/provider.rs
- `test_panic_never_rotates()` --calls--> `build_trio_with_caspar()`  [INFERRED]
  tests/rotation_integration.rs → src/test_support.rs
- `test_report_shows_and_omits_model_rotations_section()` --calls--> `build_trio_with_caspar()`  [INFERRED]
  tests/rotation_integration.rs → src/test_support.rs
- `test_successful_retry_avoids_rotation()` --calls--> `build_trio_with_caspar()`  [INFERRED]
  tests/rotation_integration.rs → src/test_support.rs
- `test_schema_fail_is_mage_local_not_run_wide()` --calls--> `build_schema_local_case()`  [INFERRED]
  tests/rotation_integration.rs → src/test_support.rs

## Import Cycles
- None detected.

## Communities (108 total, 35 thin omitted)

### Community 0 - "orchestrator.rs"
Cohesion: 0.12
Nodes (47): dispatch_one_agent(), echoed_example_response(), magi_error_for(), mock_agent_json(), mock_agent_object(), parse_agent_response(), parse_and_validate(), parse_validate_and_check() (+39 more)

### Community 1 - "reporting.rs"
Cohesion: 0.06
Nodes (79): Condition, ConsensusResult, DedupFinding, Dissent, BTreeMap, Option, String, Vec (+71 more)

### Community 2 - "consensus.rs"
Cohesion: 0.10
Nodes (61): ConsensusConfig, ConsensusEngine, dedup_key(), DedupKey, finding_key(), make_output(), Default, Result (+53 more)

### Community 3 - "validate.rs"
Cohesion: 0.05
Nodes (46): MagiError, Error, From, Option, Self, Vec, clean_title(), finding_with_title() (+38 more)

### Community 4 - "Finding"
Cohesion: 0.10
Nodes (34): report_with_one_failing_agent(), F, test_a_2_2_0_report_without_the_field_still_deserializes(), test_a_clean_run_adds_no_section_to_the_human_report(), test_a_recovered_retry_still_records_its_cause(), test_analyze_all_agents_fail_returns_insufficient_agents(), test_analyze_input_too_large_rejects_without_launching_agents(), test_analyze_no_retry_on_timeout_keeps_retried_empty() (+26 more)

### Community 5 - ".new"
Cohesion: 0.16
Nodes (15): Candidate, digest_collision(), empty_s(), empty_wr(), FailingProbe, MockProbe, ModelCapability, RotationPolicy (+7 more)

### Community 6 - "provider.rs"
Cohesion: 0.31
Nodes (19): Self, test_base_zero_with_three_retries_emits_exactly_four_requests(), test_budget_exhaustion_abandons_with_typed_reason(), test_honored_retry_after_can_overrun_a_small_budget(), test_max_retries_zero_does_not_retry(), test_operation_budget_zero_yields_single_attempt(), test_retry_after_beyond_cap_abandons_with_typed_reason(), test_retry_provider_does_not_retry_on_auth() (+11 more)

### Community 7 - "schema.rs"
Cohesion: 0.05
Nodes (6): make_output(), test_agent_output_conditional_is_not_dissenting_from_approve_majority(), test_agent_output_effective_verdict_maps_conditional_to_approve(), test_agent_output_empty_findings_valid(), test_agent_output_is_dissenting_when_verdict_differs_from_majority(), test_agent_output_is_not_dissenting_when_verdict_matches_majority()

### Community 8 - "openai_compat.rs"
Cohesion: 0.09
Nodes (31): OpenAiChoice, OpenAiCompatibleProvider, OpenAiMessage, OpenAiRequest, OpenAiRespMessage, OpenAiResponse, Client, Debug (+23 more)

### Community 9 - "claude.rs"
Cohesion: 0.09
Nodes (34): ClaudeMessage, ClaudeProvider, ClaudeRequest, ClaudeResponse, ContentBlock, Client, Debug, Duration (+26 more)

### Community 11 - "claude_cli.rs"
Cohesion: 0.13
Nodes (29): ClaudeCliProvider, CliOutput, parse_cli_output(), F, Into, Result, Self, String (+21 more)

### Community 13 - "Balthasar — The Pragmatist"
Cohesion: 0.17
Nodes (11): Balthasar — The Pragmatist, Constraints, Finding calibration (code-review mode only), In analysis mode, In code review mode, In design mode, Input format, Output format (+3 more)

### Community 14 - "MAGI System Technical Documentation"
Cohesion: 0.22
Nodes (10): main(), main(), Path, MAGI R1 W4: pre-write check that the pinned SHA exists in the repo     before r, verify_sha_exists(), apply_divergences(), Path, Apply every declared divergence to a reference blob, failing loudly.      Retu (+2 more)

### Community 15 - "Caspar — The Critic"
Cohesion: 0.17
Nodes (11): Caspar — The Critic, Constraints, Finding calibration (code-review mode only), In analysis mode, In code review mode, In design mode, Input format, Output format (+3 more)

### Community 16 - "Melchior — The Scientist"
Cohesion: 0.17
Nodes (11): Constraints, Finding calibration (code-review mode only), In analysis mode, In code review mode, In design mode, Input format, Melchior — The Scientist, Output format (+3 more)

### Community 17 - ".new"
Cohesion: 0.17
Nodes (12): Finding, Into, String, test_agent_name_btreeset_orders_alphabetically(), test_agent_name_usable_as_btreemap_key(), test_finding_new_constructs_three_field_finding(), test_finding_new_defaults_optional_fields(), test_finding_serde_roundtrip() (+4 more)

### Community 18 - "mod.rs"
Cohesion: 0.11
Nodes (18): build_retry_prompt(), test_build_retry_prompt_appends_feedback_block_exact_format(), test_build_retry_prompt_does_not_neutralize_midline_tokens(), test_build_retry_prompt_does_not_resanitize_content(), test_build_retry_prompt_feedback_block_after_end_delimiter(), test_build_retry_prompt_includes_seven_keys_list(), test_build_retry_prompt_neutralizes_dash_variant_retry_markers(), test_build_retry_prompt_neutralizes_injected_retry_feedback_marker() (+10 more)

### Community 19 - "RoutingMockProvider"
Cohesion: 0.29
Nodes (14): create_provider(), main(), parse_mode(), print_usage(), ProviderArgs, read_input(), Arc, Box (+6 more)

### Community 20 - "MAGI System Technical Documentation"
Cohesion: 0.40
Nodes (4): Exemption from CLAUDE.local.md §0.2 file-header rule, Local divergence from the pinned reference (F0, 2026-07-16), Regeneration, `src/prompts_md/` — Embedded prompt data

### Community 21 - "prelude.rs"
Cohesion: 0.12
Nodes (16): AgentSlotGuard, LineageRegistry, reg(), Arc, Drop, Mutex, test_5xx_does_not_count_toward_endpoint_down(), test_endpoint_down_latch_exactly_one_true_concurrent() (+8 more)

### Community 22 - "finding_id.rs"
Cohesion: 0.14
Nodes (9): Barrier, FallbackCandidate, FallbackPool, OverlapProbe, ProviderProbe, RotationConfig, AtomicUsize, Send (+1 more)

### Community 23 - ".cmp"
Cohesion: 0.27
Nodes (9): contract_prompt(), test_analyze_applies_mode_agnostic_override_to_melchior(), test_analyze_per_mode_override_supersedes_all_modes(), test_build_aborts_on_a_corrupt_custom_prompt_before_any_provider_call(), test_legacy_with_custom_prompt_delegates_to_for_mode(), test_legacy_with_custom_prompt_shim_roundtrip(), test_with_custom_prompt_all_modes_stores_with_none_key(), test_with_custom_prompt_for_mode_stores_with_some_key() (+1 more)

### Community 25 - "LlmProvider"
Cohesion: 0.12
Nodes (26): AbortHandle, DispatchOutcome, JoinError, AbortGuard, attempt_model(), CapturingMockProvider, collect_probe_targets(), default_rotations() (+18 more)

### Community 28 - "[0.5.0] - 2026-05-16"
Cohesion: 0.25
Nodes (8): [0.5.0] - 2026-05-16, Added, Backward compatibility, Changed (breaking), Documentation, Performance, Pre-merge gates (CLAUDE.local.md §6), Test count

### Community 29 - "Release workflow (publish to crates.io)"
Cohesion: 0.05
Nodes (20): Fn, test_the_two_scripted_bodies_fail_and_succeed_where_their_names_claim(), extract(), is_marker_line(), locate(), locate_block(), normalize_line(), Cow (+12 more)

### Community 30 - "[0.2.0] - 2026-04-18"
Cohesion: 0.29
Nodes (7): [0.2.0] - 2026-04-18, Added, Changed (breaking), Dependencies, Deprecated, Not included (deferred to v0.3.0), Security considerations

### Community 31 - "lib.rs"
Cohesion: 0.10
Nodes (20): ComplexityGate, PathBuf, MagiBuilder, MagiConfig, Box, Default, Duration, P (+12 more)

### Community 32 - "[0.4.0] - 2026-05-16"
Cohesion: 0.29
Nodes (7): [0.4.0] - 2026-05-16, Added, Backward compatibility, Changed, Documentation, Performance, Test count

### Community 33 - "[0.3.0] - 2026-04-18"
Cohesion: 0.09
Nodes (32): Agent, AgentFactory, MockProvider, Arc, AtomicUsize, BTreeMap, Default, Option (+24 more)

### Community 34 - "[0.6.0] - 2026-05-21"
Cohesion: 0.33
Nodes (6): [0.6.0] - 2026-05-21, Backward compatibility, Changed, Pre-merge gates (CLAUDE.local.md §6), Security, Test count

### Community 35 - "[1.0.0] - 2026-05-24"
Cohesion: 0.40
Nodes (5): [1.0.0] - 2026-05-24, Added, Changed (breaking), Notes, Security

### Community 36 - "[0.3.1] - 2026-04-19"
Cohesion: 0.23
Nodes (13): D, de_category(), de_opt_file(), de_opt_line(), generate_finding_id(), normalize_category(), normalize_path(), Error (+5 more)

### Community 37 - "RoutingMockProvider"
Cohesion: 0.08
Nodes (38): Beh, build_schema_local_case(), build_trio_with_caspar(), build_two_5xx_with_local_fallbacks(), build_two_failing_with_single_free_fallback(), build_two_network_failing_no_fallback(), MockProbe, ok() (+30 more)

### Community 38 - "[1.1.1] - 2026-07-17"
Cohesion: 0.14
Nodes (24): Sized, build_user_prompt(), fixed_nonce(), Result, Self, Vec, test_build_user_prompt_accepts_empty_content(), test_build_user_prompt_all_5_unicode_separators_positive_neutralization() (+16 more)

### Community 39 - "[1.1.1] - 2026-07-17"
Cohesion: 0.22
Nodes (9): [0.1.2] - 2026-04-05, [0.3.1] - 2026-04-19, [3.0.0] - 2026-07-30, Added, BREAKING, Changed, Changelog, Fixed (+1 more)

### Community 40 - "error.rs"
Cohesion: 0.13
Nodes (28): ae(), AgentRotation, AgentRotationState, cap(), caps_map(), digest_case(), digest_case_self(), digest_case_two_active() (+20 more)

### Community 41 - "magi-core"
Cohesion: 0.15
Nodes (13): Architecture, Changelog, Consensus Labels, Contribution, Example, Feature Flags, Features, Implementing a Custom Provider (+5 more)

### Community 42 - "normalize_newlines"
Cohesion: 0.20
Nodes (12): neutralize_headers(), normalize_newlines(), Cow, String, sanitize_error_for_retry_feedback(), strip_invisibles(), test_neutralize_headers_not_bypassed_by_mongolian_vowel_separator(), test_neutralize_headers_preserves_unmatched_lines_borrowed() (+4 more)

### Community 43 - "MAGI System — Complete Technical Documentation"
Cohesion: 0.25
Nodes (8): 2.1 Conceptual Mapping, 2.2 Why Three Perspectives and Not Two or Five, 2.3 Addressing Cognitive Biases, 2. Translation to the Software Engineering Domain, 8. Evangelion Correspondence Table, 9. Relationship to the MAGI Python Plugin, MAGI System — Complete Technical Documentation, Multi-Perspective Analysis Library for Rust

### Community 44 - "FallbackPool"
Cohesion: 0.23
Nodes (12): FallbackPoolBuilder, P, run_preflight(), test_duplicate_lineage_warns_but_builds(), test_empty_pool_is_valid_and_keeps_max_rotations(), test_lineage_ord_for_btree_keys(), test_preflight_builds_capabilities_from_probes(), test_probe_failure_yields_none_and_does_not_abort() (+4 more)

### Community 45 - "String"
Cohesion: 0.15
Nodes (12): Response, OllamaProvider, push_within_cap(), read_capped(), Client, Into, Option, Result (+4 more)

### Community 46 - "Quick Start"
Cohesion: 0.29
Nodes (7): Basic Usage, Cost Control with Complexity Gate, Custom System Prompts, Quick Start, The Output Contract, Using the Built-in Claude CLI Provider, With Builder

### Community 48 - "Lineage"
Cohesion: 0.11
Nodes (14): ActiveEntry, empty(), Lineage, MockProvider, RegistryInner, RotationEvent, RotationKind, BTreeSet (+6 more)

### Community 49 - ".new"
Cohesion: 0.50
Nodes (4): [2.0.0] - 2026-07-25, Added, BREAKING, Changed

### Community 50 - "FixedRng"
Cohesion: 0.40
Nodes (4): FixedRng, test_fastrand_source_returns_distinct_values_across_calls(), test_fixed_rng_panics_when_exhausted(), VecDeque

### Community 52 - ".validate"
Cohesion: 0.33
Nodes (6): [0.3.0] - 2026-04-18, Added, Changed (breaking), Dependencies, Not included (deferred beyond v0.3.0), Security considerations (MAGI R3 W8)

### Community 53 - "LineageRegistry"
Cohesion: 0.13
Nodes (7): RetryClass, RetryConfig, RetryProvider, Arc, Default, Duration, SlowFailingProvider

### Community 54 - "backoff.rs"
Cohesion: 0.16
Nodes (25): FnMut, fixed(), next_backoff(), parse_retry_after(), RetryAfter, Duration, Option, String (+17 more)

### Community 56 - "bytes"
Cohesion: 0.50
Nodes (4): Declaring fallbacks, Model Rotation (MS2), Ollama probe (feature `ollama`), What a rotation looks like

### Community 64 - ".with_limits"
Cohesion: 0.31
Nodes (4): JoinHandle, String, spawn_429_with_retry_after(), spawn_hanging_headers()

### Community 65 - "5. Data Schema and Consensus Protocol"
Cohesion: 0.10
Nodes (21): balthasar_prompt(), caspar_prompt(), embedded_prompt_for(), lookup_prompt(), melchior_prompt(), BTreeMap, Option, Result (+13 more)

### Community 66 - "AgentName"
Cohesion: 0.39
Nodes (3): AgentOutput, Vec, Verdict

### Community 67 - "2. Translation to the Software Engineering Domain"
Cohesion: 0.33
Nodes (6): 1.1 Context in the Series, 1.2 The Three Units, 1.3 Decision Mechanism, 1.4 The Philosophical Principle, 1.5 Why Structured Disagreement Works, 1. Origin: The MAGI Supercomputers from Evangelion

### Community 74 - "mock_server.rs"
Cohesion: 0.67
Nodes (3): [1.0.1] - 2026-05-25, Fixed, Internal

### Community 75 - "MockProvider"
Cohesion: 0.10
Nodes (16): default_model_for_mode(), resolve_claude_alias(), test_completion_config_default_values(), test_completion_config_is_non_exhaustive(), test_dangerous_config_is_announced_for_retry_after_cap_over_budget(), test_dangerous_config_is_announced_for_zero_base_delay(), test_dangerous_config_is_announced_for_zero_cap(), test_dangerous_config_is_announced_for_zero_retry_after_cap() (+8 more)

### Community 82 - "Result"
Cohesion: 0.40
Nodes (5): 4.1 Module Structure, 4.2 Dependency Flow, 4.3 Execution Pipeline, 4.4 Concurrency Model, 4. Library Architecture

### Community 103 - "Self"
Cohesion: 0.40
Nodes (5): 5.1 Agent Output Schema, 5.2 Voting Rules, 5.3 Confidence Formula, 5.4 Findings Deduplication, 5. Data Schema and Consensus Protocol

### Community 105 - "String"
Cohesion: 0.40
Nodes (5): 7.1 Dissent is a Feature, 7.2 Adversarial by Design, 7.3 Proportionality, 7.4 LLM-Agnostic Design, 7. Design Philosophy

### Community 106 - "Vec"
Cohesion: 0.50
Nodes (4): 3.1 Melchior — The Scientist, 3.2 Balthasar — The Pragmatist, 3.3 Caspar — The Critic, 3. The Three Agents in Detail

### Community 109 - "String"
Cohesion: 0.50
Nodes (4): 6.1 Code Review, 6.2 Design, 6.3 Analysis, 6. Modes of Operation

### Community 110 - "Vec"
Cohesion: 0.67
Nodes (3): [1.1.0] - 2026-05-25, Added, Notes

### Community 112 - "[1.1.1] - 2026-07-17"
Cohesion: 0.67
Nodes (3): [1.1.1] - 2026-07-17, Changed, Fixed

### Community 113 - "[2.1.0] - 2026-07-27"
Cohesion: 0.67
Nodes (3): [2.1.0] - 2026-07-27, Added, Compatibility

### Community 114 - "[2.2.0] - 2026-07-27"
Cohesion: 0.67
Nodes (3): [2.2.0] - 2026-07-27, Changed, Fixed

### Community 133 - "ProviderError"
Cohesion: 0.10
Nodes (17): AtomicU32, AbandonReason, ProviderError, Duration, Instant, String, classify(), CompletionConfig (+9 more)

### Community 139 - "Vec"
Cohesion: 0.26
Nodes (7): Ord, Ordering, PartialOrd, Display, Option, Self, Severity

## Knowledge Gaps
- **161 isolated node(s):** `check_r0.sh script`, `BREAKING`, `Added`, `Changed`, `Fixed` (+156 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **35 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `ProviderError` connect `ProviderError` to `[0.3.0] - 2026-04-18`, `validate.rs`, `Finding`, `.new`, `provider.rs`, `RoutingMockProvider`, `openai_compat.rs`, `claude.rs`, `error.rs`, `MockProvider`, `claude_cli.rs`, `String`, `error.rs`, `finding_id.rs`, `LlmProvider`?**
  _High betweenness centrality (0.095) - this node is a cross-community bridge._
- **Why does `MagiError` connect `validate.rs` to `orchestrator.rs`, `[0.3.0] - 2026-04-18`, `consensus.rs`, `reporting.rs`, `Finding`, `ProviderError`, `5. Data Schema and Consensus Protocol`, `[1.1.1] - 2026-07-17`, `user_prompt.rs`, `error.rs`, `Lineage`, `.cmp`, `LlmProvider`?**
  _High betweenness centrality (0.095) - this node is a cross-community bridge._
- **Why does `LlmProvider` connect `[0.3.0] - 2026-04-18` to `Finding`, `ProviderError`, `provider.rs`, `.new`, `openai_compat.rs`, `claude.rs`, `error.rs`, `MockProvider`, `claude_cli.rs`, `String`, `FallbackPool`, `Lineage`, `RoutingMockProvider`, `LineageRegistry`, `finding_id.rs`, `LlmProvider`, `RoutingMockProvider`, `lib.rs`?**
  _High betweenness centrality (0.051) - this node is a cross-community bridge._
- **What connects `check_r0.sh script`, `Apply every declared divergence to a reference blob, failing loudly.      Retu`, `Read a file's bytes at a specific ref via `git show`, no checkout.` to the rest of the system?**
  _166 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `orchestrator.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.11843137254901961 - nodes in this community are weakly interconnected._
- **Should `reporting.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.06016731016731017 - nodes in this community are weakly interconnected._
- **Should `consensus.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.0989648033126294 - nodes in this community are weakly interconnected._