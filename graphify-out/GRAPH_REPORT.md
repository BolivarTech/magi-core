# Graph Report - MAGI-Core  (2026-08-18)

## Corpus Check
- 160 files · ~187,042 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 2632 nodes · 5974 edges · 254 communities (177 shown, 77 thin omitted)
- Extraction: 99% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 55 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `2fae04e6`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- rotation.rs
- reporting.rs
- consensus.rs
- validate.rs
- .analyze
- ProviderProbe
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
- prelude.rs
- MAGI System Technical Documentation
- weakened.rs
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
- evaluate
- [0.6.0] - 2026-05-21
- [1.0.0] - 2026-05-24
- finding_id.rs
- RoutingMockProvider
- [1.1.1] - 2026-07-17
- [1.1.1] - 2026-07-17
- report.rs
- magi-core
- normalize_newlines
- config.rs
- SlowFailingProvider
- Finding
- Quick Start
- MagiError
- Lineage
- .cmp
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
- preflight.rs
- MagiReport
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
- Error
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
- Instant
- Into
- Option
- AssertionRow
- Box
- BTreeMap
- Drop
- F
- HashMap
- P
- ProviderError
- RetryAfterProvider
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
- Report
- ProviderError
- Severity
- Result
- RequestBuilder
- Response
- Box
- [1.1.0] - 2026-05-25
- sync-fixtures.sh
- Drop
- F
- VerdictExtractionError
- MagiConfig
- Validator
- SpyProxy
- Config
- AgentName
- provider_url.rs
- AgentName
- pattern0nonterminal_bad.rs
- pattern0nonterminal_good.rs
- .new_checked
- ValidationLimits
- embedded_prompt_for
- compose_transport_message
- Mode
- Injection
- Into
- Option
- build_retry_prompt
- RunOutcome
- RunContext
- RequestRecord
- SpyProxy
- .new
- .new
- Result
- s4_rotation_and_its_cause
- RunId
- ProviderRequest
- RequestRecord
- .redacted
- RequestRecord
- AgentName
- ReportConfig
- LineageRegistry
- magi-smoke — the smoke harness
- .new
- Arc
- AgentOutput
- compose_transport_message
- .format_dissent
- magi-core
- Self
- ProviderError
- [2.0.0] - 2026-07-25
- [1.1.0] - 2026-05-25
- FallbackPool
- log_failure
- Duration
- .send
- described
- [3.0.2] - 2026-07-30
- pattern8bnospace_bad.rs
- pattern8bnospace_good.rs
- Mutex
- String
- pattern7builderalias_bad.rs
- pattern7builderalias_good.rs
- pattern7default_bad.rs
- pattern7default_good.rs

## God Nodes (most connected - your core abstractions)
1. `AgentName` - 70 edges
2. `LlmProvider` - 50 edges
3. `MagiBuilder` - 43 edges
4. `blank_ctx()` - 43 edges
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

## Communities (254 total, 77 thin omitted)

### Community 0 - "rotation.rs"
Cohesion: 0.11
Nodes (26): ae(), policy(), pool(), reg(), state(), test_5xx_does_not_count_toward_endpoint_down(), test_active_unverifiable_digest_does_not_block_rotation(), test_calling_agent_excluded_from_digest_check() (+18 more)

### Community 1 - "reporting.rs"
Cohesion: 0.07
Nodes (25): a_clean_run_gains_no_section_at_all(), a_report_from_before_this_version_also_gains_nothing(), a_report_from_the_previous_version_deserializes_as_none_not_zero(), an_exceeded_run_gains_exactly_one_section_naming_both_numbers(), cause_label(), estimate_tokens(), fit_content(), report_text() (+17 more)

### Community 2 - "consensus.rs"
Cohesion: 0.10
Nodes (61): ConsensusConfig, ConsensusEngine, dedup_key(), DedupKey, finding_key(), make_output(), Default, Result (+53 more)

### Community 3 - "validate.rs"
Cohesion: 0.07
Nodes (3): test_validate_accepts_confidence_at_boundaries(), test_validate_accepts_valid_agent_output(), test_validator_new_creates_with_default_limits()

### Community 4 - ".analyze"
Cohesion: 0.11
Nodes (37): a_measured_candidate_keeps_the_strict_guard_quiet(), a_strict_guard_with_nothing_measured_warns_and_still_completes(), exceeding_the_threshold_warns_and_still_completes(), probe_declaration_warnings_are_told_once_per_instance(), rotation_disabled_by_configuration_keeps_the_strict_guard_quiet(), F, test_a_clean_run_adds_no_section_to_the_human_report(), test_a_failure_before_rotation_is_attributed_to_the_pre_rotation_model() (+29 more)

### Community 5 - "ProviderProbe"
Cohesion: 0.11
Nodes (13): Barrier, FailingProbe, MockProbe, MockProvider, OverlapProbe, ProviderProbe, RotationConfig, AtomicUsize (+5 more)

### Community 6 - "LlmProvider"
Cohesion: 0.05
Nodes (47): HostedModel, Option, ProviderError, Result, String, SidecarProbe, describe(), main() (+39 more)

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
Cohesion: 0.08
Nodes (44): Beh, build_oversized_case(), build_schema_local_case(), build_trio_with_caspar(), build_two_5xx_with_local_fallbacks(), build_two_external_failing_no_fallback(), build_two_failing_with_single_free_fallback(), build_two_network_failing_no_fallback() (+36 more)

### Community 18 - "orchestrator.rs"
Cohesion: 0.07
Nodes (67): ExternalErrorKind, a_probe_declaring_another_model_is_named_not_rejected(), a_probe_declaring_its_own_model_is_quiet(), a_probe_that_claims_nothing_is_checked_against_nothing(), a_threshold_is_unreachable_when_the_smallest_warning_input_is_already_rejected(), a_threshold_that_can_never_fire_is_detected(), a_zero_threshold_warns_always_and_does_not_disable(), an_empty_input_never_exceeds_even_a_zero_threshold() (+59 more)

### Community 19 - "prelude.rs"
Cohesion: 0.06
Nodes (30): create_provider(), main(), parse_mode(), print_usage(), ProviderArgs, read_input(), Arc, Box (+22 more)

### Community 20 - "MAGI System Technical Documentation"
Cohesion: 0.40
Nodes (4): Exemption from CLAUDE.local.md §0.2 file-header rule, Local divergence from the pinned reference (F0, 2026-07-16), Regeneration, `src/prompts_md/` — Embedded prompt data

### Community 21 - "weakened.rs"
Cohesion: 0.13
Nodes (22): Path, Result, Self, a_file_on_disk_that_nobody_declares_is_an_orphan_too(), a_fixture_whose_hash_changed_is_rejected(), an_empty_manifest_verifies_clean_which_is_exactly_e1(), an_orphaned_entry_is_detected(), an_unrecognized_currency_form_is_rejected() (+14 more)

### Community 22 - "ollama.rs"
Cohesion: 0.12
Nodes (21): a_mounted_prefix_survives_both_spellings(), a_root_that_really_ends_in_v1_is_read_as_the_prefix(), every_accepted_spelling_yields_the_same_endpoints(), new_gives_the_inner_provider_the_real_credentials(), OllamaProvider, Client, Duration, Into (+13 more)

### Community 23 - "MagiBuilder"
Cohesion: 0.12
Nodes (16): ComplexityGate, MagiBuilder, Box, P, Self, Send, test_analyze_nonce_collision_returns_invalid_input(), test_analyze_shares_same_nonce_across_all_three_agents() (+8 more)

### Community 24 - ".new"
Cohesion: 0.31
Nodes (19): Self, test_base_zero_with_three_retries_emits_exactly_four_requests(), test_budget_exhaustion_abandons_with_typed_reason(), test_honored_retry_after_can_overrun_a_small_budget(), test_max_retries_zero_does_not_retry(), test_operation_budget_zero_yields_single_attempt(), test_retry_after_beyond_cap_abandons_with_typed_reason(), test_retry_provider_does_not_retry_on_auth() (+11 more)

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

### Community 33 - "evaluate"
Cohesion: 0.20
Nodes (17): I, absent_context(), Cli, crate_version(), cycle_run(), evaluate(), evaluate_preflight_only(), git_commit() (+9 more)

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

### Community 40 - "report.rs"
Cohesion: 0.24
Nodes (13): a_clean_tree_gets_the_certificate_written_at_cert_path(), a_dirty_tree_gets_no_certificate_not_a_caveated_one(), a_time_failure_renders_visibly_different_from_an_assertion_failure(), every_rendered_row_carries_the_run_id_that_fed_it(), format_row(), git_status_porcelain(), repo_with_uncommitted_changes(), Path (+5 more)

### Community 41 - "magi-core"
Cohesion: 0.17
Nodes (12): Changelog, Consensus Labels, Contribution, Credentials in a provider URL, Example, Feature Flags, Features, Implementing a Custom Provider (+4 more)

### Community 42 - "normalize_newlines"
Cohesion: 0.20
Nodes (12): neutralize_headers(), normalize_newlines(), Cow, String, sanitize_error_for_retry_feedback(), strip_invisibles(), test_neutralize_headers_not_bypassed_by_mongolian_vowel_separator(), test_neutralize_headers_preserves_unmatched_lines_borrowed() (+4 more)

### Community 43 - "config.rs"
Cohesion: 0.08
Nodes (35): Default, Display, Drop, a_correctly_spelled_override_still_loads(), a_zero_timeout_is_rejected_and_the_field_is_named(), absent_file_yields_defaults_and_says_so(), an_unmatched_magi_smoke_var_is_rejected_and_named(), b_happy() (+27 more)

### Community 44 - "SlowFailingProvider"
Cohesion: 0.29
Nodes (3): AtomicUsize, Duration, SlowFailingProvider

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
Cohesion: 0.19
Nodes (8): Lineage, RotationEvent, RotationKind, Cow, Display, From, Into, trim_cow()

### Community 49 - ".cmp"
Cohesion: 0.50
Nodes (3): Ordering, Option, Self

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

### Community 66 - "preflight.rs"
Cohesion: 0.06
Nodes (59): Arc, AtomicUsize, Debug, Duration, Formatter, PathBuf, a_broken_proxy_makes_the_preflight_say_cannot_test_not_failed(), a_cold_model_passes_on_the_second_probe_attempt() (+51 more)

### Community 69 - "ProviderResponse"
Cohesion: 0.15
Nodes (12): X, ProviderError, Result, X, body_cap(), ProviderResponse, push_within_cap(), Option (+4 more)

### Community 70 - ".parse"
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

### Community 75 - "provider.rs"
Cohesion: 0.09
Nodes (18): cause_chain(), cause_chain_skips_the_top_level_error(), client_build_error(), default_model_for_mode(), describe_parse_error(), resolve_claude_alias(), Error, test_completion_config_default_values() (+10 more)

### Community 76 - "e1.rs"
Cohesion: 0.09
Nodes (46): ScenarioState, assert_that(), blank_ctx(), e1_contains_exactly_the_scenarios_valid_against_3_2_0(), e1_scenarios(), every_scenario_declares_whether_it_needs_a_backend(), record(), recorded_response() (+38 more)

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

### Community 107 - "outcome.rs"
Cohesion: 0.11
Nodes (11): F, a_panic_inside_a_run_is_caught_and_attributed_instead_of_killing_the_harness(), a_run_that_finishes_is_returned_untouched(), classify_panic(), exit_code(), install_panic_hook(), Option, Result (+3 more)

### Community 108 - "Self"
Cohesion: 0.29
Nodes (4): 8. Evangelion Correspondence Table, 9. Relationship to the MAGI Python Plugin, MAGI System — Complete Technical Documentation, Multi-Perspective Analysis Library for Rust

### Community 109 - "provider_url.rs"
Cohesion: 0.18
Nodes (7): a_partial_read_stays_within_the_cap_including_its_marker(), diagnostic_truncation_is_announced_and_utf8_safe(), diagnostic_under_the_cap_is_untouched(), mark_truncated(), marking_a_short_text_does_not_pad_it(), marking_never_splits_a_multibyte_character(), truncate_diagnostic()

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

### Community 121 - "AssertionRow"
Cohesion: 0.21
Nodes (13): AssertionRow, large_payload_priority(), only_the_second_cycle_run_emits_a_certificate(), render_certificate(), row_to_json(), Duration, Option, RunId (+5 more)

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

### Community 129 - "RetryAfterProvider"
Cohesion: 0.22
Nodes (6): RetryAfterProvider, Vec, test_dangerous_config_is_announced_for_retry_after_cap_over_budget(), test_dangerous_config_is_announced_for_zero_base_delay(), test_dangerous_config_is_announced_for_zero_cap(), test_dangerous_config_is_announced_for_zero_retry_after_cap()

### Community 132 - "MockProvider"
Cohesion: 0.19
Nodes (6): AtomicU32, MockProvider, RetryConfig, RetryProvider, Arc, Mutex

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
Cohesion: 0.08
Nodes (37): AgentName, Fallback, Magi, MagiError, Payload, PayloadError, Seat, RunOutcome (+29 more)

### Community 152 - "Report"
Cohesion: 0.22
Nodes (5): cycle_run_label(), CycleRun, render_json_is_parseable_and_carries_the_same_facts_as_the_human_table(), Report, ExitCode

### Community 164 - "ProviderError"
Cohesion: 0.50
Nodes (4): 6.1 Code Review, 6.2 Design, 6.3 Analysis, 6. Modes of Operation

### Community 165 - "Severity"
Cohesion: 0.24
Nodes (6): Ord, PartialOrd, Display, Formatter, Result, Severity

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
Cohesion: 0.15
Nodes (16): Fn, test_the_two_scripted_bodies_fail_and_succeed_where_their_names_claim(), extract(), locate(), locate_block(), Display, Error, Formatter (+8 more)

### Community 176 - "MagiConfig"
Cohesion: 0.40
Nodes (6): exceeds(), MagiConfig, measure_input(), Default, the_reported_flag_always_agrees_with_the_reported_count(), warn_threshold_is_unreachable()

### Community 177 - "Validator"
Cohesion: 0.38
Nodes (5): clean_title(), Result, String, test_clean_title_is_idempotent(), Validator

### Community 179 - "SpyProxy"
Cohesion: 0.08
Nodes (39): AtomicBool, B, Box, Bytes, X, HeaderMap, Incoming, Infallible (+31 more)

### Community 181 - "AgentName"
Cohesion: 0.17
Nodes (19): Condition, ConsensusResult, DedupFinding, Dissent, BTreeMap, Option, String, Vec (+11 more)

### Community 183 - "provider_url.rs"
Cohesion: 0.50
Nodes (3): leaky(), Error, String

### Community 188 - ".new_checked"
Cohesion: 0.40
Nodes (3): leaky(), Error, String

### Community 191 - "ValidationLimits"
Cohesion: 0.33
Nodes (6): Default, Self, test_title_length_checked_after_strip_zero_width(), test_validate_mut_atomic_no_partial_mutation_on_error(), test_validator_with_limits_uses_custom_limits(), ValidationLimits

### Community 192 - "embedded_prompt_for"
Cohesion: 0.57
Nodes (7): balthasar_prompt(), caspar_prompt(), embedded_prompt_for(), melchior_prompt(), test_echo_canary_values_are_present_in_every_shipped_prompt(), test_prompts_match_pinned_reference_sha256(), test_shipped_prompts_satisfy_the_verdict_marker_contract()

### Community 193 - "compose_transport_message"
Cohesion: 0.12
Nodes (11): f(), f(), f(), f(), P, Self, compose_caps_the_head_even_with_no_cause_chain(), compose_does_not_truncate_when_under_the_cap() (+3 more)

### Community 194 - "Mode"
Cohesion: 0.24
Nodes (10): contract_prompt(), test_analyze_applies_mode_agnostic_override_to_melchior(), test_analyze_per_mode_override_supersedes_all_modes(), test_analyze_respects_prompts_dir_loaded_files(), test_build_aborts_on_a_corrupt_custom_prompt_before_any_provider_call(), test_legacy_with_custom_prompt_delegates_to_for_mode(), test_legacy_with_custom_prompt_shim_roundtrip(), test_with_custom_prompt_all_modes_stores_with_none_key() (+2 more)

### Community 198 - "build_retry_prompt"
Cohesion: 0.11
Nodes (18): build_retry_prompt(), test_build_retry_prompt_appends_feedback_block_exact_format(), test_build_retry_prompt_does_not_neutralize_midline_tokens(), test_build_retry_prompt_does_not_resanitize_content(), test_build_retry_prompt_feedback_block_after_end_delimiter(), test_build_retry_prompt_includes_seven_keys_list(), test_build_retry_prompt_neutralizes_dash_variant_retry_markers(), test_build_retry_prompt_neutralizes_injected_retry_feedback_marker() (+10 more)

### Community 200 - "RunContext"
Cohesion: 0.14
Nodes (23): Assertion, Duration, MagiReport, RunContext, preflight_error_for_stage(), Result, String, Vec (+15 more)

### Community 204 - ".new"
Cohesion: 0.20
Nodes (30): make_agent(), make_consensus(), test_agent_display_fallback_to_agent_name_methods(), test_agent_line_format(), test_banner_all_lines_are_exactly_banner_width(), test_banner_consensus_line_includes_split_for_go_with_caveats(), test_banner_labels_are_column_aligned_to_max_label_len(), test_banner_lines_are_exactly_52_chars_wide() (+22 more)

### Community 209 - "Result"
Cohesion: 0.14
Nodes (13): JoinError, DeclaringProbe, MockProvider, resolve_abnormal_exit(), resolve_endpoint_down(), AtomicUsize, Option, ProviderError (+5 more)

### Community 210 - "s4_rotation_and_its_cause"
Cohesion: 0.36
Nodes (12): report_with_caspar_rotation_chain(), MagiReport, s4_cause_check_fails_on_a_hop_misclassified_as_schema(), s4_cause_check_fails_rather_than_skips_when_nothing_rotated(), s4_cause_check_passes_on_a_transport_classified_hop(), s4_ctx_with_rotation(), s4_rotated_check_fails_when_the_chain_is_empty_did_not_rotate(), s4_rotated_check_fails_when_the_hop_lands_on_the_same_lineage() (+4 more)

### Community 212 - "ProviderRequest"
Cohesion: 0.20
Nodes (7): X, X, Method, ProviderRequest, Client, RequestBuilder, send_composes_a_redacted_error_on_connection_failure()

### Community 214 - ".redacted"
Cohesion: 0.29
Nodes (5): redacted_hides_all_query_values_and_keeps_names(), redacted_hides_userinfo_and_keeps_host(), redacted_placeholder_is_url_safe_and_not_percent_encoded(), Formatter, Result

### Community 218 - "ReportConfig"
Cohesion: 0.16
Nodes (13): ReportConfig, ReportError, Default, Display, Formatter, Result, Self, test_new_checked_accepts_all_ascii_titles() (+5 more)

### Community 219 - "LineageRegistry"
Cohesion: 0.17
Nodes (10): ActiveEntry, empty(), empty_s(), empty_wr(), LineageRegistry, RegistryInner, BTreeSet, Mutex (+2 more)

### Community 220 - "magi-smoke — the smoke harness"
Cohesion: 0.15
Nodes (12): 10. `cargo audit` for the harness, 11. The reproduction that justifies the harness, 1. Three exit codes, and the difference between two of them, 2. Two dependency modes, 3. What the proxy does, and why its red is never the crate's, 4. The contention probe, and its declared scope, 5. Where it fits in the cycle, 6. Cost per run (+4 more)

### Community 221 - ".new"
Cohesion: 0.27
Nodes (11): run_preflight(), strict_guard_is_inert(), test_lineage_ord_for_btree_keys(), test_preflight_builds_capabilities_from_probes(), test_probe_failure_yields_none_and_does_not_abort(), test_probes_run_concurrently_via_overlap_counter(), test_strict_guard_is_inert_when_all_candidates_probed_but_window_unmeasurable(), test_strict_guard_is_inert_when_guard_off_and_candidate_unmeasured() (+3 more)

### Community 222 - "Arc"
Cohesion: 0.29
Nodes (5): AgentSlotGuard, Arc, Drop, test_slot_guard_mark_released_suppresses_drop(), test_slot_guard_releases_on_drop_unless_succeeded()

### Community 223 - "AgentOutput"
Cohesion: 0.39
Nodes (3): AgentOutput, Vec, Verdict

### Community 224 - "compose_transport_message"
Cohesion: 0.50
Nodes (3): Error, String, spawn_failure()

### Community 225 - ".format_dissent"
Cohesion: 0.50
Nodes (3): test_dissent_line_contains_summary_not_reasoning(), test_dissent_section_has_blank_line_after(), test_dissent_shows_one_line_per_dissenter()

### Community 227 - "Self"
Cohesion: 0.33
Nodes (3): parent_climbs_one_level_and_stops_at_the_root(), Self, T

### Community 228 - "ProviderError"
Cohesion: 0.23
Nodes (7): RetryClass, classify(), FailingProvider, is_retryable(), ProviderError, Result, String

### Community 230 - "[2.0.0] - 2026-07-25"
Cohesion: 0.50
Nodes (4): [2.0.0] - 2026-07-25, Added, BREAKING, Changed

### Community 239 - "[1.1.0] - 2026-05-25"
Cohesion: 0.67
Nodes (3): [1.1.0] - 2026-05-25, Added, Notes

### Community 251 - "FallbackPool"
Cohesion: 0.17
Nodes (13): FallbackCandidate, FallbackPool, FallbackPoolBuilder, P, Self, test_duplicate_lineage_warns_but_builds(), test_empty_pool_is_valid_and_keeps_max_rotations(), test_push_probing_stores_both_views() (+5 more)

### Community 267 - "[3.0.2] - 2026-07-30"
Cohesion: 0.67
Nodes (3): [3.0.2] - 2026-07-30, Changed, Fixed

### Community 271 - "pattern8bnospace_bad.rs"
Cohesion: 0.50
Nodes (4): build(), ProviderError, Self, sneak()

### Community 272 - "pattern8bnospace_good.rs"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 281 - "Mutex"
Cohesion: 0.11
Nodes (30): TcpStream, a_chunked_probe_body_degrades_from_the_streaming_branch(), a_chunked_probe_body_under_the_cap_is_read_and_parsed(), a_probe_body_over_the_cap_degrades_instead_of_failing(), a_success_body_at_the_cap_is_read_whole(), a_success_body_over_the_cap_fails_rather_than_arriving_truncated(), a_verdict_body_that_is_not_utf8_fails_instead_of_being_mangled(), an_error_body_over_the_cap_keeps_its_prefix_and_announces_the_cut() (+22 more)

### Community 289 - "String"
Cohesion: 0.27
Nodes (17): AgentRotationState, Candidate, cap(), caps_map(), digest_case(), digest_case_self(), digest_case_two_active(), digest_collision() (+9 more)

### Community 293 - "pattern7builderalias_bad.rs"
Cohesion: 0.67
Nodes (3): bare(), configured(), Client

### Community 294 - "pattern7builderalias_good.rs"
Cohesion: 0.67
Nodes (3): also_configured(), configured(), Client

## Knowledge Gaps
- **214 isolated node(s):** `magi-core`, `1. Three exit codes, and the difference between two of them`, `2. Two dependency modes`, `3. What the proxy does, and why its red is never the crate's`, `4. The contention probe, and its declared scope` (+209 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **77 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `LlmProvider` connect `LlmProvider` to `rotation.rs`, `RetryAfterProvider`, `.analyze`, `MockProvider`, `ProviderProbe`, `claude.rs`, `claude_cli.rs`, `test_support.rs`, `prelude.rs`, `ollama.rs`, `MagiBuilder`, `.new`, `Magi`, `SlowFailingProvider`, `provider.rs`, `Result`, `ProviderError`, `Instant`, `FallbackPool`?**
  _High betweenness centrality (0.058) - this node is a cross-community bridge._
- **Why does `run_catching()` connect `outcome.rs` to `runner.rs`, `Self`?**
  _High betweenness centrality (0.047) - this node is a cross-community bridge._
- **Why does `AgentName` connect `AgentName` to `rotation.rs`, `consensus.rs`, `.analyze`, `ProviderProbe`, `LlmProvider`, `schema.rs`, `test_support.rs`, `orchestrator.rs`, `MagiBuilder`, `Magi`, `String`, `Severity`, `MagiError`, `.cmp`, `embedded_prompt_for`, `prompts/mod.rs`, `Mode`, `.new`, `Result`, `ReportConfig`, `LineageRegistry`, `Arc`, `AgentOutput`?**
  _High betweenness centrality (0.041) - this node is a cross-community bridge._
- **What connects `magi-core`, `1. Three exit codes, and the difference between two of them`, `2. Two dependency modes` to the rest of the system?**
  _214 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `rotation.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.10661268556005399 - nodes in this community are weakly interconnected._
- **Should `reporting.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.07017543859649122 - nodes in this community are weakly interconnected._
- **Should `consensus.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.0989648033126294 - nodes in this community are weakly interconnected._