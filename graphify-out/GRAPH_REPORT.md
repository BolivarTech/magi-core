# Graph Report - MAGI-Core  (2026-08-20)

## Corpus Check
- 177 files · ~316,613 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 3415 nodes · 7648 edges · 411 communities (194 shown, 217 thin omitted)
- Extraction: 99% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 114 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `8413a9c4`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- test_support.rs
- fixtures.rs
- consensus.rs
- validate.rs
- .analyze
- ProviderError
- LlmProvider
- schema.rs
- ollama_wire.rs
- claude.rs
- claude_cli.rs
- consensus
- Balthasar — The Pragmatist
- MAGI System Technical Documentation
- Caspar — The Critic
- Melchior — The Scientist
- runner.rs
- orchestrator.rs
- Option
- MAGI System Technical Documentation
- Duration
- MagiError
- MagiBuilder
- proxy.rs
- reporting.rs
- magi_report_v0_3_1.json fixture
- make_output
- [0.5.0] - 2026-05-16
- verdict_markers.rs
- [0.2.0] - 2026-04-18
- lib.rs
- [0.4.0] - 2026-05-16
- report.rs
- [0.6.0] - 2026-05-21
- [1.0.0] - 2026-05-24
- provider_url.rs
- RoutingMockProvider
- build_user_prompt
- [1.1.1] - 2026-07-17
- ExternalErrorKind
- magi-core
- normalize_newlines
- e2.rs
- .new_checked
- .new
- Quick Start
- error.rs
- Lineage
- ExitCode
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
- prompts/mod.rs
- Path
- contract_prompt
- VerdictExtractionError
- MockProvider
- main.rs
- 3. The Three Agents in Detail
- 6. Modes of Operation
- Model Rotation
- rotation.rs
- From
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
- Self
- Formatter
- ProviderError
- outcome.rs
- Self
- basic_analysis.rs
- [1.1.0] - 2026-05-25
- BTreeMap
- Default
- Mutex
- Send
- Sync
- openai_compat.rs
- Into
- Option
- HostedModel
- Box
- BTreeMap
- Drop
- F
- HashMap
- P
- ProviderError
- String
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
- AgentName
- redacted
- redacted
- .leak
- redacted
- redacted
- .leak
- redacted
- finding_id.rs
- ProviderError
- run
- Result
- RequestBuilder
- Response
- Box
- [1.1.0] - 2026-05-25
- sync-fixtures.sh
- Drop
- F
- Finding
- Path
- provider.rs
- Result
- Option
- accept_one
- provider_url.rs
- AgentName
- pattern0nonterminal_bad.rs
- pattern0nonterminal_good.rs
- .new_checked
- String
- Vec
- SlowFailingProvider
- PathBuf
- Vec
- config.rs
- Display
- I
- .new
- Into
- Vec
- Completion
- .new
- git.rs
- CompletionRecord
- CompletionConfig
- .redacted
- Self
- PathBuf
- ProviderUrl
- serve_once
- Self
- ExitStatus
- magi-smoke — the smoke harness
- ScenarioState
- .new
- RunContext
- compose_transport_message
- magi-core
- Scenario
- BTreeSet
- Arc
- [2.0.0] - 2026-07-25
- Drop
- build_retry_prompt
- Arc
- BuildOutcome
- String
- .send
- Error
- report_text
- [1.1.0] - 2026-05-25
- Mutex
- ProviderProbe
- EventLog
- magi-smoke
- Default
- Completion
- RunResult
- .new
- ScenarioState
- log_failure
- ConsensusEngine
- RunSpec
- ProviderError
- Output
- I
- Duration
- .send
- described
- PathBuf
- [3.0.2] - 2026-07-30
- RunContext
- Scenario
- PreflightError
- pattern8bnospace_bad.rs
- pattern8bnospace_good.rs
- ProviderResponse
- Completion
- embedded_prompt_for
- CompletionConfig
- Cow
- Mutex
- Completion
- CompletionConfig
- body_bounds.rs
- TransparencyProbe
- e1.rs
- ExitCode
- Duration
- AgentName
- LlmProvider
- ProviderError
- Option
- EC fixtures — captured responses from Ollama, both wire formats
- .format_init_banner
- pattern7builderalias_bad.rs
- pattern7builderalias_good.rs
- pattern7default_bad.rs
- pattern7default_good.rs
- Duration
- LlmProvider
- LlmProvider
- weakened.rs
- ProviderError
- ReasoningControl
- ReasoningControl
- AgentOutput
- Completion
- AgentRotation
- Config
- ProviderRequest
- Drop
- .parse
- RequestBuilder
- CompletionConfig
- Path
- ProviderError
- Box
- redaction_negative.rs
- BTreeSet
- String
- Drop
- RequestRecord
- tempdir_with
- BTreeMap
- MockProvider
- Arc
- Error
- LlmProvider
- Option
- ExtractionFailureCause
- Assertion
- HashMap
- Option
- PathBuf
- Result
- String
- T
- ProviderError
- FinishReason
- Completion
- Response
- CompletionConfig
- Assertion
- Option
- RunSpec
- Url
- JoinHandle
- RunId
- LlmProvider
- .cross_disk_against_manifest
- ProviderError
- PreflightError
- check_pending.sh
- Completion
- Cow
- P
- Debug
- MagiConfig
- AgentName
- .format_dissent
- Duration
- Lineage
- Mode
- Formatter
- Sync
- Option
- Instant
- Sync
- Config
- Debug
- Output
- RunId
- Into
- LlmProvider
- Result
- Display
- String
- Formatter
- Path
- preflight.rs
- AgentOutput
- Result
- F
- MagiError
- Vec
- Migrating from 3.2.0 to 4.0.0
- AgentName
- Self
- MagiReport
- ProviderError
- AtomicUsize
- BTreeSet
- SpyProxy
- String
- Default
- JoinHandle
- HashMap
- Mutex
- FinishReason
- RunId
- ProviderProbe
- ProviderError
- AgentName
- Default
- Duration
- Self
- T
- I
- PathBuf
- MagiReport

## God Nodes (most connected - your core abstractions)
1. `ProviderError` - 60 edges
2. `blank_ctx()` - 60 edges
3. `MagiBuilder` - 43 edges
4. `LlmProvider` - 41 edges
5. `MagiError` - 38 edges
6. `make_consensus()` - 35 edges
7. `RunContext` - 34 edges
8. `make_agent()` - 34 edges
9. `Completion` - 34 edges
10. `Config` - 32 edges

## Surprising Connections (you probably didn't know these)
- `an_oversized_response_is_mage_local_and_the_run_completes()` --calls--> `build_oversized_case()`  [INFERRED]
  tests/rotation_integration.rs → src/test_support.rs
- `test_schema_fail_is_mage_local_not_run_wide()` --calls--> `build_schema_local_case()`  [INFERRED]
  tests/rotation_integration.rs → src/test_support.rs
- `an_external_failure_rotates_the_seat_and_the_run_completes()` --calls--> `build_trio_with_caspar()`  [INFERRED]
  tests/rotation_integration.rs → src/test_support.rs
- `test_panic_never_rotates()` --calls--> `build_trio_with_caspar()`  [INFERRED]
  tests/rotation_integration.rs → src/test_support.rs
- `test_report_shows_and_omits_model_rotations_section()` --calls--> `build_trio_with_caspar()`  [INFERRED]
  tests/rotation_integration.rs → src/test_support.rs

## Import Cycles
- None detected.

## Communities (411 total, 217 thin omitted)

### Community 0 - "test_support.rs"
Cohesion: 0.06
Nodes (48): HashMap, Magi, Mutex, Beh, build_oversized_case(), build_schema_local_case(), build_trio_with_caspar(), build_two_5xx_with_local_fallbacks() (+40 more)

### Community 1 - "fixtures.rs"
Cohesion: 0.20
Nodes (20): a_file_on_disk_that_nobody_declares_is_an_orphan_too(), a_fixture_whose_hash_changed_is_rejected(), a_manifest_path_that_escapes_the_corpus_is_refused(), a_missing_fixtures_directory_is_reported_not_silently_clean(), a_nested_corpus_is_refused_because_the_walk_does_not_recurse(), a_readable_entry_is_still_crossed_against_the_manifest(), a_sync_that_cannot_finish_leaves_the_tracked_manifest_alone(), a_verified_by_naming_a_live_scenario_is_accepted() (+12 more)

### Community 2 - "consensus.rs"
Cohesion: 0.14
Nodes (49): make_output(), Result, test_agent_count_reflects_input_count(), test_approve_conditional_reject_produces_go_with_caveats(), test_conditions_are_distinct_from_recommendations_section(), test_conditions_extracted_from_conditional_agents(), test_conditions_use_summary_field_not_recommendation_field(), test_confidence_formula_clamped_and_rounded() (+41 more)

### Community 3 - "validate.rs"
Cohesion: 0.06
Nodes (40): clean_title(), finding_with_title(), output_with_confidence(), output_with_findings(), Default, Result, Self, String (+32 more)

### Community 4 - ".analyze"
Cohesion: 0.09
Nodes (48): MagiReport, a_completion_record_is_built_in_exactly_one_place(), a_completion_that_failed_is_recorded_too(), a_measured_candidate_keeps_the_strict_guard_quiet(), a_run_with_no_cuts_still_records_one_entry_per_completion(), a_strict_guard_with_nothing_measured_warns_and_still_completes(), exceeding_the_threshold_warns_and_still_completes(), probe_declaration_warnings_are_told_once_per_instance() (+40 more)

### Community 5 - "ProviderError"
Cohesion: 0.09
Nodes (19): describe(), main(), MinimalProvider, MyBackend, Result, String, Instant, Ok (+11 more)

### Community 6 - "LlmProvider"
Cohesion: 0.10
Nodes (32): Path, Agent, AgentFactory, MockProvider, AgentName, Arc, AtomicUsize, BTreeMap (+24 more)

### Community 7 - "schema.rs"
Cohesion: 0.04
Nodes (14): make_output(), test_agent_name_btreeset_orders_alphabetically(), test_agent_name_usable_as_btreemap_key(), test_agent_output_conditional_is_not_dissenting_from_approve_majority(), test_agent_output_effective_verdict_maps_conditional_to_approve(), test_agent_output_empty_findings_valid(), test_agent_output_is_dissenting_when_verdict_differs_from_majority(), test_agent_output_is_not_dissenting_when_verdict_matches_majority() (+6 more)

### Community 8 - "ollama_wire.rs"
Cohesion: 0.05
Nodes (46): a_mounted_prefix_survives_both_spellings(), a_root_that_really_ends_in_v1_is_read_as_the_prefix(), construction_keeps_the_real_credentials_on_the_authority(), every_accepted_spelling_yields_the_same_endpoints(), OllamaProvider, Client, Duration, Into (+38 more)

### Community 9 - "claude.rs"
Cohesion: 0.09
Nodes (40): ClaudeMessage, ClaudeProvider, ClaudeRequest, ClaudeResponse, ClaudeUsage, ContentBlock, Client, Debug (+32 more)

### Community 11 - "claude_cli.rs"
Cohesion: 0.11
Nodes (36): ClaudeCliProvider, CliOutput, CliUsage, parse_completion(), parse_envelope(), F, Into, Option (+28 more)

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
Cohesion: 0.10
Nodes (25): AgentName, Config, MagiError, RequestRecord, RunOutcome, a_timed_out_run_reports_its_cap_verbatim(), attempts_for(), cannot_test_is_a_skip_with_a_reason_not_a_silent_empty_result() (+17 more)

### Community 18 - "orchestrator.rs"
Cohesion: 0.07
Nodes (64): AbortHandle, ExtractionFailureCause, a_crate_defect_records_nothing_because_its_report_will_not_exist(), a_probe_declaring_another_model_is_named_not_rejected(), a_probe_declaring_its_own_model_is_quiet(), a_probe_that_claims_nothing_is_checked_against_nothing(), a_threshold_is_unreachable_when_the_smallest_warning_input_is_already_rejected(), a_threshold_that_can_never_fire_is_detected() (+56 more)

### Community 20 - "MAGI System Technical Documentation"
Cohesion: 0.40
Nodes (4): Exemption from CLAUDE.local.md §0.2 file-header rule, Local divergence from the pinned reference (F0, 2026-07-16), Regeneration, `src/prompts_md/` — Embedded prompt data

### Community 21 - "Duration"
Cohesion: 0.11
Nodes (18): Duration, Error, PayloadError, Result, RunId, AlwaysFailsExternally, Vec, run_label() (+10 more)

### Community 22 - "MagiError"
Cohesion: 0.10
Nodes (49): Agent, AgentOutput, AgentRotation, BTreeMap, CompletionRecord, CrateDefectRecord, DispatchOutcome, Elapsed (+41 more)

### Community 23 - "MagiBuilder"
Cohesion: 0.10
Nodes (23): AgentFactory, Box, ComplexityGate, ConsensusConfig, ConsensusEngine, FallbackPool, P, ReportConfig (+15 more)

### Community 24 - "proxy.rs"
Cohesion: 0.06
Nodes (56): AtomicBool, B, Bytes, X, Client, HeaderMap, Incoming, Infallible (+48 more)

### Community 25 - "reporting.rs"
Cohesion: 0.07
Nodes (25): DedupFinding, a_fresh_record_measures_nothing_and_says_so(), a_recorded_cut_is_not_an_extraction_failure(), a_report_from_the_previous_version_deserializes_as_none_not_zero(), a_report_with_no_completions_at_all_carries_no_key(), an_older_report_without_the_field_still_deserializes(), cause_label(), estimate_tokens() (+17 more)

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

### Community 31 - "lib.rs"
Cohesion: 0.50
Nodes (4): [3.0.1] - 2026-07-30, Changed, Fixed, Notes

### Community 32 - "[0.4.0] - 2026-05-16"
Cohesion: 0.29
Nodes (7): [0.4.0] - 2026-05-16, Added, Backward compatibility, Changed, Documentation, Performance, Test count

### Community 33 - "report.rs"
Cohesion: 0.08
Nodes (50): ExitCode, FixtureSummary, Option, ScenarioState, a_certificate_that_cannot_name_its_subject_or_its_cost_is_refused(), a_certificate_that_could_not_be_written_never_replaces_the_verdict(), a_certificate_with_no_large_payload_row_says_so_instead_of_omitting_it(), a_clean_tree_gets_the_certificate_written_at_cert_path() (+42 more)

### Community 34 - "[0.6.0] - 2026-05-21"
Cohesion: 0.33
Nodes (6): [0.6.0] - 2026-05-21, Backward compatibility, Changed, Pre-merge gates (CLAUDE.local.md §6), Security, Test count

### Community 35 - "[1.0.0] - 2026-05-24"
Cohesion: 0.40
Nodes (5): [1.0.0] - 2026-05-24, Added, Changed (breaking), Notes, Security

### Community 36 - "provider_url.rs"
Cohesion: 0.15
Nodes (8): a_partial_read_stays_within_the_cap_including_its_marker(), body_cap(), diagnostic_truncation_is_announced_and_utf8_safe(), diagnostic_under_the_cap_is_untouched(), mark_truncated(), marking_a_short_text_does_not_pad_it(), marking_never_splits_a_multibyte_character(), truncate_diagnostic()

### Community 37 - "RoutingMockProvider"
Cohesion: 0.50
Nodes (3): leaky(), Error, String

### Community 38 - "build_user_prompt"
Cohesion: 0.13
Nodes (26): Sized, build_user_prompt(), fixed_nonce(), MagiError, Mode, Result, Self, Vec (+18 more)

### Community 39 - "[1.1.1] - 2026-07-17"
Cohesion: 0.18
Nodes (11): [0.1.2] - 2026-04-05, [0.3.1] - 2026-04-19, [2.2.0] - 2026-07-27, [3.2.0] - 2026-08-10, Added, Changed, Changelog, Fixed (+3 more)

### Community 41 - "magi-core"
Cohesion: 0.17
Nodes (12): Changelog, Consensus Labels, Contribution, Credentials in a provider URL, Example, Feature Flags, Features, Implementing a Custom Provider (+4 more)

### Community 42 - "normalize_newlines"
Cohesion: 0.22
Nodes (9): neutralize_headers(), normalize_newlines(), sanitize_error_for_retry_feedback(), strip_invisibles(), test_neutralize_headers_not_bypassed_by_mongolian_vowel_separator(), test_neutralize_headers_preserves_unmatched_lines_borrowed(), test_normalize_newlines_handles_empty_string(), test_normalize_newlines_preserves_existing_lf_borrows() (+1 more)

### Community 43 - "e2.rs"
Cohesion: 0.09
Nodes (51): Assertion, RunContext, Scenario, assert_that(), content_failure_detail(), e2_scenarios(), erosion_ctx(), r17_fails_rather_than_passing_vacuously_when_nothing_was_measured() (+43 more)

### Community 44 - ".new_checked"
Cohesion: 0.22
Nodes (8): ReportError, Result, test_new_checked_accepts_all_ascii_titles(), test_new_checked_rejects_banner_width_too_small(), test_new_checked_rejects_non_ascii_display_name(), test_new_checked_rejects_non_ascii_title_field(), test_with_config_rejects_banner_width_too_small(), test_with_config_rejects_non_ascii_agent_title()

### Community 45 - ".new"
Cohesion: 0.19
Nodes (31): make_agent(), make_consensus(), test_agent_display_fallback_to_agent_name_methods(), test_agent_line_format(), test_banner_all_lines_are_exactly_banner_width(), test_banner_consensus_line_includes_split_for_go_with_caveats(), test_banner_labels_are_column_aligned_to_max_label_len(), test_banner_lines_are_exactly_52_chars_wide() (+23 more)

### Community 46 - "Quick Start"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 47 - "error.rs"
Cohesion: 0.05
Nodes (17): Display, Formatter, Into, an_assertion_that_could_not_be_tested_carries_its_reason(), a_contract_detail_is_capped_at_the_declared_length(), a_contract_detail_under_the_cap_is_untouched(), a_message_of_exactly_the_cap_is_kept_whole(), a_multi_byte_contract_detail_is_cut_without_panicking() (+9 more)

### Community 48 - "Lineage"
Cohesion: 0.14
Nodes (15): Cow, FallbackCandidate, FallbackPoolBuilder, Lineage, ProviderProbe, RotationEvent, RotationKind, Display (+7 more)

### Community 50 - "FixedRng"
Cohesion: 0.22
Nodes (7): FastrandSource, FixedRng, RngLike, Send, test_fastrand_source_returns_distinct_values_across_calls(), test_fixed_rng_panics_when_exhausted(), VecDeque

### Community 52 - ".validate"
Cohesion: 0.33
Nodes (6): [0.3.0] - 2026-04-18, Added, Changed (breaking), Dependencies, Not included (deferred beyond v0.3.0), Security considerations

### Community 54 - "backoff.rs"
Cohesion: 0.16
Nodes (26): FnMut, fixed(), next_backoff(), parse_retry_after(), RetryAfter, RetryClass, Duration, Option (+18 more)

### Community 65 - "prompts/mod.rs"
Cohesion: 0.11
Nodes (14): lookup_prompt(), BTreeMap, Option, Result, String, test_guard_and_parser_agree_on_the_same_block(), test_seat_aware_validation_names_the_agent_and_mode(), test_the_error_is_actionable_on_its_own() (+6 more)

### Community 67 - "contract_prompt"
Cohesion: 0.27
Nodes (9): contract_prompt(), Mode, test_analyze_applies_mode_agnostic_override_to_melchior(), test_analyze_per_mode_override_supersedes_all_modes(), test_build_aborts_on_a_corrupt_custom_prompt_before_any_provider_call(), test_legacy_with_custom_prompt_delegates_to_for_mode(), test_legacy_with_custom_prompt_shim_roundtrip(), test_with_custom_prompt_all_modes_stores_with_none_key() (+1 more)

### Community 68 - "VerdictExtractionError"
Cohesion: 0.16
Nodes (16): Fn, extract(), ExtractionFailureCause, locate(), locate_block(), Display, Error, Formatter (+8 more)

### Community 69 - "MockProvider"
Cohesion: 0.13
Nodes (10): AtomicU32, MockProvider, RetryAfterProvider, RetryConfig, RetryProvider, Arc, AtomicUsize, Default (+2 more)

### Community 70 - "main.rs"
Cohesion: 0.09
Nodes (41): AssertionRow, CycleRun, Output, a_genuine_skip_still_reports_exit_two(), a_healthy_run_can_reach_exit_zero(), a_run_that_could_not_start_is_never_a_verdict_about_the_crate(), a_scenario_filtered_out_by_no_backend_is_OUT_OF_SCOPE_not_omitted(), a_scenario_reading_one_run_never_sees_ANOTHER_runs_records() (+33 more)

### Community 72 - "3. The Three Agents in Detail"
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

### Community 104 - "Self"
Cohesion: 0.16
Nodes (13): Deserialize, Serialize, an_external_implementor_can_report_real_telemetry_not_only_unmeasured(), CompletionTelemetry, finish_reason_keeps_an_unknown_wire_value_instead_of_dropping_it(), finish_reason_other_is_capped_at_64_chars_on_a_char_boundary(), FinishReason, ReasoningState (+5 more)

### Community 107 - "outcome.rs"
Cohesion: 0.08
Nodes (16): F, a_hookless_unwind_cannot_inherit_an_earlier_panics_attribution(), a_panic_inside_a_run_is_caught_and_attributed_instead_of_killing_the_harness(), a_run_that_finishes_is_returned_untouched(), classify_panic(), crate_under_test_packages(), every_harness_only_dep_is_ABSENT_from_the_crate_under_tests_own_graph(), exit_code() (+8 more)

### Community 108 - "Self"
Cohesion: 0.29
Nodes (4): 8. Evangelion Correspondence Table, 9. Relationship to the MAGI Python Plugin, MAGI System — Complete Technical Documentation, Multi-Perspective Analysis Library for Rust

### Community 109 - "basic_analysis.rs"
Cohesion: 0.29
Nodes (14): create_provider(), main(), parse_mode(), print_usage(), ProviderArgs, read_input(), Arc, Box (+6 more)

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
Cohesion: 0.08
Nodes (42): a_body_without_choices_is_a_contract_failure_named_for_what_is_missing(), a_control_that_cannot_be_honoured_is_declared_even_when_reasoning_came_back(), absent_usage_leaves_the_counters_unmeasured_rather_than_zero(), an_empty_completion_names_the_budget_that_cut_it(), content_with_text_is_returned(), into_completion_carries_the_telemetry_of_a_real_success(), OpenAiChoice, OpenAiCompatibleProvider (+34 more)

### Community 117 - "Into"
Cohesion: 0.38
Nodes (5): build(), ProviderError, Error, From, Self

### Community 118 - "Option"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 121 - "HostedModel"
Cohesion: 0.23
Nodes (5): HostedModel, Option, Result, String, SidecarProbe

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

### Community 129 - "String"
Cohesion: 0.25
Nodes (13): Condition, ConsensusResult, ExtractionFailure, MagiReport, ReportConfig, ReportFormatter, AgentName, AgentOutput (+5 more)

### Community 132 - "LineageRegistry"
Cohesion: 0.15
Nodes (13): ActiveEntry, AgentSlotGuard, CrateDefectRecord, LineageRegistry, RegistryInner, RotationConfig, AgentName, Arc (+5 more)

### Community 133 - "Instant"
Cohesion: 0.50
Nodes (4): 2.1 Conceptual Mapping, 2.2 Why Three Perspectives and Not Two or Five, 2.3 Addressing Cognitive Biases, 2. Translation to the Software Engineering Domain

### Community 134 - "testkit.rs"
Cohesion: 0.05
Nodes (48): Announcement, AsRef, AtomicUsize, Deref, PreflightError, a_backend_that_does_not_hold_the_probe_model_is_INCONCLUSIVE_not_clear(), a_broken_proxy_makes_the_preflight_say_cannot_test_not_failed(), a_cold_model_passes_on_the_second_probe_attempt() (+40 more)

### Community 137 - "String"
Cohesion: 0.50
Nodes (3): ProviderUrl, redacted(), String

### Community 138 - "Vec"
Cohesion: 0.50
Nodes (4): 3.1 Melchior — The Scientist, 3.2 Balthasar — The Pragmatist, 3.3 Caspar — The Critic, 3. The Three Agents in Detail

### Community 144 - "AgentName"
Cohesion: 0.16
Nodes (17): Condition, ConsensusResult, dedup_key(), DedupFinding, DedupKey, Dissent, finding_key(), BTreeMap (+9 more)

### Community 152 - "finding_id.rs"
Cohesion: 0.23
Nodes (13): D, de_category(), de_opt_file(), de_opt_line(), generate_finding_id(), normalize_category(), normalize_path(), Error (+5 more)

### Community 164 - "ProviderError"
Cohesion: 0.50
Nodes (4): 6.1 Code Review, 6.2 Design, 6.3 Analysis, 6. Modes of Operation

### Community 165 - "run"
Cohesion: 0.08
Nodes (42): Debug, Field, a_backend_listing_the_whole_trio_gets_past_the_backend_step(), a_listing_larger_than_the_cap_is_not_held_in_memory(), a_probe_answered_with_404_names_both_possible_causes_too(), a_probe_that_is_slow_names_both_possible_causes(), a_rotation_candidate_the_backend_does_not_hold_is_refused_too(), a_seat_model_the_backend_does_not_hold_is_refused_before_any_scenario_runs() (+34 more)

### Community 166 - "Result"
Cohesion: 0.50
Nodes (4): Declaring fallbacks, Model Rotation, Ollama probe (feature `ollama`), What a rotation looks like

### Community 167 - "RequestBuilder"
Cohesion: 0.67
Nodes (3): [1.0.1] - 2026-05-25, Fixed, Internal

### Community 168 - "Response"
Cohesion: 0.67
Nodes (3): [1.1.1] - 2026-07-17, Changed, Fixed

### Community 173 - "F"
Cohesion: 0.67
Nodes (3): Architecture, Module Dependency Graph, Prompt Injection Defense

### Community 174 - "Finding"
Cohesion: 0.19
Nodes (10): Ord, Ordering, PartialOrd, Finding, Into, Option, Self, String (+2 more)

### Community 177 - "provider.rs"
Cohesion: 0.05
Nodes (34): f(), f(), f(), f(), RetryClass, a_refused_redirect_is_mage_local_and_never_retried(), cause_chain(), cause_chain_skips_the_top_level_error() (+26 more)

### Community 181 - "accept_one"
Cohesion: 0.36
Nodes (8): TcpStream, accept_one(), bind_loopback(), ends_header(), hang_up(), Option, String, TcpListener

### Community 183 - "provider_url.rs"
Cohesion: 0.50
Nodes (3): leaky(), Error, String

### Community 188 - ".new_checked"
Cohesion: 0.40
Nodes (3): leaky(), Error, String

### Community 196 - "config.rs"
Cohesion: 0.07
Nodes (50): Default, FnOnce, a_budget_beyond_the_ceiling_is_rejected_and_the_field_is_named(), a_correctly_spelled_override_still_loads(), a_file_that_is_still_illegal_once_every_override_is_applied_is_refused(), a_file_that_omits_seats_and_fallbacks_gets_the_built_in_ones(), a_file_value_an_override_rescues_is_judged_on_the_final_set_not_the_file_alone(), a_set_of_overrides_that_is_still_illegal_at_the_end_is_refused() (+42 more)

### Community 199 - ".new"
Cohesion: 0.23
Nodes (24): test_base_zero_with_three_retries_emits_exactly_four_requests(), test_budget_exhaustion_abandons_with_typed_reason(), test_dangerous_config_is_announced_for_retry_after_cap_over_budget(), test_dangerous_config_is_announced_for_zero_base_delay(), test_dangerous_config_is_announced_for_zero_cap(), test_dangerous_config_is_announced_for_zero_retry_after_cap(), test_honored_retry_after_can_overrun_a_small_budget(), test_max_retries_zero_does_not_retry() (+16 more)

### Community 204 - "git.rs"
Cohesion: 0.33
Nodes (4): Path, Result, String, status_porcelain()

### Community 210 - "CompletionRecord"
Cohesion: 0.32
Nodes (8): CompletionTelemetry, FinishReason, ReasoningState, CompletionRecord, from_telemetry_copies_every_measurement_across(), from_telemetry_invents_nothing_when_nothing_was_measured(), Self, the_unsupported_declaration_survives_the_conversion()

### Community 212 - ".redacted"
Cohesion: 0.17
Nodes (8): Method, redacted_hides_all_query_values_and_keeps_names(), redacted_hides_userinfo_and_keeps_host(), redacted_placeholder_is_url_safe_and_not_percent_encoded(), Client, Formatter, Result, send_composes_a_redacted_error_on_connection_failure()

### Community 214 - "PathBuf"
Cohesion: 0.29
Nodes (10): PathBuf, feature_matrix_target_dir(), metadata_target_dir(), nothing_the_harness_generates_lands_in_the_repo(), repo_root(), resolve_deepest_existing(), smoke_dir(), the_certificate_is_the_one_declared_exception_and_it_is_named() (+2 more)

### Community 217 - "serve_once"
Cohesion: 0.42
Nodes (9): a_client_configured_the_way_this_crate_does_it_leaks_nothing(), authorization_is_stripped_across_origins(), authorization_survives_a_same_origin_redirect(), redirect_to(), Option, String, TcpListener, serve_once() (+1 more)

### Community 218 - "Self"
Cohesion: 0.22
Nodes (4): Drop, Self, EnvVarGuard, RunContext<'static>

### Community 220 - "magi-smoke — the smoke harness"
Cohesion: 0.14
Nodes (13): 10. `cargo audit` for the harness, 11. The reproduction that justifies the harness, 1. Three exit codes, and the difference between two of them, 2. Two dependency modes, 3. What the proxy does, and why its red is never the crate's, 4. The contention probe, and its declared scope, 5. Where it fits in the cycle, 6. Cost per run (+5 more)

### Community 224 - "compose_transport_message"
Cohesion: 0.50
Nodes (3): Error, String, spawn_failure()

### Community 228 - "BTreeSet"
Cohesion: 0.40
Nodes (5): empty(), empty_s(), empty_wr(), BTreeSet, test_next_model_is_deterministic()

### Community 230 - "[2.0.0] - 2026-07-25"
Cohesion: 0.50
Nodes (4): [2.0.0] - 2026-07-25, Added, BREAKING, Changed

### Community 232 - "build_retry_prompt"
Cohesion: 0.09
Nodes (26): a_cut_that_was_not_the_budget_also_keeps_the_old_wording(), a_known_budget_cut_stops_asking_the_model_not_to_stop(), an_unterminated_of_unknown_cause_keeps_the_old_wording(), build_retry_prompt(), retry_template(), ExtractionFailureCause, Option, String (+18 more)

### Community 235 - "String"
Cohesion: 0.09
Nodes (16): Barrier, digest_collision(), FailingProbe, MockProbe, MockProvider, OverlapProbe, AtomicUsize, Completion (+8 more)

### Community 236 - ".send"
Cohesion: 0.50
Nodes (3): ProviderError, Result, X

### Community 238 - "report_text"
Cohesion: 0.32
Nodes (7): a_clean_run_gains_no_section_at_all(), a_report_from_before_this_version_also_gains_nothing(), an_exceeded_run_gains_exactly_one_section_naming_both_numbers(), InputSize, report_text(), Option, the_section_is_the_only_thing_an_exceeded_run_adds()

### Community 239 - "[1.1.0] - 2026-05-25"
Cohesion: 0.67
Nodes (3): [1.1.0] - 2026-05-25, Added, Notes

### Community 242 - "EventLog"
Cohesion: 0.17
Nodes (9): Attributes, Event, Id, Metadata, Record, EventLog, FieldWriter, Subscriber (+1 more)

### Community 251 - ".new"
Cohesion: 0.14
Nodes (16): FallbackPool, strict_guard_is_inert(), test_duplicate_lineage_warns_but_builds(), test_empty_pool_is_valid_and_keeps_max_rotations(), test_lineage_ord_for_btree_keys(), test_push_probing_stores_both_views(), test_push_with_probe_accepts_a_provider_that_cannot_probe(), test_push_with_probe_matches_push_probing_for_the_same_object() (+8 more)

### Community 258 - "ConsensusEngine"
Cohesion: 0.33
Nodes (4): ConsensusConfig, ConsensusEngine, Default, Self

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
Cohesion: 0.27
Nodes (6): X, ProviderResponse, push_within_cap(), Option, String, Vec

### Community 275 - "embedded_prompt_for"
Cohesion: 0.57
Nodes (7): balthasar_prompt(), caspar_prompt(), embedded_prompt_for(), melchior_prompt(), test_echo_canary_values_are_present_in_every_shipped_prompt(), test_prompts_match_pinned_reference_sha256(), test_shipped_prompts_satisfy_the_verdict_marker_contract()

### Community 281 - "body_bounds.rs"
Cohesion: 0.28
Nodes (12): a_chunked_probe_body_degrades_from_the_streaming_branch(), a_chunked_probe_body_under_the_cap_is_read_and_parsed(), a_probe_body_over_the_cap_degrades_instead_of_failing(), a_success_body_at_the_cap_is_read_whole(), a_success_body_over_the_cap_fails_rather_than_arriving_truncated(), a_verdict_body_that_is_not_utf8_fails_instead_of_being_mangled(), an_error_body_over_the_cap_keeps_its_prefix_and_announces_the_cut(), Framing (+4 more)

### Community 283 - "e1.rs"
Cohesion: 0.05
Nodes (114): RotationKind, sha256_hex(), Assertion, RunContext, a_fired_injection(), a_typed_crate_failure_is_a_verdict_about_the_crate(), an_unclassified_failure_is_read_as_the_crate_s(), analyze_produced_a_report() (+106 more)

### Community 289 - "Option"
Cohesion: 0.17
Nodes (28): ae(), AgentRotation, AgentRotationState, Candidate, cap(), caps_map(), digest_case(), digest_case_self() (+20 more)

### Community 290 - "EC fixtures — captured responses from Ollama, both wire formats"
Cohesion: 0.40
Nodes (4): EC fixtures — captured responses from Ollama, both wire formats, Files, The cold-start experiment: **GO**, Two findings the experiment was not looking for

### Community 292 - ".format_init_banner"
Cohesion: 0.40
Nodes (3): Mode, test_format_init_banner_shows_mode_model_timeout(), test_separator_format()

### Community 293 - "pattern7builderalias_bad.rs"
Cohesion: 0.67
Nodes (3): bare(), configured(), Client

### Community 294 - "pattern7builderalias_good.rs"
Cohesion: 0.67
Nodes (3): also_configured(), configured(), Client

### Community 301 - "weakened.rs"
Cohesion: 0.18
Nodes (10): a_source_file_that_cannot_be_decoded_is_reported_too(), an_unreadable_path_is_reported_and_never_skipped(), every_mark_outside_this_guards_own_fixtures_is_well_formed(), Path, Result, String, Vec, scan() (+2 more)

### Community 309 - "ProviderRequest"
Cohesion: 0.22
Nodes (6): X, X, RequestBuilder, ProviderRequest, Self, T

### Community 311 - ".parse"
Cohesion: 0.13
Nodes (17): canonicalizing_does_not_touch_the_query_or_the_fragment(), debug_and_display_are_both_redacted(), ends_with_segment_is_case_sensitive_and_ignores_a_trailing_slash(), join_path_is_idempotent_over_trailing_slash(), join_path_preserves_fragment(), join_path_preserves_query_and_appends_segments(), joining_a_segment_keeps_the_real_credentials(), joining_a_segment_produces_one_separator_not_two() (+9 more)

### Community 318 - "redaction_negative.rs"
Cohesion: 0.37
Nodes (12): a_query_secret_never_appears_in_debug_but_its_name_does(), assert_clean(), both_forms_in_one_url_are_both_redacted_in_debug(), credentials_never_appear_in_a_transport_error(), credentials_never_reach_the_serialized_report(), ollama_credentials_never_appear_in_a_completion_error(), ollama_credentials_never_appear_in_a_probe_error(), ollama_credentials_never_reach_the_serialized_report() (+4 more)

### Community 320 - "String"
Cohesion: 0.17
Nodes (9): FixtureAudit, FixtureEntry, FixtureSummary, Manifest, manifest_load_of_a_missing_file_is_an_empty_manifest_not_an_error(), the_end_of_run_count_names_both_classes(), the_thirty_percent_warning_fires_only_strictly_above_the_threshold(), suffix() (+1 more)

### Community 323 - "tempdir_with"
Cohesion: 0.20
Nodes (22): a_root_that_exists_but_cannot_be_read_is_reported_while_an_absent_one_is_not(), a_subdirectory_that_cannot_be_read_is_an_error_not_a_silent_skip(), a_target_landing_inside_a_multibyte_character_still_returns_exactly_target_bytes(), an_entry_the_walk_cannot_read_is_an_error_not_a_dropped_file(), collect_from_entries(), collect_rs(), collect_subdir(), falls_short_of_target_fails_loudly_instead_of_returning_a_small_payload() (+14 more)

### Community 325 - "MockProvider"
Cohesion: 0.17
Nodes (8): MockProvider, AtomicUsize, test_analyze_all_agents_fail_returns_insufficient_agents(), test_analyze_one_agent_bad_json_degrades_gracefully(), test_analyze_one_agent_timeout_degrades_gracefully(), test_analyze_plain_text_response_treated_as_failure(), test_analyze_two_agents_fail_returns_insufficient_agents(), test_analyze_with_retry_disabled_skips_retry()

### Community 326 - "Arc"
Cohesion: 0.22
Nodes (8): Arc, JoinHandle, CapturedRequest, Option, String, spawn_429_with_retry_after(), spawn_capturing(), spawn_hanging_headers()

### Community 345 - "RunSpec"
Cohesion: 0.20
Nodes (12): CostLedger, Fallback, Injection, Payload, ReasoningControl, Seat, dispatch_measured(), ExecutesRuns (+4 more)

### Community 350 - ".cross_disk_against_manifest"
Cohesion: 0.29
Nodes (6): BTreeSet, I, Lineage, an_entry_that_cannot_be_read_is_reported_not_silently_dropped(), report_run_failed(), MagiReport

### Community 358 - "MagiConfig"
Cohesion: 0.33
Nodes (7): InputSize, exceeds(), MagiConfig, measure_input(), Default, the_reported_flag_always_agrees_with_the_reported_count(), warn_threshold_is_unreachable()

### Community 360 - ".format_dissent"
Cohesion: 0.40
Nodes (4): Dissent, test_dissent_line_contains_summary_not_reasoning(), test_dissent_section_has_blank_line_after(), test_dissent_shows_one_line_per_dissenter()

### Community 380 - "preflight.rs"
Cohesion: 0.11
Nodes (40): CompletionConfig, Future, a_backend_holding_every_seat_model_passes_the_check(), a_config_that_is_not_a_full_trio_is_rejected_as_a_config_fault(), a_config_with_no_fallbacks_is_a_config_fault_not_a_crate_verdict(), a_config_without_seats_is_rejected_by_name_in_the_config_step(), a_fallback_sharing_a_seats_lineage_is_rejected(), a_fallback_sharing_a_seats_model_is_rejected() (+32 more)

### Community 381 - "AgentOutput"
Cohesion: 0.20
Nodes (7): AgentOutput, Mode, Display, Formatter, Result, Vec, Verdict

### Community 387 - "Migrating from 3.2.0 to 4.0.0"
Cohesion: 0.18
Nodes (10): 1. `complete()` returns `Completion`, not `String`, 2. `RotationKind` gains four variants and becomes `#[non_exhaustive]`, 3. `ProviderError::Http { status: 0 }` no longer exists, 4. `OllamaProvider` completes on `/api/chat`, 5. `CompletionConfig::max_tokens` defaults to `16_384`, 6. `MagiReport` gains `completions`, 7. The time defaults change, Migrating from 3.2.0 to 4.0.0 (+2 more)

## Knowledge Gaps
- **227 isolated node(s):** `ProviderError`, `ProviderError`, `ProviderError`, `ProviderError`, `ProviderError` (+222 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **217 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `ProviderError` connect `ProviderError` to `ProviderResponse`, `orchestrator.rs`, `Duration`, `MagiError`, `report.rs`, `provider_url.rs`, `e2.rs`, `error.rs`, `provider.rs`, `.parse`, `String`, `SlowFailingProvider`, `MockProvider`, `MockProvider`, `.new`, `rotation.rs`, `CompletionRecord`, `.redacted`, `String`, `HostedModel`?**
  _High betweenness centrality (0.066) - this node is a cross-community bridge._
- **Why does `MagiError` connect `MagiError` to `String`, `report.rs`, `consensus.rs`, `prompts/mod.rs`, `.analyze`, `ProviderError`, `LlmProvider`, `validate.rs`, `e2.rs`, `error.rs`, `runner.rs`, `orchestrator.rs`, `.cross_disk_against_manifest`?**
  _High betweenness centrality (0.030) - this node is a cross-community bridge._
- **Why does `LlmProvider` connect `LlmProvider` to `SlowFailingProvider`, `.analyze`, `ProviderError`, `MockProvider`, `MockProvider`, `.new`, `claude.rs`, `ollama_wire.rs`, `claude_cli.rs`, `basic_analysis.rs`, `provider.rs`, `openai_compat.rs`, `Duration`, `MagiError`, `MagiBuilder`, `proxy.rs`, `HostedModel`?**
  _High betweenness centrality (0.029) - this node is a cross-community bridge._
- **What connects `ProviderError`, `ProviderError`, `ProviderError` to the rest of the system?**
  _227 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `test_support.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.06421052631578947 - nodes in this community are weakly interconnected._
- **Should `consensus.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.14046121593291405 - nodes in this community are weakly interconnected._
- **Should `validate.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.05980861244019139 - nodes in this community are weakly interconnected._