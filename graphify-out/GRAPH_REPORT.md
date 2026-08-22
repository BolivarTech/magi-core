# Graph Report - MAGI-Core  (2026-08-22)

## Corpus Check
- 186 files · ~349,362 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 3603 nodes · 7984 edges · 432 communities (211 shown, 221 thin omitted)
- Extraction: 98% EXTRACTED · 2% INFERRED · 0% AMBIGUOUS · INFERRED: 120 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `eddcc5f6`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- RoutingMockProvider
- fixtures.rs
- consensus.rs
- validate.rs
- .analyze
- rotation_integration.rs
- LlmProvider
- schema.rs
- ollama_wire.rs
- orchestrator.rs
- claude_cli.rs
- consensus
- Balthasar — The Pragmatist
- MAGI System Technical Documentation
- Caspar — The Critic
- Melchior — The Scientist
- runner.rs
- ReportFormatter
- Option
- MAGI System Technical Documentation
- ollama.rs
- FinishReason
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
- .new
- magi-core
- normalize_newlines
- e2.rs
- Result
- .new
- Quick Start
- ProviderError
- Lineage
- ExitCode
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
- Path
- ProviderError
- tempdir_with
- RunResult
- main.rs
- [3.0.0] - 2026-07-30
- 6. Modes of Operation
- Model Rotation
- rotation.rs
- basic_analysis.rs
- int
- int
- str
- test_support.rs
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
- Completion
- Formatter
- ProviderError
- outcome.rs
- Self
- PathBuf
- [1.1.0] - 2026-05-25
- BTreeMap
- Default
- Mutex
- Send
- Sync
- openai_compat.rs
- Into
- Option
- VerdictExtractionError
- Box
- BTreeMap
- Drop
- F
- HashMap
- P
- ProviderError
- TempDir
- Debug
- Duration
- LineageRegistry
- Instant
- testkit.rs
- String
- Vec
- check_calibration.sh
- redacted
- .leak
- redacted
- redacted
- MagiError
- redacted
- redacted
- .leak
- redacted
- redacted
- .leak
- redacted
- finding_id.rs
- ProviderError
- [4.0.0] - Unreleased
- Result
- [1.0.1] - 2026-05-25
- [1.1.1] - 2026-07-17
- Box
- [1.1.0] - 2026-05-25
- sync-fixtures.sh
- Drop
- F
- provider_url.rs
- Path
- CertificateFacts
- Result
- mock_server.rs
- .new
- provider_url.rs
- AgentName
- pattern0nonterminal_bad.rs
- pattern0nonterminal_good.rs
- .new_checked
- String
- String
- RunContext
- PathBuf
- Vec
- config.rs
- Display
- I
- .new
- [2.2.0] - 2026-07-27
- Vec
- smoke-certificate.md
- .new
- git.rs
- HostedModel
- CompletionRecord
- MockProvider
- body_bounds.rs
- Self
- announce_cost
- .with_limits
- claude.rs
- FinishReason
- ExitStatus
- magi-smoke — the smoke harness
- ScenarioState
- ProviderError
- RunContext
- compose_transport_message
- magi-core
- Scenario
- Instant
- Arc
- [2.0.0] - 2026-07-25
- Drop
- build_retry_prompt
- RunId
- Manifest
- String
- .send
- BudgetBearing
- compose_transport_message
- [1.1.0] - 2026-05-25
- Mutex
- Report
- EventLog
- magi-smoke
- Default
- pattern8bconst_bad.rs
- Client
- .new
- ScenarioState
- log_failure
- embedded_prompt_for
- ProviderError
- Output
- I
- Duration
- .send
- described
- PathBuf
- [3.0.2] - 2026-07-30
- ProviderUrl
- RunSpec
- PreflightError
- pattern8bnospace_bad.rs
- pattern8bnospace_good.rs
- ProviderResponse
- pattern8bcrate_good.rs
- run
- AgentOutput
- Cow
- ProviderUrl
- Completion
- CompletionConfig
- dedup_key
- AgentName
- e1.rs
- ExitCode
- Duration
- LlmProvider
- ProviderError
- Option
- EC fixtures — captured responses from Ollama, both wire formats
- .format_model_rotations
- Debug
- pattern7builderalias_bad.rs
- pattern7builderalias_good.rs
- pattern7default_bad.rs
- pattern7default_good.rs
- Formatter
- LlmProvider
- AgentRotation
- weakened.rs
- BTreeMap
- ReasoningControl
- AtomicUsize
- AgentOutput
- HashMap
- AgentRotation
- MagiReport
- ProviderRequest
- Drop
- .parse
- RequestBuilder
- Mode
- Path
- ProviderError
- Lineage
- Box
- .probe
- MagiReport
- Path
- Drop
- RequestRecord
- RunId
- BTreeMap
- Mutex
- RunId
- .request
- CompletionTelemetry
- ExtractionFailureCause
- Arc
- ProviderProbe
- F
- AtomicUsize
- Completion
- Error
- T
- CompletionConfig
- String
- HashMap
- Response
- From
- Assertion
- Url
- RunId
- MagiError
- ProviderError
- PreflightError
- check_pending.sh
- build_with
- Cow
- P
- Lineage
- AgentName
- LlmProvider
- ProviderError
- Send
- AtomicU32
- ReasoningState
- Completion
- CompletionConfig
- Sync
- Mutex
- Output
- CompletionTelemetry
- Lineage
- FinishReason
- ProviderError
- Display
- Completion
- Completion
- Send
- preflight.rs
- AgentOutput
- CompletionConfig
- ProviderProbe
- ReasoningControl
- AssertionRow
- Migrating from 3.2.0 to 4.0.0
- Sync
- AgentName
- ConsensusEngine
- ProviderError
- BTreeSet
- ExtractionFailureCause
- SpyProxy
- FinishReason
- BTreeSet
- Announcement
- Default
- Display
- CompletionTelemetry
- FinishReason
- LlmProvider
- SpyProxy
- T
- PathBuf
- Sync
- lookup_prompt
- RequestRecord
- ReasoningControl
- Completion
- CompletionConfig
- AgentName
- Default
- Duration
- Self
- T
- I
- PathBuf
- FinishReason
- LlmProvider
- ReasoningControl
- TcpListener
- Announcement
- JoinHandle
- .format_init_banner
- ProviderError
- ProviderError
- BuildOutcome
- Self
- ErosionProbe
- RunContext
- .fmt
- RunResult
- RunSpec
- Scenario
- Config
- Into
- ReasoningControl
- TransparencyProbe

## God Nodes (most connected - your core abstractions)
1. `blank_ctx()` - 60 edges
2. `ProviderError` - 55 edges
3. `MagiBuilder` - 43 edges
4. `LlmProvider` - 41 edges
5. `Completion` - 35 edges
6. `make_consensus()` - 35 edges
7. `make_agent()` - 34 edges
8. `dispatch_one_agent()` - 33 edges
9. `Magi` - 32 edges
10. `CompletionConfig` - 31 edges

## Surprising Connections (you probably didn't know these)
- `an_oversized_response_is_mage_local_and_the_run_completes()` --calls--> `build_oversized_case()`  [INFERRED]
  tests/rotation_integration.rs → src/test_support.rs
- `test_schema_fail_is_mage_local_not_run_wide()` --calls--> `build_schema_local_case()`  [INFERRED]
  tests/rotation_integration.rs → src/test_support.rs
- `test_5xx_condemns_but_does_not_abort_endpoint_down()` --calls--> `build_two_5xx_with_local_fallbacks()`  [INFERRED]
  tests/rotation_integration.rs → src/test_support.rs
- `an_external_failure_is_mage_local_and_never_trips_endpoint_down()` --calls--> `build_two_external_failing_no_fallback()`  [INFERRED]
  tests/rotation_integration.rs → src/test_support.rs
- `test_short_pool_yields_insufficient_agents_not_collapse()` --calls--> `build_two_failing_with_single_free_fallback()`  [INFERRED]
  tests/rotation_integration.rs → src/test_support.rs

## Import Cycles
- None detected.

## Communities (432 total, 221 thin omitted)

### Community 0 - "RoutingMockProvider"
Cohesion: 0.13
Nodes (14): Completion, CompletionConfig, HashMap, ProviderError, Beh, MockProbe, RoutingMockProvider, AgentName (+6 more)

### Community 1 - "fixtures.rs"
Cohesion: 0.17
Nodes (23): Output, Path, Result, a_file_on_disk_that_nobody_declares_is_an_orphan_too(), a_fixture_whose_hash_changed_is_rejected(), a_manifest_path_that_escapes_the_corpus_is_refused(), a_missing_fixtures_directory_is_reported_not_silently_clean(), a_readable_entry_is_still_crossed_against_the_manifest() (+15 more)

### Community 2 - "consensus.rs"
Cohesion: 0.14
Nodes (49): make_output(), Result, test_agent_count_reflects_input_count(), test_approve_conditional_reject_produces_go_with_caveats(), test_conditions_are_distinct_from_recommendations_section(), test_conditions_extracted_from_conditional_agents(), test_conditions_use_summary_field_not_recommendation_field(), test_confidence_formula_clamped_and_rounded() (+41 more)

### Community 3 - "validate.rs"
Cohesion: 0.07
Nodes (29): finding_with_title(), output_with_confidence(), output_with_findings(), Vec, test_validate_accepts_confidence_at_boundaries(), test_validate_accepts_finding_with_normal_title(), test_validate_accepts_valid_agent_output(), test_validate_mut_collapses_control_whitespace_in_titles() (+21 more)

### Community 4 - ".analyze"
Cohesion: 0.10
Nodes (43): a_completion_that_failed_is_recorded_too(), a_measured_candidate_keeps_the_strict_guard_quiet(), a_run_with_no_cuts_still_records_one_entry_per_completion(), a_strict_guard_with_nothing_measured_warns_and_still_completes(), exceeding_the_threshold_warns_and_still_completes(), probe_declaration_warnings_are_told_once_per_instance(), rotation_disabled_by_configuration_keeps_the_strict_guard_quiet(), F (+35 more)

### Community 5 - "rotation_integration.rs"
Cohesion: 0.11
Nodes (13): a_seat_that_rotated_leaves_one_entry_per_model(), an_empty_completion_leaves_its_measurement_in_the_report(), an_external_failure_is_mage_local_and_never_trips_endpoint_down(), an_oversized_response_is_mage_local_and_the_run_completes(), retry0(), Arc, LlmProvider, test_5xx_condemns_but_does_not_abort_endpoint_down() (+5 more)

### Community 6 - "LlmProvider"
Cohesion: 0.08
Nodes (37): describe(), main(), MinimalProvider, MyBackend, String, Agent, AgentFactory, MockProvider (+29 more)

### Community 7 - "schema.rs"
Cohesion: 0.04
Nodes (14): make_output(), test_agent_name_btreeset_orders_alphabetically(), test_agent_name_usable_as_btreemap_key(), test_agent_output_conditional_is_not_dissenting_from_approve_majority(), test_agent_output_effective_verdict_maps_conditional_to_approve(), test_agent_output_empty_findings_valid(), test_agent_output_is_dissenting_when_verdict_differs_from_majority(), test_agent_output_is_not_dissenting_when_verdict_matches_majority() (+6 more)

### Community 8 - "ollama_wire.rs"
Cohesion: 0.08
Nodes (29): ReasoningControl, a_native_empty_completion_carries_the_reasoning_it_burned(), a_native_empty_without_the_footprint_falls_back_to_empty_completion(), an_overlong_done_reason_is_capped_as_it_is_deserialized(), done_reason_body(), into_completion_carries_a_measured_reasoning_trace_when_present(), into_completion_carries_the_telemetry_of_a_real_success(), is_crate_defect() (+21 more)

### Community 9 - "orchestrator.rs"
Cohesion: 0.06
Nodes (74): CrateDefectRecord, Elapsed, ExtractionFailureCause, a_completion_record_is_built_in_exactly_one_place(), a_crate_defect_records_nothing_because_its_report_will_not_exist(), a_probe_declaring_another_model_is_named_not_rejected(), a_probe_declaring_its_own_model_is_quiet(), a_probe_that_claims_nothing_is_checked_against_nothing() (+66 more)

### Community 11 - "claude_cli.rs"
Cohesion: 0.11
Nodes (36): F, ClaudeCliProvider, CliOutput, CliUsage, parse_completion(), parse_envelope(), Into, Option (+28 more)

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
Cohesion: 0.12
Nodes (14): SessionFacts, BackendNeed, classify_error(), ErrorClass, MagiError, shipped_timings(), the_announced_runs_are_the_runs_this_stage_actually_launches(), the_degradation_run_has_no_pool_so_the_seat_actually_degrades() (+6 more)

### Community 18 - "ReportFormatter"
Cohesion: 0.12
Nodes (14): AgentName, Condition, ReportConfig, ReportError, ReportFormatter, Default, Formatter, Result (+6 more)

### Community 20 - "MAGI System Technical Documentation"
Cohesion: 0.40
Nodes (4): Exemption from CLAUDE.local.md §0.2 file-header rule, Local divergence from the pinned reference (F0, 2026-07-16), Regeneration, `src/prompts_md/` — Embedded prompt data

### Community 21 - "ollama.rs"
Cohesion: 0.08
Nodes (36): a_mounted_prefix_survives_both_spellings(), a_root_that_really_ends_in_v1_is_read_as_the_prefix(), construction_keeps_the_real_credentials_on_the_authority(), every_accepted_spelling_yields_the_same_endpoints(), OllamaProvider, Client, Completion, CompletionConfig (+28 more)

### Community 22 - "FinishReason"
Cohesion: 0.11
Nodes (20): Ok, S, Serialize, cause_chain(), cause_chain_skips_the_top_level_error(), classify(), client_build_error(), FailingProvider (+12 more)

### Community 23 - "MagiBuilder"
Cohesion: 0.08
Nodes (34): Box, ComplexityGate, ConsensusConfig, FallbackPool, Lineage, P, ProviderProbe, ReportConfig (+26 more)

### Community 24 - "proxy.rs"
Cohesion: 0.05
Nodes (60): Arc, AtomicBool, B, Bytes, X, Error, HeaderMap, Incoming (+52 more)

### Community 25 - "reporting.rs"
Cohesion: 0.08
Nodes (23): DedupFinding, Dissent, a_fresh_record_measures_nothing_and_says_so(), a_report_from_the_previous_version_deserializes_as_none_not_zero(), a_report_with_no_completions_at_all_carries_no_key(), an_older_report_without_the_field_still_deserializes(), cause_label(), fit_content() (+15 more)

### Community 27 - "make_output"
Cohesion: 0.40
Nodes (4): Adding a pair, One pair per ALTERNATIVE, not per rule, Redaction-gate fixtures, Verifying a change

### Community 28 - "[0.5.0] - 2026-05-16"
Cohesion: 0.25
Nodes (8): [0.5.0] - 2026-05-16, Added, Backward compatibility, Changed (breaking), Documentation, Performance, Pre-merge gates (CLAUDE.local.md §6), Test count

### Community 29 - "verdict_markers.rs"
Cohesion: 0.07
Nodes (4): is_marker_line(), normalize_line(), Cow, test_locate_block_argument_order_is_not_symmetric()

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
Nodes (28): a_certificate_is_refused_over_a_table_that_established_nothing(), a_certificate_that_cannot_name_its_subject_or_its_cost_is_refused(), a_certificate_that_could_not_be_written_never_replaces_the_verdict(), a_certificate_with_no_large_payload_row_says_so_instead_of_omitting_it(), a_clean_tree_gets_the_certificate_written_at_cert_path(), a_corpus_over_the_unverified_threshold_warns_in_the_certificate(), a_dirty_tree_gets_no_certificate_not_a_caveated_one(), a_fixture_repository_is_removed_when_its_guard_drops() (+20 more)

### Community 34 - "[0.6.0] - 2026-05-21"
Cohesion: 0.33
Nodes (6): [0.6.0] - 2026-05-21, Backward compatibility, Changed, Pre-merge gates (CLAUDE.local.md §6), Security, Test count

### Community 35 - "[1.0.0] - 2026-05-24"
Cohesion: 0.40
Nodes (5): [1.0.0] - 2026-05-24, Added, Changed (breaking), Notes, Security

### Community 36 - "provider.rs"
Cohesion: 0.05
Nodes (32): Range, a_budget_inside_the_window_says_nothing(), a_limited_class_gets_two_attempts_not_four(), a_refused_redirect_is_mage_local_and_never_retried(), a_single_honoured_wait_that_eats_the_whole_budget_is_flagged(), a_wait_exactly_equal_to_the_budget_is_flagged_too(), budget_window(), completion_new_takes_only_the_mandatory_field() (+24 more)

### Community 37 - "RoutingMockProvider"
Cohesion: 0.50
Nodes (3): leaky(), Error, String

### Community 38 - "build_user_prompt"
Cohesion: 0.13
Nodes (26): MagiError, Sized, build_user_prompt(), fixed_nonce(), Mode, Result, Self, Vec (+18 more)

### Community 39 - "Changelog"
Cohesion: 0.18
Nodes (11): [0.1.2] - 2026-04-05, [0.3.1] - 2026-04-19, [2.1.0] - 2026-07-27, [3.2.0] - 2026-08-10, Added, Added, Changelog, Compatibility (+3 more)

### Community 40 - ".new"
Cohesion: 0.24
Nodes (21): a_limited_class_stops_at_its_own_count_not_at_max_retries(), test_base_zero_with_three_retries_emits_exactly_four_requests(), test_budget_exhaustion_abandons_with_typed_reason(), test_honored_retry_after_can_overrun_a_small_budget(), test_max_retries_zero_does_not_retry(), test_operation_budget_zero_yields_single_attempt(), test_retry_after_beyond_cap_abandons_with_typed_reason(), test_retry_provider_does_not_retry_on_auth() (+13 more)

### Community 41 - "magi-core"
Cohesion: 0.17
Nodes (12): Changelog, Consensus Labels, Contribution, Credentials in a provider URL, Example, Feature Flags, Features, Implementing a Custom Provider (+4 more)

### Community 42 - "normalize_newlines"
Cohesion: 0.22
Nodes (9): neutralize_headers(), normalize_newlines(), sanitize_error_for_retry_feedback(), strip_invisibles(), test_neutralize_headers_not_bypassed_by_mongolian_vowel_separator(), test_neutralize_headers_preserves_unmatched_lines_borrowed(), test_normalize_newlines_handles_empty_string(), test_normalize_newlines_preserves_existing_lf_borrows() (+1 more)

### Community 43 - "e2.rs"
Cohesion: 0.08
Nodes (53): assert_that(), content_failure_detail(), e2_scenarios(), erosion_ctx(), r17_fails_rather_than_passing_vacuously_when_nothing_was_measured(), r17_fails_when_only_some_completions_saw_the_large_payload(), r17_fails_when_the_payload_silently_shrank(), r17_passes_when_the_payload_arrived_large() (+45 more)

### Community 44 - "Result"
Cohesion: 0.18
Nodes (8): DeclaringProbe, MockProvider, AtomicUsize, ProviderError, Result, test_analyze_all_agents_fail_returns_insufficient_agents(), test_analyze_one_agent_timeout_degrades_gracefully(), test_analyze_two_agents_fail_returns_insufficient_agents()

### Community 45 - ".new"
Cohesion: 0.19
Nodes (31): make_agent(), make_consensus(), test_agent_display_fallback_to_agent_name_methods(), test_agent_line_format(), test_banner_all_lines_are_exactly_banner_width(), test_banner_consensus_line_includes_split_for_go_with_caveats(), test_banner_labels_are_column_aligned_to_max_label_len(), test_banner_lines_are_exactly_52_chars_wide() (+23 more)

### Community 46 - "Quick Start"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 47 - "ProviderError"
Cohesion: 0.06
Nodes (23): Display, a_contract_detail_is_capped_at_the_declared_length(), a_contract_detail_under_the_cap_is_untouched(), a_message_of_exactly_the_cap_is_kept_whole(), a_multi_byte_contract_detail_is_cut_without_panicking(), a_short_external_message_survives_untouched(), AbandonReason, an_oversized_external_message_is_cut_and_says_so() (+15 more)

### Community 48 - "Lineage"
Cohesion: 0.14
Nodes (15): Cow, FallbackCandidate, FallbackPoolBuilder, Lineage, ProviderProbe, RotationEvent, RotationKind, Display (+7 more)

### Community 50 - "FixedRng"
Cohesion: 0.22
Nodes (7): Send, FastrandSource, FixedRng, RngLike, test_fastrand_source_returns_distinct_values_across_calls(), test_fixed_rng_panics_when_exhausted(), VecDeque

### Community 52 - "[0.3.0] - 2026-04-18"
Cohesion: 0.33
Nodes (6): [0.3.0] - 2026-04-18, Added, Changed (breaking), Dependencies, Not included (deferred beyond v0.3.0), Security considerations

### Community 54 - "backoff.rs"
Cohesion: 0.16
Nodes (26): FnMut, fixed(), next_backoff(), parse_retry_after(), RetryAfter, RetryClass, Duration, Option (+18 more)

### Community 65 - "prompts/mod.rs"
Cohesion: 0.12
Nodes (10): Result, test_guard_and_parser_agree_on_the_same_block(), test_seat_aware_validation_names_the_agent_and_mode(), test_the_error_is_actionable_on_its_own(), test_unassigned_validation_does_not_name_a_mage(), test_validate_prompt_accepts_exactly_one_ordered_pair(), test_validate_prompt_counts_markers_in_a_cr_only_prompt(), test_validate_prompt_tolerates_a_leading_bom() (+2 more)

### Community 68 - "tempdir_with"
Cohesion: 0.17
Nodes (24): a_nested_corpus_is_refused_because_the_walk_does_not_recurse(), a_root_that_exists_but_cannot_be_read_is_reported_while_an_absent_one_is_not(), a_subdirectory_that_cannot_be_read_is_an_error_not_a_silent_skip(), a_target_landing_inside_a_multibyte_character_still_returns_exactly_target_bytes(), an_entry_the_walk_cannot_read_is_an_error_not_a_dropped_file(), collect_from_entries(), collect_rs(), collect_subdir() (+16 more)

### Community 69 - "RunResult"
Cohesion: 0.18
Nodes (17): RequestRecord, RunOutcome, a_timed_out_run_reports_its_cap_verbatim(), attempts_for(), cannot_test_is_a_skip_with_a_reason_not_a_silent_empty_result(), ErosionProbe, injected_agent(), render_error() (+9 more)

### Community 70 - "main.rs"
Cohesion: 0.09
Nodes (36): AssertionRow, CycleRun, PreflightError, a_genuine_skip_still_reports_exit_two(), a_healthy_run_can_reach_exit_zero(), a_run_that_could_not_start_is_never_a_verdict_about_the_crate(), a_scenario_filtered_out_by_no_backend_is_OUT_OF_SCOPE_not_omitted(), a_scenario_reading_one_run_never_sees_ANOTHER_runs_records() (+28 more)

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
Cohesion: 0.09
Nodes (16): extract_item(), reg(), test_5xx_does_not_count_toward_endpoint_down(), test_active_unverifiable_digest_does_not_block_rotation(), test_calling_agent_excluded_from_digest_check(), test_candidate_without_digest_is_accepted_trusting_lineage(), test_endpoint_down_latch_exactly_one_true_concurrent(), test_lineage_from_owned_string_is_owned() (+8 more)

### Community 76 - "basic_analysis.rs"
Cohesion: 0.29
Nodes (14): create_provider(), main(), parse_mode(), print_usage(), ProviderArgs, read_input(), Arc, Box (+6 more)

### Community 80 - "test_support.rs"
Cohesion: 0.19
Nodes (17): Magi, build_oversized_case(), build_schema_local_case(), build_trio_with_caspar(), build_two_5xx_with_local_fallbacks(), build_two_external_failing_no_fallback(), build_two_failing_with_single_free_fallback(), build_two_network_failing_no_fallback() (+9 more)

### Community 81 - "[3.1.0] - 2026-07-31"
Cohesion: 0.40
Nodes (5): [3.1.0] - 2026-07-31, Changed, Documented, Fixed, Security

### Community 82 - "5. Data Schema and Consensus Protocol"
Cohesion: 0.40
Nodes (3): redacted(), String, X

### Community 103 - "Debug"
Cohesion: 0.40
Nodes (3): redacted(), String, X

### Community 104 - "Completion"
Cohesion: 0.11
Nodes (8): Result, From, Result, Completion, CompletionConfig, Duration, SlowFailingProvider, the_provider_trace_and_the_verdict_reasoning_are_different_fields()

### Community 107 - "outcome.rs"
Cohesion: 0.07
Nodes (20): a_hookless_unwind_cannot_inherit_an_earlier_panics_attribution(), a_panic_inside_a_run_is_caught_and_attributed_instead_of_killing_the_harness(), a_run_that_finishes_is_returned_untouched(), classify_panic(), crate_under_test_packages(), every_harness_only_dep_is_ABSENT_from_the_crate_under_tests_own_graph(), exit_code(), graph_query_target_dir() (+12 more)

### Community 108 - "Self"
Cohesion: 0.29
Nodes (4): 8. Evangelion Correspondence Table, 9. Relationship to the MAGI Python Plugin, MAGI System — Complete Technical Documentation, Multi-Perspective Analysis Library for Rust

### Community 109 - "PathBuf"
Cohesion: 0.24
Nodes (11): PathBuf, feature_matrix_target_dir(), metadata_target_dir(), nothing_the_harness_generates_lands_in_the_repo(), repo_root(), resolve_deepest_existing(), smoke_dir(), the_certificate_is_the_one_declared_exception_and_it_is_named() (+3 more)

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
Nodes (43): a_body_without_choices_is_a_contract_failure_named_for_what_is_missing(), a_control_that_cannot_be_honoured_is_declared_even_when_reasoning_came_back(), a_filtered_reply_is_not_answered_with_raise_your_budget(), absent_usage_leaves_the_counters_unmeasured_rather_than_zero(), an_empty_completion_names_the_budget_that_cut_it(), content_with_text_is_returned(), into_completion_carries_the_telemetry_of_a_real_success(), OpenAiChoice (+35 more)

### Community 117 - "Into"
Cohesion: 0.38
Nodes (5): build(), ProviderError, Error, From, Self

### Community 118 - "Option"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 121 - "VerdictExtractionError"
Cohesion: 0.16
Nodes (16): Fn, extract(), ExtractionFailureCause, locate(), locate_block(), Display, Error, Formatter (+8 more)

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

### Community 129 - "TempDir"
Cohesion: 0.20
Nodes (11): AsRef, Deref, pid_is_alive(), sweep_stale_temps(), the_preflight_stops_if_smoke_cargo_lock_is_not_actually_tracked(), the_startup_sweep_removes_only_temps_whose_pid_is_dead(), fresh_temp_dir(), repo_where_the_negation_was_removed() (+3 more)

### Community 132 - "LineageRegistry"
Cohesion: 0.15
Nodes (13): ActiveEntry, AgentSlotGuard, CrateDefectRecord, LineageRegistry, RegistryInner, RotationConfig, AgentName, Arc (+5 more)

### Community 133 - "Instant"
Cohesion: 0.50
Nodes (4): 2.1 Conceptual Mapping, 2.2 Why Three Perspectives and Not Two or Five, 2.3 Addressing Cognitive Biases, 2. Translation to the Software Engineering Domain

### Community 134 - "testkit.rs"
Cohesion: 0.09
Nodes (26): AtomicUsize, Mutex, a_broken_proxy_makes_the_preflight_say_cannot_test_not_failed(), a_cold_model_passes_on_the_second_probe_attempt(), an_endpoint_slow_every_time_still_reports_cannot_test(), AlwaysSlowStub, block_comment_depth_after(), BulkStub (+18 more)

### Community 137 - "String"
Cohesion: 0.50
Nodes (3): ProviderUrl, redacted(), String

### Community 138 - "Vec"
Cohesion: 0.50
Nodes (4): 3.1 Melchior — The Scientist, 3.2 Balthasar — The Pragmatist, 3.3 Caspar — The Critic, 3. The Three Agents in Detail

### Community 144 - "MagiError"
Cohesion: 0.24
Nodes (10): MagiError, Error, From, Mode, Self, clean_title(), Result, String (+2 more)

### Community 152 - "finding_id.rs"
Cohesion: 0.23
Nodes (13): D, de_category(), de_opt_file(), de_opt_line(), generate_finding_id(), normalize_category(), normalize_path(), Error (+5 more)

### Community 164 - "ProviderError"
Cohesion: 0.50
Nodes (4): 6.1 Code Review, 6.2 Design, 6.3 Analysis, 6. Modes of Operation

### Community 165 - "[4.0.0] - Unreleased"
Cohesion: 0.33
Nodes (6): [4.0.0] - Unreleased, Added, Changed, Fixed, Migration, One story, not two: the completion budget and the time budget

### Community 166 - "Result"
Cohesion: 0.50
Nodes (4): Declaring fallbacks, Model Rotation, Ollama probe (feature `ollama`), What a rotation looks like

### Community 167 - "[1.0.1] - 2026-05-25"
Cohesion: 0.67
Nodes (3): [1.0.1] - 2026-05-25, Fixed, Internal

### Community 168 - "[1.1.1] - 2026-07-17"
Cohesion: 0.67
Nodes (3): [1.1.1] - 2026-07-17, Changed, Fixed

### Community 173 - "F"
Cohesion: 0.67
Nodes (3): Architecture, Module Dependency Graph, Prompt Injection Defense

### Community 174 - "provider_url.rs"
Cohesion: 0.15
Nodes (8): a_partial_read_stays_within_the_cap_including_its_marker(), diagnostic_truncation_is_announced_and_utf8_safe(), diagnostic_under_the_cap_is_untouched(), mark_truncated(), marking_a_short_text_does_not_pad_it(), marking_never_splits_a_multibyte_character(), parse_error_never_echoes_the_raw_input(), truncate_diagnostic()

### Community 177 - "CertificateFacts"
Cohesion: 0.29
Nodes (8): FixtureSummary, CertificateFacts, iso_date_utc(), large_payload_priority(), Option, String, unresolved(), SystemTime

### Community 180 - "mock_server.rs"
Cohesion: 0.26
Nodes (11): JoinHandle, CapturedRequest, Arc, AtomicU32, Mutex, Option, String, spawn_429_then_hang() (+3 more)

### Community 181 - ".new"
Cohesion: 0.38
Nodes (10): Future, a_receipt_BEFORE_the_runs_is_refused_even_though_the_estimate_was_announced(), a_receipt_is_refused_when_the_intervals_do_not_MATCH_the_announced_runs(), a_run_that_ends_badly_is_still_measured_so_the_receipt_is_not_withheld(), backend_runs_of(), CostLedger, the_cost_is_announced_BEFORE_the_runs_and_recorded_AFTER(), the_count_check_is_the_only_implementation_of_the_interval_invariant() (+2 more)

### Community 183 - "provider_url.rs"
Cohesion: 0.50
Nodes (3): leaky(), Error, String

### Community 188 - ".new_checked"
Cohesion: 0.40
Nodes (3): leaky(), Error, String

### Community 192 - "String"
Cohesion: 0.25
Nodes (14): AgentOutput, AgentRotation, ConsensusResult, a_clean_run_gains_no_section_at_all(), a_report_from_before_this_version_also_gains_nothing(), an_exceeded_run_gains_exactly_one_section_naming_both_numbers(), ExtractionFailure, InputSize (+6 more)

### Community 193 - "RunContext"
Cohesion: 0.21
Nodes (17): Into, ScenarioState, an_assertion_that_could_not_be_tested_carries_its_reason(), Assertion, Self, RunContext, Scenario, f_scenarios() (+9 more)

### Community 196 - "config.rs"
Cohesion: 0.06
Nodes (54): Default, Deserialize, Duration, FnOnce, Self, a_budget_beyond_the_ceiling_is_rejected_and_the_field_is_named(), a_correctly_spelled_override_still_loads(), a_file_that_is_still_illegal_once_every_override_is_applied_is_refused() (+46 more)

### Community 199 - ".new"
Cohesion: 0.35
Nodes (7): Arc, Self, Vec, test_routing_mock_provider_can_inject_provider_errors(), test_routing_mock_provider_exhausted_sequence_errors(), test_routing_mock_provider_fails_when_no_task_local_scope(), test_routing_mock_provider_routes_by_task_local_identity()

### Community 200 - "[2.2.0] - 2026-07-27"
Cohesion: 0.67
Nodes (3): [2.2.0] - 2026-07-27, Changed, Fixed

### Community 204 - "git.rs"
Cohesion: 0.33
Nodes (4): Path, Result, String, status_porcelain()

### Community 209 - "HostedModel"
Cohesion: 0.24
Nodes (5): HostedModel, Option, Result, String, SidecarProbe

### Community 210 - "CompletionRecord"
Cohesion: 0.44
Nodes (5): CompletionRecord, from_telemetry_copies_every_measurement_across(), from_telemetry_invents_nothing_when_nothing_was_measured(), Self, the_unsupported_declaration_survives_the_conversion()

### Community 211 - "MockProvider"
Cohesion: 0.12
Nodes (12): AtomicU32, RetryClass, attempts_for(), MockProvider, RetryAfterProvider, RetryConfig, RetryProvider, Arc (+4 more)

### Community 212 - "body_bounds.rs"
Cohesion: 0.08
Nodes (41): TcpListener, TcpStream, a_chunked_probe_body_degrades_from_the_streaming_branch(), a_chunked_probe_body_under_the_cap_is_read_and_parsed(), a_probe_body_over_the_cap_degrades_instead_of_failing(), a_success_body_at_the_cap_is_read_whole(), a_success_body_over_the_cap_fails_rather_than_arriving_truncated(), a_verdict_body_that_is_not_utf8_fails_instead_of_being_mangled() (+33 more)

### Community 214 - "announce_cost"
Cohesion: 0.36
Nodes (9): an_absurd_run_payload_does_not_overflow_the_announced_estimate(), announce_cost(), is_large_payload(), payload_bytes_for(), RunId, the_announced_cost_counts_every_seat_and_both_sides_of_the_wire(), the_cost_announced_is_the_cost_of_the_runs_that_will_actually_happen(), with_no_backend_the_announcement_says_nothing_will_be_spent() (+1 more)

### Community 216 - ".with_limits"
Cohesion: 0.33
Nodes (6): Default, Self, test_title_length_checked_after_strip_zero_width(), test_validate_mut_atomic_no_partial_mutation_on_error(), test_validator_with_limits_uses_custom_limits(), ValidationLimits

### Community 217 - "claude.rs"
Cohesion: 0.06
Nodes (53): a_non_text_block_with_a_normal_ending_is_not_reported_as_a_budget_cut(), a_non_text_contract_failure_names_the_shape_it_saw(), a_present_but_empty_thinking_payload_is_a_measured_zero_like_the_other_wire(), a_redacted_block_beside_a_readable_one_is_not_a_complete_measurement(), a_verdict_in_a_later_text_block_survives_an_empty_or_null_earlier_one(), an_anthropic_budget_cut_with_no_text_block_names_the_budget_not_a_broken_contract(), an_empty_text_block_beside_a_tool_use_is_not_a_budget_cut(), an_empty_text_block_is_an_empty_completion_like_the_other_wire() (+45 more)

### Community 220 - "magi-smoke — the smoke harness"
Cohesion: 0.14
Nodes (13): 10. `cargo audit` for the harness, 11. The reproduction that justifies the harness, 1. Three exit codes, and the difference between two of them, 2. Two dependency modes, 3. What the proxy does, and why its red is never the crate's, 4. The contention probe, and its declared scope, 5. Where it fits in the cycle, 6. Cost per run (+5 more)

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
Cohesion: 0.08
Nodes (30): a_budget_cut_before_the_opening_marker_is_also_attributed_to_the_budget(), a_cut_that_was_not_the_budget_also_keeps_the_old_wording(), a_known_budget_cut_stops_asking_the_model_not_to_stop(), an_unterminated_of_unknown_cause_keeps_the_old_wording(), build_retry_prompt(), missing_markers_without_a_budget_cut_keeps_its_own_wording(), retry_template(), ExtractionFailureCause (+22 more)

### Community 233 - "RunId"
Cohesion: 0.16
Nodes (16): Config, PayloadError, RunId, run_label(), build_with_absurd_timings(), chat_request(), probe_is_in_scope(), probe_window() (+8 more)

### Community 234 - "Manifest"
Cohesion: 0.15
Nodes (12): BTreeSet, I, Option, an_entry_that_cannot_be_read_is_reported_not_silently_dropped(), FixtureAudit, FixtureEntry, FixtureSummary, Manifest (+4 more)

### Community 235 - "String"
Cohesion: 0.09
Nodes (16): Barrier, digest_collision(), FailingProbe, MockProbe, MockProvider, OverlapProbe, AtomicUsize, Completion (+8 more)

### Community 236 - ".send"
Cohesion: 0.50
Nodes (3): ProviderError, Result, X

### Community 238 - "compose_transport_message"
Cohesion: 0.12
Nodes (11): f(), f(), f(), f(), P, Self, compose_caps_the_head_even_with_no_cause_chain(), compose_does_not_truncate_when_under_the_cap() (+3 more)

### Community 239 - "[1.1.0] - 2026-05-25"
Cohesion: 0.67
Nodes (3): [1.1.0] - 2026-05-25, Added, Notes

### Community 241 - "Report"
Cohesion: 0.15
Nodes (11): ExitCode, a_harness_fault_reports_the_cycle_run_it_actually_was(), a_row_that_belongs_to_no_run_is_not_attributed_to_one(), a_time_failure_renders_visibly_different_from_an_assertion_failure(), a_verdict_about_the_crate_outranks_our_own_failure_to_certify(), both_renderers_carry_the_scenario_id_its_field_promises(), cycle_run_label(), CycleRun (+3 more)

### Community 242 - "EventLog"
Cohesion: 0.13
Nodes (11): Attributes, Event, Field, Id, Metadata, Record, EventLog, FieldWriter (+3 more)

### Community 245 - "pattern8bconst_bad.rs"
Cohesion: 0.50
Nodes (4): build(), ProviderError, Self, sneak()

### Community 251 - ".new"
Cohesion: 0.14
Nodes (16): FallbackPool, strict_guard_is_inert(), test_duplicate_lineage_warns_but_builds(), test_empty_pool_is_valid_and_keeps_max_rotations(), test_lineage_ord_for_btree_keys(), test_push_probing_stores_both_views(), test_push_with_probe_accepts_a_provider_that_cannot_probe(), test_push_with_probe_matches_push_probing_for_the_same_object() (+8 more)

### Community 258 - "embedded_prompt_for"
Cohesion: 0.57
Nodes (7): balthasar_prompt(), caspar_prompt(), embedded_prompt_for(), melchior_prompt(), test_echo_canary_values_are_present_in_every_shipped_prompt(), test_prompts_match_pinned_reference_sha256(), test_shipped_prompts_satisfy_the_verdict_marker_contract()

### Community 267 - "[3.0.2] - 2026-07-30"
Cohesion: 0.67
Nodes (3): [3.0.2] - 2026-07-30, Changed, Fixed

### Community 269 - "RunSpec"
Cohesion: 0.16
Nodes (17): CostLedger, Fallback, Injection, Payload, Seat, dispatch_measured(), dispatching_alone_cannot_produce_a_receipt_the_estimate_never_preceded(), ExecutesRuns (+9 more)

### Community 271 - "pattern8bnospace_bad.rs"
Cohesion: 0.50
Nodes (4): build(), ProviderError, Self, sneak()

### Community 272 - "pattern8bnospace_good.rs"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 273 - "ProviderResponse"
Cohesion: 0.24
Nodes (7): X, body_cap(), ProviderResponse, push_within_cap(), Option, String, Vec

### Community 274 - "pattern8bcrate_good.rs"
Cohesion: 0.50
Nodes (4): bound_it(), build(), ProviderError, Self

### Community 275 - "run"
Cohesion: 0.17
Nodes (27): audit_published_package(), check_lock_is_tracked(), check_seat_models(), check_workspace_isolation(), classify_probe_body(), harness_files_in_listing(), Inconclusive, listed_models() (+19 more)

### Community 278 - "ProviderUrl"
Cohesion: 0.29
Nodes (5): ends_with_segment_is_case_sensitive_and_ignores_a_trailing_slash(), parent_climbs_one_level_and_stops_at_the_root(), ProviderUrl, Debug, Display

### Community 281 - "dedup_key"
Cohesion: 0.33
Nodes (6): dedup_key(), test_dedup_key_casefold_greek_sigma_variants(), test_dedup_key_casefold_sharp_s_equals_double_s(), test_dedup_key_nfkc_collapses_combining_accents(), test_dedup_key_nfkc_collapses_fullwidth_latin(), test_dedup_key_preserves_interior_whitespace()

### Community 282 - "AgentName"
Cohesion: 0.15
Nodes (17): Ord, Ordering, PartialOrd, Condition, ConsensusResult, DedupFinding, DedupKey, Dissent (+9 more)

### Community 283 - "e1.rs"
Cohesion: 0.05
Nodes (113): RotationKind, sha256_hex(), a_fired_injection(), a_typed_crate_failure_is_a_verdict_about_the_crate(), an_unclassified_failure_is_read_as_the_crate_s(), analyze_produced_a_report(), answered_probes(), blank_ctx() (+105 more)

### Community 289 - "Option"
Cohesion: 0.17
Nodes (28): ae(), AgentRotation, AgentRotationState, Candidate, cap(), caps_map(), digest_case(), digest_case_self() (+20 more)

### Community 290 - "EC fixtures — captured responses from Ollama, both wire formats"
Cohesion: 0.33
Nodes (5): EC fixtures — captured responses from Ollama, both wire formats, Files, The cold-start experiment: **GO**, The two `think: false` captures, and what they are evidence FOR, Two findings the experiment was not looking for

### Community 291 - ".format_model_rotations"
Cohesion: 0.50
Nodes (3): test_multi_hop_two_lines_in_order(), test_no_rotation_report_section_is_empty(), test_single_hop_rotation_line_format()

### Community 293 - "pattern7builderalias_bad.rs"
Cohesion: 0.67
Nodes (3): bare(), configured(), Client

### Community 294 - "pattern7builderalias_good.rs"
Cohesion: 0.67
Nodes (3): also_configured(), configured(), Client

### Community 301 - "weakened.rs"
Cohesion: 0.12
Nodes (14): is_privilege_refusal(), make_dir_link(), make_symlink(), Error, a_directory_link_is_skipped_rather_than_followed_out_of_the_tree(), an_unreadable_path_is_reported_and_never_skipped(), every_mark_outside_this_guards_own_fixtures_is_well_formed(), Path (+6 more)

### Community 309 - "ProviderRequest"
Cohesion: 0.25
Nodes (5): X, X, RequestBuilder, ProviderRequest, Self

### Community 311 - ".parse"
Cohesion: 0.16
Nodes (14): canonicalizing_does_not_touch_the_query_or_the_fragment(), debug_and_display_are_both_redacted(), join_path_is_idempotent_over_trailing_slash(), join_path_preserves_fragment(), join_path_preserves_query_and_appends_segments(), joining_a_segment_keeps_the_real_credentials(), joining_a_segment_produces_one_separator_not_two(), parse_normalizes_dot_segments() (+6 more)

### Community 318 - ".probe"
Cohesion: 0.33
Nodes (6): a_backend_that_does_not_hold_the_probe_model_is_INCONCLUSIVE_not_clear(), the_backend_step_and_the_probe_step_ask_different_questions(), the_contention_probe_sends_a_real_completion_not_a_manifest_listing(), the_window_bound_has_one_entry_per_request_the_preflight_makes(), stub_that_holds_no_model(), stub_that_records_requests()

### Community 327 - ".request"
Cohesion: 0.25
Nodes (5): Method, Client, Formatter, Result, send_composes_a_redacted_error_on_connection_failure()

### Community 339 - "String"
Cohesion: 0.12
Nodes (41): AbortHandle, Agent, AgentFactory, BTreeMap, CompletionRecord, ConsensusEngine, DispatchOutcome, Drop (+33 more)

### Community 354 - "build_with"
Cohesion: 0.13
Nodes (17): InputSize, a_hanging_seat_degrades_the_run_within_the_given_ceiling(), build_with(), build_with_honours_the_three_values_it_takes(), exceeds(), it_never_returns_err_no_matter_how_absurd_the_configuration(), MagiConfig, measure_input() (+9 more)

### Community 380 - "preflight.rs"
Cohesion: 0.09
Nodes (27): a_backend_holding_every_seat_model_passes_the_check(), a_backend_listing_the_whole_trio_gets_past_the_backend_step(), a_config_that_is_not_a_full_trio_is_rejected_as_a_config_fault(), a_config_with_no_fallbacks_is_a_config_fault_not_a_crate_verdict(), a_config_without_seats_is_rejected_by_name_in_the_config_step(), a_fallback_sharing_a_seats_lineage_is_rejected(), a_fallback_sharing_a_seats_model_is_rejected(), a_listing_larger_than_the_cap_is_not_held_in_memory() (+19 more)

### Community 381 - "AgentOutput"
Cohesion: 0.21
Nodes (7): AgentOutput, Finding, Into, String, Vec, test_finding_with_location_and_category(), Verdict

### Community 386 - "AssertionRow"
Cohesion: 0.22
Nodes (8): Assertion, AssertionRow, both_renderers_name_the_cap_that_was_exceeded_rather_than_an_overrun(), format_row(), row_to_json(), Vec, state_marker(), Value

### Community 387 - "Migrating from 3.2.0 to 4.0.0"
Cohesion: 0.12
Nodes (16): 1. `complete()` returns `Completion`, not `String`, 2. `RotationKind` gains four variants and becomes `#[non_exhaustive]`, 3. `ProviderError::Http { status: 0 }` no longer exists, 4. `OllamaProvider` completes on `/api/chat`, 5. `CompletionConfig::max_tokens` defaults to `16_384`, 6. `MagiReport` gains `completions`, 7. The vendor termination vocabularies are fully translated, 8. `ClaudeProvider::parse_response` is gone (+8 more)

### Community 390 - "ConsensusEngine"
Cohesion: 0.33
Nodes (4): ConsensusConfig, ConsensusEngine, Default, Self

### Community 397 - "BTreeSet"
Cohesion: 0.40
Nodes (5): empty(), empty_s(), empty_wr(), BTreeSet, test_next_model_is_deterministic()

### Community 414 - "lookup_prompt"
Cohesion: 0.22
Nodes (8): lookup_prompt(), BTreeMap, Option, String, Mode, Display, Formatter, Result

### Community 430 - "Announcement"
Cohesion: 0.20
Nodes (8): Announcement, no_backend_skips_the_backend_steps_and_nothing_else(), preflight_step_order(), Debug, Formatter, the_order_is_config_then_fixtures_then_backend(), Result, run_against_an_unreachable_backend()

### Community 433 - ".format_init_banner"
Cohesion: 0.40
Nodes (3): Mode, test_format_init_banner_shows_mode_model_timeout(), test_separator_format()

### Community 437 - "Self"
Cohesion: 0.14
Nodes (13): Formatter, an_external_implementor_can_report_real_telemetry_not_only_unmeasured(), BudgetBearing, CompletionTelemetry, elided(), every_contract_variant_has_its_own_retry_class(), production_half(), ReasoningState (+5 more)

## Knowledge Gaps
- **240 isolated node(s):** `One story, not two: the completion budget and the time budget`, `Added`, `Changed`, `Fixed`, `Migration` (+235 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **221 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `ProviderError` connect `ProviderError` to `LlmProvider`, `.request`, `Completion`, `rotation.rs`, `String`, `provider_url.rs`, `MagiError`, `HostedModel`, `ProviderResponse`, `openai_compat.rs`, `Self`, `FinishReason`, `ollama.rs`, `claude.rs`, `.parse`?**
  _High betweenness centrality (0.081) - this node is a cross-community bridge._
- **Why does `MagiError` connect `MagiError` to `prompts/mod.rs`, `consensus.rs`, `validate.rs`, `LlmProvider`, `ProviderError`, `ReportFormatter`, `MagiBuilder`?**
  _High betweenness centrality (0.035) - this node is a cross-community bridge._
- **Why does `TempDir` connect `TempDir` to `String`, `tempdir_with`, `PathBuf`, `testkit.rs`?**
  _High betweenness centrality (0.018) - this node is a cross-community bridge._
- **What connects `One story, not two: the completion budget and the time budget`, `Added`, `Changed` to the rest of the system?**
  _240 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `RoutingMockProvider` be split into smaller, more focused modules?**
  _Cohesion score 0.13 - nodes in this community are weakly interconnected._
- **Should `consensus.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.14046121593291405 - nodes in this community are weakly interconnected._
- **Should `validate.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.07205513784461152 - nodes in this community are weakly interconnected._