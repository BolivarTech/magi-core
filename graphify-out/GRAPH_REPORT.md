# Graph Report - MAGI-Core  (2026-09-14)

## Corpus Check
- 203 files · ~457,870 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 3879 nodes · 9354 edges · 270 communities (227 shown, 43 thin omitted)
- Extraction: 98% EXTRACTED · 2% INFERRED · 0% AMBIGUOUS · INFERRED: 166 edges (avg confidence: 0.85)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `aaaaaa28`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- rotation_integration.rs
- Lineage
- consensus.rs
- validate.rs
- parse_and_validate
- .new
- migrate_openai_compat_constructors.py
- schema.rs
- ollama_wire.rs
- MagiBuilder
- user_prompt.rs
- .new
- provider_url.rs
- Balthasar — The Pragmatist
- MAGI System Technical Documentation
- Caspar — The Critic
- Melchior — The Scientist
- Self
- test_support.rs
- MagiReport
- MAGI System Technical Documentation
- ollama.rs
- r7.rs
- .new
- proxy.rs
- run
- magi_report_v0_3_1.json fixture
- make_output
- [0.5.0] - 2026-05-16
- verdict_markers.rs
- [0.2.0] - 2026-04-18
- s4_rotation_and_its_cause
- [0.4.0] - 2026-05-16
- report.rs
- Changelog
- [1.0.0] - 2026-05-24
- main
- RoutingMockProvider
- build_user_prompt
- [1.0.1] - 2026-05-25
- e1.rs
- magi-core
- build_retry_prompt
- e2.rs
- orchestrator.rs
- VerdictExtractionError
- Quick Start
- error.rs
- LlmProvider
- reporting.rs
- Finding
- [0.3.0] - 2026-04-18
- backoff.rs
- .new
- Voting rules + confidence formula
- Evangelion MAGI origin (Naoko Akagi)
- MAGI System Technical Documentation
- Structured disagreement rationale
- Why three perspectives (not 2 or 5)
- Self
- prompts/mod.rs
- .parse
- r5.rs
- runner.rs
- LineageRegistry
- main.rs
- .redacted
- 6. Modes of Operation
- Model Rotation
- ProviderError
- .new
- fixtures.rs
- Report
- .leak
- redaction_negative.rs
- [3.1.0] - 2026-07-31
- 5. Data Schema and Consensus Protocol
- HostedModel
- RetryConfig
- complete_against_stub
- Option
- .forward
- .write_certificate_in
- RunContext
- RunId
- preflight.rs
- .read_verdict_body
- .new
- ConsensusResult
- ValidationLimits
- .set_injection
- MagiError
- OllamaProvider
- .new
- .cmp
- provider.rs
- Debug
- stub_cli.rs
- Formatter
- ProviderError
- outcome.rs
- FinishReason
- ProviderProbe
- [1.1.0] - 2026-05-25
- BTreeMap
- Default
- 1. Origin: The MAGI Supercomputers from Evangelion
- Send
- Sync
- openai_compat.rs
- Into
- Option
- check_readme_version.sh
- 4. Library Architecture
- BTreeMap
- Drop
- F
- HashMap
- P
- 7. Design Philosophy
- MAGI System — Complete Technical Documentation
- Debug
- Duration
- Severity
- RunResult
- testkit.rs
- String
- Completion
- check_calibration.sh
- redacted
- .leak
- redacted
- redacted
- SpyProxy
- redacted
- redacted
- .leak
- redacted
- redacted
- .leak
- redacted
- balthasar_prompt
- finding_id.rs
- [4.0.0] - 2026-08-24
- check_no_deprecated_ctors.sh
- .skip
- CompletionRecord
- Box
- [1.1.0] - 2026-05-25
- sync-fixtures.sh
- Drop
- [3.0.2] - 2026-07-30
- check_fixture_redaction.py
- [0.6.0] - 2026-05-21
- Vec
- Injection
- infallible_to_box
- provider_url.rs
- AgentName
- pattern0nonterminal_bad.rs
- pattern0nonterminal_good.rs
- .new_checked
- check_header_sync.py
- mock_server.rs
- [3.0.0] - 2026-07-30
- 2. Translation to the Software Engineering Domain
- config.rs
- ProviderResponse
- check_warn_once.sh
- 3. The Three Agents in Detail
- AssertionRow
- smoke-certificate.md
- .new
- git.rs
- 6. Modes of Operation
- claude_cli.rs
- common/mod.rs
- Model Rotation
- [1.1.0] - 2026-05-25
- ReportFormatter
- [1.1.1] - 2026-07-17
- claude.rs
- Architecture
- magi-smoke — the smoke harness
- e.rs
- ProviderError
- compose_transport_message
- magi-core
- check_changelog_disclosures.py
- FixedRng
- [2.0.0] - 2026-07-25
- normalize_newlines
- [3.0.1] - 2026-07-30
- EventLog
- magi-smoke
- pattern8bconst_bad.rs
- FallbackPool
- log_failure
- AgentOutput
- retry_template
- rotation.rs
- Duration
- .send
- described
- pattern8bnospace_bad.rs
- pattern8bnospace_good.rs
- ProviderRequest
- pattern8bcrate_good.rs
- check_prose_artifacts.sh
- compose_transport_message
- [4.1.0] - 2026-09-14
- EC fixtures — captured responses from Ollama, both wire formats
- pattern7builderalias_bad.rs
- pattern7builderalias_good.rs
- pattern7default_bad.rs
- pattern7default_good.rs
- weakened.rs
- s15_degradation_is_honest
- AgentName
- check_pending.sh
- Migrating from 3.2.0 to 4.0.0
- check_packaged_consumer.sh

## God Nodes (most connected - your core abstractions)
1. `AgentName` - 98 edges
2. `blank_ctx()` - 60 edges
3. `RunContext` - 59 edges
4. `LlmProvider` - 59 edges
5. `Completion` - 49 edges
6. `MagiError` - 48 edges
7. `Assertion` - 46 edges
8. `MagiBuilder` - 44 edges
9. `CompletionConfig` - 42 edges
10. `MagiReport` - 42 edges

## Surprising Connections (you probably didn't know these)
- `a_verbose_child_does_not_hang_the_parent()` --calls--> `cannot_test()`  [INFERRED]
  tests/claude_cli_integration.rs → src/test_support.rs
- `f()` --calls--> `compose_transport_message()`  [INFERRED]
  ci/fixtures/redaction/pattern1debug_good.rs → src/provider.rs
- `f()` --calls--> `compose_transport_message()`  [INFERRED]
  ci/fixtures/redaction/pattern1display_good.rs → src/provider.rs
- `f()` --calls--> `compose_transport_message()`  [INFERRED]
  ci/fixtures/redaction/pattern1pretty_good.rs → src/provider.rs
- `f()` --calls--> `compose_transport_message()`  [INFERRED]
  ci/fixtures/redaction/pattern1tostring_good.rs → src/provider.rs

## Import Cycles
- None detected.

## Communities (270 total, 43 thin omitted)

### Community 0 - "rotation_integration.rs"
Cohesion: 0.12
Nodes (15): build_oversized_case(), build_schema_local_case(), build_trio_with_caspar(), a_seat_that_rotated_leaves_one_entry_per_model(), an_empty_completion_leaves_its_measurement_in_the_report(), an_external_failure_rotates_the_seat_and_the_run_completes(), an_oversized_response_is_mage_local_and_the_run_completes(), retry0() (+7 more)

### Community 1 - "Lineage"
Cohesion: 0.13
Nodes (23): ActiveEntry, Candidate, digest_collision(), EligibilityInputs, empty(), empty_s(), empty_wr(), inputs() (+15 more)

### Community 2 - "consensus.rs"
Cohesion: 0.09
Nodes (69): a_score_equal_to_epsilon_classifies_as_hold_deliberately(), agent(), ConsensusConfig, ConsensusEngine, dedup_key(), DedupKey, engine(), finding_key() (+61 more)

### Community 3 - "validate.rs"
Cohesion: 0.07
Nodes (29): finding_with_title(), output_with_confidence(), output_with_findings(), Vec, test_validate_accepts_confidence_at_boundaries(), test_validate_accepts_finding_with_normal_title(), test_validate_accepts_valid_agent_output(), test_validate_mut_collapses_control_whitespace_in_titles() (+21 more)

### Community 4 - "parse_and_validate"
Cohesion: 0.10
Nodes (24): a_missing_key_is_a_malformed_object(), a_wrong_type_is_also_malformed_object_not_invalid_json(), magi_error_for(), mock_agent_object(), parse_agent_response(), parse_and_validate(), ParseFailure, test_cause_survives_independently_of_the_error_message_text() (+16 more)

### Community 5 - ".new"
Cohesion: 0.20
Nodes (30): make_agent(), make_consensus(), test_agent_display_fallback_to_agent_name_methods(), test_agent_line_format(), test_banner_all_lines_are_exactly_banner_width(), test_banner_consensus_line_includes_split_for_go_with_caveats(), test_banner_labels_are_column_aligned_to_max_label_len(), test_banner_lines_are_exactly_52_bytes_wide() (+22 more)

### Community 6 - "migrate_openai_compat_constructors.py"
Cohesion: 0.18
Nodes (15): discover(), _find_matching_close_paren(), main(), Path, qualified_paths(), Return the index of the matching ``)`` starting from ``open_index``. Args:…, Split an argument list so the dialect can be inserted before the last arg.…, Rewrite a single-line old constructor call to ``::with_dialect``. The… (+7 more)

### Community 7 - "schema.rs"
Cohesion: 0.05
Nodes (6): make_output(), test_agent_output_conditional_is_not_dissenting_from_an_approve_verdict(), test_agent_output_effective_verdict_maps_conditional_to_approve(), test_agent_output_empty_findings_valid(), test_agent_output_is_dissenting_when_verdict_differs_from_the_given_one(), test_agent_output_is_not_dissenting_when_verdict_matches_the_given_one()

### Community 8 - "ollama_wire.rs"
Cohesion: 0.10
Nodes (21): a_native_empty_completion_carries_the_reasoning_it_burned(), a_native_empty_without_the_footprint_falls_back_to_empty_completion(), an_overlong_done_reason_is_capped_as_it_is_deserialized(), done_reason_body(), into_completion_carries_a_measured_reasoning_trace_when_present(), into_completion_carries_the_telemetry_of_a_real_success(), is_crate_defect(), native_empty_with_counters() (+13 more)

### Community 9 - "MagiBuilder"
Cohesion: 0.07
Nodes (34): ComplexityGate, MockProbe, a_replaced_provider_clears_the_probe_it_no_longer_matches(), CapturingMockProvider, contract_prompt(), default_provider(), MagiBuilder, provider_a_with_probe() (+26 more)

### Community 11 - ".new"
Cohesion: 0.17
Nodes (31): a_529_is_retried_and_the_second_attempt_succeeds(), a_limited_class_stops_at_its_own_count_not_at_max_retries(), a_mage_local_class_survives_the_attempt_cap_exit_too(), a_mage_local_class_survives_the_retry_loop_unwrapped(), a_present_retry_after_meets_the_attempt_cap_for_a_configured_http_class(), a_retry_after_that_is_too_long_abandons_with_the_incremented_count(), a_run_wide_class_still_comes_out_wrapped(), cfg() (+23 more)

### Community 12 - "provider_url.rs"
Cohesion: 0.13
Nodes (9): a_partial_read_stays_within_the_cap_including_its_marker(), diagnostic_truncation_is_announced_and_utf8_safe(), diagnostic_under_the_cap_is_untouched(), mark_truncated(), mark_truncated_at_exactly_the_cap_still_cuts_and_marks(), marking_a_short_text_does_not_pad_it(), marking_never_splits_a_multibyte_character(), truncate_diagnostic() (+1 more)

### Community 13 - "Balthasar — The Pragmatist"
Cohesion: 0.17
Nodes (11): Balthasar — The Pragmatist, Constraints, Finding calibration (code-review mode only), In analysis mode, In code review mode, In design mode, Input format, Output format (+3 more)

### Community 14 - "MAGI System Technical Documentation"
Cohesion: 0.27
Nodes (10): main(), main(), Path, MAGI R1 W4: pre-write check that the pinned SHA exists in the repo before…, verify_sha_exists(), apply_divergences(), Path, Apply every declared divergence to a reference blob, failing loudly. Returns… (+2 more)

### Community 15 - "Caspar — The Critic"
Cohesion: 0.17
Nodes (11): Caspar — The Critic, Constraints, Finding calibration (code-review mode only), In analysis mode, In code review mode, In design mode, Input format, Output format (+3 more)

### Community 16 - "Melchior — The Scientist"
Cohesion: 0.17
Nodes (11): Constraints, Finding calibration (code-review mode only), In analysis mode, In code review mode, In design mode, Input format, Melchior — The Scientist, Output format (+3 more)

### Community 17 - "Self"
Cohesion: 0.10
Nodes (16): an_external_implementor_can_report_real_telemetry_not_only_unmeasured(), BudgetBearing, CompletionTelemetry, elided(), every_contract_variant_has_its_own_retry_class(), production_half(), ReasoningState, RetryAfterProvider (+8 more)

### Community 18 - "test_support.rs"
Cohesion: 0.11
Nodes (26): an_absent_api_status_is_distinguishable_from_an_unreadable_one(), an_out_of_range_api_error_status_is_not_an_http_error(), an_oversized_envelope_result_is_capped_and_says_so(), the_cli_status_is_classified_like_the_http_path(), the_in_band_diagnosis_is_labelled_and_capped_like_its_sibling(), a_panicking_closure_still_restores_the_environment(), cannot_test(), captured_failure_with_api_error_status() (+18 more)

### Community 19 - "MagiReport"
Cohesion: 0.16
Nodes (17): a_clean_run_gains_no_section_at_all(), a_report_from_before_this_version_also_gains_nothing(), an_exceeded_run_gains_exactly_one_section_naming_both_numbers(), ExtractionFailure, InputSize, MagiReport, report_text(), BTreeMap (+9 more)

### Community 20 - "MAGI System Technical Documentation"
Cohesion: 0.40
Nodes (4): Exemption from CLAUDE.local.md §0.2 file-header rule, Local divergence from the pinned reference (F0, 2026-07-16), Regeneration, `src/prompts_md/` — Embedded prompt data

### Community 21 - "ollama.rs"
Cohesion: 0.15
Nodes (15): a_mounted_prefix_survives_both_spellings(), a_root_that_really_ends_in_v1_is_read_as_the_prefix(), construction_keeps_the_real_credentials_on_the_authority(), every_accepted_spelling_yields_the_same_endpoints(), Duration, Into, ProviderUrl, Self (+7 more)

### Community 22 - "r7.rs"
Cohesion: 0.15
Nodes (28): CompletionEvidence, a_failed_completion_skips_naming_the_error(), both_dialect_scenarios_are_registered(), cut_at_the_cap(), evidence(), finish_label(), is_cut_at_the_cap(), measured() (+20 more)

### Community 23 - ".new"
Cohesion: 0.28
Nodes (13): Future, a_receipt_BEFORE_the_runs_is_refused_even_though_the_estimate_was_announced(), a_receipt_is_refused_when_the_intervals_do_not_MATCH_the_announced_runs(), a_run_that_ends_badly_is_still_measured_so_the_receipt_is_not_withheld(), backend_runs_of(), CostLedger, Default, Output (+5 more)

### Community 24 - "proxy.rs"
Cohesion: 0.13
Nodes (18): a_backend_that_never_answers_is_cut_by_the_proxys_own_bound(), a_broken_response_read_is_not_answered_as_an_empty_success(), a_broken_response_read_is_not_recorded_as_an_empty_answer(), a_poisoned_registry_degrades_but_the_proxy_keeps_serving(), a_poisoned_registry_does_not_report_a_zero_watermark(), a_poisoned_registry_makes_records_since_report_degraded_too(), a_response_larger_than_the_cap_is_refused_instead_of_held_whole(), accept_failure_action() (+10 more)

### Community 25 - "run"
Cohesion: 0.08
Nodes (41): a_broken_proxy_makes_the_preflight_say_cannot_test_not_failed(), a_probe_that_is_slow_names_both_possible_causes(), Announcement, audit_published_package(), check_lock_is_tracked(), check_workspace_isolation(), classify_probe_body(), harness_files_in_listing() (+33 more)

### Community 27 - "make_output"
Cohesion: 0.40
Nodes (4): Adding a pair, One pair per ALTERNATIVE, not per rule, Redaction-gate fixtures, Verifying a change

### Community 28 - "[0.5.0] - 2026-05-16"
Cohesion: 0.25
Nodes (8): [0.5.0] - 2026-05-16, Added, Backward compatibility, Changed (breaking), Documentation, Performance, Pre-merge gates (CLAUDE.local.md §6), Test count

### Community 29 - "verdict_markers.rs"
Cohesion: 0.06
Nodes (4): is_marker_line(), normalize_line(), Cow, test_locate_block_argument_order_is_not_symmetric()

### Community 30 - "[0.2.0] - 2026-04-18"
Cohesion: 0.29
Nodes (7): [0.2.0] - 2026-04-18, Added, Changed (breaking), Dependencies, Deprecated, Not included (deferred to v0.3.0), Security considerations

### Community 31 - "s4_rotation_and_its_cause"
Cohesion: 0.19
Nodes (18): report_with_caspar_rotation_chain(), s4_cause_check_fails_on_a_hop_misclassified_as_schema(), s4_cause_check_fails_rather_than_skips_when_nothing_rotated(), s4_cause_check_passes_on_a_transport_classified_hop(), s4_ctx_with_rotation(), s4_fails_when_analyze_returned_a_typed_failure(), s4_rotated_check_fails_when_the_chain_is_empty_did_not_rotate(), s4_rotated_check_fails_when_the_hop_lands_on_the_same_lineage() (+10 more)

### Community 32 - "[0.4.0] - 2026-05-16"
Cohesion: 0.29
Nodes (7): [0.4.0] - 2026-05-16, Added, Backward compatibility, Changed, Documentation, Performance, Test count

### Community 33 - "report.rs"
Cohesion: 0.17
Nodes (24): a_certificate_is_refused_over_a_table_that_established_nothing(), a_certificate_that_cannot_name_its_subject_or_its_cost_is_refused(), a_certificate_that_could_not_be_written_never_replaces_the_verdict(), a_certificate_with_no_large_payload_row_says_so_instead_of_omitting_it(), a_corpus_over_the_unverified_threshold_warns_in_the_certificate(), a_dirty_tree_gets_no_certificate_not_a_caveated_one(), a_fixture_repository_is_removed_when_its_guard_drops(), a_run_with_a_failed_assertion_gets_no_certificate_at_all() (+16 more)

### Community 34 - "Changelog"
Cohesion: 0.14
Nodes (14): [0.1.2] - 2026-04-05, [0.3.1] - 2026-04-19, [2.1.0] - 2026-07-27, [2.2.0] - 2026-07-27, [3.2.0] - 2026-08-10, Added, Added, Changed (+6 more)

### Community 35 - "[1.0.0] - 2026-05-24"
Cohesion: 0.40
Nodes (5): [1.0.0] - 2026-05-24, Added, Changed (breaking), Notes, Security

### Community 36 - "main"
Cohesion: 0.15
Nodes (23): build_outcome(), Cli, crate_version(), cycle_run(), git_commit(), main(), payload_size_target(), print_payload_size_answers_about_the_config_it_was_given() (+15 more)

### Community 37 - "RoutingMockProvider"
Cohesion: 0.50
Nodes (3): leaky(), Error, String

### Community 38 - "build_user_prompt"
Cohesion: 0.14
Nodes (24): Sized, build_user_prompt(), fixed_nonce(), Result, Self, Vec, test_build_user_prompt_accepts_empty_content(), test_build_user_prompt_all_5_unicode_separators_positive_neutralization() (+16 more)

### Community 39 - "[1.0.1] - 2026-05-25"
Cohesion: 0.67
Nodes (3): [1.0.1] - 2026-05-25, Fixed, Internal

### Community 40 - "e1.rs"
Cohesion: 0.07
Nodes (61): sha256_hex(), a_typed_crate_failure_is_a_verdict_about_the_crate(), an_unclassified_failure_is_read_as_the_crate_s(), blank_ctx(), e1_contains_exactly_the_scenarios_valid_against_3_2_0(), e1_scenarios(), preflight_error_for_stage(), record() (+53 more)

### Community 41 - "magi-core"
Cohesion: 0.17
Nodes (12): Changelog, Consensus Labels, Contribution, Credentials in a provider URL, Example, Feature Flags, Features, Implementing a Custom Provider (+4 more)

### Community 42 - "build_retry_prompt"
Cohesion: 0.11
Nodes (19): build_retry_prompt(), test_build_retry_prompt_appends_feedback_block_exact_format(), test_build_retry_prompt_does_not_neutralize_midline_tokens(), test_build_retry_prompt_does_not_resanitize_content(), test_build_retry_prompt_feedback_block_after_end_delimiter(), test_build_retry_prompt_includes_seven_keys_list(), test_build_retry_prompt_neutralizes_dash_variant_retry_markers(), test_build_retry_prompt_neutralizes_injected_retry_feedback_marker() (+11 more)

### Community 43 - "e2.rs"
Cohesion: 0.08
Nodes (42): content_failure_detail(), e2_scenarios(), erosion_ctx(), r17_fails_rather_than_passing_vacuously_when_nothing_was_measured(), r17_fails_when_only_some_completions_saw_the_large_payload(), r17_fails_when_the_payload_silently_shrank(), r17_passes_when_the_payload_arrived_large(), r17_reads_the_measured_records_and_ignores_the_silent_ones() (+34 more)

### Community 44 - "orchestrator.rs"
Cohesion: 0.05
Nodes (70): a_hanging_seat_degrades_the_run_within_the_given_ceiling(), a_probe_declaring_another_model_is_named_not_rejected(), a_probe_declaring_its_own_model_is_quiet(), a_probe_that_claims_nothing_is_checked_against_nothing(), a_seat_provider_honours_its_three_rules(), a_threshold_is_unreachable_when_the_smallest_warning_input_is_already_rejected(), a_threshold_that_can_never_fire_is_detected(), a_zero_threshold_warns_always_and_does_not_disable() (+62 more)

### Community 45 - "VerdictExtractionError"
Cohesion: 0.16
Nodes (15): Fn, extract(), locate(), locate_block(), Display, Error, Formatter, Result (+7 more)

### Community 46 - "Quick Start"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 47 - "error.rs"
Cohesion: 0.05
Nodes (32): a_contract_detail_is_capped_at_the_declared_length(), a_contract_detail_under_the_cap_is_untouched(), a_message_of_exactly_the_cap_is_kept_whole(), a_multi_byte_contract_detail_is_cut_without_panicking(), a_short_external_message_survives_untouched(), an_empty_external_message_is_kept_empty(), an_oversized_external_message_is_cut_and_says_so(), empty_completion_remedy() (+24 more)

### Community 48 - "LlmProvider"
Cohesion: 0.06
Nodes (50): create_provider(), main(), parse_mode(), print_usage(), ProviderArgs, read_input(), Arc, Box (+42 more)

### Community 49 - "reporting.rs"
Cohesion: 0.09
Nodes (19): a_fresh_record_measures_nothing_and_says_so(), a_report_from_the_previous_version_deserializes_as_none_not_zero(), a_report_with_no_completions_at_all_carries_no_key(), an_empty_pool_eligibility_is_still_written_to_the_wire(), an_older_report_without_the_field_still_deserializes(), estimate_tokens(), fit_content(), report_with_no_telemetry() (+11 more)

### Community 50 - "Finding"
Cohesion: 0.18
Nodes (12): Finding, Into, String, test_agent_name_btreeset_orders_alphabetically(), test_agent_name_usable_as_btreemap_key(), test_finding_new_constructs_three_field_finding(), test_finding_new_defaults_optional_fields(), test_finding_serde_roundtrip() (+4 more)

### Community 52 - "[0.3.0] - 2026-04-18"
Cohesion: 0.33
Nodes (6): [0.3.0] - 2026-04-18, Added, Changed (breaking), Dependencies, Not included (deferred beyond v0.3.0), Security considerations

### Community 54 - "backoff.rs"
Cohesion: 0.15
Nodes (27): FnMut, fixed(), next_backoff(), parse_retry_after(), RetryAfter, RetryClass, Duration, Option (+19 more)

### Community 55 - ".new"
Cohesion: 0.11
Nodes (23): Beh, build_two_5xx_with_local_fallbacks(), build_two_external_failing_no_fallback(), build_two_failing_with_single_free_fallback(), build_two_network_failing_no_fallback(), MockProbe, ok(), Arc (+15 more)

### Community 62 - "Self"
Cohesion: 0.22
Nodes (5): MockProvider, Cow, Into, Self, trim_cow()

### Community 65 - "prompts/mod.rs"
Cohesion: 0.11
Nodes (14): lookup_prompt(), BTreeMap, Option, Result, String, test_guard_and_parser_agree_on_the_same_block(), test_seat_aware_validation_names_the_agent_and_mode(), test_the_error_is_actionable_on_its_own() (+6 more)

### Community 66 - ".parse"
Cohesion: 0.13
Nodes (17): canonicalizing_does_not_touch_the_query_or_the_fragment(), debug_and_display_are_both_redacted(), ends_with_segment_is_case_sensitive_and_ignores_a_trailing_slash(), join_path_is_idempotent_over_trailing_slash(), join_path_preserves_fragment(), join_path_preserves_query_and_appends_segments(), joining_a_segment_keeps_the_real_credentials(), joining_a_segment_produces_one_separator_not_two() (+9 more)

### Community 67 - "r5.rs"
Cohesion: 0.26
Nodes (18): both_endpoint_scenarios_are_registered(), no_hop(), no_report_skips_instead_of_passing(), r5_scenarios(), recovered_report(), report_with(), String, Vec (+10 more)

### Community 68 - "runner.rs"
Cohesion: 0.09
Nodes (26): SessionFacts, a_timed_out_run_reports_its_cap_verbatim(), build_magi_against(), build_with_absurd_timings(), cannot_test_is_a_skip_with_a_reason_not_a_silent_empty_result(), chat_request(), ProviderKind, Duration (+18 more)

### Community 69 - "LineageRegistry"
Cohesion: 0.09
Nodes (20): AgentSlotGuard, LineageRegistry, reg(), Arc, Drop, Mutex, test_5xx_does_not_count_toward_endpoint_down(), test_digest_collisions_cleared_so_dynamic_r5a_reevaluated() (+12 more)

### Community 70 - "main.rs"
Cohesion: 0.11
Nodes (23): a_genuine_skip_still_reports_exit_two(), a_healthy_run_can_reach_exit_zero(), a_run_that_could_not_start_is_never_a_verdict_about_the_crate(), a_scenario_filtered_out_by_no_backend_is_OUT_OF_SCOPE_not_omitted(), a_scenario_reading_one_run_never_sees_ANOTHER_runs_records(), absent_context(), dispatch_measured(), dispatching_alone_cannot_produce_a_receipt_the_estimate_never_preceded() (+15 more)

### Community 72 - ".redacted"
Cohesion: 0.29
Nodes (5): Method, redacted_hides_all_query_values_and_keeps_names(), redacted_hides_userinfo_and_keeps_host(), redacted_placeholder_is_url_safe_and_not_percent_encoded(), Client

### Community 73 - "6. Modes of Operation"
Cohesion: 0.43
Nodes (6): fail(), fixture_meta(), prod_only(), self_test(), check_redaction.sh script, skeleton()

### Community 74 - "Model Rotation"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 75 - "ProviderError"
Cohesion: 0.15
Nodes (15): AbandonReason, Duration, abandon(), abandon_keeps_the_typed_abandonment_for_run_wide_classes(), abandon_returns_the_original_error_for_a_mage_local_class(), abandon_returns_the_original_error_for_an_unreadable_response_contract(), abandon_without_an_original_error_is_loud_in_debug(), an_external_network_failure_stays_mage_local() (+7 more)

### Community 76 - ".new"
Cohesion: 0.08
Nodes (68): Elapsed, Receiver, a_completion_record_is_built_in_exactly_one_place(), a_completion_that_failed_is_recorded_too(), a_crate_defect_aborts_even_when_the_quorum_is_reachable(), a_crate_defect_outranks_a_simultaneous_endpoint_outage(), a_crate_defect_records_nothing_because_its_report_will_not_exist(), a_degenerate_digest_pool_is_named_once_per_instance() (+60 more)

### Community 77 - "fixtures.rs"
Cohesion: 0.05
Nodes (74): a_file_on_disk_that_nobody_declares_is_an_orphan_too(), a_fixture_whose_hash_changed_is_rejected(), a_manifest_path_that_escapes_the_corpus_is_refused(), a_missing_fixtures_directory_is_reported_not_silently_clean(), a_nested_corpus_is_refused_because_the_walk_does_not_recurse(), a_readable_entry_is_still_crossed_against_the_manifest(), a_sync_that_cannot_finish_leaves_the_tracked_manifest_alone(), a_verified_by_naming_a_live_scenario_is_accepted() (+66 more)

### Community 78 - "Report"
Cohesion: 0.17
Nodes (10): a_harness_fault_reports_the_cycle_run_it_actually_was(), a_row_that_belongs_to_no_run_is_not_attributed_to_one(), a_time_failure_renders_visibly_different_from_an_assertion_failure(), both_renderers_carry_the_scenario_id_its_field_promises(), cycle_run_label(), CycleRun, every_rendered_row_carries_the_run_id_that_fed_it(), render_json_is_parseable_and_carries_the_same_facts_as_the_human_table() (+2 more)

### Community 80 - "redaction_negative.rs"
Cohesion: 0.37
Nodes (12): a_query_secret_never_appears_in_debug_but_its_name_does(), assert_clean(), both_forms_in_one_url_are_both_redacted_in_debug(), credentials_never_appear_in_a_transport_error(), credentials_never_reach_the_serialized_report(), ollama_credentials_never_appear_in_a_completion_error(), ollama_credentials_never_appear_in_a_probe_error(), ollama_credentials_never_reach_the_serialized_report() (+4 more)

### Community 81 - "[3.1.0] - 2026-07-31"
Cohesion: 0.40
Nodes (5): [3.1.0] - 2026-07-31, Changed, Documented, Fixed, Security

### Community 82 - "5. Data Schema and Consensus Protocol"
Cohesion: 0.40
Nodes (3): redacted(), String, X

### Community 83 - "HostedModel"
Cohesion: 0.23
Nodes (6): HostedModel, Option, ProviderError, Result, String, SidecarProbe

### Community 84 - "RetryConfig"
Cohesion: 0.10
Nodes (9): AlwaysFails, fast_retry_cfg(), RetryConfig, RetryProvider, Arc, AtomicUsize, Default, Duration (+1 more)

### Community 85 - "complete_against_stub"
Cohesion: 0.24
Nodes (15): big_prompt(), captured_envelope_with_stop_reason(), a_child_that_dies_mid_write_reports_the_process_not_the_broken_pipe(), a_delivered_prompt_is_never_reported_as_truncated(), a_failed_prompt_write_is_never_ok_even_on_exit_zero(), a_local_cli_failure_stays_process_on_a_clean_exit(), a_local_cli_failure_stays_process_when_the_child_also_died(), a_nonzero_exit_code_does_not_discard_the_envelope() (+7 more)

### Community 86 - "Option"
Cohesion: 0.14
Nodes (28): a_candidate_with_a_measured_window_and_no_digest_still_rotates(), a_pool_exhausted_by_collisions_alone_is_recorded_as_degenerate(), a_pool_exhausted_by_mixed_causes_is_not_degenerate(), a_recorded_degenerate_pool_survives_a_later_healthy_pass(), a_single_colliding_candidate_is_not_a_degenerate_pool(), ae(), AgentRotationState, cap() (+20 more)

### Community 87 - ".forward"
Cohesion: 0.22
Nodes (13): B, Bytes, HeaderMap, ProxyBody, a_method_the_proxy_cannot_parse_is_not_forwarded_as_a_post(), boxed(), build_failed(), fixed() (+5 more)

### Community 88 - ".write_certificate_in"
Cohesion: 0.17
Nodes (13): a_clean_tree_gets_the_certificate_written_at_cert_path(), a_verdict_about_the_crate_outranks_our_own_failure_to_certify(), CertificateFacts, iso_date_utc(), large_payload_priority(), Option, Path, Result (+5 more)

### Community 89 - "RunContext"
Cohesion: 0.24
Nodes (20): assert_that(), Assertion, RunContext, analyze_produced_a_report(), the_crate_correctly_reporting_a_dead_backend_is_not_a_verdict(), r17_the_prompt_is_large_in_tokens(), Vec, s10_the_large_payload_costs_no_seat() (+12 more)

### Community 90 - "RunId"
Cohesion: 0.24
Nodes (8): RunId, run_with(), BackendNeed, probe_is_in_scope(), probe_window(), Scenario, Source, f_scenarios()

### Community 91 - "preflight.rs"
Cohesion: 0.08
Nodes (42): a_backend_holding_every_seat_model_passes_the_check(), a_backend_listing_the_whole_trio_gets_past_the_backend_step(), a_config_that_is_not_a_full_trio_is_rejected_as_a_config_fault(), a_config_with_no_fallbacks_is_a_config_fault_not_a_crate_verdict(), a_config_with_one_fallback_is_a_config_fault_not_a_crate_verdict(), a_config_without_seats_is_rejected_by_name_in_the_config_step(), a_fallback_sharing_a_seats_lineage_is_rejected(), a_fallback_sharing_a_seats_model_is_rejected() (+34 more)

### Community 92 - ".read_verdict_body"
Cohesion: 0.29
Nodes (5): body_cap(), Formatter, ProviderError, Result, send_composes_a_redacted_error_on_connection_failure()

### Community 93 - ".new"
Cohesion: 0.29
Nodes (7): NativeMessage, NativeOptions, NativeRequest, Self, Vec, the_default_control_omits_think_entirely(), the_native_request_serialises_the_four_fields_the_endpoint_needs()

### Community 94 - "ConsensusResult"
Cohesion: 0.25
Nodes (10): Condition, ConsensusResult, DedupFinding, Dissent, Option, String, Vec, test_dissent_line_contains_summary_not_reasoning() (+2 more)

### Community 95 - "ValidationLimits"
Cohesion: 0.33
Nodes (6): Default, Self, test_title_length_checked_after_strip_zero_width(), test_validate_mut_atomic_no_partial_mutation_on_error(), test_validator_with_limits_uses_custom_limits(), ValidationLimits

### Community 96 - ".set_injection"
Cohesion: 0.17
Nodes (11): IntoIterator, a_dropped_connection_reaches_the_client_as_a_network_error(), a_poisoned_injection_lock_keeps_injecting_and_marks_degraded(), a_response_the_proxy_cannot_build_is_an_error_not_a_fabricated_success(), a_spent_drop_budget_forwards_the_next_request(), an_always_drop_budget_cuts_every_attempt(), an_injection_does_not_also_break_that_models_capability_probe(), an_unreadable_request_is_not_forwarded_as_an_empty_one() (+3 more)

### Community 97 - "MagiError"
Cohesion: 0.24
Nodes (10): is_endpoint_down(), MagiError, Error, From, Self, clean_title(), Result, String (+2 more)

### Community 98 - "OllamaProvider"
Cohesion: 0.27
Nodes (6): OllamaProvider, Client, Option, ProviderError, Result, String

### Community 99 - ".new"
Cohesion: 0.13
Nodes (19): ClaudeCliProvider, Into, Self, Vec, test_build_args_includes_required_cli_flags(), test_new_claude_prefix_passes_through(), test_new_haiku_maps_to_claude_haiku_model(), test_new_invalid_model_returns_auth_error() (+11 more)

### Community 100 - ".cmp"
Cohesion: 0.50
Nodes (3): Ordering, Option, Self

### Community 101 - "provider.rs"
Cohesion: 0.05
Nodes (44): Range, a_budget_inside_the_window_says_nothing(), a_budget_under_the_floor_is_still_reported(), a_limited_class_gets_two_attempts_not_four(), a_limited_count_above_the_general_one_is_reported(), a_refused_redirect_is_mage_local_and_never_retried(), a_single_honoured_wait_that_eats_the_whole_budget_is_flagged(), a_wait_exactly_equal_to_the_budget_is_flagged_too() (+36 more)

### Community 103 - "Debug"
Cohesion: 0.40
Nodes (3): redacted(), String, X

### Community 104 - "stub_cli.rs"
Cohesion: 0.36
Nodes (7): R, load_config(), main(), read_exactly(), ExitCode, Result, Value

### Community 107 - "outcome.rs"
Cohesion: 0.07
Nodes (21): a_hookless_unwind_cannot_inherit_an_earlier_panics_attribution(), a_panic_inside_a_run_is_caught_and_attributed_instead_of_killing_the_harness(), a_run_that_finishes_is_returned_untouched(), classify_panic(), crate_under_test_packages(), every_harness_only_dep_is_ABSENT_from_the_crate_under_tests_own_graph(), exit_code(), graph_query_target_dir() (+13 more)

### Community 108 - "FinishReason"
Cohesion: 0.10
Nodes (14): MockProvider, Ok, S, Serialize, finish_reason_keeps_an_unknown_wire_value_instead_of_dropping_it(), finish_reason_other_is_capped_at_64_chars_on_a_char_boundary(), FinishReason, http() (+6 more)

### Community 109 - "ProviderProbe"
Cohesion: 0.11
Nodes (17): Barrier, an_absurd_preflight_budget_does_not_panic(), FailingProbe, InstrumentedProbe, MockProbe, OverlapProbe, ProviderProbe, AtomicUsize (+9 more)

### Community 110 - "[1.1.0] - 2026-05-25"
Cohesion: 0.50
Nodes (3): ProviderUrl, redacted(), String

### Community 112 - "Default"
Cohesion: 0.50
Nodes (3): ProviderUrl, redacted(), String

### Community 113 - "1. Origin: The MAGI Supercomputers from Evangelion"
Cohesion: 0.33
Nodes (6): 1.1 Context in the Series, 1.2 The Three Units, 1.3 Decision Mechanism, 1.4 The Philosophical Principle, 1.5 Why Structured Disagreement Works, 1. Origin: The MAGI Supercomputers from Evangelion

### Community 114 - "Send"
Cohesion: 0.50
Nodes (4): build(), other(), ProviderError, Self

### Community 115 - "Sync"
Cohesion: 0.29
Nodes (7): Basic Usage, Cost Control with Complexity Gate, Custom System Prompts, Quick Start, The Output Contract, Using the Built-in Claude CLI Provider, With Builder

### Community 116 - "openai_compat.rs"
Cohesion: 0.07
Nodes (50): a_body_without_choices_is_a_contract_failure_named_for_what_is_missing(), a_control_that_cannot_be_honoured_is_declared_even_when_reasoning_came_back(), a_filtered_reply_is_not_answered_with_raise_your_budget(), absent_usage_leaves_the_counters_unmeasured_rather_than_zero(), an_empty_completion_names_the_budget_that_cut_it(), cfg(), content_with_text_is_returned(), Dialect (+42 more)

### Community 117 - "Into"
Cohesion: 0.38
Nodes (5): build(), ProviderError, Error, From, Self

### Community 118 - "Option"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 121 - "check_readme_version.sh"
Cohesion: 0.46
Nodes (7): _case(), check(), manifest_version(), readme_mentions(), readme_requirements(), self_test(), check_readme_version.sh script

### Community 122 - "4. Library Architecture"
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

### Community 128 - "7. Design Philosophy"
Cohesion: 0.40
Nodes (5): 7.1 Dissent is a Feature, 7.2 Adversarial by Design, 7.3 Proportionality, 7.4 LLM-Agnostic Design, 7. Design Philosophy

### Community 129 - "MAGI System — Complete Technical Documentation"
Cohesion: 0.29
Nodes (4): 8. Evangelion Correspondence Table, 9. Relationship to the MAGI Python Plugin, MAGI System — Complete Technical Documentation, Multi-Perspective Analysis Library for Rust

### Community 132 - "Severity"
Cohesion: 0.24
Nodes (6): Ord, PartialOrd, Display, Formatter, Result, Severity

### Community 133 - "RunResult"
Cohesion: 0.21
Nodes (14): RunOutcome, attempts_for(), classify_error(), ErosionProbe, ErrorClass, injected_agent(), render_error(), Option (+6 more)

### Community 134 - "testkit.rs"
Cohesion: 0.06
Nodes (45): AsRef, Deref, a_backend_that_does_not_hold_the_probe_model_is_INCONCLUSIVE_not_clear(), a_cold_model_passes_on_the_second_probe_attempt(), an_endpoint_slow_every_time_still_reports_cannot_test(), the_backend_step_and_the_probe_step_ask_different_questions(), the_contention_probe_sends_a_real_completion_not_a_manifest_listing(), the_window_bound_has_one_entry_per_request_the_preflight_makes() (+37 more)

### Community 137 - "String"
Cohesion: 0.50
Nodes (3): ProviderUrl, redacted(), String

### Community 138 - "Completion"
Cohesion: 0.09
Nodes (16): describe(), main(), MinimalProvider, MyBackend, ProviderError, Result, String, AlwaysFailsExternally (+8 more)

### Community 144 - "SpyProxy"
Cohesion: 0.14
Nodes (14): AtomicBool, Incoming, a_query_does_not_change_which_endpoint_a_record_names(), Injected, model_of(), names_model(), RequestRecord, Client (+6 more)

### Community 152 - "balthasar_prompt"
Cohesion: 0.54
Nodes (8): balthasar_prompt(), caspar_prompt(), embedded_prompt_for(), melchior_prompt(), test_echo_canary_values_are_present_in_every_shipped_prompt(), test_prompts_match_pinned_reference_sha256(), test_shipped_prompts_satisfy_the_verdict_marker_contract(), the_embedded_prompts_match_the_sizes_the_rustdoc_publishes()

### Community 164 - "finding_id.rs"
Cohesion: 0.21
Nodes (13): de_category(), de_opt_file(), de_opt_line(), generate_finding_id(), normalize_category(), normalize_path(), D, Error (+5 more)

### Community 165 - "[4.0.0] - 2026-08-24"
Cohesion: 0.25
Nodes (8): [4.0.0] - 2026-08-24, Added, Changed, Fixed, Migration, One story, not two: the completion budget and the time budget, Removed, The defect this release exists for

### Community 166 - "check_no_deprecated_ctors.sh"
Cohesion: 0.36
Nodes (5): fail(), resolve_self(), scan(), self_test(), check_no_deprecated_ctors.sh script

### Community 167 - ".skip"
Cohesion: 0.29
Nodes (4): an_assertion_that_could_not_be_tested_carries_its_reason(), Into, Self, RunContext<'static>

### Community 168 - "CompletionRecord"
Cohesion: 0.38
Nodes (5): CompletionRecord, from_telemetry_copies_every_measurement_across(), from_telemetry_invents_nothing_when_nothing_was_measured(), Self, the_unsupported_declaration_survives_the_conversion()

### Community 173 - "[3.0.2] - 2026-07-30"
Cohesion: 0.67
Nodes (3): [3.0.2] - 2026-07-30, Changed, Fixed

### Community 174 - "check_fixture_redaction.py"
Cohesion: 0.27
Nodes (11): check(), git_knows(), key_paths(), main(), Return the findings for one fixture file, as ``(rule, detail)`` pairs., Whether ``root`` is registered in HEAD: True, False, or None if git failed.…, Run the guard. Returns ``(exit_code, rule, lines)``. ``exit_code`` is 0 for…, Yield the qualified path of every key reachable from ``node``. Arrays are… (+3 more)

### Community 176 - "[0.6.0] - 2026-05-21"
Cohesion: 0.33
Nodes (6): [0.6.0] - 2026-05-21, Backward compatibility, Changed, Pre-merge gates (CLAUDE.local.md §6), Security, Test count

### Community 177 - "Vec"
Cohesion: 0.10
Nodes (27): AbortHandle, JoinCounter, a_recovered_run_is_not_aborted(), a_truly_dead_endpoint_still_aborts_fast(), AbortGuard, all_cases(), all_errors(), an_unreachable_quorum_without_the_latch_is_not_endpoint_down() (+19 more)

### Community 179 - "Injection"
Cohesion: 0.18
Nodes (10): Item, Iterator, a_redirect_is_relayed_verbatim_and_never_followed(), claims_drop(), Injection, Arc, AtomicU32, AtomicUsize (+2 more)

### Community 180 - "infallible_to_box"
Cohesion: 0.18
Nodes (10): Infallible, ConnectionDropped, infallible_to_box(), Box, Display, Error, Formatter, Result (+2 more)

### Community 183 - "provider_url.rs"
Cohesion: 0.50
Nodes (3): leaky(), Error, String

### Community 188 - ".new_checked"
Cohesion: 0.40
Nodes (3): leaky(), Error, String

### Community 191 - "check_header_sync.py"
Cohesion: 0.23
Nodes (14): changed_since(), check(), _commit(), git(), in_scope(), last_tag(), main(), parse_version() (+6 more)

### Community 193 - "mock_server.rs"
Cohesion: 0.25
Nodes (12): CapturedRequest, Arc, JoinHandle, Mutex, Option, String, Value, Vec (+4 more)

### Community 194 - "[3.0.0] - 2026-07-30"
Cohesion: 0.50
Nodes (4): [3.0.0] - 2026-07-30, Added, BREAKING, Changed

### Community 195 - "2. Translation to the Software Engineering Domain"
Cohesion: 0.50
Nodes (4): 2.1 Conceptual Mapping, 2.2 Why Three Perspectives and Not Two or Five, 2.3 Addressing Cognitive Biases, 2. Translation to the Software Engineering Domain

### Community 196 - "config.rs"
Cohesion: 0.06
Nodes (55): Deserialize, FnOnce, a_budget_beyond_the_ceiling_is_rejected_and_the_field_is_named(), a_correctly_spelled_override_still_loads(), a_file_that_is_still_illegal_once_every_override_is_applied_is_refused(), a_file_that_omits_seats_and_fallbacks_gets_the_built_in_ones(), a_file_value_an_override_rescues_is_judged_on_the_final_set_not_the_file_alone(), a_set_of_overrides_that_is_still_illegal_at_the_end_is_refused() (+47 more)

### Community 197 - "ProviderResponse"
Cohesion: 0.16
Nodes (9): X, ProviderError, Result, X, ProviderResponse, push_within_cap(), Option, Response (+1 more)

### Community 198 - "check_warn_once.sh"
Cohesion: 0.36
Nodes (5): fail(), resolve_self(), scan(), self_test(), check_warn_once.sh script

### Community 199 - "3. The Three Agents in Detail"
Cohesion: 0.50
Nodes (4): 3.1 Melchior — The Scientist, 3.2 Balthasar — The Pragmatist, 3.3 Caspar — The Critic, 3. The Three Agents in Detail

### Community 200 - "AssertionRow"
Cohesion: 0.27
Nodes (9): an_observation_renders_its_marker_and_its_measurement(), AssertionRow, both_renderers_name_the_cap_that_was_exceeded_rather_than_an_overrun(), format_row(), row_to_json(), Duration, Value, Vec (+1 more)

### Community 204 - "git.rs"
Cohesion: 0.33
Nodes (4): Path, Result, String, status_porcelain()

### Community 209 - "6. Modes of Operation"
Cohesion: 0.50
Nodes (4): 6.1 Code Review, 6.2 Design, 6.3 Analysis, 6. Modes of Operation

### Community 210 - "claude_cli.rs"
Cohesion: 0.09
Nodes (39): a_parse_failure_does_not_carry_the_whole_offending_value(), a_prefix_longer_than_the_cap_is_itself_capped(), an_envelope_without_a_stop_reason_leaves_finish_none(), an_unknown_stop_reason_is_not_mapped_to_stop(), ApiErrorStatus, cap_to_error_budget(), classify_child_failure(), classify_envelope_error() (+31 more)

### Community 211 - "common/mod.rs"
Cohesion: 0.06
Nodes (50): Child, TcpStream, a_chunked_probe_body_degrades_from_the_streaming_branch(), a_chunked_probe_body_under_the_cap_is_read_and_parsed(), a_probe_body_over_the_cap_degrades_instead_of_failing(), a_success_body_at_the_cap_is_read_whole(), a_success_body_over_the_cap_fails_rather_than_arriving_truncated(), a_verdict_body_that_is_not_utf8_fails_instead_of_being_mangled() (+42 more)

### Community 212 - "Model Rotation"
Cohesion: 0.50
Nodes (4): Declaring fallbacks, Model Rotation, Ollama probe (feature `ollama`), What a rotation looks like

### Community 213 - "[1.1.0] - 2026-05-25"
Cohesion: 0.67
Nodes (3): [1.1.0] - 2026-05-25, Added, Notes

### Community 214 - "ReportFormatter"
Cohesion: 0.14
Nodes (15): ReportConfig, ReportError, ReportFormatter, Default, Display, Formatter, Result, test_format_init_banner_shows_mode_model_timeout() (+7 more)

### Community 216 - "[1.1.1] - 2026-07-17"
Cohesion: 0.67
Nodes (3): [1.1.1] - 2026-07-17, Changed, Fixed

### Community 217 - "claude.rs"
Cohesion: 0.06
Nodes (54): a_non_text_block_with_a_normal_ending_is_not_reported_as_a_budget_cut(), a_non_text_contract_failure_names_the_shape_it_saw(), a_present_but_empty_thinking_payload_is_a_measured_zero_like_the_other_wire(), a_redacted_block_beside_a_readable_one_is_not_a_complete_measurement(), a_verdict_in_a_later_text_block_survives_an_empty_or_null_earlier_one(), an_anthropic_budget_cut_with_no_text_block_names_the_budget_not_a_broken_contract(), an_empty_text_block_beside_a_tool_use_is_not_a_budget_cut(), an_empty_text_block_is_an_empty_completion_like_the_other_wire() (+46 more)

### Community 218 - "Architecture"
Cohesion: 0.67
Nodes (3): Architecture, Module Dependency Graph, Prompt Injection Defense

### Community 220 - "magi-smoke — the smoke harness"
Cohesion: 0.10
Nodes (19): 10. `cargo audit` for the harness, 11.1 The captured measurements this harness is calibrated against, 11. The reproduction that justifies the harness, 1. Three exit codes, and the difference between two of them, 2. Two dependency modes, 3. What the proxy does, and why its red is never the crate's, 4. The contention probe, and its declared scope, 5. Where it fits in the cycle (+11 more)

### Community 221 - "e.rs"
Cohesion: 0.33
Nodes (8): e_scenarios(), no_report_skips_instead_of_passing(), report_with(), Vec, s_e2_fails_when_nothing_was_ruled_out(), s_e2_why_a_candidate_was_not_eligible(), s_e3_all_failing_conditions(), s_e3_fails_when_only_one_condition_is_reported()

### Community 222 - "ProviderError"
Cohesion: 0.40
Nodes (4): build(), ProviderError, From, Self

### Community 224 - "compose_transport_message"
Cohesion: 0.50
Nodes (3): Error, String, spawn_failure()

### Community 229 - "check_changelog_disclosures.py"
Cohesion: 0.29
Nodes (11): check(), _full_section(), main(), present(), The `## [version]` section, up to the next `## [`. ``None`` when absent., Whether ``token`` appears in ``text`` as a whole token, not a substring. `in`…, Split a version section into ``{heading: body}`` by its `###` headings. The…, Run the floor. Returns ``(exit_code, lines)``. (+3 more)

### Community 233 - "FixedRng"
Cohesion: 0.40
Nodes (4): FixedRng, test_fastrand_source_returns_distinct_values_across_calls(), test_fixed_rng_panics_when_exhausted(), VecDeque

### Community 235 - "[2.0.0] - 2026-07-25"
Cohesion: 0.50
Nodes (4): [2.0.0] - 2026-07-25, Added, BREAKING, Changed

### Community 237 - "normalize_newlines"
Cohesion: 0.20
Nodes (12): neutralize_headers(), normalize_newlines(), Cow, String, sanitize_error_for_retry_feedback(), strip_invisibles(), test_neutralize_headers_not_bypassed_by_mongolian_vowel_separator(), test_neutralize_headers_preserves_unmatched_lines_borrowed() (+4 more)

### Community 238 - "[3.0.1] - 2026-07-30"
Cohesion: 0.50
Nodes (4): [3.0.1] - 2026-07-30, Changed, Fixed, Notes

### Community 242 - "EventLog"
Cohesion: 0.13
Nodes (11): Attributes, Event, Field, Id, Metadata, Record, EventLog, FieldWriter (+3 more)

### Community 245 - "pattern8bconst_bad.rs"
Cohesion: 0.50
Nodes (4): build(), ProviderError, Self, sneak()

### Community 251 - "FallbackPool"
Cohesion: 0.18
Nodes (12): FallbackCandidate, FallbackPool, FallbackPoolBuilder, P, test_duplicate_lineage_warns_but_builds(), test_empty_pool_is_valid_and_keeps_max_rotations(), test_push_probing_stores_both_views(), test_push_with_probe_accepts_a_provider_that_cannot_probe() (+4 more)

### Community 260 - "AgentOutput"
Cohesion: 0.39
Nodes (3): AgentOutput, Vec, Verdict

### Community 261 - "retry_template"
Cohesion: 0.25
Nodes (8): a_budget_cut_before_the_opening_marker_is_also_attributed_to_the_budget(), a_cut_that_was_not_the_budget_also_keeps_the_old_wording(), a_known_budget_cut_stops_asking_the_model_not_to_stop(), an_unterminated_of_unknown_cause_keeps_the_old_wording(), missing_markers_without_a_budget_cut_keeps_its_own_wording(), retry_template(), Option, the_malformed_object_cause_reaches_the_schema_template()

### Community 262 - "rotation.rs"
Cohesion: 0.10
Nodes (28): a_caller_holding_real_seat_state_produces_the_three_mid_run_causes(), a_seat_with_no_candidates_gets_an_empty_entry_not_a_missing_one(), a_spent_rotation_budget_is_reported_rather_than_read_as_eligible(), an_unmeasured_candidate_under_a_strict_guard_says_so(), cand(), CandidateEligibility, extract_item(), IneligibilityCause (+20 more)

### Community 271 - "pattern8bnospace_bad.rs"
Cohesion: 0.50
Nodes (4): build(), ProviderError, Self, sneak()

### Community 272 - "pattern8bnospace_good.rs"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 273 - "ProviderRequest"
Cohesion: 0.22
Nodes (7): X, X, ProviderRequest, RequestBuilder, Self, String, T

### Community 274 - "pattern8bcrate_good.rs"
Cohesion: 0.50
Nodes (4): bound_it(), build(), ProviderError, Self

### Community 276 - "check_prose_artifacts.sh"
Cohesion: 0.57
Nodes (6): assert_no_whitespace_paths(), assert_roots_exist(), canonical_root(), scan(), scan_targets(), check_prose_artifacts.sh script

### Community 278 - "compose_transport_message"
Cohesion: 0.09
Nodes (18): f(), f(), f(), f(), P, Self, cause_chain(), cause_chain_skips_the_top_level_error() (+10 more)

### Community 279 - "[4.1.0] - 2026-09-14"
Cohesion: 0.50
Nodes (4): [4.1.0] - 2026-09-14, Added, Changed, Deprecated

### Community 290 - "EC fixtures — captured responses from Ollama, both wire formats"
Cohesion: 0.33
Nodes (5): EC fixtures — captured responses from Ollama, both wire formats, Files, The cold-start experiment: **GO**, The two `think: false` captures, and what they are evidence FOR, Two findings the experiment was not looking for

### Community 293 - "pattern7builderalias_bad.rs"
Cohesion: 0.67
Nodes (3): bare(), configured(), Client

### Community 294 - "pattern7builderalias_good.rs"
Cohesion: 0.67
Nodes (3): also_configured(), configured(), Client

### Community 301 - "weakened.rs"
Cohesion: 0.16
Nodes (10): a_source_file_that_cannot_be_decoded_is_reported_too(), an_unreadable_path_is_reported_and_never_skipped(), every_mark_outside_this_guards_own_fixtures_is_well_formed(), Path, Result, String, Vec, scan() (+2 more)

### Community 307 - "s15_degradation_is_honest"
Cohesion: 0.16
Nodes (27): a_fired_injection(), answered_probes(), render_combinations(), report_from(), Into, Option, String, Vec (+19 more)

### Community 339 - "AgentName"
Cohesion: 0.11
Nodes (34): DispatchOutcome, JoinError, a_mage_local_rotation_detail_says_what_happened_and_not_its_scope(), collect_probe_targets(), completion_with_a_valid_verdict(), crate_defect_error(), crate_defect_of(), DeclaringProbe (+26 more)

### Community 353 - "check_pending.sh"
Cohesion: 0.60
Nodes (5): resolve_self(), run_crossing(), self_test(), check_pending.sh script, six_crossings_test()

### Community 387 - "Migrating from 3.2.0 to 4.0.0"
Cohesion: 0.12
Nodes (16): 1. `complete()` returns `Completion`, not `String`, 2. `RotationKind` gains four variants and becomes `#[non_exhaustive]`, 3. `ProviderError::Http { status: 0 }` no longer exists, 4. `OllamaProvider` completes on `/api/chat`, 5. `CompletionConfig::max_tokens` defaults to `16_384`, 6. `MagiReport` gains `completions`, 7. The vendor termination vocabularies are fully translated, 8. `ClaudeProvider::parse_response` is gone (+8 more)

## Knowledge Gaps
- **227 isolated node(s):** `magi-core`, `check_packaged_consumer.sh script`, `check_r0.sh script`, `ProviderError`, `ProviderError` (+222 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **43 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `AgentName` connect `AgentName` to `Lineage`, `consensus.rs`, `Severity`, `RunResult`, `.new`, `rotation.rs`, `schema.rs`, `MagiBuilder`, `AgentOutput`, `test_support.rs`, `MagiReport`, `balthasar_prompt`, `orchestrator.rs`, `LlmProvider`, `.new`, `prompts/mod.rs`, `config.rs`, `runner.rs`, `LineageRegistry`, `.new`, `ReportFormatter`, `Option`, `RunContext`, `ConsensusResult`, `MagiError`, `.cmp`?**
  _High betweenness centrality (0.071) - this node is a cross-community bridge._
- **Why does `LlmProvider` connect `LlmProvider` to `rotation_integration.rs`, `rotation.rs`, `MagiBuilder`, `Completion`, `.new`, `Self`, `test_support.rs`, `orchestrator.rs`, `Vec`, `.new`, `Self`, `ProviderError`, `.new`, `HostedModel`, `AgentName`, `RetryConfig`, `claude.rs`, `OllamaProvider`, `.new`, `provider.rs`, `FinishReason`, `ProviderProbe`, `openai_compat.rs`, `FallbackPool`?**
  _High betweenness centrality (0.037) - this node is a cross-community bridge._
- **Why does `MagiReport` connect `MagiReport` to `AgentOutput`, `RunResult`, `rotation.rs`, `test_support.rs`, `s4_rotation_and_its_cause`, `e1.rs`, `CompletionRecord`, `e2.rs`, `orchestrator.rs`, `Vec`, `reporting.rs`, `s15_degradation_is_honest`, `r5.rs`, `runner.rs`, `.new`, `AgentName`, `common/mod.rs`, `RunContext`, `e.rs`, `ConsensusResult`?**
  _High betweenness centrality (0.028) - this node is a cross-community bridge._
- **What connects `magi-core`, `check_packaged_consumer.sh script`, `check_r0.sh script` to the rest of the system?**
  _227 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `rotation_integration.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.11956521739130435 - nodes in this community are weakly interconnected._
- **Should `Lineage` be split into smaller, more focused modules?**
  _Cohesion score 0.13333333333333333 - nodes in this community are weakly interconnected._
- **Should `consensus.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.09024309024309024 - nodes in this community are weakly interconnected._