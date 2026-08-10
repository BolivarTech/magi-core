# Graph Report - MAGI-Core  (2026-08-10)

## Corpus Check
- 140 files · ~142,348 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 2162 nodes · 4901 edges · 228 communities (163 shown, 65 thin omitted)
- Extraction: 99% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 39 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `85c98bec`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- orchestrator.rs
- reporting.rs
- consensus.rs
- validate.rs
- Finding
- .new
- compose_transport_message
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
- dispatch_one_agent
- LlmProvider
- magi_report_v0_3_1.json fixture
- make_output
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
- error.rs
- Lineage
- 1. Origin: The MAGI Supercomputers from Evangelion
- FixedRng
- .validate
- backoff.rs
- bool
- Voting rules + confidence formula
- Evangelion MAGI origin (Naoko Akagi)
- MAGI System Technical Documentation
- Structured disagreement rationale
- Why three perspectives (not 2 or 5)
- basic_analysis example
- 5. Data Schema and Consensus Protocol
- AgentName
- 2. Translation to the Software Engineering Domain
- ClaudeCliProvider::build_args
- ProviderResponse
- 7. Design Philosophy
- 3. The Three Agents in Detail
- 6. Modes of Operation
- Model Rotation
- MockProvider
- 4. Library Architecture
- int
- int
- str
- [2.1.0] - 2026-07-27
- [3.0.2] - 2026-07-30
- 5. Data Schema and Consensus Protocol
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
- Debug
- ProviderResponse
- Formatter
- ProviderError
- Result
- Self
- provider_url.rs
- [1.1.0] - 2026-05-25
- BTreeMap
- Default
- Mutex
- Send
- Sync
- Instant
- Into
- Option
- From
- Box
- BTreeMap
- Drop
- F
- HashMap
- P
- ProviderError
- ProviderError
- Debug
- Duration
- Formatter
- Instant
- Option
- String
- Vec
- check_calibration.sh
- redacted
- .leak
- redacted
- redacted
- .leak
- redacted
- redacted
- .leak
- redacted
- redacted
- .leak
- redacted
- ProviderError
- .fmt
- Result
- RequestBuilder
- Response
- Box
- [1.1.0] - 2026-05-25
- BTreeMap
- Drop
- F
- MagiConfig
- provider_url.rs
- AgentName
- pattern0nonterminal_bad.rs
- pattern0nonterminal_good.rs
- .new_checked
- FieldWriter
- .new
- .new
- String
- ProviderUrl
- compose_transport_message
- Self
- Duration
- [2.0.0] - 2026-07-25
- [1.1.0] - 2026-05-25
- mock_server.rs
- log_failure
- .parse
- Duration
- .send
- described
- [3.0.2] - 2026-07-30
- .new_checked
- ProviderRequest
- pattern8bnospace_bad.rs
- pattern8bnospace_good.rs
- Mutex
- Vec
- pattern7builderalias_bad.rs
- pattern7builderalias_good.rs
- pattern7default_bad.rs
- pattern7default_good.rs
- BTreeSet
- Sync

## God Nodes (most connected - your core abstractions)
1. `AgentName` - 70 edges
2. `LlmProvider` - 50 edges
3. `MagiBuilder` - 43 edges
4. `MagiError` - 37 edges
5. `make_consensus()` - 35 edges
6. `Lineage` - 35 edges
7. `Magi` - 34 edges
8. `make_agent()` - 34 edges
9. `AgentOutput` - 32 edges
10. `CompletionConfig` - 31 edges

## Surprising Connections (you probably didn't know these)
- `f()` --calls--> `compose_transport_message()`  [INFERRED]
  ci/fixtures/redaction/pattern1debug_good.rs → src/provider.rs
- `f()` --calls--> `compose_transport_message()`  [INFERRED]
  ci/fixtures/redaction/pattern1display_good.rs → src/provider.rs
- `f()` --calls--> `compose_transport_message()`  [INFERRED]
  ci/fixtures/redaction/pattern1pretty_good.rs → src/provider.rs
- `f()` --calls--> `compose_transport_message()`  [INFERRED]
  ci/fixtures/redaction/pattern1tostring_good.rs → src/provider.rs
- `main()` --calls--> `default_model_for_mode()`  [INFERRED]
  examples/basic_analysis.rs → src/provider.rs

## Import Cycles
- None detected.

## Communities (228 total, 65 thin omitted)

### Community 0 - "orchestrator.rs"
Cohesion: 0.12
Nodes (16): AgentSlotGuard, LineageRegistry, reg(), Arc, Drop, Mutex, test_5xx_does_not_count_toward_endpoint_down(), test_claim_next_reserves_and_replaces_active() (+8 more)

### Community 1 - "reporting.rs"
Cohesion: 0.05
Nodes (90): Condition, ConsensusResult, DedupFinding, Dissent, BTreeMap, Option, String, Vec (+82 more)

### Community 2 - "consensus.rs"
Cohesion: 0.10
Nodes (61): ConsensusConfig, ConsensusEngine, dedup_key(), DedupKey, finding_key(), make_output(), Default, Result (+53 more)

### Community 3 - "validate.rs"
Cohesion: 0.07
Nodes (3): test_validate_accepts_confidence_at_boundaries(), test_validate_accepts_valid_agent_output(), test_validator_new_creates_with_default_limits()

### Community 4 - "Finding"
Cohesion: 0.15
Nodes (29): a_measured_candidate_keeps_the_strict_guard_quiet(), a_strict_guard_with_nothing_measured_warns_and_still_completes(), exceeding_the_threshold_warns_and_still_completes(), probe_declaration_warnings_are_told_once_per_instance(), rotation_disabled_by_configuration_keeps_the_strict_guard_quiet(), F, staying_under_the_threshold_announces_nothing(), test_a_clean_run_adds_no_section_to_the_human_report() (+21 more)

### Community 5 - ".new"
Cohesion: 0.16
Nodes (9): Barrier, FailingProbe, MockProbe, MockProvider, OverlapProbe, AtomicUsize, ProviderError, Result (+1 more)

### Community 6 - "compose_transport_message"
Cohesion: 0.16
Nodes (26): finding_with_title(), output_with_confidence(), output_with_findings(), Vec, test_validate_accepts_finding_with_normal_title(), test_validate_mut_collapses_control_whitespace_in_titles(), test_validate_mut_preserves_order_of_findings(), test_validate_mut_replaces_title_with_cleaned_form() (+18 more)

### Community 7 - "schema.rs"
Cohesion: 0.05
Nodes (6): make_output(), test_agent_output_conditional_is_not_dissenting_from_approve_majority(), test_agent_output_effective_verdict_maps_conditional_to_approve(), test_agent_output_empty_findings_valid(), test_agent_output_is_dissenting_when_verdict_differs_from_majority(), test_agent_output_is_not_dissenting_when_verdict_matches_majority()

### Community 8 - "openai_compat.rs"
Cohesion: 0.13
Nodes (14): run_preflight(), strict_guard_is_inert(), test_active_unverifiable_digest_does_not_block_rotation(), test_calling_agent_excluded_from_digest_check(), test_candidate_without_digest_is_accepted_trusting_lineage(), test_lineage_ord_for_btree_keys(), test_preflight_builds_capabilities_from_probes(), test_probe_failure_yields_none_and_does_not_abort() (+6 more)

### Community 9 - "claude.rs"
Cohesion: 0.07
Nodes (44): describe(), main(), MyBackend, ProviderError, Result, String, CompletionConfig, Default (+36 more)

### Community 11 - "claude_cli.rs"
Cohesion: 0.13
Nodes (30): ClaudeCliProvider, CliOutput, parse_cli_output(), F, Into, ProviderError, Result, Self (+22 more)

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
Cohesion: 0.07
Nodes (45): Beh, build_oversized_case(), build_schema_local_case(), build_trio_with_caspar(), build_two_5xx_with_local_fallbacks(), build_two_external_failing_no_fallback(), build_two_failing_with_single_free_fallback(), build_two_network_failing_no_fallback() (+37 more)

### Community 18 - "mod.rs"
Cohesion: 0.07
Nodes (72): a_probe_declaring_another_model_is_named_not_rejected(), a_probe_declaring_its_own_model_is_quiet(), a_probe_that_claims_nothing_is_checked_against_nothing(), a_threshold_is_unreachable_when_the_smallest_warning_input_is_already_rejected(), a_threshold_that_can_never_fire_is_detected(), a_zero_threshold_warns_always_and_does_not_disable(), an_empty_input_never_exceeds_even_a_zero_threshold(), dispatch_one_agent() (+64 more)

### Community 19 - "RoutingMockProvider"
Cohesion: 0.29
Nodes (14): create_provider(), main(), parse_mode(), print_usage(), ProviderArgs, read_input(), Arc, Box (+6 more)

### Community 20 - "MAGI System Technical Documentation"
Cohesion: 0.40
Nodes (4): Exemption from CLAUDE.local.md §0.2 file-header rule, Local divergence from the pinned reference (F0, 2026-07-16), Regeneration, `src/prompts_md/` — Embedded prompt data

### Community 21 - "prelude.rs"
Cohesion: 0.19
Nodes (15): ae(), policy(), pool(), state(), test_claim_next_none_leaves_registry_intact(), test_concurrent_claims_never_double_reserve_stress(), test_lineage_from_owned_string_is_owned(), test_lineage_from_static_is_borrowed_zero_alloc() (+7 more)

### Community 22 - "finding_id.rs"
Cohesion: 0.18
Nodes (13): OllamaProvider, Client, Duration, Into, Option, ProviderError, ProviderUrl, Result (+5 more)

### Community 23 - ".cmp"
Cohesion: 0.10
Nodes (19): ComplexityGate, PathBuf, MagiBuilder, Box, P, Self, Send, test_analyze_nonce_collision_returns_invalid_input() (+11 more)

### Community 24 - "dispatch_one_agent"
Cohesion: 0.19
Nodes (25): Self, test_base_zero_with_three_retries_emits_exactly_four_requests(), test_budget_exhaustion_abandons_with_typed_reason(), test_completion_config_default_values(), test_completion_config_is_non_exhaustive(), test_honored_retry_after_can_overrun_a_small_budget(), test_max_retries_zero_does_not_retry(), test_operation_budget_zero_yields_single_attempt() (+17 more)

### Community 25 - "LlmProvider"
Cohesion: 0.09
Nodes (36): AbortHandle, AtomicBool, DispatchOutcome, JoinError, ExternalErrorKind, AbortGuard, an_oversized_response_routes_to_its_own_mage_local_outcome(), attempt_model() (+28 more)

### Community 27 - "make_output"
Cohesion: 0.40
Nodes (4): Adding a pair, One pair per ALTERNATIVE, not per rule, Redaction-gate fixtures, Verifying a change

### Community 28 - "[0.5.0] - 2026-05-16"
Cohesion: 0.25
Nodes (8): [0.5.0] - 2026-05-16, Added, Backward compatibility, Changed (breaking), Documentation, Performance, Pre-merge gates (CLAUDE.local.md §6), Test count

### Community 29 - "Release workflow (publish to crates.io)"
Cohesion: 0.07
Nodes (4): is_marker_line(), normalize_line(), Cow, test_locate_block_argument_order_is_not_symmetric()

### Community 30 - "[0.2.0] - 2026-04-18"
Cohesion: 0.29
Nodes (7): [0.2.0] - 2026-04-18, Added, Changed (breaking), Dependencies, Deprecated, Not included (deferred to v0.3.0), Security considerations

### Community 31 - "lib.rs"
Cohesion: 0.50
Nodes (4): [3.0.1] - 2026-07-30, Changed, Fixed, Notes

### Community 32 - "[0.4.0] - 2026-05-16"
Cohesion: 0.29
Nodes (7): [0.4.0] - 2026-05-16, Added, Backward compatibility, Changed, Documentation, Performance, Test count

### Community 33 - "[0.3.0] - 2026-04-18"
Cohesion: 0.09
Nodes (34): Agent, AgentFactory, MockProvider, Arc, AtomicUsize, BTreeMap, Default, Option (+26 more)

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
Cohesion: 0.50
Nodes (3): leaky(), Error, String

### Community 38 - "[1.1.1] - 2026-07-17"
Cohesion: 0.14
Nodes (24): Sized, build_user_prompt(), fixed_nonce(), Result, Self, Vec, test_build_user_prompt_accepts_empty_content(), test_build_user_prompt_all_5_unicode_separators_positive_neutralization() (+16 more)

### Community 39 - "[1.1.1] - 2026-07-17"
Cohesion: 0.18
Nodes (11): [0.1.2] - 2026-04-05, [0.3.1] - 2026-04-19, [2.2.0] - 2026-07-27, [3.2.0] - 2026-08-10, Added, Changed, Changelog, Fixed (+3 more)

### Community 40 - "error.rs"
Cohesion: 0.11
Nodes (18): build_retry_prompt(), test_build_retry_prompt_appends_feedback_block_exact_format(), test_build_retry_prompt_does_not_neutralize_midline_tokens(), test_build_retry_prompt_does_not_resanitize_content(), test_build_retry_prompt_feedback_block_after_end_delimiter(), test_build_retry_prompt_includes_seven_keys_list(), test_build_retry_prompt_neutralizes_dash_variant_retry_markers(), test_build_retry_prompt_neutralizes_injected_retry_feedback_marker() (+10 more)

### Community 41 - "magi-core"
Cohesion: 0.17
Nodes (12): Changelog, Consensus Labels, Contribution, Credentials in a provider URL, Example, Feature Flags, Features, Implementing a Custom Provider (+4 more)

### Community 42 - "normalize_newlines"
Cohesion: 0.20
Nodes (12): neutralize_headers(), normalize_newlines(), Cow, String, sanitize_error_for_retry_feedback(), strip_invisibles(), test_neutralize_headers_not_bypassed_by_mongolian_vowel_separator(), test_neutralize_headers_preserves_unmatched_lines_borrowed() (+4 more)

### Community 43 - "MAGI System — Complete Technical Documentation"
Cohesion: 0.22
Nodes (8): a_mounted_prefix_survives_both_spellings(), a_root_that_really_ends_in_v1_is_read_as_the_prefix(), every_accepted_spelling_yields_the_same_endpoints(), new_gives_the_inner_provider_the_real_credentials(), test_declared_model_answers_the_model_it_probes_for(), test_new_keeps_the_default_client_timeout(), test_push_within_cap_bounds_accumulator(), the_probe_authority_keeps_the_credentials_and_stays_at_the_root()

### Community 44 - "FallbackPool"
Cohesion: 0.24
Nodes (4): RetryConfig, RetryProvider, Arc, Duration

### Community 45 - "String"
Cohesion: 0.17
Nodes (12): Finding, Into, String, test_agent_name_btreeset_orders_alphabetically(), test_agent_name_usable_as_btreemap_key(), test_finding_new_constructs_three_field_finding(), test_finding_new_defaults_optional_fields(), test_finding_serde_roundtrip() (+4 more)

### Community 46 - "Quick Start"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 47 - "error.rs"
Cohesion: 0.08
Nodes (19): a_message_of_exactly_the_cap_is_kept_whole(), a_short_external_message_survives_untouched(), AbandonReason, an_oversized_external_message_is_cut_and_says_so(), external_message(), MagiError, ProviderError, Duration (+11 more)

### Community 48 - "Lineage"
Cohesion: 0.13
Nodes (12): ActiveEntry, digest_collision(), Lineage, RegistryInner, RotationEvent, RotationKind, Cow, Display (+4 more)

### Community 49 - "1. Origin: The MAGI Supercomputers from Evangelion"
Cohesion: 0.26
Nodes (7): Ord, Ordering, PartialOrd, Display, Option, Self, Severity

### Community 50 - "FixedRng"
Cohesion: 0.40
Nodes (4): FixedRng, test_fastrand_source_returns_distinct_values_across_calls(), test_fixed_rng_panics_when_exhausted(), VecDeque

### Community 52 - ".validate"
Cohesion: 0.33
Nodes (6): [0.3.0] - 2026-04-18, Added, Changed (breaking), Dependencies, Not included (deferred beyond v0.3.0), Security considerations

### Community 54 - "backoff.rs"
Cohesion: 0.16
Nodes (26): FnMut, fixed(), next_backoff(), parse_retry_after(), RetryAfter, RetryClass, Duration, Option (+18 more)

### Community 65 - "5. Data Schema and Consensus Protocol"
Cohesion: 0.11
Nodes (14): lookup_prompt(), BTreeMap, Option, Result, String, test_guard_and_parser_agree_on_the_same_block(), test_seat_aware_validation_names_the_agent_and_mode(), test_the_error_is_actionable_on_its_own() (+6 more)

### Community 66 - "AgentName"
Cohesion: 0.29
Nodes (8): contract_prompt(), test_analyze_applies_mode_agnostic_override_to_melchior(), test_analyze_per_mode_override_supersedes_all_modes(), test_build_aborts_on_a_corrupt_custom_prompt_before_any_provider_call(), test_legacy_with_custom_prompt_delegates_to_for_mode(), test_legacy_with_custom_prompt_shim_roundtrip(), test_with_custom_prompt_all_modes_stores_with_none_key(), test_with_custom_prompt_for_mode_stores_with_some_key()

### Community 67 - "2. Translation to the Software Engineering Domain"
Cohesion: 0.38
Nodes (5): clean_title(), Result, String, test_clean_title_is_idempotent(), Validator

### Community 69 - "ProviderResponse"
Cohesion: 0.16
Nodes (12): X, body_cap(), diagnostic_truncation_is_announced_and_utf8_safe(), diagnostic_under_the_cap_is_untouched(), ProviderResponse, push_within_cap(), Option, ProviderError (+4 more)

### Community 70 - "7. Design Philosophy"
Cohesion: 0.14
Nodes (17): canonicalizing_does_not_touch_the_query_or_the_fragment(), debug_and_display_are_both_redacted(), ends_with_segment_is_case_sensitive_and_ignores_a_trailing_slash(), join_path_is_idempotent_over_trailing_slash(), join_path_preserves_fragment(), join_path_preserves_query_and_appends_segments(), parse_error_never_echoes_the_raw_input(), parse_normalizes_dot_segments() (+9 more)

### Community 72 - "3. The Three Agents in Detail"
Cohesion: 0.50
Nodes (4): [3.0.0] - 2026-07-30, Added, BREAKING, Changed

### Community 73 - "6. Modes of Operation"
Cohesion: 0.39
Nodes (5): fail(), prod_only(), self_test(), check_redaction.sh script, skeleton()

### Community 74 - "Model Rotation"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 75 - "MockProvider"
Cohesion: 0.08
Nodes (21): f(), f(), f(), f(), cause_chain(), cause_chain_skips_the_top_level_error(), client_build_error(), compose_caps_the_head_even_with_no_cause_chain() (+13 more)

### Community 76 - "4. Library Architecture"
Cohesion: 0.33
Nodes (6): Default, Self, test_title_length_checked_after_strip_zero_width(), test_validate_mut_atomic_no_partial_mutation_on_error(), test_validator_with_limits_uses_custom_limits(), ValidationLimits

### Community 80 - "[2.1.0] - 2026-07-27"
Cohesion: 0.67
Nodes (3): [2.1.0] - 2026-07-27, Added, Compatibility

### Community 81 - "[3.0.2] - 2026-07-30"
Cohesion: 0.40
Nodes (5): [3.1.0] - 2026-07-31, Changed, Documented, Fixed, Security

### Community 82 - "5. Data Schema and Consensus Protocol"
Cohesion: 0.40
Nodes (3): redacted(), String, X

### Community 103 - "Debug"
Cohesion: 0.40
Nodes (3): redacted(), String, X

### Community 104 - "ProviderResponse"
Cohesion: 0.29
Nodes (6): test_analyze_all_agents_fail_returns_insufficient_agents(), test_analyze_one_agent_bad_json_degrades_gracefully(), test_analyze_one_agent_timeout_degrades_gracefully(), test_analyze_plain_text_response_treated_as_failure(), test_analyze_two_agents_fail_returns_insufficient_agents(), test_analyze_with_retry_disabled_skips_retry()

### Community 107 - "Result"
Cohesion: 0.39
Nodes (3): AgentOutput, Vec, Verdict

### Community 108 - "Self"
Cohesion: 0.29
Nodes (4): 8. Evangelion Correspondence Table, 9. Relationship to the MAGI Python Plugin, MAGI System — Complete Technical Documentation, Multi-Perspective Analysis Library for Rust

### Community 109 - "provider_url.rs"
Cohesion: 0.21
Nodes (4): a_partial_read_stays_within_the_cap_including_its_marker(), mark_truncated(), marking_a_short_text_does_not_pad_it(), marking_never_splits_a_multibyte_character()

### Community 110 - "[1.1.0] - 2026-05-25"
Cohesion: 0.50
Nodes (3): ProviderUrl, redacted(), String

### Community 112 - "Default"
Cohesion: 0.50
Nodes (3): ProviderUrl, redacted(), String

### Community 113 - "Mutex"
Cohesion: 0.33
Nodes (6): 1.1 Context in the Series, 1.2 The Three Units, 1.3 Decision Mechanism, 1.4 The Philosophical Principle, 1.5 Why Structured Disagreement Works, 1. Origin: The MAGI Supercomputers from Evangelion

### Community 114 - "Send"
Cohesion: 0.50
Nodes (4): build(), other(), ProviderError, Self

### Community 115 - "Sync"
Cohesion: 0.29
Nodes (7): Basic Usage, Cost Control with Complexity Gate, Custom System Prompts, Quick Start, The Output Contract, Using the Built-in Claude CLI Provider, With Builder

### Community 116 - "Instant"
Cohesion: 0.09
Nodes (32): absent_null_and_empty_content_are_all_a_named_schema_failure(), OpenAiChoice, OpenAiCompatibleProvider, OpenAiMessage, OpenAiRequest, OpenAiRespMessage, OpenAiResponse, Client (+24 more)

### Community 117 - "Into"
Cohesion: 0.38
Nodes (5): build(), ProviderError, Error, From, Self

### Community 118 - "Option"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 121 - "From"
Cohesion: 0.24
Nodes (6): HostedModel, Option, ProviderError, Result, String, SidecarProbe

### Community 122 - "Box"
Cohesion: 0.40
Nodes (5): 4.1 Module Structure, 4.2 Dependency Flow, 4.3 Execution Pipeline, 4.4 Concurrency Model, 4. Library Architecture

### Community 123 - "BTreeMap"
Cohesion: 0.50
Nodes (3): ProviderUrl, redacted(), String

### Community 124 - "Drop"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 125 - "F"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 126 - "HashMap"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 127 - "P"
Cohesion: 0.40
Nodes (5): 5.1 Agent Output Schema, 5.2 Voting Rules, 5.3 Confidence Formula, 5.4 Findings Deduplication, 5. Data Schema and Consensus Protocol

### Community 128 - "ProviderError"
Cohesion: 0.40
Nodes (5): 7.1 Dissent is a Feature, 7.2 Adversarial by Design, 7.3 Proportionality, 7.4 LLM-Agnostic Design, 7. Design Philosophy

### Community 129 - "ProviderError"
Cohesion: 0.40
Nodes (4): test_dangerous_config_is_announced_for_retry_after_cap_over_budget(), test_dangerous_config_is_announced_for_zero_base_delay(), test_dangerous_config_is_announced_for_zero_cap(), test_dangerous_config_is_announced_for_zero_retry_after_cap()

### Community 132 - "Formatter"
Cohesion: 0.50
Nodes (3): ProviderError, Result, X

### Community 133 - "Instant"
Cohesion: 0.50
Nodes (4): 2.1 Conceptual Mapping, 2.2 Why Three Perspectives and Not Two or Five, 2.3 Addressing Cognitive Biases, 2. Translation to the Software Engineering Domain

### Community 134 - "Option"
Cohesion: 0.20
Nodes (7): Attributes, Event, Id, Metadata, Record, EventLog, Subscriber

### Community 137 - "String"
Cohesion: 0.50
Nodes (3): ProviderUrl, redacted(), String

### Community 138 - "Vec"
Cohesion: 0.50
Nodes (4): 3.1 Melchior — The Scientist, 3.2 Balthasar — The Pragmatist, 3.3 Caspar — The Critic, 3. The Three Agents in Detail

### Community 164 - "ProviderError"
Cohesion: 0.50
Nodes (4): 6.1 Code Review, 6.2 Design, 6.3 Analysis, 6. Modes of Operation

### Community 166 - "Result"
Cohesion: 0.50
Nodes (4): Declaring fallbacks, Model Rotation, Ollama probe (feature `ollama`), What a rotation looks like

### Community 167 - "RequestBuilder"
Cohesion: 0.67
Nodes (3): [1.0.1] - 2026-05-25, Fixed, Internal

### Community 168 - "Response"
Cohesion: 0.67
Nodes (3): [1.1.1] - 2026-07-17, Changed, Fixed

### Community 171 - "BTreeMap"
Cohesion: 0.19
Nodes (4): JoinHandle, String, spawn_429_with_retry_after(), spawn_hanging_headers()

### Community 173 - "F"
Cohesion: 0.67
Nodes (3): Architecture, Module Dependency Graph, Prompt Injection Defense

### Community 177 - "MagiConfig"
Cohesion: 0.37
Nodes (12): a_query_secret_never_appears_in_debug_but_its_name_does(), assert_clean(), both_forms_in_one_url_are_both_redacted_in_debug(), credentials_never_appear_in_a_transport_error(), credentials_never_reach_the_serialized_report(), ollama_credentials_never_appear_in_a_completion_error(), ollama_credentials_never_appear_in_a_probe_error(), ollama_credentials_never_reach_the_serialized_report() (+4 more)

### Community 183 - "provider_url.rs"
Cohesion: 0.50
Nodes (3): leaky(), Error, String

### Community 188 - ".new_checked"
Cohesion: 0.40
Nodes (3): leaky(), Error, String

### Community 191 - "FieldWriter"
Cohesion: 0.40
Nodes (4): Field, FieldWriter, Debug, Visit

### Community 212 - "String"
Cohesion: 0.20
Nodes (7): X, X, Method, ProviderRequest, Client, RequestBuilder, send_composes_a_redacted_error_on_connection_failure()

### Community 214 - "ProviderUrl"
Cohesion: 0.29
Nodes (5): redacted_hides_all_query_values_and_keeps_names(), redacted_hides_userinfo_and_keeps_host(), redacted_placeholder_is_url_safe_and_not_percent_encoded(), Formatter, Result

### Community 224 - "compose_transport_message"
Cohesion: 0.50
Nodes (3): Error, String, spawn_failure()

### Community 227 - "Self"
Cohesion: 0.33
Nodes (3): parent_climbs_one_level_and_stops_at_the_root(), Self, T

### Community 228 - "Duration"
Cohesion: 0.20
Nodes (10): AtomicU32, classify(), is_retryable(), MockProvider, RetryAfterProvider, Mutex, ProviderError, Result (+2 more)

### Community 230 - "[2.0.0] - 2026-07-25"
Cohesion: 0.50
Nodes (4): [2.0.0] - 2026-07-25, Added, BREAKING, Changed

### Community 239 - "[1.1.0] - 2026-05-25"
Cohesion: 0.67
Nodes (3): [1.1.0] - 2026-05-25, Added, Notes

### Community 251 - "mock_server.rs"
Cohesion: 0.29
Nodes (11): FallbackPoolBuilder, P, Self, test_duplicate_lineage_warns_but_builds(), test_empty_pool_is_valid_and_keeps_max_rotations(), test_push_probing_stores_both_views(), test_push_with_probe_accepts_a_provider_that_cannot_probe(), test_push_with_probe_matches_push_probing_for_the_same_object() (+3 more)

### Community 260 - ".parse"
Cohesion: 0.16
Nodes (15): Fn, extract(), locate(), locate_block(), Display, Error, Formatter, Result (+7 more)

### Community 267 - "[3.0.2] - 2026-07-30"
Cohesion: 0.67
Nodes (3): [3.0.2] - 2026-07-30, Changed, Fixed

### Community 268 - ".new_checked"
Cohesion: 0.20
Nodes (3): FailingProvider, AtomicUsize, SlowFailingProvider

### Community 270 - "ProviderRequest"
Cohesion: 0.57
Nodes (7): balthasar_prompt(), caspar_prompt(), embedded_prompt_for(), melchior_prompt(), test_echo_canary_values_are_present_in_every_shipped_prompt(), test_prompts_match_pinned_reference_sha256(), test_shipped_prompts_satisfy_the_verdict_marker_contract()

### Community 271 - "pattern8bnospace_bad.rs"
Cohesion: 0.50
Nodes (4): build(), ProviderError, Self, sneak()

### Community 272 - "pattern8bnospace_good.rs"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 281 - "Mutex"
Cohesion: 0.11
Nodes (30): TcpStream, a_chunked_probe_body_degrades_from_the_streaming_branch(), a_chunked_probe_body_under_the_cap_is_read_and_parsed(), a_probe_body_over_the_cap_degrades_instead_of_failing(), a_success_body_at_the_cap_is_read_whole(), a_success_body_over_the_cap_fails_rather_than_arriving_truncated(), a_verdict_body_that_is_not_utf8_fails_instead_of_being_mangled(), an_error_body_over_the_cap_keeps_its_prefix_and_announces_the_cut() (+22 more)

### Community 289 - "Vec"
Cohesion: 0.28
Nodes (15): AgentRotationState, Candidate, cap(), caps_map(), digest_case(), digest_case_self(), digest_case_two_active(), ModelCapability (+7 more)

### Community 293 - "pattern7builderalias_bad.rs"
Cohesion: 0.67
Nodes (3): bare(), configured(), Client

### Community 294 - "pattern7builderalias_good.rs"
Cohesion: 0.67
Nodes (3): also_configured(), configured(), Client

### Community 301 - "BTreeSet"
Cohesion: 0.40
Nodes (5): empty(), empty_s(), empty_wr(), BTreeSet, test_next_model_is_deterministic()

### Community 316 - "Sync"
Cohesion: 0.24
Nodes (6): FallbackCandidate, FallbackPool, ProviderProbe, RotationConfig, Send, Sync

## Knowledge Gaps
- **201 isolated node(s):** `check_r0.sh script`, `ProviderError`, `ProviderError`, `ProviderError`, `X` (+196 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **65 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `LlmProvider` connect `[0.3.0] - 2026-04-18` to `Finding`, `.new`, `openai_compat.rs`, `claude.rs`, `claude_cli.rs`, `.new_checked`, `.new`, `RoutingMockProvider`, `finding_id.rs`, `.cmp`, `dispatch_one_agent`, `LlmProvider`, `FallbackPool`, `Sync`, `MockProvider`, `Duration`, `Instant`, `From`, `mock_server.rs`?**
  _High betweenness centrality (0.078) - this node is a cross-community bridge._
- **Why does `AgentName` connect `reporting.rs` to `orchestrator.rs`, `consensus.rs`, `schema.rs`, `openai_compat.rs`, `ProviderRequest`, `.new`, `mod.rs`, `.cmp`, `LlmProvider`, `[0.3.0] - 2026-04-18`, `Vec`, `error.rs`, `Lineage`, `1. Origin: The MAGI Supercomputers from Evangelion`, `Sync`, `5. Data Schema and Consensus Protocol`, `AgentName`, `Result`, `mock_server.rs`?**
  _High betweenness centrality (0.038) - this node is a cross-community bridge._
- **Why does `MagiError` connect `error.rs` to `[0.3.0] - 2026-04-18`, `consensus.rs`, `reporting.rs`, `Finding`, `5. Data Schema and Consensus Protocol`, `[1.1.1] - 2026-07-17`, `validate.rs`, `compose_transport_message`, `2. Translation to the Software Engineering Domain`, `user_prompt.rs`, `Lineage`, `mod.rs`, `LlmProvider`?**
  _High betweenness centrality (0.034) - this node is a cross-community bridge._
- **What connects `check_r0.sh script`, `ProviderError`, `ProviderError` to the rest of the system?**
  _206 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `orchestrator.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.12 - nodes in this community are weakly interconnected._
- **Should `reporting.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.05265748031496063 - nodes in this community are weakly interconnected._
- **Should `consensus.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.0989648033126294 - nodes in this community are weakly interconnected._