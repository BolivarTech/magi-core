# Graph Report - MAGI-Core  (2026-08-23)

## Corpus Check
- 188 files · ~365,015 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 3669 nodes · 8150 edges · 447 communities (204 shown, 243 thin omitted)
- Extraction: 99% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 117 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `32c3f89d`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- test_support.rs
- fixtures.rs
- consensus.rs
- validate.rs
- .build
- String
- LlmProvider
- schema.rs
- ollama_wire.rs
- orchestrator.rs
- user_prompt.rs
- claude_cli.rs
- consensus
- Balthasar — The Pragmatist
- MAGI System Technical Documentation
- Caspar — The Critic
- Melchior — The Scientist
- runner.rs
- s4_rotation_and_its_cause
- Config
- MAGI System Technical Documentation
- ollama.rs
- Self
- MagiBuilder
- proxy.rs
- reporting.rs
- magi_report_v0_3_1.json fixture
- make_output
- [0.5.0] - 2026-05-16
- verdict_markers.rs
- [0.2.0] - 2026-04-18
- [3.0.1] - 2026-07-30
- [0.4.0] - 2026-05-16
- report.rs
- [0.6.0] - 2026-05-21
- [1.0.0] - 2026-05-24
- provider.rs
- RoutingMockProvider
- build_user_prompt
- Changelog
- s15_degradation_is_honest
- magi-core
- normalize_newlines
- e2.rs
- .new
- .new
- Quick Start
- ProviderError
- Lineage
- rotation_integration.rs
- FixedRng
- [0.3.0] - 2026-04-18
- backoff.rs
- bool
- Voting rules + confidence formula
- Evangelion MAGI origin (Naoko Akagi)
- MAGI System Technical Documentation
- Structured disagreement rationale
- Why three perspectives (not 2 or 5)
- basic_analysis example
- prompts/mod.rs
- AgentName
- ProviderError
- mock_server.rs
- SlowFailingProvider
- main.rs
- [3.0.0] - 2026-07-30
- 6. Modes of Operation
- Model Rotation
- rotation.rs
- basic_analysis.rs
- int
- int
- str
- .default
- [3.1.0] - 2026-07-31
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
- RunId
- Formatter
- ProviderError
- outcome.rs
- Self
- run_preflight
- [1.1.0] - 2026-05-25
- BTreeMap
- Default
- Mutex
- Send
- Sync
- openai_compat.rs
- Into
- Option
- ProviderUrl
- Box
- BTreeMap
- Drop
- F
- HashMap
- P
- ProviderError
- HostedModel
- Debug
- Duration
- s2b_the_proxy_is_transparent
- Instant
- testkit.rs
- String
- Vec
- check_calibration.sh
- redacted
- .leak
- redacted
- redacted
- redacted
- redacted
- .leak
- redacted
- redacted
- .leak
- redacted
- finding_id.rs
- ProviderError
- [4.0.0] - 2026-08-23
- Result
- [1.0.1] - 2026-05-25
- e.rs
- Box
- [1.1.0] - 2026-05-25
- sync-fixtures.sh
- Drop
- F
- common/mod.rs
- CompletionConfig
- serve_once
- dedup_key
- preflight.rs
- provider_url.rs
- AgentName
- pattern0nonterminal_bad.rs
- pattern0nonterminal_good.rs
- .new_checked
- RunContext
- MagiReport
- RunContext
- retry_template
- .format_findings
- config.rs
- .probe
- Self
- ConsensusEngine
- [2.2.0] - 2026-07-27
- cross_milestone.rs
- smoke-certificate.md
- .new
- git.rs
- MockProbe
- CompletionRecord
- body_bounds.rs
- pool
- .fmt
- Report
- Agent
- claude.rs
- AgentFactory
- ExitStatus
- magi-smoke — the smoke harness
- ScenarioState
- ProviderError
- ExitCode
- compose_transport_message
- prelude.rs
- magi-core
- I
- Instant
- AgentOutput
- [2.0.0] - 2026-07-25
- Output
- build_retry_prompt
- ReportConfig
- Path
- PathBuf
- AtomicBool
- BudgetBearing
- AtomicUsize
- [1.1.0] - 2026-05-25
- Box
- .format_init_banner
- EventLog
- magi-smoke
- Self
- pattern8bconst_bad.rs
- Client
- FallbackPool
- ScenarioState
- log_failure
- Completion
- BTreeMap
- ProviderError
- BTreeSet
- Default
- Duration
- .send
- described
- ProviderUrl
- [3.0.2] - 2026-07-30
- ProviderUrl
- FinishReason
- PreflightError
- pattern8bnospace_bad.rs
- pattern8bnospace_good.rs
- ProviderResponse
- pattern8bcrate_good.rs
- Value
- Completion
- CompletionRecord
- AgentRotation
- Completion
- CompletionConfig
- .new
- compose_transport_message
- e1.rs
- CompletionTelemetry
- Config
- Cow
- LlmProvider
- ProviderError
- String
- EC fixtures — captured responses from Ollama, both wire formats
- Into
- resolve_claude_alias
- pattern7builderalias_bad.rs
- pattern7builderalias_good.rs
- pattern7default_bad.rs
- pattern7default_good.rs
- Debug
- AgentRotation
- weakened.rs
- CrateDefectRecord
- ReasoningControl
- Display
- Drop
- AgentRotation
- MagiReport
- ProviderRequest
- ExternalErrorKind
- provider_url.rs
- RequestBuilder
- ExtractionFailure
- F
- ProviderError
- Lineage
- Fallback
- FallbackPool
- MagiReport
- FinishReason
- FixtureSummary
- RequestRecord
- Formatter
- .send
- From
- HashMap
- Completion
- LineageRegistry
- CompletionConfig
- ProviderProbe
- Injection
- InputSize
- Scenario
- JoinHandle
- Magi
- Lineage
- LlmProvider
- Magi
- Assertion
- ModelCapability
- MagiError
- Url
- RunId
- Mode
- MagiError
- LlmProvider
- PreflightError
- check_pending.sh
- Mutex
- Option
- build_with
- Lineage
- RotationConfig
- captured_warnings
- ProviderError
- P
- Payload
- AtomicU32
- PayloadError
- Completion
- CompletionConfig
- ProviderError
- ProviderProbe
- ReasoningControl
- MagiError
- CompletionTelemetry
- Lineage
- FinishReason
- ProviderError
- ReasoningState
- Completion
- Completion
- ReportConfig
- Config
- AgentOutput
- CompletionConfig
- ProviderProbe
- RunId
- ReasoningControl
- ReportFormatter
- Migrating from 3.2.0 to 4.0.0
- Response
- Result
- RetryClass
- ProviderError
- RngLike
- RunOutcome
- FinishReason
- AtomicU32
- Announcement
- Send
- SpyProxy
- CompletionTelemetry
- FinishReason
- LlmProvider
- ProviderError
- SpyProxy
- MagiError
- Assertion
- MagiReport
- e1_scenarios
- Assertion
- MagiReport
- RequestRecord
- ReasoningControl
- Completion
- CompletionConfig
- ProviderError
- RunContext
- Announcement
- Scenario
- SpyProxy
- FinishReason
- LlmProvider
- ReasoningControl
- TcpListener
- Completion
- CompletionConfig
- LlmProvider
- ProviderError
- ProviderError
- ProviderError
- MockProvider
- ProviderError
- CompletionConfig
- LlmProvider
- ProviderProbe
- ProviderUrl
- ProviderUrl
- .skip
- [1.1.1] - 2026-07-17
- LlmProvider
- check_packaged_consumer.sh
- FinishReason
- String
- RunId
- RunContext
- Sync
- T
- TempDir
- Config
- MagiReport
- MagiError
- MagiReport
- LlmProvider
- ReasoningControl
- Vec

## God Nodes (most connected - your core abstractions)
1. `ProviderError` - 95 edges
2. `blank_ctx()` - 60 edges
3. `LlmProvider` - 53 edges
4. `MagiError` - 46 edges
5. `Completion` - 43 edges
6. `MagiBuilder` - 43 edges
7. `Magi` - 39 edges
8. `CompletionConfig` - 37 edges
9. `Config` - 37 edges
10. `Lineage` - 37 edges

## Surprising Connections (you probably didn't know these)
- `test_5xx_condemns_but_does_not_abort_endpoint_down()` --calls--> `build_two_5xx_with_local_fallbacks()`  [INFERRED]
  tests/rotation_integration.rs → src/test_support.rs
- `an_external_failure_is_mage_local_and_never_trips_endpoint_down()` --calls--> `build_two_external_failing_no_fallback()`  [INFERRED]
  tests/rotation_integration.rs → src/test_support.rs
- `test_short_pool_yields_insufficient_agents_not_collapse()` --calls--> `build_two_failing_with_single_free_fallback()`  [INFERRED]
  tests/rotation_integration.rs → src/test_support.rs
- `test_two_connection_failures_abort_with_endpoint_down()` --calls--> `build_two_network_failing_no_fallback()`  [INFERRED]
  tests/rotation_integration.rs → src/test_support.rs
- `run_where_the_first_response_fails_schema_and_the_retry_succeeds()` --calls--> `build_trio_with_caspar()`  [INFERRED]
  tests/common/mod.rs → src/test_support.rs

## Import Cycles
- None detected.

## Communities (447 total, 243 thin omitted)

### Community 0 - "test_support.rs"
Cohesion: 0.10
Nodes (29): Beh, build_two_5xx_with_local_fallbacks(), build_two_external_failing_no_fallback(), build_two_failing_with_single_free_fallback(), build_two_network_failing_no_fallback(), ok(), report_run_failed(), RoutingMockProvider (+21 more)

### Community 1 - "fixtures.rs"
Cohesion: 0.05
Nodes (74): a_file_on_disk_that_nobody_declares_is_an_orphan_too(), a_fixture_whose_hash_changed_is_rejected(), a_manifest_path_that_escapes_the_corpus_is_refused(), a_missing_fixtures_directory_is_reported_not_silently_clean(), a_nested_corpus_is_refused_because_the_walk_does_not_recurse(), a_readable_entry_is_still_crossed_against_the_manifest(), a_sync_that_cannot_finish_leaves_the_tracked_manifest_alone(), a_verified_by_naming_a_live_scenario_is_accepted() (+66 more)

### Community 2 - "consensus.rs"
Cohesion: 0.14
Nodes (49): make_output(), Result, test_agent_count_reflects_input_count(), test_approve_conditional_reject_produces_go_with_caveats(), test_conditions_are_distinct_from_recommendations_section(), test_conditions_extracted_from_conditional_agents(), test_conditions_use_summary_field_not_recommendation_field(), test_confidence_formula_clamped_and_rounded() (+41 more)

### Community 3 - "validate.rs"
Cohesion: 0.06
Nodes (45): MagiError, AgentName, From, Mode, Vec, clean_title(), finding_with_title(), output_with_confidence() (+37 more)

### Community 4 - ".build"
Cohesion: 0.09
Nodes (49): a_completion_record_is_built_in_exactly_one_place(), a_completion_that_failed_is_recorded_too(), a_mage_local_failure_does_not_condemn_the_lineage_for_the_other_seats(), a_measured_candidate_keeps_the_strict_guard_quiet(), a_run_with_no_cuts_still_records_one_entry_per_completion(), a_strict_guard_with_nothing_measured_warns_and_still_completes(), exceeding_the_threshold_warns_and_still_completes(), probe_declaration_warnings_are_told_once_per_instance() (+41 more)

### Community 5 - "String"
Cohesion: 0.16
Nodes (24): a_probe_answered_with_404_names_both_possible_causes_too(), a_probe_that_is_slow_names_both_possible_causes(), an_unsuccessful_status_is_not_described_as_a_successful_answer(), audit_published_package(), check_lock_is_tracked(), check_workspace_isolation(), classify_probe_body(), harness_files_in_listing() (+16 more)

### Community 6 - "LlmProvider"
Cohesion: 0.09
Nodes (34): Agent, AgentFactory, MockProvider, AgentName, Arc, AtomicUsize, BTreeMap, Default (+26 more)

### Community 7 - "schema.rs"
Cohesion: 0.04
Nodes (18): Finding, make_output(), Into, String, test_agent_name_btreeset_orders_alphabetically(), test_agent_name_usable_as_btreemap_key(), test_agent_output_conditional_is_not_dissenting_from_approve_majority(), test_agent_output_effective_verdict_maps_conditional_to_approve() (+10 more)

### Community 8 - "ollama_wire.rs"
Cohesion: 0.08
Nodes (27): a_native_empty_completion_carries_the_reasoning_it_burned(), a_native_empty_without_the_footprint_falls_back_to_empty_completion(), an_overlong_done_reason_is_capped_as_it_is_deserialized(), done_reason_body(), into_completion_carries_a_measured_reasoning_trace_when_present(), into_completion_carries_the_telemetry_of_a_real_success(), is_crate_defect(), native_empty_with_counters() (+19 more)

### Community 9 - "orchestrator.rs"
Cohesion: 0.06
Nodes (80): Elapsed, a_crate_defect_outranks_a_simultaneous_endpoint_outage(), a_crate_defect_records_nothing_because_its_report_will_not_exist(), a_probe_declaring_another_model_is_named_not_rejected(), a_probe_declaring_its_own_model_is_quiet(), a_probe_that_claims_nothing_is_checked_against_nothing(), a_threshold_is_unreachable_when_the_smallest_warning_input_is_already_rejected(), a_threshold_that_can_never_fire_is_detected() (+72 more)

### Community 11 - "claude_cli.rs"
Cohesion: 0.11
Nodes (35): ClaudeCliProvider, CliOutput, CliUsage, parse_completion(), parse_envelope(), F, Into, Option (+27 more)

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

### Community 17 - "runner.rs"
Cohesion: 0.08
Nodes (39): RunOutcome, a_timed_out_run_reports_its_cap_verbatim(), attempts_for(), build_magi_against(), build_with_absurd_timings(), cannot_test_is_a_skip_with_a_reason_not_a_silent_empty_result(), chat_request(), classify_error() (+31 more)

### Community 18 - "s4_rotation_and_its_cause"
Cohesion: 0.19
Nodes (18): report_with_caspar_rotation_chain(), s4_cause_check_fails_on_a_hop_misclassified_as_schema(), s4_cause_check_fails_rather_than_skips_when_nothing_rotated(), s4_cause_check_passes_on_a_transport_classified_hop(), s4_ctx_with_rotation(), s4_fails_when_analyze_returned_a_typed_failure(), s4_rotated_check_fails_when_the_chain_is_empty_did_not_rotate(), s4_rotated_check_fails_when_the_hop_lands_on_the_same_lineage() (+10 more)

### Community 19 - "Config"
Cohesion: 0.18
Nodes (13): Config, Duration, a_backend_listing_the_whole_trio_gets_past_the_backend_step(), a_listing_larger_than_the_cap_is_not_held_in_memory(), PreflightError, raise_proxy(), Display, Error (+5 more)

### Community 20 - "MAGI System Technical Documentation"
Cohesion: 0.40
Nodes (4): Exemption from CLAUDE.local.md §0.2 file-header rule, Local divergence from the pinned reference (F0, 2026-07-16), Regeneration, `src/prompts_md/` — Embedded prompt data

### Community 21 - "ollama.rs"
Cohesion: 0.09
Nodes (31): a_mounted_prefix_survives_both_spellings(), a_root_that_really_ends_in_v1_is_read_as_the_prefix(), construction_keeps_the_real_credentials_on_the_authority(), every_accepted_spelling_yields_the_same_endpoints(), OllamaProvider, Client, Duration, Into (+23 more)

### Community 22 - "Self"
Cohesion: 0.12
Nodes (16): Deserialize, Serialize, an_external_implementor_can_report_real_telemetry_not_only_unmeasured(), BudgetBearing, CompletionTelemetry, elided(), finish_reason_keeps_an_unknown_wire_value_instead_of_dropping_it(), finish_reason_other_is_capped_at_64_chars_on_a_char_boundary() (+8 more)

### Community 23 - "MagiBuilder"
Cohesion: 0.08
Nodes (28): ComplexityGate, ConsensusConfig, CapturingMockProvider, contract_prompt(), MagiBuilder, Arc, Box, HashMap (+20 more)

### Community 24 - "proxy.rs"
Cohesion: 0.05
Nodes (63): B, Bytes, X, HeaderMap, Incoming, Infallible, Item, ProxyBody (+55 more)

### Community 25 - "reporting.rs"
Cohesion: 0.07
Nodes (28): Dissent, a_clean_run_gains_no_section_at_all(), a_fresh_record_measures_nothing_and_says_so(), a_report_from_before_this_version_also_gains_nothing(), a_report_from_the_previous_version_deserializes_as_none_not_zero(), a_report_with_no_completions_at_all_carries_no_key(), an_empty_pool_eligibility_is_still_written_to_the_wire(), an_exceeded_run_gains_exactly_one_section_naming_both_numbers() (+20 more)

### Community 27 - "make_output"
Cohesion: 0.40
Nodes (4): Adding a pair, One pair per ALTERNATIVE, not per rule, Redaction-gate fixtures, Verifying a change

### Community 28 - "[0.5.0] - 2026-05-16"
Cohesion: 0.25
Nodes (8): [0.5.0] - 2026-05-16, Added, Backward compatibility, Changed (breaking), Documentation, Performance, Pre-merge gates (CLAUDE.local.md §6), Test count

### Community 29 - "verdict_markers.rs"
Cohesion: 0.05
Nodes (20): Fn, extract(), ExtractionFailureCause, is_marker_line(), locate(), locate_block(), normalize_line(), Cow (+12 more)

### Community 30 - "[0.2.0] - 2026-04-18"
Cohesion: 0.29
Nodes (7): [0.2.0] - 2026-04-18, Added, Changed (breaking), Dependencies, Deprecated, Not included (deferred to v0.3.0), Security considerations

### Community 31 - "[3.0.1] - 2026-07-30"
Cohesion: 0.50
Nodes (4): [3.0.1] - 2026-07-30, Changed, Fixed, Notes

### Community 32 - "[0.4.0] - 2026-05-16"
Cohesion: 0.29
Nodes (7): [0.4.0] - 2026-05-16, Added, Backward compatibility, Changed, Documentation, Performance, Test count

### Community 33 - "report.rs"
Cohesion: 0.15
Nodes (27): a_certificate_is_refused_over_a_table_that_established_nothing(), a_certificate_that_cannot_name_its_subject_or_its_cost_is_refused(), a_certificate_that_could_not_be_written_never_replaces_the_verdict(), a_certificate_with_no_large_payload_row_says_so_instead_of_omitting_it(), a_clean_tree_gets_the_certificate_written_at_cert_path(), a_corpus_over_the_unverified_threshold_warns_in_the_certificate(), a_dirty_tree_gets_no_certificate_not_a_caveated_one(), a_fixture_repository_is_removed_when_its_guard_drops() (+19 more)

### Community 34 - "[0.6.0] - 2026-05-21"
Cohesion: 0.33
Nodes (6): [0.6.0] - 2026-05-21, Backward compatibility, Changed, Pre-merge gates (CLAUDE.local.md §6), Security, Test count

### Community 35 - "[1.0.0] - 2026-05-24"
Cohesion: 0.40
Nodes (5): [1.0.0] - 2026-05-24, Added, Changed (breaking), Notes, Security

### Community 36 - "provider.rs"
Cohesion: 0.06
Nodes (34): Range, a_budget_inside_the_window_says_nothing(), a_budget_under_the_floor_is_still_reported(), a_limited_class_gets_two_attempts_not_four(), a_limited_count_above_the_general_one_is_reported(), a_single_honoured_wait_that_eats_the_whole_budget_is_flagged(), a_wait_exactly_equal_to_the_budget_is_flagged_too(), budget_window() (+26 more)

### Community 37 - "RoutingMockProvider"
Cohesion: 0.50
Nodes (3): leaky(), Error, String

### Community 38 - "build_user_prompt"
Cohesion: 0.13
Nodes (25): Sized, build_user_prompt(), fixed_nonce(), Mode, Result, Self, Vec, test_build_user_prompt_accepts_empty_content() (+17 more)

### Community 39 - "Changelog"
Cohesion: 0.18
Nodes (11): [0.1.2] - 2026-04-05, [0.3.1] - 2026-04-19, [2.1.0] - 2026-07-27, [3.2.0] - 2026-08-10, Added, Added, Changelog, Compatibility (+3 more)

### Community 40 - "s15_degradation_is_honest"
Cohesion: 0.29
Nodes (16): MagiReport, a_fired_injection(), report_from(), s15_ctx(), s15_degradation_is_honest(), s15_fails_the_named_agent_check_when_a_different_agent_was_injected(), s15_fails_the_strong_label_check_on_an_illegitimate_strong_label(), s15_fails_when_analyze_returned_a_typed_failure() (+8 more)

### Community 41 - "magi-core"
Cohesion: 0.17
Nodes (12): Changelog, Consensus Labels, Contribution, Credentials in a provider URL, Example, Feature Flags, Features, Implementing a Custom Provider (+4 more)

### Community 42 - "normalize_newlines"
Cohesion: 0.24
Nodes (10): neutralize_headers(), normalize_newlines(), Cow, sanitize_error_for_retry_feedback(), strip_invisibles(), test_neutralize_headers_not_bypassed_by_mongolian_vowel_separator(), test_neutralize_headers_preserves_unmatched_lines_borrowed(), test_normalize_newlines_handles_empty_string() (+2 more)

### Community 43 - "e2.rs"
Cohesion: 0.09
Nodes (40): content_failure_detail(), erosion_ctx(), r17_fails_rather_than_passing_vacuously_when_nothing_was_measured(), r17_fails_when_only_some_completions_saw_the_large_payload(), r17_fails_when_the_payload_silently_shrank(), r17_passes_when_the_payload_arrived_large(), r17_reads_the_measured_records_and_ignores_the_silent_ones(), rec() (+32 more)

### Community 44 - ".new"
Cohesion: 0.15
Nodes (31): a_limited_class_stops_at_its_own_count_not_at_max_retries(), a_present_retry_after_meets_the_attempt_cap_for_a_configured_http_class(), a_refused_redirect_is_mage_local_and_never_retried(), completion_new_takes_only_the_mandatory_field(), no_synthetic_http_status_survives_anywhere_in_the_crate(), test_base_zero_with_three_retries_emits_exactly_four_requests(), test_budget_exhaustion_abandons_with_typed_reason(), test_honored_retry_after_can_overrun_a_small_budget() (+23 more)

### Community 45 - ".new"
Cohesion: 0.19
Nodes (31): make_agent(), make_consensus(), test_agent_display_fallback_to_agent_name_methods(), test_agent_line_format(), test_banner_all_lines_are_exactly_banner_width(), test_banner_consensus_line_includes_split_for_go_with_caveats(), test_banner_labels_are_column_aligned_to_max_label_len(), test_banner_lines_are_exactly_52_chars_wide() (+23 more)

### Community 46 - "Quick Start"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 47 - "ProviderError"
Cohesion: 0.06
Nodes (23): a_contract_detail_is_capped_at_the_declared_length(), a_contract_detail_under_the_cap_is_untouched(), a_message_of_exactly_the_cap_is_kept_whole(), a_multi_byte_contract_detail_is_cut_without_panicking(), a_short_external_message_survives_untouched(), AbandonReason, an_oversized_external_message_is_cut_and_says_so(), empty_completion_remedy() (+15 more)

### Community 48 - "Lineage"
Cohesion: 0.12
Nodes (13): empty(), empty_s(), Lineage, RotationEvent, RotationKind, BTreeSet, Cow, Display (+5 more)

### Community 49 - "rotation_integration.rs"
Cohesion: 0.11
Nodes (16): ExtractionFailureCause, build_oversized_case(), build_schema_local_case(), build_trio_with_caspar(), a_seat_that_rotated_leaves_one_entry_per_model(), an_empty_completion_leaves_its_measurement_in_the_report(), an_external_failure_rotates_the_seat_and_the_run_completes(), an_oversized_response_is_mage_local_and_the_run_completes() (+8 more)

### Community 50 - "FixedRng"
Cohesion: 0.40
Nodes (4): FixedRng, test_fastrand_source_returns_distinct_values_across_calls(), test_fixed_rng_panics_when_exhausted(), VecDeque

### Community 52 - "[0.3.0] - 2026-04-18"
Cohesion: 0.33
Nodes (6): [0.3.0] - 2026-04-18, Added, Changed (breaking), Dependencies, Not included (deferred beyond v0.3.0), Security considerations

### Community 54 - "backoff.rs"
Cohesion: 0.16
Nodes (25): FnMut, fixed(), next_backoff(), parse_retry_after(), RetryAfter, Duration, Option, String (+17 more)

### Community 65 - "prompts/mod.rs"
Cohesion: 0.10
Nodes (21): balthasar_prompt(), caspar_prompt(), embedded_prompt_for(), lookup_prompt(), melchior_prompt(), BTreeMap, Option, Result (+13 more)

### Community 66 - "AgentName"
Cohesion: 0.17
Nodes (15): Ord, Ordering, PartialOrd, Condition, ConsensusResult, DedupFinding, Dissent, BTreeMap (+7 more)

### Community 68 - "mock_server.rs"
Cohesion: 0.21
Nodes (12): CapturedRequest, Arc, JoinHandle, Mutex, Option, String, Value, Vec (+4 more)

### Community 70 - "main.rs"
Cohesion: 0.06
Nodes (60): AssertionRow, BuildOutcome, CostLedger, CycleRun, Default, ErosionProbe, ExitCode, I (+52 more)

### Community 72 - "[3.0.0] - 2026-07-30"
Cohesion: 0.50
Nodes (4): [3.0.0] - 2026-07-30, Added, BREAKING, Changed

### Community 73 - "6. Modes of Operation"
Cohesion: 0.39
Nodes (5): fail(), prod_only(), self_test(), check_redaction.sh script, skeleton()

### Community 74 - "Model Rotation"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 75 - "rotation.rs"
Cohesion: 0.07
Nodes (24): CandidateEligibility, extract_item(), IneligibilityCause, reg(), Mutex, test_5xx_does_not_count_toward_endpoint_down(), test_active_unverifiable_digest_does_not_block_rotation(), test_calling_agent_excluded_from_digest_check() (+16 more)

### Community 76 - "basic_analysis.rs"
Cohesion: 0.29
Nodes (14): create_provider(), main(), parse_mode(), print_usage(), ProviderArgs, read_input(), Arc, Box (+6 more)

### Community 80 - ".default"
Cohesion: 0.19
Nodes (23): Future, a_config_without_seats_is_rejected_by_name_in_the_config_step(), a_receipt_BEFORE_the_runs_is_refused_even_though_the_estimate_was_announced(), a_receipt_is_refused_when_the_intervals_do_not_MATCH_the_announced_runs(), a_run_that_ends_badly_is_still_measured_so_the_receipt_is_not_withheld(), an_absurd_run_payload_does_not_overflow_the_announced_estimate(), announce_cost(), backend_runs_of() (+15 more)

### Community 81 - "[3.1.0] - 2026-07-31"
Cohesion: 0.40
Nodes (5): [3.1.0] - 2026-07-31, Changed, Documented, Fixed, Security

### Community 82 - "5. Data Schema and Consensus Protocol"
Cohesion: 0.40
Nodes (3): redacted(), String, X

### Community 103 - "Debug"
Cohesion: 0.40
Nodes (3): redacted(), String, X

### Community 104 - "RunId"
Cohesion: 0.15
Nodes (17): RunId, AssertionRow, both_renderers_name_the_cap_that_was_exceeded_rather_than_an_overrun(), CertificateFacts, format_row(), iso_date_utc(), large_payload_priority(), row_to_json() (+9 more)

### Community 107 - "outcome.rs"
Cohesion: 0.07
Nodes (21): a_hookless_unwind_cannot_inherit_an_earlier_panics_attribution(), a_panic_inside_a_run_is_caught_and_attributed_instead_of_killing_the_harness(), a_run_that_finishes_is_returned_untouched(), classify_panic(), crate_under_test_packages(), every_harness_only_dep_is_ABSENT_from_the_crate_under_tests_own_graph(), exit_code(), graph_query_target_dir() (+13 more)

### Community 108 - "Self"
Cohesion: 0.29
Nodes (4): 8. Evangelion Correspondence Table, 9. Relationship to the MAGI Python Plugin, MAGI System — Complete Technical Documentation, Multi-Perspective Analysis Library for Rust

### Community 109 - "run_preflight"
Cohesion: 0.10
Nodes (12): Barrier, FailingProbe, MockProbe, MockProvider, OverlapProbe, AtomicUsize, Formatter, Result (+4 more)

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

### Community 116 - "openai_compat.rs"
Cohesion: 0.07
Nodes (42): a_body_without_choices_is_a_contract_failure_named_for_what_is_missing(), a_control_that_cannot_be_honoured_is_declared_even_when_reasoning_came_back(), a_filtered_reply_is_not_answered_with_raise_your_budget(), absent_usage_leaves_the_counters_unmeasured_rather_than_zero(), an_empty_completion_names_the_budget_that_cut_it(), content_with_text_is_returned(), into_completion_carries_the_telemetry_of_a_real_success(), OpenAiChoice (+34 more)

### Community 117 - "Into"
Cohesion: 0.38
Nodes (5): build(), ProviderError, Error, From, Self

### Community 118 - "Option"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

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

### Community 129 - "HostedModel"
Cohesion: 0.27
Nodes (5): HostedModel, Option, Result, String, SidecarProbe

### Community 132 - "s2b_the_proxy_is_transparent"
Cohesion: 0.30
Nodes (11): RequestRecord, sha256_hex(), record(), recorded_response(), s2b_fails_when_the_recorded_request_body_hash_disagrees(), s2b_fails_when_the_relayed_status_differs_from_the_one_the_backend_gave(), s2b_passes_when_both_hashes_and_the_status_match(), s2b_skips_when_the_direct_half_left_no_status_to_compare_against() (+3 more)

### Community 133 - "Instant"
Cohesion: 0.50
Nodes (4): 2.1 Conceptual Mapping, 2.2 Why Three Perspectives and Not Two or Five, 2.3 Addressing Cognitive Biases, 2. Translation to the Software Engineering Domain

### Community 134 - "testkit.rs"
Cohesion: 0.06
Nodes (40): AsRef, Deref, a_cold_model_passes_on_the_second_probe_attempt(), an_endpoint_slow_every_time_still_reports_cannot_test(), the_preflight_stops_if_smoke_cargo_lock_is_not_actually_tracked(), AlwaysSlowStub, block_comment_depth_after(), BulkStub (+32 more)

### Community 137 - "String"
Cohesion: 0.50
Nodes (3): ProviderUrl, redacted(), String

### Community 138 - "Vec"
Cohesion: 0.50
Nodes (4): 3.1 Melchior — The Scientist, 3.2 Balthasar — The Pragmatist, 3.3 Caspar — The Critic, 3. The Three Agents in Detail

### Community 152 - "finding_id.rs"
Cohesion: 0.23
Nodes (13): D, de_category(), de_opt_file(), de_opt_line(), generate_finding_id(), normalize_category(), normalize_path(), Error (+5 more)

### Community 164 - "ProviderError"
Cohesion: 0.50
Nodes (4): 6.1 Code Review, 6.2 Design, 6.3 Analysis, 6. Modes of Operation

### Community 165 - "[4.0.0] - 2026-08-23"
Cohesion: 0.25
Nodes (8): [4.0.0] - 2026-08-23, Added, Changed, Fixed, Migration, One story, not two: the completion budget and the time budget, Removed, The defect this release exists for

### Community 166 - "Result"
Cohesion: 0.50
Nodes (4): Declaring fallbacks, Model Rotation, Ollama probe (feature `ollama`), What a rotation looks like

### Community 167 - "[1.0.1] - 2026-05-25"
Cohesion: 0.67
Nodes (3): [1.0.1] - 2026-05-25, Fixed, Internal

### Community 168 - "e.rs"
Cohesion: 0.26
Nodes (10): RunId, ScenarioState, e_scenarios(), no_report_skips_instead_of_passing(), report_with(), Vec, s_e2_fails_when_nothing_was_ruled_out(), s_e2_why_a_candidate_was_not_eligible() (+2 more)

### Community 173 - "F"
Cohesion: 0.67
Nodes (3): Architecture, Module Dependency Graph, Prompt Injection Defense

### Community 174 - "common/mod.rs"
Cohesion: 0.26
Nodes (11): TcpListener, TcpStream, accept_one(), bind_loopback(), ends_header(), hang_up(), Option, Result (+3 more)

### Community 176 - "CompletionConfig"
Cohesion: 0.12
Nodes (10): describe(), main(), MinimalProvider, MyBackend, Result, String, CompletionConfig, ReasoningControl (+2 more)

### Community 179 - "serve_once"
Cohesion: 0.42
Nodes (9): a_client_configured_the_way_this_crate_does_it_leaks_nothing(), authorization_is_stripped_across_origins(), authorization_survives_a_same_origin_redirect(), redirect_to(), Option, String, TcpListener, serve_once() (+1 more)

### Community 180 - "dedup_key"
Cohesion: 0.25
Nodes (8): dedup_key(), DedupKey, finding_key(), test_dedup_key_casefold_greek_sigma_variants(), test_dedup_key_casefold_sharp_s_equals_double_s(), test_dedup_key_nfkc_collapses_combining_accents(), test_dedup_key_nfkc_collapses_fullwidth_latin(), test_dedup_key_preserves_interior_whitespace()

### Community 181 - "preflight.rs"
Cohesion: 0.08
Nodes (22): CompletionConfig, Seat, a_backend_holding_every_seat_model_passes_the_check(), a_config_that_is_not_a_full_trio_is_rejected_as_a_config_fault(), a_config_with_no_fallbacks_is_a_config_fault_not_a_crate_verdict(), a_fallback_sharing_a_seats_lineage_is_rejected(), a_fallback_sharing_a_seats_model_is_rejected(), a_rotation_candidate_the_backend_does_not_hold_is_refused_too() (+14 more)

### Community 183 - "provider_url.rs"
Cohesion: 0.50
Nodes (3): leaky(), Error, String

### Community 188 - ".new_checked"
Cohesion: 0.40
Nodes (3): leaky(), Error, String

### Community 191 - "RunContext"
Cohesion: 0.18
Nodes (23): Assertion, Into, analyze_produced_a_report(), answered_probes(), preflight_error_for_stage(), render_combinations(), Option, Result (+15 more)

### Community 192 - "MagiReport"
Cohesion: 0.24
Nodes (14): Condition, ConsensusResult, ExtractionFailure, InputSize, MagiReport, ReportFormatter, AgentName, AgentOutput (+6 more)

### Community 193 - "RunContext"
Cohesion: 0.18
Nodes (25): assert_that(), Assertion, BackendNeed, BuildOutcome, RunContext, Scenario, Source, e2_scenarios() (+17 more)

### Community 194 - "retry_template"
Cohesion: 0.25
Nodes (8): a_budget_cut_before_the_opening_marker_is_also_attributed_to_the_budget(), a_cut_that_was_not_the_budget_also_keeps_the_old_wording(), a_known_budget_cut_stops_asking_the_model_not_to_stop(), an_unterminated_of_unknown_cause_keeps_the_old_wording(), missing_markers_without_a_budget_cut_keeps_its_own_wording(), retry_template(), ExtractionFailureCause, Option

### Community 195 - ".format_findings"
Cohesion: 0.29
Nodes (6): DedupFinding, test_findings_line_does_not_contain_detail_text(), test_findings_line_marker_column_is_5_chars_left_justified(), test_findings_line_matches_python_layout_exactly(), test_findings_line_severity_label_column_is_14_chars_left_justified(), test_report_markdown_omits_structured_finding_fields()

### Community 196 - "config.rs"
Cohesion: 0.06
Nodes (53): FnOnce, a_budget_beyond_the_ceiling_is_rejected_and_the_field_is_named(), a_correctly_spelled_override_still_loads(), a_file_that_is_still_illegal_once_every_override_is_applied_is_refused(), a_file_that_omits_seats_and_fallbacks_gets_the_built_in_ones(), a_file_value_an_override_rescues_is_judged_on_the_final_set_not_the_file_alone(), a_set_of_overrides_that_is_still_illegal_at_the_end_is_refused(), a_zero_budget_names_the_budget_and_not_the_probe_timeout() (+45 more)

### Community 197 - ".probe"
Cohesion: 0.33
Nodes (6): a_backend_that_does_not_hold_the_probe_model_is_INCONCLUSIVE_not_clear(), the_backend_step_and_the_probe_step_ask_different_questions(), the_contention_probe_sends_a_real_completion_not_a_manifest_listing(), the_window_bound_has_one_entry_per_request_the_preflight_makes(), stub_that_holds_no_model(), stub_that_records_requests()

### Community 198 - "Self"
Cohesion: 0.33
Nodes (3): parent_climbs_one_level_and_stops_at_the_root(), Self, T

### Community 199 - "ConsensusEngine"
Cohesion: 0.33
Nodes (4): ConsensusConfig, ConsensusEngine, Default, Self

### Community 200 - "[2.2.0] - 2026-07-27"
Cohesion: 0.67
Nodes (3): [2.2.0] - 2026-07-27, Changed, Fixed

### Community 204 - "git.rs"
Cohesion: 0.33
Nodes (4): Path, Result, String, status_porcelain()

### Community 209 - "MockProbe"
Cohesion: 0.43
Nodes (3): MockProbe, Option, String

### Community 210 - "CompletionRecord"
Cohesion: 0.33
Nodes (6): CompletionRecord, from_telemetry_copies_every_measurement_across(), from_telemetry_invents_nothing_when_nothing_was_measured(), Self, test_with_config_rejects_banner_width_too_small(), the_unsupported_declaration_survives_the_conversion()

### Community 211 - "body_bounds.rs"
Cohesion: 0.26
Nodes (13): a_chunked_probe_body_degrades_from_the_streaming_branch(), a_chunked_probe_body_under_the_cap_is_read_and_parsed(), a_probe_body_over_the_cap_degrades_instead_of_failing(), a_success_body_at_the_cap_is_read_whole(), a_success_body_over_the_cap_fails_rather_than_arriving_truncated(), a_verdict_body_that_is_not_utf8_fails_instead_of_being_mangled(), an_error_body_over_the_cap_keeps_its_prefix_and_announces_the_cut(), Framing (+5 more)

### Community 212 - "pool"
Cohesion: 0.25
Nodes (12): ae(), policy(), pool(), record_digest_collision_writes_into_the_collision_map(), state(), test_claim_next_none_leaves_registry_intact(), test_concurrent_claims_never_double_reserve_stress(), test_max_rotations_gate() (+4 more)

### Community 214 - "Report"
Cohesion: 0.14
Nodes (11): a_harness_fault_reports_the_cycle_run_it_actually_was(), a_row_that_belongs_to_no_run_is_not_attributed_to_one(), a_time_failure_renders_visibly_different_from_an_assertion_failure(), a_verdict_about_the_crate_outranks_our_own_failure_to_certify(), both_renderers_carry_the_scenario_id_its_field_promises(), cycle_run_label(), CycleRun, every_rendered_row_carries_the_run_id_that_fed_it() (+3 more)

### Community 217 - "claude.rs"
Cohesion: 0.05
Nodes (57): Error, AlwaysFailsExternally, Result, Completion, From, a_non_text_block_with_a_normal_ending_is_not_reported_as_a_budget_cut(), a_non_text_contract_failure_names_the_shape_it_saw(), a_present_but_empty_thinking_payload_is_a_measured_zero_like_the_other_wire() (+49 more)

### Community 220 - "magi-smoke — the smoke harness"
Cohesion: 0.13
Nodes (14): 10. `cargo audit` for the harness, 11. The reproduction that justifies the harness, 1. Three exit codes, and the difference between two of them, 2. Two dependency modes, 3. What the proxy does, and why its red is never the crate's, 4. The contention probe, and its declared scope, 5. Where it fits in the cycle, 6. Cost per run (+6 more)

### Community 222 - "ProviderError"
Cohesion: 0.40
Nodes (4): build(), ProviderError, From, Self

### Community 224 - "compose_transport_message"
Cohesion: 0.50
Nodes (3): Error, String, spawn_failure()

### Community 230 - "[2.0.0] - 2026-07-25"
Cohesion: 0.50
Nodes (4): [2.0.0] - 2026-07-25, Added, BREAKING, Changed

### Community 232 - "build_retry_prompt"
Cohesion: 0.10
Nodes (21): build_retry_prompt(), String, test_build_retry_prompt_appends_feedback_block_exact_format(), test_build_retry_prompt_does_not_neutralize_midline_tokens(), test_build_retry_prompt_does_not_resanitize_content(), test_build_retry_prompt_feedback_block_after_end_delimiter(), test_build_retry_prompt_includes_seven_keys_list(), test_build_retry_prompt_neutralizes_dash_variant_retry_markers() (+13 more)

### Community 233 - "ReportConfig"
Cohesion: 0.18
Nodes (11): ReportConfig, ReportError, Default, Display, Formatter, Result, test_new_checked_accepts_all_ascii_titles(), test_new_checked_rejects_banner_width_too_small() (+3 more)

### Community 239 - "[1.1.0] - 2026-05-25"
Cohesion: 0.67
Nodes (3): [1.1.0] - 2026-05-25, Added, Notes

### Community 241 - ".format_init_banner"
Cohesion: 0.50
Nodes (3): Mode, test_format_init_banner_shows_mode_model_timeout(), test_separator_format()

### Community 242 - "EventLog"
Cohesion: 0.13
Nodes (11): Attributes, Event, Field, Id, Metadata, Record, EventLog, FieldWriter (+3 more)

### Community 245 - "pattern8bconst_bad.rs"
Cohesion: 0.50
Nodes (4): build(), ProviderError, Self, sneak()

### Community 251 - "FallbackPool"
Cohesion: 0.15
Nodes (16): AgentSlotGuard, FallbackCandidate, FallbackPool, FallbackPoolBuilder, Arc, Drop, P, Self (+8 more)

### Community 266 - "ProviderUrl"
Cohesion: 0.25
Nodes (6): ends_with_segment_is_case_sensitive_and_ignores_a_trailing_slash(), ProviderUrl, Debug, Display, Formatter, Result

### Community 267 - "[3.0.2] - 2026-07-30"
Cohesion: 0.67
Nodes (3): [3.0.2] - 2026-07-30, Changed, Fixed

### Community 271 - "pattern8bnospace_bad.rs"
Cohesion: 0.50
Nodes (4): build(), ProviderError, Self, sneak()

### Community 272 - "pattern8bnospace_good.rs"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 273 - "ProviderResponse"
Cohesion: 0.14
Nodes (15): X, a_partial_read_stays_within_the_cap_including_its_marker(), body_cap(), diagnostic_truncation_is_announced_and_utf8_safe(), diagnostic_under_the_cap_is_untouched(), mark_truncated(), marking_a_short_text_does_not_pad_it(), marking_never_splits_a_multibyte_character() (+7 more)

### Community 274 - "pattern8bcrate_good.rs"
Cohesion: 0.50
Nodes (4): bound_it(), build(), ProviderError, Self

### Community 281 - ".new"
Cohesion: 0.23
Nodes (19): a_caller_holding_real_seat_state_produces_the_three_mid_run_causes(), a_seat_with_no_candidates_gets_an_empty_entry_not_a_missing_one(), a_spent_rotation_budget_is_reported_rather_than_read_as_eligible(), ActiveEntry, an_unmeasured_candidate_under_a_strict_guard_says_so(), cand(), EligibilityInputs, inputs() (+11 more)

### Community 282 - "compose_transport_message"
Cohesion: 0.10
Nodes (16): f(), f(), f(), f(), P, Self, cause_chain(), cause_chain_skips_the_top_level_error() (+8 more)

### Community 283 - "e1.rs"
Cohesion: 0.09
Nodes (43): RotationKind, a_typed_crate_failure_is_a_verdict_about_the_crate(), an_unclassified_failure_is_read_as_the_crate_s(), blank_ctx(), RunId, s14_does_not_blame_the_crate_for_a_config_fault_of_another_shape(), s14_fails_when_the_rejection_does_not_name_the_unknown_field(), s14_illegible_toml_is_fatal() (+35 more)

### Community 289 - "String"
Cohesion: 0.19
Nodes (20): AgentRotationState, Candidate, cap(), caps_map(), CrateDefectRecord, digest_case(), digest_case_self(), digest_case_two_active() (+12 more)

### Community 290 - "EC fixtures — captured responses from Ollama, both wire formats"
Cohesion: 0.33
Nodes (5): EC fixtures — captured responses from Ollama, both wire formats, Files, The cold-start experiment: **GO**, The two `think: false` captures, and what they are evidence FOR, Two findings the experiment was not looking for

### Community 292 - "resolve_claude_alias"
Cohesion: 0.10
Nodes (12): Ok, S, FailingProvider, resolve_claude_alias(), RetryAfterProvider, AtomicUsize, Formatter, Result (+4 more)

### Community 293 - "pattern7builderalias_bad.rs"
Cohesion: 0.67
Nodes (3): bare(), configured(), Client

### Community 294 - "pattern7builderalias_good.rs"
Cohesion: 0.67
Nodes (3): also_configured(), configured(), Client

### Community 301 - "weakened.rs"
Cohesion: 0.16
Nodes (10): a_source_file_that_cannot_be_decoded_is_reported_too(), an_unreadable_path_is_reported_and_never_skipped(), every_mark_outside_this_guards_own_fixtures_is_well_formed(), Path, Result, String, Vec, scan() (+2 more)

### Community 309 - "ProviderRequest"
Cohesion: 0.20
Nodes (7): X, X, Method, RequestBuilder, ProviderRequest, Client, send_composes_a_redacted_error_on_connection_failure()

### Community 311 - "provider_url.rs"
Cohesion: 0.13
Nodes (15): canonicalizing_does_not_touch_the_query_or_the_fragment(), debug_and_display_are_both_redacted(), join_path_is_idempotent_over_trailing_slash(), join_path_preserves_fragment(), join_path_preserves_query_and_appends_segments(), joining_a_segment_keeps_the_real_credentials(), joining_a_segment_produces_one_separator_not_two(), parse_error_never_echoes_the_raw_input() (+7 more)

### Community 324 - ".send"
Cohesion: 0.50
Nodes (3): ProviderError, Result, X

### Community 339 - "Magi"
Cohesion: 0.09
Nodes (40): AbortHandle, ConsensusEngine, DispatchOutcome, JoinError, ExternalErrorKind, a_mage_local_rotation_detail_says_what_happened_and_not_its_scope(), AbortGuard, attempt_model() (+32 more)

### Community 356 - "build_with"
Cohesion: 0.16
Nodes (14): a_hanging_seat_degrades_the_run_within_the_given_ceiling(), build_with(), build_with_honours_the_three_values_it_takes(), exceeds(), it_never_returns_err_no_matter_how_absurd_the_configuration(), MagiConfig, measure_input(), Default (+6 more)

### Community 360 - "captured_warnings"
Cohesion: 0.38
Nodes (6): captured_warnings(), F, Vec, captured_warnings_sees_a_warning_that_was_emitted(), captured_warnings_stays_empty_when_nothing_warns(), with_config_actually_emits_the_dangerous_settings_warnings()

### Community 381 - "AgentOutput"
Cohesion: 0.20
Nodes (7): AgentOutput, Mode, Display, Formatter, Result, Vec, Verdict

### Community 387 - "Migrating from 3.2.0 to 4.0.0"
Cohesion: 0.12
Nodes (16): 1. `complete()` returns `Completion`, not `String`, 2. `RotationKind` gains four variants and becomes `#[non_exhaustive]`, 3. `ProviderError::Http { status: 0 }` no longer exists, 4. `OllamaProvider` completes on `/api/chat`, 5. `CompletionConfig::max_tokens` defaults to `16_384`, 6. `MagiReport` gains `completions`, 7. The vendor termination vocabularies are fully translated, 8. `ClaudeProvider::parse_response` is gone (+8 more)

### Community 412 - "e1_scenarios"
Cohesion: 0.50
Nodes (4): e1_contains_exactly_the_scenarios_valid_against_3_2_0(), e1_scenarios(), Scenario, the_no_backend_partition_is_neither_empty_nor_everything()

### Community 423 - "Announcement"
Cohesion: 0.17
Nodes (11): a_broken_proxy_makes_the_preflight_say_cannot_test_not_failed(), Announcement, no_backend_skips_the_backend_steps_and_nothing_else(), preflight_step_order(), Debug, Formatter, the_order_is_config_then_fixtures_then_backend(), Result (+3 more)

### Community 437 - "MockProvider"
Cohesion: 0.13
Nodes (11): AtomicU32, RetryClass, attempts_for(), classify(), every_contract_variant_has_its_own_retry_class(), is_retryable(), MockProvider, RetryConfig (+3 more)

### Community 445 - ".skip"
Cohesion: 0.29
Nodes (4): an_assertion_that_could_not_be_tested_carries_its_reason(), Into, Self, RunContext<'static>

### Community 446 - "[1.1.1] - 2026-07-17"
Cohesion: 0.67
Nodes (3): [1.1.1] - 2026-07-17, Changed, Fixed

## Knowledge Gaps
- **244 isolated node(s):** `One story, not two: the completion budget and the time budget`, `The defect this release exists for`, `Added`, `Changed`, `Removed` (+239 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **243 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `ProviderError` connect `ProviderError` to `test_support.rs`, `HostedModel`, `validate.rs`, `.build`, `LlmProvider`, `ollama_wire.rs`, `orchestrator.rs`, `claude_cli.rs`, `ProviderResponse`, `ollama.rs`, `Self`, `MagiBuilder`, `compose_transport_message`, `String`, `resolve_claude_alias`, `.new`, `CompletionConfig`, `MockProvider`, `ProviderRequest`, `provider_url.rs`, `SlowFailingProvider`, `rotation.rs`, `MockProbe`, `Magi`, `claude.rs`, `run_preflight`, `openai_compat.rs`?**
  _High betweenness centrality (0.065) - this node is a cross-community bridge._
- **Why does `MagiError` connect `validate.rs` to `prompts/mod.rs`, `consensus.rs`, `.build`, `build_with`, `LlmProvider`, `build_user_prompt`, `orchestrator.rs`, `user_prompt.rs`, `common/mod.rs`, `ProviderError`, `Lineage`, `runner.rs`, `Magi`?**
  _High betweenness centrality (0.062) - this node is a cross-community bridge._
- **Why does `Config` connect `Config` to `config.rs`, `String`, `.default`, `runner.rs`, `preflight.rs`?**
  _High betweenness centrality (0.055) - this node is a cross-community bridge._
- **What connects `One story, not two: the completion budget and the time budget`, `The defect this release exists for`, `Added` to the rest of the system?**
  _244 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `test_support.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.10101010101010101 - nodes in this community are weakly interconnected._
- **Should `fixtures.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.05016722408026756 - nodes in this community are weakly interconnected._
- **Should `consensus.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.14046121593291405 - nodes in this community are weakly interconnected._