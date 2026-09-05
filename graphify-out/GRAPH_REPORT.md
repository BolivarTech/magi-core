# Graph Report - MAGI-Core  (2026-08-29)

## Corpus Check
- 189 files · ~373,367 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 3445 nodes · 8147 edges · 252 communities (206 shown, 46 thin omitted)
- Extraction: 99% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 113 edges (avg confidence: 0.85)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `3c24a94d`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- test_support.rs
- tempdir_with
- consensus.rs
- validate.rs
- .new
- run
- LlmProvider
- schema.rs
- ollama_wire.rs
- orchestrator.rs
- user_prompt.rs
- claude_cli.rs
- TempDir
- Balthasar — The Pragmatist
- MAGI System Technical Documentation
- Caspar — The Critic
- Melchior — The Scientist
- .new
- fixtures.rs
- Manifest
- MAGI System Technical Documentation
- ollama.rs
- Completion
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
- RunId
- magi-core
- normalize_newlines
- e2.rs
- String
- .new
- Quick Start
- error.rs
- Lineage
- RunContext
- FixedRng
- [0.3.0] - 2026-04-18
- backoff.rs
- Config
- Voting rules + confidence formula
- Evangelion MAGI origin (Naoko Akagi)
- MAGI System Technical Documentation
- Structured disagreement rationale
- Why three perspectives (not 2 or 5)
- basic_analysis example
- prompts/mod.rs
- .cmp
- s4_rotation_and_its_cause
- runner.rs
- paths.rs
- main.rs
- [3.0.0] - 2026-07-30
- 6. Modes of Operation
- Model Rotation
- LineageRegistry
- basic_analysis.rs
- .complete
- .leak
- .new
- [3.1.0] - 2026-07-31
- 5. Data Schema and Consensus Protocol
- main
- s15_degradation_is_honest
- CompletionRecord
- e.rs
- AgentOutput
- AlwaysFailsExternally
- .probe
- [2.1.0] - 2026-07-27
- [3.2.0] - 2026-08-10
- .format_init_banner
- Debug
- CertificateFacts
- Formatter
- ProviderError
- outcome.rs
- Self
- Option
- [1.1.0] - 2026-05-25
- BTreeMap
- Default
- Mutex
- Send
- Sync
- openai_compat.rs
- Into
- Option
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
- [4.0.0] - 2026-08-24
- Result
- [1.0.1] - 2026-05-25
- Box
- [1.1.0] - 2026-05-25
- sync-fixtures.sh
- Drop
- F
- Finding
- preflight.rs
- provider_url.rs
- AgentName
- pattern0nonterminal_bad.rs
- pattern0nonterminal_good.rs
- .new_checked
- AgentName
- mock_server.rs
- retry_template
- VerdictExtractionError
- config.rs
- ProviderRequest
- RunResult
- smoke-certificate.md
- .new
- git.rs
- body_bounds.rs
- AssertionRow
- .render_human
- claude.rs
- magi-smoke — the smoke harness
- ProviderError
- compose_transport_message
- magi-core
- [2.0.0] - 2026-07-25
- build_retry_prompt
- ReportConfig
- Severity
- [1.1.0] - 2026-05-25
- EventLog
- magi-smoke
- pattern8bconst_bad.rs
- FallbackPool
- compose_transport_message
- log_failure
- Duration
- .send
- described
- ProviderUrl
- [3.0.2] - 2026-07-30
- pattern8bnospace_bad.rs
- pattern8bnospace_good.rs
- ProviderResponse
- pattern8bcrate_good.rs
- check_prose_artifacts.sh
- rotation.rs
- CompletionConfig
- e1.rs
- digest_case
- EC fixtures — captured responses from Ollama, both wire formats
- pattern7builderalias_bad.rs
- pattern7builderalias_good.rs
- pattern7default_bad.rs
- pattern7default_good.rs
- weakened.rs
- embedded_prompt_for
- .read_verdict_body
- provider_url.rs
- .send
- MagiError
- check_pending.sh
- Migrating from 3.2.0 to 4.0.0
- MockProvider
- .skip
- check_packaged_consumer.sh

## God Nodes (most connected - your core abstractions)
1. `AgentName` - 86 edges
2. `blank_ctx()` - 60 edges
3. `RunContext` - 53 edges
4. `LlmProvider` - 53 edges
5. `MagiError` - 46 edges
6. `MagiBuilder` - 43 edges
7. `Completion` - 43 edges
8. `Assertion` - 41 edges
9. `Magi` - 39 edges
10. `Config` - 37 edges

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

## Communities (252 total, 46 thin omitted)

### Community 0 - "test_support.rs"
Cohesion: 0.06
Nodes (47): Beh, build_oversized_case(), build_schema_local_case(), build_trio_with_caspar(), build_two_5xx_with_local_fallbacks(), build_two_external_failing_no_fallback(), build_two_failing_with_single_free_fallback(), build_two_network_failing_no_fallback() (+39 more)

### Community 1 - "tempdir_with"
Cohesion: 0.18
Nodes (24): a_root_that_exists_but_cannot_be_read_is_reported_while_an_absent_one_is_not(), a_subdirectory_that_cannot_be_read_is_an_error_not_a_silent_skip(), a_target_landing_inside_a_multibyte_character_still_returns_exactly_target_bytes(), an_entry_the_walk_cannot_read_is_an_error_not_a_dropped_file(), collect_from_entries(), collect_rs(), collect_subdir(), falls_short_of_target_fails_loudly_instead_of_returning_a_small_payload() (+16 more)

### Community 2 - "consensus.rs"
Cohesion: 0.09
Nodes (64): ConsensusConfig, ConsensusEngine, dedup_key(), DedupFinding, DedupKey, finding_key(), make_output(), Default (+56 more)

### Community 3 - "validate.rs"
Cohesion: 0.06
Nodes (40): clean_title(), finding_with_title(), output_with_confidence(), output_with_findings(), Default, Result, Self, String (+32 more)

### Community 4 - ".new"
Cohesion: 0.09
Nodes (61): a_completion_record_is_built_in_exactly_one_place(), a_completion_that_failed_is_recorded_too(), a_mage_local_failure_does_not_condemn_the_lineage_for_the_other_seats(), a_measured_candidate_keeps_the_strict_guard_quiet(), a_run_with_no_cuts_still_records_one_entry_per_completion(), a_strict_guard_with_nothing_measured_warns_and_still_completes(), contract_prompt(), exceeding_the_threshold_warns_and_still_completes() (+53 more)

### Community 5 - "run"
Cohesion: 0.13
Nodes (26): Announcement, audit_published_package(), check_lock_is_tracked(), check_workspace_isolation(), harness_files_in_listing(), listed_models(), no_backend_skips_the_backend_steps_and_nothing_else(), nothing_under_the_harness_reaches_the_published_crate() (+18 more)

### Community 6 - "LlmProvider"
Cohesion: 0.09
Nodes (33): Agent, AgentFactory, MockProvider, Arc, AtomicUsize, BTreeMap, Default, Option (+25 more)

### Community 7 - "schema.rs"
Cohesion: 0.05
Nodes (6): make_output(), test_agent_output_conditional_is_not_dissenting_from_approve_majority(), test_agent_output_effective_verdict_maps_conditional_to_approve(), test_agent_output_empty_findings_valid(), test_agent_output_is_dissenting_when_verdict_differs_from_majority(), test_agent_output_is_not_dissenting_when_verdict_matches_majority()

### Community 8 - "ollama_wire.rs"
Cohesion: 0.08
Nodes (28): a_native_empty_completion_carries_the_reasoning_it_burned(), a_native_empty_without_the_footprint_falls_back_to_empty_completion(), an_overlong_done_reason_is_capped_as_it_is_deserialized(), done_reason_body(), into_completion_carries_a_measured_reasoning_trace_when_present(), into_completion_carries_the_telemetry_of_a_real_success(), is_crate_defect(), native_empty_with_counters() (+20 more)

### Community 9 - "orchestrator.rs"
Cohesion: 0.05
Nodes (70): a_crate_defect_outranks_a_simultaneous_endpoint_outage(), a_probe_declaring_another_model_is_named_not_rejected(), a_probe_declaring_its_own_model_is_quiet(), a_probe_that_claims_nothing_is_checked_against_nothing(), a_threshold_is_unreachable_when_the_smallest_warning_input_is_already_rejected(), a_threshold_that_can_never_fire_is_detected(), a_zero_threshold_warns_always_and_does_not_disable(), an_empty_input_never_exceeds_even_a_zero_threshold() (+62 more)

### Community 11 - "claude_cli.rs"
Cohesion: 0.11
Nodes (37): Deserialize, ClaudeCliProvider, CliOutput, CliUsage, parse_completion(), parse_envelope(), F, Into (+29 more)

### Community 12 - "TempDir"
Cohesion: 0.12
Nodes (19): AsRef, Deref, pid_is_alive(), sweep_stale_temps(), the_preflight_stops_if_smoke_cargo_lock_is_not_actually_tracked(), the_startup_sweep_removes_only_temps_whose_pid_is_dead(), fresh_temp_dir(), is_privilege_refusal() (+11 more)

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

### Community 17 - ".new"
Cohesion: 0.22
Nodes (24): a_limited_class_stops_at_its_own_count_not_at_max_retries(), a_present_retry_after_meets_the_attempt_cap_for_a_configured_http_class(), test_base_zero_with_three_retries_emits_exactly_four_requests(), test_budget_exhaustion_abandons_with_typed_reason(), test_honored_retry_after_can_overrun_a_small_budget(), test_max_retries_zero_does_not_retry(), test_operation_budget_zero_yields_single_attempt(), test_retry_after_beyond_cap_abandons_with_typed_reason() (+16 more)

### Community 18 - "fixtures.rs"
Cohesion: 0.16
Nodes (24): a_file_on_disk_that_nobody_declares_is_an_orphan_too(), a_fixture_whose_hash_changed_is_rejected(), a_manifest_path_that_escapes_the_corpus_is_refused(), a_missing_fixtures_directory_is_reported_not_silently_clean(), a_nested_corpus_is_refused_because_the_walk_does_not_recurse(), a_readable_entry_is_still_crossed_against_the_manifest(), a_sync_that_cannot_finish_leaves_the_tracked_manifest_alone(), a_verified_by_naming_a_live_scenario_is_accepted() (+16 more)

### Community 19 - "Manifest"
Cohesion: 0.13
Nodes (14): an_entry_that_cannot_be_read_is_reported_not_silently_dropped(), FixtureAudit, FixtureEntry, FixtureSummary, Manifest, manifest_load_of_a_missing_file_is_an_empty_manifest_not_an_error(), BTreeSet, I (+6 more)

### Community 20 - "MAGI System Technical Documentation"
Cohesion: 0.40
Nodes (4): Exemption from CLAUDE.local.md §0.2 file-header rule, Local divergence from the pinned reference (F0, 2026-07-16), Regeneration, `src/prompts_md/` — Embedded prompt data

### Community 21 - "ollama.rs"
Cohesion: 0.09
Nodes (33): a_mounted_prefix_survives_both_spellings(), a_root_that_really_ends_in_v1_is_read_as_the_prefix(), construction_keeps_the_real_credentials_on_the_authority(), every_accepted_spelling_yields_the_same_endpoints(), OllamaProvider, Client, Duration, Into (+25 more)

### Community 22 - "Completion"
Cohesion: 0.12
Nodes (16): an_external_implementor_can_report_real_telemetry_not_only_unmeasured(), BudgetBearing, Completion, CompletionTelemetry, elided(), every_contract_variant_has_its_own_retry_class(), production_half(), ReasoningState (+8 more)

### Community 23 - "MagiBuilder"
Cohesion: 0.09
Nodes (24): ComplexityGate, build_with(), build_with_honours_the_three_values_it_takes(), CapturingMockProvider, it_never_returns_err_no_matter_how_absurd_the_configuration(), Magi, MagiBuilder, Arc (+16 more)

### Community 24 - "proxy.rs"
Cohesion: 0.05
Nodes (62): B, Bytes, HeaderMap, Incoming, Infallible, Item, ProxyBody, a_backend_that_never_answers_is_cut_by_the_proxys_own_bound() (+54 more)

### Community 25 - "reporting.rs"
Cohesion: 0.07
Nodes (28): a_clean_run_gains_no_section_at_all(), a_fresh_record_measures_nothing_and_says_so(), a_report_from_before_this_version_also_gains_nothing(), a_report_from_the_previous_version_deserializes_as_none_not_zero(), a_report_with_no_completions_at_all_carries_no_key(), an_empty_pool_eligibility_is_still_written_to_the_wire(), an_exceeded_run_gains_exactly_one_section_naming_both_numbers(), an_older_report_without_the_field_still_deserializes() (+20 more)

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
Nodes (27): a_certificate_is_refused_over_a_table_that_established_nothing(), a_certificate_that_cannot_name_its_subject_or_its_cost_is_refused(), a_certificate_that_could_not_be_written_never_replaces_the_verdict(), a_certificate_with_no_large_payload_row_says_so_instead_of_omitting_it(), a_clean_tree_gets_the_certificate_written_at_cert_path(), a_corpus_over_the_unverified_threshold_warns_in_the_certificate(), a_dirty_tree_gets_no_certificate_not_a_caveated_one(), a_fixture_repository_is_removed_when_its_guard_drops() (+19 more)

### Community 34 - "[0.6.0] - 2026-05-21"
Cohesion: 0.33
Nodes (6): [0.6.0] - 2026-05-21, Backward compatibility, Changed, Pre-merge gates (CLAUDE.local.md §6), Security, Test count

### Community 35 - "[1.0.0] - 2026-05-24"
Cohesion: 0.40
Nodes (5): [1.0.0] - 2026-05-24, Added, Changed (breaking), Notes, Security

### Community 36 - "provider.rs"
Cohesion: 0.05
Nodes (40): Range, a_budget_inside_the_window_says_nothing(), a_budget_under_the_floor_is_still_reported(), a_limited_class_gets_two_attempts_not_four(), a_limited_count_above_the_general_one_is_reported(), a_refused_redirect_is_mage_local_and_never_retried(), a_single_honoured_wait_that_eats_the_whole_budget_is_flagged(), a_wait_exactly_equal_to_the_budget_is_flagged_too() (+32 more)

### Community 37 - "RoutingMockProvider"
Cohesion: 0.50
Nodes (3): leaky(), Error, String

### Community 38 - "build_user_prompt"
Cohesion: 0.14
Nodes (24): Sized, build_user_prompt(), fixed_nonce(), Result, Self, Vec, test_build_user_prompt_accepts_empty_content(), test_build_user_prompt_all_5_unicode_separators_positive_neutralization() (+16 more)

### Community 39 - "Changelog"
Cohesion: 0.18
Nodes (11): [0.1.2] - 2026-04-05, [0.3.1] - 2026-04-19, [1.1.1] - 2026-07-17, [2.2.0] - 2026-07-27, Changed, Changed, Changelog, Fixed (+3 more)

### Community 40 - "RunId"
Cohesion: 0.17
Nodes (14): RunId, build_with_absurd_timings(), chat_request(), ErosionProbe, probe_is_in_scope(), probe_window(), Error, Path (+6 more)

### Community 41 - "magi-core"
Cohesion: 0.17
Nodes (12): Changelog, Consensus Labels, Contribution, Credentials in a provider URL, Example, Feature Flags, Features, Implementing a Custom Provider (+4 more)

### Community 42 - "normalize_newlines"
Cohesion: 0.20
Nodes (12): neutralize_headers(), normalize_newlines(), Cow, String, sanitize_error_for_retry_feedback(), strip_invisibles(), test_neutralize_headers_not_bypassed_by_mongolian_vowel_separator(), test_neutralize_headers_preserves_unmatched_lines_borrowed() (+4 more)

### Community 43 - "e2.rs"
Cohesion: 0.08
Nodes (43): content_failure_detail(), e2_scenarios(), erosion_ctx(), r17_fails_rather_than_passing_vacuously_when_nothing_was_measured(), r17_fails_when_only_some_completions_saw_the_large_payload(), r17_fails_when_the_payload_silently_shrank(), r17_passes_when_the_payload_arrived_large(), r17_reads_the_measured_records_and_ignores_the_silent_ones() (+35 more)

### Community 44 - "String"
Cohesion: 0.24
Nodes (14): ActiveEntry, Candidate, EligibilityInputs, empty(), empty_s(), empty_wr(), inputs(), ModelCapability (+6 more)

### Community 45 - ".new"
Cohesion: 0.20
Nodes (30): make_agent(), make_consensus(), test_agent_display_fallback_to_agent_name_methods(), test_agent_line_format(), test_banner_all_lines_are_exactly_banner_width(), test_banner_consensus_line_includes_split_for_go_with_caveats(), test_banner_labels_are_column_aligned_to_max_label_len(), test_banner_lines_are_exactly_52_chars_wide() (+22 more)

### Community 46 - "Quick Start"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 47 - "error.rs"
Cohesion: 0.05
Nodes (26): a_contract_detail_is_capped_at_the_declared_length(), a_contract_detail_under_the_cap_is_untouched(), a_message_of_exactly_the_cap_is_kept_whole(), a_multi_byte_contract_detail_is_cut_without_panicking(), a_short_external_message_survives_untouched(), AbandonReason, an_oversized_external_message_is_cut_and_says_so(), empty_completion_remedy() (+18 more)

### Community 48 - "Lineage"
Cohesion: 0.14
Nodes (10): digest_collision(), Lineage, RotationEvent, RotationKind, Cow, Display, Formatter, From (+2 more)

### Community 49 - "RunContext"
Cohesion: 0.15
Nodes (32): assert_that(), Assertion, RunContext, analyze_produced_a_report(), answered_probes(), render_combinations(), Into, Option (+24 more)

### Community 50 - "FixedRng"
Cohesion: 0.40
Nodes (4): FixedRng, test_fastrand_source_returns_distinct_values_across_calls(), test_fixed_rng_panics_when_exhausted(), VecDeque

### Community 52 - "[0.3.0] - 2026-04-18"
Cohesion: 0.33
Nodes (6): [0.3.0] - 2026-04-18, Added, Changed (breaking), Dependencies, Not included (deferred beyond v0.3.0), Security considerations

### Community 54 - "backoff.rs"
Cohesion: 0.16
Nodes (25): FnMut, fixed(), next_backoff(), parse_retry_after(), RetryAfter, Duration, Option, String (+17 more)

### Community 55 - "Config"
Cohesion: 0.22
Nodes (12): Config, Duration, a_probe_answered_with_404_names_both_possible_causes_too(), a_probe_that_is_slow_names_both_possible_causes(), an_unsuccessful_status_is_not_described_as_a_successful_answer(), classify_probe_body(), Inconclusive, Probe (+4 more)

### Community 65 - "prompts/mod.rs"
Cohesion: 0.11
Nodes (14): lookup_prompt(), BTreeMap, Option, Result, String, test_guard_and_parser_agree_on_the_same_block(), test_seat_aware_validation_names_the_agent_and_mode(), test_the_error_is_actionable_on_its_own() (+6 more)

### Community 66 - ".cmp"
Cohesion: 0.50
Nodes (3): Ordering, Option, Self

### Community 67 - "s4_rotation_and_its_cause"
Cohesion: 0.19
Nodes (18): report_with_caspar_rotation_chain(), s4_cause_check_fails_on_a_hop_misclassified_as_schema(), s4_cause_check_fails_rather_than_skips_when_nothing_rotated(), s4_cause_check_passes_on_a_transport_classified_hop(), s4_ctx_with_rotation(), s4_fails_when_analyze_returned_a_typed_failure(), s4_rotated_check_fails_when_the_chain_is_empty_did_not_rotate(), s4_rotated_check_fails_when_the_hop_lands_on_the_same_lineage() (+10 more)

### Community 68 - "runner.rs"
Cohesion: 0.11
Nodes (15): SessionFacts, BackendNeed, classify_error(), ErrorClass, Scenario, shipped_timings(), Source, the_announced_runs_are_the_runs_this_stage_actually_launches() (+7 more)

### Community 69 - "paths.rs"
Cohesion: 0.24
Nodes (12): feature_matrix_target_dir(), metadata_target_dir(), nothing_the_harness_generates_lands_in_the_repo(), repo_root(), resolve_deepest_existing(), Path, PathBuf, Vec (+4 more)

### Community 70 - "main.rs"
Cohesion: 0.12
Nodes (23): a_genuine_skip_still_reports_exit_two(), a_healthy_run_can_reach_exit_zero(), a_run_that_could_not_start_is_never_a_verdict_about_the_crate(), a_scenario_filtered_out_by_no_backend_is_OUT_OF_SCOPE_not_omitted(), a_scenario_reading_one_run_never_sees_ANOTHER_runs_records(), absent_context(), dispatch_measured(), dispatching_alone_cannot_produce_a_receipt_the_estimate_never_preceded() (+15 more)

### Community 72 - "[3.0.0] - 2026-07-30"
Cohesion: 0.50
Nodes (4): [3.0.0] - 2026-07-30, Added, BREAKING, Changed

### Community 73 - "6. Modes of Operation"
Cohesion: 0.43
Nodes (6): fail(), fixture_meta(), prod_only(), self_test(), check_redaction.sh script, skeleton()

### Community 74 - "Model Rotation"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 75 - "LineageRegistry"
Cohesion: 0.10
Nodes (19): AgentSlotGuard, CrateDefectRecord, LineageRegistry, reg(), RegistryInner, Arc, Drop, Mutex (+11 more)

### Community 76 - "basic_analysis.rs"
Cohesion: 0.29
Nodes (14): create_provider(), main(), parse_mode(), print_usage(), ProviderArgs, read_input(), Arc, Box (+6 more)

### Community 78 - ".complete"
Cohesion: 0.19
Nodes (7): describe(), main(), MinimalProvider, MyBackend, ProviderError, Result, String

### Community 80 - ".new"
Cohesion: 0.28
Nodes (13): Future, a_receipt_BEFORE_the_runs_is_refused_even_though_the_estimate_was_announced(), a_receipt_is_refused_when_the_intervals_do_not_MATCH_the_announced_runs(), a_run_that_ends_badly_is_still_measured_so_the_receipt_is_not_withheld(), backend_runs_of(), CostLedger, Default, Output (+5 more)

### Community 81 - "[3.1.0] - 2026-07-31"
Cohesion: 0.40
Nodes (5): [3.1.0] - 2026-07-31, Changed, Documented, Fixed, Security

### Community 82 - "5. Data Schema and Consensus Protocol"
Cohesion: 0.40
Nodes (3): redacted(), String, X

### Community 83 - "main"
Cohesion: 0.16
Nodes (22): build_outcome(), Cli, crate_version(), git_commit(), main(), payload_size_target(), print_payload_size_answers_about_the_config_it_was_given(), repo_status() (+14 more)

### Community 85 - "s15_degradation_is_honest"
Cohesion: 0.31
Nodes (15): a_fired_injection(), report_from(), s15_ctx(), s15_degradation_is_honest(), s15_fails_the_named_agent_check_when_a_different_agent_was_injected(), s15_fails_the_strong_label_check_on_an_illegitimate_strong_label(), s15_fails_when_analyze_returned_a_typed_failure(), s15_names_the_same_four_properties_whether_it_asserts_or_skips() (+7 more)

### Community 86 - "CompletionRecord"
Cohesion: 0.33
Nodes (6): CompletionRecord, from_telemetry_copies_every_measurement_across(), from_telemetry_invents_nothing_when_nothing_was_measured(), Self, test_with_config_rejects_banner_width_too_small(), the_unsupported_declaration_survives_the_conversion()

### Community 87 - "e.rs"
Cohesion: 0.33
Nodes (8): e_scenarios(), no_report_skips_instead_of_passing(), report_with(), Vec, s_e2_fails_when_nothing_was_ruled_out(), s_e2_why_a_candidate_was_not_eligible(), s_e3_all_failing_conditions(), s_e3_fails_when_only_one_condition_is_reported()

### Community 88 - "AgentOutput"
Cohesion: 0.39
Nodes (3): AgentOutput, Vec, Verdict

### Community 89 - "AlwaysFailsExternally"
Cohesion: 0.29
Nodes (3): AlwaysFailsExternally, ProviderError, Result

### Community 90 - ".probe"
Cohesion: 0.33
Nodes (6): a_backend_that_does_not_hold_the_probe_model_is_INCONCLUSIVE_not_clear(), the_backend_step_and_the_probe_step_ask_different_questions(), the_contention_probe_sends_a_real_completion_not_a_manifest_listing(), the_window_bound_has_one_entry_per_request_the_preflight_makes(), stub_that_holds_no_model(), stub_that_records_requests()

### Community 91 - "[2.1.0] - 2026-07-27"
Cohesion: 0.67
Nodes (3): [2.1.0] - 2026-07-27, Added, Compatibility

### Community 92 - "[3.2.0] - 2026-08-10"
Cohesion: 0.67
Nodes (3): [3.2.0] - 2026-08-10, Added, Fixed

### Community 103 - "Debug"
Cohesion: 0.40
Nodes (3): redacted(), String, X

### Community 104 - "CertificateFacts"
Cohesion: 0.29
Nodes (8): CertificateFacts, iso_date_utc(), large_payload_priority(), Option, String, run_label(), unresolved(), SystemTime

### Community 107 - "outcome.rs"
Cohesion: 0.07
Nodes (21): a_hookless_unwind_cannot_inherit_an_earlier_panics_attribution(), a_panic_inside_a_run_is_caught_and_attributed_instead_of_killing_the_harness(), a_run_that_finishes_is_returned_untouched(), classify_panic(), crate_under_test_packages(), every_harness_only_dep_is_ABSENT_from_the_crate_under_tests_own_graph(), exit_code(), graph_query_target_dir() (+13 more)

### Community 108 - "Self"
Cohesion: 0.29
Nodes (4): 8. Evangelion Correspondence Table, 9. Relationship to the MAGI Python Plugin, MAGI System — Complete Technical Documentation, Multi-Perspective Analysis Library for Rust

### Community 109 - "Option"
Cohesion: 0.16
Nodes (10): Barrier, FailingProbe, MockProbe, MockProvider, OverlapProbe, AtomicUsize, Option, ProviderError (+2 more)

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
Nodes (44): a_body_without_choices_is_a_contract_failure_named_for_what_is_missing(), a_control_that_cannot_be_honoured_is_declared_even_when_reasoning_came_back(), a_filtered_reply_is_not_answered_with_raise_your_budget(), absent_usage_leaves_the_counters_unmeasured_rather_than_zero(), an_empty_completion_names_the_budget_that_cut_it(), content_with_text_is_returned(), into_completion_carries_the_telemetry_of_a_real_success(), OpenAiChoice (+36 more)

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
Cohesion: 0.23
Nodes (6): HostedModel, Option, ProviderError, Result, String, SidecarProbe

### Community 133 - "Instant"
Cohesion: 0.50
Nodes (4): 2.1 Conceptual Mapping, 2.2 Why Three Perspectives and Not Two or Five, 2.3 Addressing Cognitive Biases, 2. Translation to the Software Engineering Domain

### Community 134 - "testkit.rs"
Cohesion: 0.09
Nodes (27): a_broken_proxy_makes_the_preflight_say_cannot_test_not_failed(), a_cold_model_passes_on_the_second_probe_attempt(), an_endpoint_slow_every_time_still_reports_cannot_test(), AlwaysSlowStub, block_comment_depth_after(), BulkStub, EchoServer, ListingStub (+19 more)

### Community 137 - "String"
Cohesion: 0.50
Nodes (3): ProviderUrl, redacted(), String

### Community 138 - "Vec"
Cohesion: 0.50
Nodes (4): 3.1 Melchior — The Scientist, 3.2 Balthasar — The Pragmatist, 3.3 Caspar — The Critic, 3. The Three Agents in Detail

### Community 152 - "finding_id.rs"
Cohesion: 0.21
Nodes (13): de_category(), de_opt_file(), de_opt_line(), generate_finding_id(), normalize_category(), normalize_path(), D, Error (+5 more)

### Community 164 - "ProviderError"
Cohesion: 0.50
Nodes (4): 6.1 Code Review, 6.2 Design, 6.3 Analysis, 6. Modes of Operation

### Community 165 - "[4.0.0] - 2026-08-24"
Cohesion: 0.25
Nodes (8): [4.0.0] - 2026-08-24, Added, Changed, Fixed, Migration, One story, not two: the completion budget and the time budget, Removed, The defect this release exists for

### Community 166 - "Result"
Cohesion: 0.50
Nodes (4): Declaring fallbacks, Model Rotation, Ollama probe (feature `ollama`), What a rotation looks like

### Community 167 - "[1.0.1] - 2026-05-25"
Cohesion: 0.67
Nodes (3): [1.0.1] - 2026-05-25, Fixed, Internal

### Community 173 - "F"
Cohesion: 0.67
Nodes (3): Architecture, Module Dependency Graph, Prompt Injection Defense

### Community 180 - "Finding"
Cohesion: 0.18
Nodes (12): Finding, Into, String, test_agent_name_btreeset_orders_alphabetically(), test_agent_name_usable_as_btreemap_key(), test_finding_new_constructs_three_field_finding(), test_finding_new_defaults_optional_fields(), test_finding_serde_roundtrip() (+4 more)

### Community 181 - "preflight.rs"
Cohesion: 0.09
Nodes (34): a_backend_holding_every_seat_model_passes_the_check(), a_backend_listing_the_whole_trio_gets_past_the_backend_step(), a_config_that_is_not_a_full_trio_is_rejected_as_a_config_fault(), a_config_with_no_fallbacks_is_a_config_fault_not_a_crate_verdict(), a_config_without_seats_is_rejected_by_name_in_the_config_step(), a_fallback_sharing_a_seats_lineage_is_rejected(), a_fallback_sharing_a_seats_model_is_rejected(), a_listing_larger_than_the_cap_is_not_held_in_memory() (+26 more)

### Community 183 - "provider_url.rs"
Cohesion: 0.50
Nodes (3): leaky(), Error, String

### Community 188 - ".new_checked"
Cohesion: 0.40
Nodes (3): leaky(), Error, String

### Community 192 - "AgentName"
Cohesion: 0.18
Nodes (19): Condition, ConsensusResult, Dissent, BTreeMap, String, ExtractionFailure, InputSize, MagiReport (+11 more)

### Community 193 - "mock_server.rs"
Cohesion: 0.25
Nodes (12): CapturedRequest, Arc, JoinHandle, Mutex, Option, String, Value, Vec (+4 more)

### Community 194 - "retry_template"
Cohesion: 0.29
Nodes (7): a_budget_cut_before_the_opening_marker_is_also_attributed_to_the_budget(), a_cut_that_was_not_the_budget_also_keeps_the_old_wording(), a_known_budget_cut_stops_asking_the_model_not_to_stop(), an_unterminated_of_unknown_cause_keeps_the_old_wording(), missing_markers_without_a_budget_cut_keeps_its_own_wording(), retry_template(), Option

### Community 195 - "VerdictExtractionError"
Cohesion: 0.16
Nodes (15): Fn, extract(), locate(), locate_block(), Display, Error, Formatter, Result (+7 more)

### Community 196 - "config.rs"
Cohesion: 0.06
Nodes (50): FnOnce, a_budget_beyond_the_ceiling_is_rejected_and_the_field_is_named(), a_correctly_spelled_override_still_loads(), a_file_that_is_still_illegal_once_every_override_is_applied_is_refused(), a_file_that_omits_seats_and_fallbacks_gets_the_built_in_ones(), a_file_value_an_override_rescues_is_judged_on_the_final_set_not_the_file_alone(), a_set_of_overrides_that_is_still_illegal_at_the_end_is_refused(), a_zero_budget_names_the_budget_and_not_the_probe_timeout() (+42 more)

### Community 198 - "ProviderRequest"
Cohesion: 0.18
Nodes (7): X, X, parent_climbs_one_level_and_stops_at_the_root(), ProviderRequest, RequestBuilder, Self, T

### Community 199 - "RunResult"
Cohesion: 0.16
Nodes (18): Fallback, Seat, runner::Runner, RunOutcome, a_timed_out_run_reports_its_cap_verbatim(), attempts_for(), build_magi_against(), cannot_test_is_a_skip_with_a_reason_not_a_silent_empty_result() (+10 more)

### Community 204 - "git.rs"
Cohesion: 0.33
Nodes (4): Path, Result, String, status_porcelain()

### Community 211 - "body_bounds.rs"
Cohesion: 0.08
Nodes (39): TcpStream, a_chunked_probe_body_degrades_from_the_streaming_branch(), a_chunked_probe_body_under_the_cap_is_read_and_parsed(), a_probe_body_over_the_cap_degrades_instead_of_failing(), a_success_body_at_the_cap_is_read_whole(), a_success_body_over_the_cap_fails_rather_than_arriving_truncated(), a_verdict_body_that_is_not_utf8_fails_instead_of_being_mangled(), an_error_body_over_the_cap_keeps_its_prefix_and_announces_the_cut() (+31 more)

### Community 213 - "AssertionRow"
Cohesion: 0.17
Nodes (11): a_verdict_about_the_crate_outranks_our_own_failure_to_certify(), AssertionRow, both_renderers_name_the_cap_that_was_exceeded_rather_than_an_overrun(), format_row(), Report, row_to_json(), Duration, ExitCode (+3 more)

### Community 214 - ".render_human"
Cohesion: 0.18
Nodes (9): cycle_run(), a_harness_fault_reports_the_cycle_run_it_actually_was(), a_row_that_belongs_to_no_run_is_not_attributed_to_one(), a_time_failure_renders_visibly_different_from_an_assertion_failure(), both_renderers_carry_the_scenario_id_its_field_promises(), cycle_run_label(), CycleRun, every_rendered_row_carries_the_run_id_that_fed_it() (+1 more)

### Community 217 - "claude.rs"
Cohesion: 0.06
Nodes (54): a_non_text_block_with_a_normal_ending_is_not_reported_as_a_budget_cut(), a_non_text_contract_failure_names_the_shape_it_saw(), a_present_but_empty_thinking_payload_is_a_measured_zero_like_the_other_wire(), a_redacted_block_beside_a_readable_one_is_not_a_complete_measurement(), a_verdict_in_a_later_text_block_survives_an_empty_or_null_earlier_one(), an_anthropic_budget_cut_with_no_text_block_names_the_budget_not_a_broken_contract(), an_empty_text_block_beside_a_tool_use_is_not_a_budget_cut(), an_empty_text_block_is_an_empty_completion_like_the_other_wire() (+46 more)

### Community 220 - "magi-smoke — the smoke harness"
Cohesion: 0.10
Nodes (19): 10. `cargo audit` for the harness, 11.1 The captured measurements this harness is calibrated against, 11. The reproduction that justifies the harness, 1. Three exit codes, and the difference between two of them, 2. Two dependency modes, 3. What the proxy does, and why its red is never the crate's, 4. The contention probe, and its declared scope, 5. Where it fits in the cycle (+11 more)

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
Cohesion: 0.11
Nodes (19): build_retry_prompt(), test_build_retry_prompt_appends_feedback_block_exact_format(), test_build_retry_prompt_does_not_neutralize_midline_tokens(), test_build_retry_prompt_does_not_resanitize_content(), test_build_retry_prompt_feedback_block_after_end_delimiter(), test_build_retry_prompt_includes_seven_keys_list(), test_build_retry_prompt_neutralizes_dash_variant_retry_markers(), test_build_retry_prompt_neutralizes_injected_retry_feedback_marker() (+11 more)

### Community 233 - "ReportConfig"
Cohesion: 0.18
Nodes (11): ReportConfig, ReportError, Default, Display, Formatter, Result, test_new_checked_accepts_all_ascii_titles(), test_new_checked_rejects_banner_width_too_small() (+3 more)

### Community 238 - "Severity"
Cohesion: 0.24
Nodes (6): Ord, PartialOrd, Display, Formatter, Result, Severity

### Community 239 - "[1.1.0] - 2026-05-25"
Cohesion: 0.67
Nodes (3): [1.1.0] - 2026-05-25, Added, Notes

### Community 242 - "EventLog"
Cohesion: 0.13
Nodes (11): Attributes, Event, Field, Id, Metadata, Record, EventLog, FieldWriter (+3 more)

### Community 245 - "pattern8bconst_bad.rs"
Cohesion: 0.50
Nodes (4): build(), ProviderError, Self, sneak()

### Community 251 - "FallbackPool"
Cohesion: 0.18
Nodes (13): FallbackCandidate, FallbackPool, FallbackPoolBuilder, P, Self, test_duplicate_lineage_warns_but_builds(), test_empty_pool_is_valid_and_keeps_max_rotations(), test_push_probing_stores_both_views() (+5 more)

### Community 252 - "compose_transport_message"
Cohesion: 0.12
Nodes (11): f(), f(), f(), f(), P, Self, compose_caps_the_head_even_with_no_cause_chain(), compose_does_not_truncate_when_under_the_cap() (+3 more)

### Community 266 - "ProviderUrl"
Cohesion: 0.18
Nodes (9): Method, ends_with_segment_is_case_sensitive_and_ignores_a_trailing_slash(), ProviderUrl, redacted_hides_all_query_values_and_keeps_names(), redacted_hides_userinfo_and_keeps_host(), redacted_placeholder_is_url_safe_and_not_percent_encoded(), Client, Debug (+1 more)

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
Cohesion: 0.15
Nodes (14): X, a_partial_read_stays_within_the_cap_including_its_marker(), diagnostic_truncation_is_announced_and_utf8_safe(), diagnostic_under_the_cap_is_untouched(), mark_truncated(), marking_a_short_text_does_not_pad_it(), marking_never_splits_a_multibyte_character(), ProviderResponse (+6 more)

### Community 274 - "pattern8bcrate_good.rs"
Cohesion: 0.50
Nodes (4): bound_it(), build(), ProviderError, Self

### Community 276 - "check_prose_artifacts.sh"
Cohesion: 0.80
Nodes (4): assert_no_whitespace_paths(), scan(), scan_targets(), check_prose_artifacts.sh script

### Community 281 - "rotation.rs"
Cohesion: 0.12
Nodes (24): a_caller_holding_real_seat_state_produces_the_three_mid_run_causes(), a_seat_with_no_candidates_gets_an_empty_entry_not_a_missing_one(), a_spent_rotation_budget_is_reported_rather_than_read_as_eligible(), an_unmeasured_candidate_under_a_strict_guard_says_so(), cand(), CandidateEligibility, extract_item(), IneligibilityCause (+16 more)

### Community 282 - "CompletionConfig"
Cohesion: 0.09
Nodes (22): Ok, S, Serialize, cause_chain(), cause_chain_skips_the_top_level_error(), classify(), client_build_error(), CompletionConfig (+14 more)

### Community 283 - "e1.rs"
Cohesion: 0.07
Nodes (61): sha256_hex(), a_typed_crate_failure_is_a_verdict_about_the_crate(), an_unclassified_failure_is_read_as_the_crate_s(), blank_ctx(), e1_contains_exactly_the_scenarios_valid_against_3_2_0(), e1_scenarios(), preflight_error_for_stage(), record() (+53 more)

### Community 289 - "digest_case"
Cohesion: 0.14
Nodes (27): ae(), AgentRotationState, cap(), caps_map(), digest_case(), digest_case_self(), digest_case_two_active(), policy() (+19 more)

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

### Community 302 - "embedded_prompt_for"
Cohesion: 0.57
Nodes (7): balthasar_prompt(), caspar_prompt(), embedded_prompt_for(), melchior_prompt(), test_echo_canary_values_are_present_in_every_shipped_prompt(), test_prompts_match_pinned_reference_sha256(), test_shipped_prompts_satisfy_the_verdict_marker_contract()

### Community 309 - ".read_verdict_body"
Cohesion: 0.29
Nodes (5): body_cap(), Formatter, ProviderError, Result, send_composes_a_redacted_error_on_connection_failure()

### Community 311 - "provider_url.rs"
Cohesion: 0.14
Nodes (12): canonicalizing_does_not_touch_the_query_or_the_fragment(), debug_and_display_are_both_redacted(), join_path_is_idempotent_over_trailing_slash(), join_path_preserves_fragment(), join_path_preserves_query_and_appends_segments(), joining_a_segment_keeps_the_real_credentials(), joining_a_segment_produces_one_separator_not_two(), parse_error_never_echoes_the_raw_input() (+4 more)

### Community 324 - ".send"
Cohesion: 0.50
Nodes (3): ProviderError, Result, X

### Community 339 - "MagiError"
Cohesion: 0.07
Nodes (52): AbortHandle, DispatchOutcome, Elapsed, JoinError, ExternalErrorKind, MagiError, From, a_crate_defect_records_nothing_because_its_report_will_not_exist() (+44 more)

### Community 387 - "Migrating from 3.2.0 to 4.0.0"
Cohesion: 0.12
Nodes (16): 1. `complete()` returns `Completion`, not `String`, 2. `RotationKind` gains four variants and becomes `#[non_exhaustive]`, 3. `ProviderError::Http { status: 0 }` no longer exists, 4. `OllamaProvider` completes on `/api/chat`, 5. `CompletionConfig::max_tokens` defaults to `16_384`, 6. `MagiReport` gains `completions`, 7. The vendor termination vocabularies are fully translated, 8. `ClaudeProvider::parse_response` is gone (+8 more)

### Community 437 - "MockProvider"
Cohesion: 0.09
Nodes (14): AtomicU32, RetryClass, attempts_for(), MockProvider, RetryAfterProvider, RetryConfig, RetryProvider, Arc (+6 more)

### Community 445 - ".skip"
Cohesion: 0.29
Nodes (4): an_assertion_that_could_not_be_tested_carries_its_reason(), Into, Self, RunContext<'static>

## Knowledge Gaps
- **225 isolated node(s):** `magi-core`, `check_packaged_consumer.sh script`, `check_r0.sh script`, `ProviderError`, `ProviderError` (+220 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **46 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `AgentName` connect `AgentName` to `test_support.rs`, `consensus.rs`, `.new`, `LlmProvider`, `schema.rs`, `orchestrator.rs`, `MagiBuilder`, `rotation.rs`, `String`, `.new`, `embedded_prompt_for`, `RunContext`, `prompts/mod.rs`, `.cmp`, `config.rs`, `RunResult`, `LineageRegistry`, `MagiError`, `AgentOutput`, `ReportConfig`, `Severity`?**
  _High betweenness centrality (0.068) - this node is a cross-community bridge._
- **Why does `Config` connect `Config` to `config.rs`, `run`, `runner.rs`, `RunResult`, `RunId`, `.new`, `preflight.rs`?**
  _High betweenness centrality (0.066) - this node is a cross-community bridge._
- **Why does `RunId` connect `RunId` to `report.rs`, `config.rs`, `runner.rs`, `main.rs`, `RunResult`, `CertificateFacts`, `RunContext`, `preflight.rs`, `AssertionRow`, `Config`, `e1.rs`, `.skip`?**
  _High betweenness centrality (0.048) - this node is a cross-community bridge._
- **What connects `magi-core`, `check_packaged_consumer.sh script`, `check_r0.sh script` to the rest of the system?**
  _225 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `test_support.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.06 - nodes in this community are weakly interconnected._
- **Should `consensus.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.0928462709284627 - nodes in this community are weakly interconnected._
- **Should `validate.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.05980861244019139 - nodes in this community are weakly interconnected._