# Graph Report - MAGI-Core  (2026-07-31)

## Corpus Check
- 138 files · ~135,467 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 2191 nodes · 4677 edges · 289 communities (138 shown, 151 thin omitted)
- Extraction: 99% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 24 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `35799c7a`
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
- 4. Library Architecture
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
- Arc
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
- Architecture
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
- P
- Sync
- Display
- Formatter
- BTreeSet
- provider_url.rs
- AgentName
- pattern0nonterminal_bad.rs
- pattern0nonterminal_good.rs
- .new_checked
- FieldWriter
- prelude.rs
- AgentName
- AtomicUsize
- Default
- HashMap
- Lineage
- MagiReport
- Mutex
- RetryClass
- From
- Instant
- .new
- .new
- Into
- Arc
- Default
- Mutex
- ProviderError
- Send
- Client
- Debug
- ProviderError
- RequestBuilder
- Response
- Url
- FieldWriter
- Option
- Result
- compose_transport_message
- Vec
- .with_limits
- Self
- Duration
- Error
- Mode
- Self
- String
- Vec
- Into
- Option
- ProviderUrl
- Option
- serve_once
- prelude.rs
- Debug
- Duration
- Formatter
- Instant
- Vec
- ProviderProbe
- Vec
- log_failure
- String
- TcpListener
- .complete
- Duration
- .send
- described
- Result
- .new_checked
- pattern8bnospace_bad.rs
- pattern8bnospace_good.rs
- Arc
- AtomicUsize
- Default
- Duration
- HashMap
- Lineage
- MagiReport
- Mode
- Mutex
- Option
- ProviderError
- ProviderProbe
- Result
- Self
- Send
- String
- Vec
- String
- pattern7builderalias_bad.rs
- pattern7builderalias_good.rs
- pattern7default_bad.rs
- pattern7default_good.rs
- String

## God Nodes (most connected - your core abstractions)
1. `AgentName` - 48 edges
2. `MagiBuilder` - 42 edges
3. `make_consensus()` - 35 edges
4. `make_agent()` - 34 edges
5. `LlmProvider` - 29 edges
6. `MagiError` - 28 edges
7. `build_user_prompt()` - 28 edges
8. `Lineage` - 27 edges
9. `Magi` - 26 edges
10. `dispatch_one_agent()` - 26 edges

## Surprising Connections (you probably didn't know these)
- `main()` --calls--> `default_model_for_mode()`  [INFERRED]
  examples/basic_analysis.rs → src/provider.rs
- `f()` --calls--> `compose_transport_message()`  [INFERRED]
  ci/fixtures/redaction/pattern1debug_good.rs → src/provider.rs
- `f()` --calls--> `compose_transport_message()`  [INFERRED]
  ci/fixtures/redaction/pattern1display_good.rs → src/provider.rs
- `f()` --calls--> `compose_transport_message()`  [INFERRED]
  ci/fixtures/redaction/pattern1tostring_good.rs → src/provider.rs
- `an_external_failure_rotates_the_seat_and_the_run_completes()` --calls--> `build_trio_with_caspar()`  [INFERRED]
  tests/rotation_integration.rs → src/test_support.rs

## Import Cycles
- None detected.

## Communities (289 total, 151 thin omitted)

### Community 0 - "orchestrator.rs"
Cohesion: 0.09
Nodes (58): ExtractionFailureCause, InputSize, a_threshold_is_unreachable_when_the_smallest_warning_input_is_already_rejected(), a_threshold_that_can_never_fire_is_detected(), a_zero_threshold_warns_always_and_does_not_disable(), an_empty_input_never_exceeds_even_a_zero_threshold(), dispatch_one_agent(), echoed_example_response() (+50 more)

### Community 1 - "reporting.rs"
Cohesion: 0.05
Nodes (86): Condition, ConsensusResult, Dissent, a_clean_run_gains_no_section_at_all(), a_report_from_before_this_version_also_gains_nothing(), a_report_from_the_previous_version_deserializes_as_none_not_zero(), an_exceeded_run_gains_exactly_one_section_naming_both_numbers(), ExtractionFailure (+78 more)

### Community 2 - "consensus.rs"
Cohesion: 0.09
Nodes (66): ConsensusConfig, ConsensusEngine, dedup_key(), DedupFinding, DedupKey, finding_key(), make_output(), BTreeMap (+58 more)

### Community 3 - "validate.rs"
Cohesion: 0.06
Nodes (42): From, MagiError, clean_title(), finding_with_title(), output_with_confidence(), output_with_findings(), Default, Result (+34 more)

### Community 4 - "Finding"
Cohesion: 0.09
Nodes (39): BTreeSet, F, MagiReport, report_with_one_failing_agent(), test_a_2_2_0_report_without_the_field_still_deserializes(), test_a_clean_run_adds_no_section_to_the_human_report(), test_a_failure_before_rotation_is_attributed_to_the_pre_rotation_model(), test_a_recovered_retry_still_records_its_cause() (+31 more)

### Community 5 - ".new"
Cohesion: 0.13
Nodes (9): Barrier, FailingProbe, MockProbe, MockProvider, OverlapProbe, AtomicUsize, Formatter, ProviderError (+1 more)

### Community 6 - "provider.rs"
Cohesion: 0.19
Nodes (16): AgentName, P, Mode, Mutex, CapturingMockProvider, contract_prompt(), FieldWriter, test_analyze_applies_mode_agnostic_override_to_melchior() (+8 more)

### Community 7 - "schema.rs"
Cohesion: 0.05
Nodes (6): make_output(), test_agent_output_conditional_is_not_dissenting_from_approve_majority(), test_agent_output_effective_verdict_maps_conditional_to_approve(), test_agent_output_empty_findings_valid(), test_agent_output_is_dissenting_when_verdict_differs_from_majority(), test_agent_output_is_not_dissenting_when_verdict_matches_majority()

### Community 8 - "openai_compat.rs"
Cohesion: 0.17
Nodes (27): AgentRotationState, Candidate, cap(), caps_map(), digest_case(), digest_case_self(), digest_case_two_active(), empty_s() (+19 more)

### Community 9 - "claude.rs"
Cohesion: 0.09
Nodes (36): Debug, Field, Instant, ClaudeMessage, ClaudeProvider, ClaudeRequest, ClaudeResponse, ContentBlock (+28 more)

### Community 10 - "user_prompt.rs"
Cohesion: 0.06
Nodes (18): build_retry_prompt(), test_build_retry_prompt_appends_feedback_block_exact_format(), test_build_retry_prompt_does_not_neutralize_midline_tokens(), test_build_retry_prompt_does_not_resanitize_content(), test_build_retry_prompt_feedback_block_after_end_delimiter(), test_build_retry_prompt_includes_seven_keys_list(), test_build_retry_prompt_neutralizes_dash_variant_retry_markers(), test_build_retry_prompt_neutralizes_injected_retry_feedback_marker() (+10 more)

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
Cohesion: 0.08
Nodes (45): HashMap, Magi, Beh, build_oversized_case(), build_schema_local_case(), build_trio_with_caspar(), build_two_5xx_with_local_fallbacks(), build_two_external_failing_no_fallback() (+37 more)

### Community 18 - "mod.rs"
Cohesion: 0.13
Nodes (17): AgentSlotGuard, FallbackCandidate, FallbackPool, FallbackPoolBuilder, ProviderProbe, RotationConfig, Arc, Drop (+9 more)

### Community 19 - "RoutingMockProvider"
Cohesion: 0.29
Nodes (14): create_provider(), main(), parse_mode(), print_usage(), ProviderArgs, read_input(), Arc, Box (+6 more)

### Community 20 - "MAGI System Technical Documentation"
Cohesion: 0.40
Nodes (4): Exemption from CLAUDE.local.md §0.2 file-header rule, Local divergence from the pinned reference (F0, 2026-07-16), Regeneration, `src/prompts_md/` — Embedded prompt data

### Community 22 - "finding_id.rs"
Cohesion: 0.09
Nodes (31): OllamaProvider, OpenAiCompatibleProvider, ProviderProbe, a_mounted_prefix_survives_both_spellings(), a_root_that_really_ends_in_v1_is_read_as_the_prefix(), every_accepted_spelling_yields_the_same_endpoints(), new_gives_the_inner_provider_the_real_credentials(), OllamaProvider (+23 more)

### Community 23 - ".cmp"
Cohesion: 0.11
Nodes (24): AgentFactory, Arc, Box, ComplexityGate, ConsensusConfig, ConsensusEngine, FallbackPool, Lineage (+16 more)

### Community 24 - "dispatch_one_agent"
Cohesion: 0.30
Nodes (19): Self, test_base_zero_with_three_retries_emits_exactly_four_requests(), test_budget_exhaustion_abandons_with_typed_reason(), test_honored_retry_after_can_overrun_a_small_budget(), test_max_retries_zero_does_not_retry(), test_operation_budget_zero_yields_single_attempt(), test_retry_after_beyond_cap_abandons_with_typed_reason(), test_retry_provider_does_not_retry_on_auth() (+11 more)

### Community 25 - "LlmProvider"
Cohesion: 0.12
Nodes (29): AbortHandle, Agent, AgentOutput, AgentRotation, BTreeMap, DispatchOutcome, Drop, ExternalErrorKind (+21 more)

### Community 27 - "make_output"
Cohesion: 0.40
Nodes (4): Adding a pair, One pair per ALTERNATIVE, not per rule, Redaction-gate fixtures, Verifying a change

### Community 28 - "[0.5.0] - 2026-05-16"
Cohesion: 0.25
Nodes (8): [0.5.0] - 2026-05-16, Added, Backward compatibility, Changed (breaking), Documentation, Performance, Pre-merge gates (CLAUDE.local.md §6), Test count

### Community 29 - "Release workflow (publish to crates.io)"
Cohesion: 0.05
Nodes (21): Fn, cause_label(), extract(), ExtractionFailureCause, is_marker_line(), locate(), locate_block(), normalize_line() (+13 more)

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
Nodes (33): Agent, AgentFactory, MockProvider, Arc, AtomicUsize, BTreeMap, Default, Option (+25 more)

### Community 34 - "[0.6.0] - 2026-05-21"
Cohesion: 0.33
Nodes (6): [0.6.0] - 2026-05-21, Backward compatibility, Changed, Pre-merge gates (CLAUDE.local.md §6), Security, Test count

### Community 35 - "[1.0.0] - 2026-05-24"
Cohesion: 0.40
Nodes (5): [1.0.0] - 2026-05-24, Added, Changed (breaking), Notes, Security

### Community 36 - "[0.3.1] - 2026-04-19"
Cohesion: 0.21
Nodes (13): D, de_category(), de_opt_file(), de_opt_line(), generate_finding_id(), normalize_category(), normalize_path(), Error (+5 more)

### Community 38 - "[1.1.1] - 2026-07-17"
Cohesion: 0.14
Nodes (24): Sized, build_user_prompt(), fixed_nonce(), Result, Self, Vec, test_build_user_prompt_accepts_empty_content(), test_build_user_prompt_all_5_unicode_separators_positive_neutralization() (+16 more)

### Community 39 - "[1.1.1] - 2026-07-17"
Cohesion: 0.17
Nodes (12): [0.1.2] - 2026-04-05, [0.3.1] - 2026-04-19, [2.2.0] - 2026-07-27, [3.0.0] - 2026-07-30, Added, BREAKING, Changed, Changed (+4 more)

### Community 40 - "error.rs"
Cohesion: 0.10
Nodes (25): ae(), reg(), run_preflight(), state(), test_5xx_does_not_count_toward_endpoint_down(), test_claim_next_none_leaves_registry_intact(), test_concurrent_claims_never_double_reserve_stress(), test_endpoint_down_latch_exactly_one_true_concurrent() (+17 more)

### Community 41 - "magi-core"
Cohesion: 0.17
Nodes (12): Changelog, Consensus Labels, Contribution, Credentials in a provider URL, Example, Feature Flags, Features, Implementing a Custom Provider (+4 more)

### Community 42 - "normalize_newlines"
Cohesion: 0.20
Nodes (12): neutralize_headers(), normalize_newlines(), Cow, String, sanitize_error_for_retry_feedback(), strip_invisibles(), test_neutralize_headers_not_bypassed_by_mongolian_vowel_separator(), test_neutralize_headers_preserves_unmatched_lines_borrowed() (+4 more)

### Community 43 - "MAGI System — Complete Technical Documentation"
Cohesion: 0.29
Nodes (4): 8. Evangelion Correspondence Table, 9. Relationship to the MAGI Python Plugin, MAGI System — Complete Technical Documentation, Multi-Perspective Analysis Library for Rust

### Community 44 - "FallbackPool"
Cohesion: 0.15
Nodes (8): Default, RetryClass, RetryConfig, RetryProvider, test_dangerous_config_is_announced_for_retry_after_cap_over_budget(), test_dangerous_config_is_announced_for_zero_base_delay(), test_dangerous_config_is_announced_for_zero_cap(), test_dangerous_config_is_announced_for_zero_retry_after_cap()

### Community 45 - "String"
Cohesion: 0.18
Nodes (12): Finding, Into, String, test_agent_name_btreeset_orders_alphabetically(), test_agent_name_usable_as_btreemap_key(), test_finding_new_constructs_three_field_finding(), test_finding_new_defaults_optional_fields(), test_finding_serde_roundtrip() (+4 more)

### Community 46 - "Quick Start"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 47 - "error.rs"
Cohesion: 0.08
Nodes (14): describe(), main(), MyBackend, String, a_message_of_exactly_the_cap_is_kept_whole(), a_short_external_message_survives_untouched(), AbandonReason, an_oversized_external_message_is_cut_and_says_so() (+6 more)

### Community 48 - "Lineage"
Cohesion: 0.11
Nodes (16): ActiveEntry, digest_collision(), empty(), Lineage, LineageRegistry, RegistryInner, RotationEvent, RotationKind (+8 more)

### Community 49 - "1. Origin: The MAGI Supercomputers from Evangelion"
Cohesion: 0.50
Nodes (3): Ordering, Option, Self

### Community 50 - "FixedRng"
Cohesion: 0.22
Nodes (7): FastrandSource, FixedRng, RngLike, Send, test_fastrand_source_returns_distinct_values_across_calls(), test_fixed_rng_panics_when_exhausted(), VecDeque

### Community 52 - ".validate"
Cohesion: 0.33
Nodes (6): [0.3.0] - 2026-04-18, Added, Changed (breaking), Dependencies, Not included (deferred beyond v0.3.0), Security considerations

### Community 54 - "backoff.rs"
Cohesion: 0.16
Nodes (26): FnMut, fixed(), next_backoff(), parse_retry_after(), RetryAfter, RetryClass, Duration, Option (+18 more)

### Community 65 - "5. Data Schema and Consensus Protocol"
Cohesion: 0.10
Nodes (21): balthasar_prompt(), caspar_prompt(), embedded_prompt_for(), lookup_prompt(), melchior_prompt(), BTreeMap, Option, Result (+13 more)

### Community 66 - "AgentName"
Cohesion: 0.67
Nodes (3): [1.0.1] - 2026-05-25, Fixed, Internal

### Community 67 - "2. Translation to the Software Engineering Domain"
Cohesion: 0.50
Nodes (4): [2.0.0] - 2026-07-25, Added, BREAKING, Changed

### Community 69 - "4. Library Architecture"
Cohesion: 0.67
Nodes (3): [1.1.1] - 2026-07-17, Changed, Fixed

### Community 70 - "7. Design Philosophy"
Cohesion: 0.05
Nodes (44): X, X, X, ProviderError, Result, X, Display, Method (+36 more)

### Community 72 - "3. The Three Agents in Detail"
Cohesion: 0.67
Nodes (3): [3.0.2] - 2026-07-30, Changed, Fixed

### Community 73 - "6. Modes of Operation"
Cohesion: 0.39
Nodes (5): fail(), prod_only(), self_test(), check_redaction.sh script, skeleton()

### Community 74 - "Model Rotation"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 75 - "MockProvider"
Cohesion: 0.07
Nodes (20): f(), f(), f(), compose_caps_the_head_even_with_no_cause_chain(), compose_does_not_truncate_when_under_the_cap(), compose_never_panics_on_multibyte_boundaries(), compose_puts_operation_and_url_first_and_truncates_the_cause_tail(), compose_transport_message() (+12 more)

### Community 80 - "[2.1.0] - 2026-07-27"
Cohesion: 0.67
Nodes (3): [2.1.0] - 2026-07-27, Added, Compatibility

### Community 81 - "[3.0.2] - 2026-07-30"
Cohesion: 0.40
Nodes (5): [3.1.0] - 2026-07-30, Changed, Documented, Fixed, Security

### Community 82 - "5. Data Schema and Consensus Protocol"
Cohesion: 0.40
Nodes (3): redacted(), String, X

### Community 103 - "Debug"
Cohesion: 0.40
Nodes (3): redacted(), String, X

### Community 109 - "Arc"
Cohesion: 0.29
Nodes (7): Basic Usage, Cost Control with Complexity Gate, Custom System Prompts, Quick Start, The Output Contract, Using the Built-in Claude CLI Provider, With Builder

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
Cohesion: 0.40
Nodes (5): 4.1 Module Structure, 4.2 Dependency Flow, 4.3 Execution Pipeline, 4.4 Concurrency Model, 4. Library Architecture

### Community 116 - "Instant"
Cohesion: 0.08
Nodes (32): P, Cli, Client, Formatter, Into, Option, ProviderUrl, absent_null_and_empty_content_are_all_a_named_schema_failure() (+24 more)

### Community 117 - "Into"
Cohesion: 0.38
Nodes (5): build(), ProviderError, Error, From, Self

### Community 118 - "Option"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 121 - "From"
Cohesion: 0.40
Nodes (5): 5.1 Agent Output Schema, 5.2 Voting Rules, 5.3 Confidence Formula, 5.4 Findings Deduplication, 5. Data Schema and Consensus Protocol

### Community 122 - "Box"
Cohesion: 0.40
Nodes (5): 7.1 Dissent is a Feature, 7.2 Adversarial by Design, 7.3 Proportionality, 7.4 LLM-Agnostic Design, 7. Design Philosophy

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
Cohesion: 0.50
Nodes (4): 3.1 Melchior — The Scientist, 3.2 Balthasar — The Pragmatist, 3.3 Caspar — The Critic, 3. The Three Agents in Detail

### Community 128 - "ProviderError"
Cohesion: 0.50
Nodes (4): 6.1 Code Review, 6.2 Design, 6.3 Analysis, 6. Modes of Operation

### Community 129 - "ProviderError"
Cohesion: 0.50
Nodes (4): Declaring fallbacks, Model Rotation, Ollama probe (feature `ollama`), What a rotation looks like

### Community 134 - "Option"
Cohesion: 0.20
Nodes (7): Attributes, Event, Id, Metadata, Record, EventLog, Subscriber

### Community 137 - "String"
Cohesion: 0.50
Nodes (3): ProviderUrl, redacted(), String

### Community 138 - "Vec"
Cohesion: 0.50
Nodes (4): 2.1 Conceptual Mapping, 2.2 Why Three Perspectives and Not Two or Five, 2.3 Addressing Cognitive Biases, 2. Translation to the Software Engineering Domain

### Community 152 - "Architecture"
Cohesion: 0.67
Nodes (3): Architecture, Module Dependency Graph, Prompt Injection Defense

### Community 165 - ".fmt"
Cohesion: 0.24
Nodes (6): Ord, PartialOrd, Display, Formatter, Result, Severity

### Community 170 - "[1.1.0] - 2026-05-25"
Cohesion: 0.67
Nodes (3): [1.1.0] - 2026-05-25, Added, Notes

### Community 183 - "provider_url.rs"
Cohesion: 0.18
Nodes (10): leaky(), String, leaky(), String, Error, cause_chain(), cause_chain_skips_the_top_level_error(), client_build_error() (+2 more)

### Community 188 - ".new_checked"
Cohesion: 0.40
Nodes (3): leaky(), Error, String

### Community 224 - "compose_transport_message"
Cohesion: 0.50
Nodes (3): Error, String, spawn_failure()

### Community 228 - "Duration"
Cohesion: 0.15
Nodes (12): AtomicU32, AtomicUsize, classify(), CompletionConfig, FailingProvider, is_retryable(), MockProvider, RetryAfterProvider (+4 more)

### Community 239 - "prelude.rs"
Cohesion: 0.06
Nodes (29): JoinHandle, TcpListener, TcpStream, a_chunked_probe_body_degrades_from_the_streaming_branch(), a_chunked_probe_body_under_the_cap_is_read_and_parsed(), a_probe_body_over_the_cap_degrades_instead_of_failing(), a_success_body_at_the_cap_is_read_whole(), a_success_body_over_the_cap_fails_rather_than_arriving_truncated() (+21 more)

### Community 266 - "Result"
Cohesion: 0.14
Nodes (11): CompletionConfig, ProviderError, Result, an_oversized_response_routes_to_its_own_mage_local_outcome(), every_external_shape_routes_to_its_own_mage_local_outcome(), is_connection(), MockProvider, provider_err_outcome() (+3 more)

### Community 271 - "pattern8bnospace_bad.rs"
Cohesion: 0.50
Nodes (4): build(), ProviderError, Self, sneak()

### Community 272 - "pattern8bnospace_good.rs"
Cohesion: 0.50
Nodes (3): build(), ProviderError, Self

### Community 293 - "pattern7builderalias_bad.rs"
Cohesion: 0.67
Nodes (3): bare(), configured(), Client

### Community 294 - "pattern7builderalias_good.rs"
Cohesion: 0.67
Nodes (3): also_configured(), configured(), Client

## Knowledge Gaps
- **199 isolated node(s):** `Security`, `Fixed`, `Changed`, `Documented`, `Fixed` (+194 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **151 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `MagiError` connect `validate.rs` to `[0.3.0] - 2026-04-18`, `consensus.rs`, `5. Data Schema and Consensus Protocol`, `provider.rs`, `[1.1.1] - 2026-07-17`, `user_prompt.rs`, `error.rs`, `.cmp`, `Instant`, `provider_url.rs`, `LlmProvider`?**
  _High betweenness centrality (0.037) - this node is a cross-community bridge._
- **Why does `AgentName` connect `reporting.rs` to `5. Data Schema and Consensus Protocol`, `[0.3.0] - 2026-04-18`, `consensus.rs`, `.fmt`, `schema.rs`, `error.rs`, `openai_compat.rs`, `Lineage`, `1. Origin: The MAGI Supercomputers from Evangelion`, `mod.rs`?**
  _High betweenness centrality (0.036) - this node is a cross-community bridge._
- **Why does `LlmProvider` connect `[0.3.0] - 2026-04-18` to `Duration`, `.new`, `error.rs`, `MockProvider`, `FallbackPool`, `.new_checked`, `claude_cli.rs`, `mod.rs`, `RoutingMockProvider`, `Instant`, `.cmp`, `dispatch_one_agent`?**
  _High betweenness centrality (0.025) - this node is a cross-community bridge._
- **What connects `Security`, `Fixed`, `Changed` to the rest of the system?**
  _204 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `orchestrator.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.08557692307692308 - nodes in this community are weakly interconnected._
- **Should `reporting.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.052355396541443056 - nodes in this community are weakly interconnected._
- **Should `consensus.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.08972972972972973 - nodes in this community are weakly interconnected._