# Graph Report - MAGI-Core  (2026-08-19)

## Corpus Check
- 161 files · ~233,977 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 2895 nodes · 6557 edges · 330 communities (185 shown, 145 thin omitted)
- Extraction: 99% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 88 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `db18bc47`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- Formatter
- reporting.rs
- consensus.rs
- validate.rs
- .analyze
- MockProbe
- LlmProvider
- schema.rs
- .new
- claude.rs
- user_prompt.rs
- claude_cli.rs
- consensus
- Balthasar — The Pragmatist
- MAGI System Technical Documentation
- Caspar — The Critic
- Melchior — The Scientist
- test_support.rs
- orchestrator.rs
- Option
- MAGI System Technical Documentation
- AgentName
- ollama.rs
- MagiBuilder
- .new
- Magi
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
- finding_id.rs
- RoutingMockProvider
- [1.1.1] - 2026-07-17
- [1.1.1] - 2026-07-17
- basic_analysis.rs
- magi-core
- normalize_newlines
- build_retry_prompt
- fixtures.rs
- Finding
- Quick Start
- MagiError
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
- payload.rs
- ClaudeCliProvider::build_args
- ProviderResponse
- .parse
- 3. The Three Agents in Detail
- 6. Modes of Operation
- Model Rotation
- provider.rs
- e1.rs
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
- Mode
- Formatter
- ProviderError
- outcome.rs
- Self
- provider_url.rs
- [1.1.0] - 2026-05-25
- BTreeMap
- Default
- Mutex
- Send
- Sync
- openai_compat.rs
- Into
- Option
- Config
- Box
- BTreeMap
- Drop
- F
- HashMap
- P
- ProviderError
- CompletionConfig
- Debug
- Duration
- MockProvider
- Instant
- EventLog
- String
- Vec
- check_calibration.sh
- redacted
- .leak
- redacted
- redacted
- runner.rs
- redacted
- redacted
- .leak
- redacted
- redacted
- .leak
- redacted
- pool
- ProviderError
- Result
- Result
- RequestBuilder
- Response
- Box
- [1.1.0] - 2026-05-25
- sync-fixtures.sh
- Drop
- F
- VerdictExtractionError
- Arc
- paths.rs
- Result
- testkit.rs
- Self
- provider_url.rs
- AgentName
- pattern0nonterminal_bad.rs
- pattern0nonterminal_good.rs
- .new_checked
- String
- embedded_prompt_for
- Display
- Default
- .new
- config.rs
- Error
- I
- AgentName
- main.rs
- Vec
- Duration
- .new
- git.rs
- Validator
- rotation.rs
- .dangerous_settings
- ProviderRequest
- Self
- .redacted
- BuildOutcome
- Error
- Into
- ExitStatus
- magi-smoke — the smoke harness
- ScenarioState
- Into
- RunContext
- compose_transport_message
- ValidationLimits
- magi-core
- Scenario
- SlowFailingProvider
- PathBuf
- [2.0.0] - 2026-07-25
- Formatter
- MagiReport
- Drop
- Output
- proxy.rs
- PreflightError
- Path
- Path
- [1.1.0] - 2026-05-25
- .new
- RunId
- RunResult
- RequestRecord
- AgentName
- RunResult
- .read_verdict_body
- .new
- ScenarioState
- log_failure
- PathBuf
- I
- MagiReport
- RequestRecord
- Result
- Duration
- .send
- described
- Assertion
- [3.0.2] - 2026-07-30
- RunContext
- Scenario
- Option
- pattern8bnospace_bad.rs
- pattern8bnospace_good.rs
- Self
- .cmp
- Announcement
- Display
- Arc
- Drop
- Formatter
- TransparencyProbe
- body_bounds.rs
- SpyProxy
- ProviderProbe
- ExitCode
- Duration
- I
- String
- Duration
- String
- Vec
- ReportConfig
- preflight.rs
- pattern7builderalias_bad.rs
- pattern7builderalias_good.rs
- pattern7default_bad.rs
- pattern7default_good.rs
- Option
- Result
- Duration
- weakened.rs
- Option
- Option
- Severity
- Instant
- Result
- Self
- String
- Vec
- Self
- String
- Vec
- Result
- String
- HostedModel
- redaction_negative.rs
- mock_server.rs
- .complete
- AgentOutput
- MagiConfig
- .format_dissent
- Display
- Formatter
- Path
- PathBuf
- Result
- String
- Vec

## God Nodes (most connected - your core abstractions)
1. `AgentName` - 70 edges
2. `LlmProvider` - 50 edges
3. `blank_ctx()` - 50 edges
4. `MagiBuilder` - 43 edges
5. `MagiError` - 37 edges
6. `make_consensus()` - 35 edges
7. `Lineage` - 35 edges
8. `make_agent()` - 34 edges
9. `Magi` - 34 edges
10. `AgentOutput` - 32 edges

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

## Communities (330 total, 145 thin omitted)

### Community 0 - "Formatter"
Cohesion: 0.33
Nodes (3): Formatter, preflight_step_order(), the_order_is_config_then_fixtures_then_backend()

### Community 1 - "reporting.rs"
Cohesion: 0.08
Nodes (23): a_clean_run_gains_no_section_at_all(), a_report_from_before_this_version_also_gains_nothing(), a_report_from_the_previous_version_deserializes_as_none_not_zero(), an_exceeded_run_gains_exactly_one_section_naming_both_numbers(), cause_label(), estimate_tokens(), fit_content(), report_text() (+15 more)

### Community 2 - "consensus.rs"
Cohesion: 0.10
Nodes (62): ConsensusConfig, ConsensusEngine, dedup_key(), DedupKey, finding_key(), make_output(), BTreeMap, Default (+54 more)

### Community 3 - "validate.rs"
Cohesion: 0.07
Nodes (3): test_validate_accepts_confidence_at_boundaries(), test_validate_accepts_valid_agent_output(), test_validator_new_creates_with_default_limits()

### Community 4 - ".analyze"
Cohesion: 0.11
Nodes (37): a_measured_candidate_keeps_the_strict_guard_quiet(), a_strict_guard_with_nothing_measured_warns_and_still_completes(), exceeding_the_threshold_warns_and_still_completes(), probe_declaration_warnings_are_told_once_per_instance(), rotation_disabled_by_configuration_keeps_the_strict_guard_quiet(), F, test_a_clean_run_adds_no_section_to_the_human_report(), test_a_failure_before_rotation_is_attributed_to_the_pre_rotation_model() (+29 more)

### Community 5 - "MockProbe"
Cohesion: 0.13
Nodes (9): Barrier, FailingProbe, MockProbe, MockProvider, OverlapProbe, AtomicUsize, Formatter, ProviderError (+1 more)

### Community 6 - "LlmProvider"
Cohesion: 0.09
Nodes (33): Agent, AgentFactory, MockProvider, Arc, AtomicUsize, BTreeMap, Default, Option (+25 more)

### Community 7 - "schema.rs"
Cohesion: 0.05
Nodes (6): make_output(), test_agent_output_conditional_is_not_dissenting_from_approve_majority(), test_agent_output_effective_verdict_maps_conditional_to_approve(), test_agent_output_empty_findings_valid(), test_agent_output_is_dissenting_when_verdict_differs_from_majority(), test_agent_output_is_not_dissenting_when_verdict_matches_majority()

### Community 8 - ".new"
Cohesion: 0.16
Nodes (26): finding_with_title(), output_with_confidence(), output_with_findings(), Vec, test_validate_accepts_finding_with_normal_title(), test_validate_mut_collapses_control_whitespace_in_titles(), test_validate_mut_preserves_order_of_findings(), test_validate_mut_replaces_title_with_cleaned_form() (+18 more)

### Community 9 - "claude.rs"
Cohesion: 0.09
Nodes (36): ClaudeMessage, ClaudeProvider, ClaudeRequest, ClaudeResponse, ContentBlock, Client, Debug, Duration (+28 more)

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

### Community 17 - "test_support.rs"
Cohesion: 0.07
Nodes (45): Beh, build_oversized_case(), build_schema_local_case(), build_trio_with_caspar(), build_two_5xx_with_local_fallbacks(), build_two_external_failing_no_fallback(), build_two_failing_with_single_free_fallback(), build_two_network_failing_no_fallback() (+37 more)

### Community 18 - "orchestrator.rs"
Cohesion: 0.07
Nodes (67): ExternalErrorKind, a_probe_declaring_another_model_is_named_not_rejected(), a_probe_declaring_its_own_model_is_quiet(), a_probe_that_claims_nothing_is_checked_against_nothing(), a_threshold_is_unreachable_when_the_smallest_warning_input_is_already_rejected(), a_threshold_that_can_never_fire_is_detected(), a_zero_threshold_warns_always_and_does_not_disable(), an_empty_input_never_exceeds_even_a_zero_threshold() (+59 more)

### Community 20 - "MAGI System Technical Documentation"
Cohesion: 0.40
Nodes (4): Exemption from CLAUDE.local.md §0.2 file-header rule, Local divergence from the pinned reference (F0, 2026-07-16), Regeneration, `src/prompts_md/` — Embedded prompt data

### Community 21 - "AgentName"
Cohesion: 0.17
Nodes (20): Condition, ConsensusResult, DedupFinding, Dissent, Option, String, Vec, ExtractionFailure (+12 more)

### Community 22 - "ollama.rs"
Cohesion: 0.12
Nodes (21): a_mounted_prefix_survives_both_spellings(), a_root_that_really_ends_in_v1_is_read_as_the_prefix(), every_accepted_spelling_yields_the_same_endpoints(), new_gives_the_inner_provider_the_real_credentials(), OllamaProvider, Client, Duration, Into (+13 more)

### Community 23 - "MagiBuilder"
Cohesion: 0.12
Nodes (16): ComplexityGate, MagiBuilder, Box, P, Self, Send, test_analyze_nonce_collision_returns_invalid_input(), test_analyze_shares_same_nonce_across_all_three_agents() (+8 more)

### Community 24 - ".new"
Cohesion: 0.32
Nodes (18): test_base_zero_with_three_retries_emits_exactly_four_requests(), test_budget_exhaustion_abandons_with_typed_reason(), test_honored_retry_after_can_overrun_a_small_budget(), test_max_retries_zero_does_not_retry(), test_operation_budget_zero_yields_single_attempt(), test_retry_after_beyond_cap_abandons_with_typed_reason(), test_retry_provider_does_not_retry_on_auth(), test_retry_provider_does_not_retry_on_http_4xx() (+10 more)

### Community 25 - "Magi"
Cohesion: 0.19
Nodes (20): AbortHandle, DispatchOutcome, AbortGuard, attempt_model(), CapturingMockProvider, collect_probe_targets(), default_rotations(), dispatch_one_agent_rotating() (+12 more)

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
Nodes (42): ExitCode, FixtureSummary, ScenarioState, a_certificate_that_could_not_be_written_never_replaces_the_verdict(), a_clean_tree_gets_the_certificate_written_at_cert_path(), a_corpus_over_the_unverified_threshold_warns_in_the_certificate(), a_dirty_tree_gets_no_certificate_not_a_caveated_one(), a_run_with_a_failed_assertion_gets_no_certificate_at_all() (+34 more)

### Community 34 - "[0.6.0] - 2026-05-21"
Cohesion: 0.33
Nodes (6): [0.6.0] - 2026-05-21, Backward compatibility, Changed, Pre-merge gates (CLAUDE.local.md §6), Security, Test count

### Community 35 - "[1.0.0] - 2026-05-24"
Cohesion: 0.40
Nodes (5): [1.0.0] - 2026-05-24, Added, Changed (breaking), Notes, Security

### Community 36 - "finding_id.rs"
Cohesion: 0.21
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

### Community 40 - "basic_analysis.rs"
Cohesion: 0.29
Nodes (14): create_provider(), main(), parse_mode(), print_usage(), ProviderArgs, read_input(), Arc, Box (+6 more)

### Community 41 - "magi-core"
Cohesion: 0.17
Nodes (12): Changelog, Consensus Labels, Contribution, Credentials in a provider URL, Example, Feature Flags, Features, Implementing a Custom Provider (+4 more)

### Community 42 - "normalize_newlines"
Cohesion: 0.20
Nodes (12): neutralize_headers(), normalize_newlines(), Cow, String, sanitize_error_for_retry_feedback(), strip_invisibles(), test_neutralize_headers_not_bypassed_by_mongolian_vowel_separator(), test_neutralize_headers_preserves_unmatched_lines_borrowed() (+4 more)

### Community 43 - "build_retry_prompt"
Cohesion: 0.11
Nodes (18): build_retry_prompt(), test_build_retry_prompt_appends_feedback_block_exact_format(), test_build_retry_prompt_does_not_neutralize_midline_tokens(), test_build_retry_prompt_does_not_resanitize_content(), test_build_retry_prompt_feedback_block_after_end_delimiter(), test_build_retry_prompt_includes_seven_keys_list(), test_build_retry_prompt_neutralizes_dash_variant_retry_markers(), test_build_retry_prompt_neutralizes_injected_retry_feedback_marker() (+10 more)

### Community 44 - "fixtures.rs"
Cohesion: 0.13
Nodes (23): BTreeSet, I, a_file_on_disk_that_nobody_declares_is_an_orphan_too(), a_fixture_whose_hash_changed_is_rejected(), a_missing_fixtures_directory_is_reported_not_silently_clean(), a_nested_corpus_is_refused_because_the_walk_does_not_recurse(), a_readable_entry_is_still_crossed_against_the_manifest(), a_verified_by_naming_a_live_scenario_is_accepted() (+15 more)

### Community 45 - "Finding"
Cohesion: 0.18
Nodes (12): Finding, Into, String, test_agent_name_btreeset_orders_alphabetically(), test_agent_name_usable_as_btreemap_key(), test_finding_new_constructs_three_field_finding(), test_finding_new_defaults_optional_fields(), test_finding_serde_roundtrip() (+4 more)

### Community 46 - "Quick Start"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 47 - "MagiError"
Cohesion: 0.08
Nodes (19): a_message_of_exactly_the_cap_is_kept_whole(), a_short_external_message_survives_untouched(), AbandonReason, an_oversized_external_message_is_cut_and_says_so(), external_message(), MagiError, ProviderError, Duration (+11 more)

### Community 48 - "Lineage"
Cohesion: 0.13
Nodes (15): ActiveEntry, empty(), empty_s(), empty_wr(), Lineage, RegistryInner, RotationEvent, RotationKind (+7 more)

### Community 50 - "FixedRng"
Cohesion: 0.40
Nodes (4): FixedRng, test_fastrand_source_returns_distinct_values_across_calls(), test_fixed_rng_panics_when_exhausted(), VecDeque

### Community 52 - ".validate"
Cohesion: 0.33
Nodes (6): [0.3.0] - 2026-04-18, Added, Changed (breaking), Dependencies, Not included (deferred beyond v0.3.0), Security considerations

### Community 54 - "backoff.rs"
Cohesion: 0.16
Nodes (25): FnMut, fixed(), next_backoff(), parse_retry_after(), RetryAfter, Duration, Option, String (+17 more)

### Community 65 - "prompts/mod.rs"
Cohesion: 0.11
Nodes (14): lookup_prompt(), BTreeMap, Option, Result, String, test_guard_and_parser_agree_on_the_same_block(), test_seat_aware_validation_names_the_agent_and_mode(), test_the_error_is_actionable_on_its_own() (+6 more)

### Community 67 - "payload.rs"
Cohesion: 0.14
Nodes (25): Path, PathBuf, a_root_that_exists_but_cannot_be_read_is_reported_while_an_absent_one_is_not(), a_subdirectory_that_cannot_be_read_is_an_error_not_a_silent_skip(), a_target_landing_inside_a_multibyte_character_still_returns_exactly_target_bytes(), an_entry_the_walk_cannot_read_is_an_error_not_a_dropped_file(), block_comment_depth_after(), collect_from_entries() (+17 more)

### Community 69 - "ProviderResponse"
Cohesion: 0.14
Nodes (13): X, ProviderError, Result, X, diagnostic_truncation_is_announced_and_utf8_safe(), diagnostic_under_the_cap_is_untouched(), ProviderResponse, push_within_cap() (+5 more)

### Community 70 - ".parse"
Cohesion: 0.13
Nodes (18): canonicalizing_does_not_touch_the_query_or_the_fragment(), debug_and_display_are_both_redacted(), ends_with_segment_is_case_sensitive_and_ignores_a_trailing_slash(), join_path_is_idempotent_over_trailing_slash(), join_path_preserves_fragment(), join_path_preserves_query_and_appends_segments(), parent_climbs_one_level_and_stops_at_the_root(), parse_error_never_echoes_the_raw_input() (+10 more)

### Community 72 - "3. The Three Agents in Detail"
Cohesion: 0.50
Nodes (4): [3.0.0] - 2026-07-30, Added, BREAKING, Changed

### Community 73 - "6. Modes of Operation"
Cohesion: 0.39
Nodes (5): fail(), prod_only(), self_test(), check_redaction.sh script, skeleton()

### Community 74 - "Model Rotation"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 75 - "provider.rs"
Cohesion: 0.06
Nodes (29): f(), f(), f(), f(), P, Self, cause_chain(), cause_chain_skips_the_top_level_error() (+21 more)

### Community 76 - "e1.rs"
Cohesion: 0.06
Nodes (101): MagiReport, RequestRecord, RunContext, Scenario, sha256_hex(), assert_that(), a_fired_injection(), answered_probes() (+93 more)

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

### Community 104 - "Mode"
Cohesion: 0.24
Nodes (10): contract_prompt(), test_analyze_applies_mode_agnostic_override_to_melchior(), test_analyze_per_mode_override_supersedes_all_modes(), test_analyze_respects_prompts_dir_loaded_files(), test_build_aborts_on_a_corrupt_custom_prompt_before_any_provider_call(), test_legacy_with_custom_prompt_delegates_to_for_mode(), test_legacy_with_custom_prompt_shim_roundtrip(), test_with_custom_prompt_all_modes_stores_with_none_key() (+2 more)

### Community 107 - "outcome.rs"
Cohesion: 0.08
Nodes (12): F, a_panic_inside_a_run_is_caught_and_attributed_instead_of_killing_the_harness(), a_run_that_finishes_is_returned_untouched(), classify_panic(), exit_code(), install_panic_hook(), is_dependency_source(), is_harness_source() (+4 more)

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

### Community 116 - "openai_compat.rs"
Cohesion: 0.09
Nodes (32): absent_null_and_empty_content_are_all_a_named_schema_failure(), OpenAiChoice, OpenAiCompatibleProvider, OpenAiMessage, OpenAiRequest, OpenAiRespMessage, OpenAiResponse, Client (+24 more)

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

### Community 129 - "CompletionConfig"
Cohesion: 0.22
Nodes (9): classify(), CompletionConfig, is_retryable(), RetryAfterProvider, Default, ProviderError, Result, String (+1 more)

### Community 132 - "MockProvider"
Cohesion: 0.18
Nodes (7): AtomicU32, RetryClass, MockProvider, RetryConfig, RetryProvider, Arc, Mutex

### Community 133 - "Instant"
Cohesion: 0.50
Nodes (4): 2.1 Conceptual Mapping, 2.2 Why Three Perspectives and Not Two or Five, 2.3 Addressing Cognitive Biases, 2. Translation to the Software Engineering Domain

### Community 134 - "EventLog"
Cohesion: 0.13
Nodes (11): Attributes, Event, Field, Id, Metadata, Record, EventLog, FieldWriter (+3 more)

### Community 137 - "String"
Cohesion: 0.50
Nodes (3): ProviderUrl, redacted(), String

### Community 138 - "Vec"
Cohesion: 0.50
Nodes (4): 3.1 Melchior — The Scientist, 3.2 Balthasar — The Pragmatist, 3.3 Caspar — The Critic, 3. The Three Agents in Detail

### Community 144 - "runner.rs"
Cohesion: 0.15
Nodes (9): MagiError, an_assertion_that_could_not_be_tested_carries_its_reason(), Assertion, BackendNeed, render_error(), Into, Self, Scenario (+1 more)

### Community 152 - "pool"
Cohesion: 0.28
Nodes (11): ae(), policy(), pool(), state(), test_claim_next_none_leaves_registry_intact(), test_concurrent_claims_never_double_reserve_stress(), test_max_rotations_gate(), test_next_model_returns_first_eligible_in_declared_order() (+3 more)

### Community 164 - "ProviderError"
Cohesion: 0.50
Nodes (4): 6.1 Code Review, 6.2 Design, 6.3 Analysis, 6. Modes of Operation

### Community 165 - "Result"
Cohesion: 0.14
Nodes (13): JoinError, DeclaringProbe, MockProvider, resolve_abnormal_exit(), resolve_endpoint_down(), AtomicUsize, Option, ProviderError (+5 more)

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

### Community 174 - "VerdictExtractionError"
Cohesion: 0.16
Nodes (15): Fn, extract(), locate(), locate_block(), Display, Error, Formatter, Result (+7 more)

### Community 177 - "paths.rs"
Cohesion: 0.24
Nodes (12): feature_matrix_target_dir(), metadata_target_dir(), nothing_the_harness_generates_lands_in_the_repo(), repo_root(), resolve_deepest_existing(), Path, PathBuf, Vec (+4 more)

### Community 180 - "testkit.rs"
Cohesion: 0.09
Nodes (28): Arc, AtomicUsize, Mutex, a_broken_proxy_makes_the_preflight_say_cannot_test_not_failed(), no_backend_skips_the_backend_steps_and_nothing_else(), the_preflight_stops_if_smoke_cargo_lock_is_not_actually_tracked(), AlwaysSlowStub, EchoServer (+20 more)

### Community 181 - "Self"
Cohesion: 0.29
Nodes (4): Drop, Self, EnvVarGuard, TempDir

### Community 183 - "provider_url.rs"
Cohesion: 0.50
Nodes (3): leaky(), Error, String

### Community 188 - ".new_checked"
Cohesion: 0.40
Nodes (3): leaky(), Error, String

### Community 192 - "embedded_prompt_for"
Cohesion: 0.57
Nodes (7): balthasar_prompt(), caspar_prompt(), embedded_prompt_for(), melchior_prompt(), test_echo_canary_values_are_present_in_every_shipped_prompt(), test_prompts_match_pinned_reference_sha256(), test_shipped_prompts_satisfy_the_verdict_marker_contract()

### Community 195 - ".new"
Cohesion: 0.20
Nodes (30): make_agent(), make_consensus(), test_agent_display_fallback_to_agent_name_methods(), test_agent_line_format(), test_banner_all_lines_are_exactly_banner_width(), test_banner_consensus_line_includes_split_for_go_with_caveats(), test_banner_labels_are_column_aligned_to_max_label_len(), test_banner_lines_are_exactly_52_chars_wide() (+22 more)

### Community 196 - "config.rs"
Cohesion: 0.08
Nodes (39): Default, Duration, FnOnce, Result, a_budget_beyond_the_ceiling_is_rejected_and_the_field_is_named(), a_correctly_spelled_override_still_loads(), a_set_of_overrides_that_is_still_illegal_at_the_end_is_refused(), a_zero_timeout_is_rejected_and_the_field_is_named() (+31 more)

### Community 200 - "main.rs"
Cohesion: 0.12
Nodes (30): AssertionRow, CycleRun, a_genuine_skip_still_reports_exit_two(), a_healthy_run_can_reach_exit_zero(), a_scenario_filtered_out_by_no_backend_is_OUT_OF_SCOPE_not_omitted(), a_scenario_reading_one_run_never_sees_ANOTHER_runs_records(), absent_context(), build_outcome() (+22 more)

### Community 204 - "git.rs"
Cohesion: 0.33
Nodes (4): Path, Result, String, status_porcelain()

### Community 209 - "Validator"
Cohesion: 0.38
Nodes (5): clean_title(), Result, String, test_clean_title_is_idempotent(), Validator

### Community 210 - "rotation.rs"
Cohesion: 0.07
Nodes (30): AgentSlotGuard, LineageRegistry, reg(), Arc, Drop, Mutex, strict_guard_is_inert(), test_5xx_does_not_count_toward_endpoint_down() (+22 more)

### Community 211 - ".dangerous_settings"
Cohesion: 0.40
Nodes (4): test_dangerous_config_is_announced_for_retry_after_cap_over_budget(), test_dangerous_config_is_announced_for_zero_base_delay(), test_dangerous_config_is_announced_for_zero_cap(), test_dangerous_config_is_announced_for_zero_retry_after_cap()

### Community 212 - "ProviderRequest"
Cohesion: 0.25
Nodes (5): X, X, ProviderRequest, RequestBuilder, Self

### Community 214 - ".redacted"
Cohesion: 0.29
Nodes (5): Method, redacted_hides_all_query_values_and_keeps_names(), redacted_hides_userinfo_and_keeps_host(), redacted_placeholder_is_url_safe_and_not_percent_encoded(), Client

### Community 220 - "magi-smoke — the smoke harness"
Cohesion: 0.15
Nodes (12): 10. `cargo audit` for the harness, 11. The reproduction that justifies the harness, 1. Three exit codes, and the difference between two of them, 2. Two dependency modes, 3. What the proxy does, and why its red is never the crate's, 4. The contention probe, and its declared scope, 5. Where it fits in the cycle, 6. Cost per run (+4 more)

### Community 224 - "compose_transport_message"
Cohesion: 0.50
Nodes (3): Error, String, spawn_failure()

### Community 225 - "ValidationLimits"
Cohesion: 0.33
Nodes (6): Default, Self, test_title_length_checked_after_strip_zero_width(), test_validate_mut_atomic_no_partial_mutation_on_error(), test_validator_with_limits_uses_custom_limits(), ValidationLimits

### Community 228 - "SlowFailingProvider"
Cohesion: 0.16
Nodes (5): FailingProvider, AtomicUsize, Duration, Self, SlowFailingProvider

### Community 230 - "[2.0.0] - 2026-07-25"
Cohesion: 0.50
Nodes (4): [2.0.0] - 2026-07-25, Added, BREAKING, Changed

### Community 235 - "proxy.rs"
Cohesion: 0.06
Nodes (48): AtomicBool, B, Box, Bytes, X, Client, Error, HeaderMap (+40 more)

### Community 239 - "[1.1.0] - 2026-05-25"
Cohesion: 0.67
Nodes (3): [1.1.0] - 2026-05-25, Added, Notes

### Community 240 - ".new"
Cohesion: 0.16
Nodes (16): PayloadError, Config, Error, Path, Result, SpyProxy, Vec, Runner (+8 more)

### Community 245 - "RunResult"
Cohesion: 0.17
Nodes (19): AgentName, Fallback, Injection, Magi, Payload, RunOutcome, Seat, attempts_for() (+11 more)

### Community 249 - ".read_verdict_body"
Cohesion: 0.29
Nodes (5): body_cap(), Formatter, ProviderError, Result, send_composes_a_redacted_error_on_connection_failure()

### Community 251 - ".new"
Cohesion: 0.23
Nodes (15): FallbackPoolBuilder, P, Self, run_preflight(), test_duplicate_lineage_warns_but_builds(), test_empty_pool_is_valid_and_keeps_max_rotations(), test_preflight_builds_capabilities_from_probes(), test_probe_failure_yields_none_and_does_not_abort() (+7 more)

### Community 267 - "[3.0.2] - 2026-07-30"
Cohesion: 0.67
Nodes (3): [3.0.2] - 2026-07-30, Changed, Fixed

### Community 271 - "pattern8bnospace_bad.rs"
Cohesion: 0.50
Nodes (4): build(), ProviderError, Self, sneak()

### Community 272 - "pattern8bnospace_good.rs"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 274 - ".cmp"
Cohesion: 0.50
Nodes (3): Ordering, Option, Self

### Community 281 - "body_bounds.rs"
Cohesion: 0.11
Nodes (30): TcpStream, a_chunked_probe_body_degrades_from_the_streaming_branch(), a_chunked_probe_body_under_the_cap_is_read_and_parsed(), a_probe_body_over_the_cap_degrades_instead_of_failing(), a_success_body_at_the_cap_is_read_whole(), a_success_body_over_the_cap_fails_rather_than_arriving_truncated(), a_verdict_body_that_is_not_utf8_fails_instead_of_being_mangled(), an_error_body_over_the_cap_keeps_its_prefix_and_announces_the_cut() (+22 more)

### Community 283 - "ProviderProbe"
Cohesion: 0.24
Nodes (6): FallbackCandidate, FallbackPool, ProviderProbe, RotationConfig, Send, Sync

### Community 289 - "String"
Cohesion: 0.27
Nodes (17): AgentRotationState, Candidate, cap(), caps_map(), digest_case(), digest_case_self(), digest_case_two_active(), digest_collision() (+9 more)

### Community 291 - "ReportConfig"
Cohesion: 0.16
Nodes (13): ReportConfig, ReportError, Default, Display, Formatter, Result, Self, test_new_checked_accepts_all_ascii_titles() (+5 more)

### Community 292 - "preflight.rs"
Cohesion: 0.10
Nodes (50): Config, Debug, Display, Future, Into, Output, PayloadError, a_backend_that_does_not_hold_the_probe_model_is_INCONCLUSIVE_not_clear() (+42 more)

### Community 293 - "pattern7builderalias_bad.rs"
Cohesion: 0.67
Nodes (3): bare(), configured(), Client

### Community 294 - "pattern7builderalias_good.rs"
Cohesion: 0.67
Nodes (3): also_configured(), configured(), Client

### Community 301 - "weakened.rs"
Cohesion: 0.18
Nodes (10): a_source_file_that_cannot_be_decoded_is_reported_too(), an_unreadable_path_is_reported_and_never_skipped(), every_mark_outside_this_guards_own_fixtures_is_well_formed(), Path, Result, String, Vec, scan() (+2 more)

### Community 304 - "Severity"
Cohesion: 0.24
Nodes (6): Ord, PartialOrd, Display, Formatter, Result, Severity

### Community 316 - "HostedModel"
Cohesion: 0.24
Nodes (6): HostedModel, Option, ProviderError, Result, String, SidecarProbe

### Community 317 - "redaction_negative.rs"
Cohesion: 0.37
Nodes (12): a_query_secret_never_appears_in_debug_but_its_name_does(), assert_clean(), both_forms_in_one_url_are_both_redacted_in_debug(), credentials_never_appear_in_a_transport_error(), credentials_never_reach_the_serialized_report(), ollama_credentials_never_appear_in_a_completion_error(), ollama_credentials_never_appear_in_a_probe_error(), ollama_credentials_never_reach_the_serialized_report() (+4 more)

### Community 318 - "mock_server.rs"
Cohesion: 0.19
Nodes (4): JoinHandle, String, spawn_429_with_retry_after(), spawn_hanging_headers()

### Community 319 - ".complete"
Cohesion: 0.27
Nodes (6): describe(), main(), MyBackend, ProviderError, Result, String

### Community 320 - "AgentOutput"
Cohesion: 0.39
Nodes (3): AgentOutput, Vec, Verdict

### Community 321 - "MagiConfig"
Cohesion: 0.40
Nodes (6): exceeds(), MagiConfig, measure_input(), Default, the_reported_flag_always_agrees_with_the_reported_count(), warn_threshold_is_unreachable()

### Community 322 - ".format_dissent"
Cohesion: 0.50
Nodes (3): test_dissent_line_contains_summary_not_reasoning(), test_dissent_section_has_blank_line_after(), test_dissent_shows_one_line_per_dissenter()

## Knowledge Gaps
- **214 isolated node(s):** `melchior`, `report`, `X`, `8. Evangelion Correspondence Table`, `9. Relationship to the MAGI Python Plugin` (+209 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **145 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `run_catching()` connect `outcome.rs` to `config.rs`, `RunResult`?**
  _High betweenness centrality (0.050) - this node is a cross-community bridge._
- **Why does `LlmProvider` connect `LlmProvider` to `CompletionConfig`, `.analyze`, `MockProvider`, `MockProbe`, `claude.rs`, `claude_cli.rs`, `test_support.rs`, `ollama.rs`, `MagiBuilder`, `.new`, `Magi`, `ProviderProbe`, `Result`, `basic_analysis.rs`, `HostedModel`, `.complete`, `provider.rs`, `rotation.rs`, `SlowFailingProvider`, `openai_compat.rs`, `.new`?**
  _High betweenness centrality (0.038) - this node is a cross-community bridge._
- **Why does `ProviderRequest` connect `ProviderRequest` to `.read_verdict_body`, `ProviderResponse`, `provider_url.rs`, `.redacted`?**
  _High betweenness centrality (0.037) - this node is a cross-community bridge._
- **What connects `melchior`, `report`, `X` to the rest of the system?**
  _214 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `reporting.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.0761904761904762 - nodes in this community are weakly interconnected._
- **Should `consensus.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.096579476861167 - nodes in this community are weakly interconnected._
- **Should `validate.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.06896551724137931 - nodes in this community are weakly interconnected._