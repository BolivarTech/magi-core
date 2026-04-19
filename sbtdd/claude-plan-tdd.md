# TDD Plan: magi-core (MAGI-Reviewed)

> Companion to `claude-plan.md`. Defines test stubs for each section.
> Tests must be written BEFORE implementation (Red-Green-Refactor).
>
> **Reviewed**: Critical review + MAGI review (GO WITH CAVEATS). All conditions resolved.
> Original preserved in `claude-plan-tdd-org.md`.

**Testing setup**: Rust built-in `#[test]`, `cargo nextest`, TDD-Guard enforcement.
**Mocking**: `mockall` for `LlmProvider` trait.
**Naming**: Behavior-descriptive, e.g. `test_consensus_caps_strong_label_in_degraded_mode`.
**Convention**: `[BDD-NN]` tags link stubs to spec scenarios for traceability.
**Convention**: `[ADDENDUM]` tags mark tests not backed by a BDD scenario but added for production safety.

---

## Section 1: Foundation -- Error Types and Domain Schema

### error.rs -- ProviderError

```rust
// Test: http_error_display_contains_status_code_and_body
// Test: network_error_display_contains_message
// Test: timeout_error_display_describes_timeout
// Test: auth_error_display_contains_message
// Test: process_error_display_includes_exit_code_and_stderr
// Test: process_error_display_handles_none_exit_code
// Test: nested_session_error_display_describes_restriction
```

### error.rs -- MagiError

```rust
// Test: validation_error_display_contains_descriptive_message
// Test: insufficient_agents_error_display_formats_succeeded_and_required
// Test: input_too_large_error_display_formats_size_and_max
// Test: deserialization_error_display_contains_parse_failure_detail
// Test: io_error_display_preserves_underlying_io_message
// Test: provider_error_converts_to_magi_error_provider_variant
// Test: serde_json_error_converts_to_magi_error_deserialization_variant
// Test: io_error_converts_to_magi_error_io_variant
```

### schema.rs -- Verdict

```rust
// Test: approve_weight_returns_positive_one
// Test: reject_weight_returns_negative_one
// Test: conditional_weight_returns_zero_point_five
// Test: conditional_effective_maps_to_approve
// Test: approve_effective_returns_approve_identity
// Test: reject_effective_returns_reject_identity
// Test: verdict_display_outputs_uppercase_approve_reject_conditional
// Test: verdict_serializes_as_lowercase_strings
// Test: verdict_deserializes_from_lowercase_strings
// [EDGE] verdict_deserialization_rejects_invalid_string
```

### schema.rs -- Severity

```rust
// Test: severity_ordering_critical_greater_than_warning_greater_than_info
// Test: severity_icon_returns_expected_brackets_for_each_level
// Test: severity_display_outputs_uppercase_strings
// Test: severity_serializes_as_lowercase
// [EDGE] severity_deserialization_rejects_invalid_string
```

### schema.rs -- Mode

```rust
// Test: mode_display_outputs_code_review_design_analysis
// Test: mode_serializes_as_kebab_case_lowercase
// Test: mode_deserializes_from_kebab_case_strings
// [EDGE] mode_deserialization_rejects_invalid_string
```

### schema.rs -- AgentName

```rust
// Test: agent_name_title_returns_scientist_pragmatist_critic
// Test: agent_name_display_name_returns_melchior_balthasar_caspar
// Test: agent_name_ord_follows_alphabetical_balthasar_caspar_melchior
// Test: agent_name_serializes_as_lowercase
// Test: agent_name_usable_as_btreemap_key
// [EDGE] agent_name_deserialization_rejects_invalid_string
```

### schema.rs -- Finding

```rust
// Test: stripped_title_removes_zero_width_unicode_characters
// Test: stripped_title_preserves_normal_text_unchanged
// Test: finding_serializes_and_deserializes_roundtrip
// [EDGE] stripped_title_handles_mixed_zero_width_and_normal_chars
// [EDGE] stripped_title_returns_empty_string_when_title_is_only_zero_width
```

### schema.rs -- AgentOutput

```rust
// Test: is_approving_returns_true_for_approve_verdict
// Test: is_approving_returns_true_for_conditional_verdict
// Test: is_approving_returns_false_for_reject_verdict
// Test: is_dissenting_returns_true_when_effective_verdict_differs_from_majority
// Test: is_dissenting_returns_false_when_effective_verdict_matches_majority
// Test: conditional_agent_is_not_dissenting_against_approve_majority
// Test: effective_verdict_maps_conditional_to_approve
// Test: agent_output_serializes_and_deserializes_roundtrip_with_all_fields
// [EDGE] agent_output_with_empty_findings_vec_is_valid
// [EDGE] agent_output_deserialization_ignores_unknown_fields
```

---

## Section 2: Validation

### validate.rs

```rust
// Test: validator_new_creates_with_default_limits
// Test: validation_limits_default_returns_spec_values_100_500_10000_50000_0_1
// Test: validator_with_limits_uses_custom_limits

// [BDD-10] confidence out of range
// Test: validate_rejects_confidence_above_one_with_validation_error
// Test: validate_rejects_confidence_below_zero_with_validation_error
// Test: validate_accepts_confidence_at_exact_boundaries_zero_and_one
// [EDGE] validate_rejects_confidence_nan_with_validation_error
// [EDGE] validate_rejects_confidence_infinity_with_validation_error

// [BDD-11] empty title after strip zero-width
// Test: validate_rejects_finding_with_title_containing_only_zero_width_chars
// Test: validate_accepts_finding_with_normal_title

// [BDD-12] text field exceeds max_text_len
// Test: validate_rejects_reasoning_exceeding_max_text_len
// Test: validate_rejects_summary_exceeding_max_text_len
// Test: validate_rejects_recommendation_exceeding_max_text_len
// [EDGE] validate_accepts_text_field_at_exact_max_text_len_boundary

// Test: validate_rejects_findings_count_exceeding_max_findings
// Test: validate_rejects_finding_title_exceeding_max_title_len
// Test: validate_rejects_finding_detail_exceeding_max_detail_len
// Test: validate_accepts_valid_agent_output_with_all_fields_within_limits
// Test: validate_accepts_agent_output_with_zero_findings
// Test: validate_reports_first_failing_field_in_validation_order
// Test: validation_error_message_includes_field_name_for_diagnostics
// Test: strip_zero_width_removes_unicode_category_cf_characters
// [EDGE] validate_with_custom_lower_limits_rejects_shorter_text
// [EDGE] validate_rejects_finding_with_empty_string_title
// [EDGE] validate_title_length_checked_after_strip_zero_width_not_before
//   Assert: title with 500 normal + 10 zero-width chars (510 bytes total) passes
//   max_title_len=500 because length is checked on the stripped result
```

---

## Section 3: Consensus Engine

### consensus.rs -- Core Scenarios

```rust
// [BDD-01] unanimous approve
// Test: three_approve_agents_produce_strong_go_with_full_confidence
//   Assert: score=1.0, label="STRONG GO", verdict=Approve, confidence≈0.9, dissent empty

// [BDD-02] mixed 2 approve + 1 reject
// Test: two_approve_one_reject_produce_go_with_dissent
//   Assert: score≈0.333, label="GO (2-1)", verdict=Approve, dissent contains Caspar

// [BDD-03] approve + conditional + reject
// Test: approve_conditional_reject_produce_go_with_caveats
//   Assert: score≈0.167, label="GO WITH CAVEATS", verdict=Approve, conditions from conditional agent

// [BDD-04] unanimous reject
// Test: three_reject_agents_produce_strong_no_go
//   Assert: score=-1.0, label="STRONG NO-GO", verdict=Reject, confidence≈0.95

// [BDD-05] tie with 2 synthetic agents
// Test: one_approve_one_reject_two_agents_produce_hold_tie
//   Assert: score=0, label="HOLD -- TIE", verdict=Reject (tie favors rejection)

// [BDD-33] degraded mode caps STRONG labels
// Test: two_approve_degraded_caps_strong_go_to_go_label
//   Assert: label="GO (2-0)", NOT "STRONG GO", agent_count=2
// Test: two_reject_degraded_caps_strong_no_go_to_hold_label
//   Assert: label="HOLD (2-0)", NOT "STRONG NO-GO"
```

### consensus.rs -- Finding Deduplication

```rust
// [BDD-13] deduplication by title
// Test: same_title_different_case_merges_and_promotes_severity_to_highest
// Test: merged_finding_sources_include_all_reporting_agents
// Test: merged_detail_preserved_from_highest_severity_finding
// Test: same_severity_preserves_detail_from_first_agent_in_iteration_order
// [EDGE] three_agents_same_finding_title_produces_single_finding_with_three_sources
// [EDGE] all_agents_with_zero_findings_produce_empty_dedup_findings
// [EDGE] findings_sorted_by_severity_critical_first_info_last
// [EDGE] finding_from_single_agent_not_deduplicated_sources_has_one_entry
```

### consensus.rs -- Majority Tiebreak

```rust
// Test: three_agent_binary_verdicts_always_produce_clear_majority
//   Assert: with 3 agents and effective verdicts {Approve, Reject}, a 2-1 or 3-0
//   split always has a clear majority — no tiebreak needed. Document this invariant.
// [EDGE] two_agent_count_tie_broken_by_agent_name_alphabetical_order
//   Assert: with 2 agents (Caspar=approve, Melchior=reject), Caspar's side wins
//   because Caspar < Melchior alphabetically (AgentName::cmp Ord)
```

### consensus.rs -- Error Handling and Config Defaults

```rust
// Test: determine_rejects_fewer_than_min_agents_with_insufficient_agents_error
// Test: determine_rejects_duplicate_agent_names_with_validation_error
// Test: consensus_config_default_returns_min_agents_two_and_epsilon_1e9
// [ADDENDUM] consensus_config_rejects_min_agents_zero
//   Note: spec does not explicitly require constructor validation of min_agents >= 1,
//   but division by zero in compute_score would panic. Added as defensive measure.
//   Implementation: requires adding a validating constructor (e.g., ConsensusConfig::new
//   or a builder) beyond the spec's Default impl. The Default returns min_agents=2
//   which is safe; this test covers the case where a user constructs with min_agents=0.
// [EDGE] determine_with_exactly_min_agents_succeeds
// [EDGE] determine_rejects_empty_agents_slice_with_insufficient_agents_error
```

### consensus.rs -- Epsilon and Boundaries

```rust
// Test: epsilon_aware_classification_near_zero_produces_hold_tie
// Test: epsilon_aware_classification_near_positive_one_produces_strong_go
// Test: epsilon_aware_classification_near_negative_one_produces_strong_no_go
// [EDGE] score_just_above_epsilon_classifies_as_go_not_tie
// [EDGE] score_just_below_negative_epsilon_classifies_as_hold_not_tie
```

### consensus.rs -- Confidence Formula

```rust
// Test: confidence_formula_applies_weight_factor_to_base_confidence
// Test: confidence_clamped_to_zero_one_range
// Test: confidence_rounded_to_two_decimal_places
// [EDGE] confidence_with_unanimous_full_score_produces_maximum_weighted_value
// [EDGE] confidence_diluted_by_dissenting_agent_count
```

### consensus.rs -- Result Fields

```rust
// Test: majority_summary_joins_majority_agent_summaries_with_pipe_separator
// Test: conditions_extracted_from_agents_with_conditional_verdict
// Test: recommendations_map_includes_all_participating_agents
// Test: votes_map_contains_individual_verdict_per_agent
// Test: consensus_result_score_and_agent_count_match_inputs
// [EDGE] three_conditional_agents_produce_go_with_caveats_all_conditions
// [EDGE] two_conditional_one_reject_at_score_zero_produce_hold_tie
```

---

## Section 4: Reporting

### reporting.rs -- ReportFormatter

```rust
// [BDD-15] banner width
// Test: all_banner_lines_are_exactly_52_characters_wide
// Test: banner_with_long_consensus_label_fits_52_chars
// [EDGE] banner_with_confidence_100_percent_formats_correctly
// [EDGE] banner_with_confidence_0_percent_formats_correctly

// [BDD-16] report contains all sections
// Test: mixed_consensus_report_contains_all_five_markdown_headers
// Test: unanimous_approve_report_omits_dissenting_opinion_section
// Test: no_conditions_report_omits_conditions_for_approval_section
// Test: no_findings_report_omits_key_findings_section
// [EDGE] report_section_order_matches_spec_banner_summary_findings_dissent_conditions_actions

// Test: format_banner_generates_correct_ascii_art_structure
// Test: format_init_banner_includes_mode_model_and_timeout
// Test: format_separator_produces_plus_equals_50_plus
// Test: format_agent_line_shows_name_title_verdict_percentage
// Test: format_findings_shows_icon_severity_title_sources_detail
// Test: format_dissent_shows_agent_name_summary_full_reasoning
// Test: format_conditions_shows_bulleted_list_with_agent_names
// Test: format_recommendations_shows_per_agent_recommendations
// Test: agent_display_falls_back_to_agent_name_methods_for_unknown_agent
// [EDGE] format_findings_uses_question_mark_icon_for_unknown_severity
```

### reporting.rs -- MagiReport

```rust
// Test: magi_report_serializes_to_json_matching_python_original_structure
// Test: magi_report_degraded_false_when_all_three_agents_succeed
// Test: magi_report_degraded_true_with_failed_agents_populated
// Test: magi_report_agent_names_serialize_as_lowercase_in_json
// Test: magi_report_consensus_confidence_rounded_to_two_decimals
// [EDGE] magi_report_json_votes_keys_are_lowercase_agent_names
// [EDGE] magi_report_json_omits_failed_agents_when_not_degraded
//   Assert: when degraded=false, the serialized JSON either omits the
//   "failed_agents" field entirely (via #[serde(skip_serializing_if)])
//   or includes it as []. Spec says "solo se incluye cuando degraded es true".
//   Verify against Python original behavior (always present as []).
// [EDGE] magi_report_json_includes_failed_agents_when_degraded
//   Assert: when degraded=true, "failed_agents" is present with agent names lowercase
```

---

## Section 5: LlmProvider Trait and RetryProvider

### provider.rs -- CompletionConfig

```rust
// Test: completion_config_default_has_max_tokens_4096_temperature_0
// Test: completion_config_allows_field_modification_after_construction
```

### provider.rs -- RetryProvider

> **[ADDENDUM]** RetryProvider is not backed by a BDD scenario.
> The spec says "No debe hacer retry automatico a nivel de orquestador."
> RetryProvider is an opt-in user-facing wrapper added per interview decision
> (claude-interview.md Q4). It lives in provider.rs, not in the orchestrator.
> All tests below are tagged [ADDENDUM] for traceability.

```rust
// [ADDENDUM] retry_provider_name_returns_inner_provider_name_and_model
// [ADDENDUM] retry_provider_retries_on_timeout_up_to_max_retries
// [ADDENDUM] retry_provider_retries_on_http_500_server_error
// [ADDENDUM] retry_provider_retries_on_http_429_rate_limit
// [ADDENDUM] retry_provider_retries_on_network_error
// [ADDENDUM] retry_provider_does_not_retry_on_auth_error
// [ADDENDUM] retry_provider_does_not_retry_on_process_error
// [ADDENDUM] retry_provider_does_not_retry_on_nested_session_error
// [ADDENDUM] retry_provider_returns_last_error_after_exhausting_all_retries
// [ADDENDUM] retry_provider_returns_success_on_first_successful_attempt
// [ADDENDUM] retry_provider_default_config_three_retries_one_second_delay
// [ADDENDUM][EDGE] retry_provider_with_zero_max_retries_makes_single_attempt
// [ADDENDUM][EDGE] retry_provider_does_not_retry_http_400_bad_request
// [ADDENDUM][EDGE] retry_provider_does_not_retry_http_401_unauthorized
// [ADDENDUM][EDGE] retry_provider_does_not_retry_http_403_forbidden
// [ADDENDUM][EDGE] retry_provider_does_not_retry_http_404_not_found
```

---

## Section 6: Agents and AgentFactory

### agent.rs -- Agent

```rust
// Test: agent_default_prompt_reflects_agent_role_and_mode
// Test: agent_with_custom_prompt_uses_provided_prompt_string
// Test: agent_execute_delegates_to_provider_complete_with_system_prompt
// Test: agent_accessors_return_correct_name_mode_prompt_provider_info

// [BDD-30] modes generate different prompts
// Test: each_mode_produces_distinct_system_prompts_per_agent

// [BDD-31] from_directory with nonexistent path
// Test: agent_from_file_with_nonexistent_path_returns_io_error
// [EDGE] agent_from_file_with_valid_file_reads_prompt_content
// [EDGE] default_prompts_contain_json_schema_instructions
// [EDGE] default_prompts_instruct_english_only_responses
```

### agent.rs -- AgentFactory

```rust
// [BDD-26] agents with different providers
// Test: each_agent_invokes_its_own_provider_verified_by_mock_call_count

// [BDD-27] factory with default and override
// Test: factory_uses_default_provider_for_unoverridden_agents_and_override_for_specified

// Test: factory_new_creates_three_agents_sharing_default_provider
// Test: factory_with_provider_overrides_provider_for_specific_agent
// Test: factory_with_custom_prompt_overrides_prompt_for_specific_agent
// Test: factory_create_agents_returns_exactly_three_agents_for_any_mode
// [EDGE] factory_create_agents_returns_agents_in_order_melchior_balthasar_caspar
// [EDGE] factory_from_directory_skips_missing_individual_agent_files_silently
//   Note: This is NOT the same as BDD-31. BDD-31 tests Agent::from_file() with a
//   nonexistent file path → MagiError::Io. Here, AgentFactory::from_directory()
//   requires the DIRECTORY to exist (else MagiError::Io), but individual agent
//   files (melchior.md, balthasar.md, caspar.md) within it may be absent — those
//   are silently skipped, falling back to default embedded prompts.
// [EDGE] factory_from_directory_returns_io_error_when_directory_does_not_exist
```

---

## Section 7: Orchestrator

### orchestrator.rs -- MagiConfig

```rust
// Test: magi_config_default_timeout_is_300_seconds
// Test: magi_config_default_max_input_len_is_one_megabyte
```

### orchestrator.rs -- MagiBuilder

```rust
// [BDD-28] Magi::new with single provider
// Test: magi_new_creates_instance_with_single_provider_and_all_defaults

// [BDD-29] builder with mixed providers and custom config
// Test: builder_assigns_per_agent_providers_and_custom_timeout

// Test: builder_build_returns_result_ok_with_valid_provider
// [EDGE] magi_new_equivalent_to_builder_with_defaults
```

### orchestrator.rs -- build_prompt

```rust
// Test: build_prompt_formats_mode_context_with_newlines
// [EDGE] build_prompt_with_empty_content_produces_valid_format
```

### orchestrator.rs -- parse_agent_response

```rust
// Test: parse_agent_response_strips_json_code_fences
// Test: parse_agent_response_finds_json_object_in_preamble_text
// Test: parse_agent_response_fails_on_completely_invalid_input_with_deserialization_error
// [EDGE] parse_agent_response_strips_plain_code_fences_without_json_suffix
// [EDGE] parse_agent_response_handles_json_preceded_by_explanation_text
// [EDGE] parse_agent_response_handles_valid_json_without_any_wrapping
```

### orchestrator.rs -- analyze flow

```rust
// [BDD-01] successful analysis end-to-end
// Test: analyze_with_three_unanimous_approve_returns_full_report_not_degraded

// [BDD-06] degradation - 1 agent timeout
// Test: analyze_with_one_agent_timeout_returns_degraded_report_with_two_agents

// [BDD-07] degradation - 1 agent invalid JSON
// Test: analyze_with_one_agent_bad_json_returns_degraded_report

// [BDD-08] 2 agents fail
// Test: analyze_with_two_agents_failed_returns_insufficient_agents_error

// [BDD-09] all agents fail
// Test: analyze_with_all_agents_failed_returns_insufficient_agents_error_zero_succeeded

// [BDD-14] LLM returns non-JSON
// Test: analyze_treats_non_json_agent_response_as_failure_continues_with_remaining

// [BDD-32] input too large
// Test: analyze_rejects_input_exceeding_max_len_with_input_too_large_error_without_launching_agents

// [EDGE] analyze_accepts_empty_content_zero_bytes
// [EDGE] analyze_accepts_content_at_exact_max_input_len_boundary
// [EDGE] analyze_validation_failure_in_agent_output_treated_as_agent_failure_not_propagated
// [EDGE] analyze_tracks_failed_agent_names_correctly_in_report
```

### orchestrator.rs -- Cancellation Safety

```rust
// [ADDENDUM] dropped_analyze_future_aborts_spawned_agent_tasks
//   Not in any BDD scenario. Tests tokio JoinSet cancellation safety to prevent
//   leaked LLM API calls when the caller drops the analyze future.
```

---

## Section 8: MagiReport (in reporting.rs)

Covered by Section 4 tests above.

---

## Section 9: RetryProvider (in provider.rs)

Covered by Section 5 tests above.

---

## Section 10: ClaudeProvider (HTTP API)

### providers/claude.rs

```rust
// Test: claude_provider_new_creates_provider_with_api_key_and_model
// Test: claude_provider_name_returns_claude
// Test: claude_provider_model_returns_configured_model_string
// Test: complete_sends_post_to_messages_api_with_correct_headers
// Test: complete_maps_non_2xx_response_to_http_provider_error
// Test: complete_extracts_text_content_from_claude_response_format
// Test: claude_provider_maintains_connection_pool_across_calls
// [EDGE] complete_maps_connection_error_to_network_provider_error
// [EDGE] complete_maps_request_timeout_to_timeout_provider_error
// [EDGE] complete_handles_empty_content_blocks_in_response
// [EDGE] api_key_not_exposed_in_debug_output
```

---

## Section 11: ClaudeCliProvider (CLI Subprocess)

### providers/claude_cli.rs -- Constructor and Model Resolution

```rust
// Test: new_sonnet_maps_to_claude_sonnet_model_id
// Test: new_opus_maps_to_claude_opus_model_id
// Test: new_haiku_maps_to_claude_haiku_model_id
// Test: new_passthrough_accepts_model_id_containing_claude_prefix
// Test: new_rejects_invalid_model_with_auth_error
// Test: provider_name_returns_claude_cli
// Test: provider_model_returns_resolved_model_id
// [EDGE] new_rejects_empty_string_model_with_auth_error
// [EDGE] new_rejects_mixed_case_alias_sonnet_is_case_sensitive
//   Assert: new("Sonnet") and new("SONNET") return ProviderError::Auth
//   because alias matching is lowercase-only per spec whitelist

// [BDD-23] nested session detection
// Test: new_with_claudecode_env_var_returns_nested_session_error
```

### providers/claude_cli.rs -- CLI Arguments and I/O

```rust
// [BDD-18] launches subprocesses with correct flags
// Test: build_args_includes_print_output_format_model_system_prompt
// Test: user_prompt_sent_via_stdin_not_as_cli_argument
```

### providers/claude_cli.rs -- JSON Parsing

```rust
// [BDD-19] parses double-nested JSON
// Test: parse_cli_output_extracts_inner_json_from_result_envelope

// [BDD-20] detects error in CLI response
// Test: parse_cli_output_returns_process_error_when_is_error_true

// [BDD-21] strips code fences
// Test: extract_json_removes_json_code_fence_wrapping
// Test: extract_json_returns_unchanged_text_without_fences
// [EDGE] extract_json_strips_plain_code_fences_without_json_suffix
// [EDGE] parse_cli_output_returns_process_error_for_malformed_outer_json
```

### providers/claude_cli.rs -- Timeout and Process Errors

```rust
// [BDD-22] handles timeout
// Test: timeout_kills_child_process_and_returns_timeout_error

// [EDGE] non_zero_exit_code_returns_process_error_with_exit_code_and_stderr
```

---

## Section 12: Prelude and Crate Root

```rust
// Test: prelude_reexports_core_types_magi_mode_verdict_severity_agent_name
// Test: prelude_reexports_config_types_completion_config_magi_config
// Test: prelude_reexports_error_types_magi_error_provider_error
// Test: crate_compiles_with_no_features_enabled_core_only
// Test: crate_compiles_with_claude_api_feature
// Test: crate_compiles_with_claude_cli_feature
// [EDGE] crate_compiles_with_all_features_enabled
```

---

## Out of Scope (v1.1 / v1.2) -- Documented for Traceability

```
// [BDD-17] OpenAiProvider with custom base_url (v1.2) -- NOT in v1.0 scope
// [BDD-24] GeminiCliProvider (v1.1) -- NOT in v1.0 scope
// [BDD-25] OpenAiProvider without API key (v1.2) -- NOT in v1.0 scope
```

---

## Review Summary

| Category | Original | Post-review | Post-MAGI | Delta |
|----------|----------|-------------|-----------|-------|
| BDD scenarios covered | 30/33 | 30/33 + 3 OOS | same | +3 traced |
| Error type tests | 8 | 16 | same | +8 |
| Edge case tests | 0 | 42 `[EDGE]` | 59 `[EDGE]` | +17 MAGI |
| Addendum tests | 0 | 0 | 18 `[ADDENDUM]` | +18 traced |
| Naming fixes | 0 | ~15 | +4 MAGI | behavior over impl |
| Tiebreak test | 0 | 0 | 2 | +2 (critical gap) |
| Default value tests | 0 | 0 | 2 | +2 (regression safety) |
| **Total test stubs** | ~120 | ~175 | **247** | +72 total |
|   - `Test:` (BDD-backed) | | | 170 | |
|   - `[EDGE]` | | | 59 | |
|   - `[ADDENDUM]` | | | 13 | |
|   - `[ADDENDUM][EDGE]` | | | 5 | |

### MAGI Conditions Resolved

| Issue | Agent(s) | Resolution |
|-------|----------|------------|
| Missing tiebreak test | All 3 | Added `two_agent_count_tie_broken_by_agent_name_alphabetical_order` + invariant test |
| ValidationLimits defaults | Balthasar | Added `validation_limits_default_returns_spec_values_100_500_10000_50000_0_1` |
| strip_zero_width vs max_title_len | Melchior | Added `validate_title_length_checked_after_strip_zero_width_not_before` |
| RetryProvider traceability | Caspar | All 16 RetryProvider stubs tagged `[ADDENDUM]` with justification note |
| failed_agents JSON ambiguity | Caspar | Added `magi_report_json_omits_failed_agents_when_not_degraded` + `_includes_` test |
| CLI alias case sensitivity | Caspar | Added `new_rejects_mixed_case_alias_sonnet_is_case_sensitive` |
| from_directory vs from_file | Balthasar | Clarified with inline note + added `factory_from_directory_returns_io_error_when_directory_does_not_exist` |
| Impl-describing names | Melchior+Caspar | Renamed 4 tests to describe behavior |
| ConsensusConfig defaults | Balthasar | Added `consensus_config_default_returns_min_agents_two_and_epsilon_1e9` |
| min_agents validation | Melchior | Flagged as `[ADDENDUM]` with defensive justification note |
