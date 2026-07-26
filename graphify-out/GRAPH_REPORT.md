# Graph Report - MAGI-Core  (2026-07-25)

## Corpus Check
- 36 files · ~67,252 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 1237 nodes · 2808 edges · 113 communities (49 shown, 64 thin omitted)
- Extraction: 100% EXTRACTED · 0% INFERRED · 0% AMBIGUOUS · INFERRED: 9 edges (avg confidence: 0.79)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `b437fe94`
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
- [1.0.1] - 2026-05-25
- [1.1.1] - 2026-07-17
- .new
- error.rs
- magi-core
- normalize_newlines
- MAGI System — Complete Technical Documentation
- ProviderError
- 1. Origin: The MAGI Supercomputers from Evangelion
- Quick Start
- 4. Library Architecture
- 5. Data Schema and Consensus Protocol
- 7. Design Philosophy
- FixedRng
- AgentFactory
- MockProvider
- ProviderError
- backoff.rs
- bool
- bytes
- Voting rules + confidence formula
- Evangelion MAGI origin (Naoko Akagi)
- MAGI System Technical Documentation
- Structured disagreement rationale
- Why three perspectives (not 2 or 5)
- basic_analysis example
- CompletionConfig
- MockProvider
- .dispatch_with_retry
- ClaudeProvider::build_request_body
- ClaudeCliProvider::build_args
- ClaudeProvider::map_status_to_error
- ClaudeProvider::parse_response
- OpenAiCompatibleProvider::auth_header
- OpenAiCompatibleProvider::build_request_body
- OpenAiCompatibleProvider::endpoint_url
- OpenAiCompatibleProvider::map_status_to_error
- .cmp
- int
- int
- str
- str
- MAGI_REF_SHA pin (Python MAGI v3.0.0)
- _magi_ref.py (single source of truth)
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
- Duration
- String
- Vec
- LlmProvider
- Mode
- Mutex
- CompletionConfig
- Send
- [1.1.0] - 2026-05-25
- LlmProvider

## God Nodes (most connected - your core abstractions)
1. `AgentName` - 35 edges
2. `MagiBuilder` - 34 edges
3. `make_consensus()` - 33 edges
4. `make_agent()` - 32 edges
5. `LlmProvider` - 31 edges
6. `build_user_prompt()` - 28 edges
7. `parse_agent_response()` - 25 edges
8. `MagiError` - 24 edges
9. `AgentOutput` - 23 edges
10. `Magi` - 23 edges

## Surprising Connections (you probably didn't know these)
- `main()` --calls--> `default_model_for_mode()`  [INFERRED]
  examples/basic_analysis.rs → src/provider.rs
- `create_provider()` --references--> `LlmProvider`  [EXTRACTED]
  examples/basic_analysis.rs → src/provider.rs
- `parse_mode()` --references--> `Mode`  [EXTRACTED]
  examples/basic_analysis.rs → src/schema.rs
- `dispatch_one_agent()` --references--> `CompletionConfig`  [EXTRACTED]
  src/orchestrator.rs → src/provider.rs
- `MagiConfig` --references--> `CompletionConfig`  [EXTRACTED]
  src/orchestrator.rs → src/provider.rs

## Import Cycles
- None detected.

## Communities (113 total, 64 thin omitted)

### Community 0 - "orchestrator.rs"
Cohesion: 0.14
Nodes (31): AgentOutput, mock_agent_json(), parse_agent_response(), parse_and_validate(), test_lookup_prompt_falls_back_to_embedded_default_when_no_override(), test_lookup_prompt_falls_back_to_mode_agnostic_when_mode_specific_missing(), test_lookup_prompt_prefers_mode_specific_override(), test_lookup_prompt_returns_correct_embedded_default_per_agent() (+23 more)

### Community 1 - "reporting.rs"
Cohesion: 0.07
Nodes (64): fit_content(), MagiReport, make_agent(), make_consensus(), ReportConfig, ReportError, ReportFormatter, BTreeMap (+56 more)

### Community 2 - "consensus.rs"
Cohesion: 0.09
Nodes (69): Default, MagiError, Condition, ConsensusConfig, ConsensusEngine, ConsensusResult, dedup_key(), DedupFinding (+61 more)

### Community 3 - "validate.rs"
Cohesion: 0.06
Nodes (44): Error, From, MagiError, Self, clean_title(), finding_with_title(), output_with_confidence(), output_with_findings() (+36 more)

### Community 4 - "Finding"
Cohesion: 0.14
Nodes (33): F, MagiReport, test_analyze_all_agents_fail_returns_insufficient_agents(), test_analyze_applies_mode_agnostic_override_to_melchior(), test_analyze_input_too_large_rejects_without_launching_agents(), test_analyze_no_retry_on_timeout_keeps_retried_empty(), test_analyze_nonce_collision_returns_invalid_input(), test_analyze_one_agent_bad_json_degrades_gracefully() (+25 more)

### Community 5 - ".new"
Cohesion: 0.09
Nodes (34): Send, Agent, AgentFactory, MockProvider, Arc, AtomicUsize, BTreeMap, Default (+26 more)

### Community 6 - "provider.rs"
Cohesion: 0.06
Nodes (55): Arc, AtomicU32, AtomicUsize, Duration, Mode, Mutex, ProviderError, Result (+47 more)

### Community 8 - "openai_compat.rs"
Cohesion: 0.09
Nodes (32): OpenAiChoice, OpenAiCompatibleProvider, OpenAiMessage, OpenAiRequest, OpenAiRespMessage, OpenAiResponse, Client, Debug (+24 more)

### Community 9 - "claude.rs"
Cohesion: 0.09
Nodes (35): ClaudeMessage, ClaudeProvider, ClaudeRequest, ClaudeResponse, ContentBlock, Client, Debug, Duration (+27 more)

### Community 10 - "user_prompt.rs"
Cohesion: 0.07
Nodes (13): build_retry_prompt(), test_build_retry_prompt_appends_feedback_block_exact_format(), test_build_retry_prompt_does_not_neutralize_midline_tokens(), test_build_retry_prompt_does_not_resanitize_content(), test_build_retry_prompt_feedback_block_after_end_delimiter(), test_build_retry_prompt_includes_seven_keys_list(), test_build_retry_prompt_neutralizes_dash_variant_retry_markers(), test_build_retry_prompt_neutralizes_injected_retry_feedback_marker() (+5 more)

### Community 11 - "claude_cli.rs"
Cohesion: 0.13
Nodes (29): ClaudeCliProvider, CliOutput, parse_cli_output(), F, Into, Result, Self, String (+21 more)

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
Cohesion: 0.17
Nodes (12): Into, Finding, String, test_agent_name_btreeset_orders_alphabetically(), test_agent_name_usable_as_btreemap_key(), test_finding_new_constructs_three_field_finding(), test_finding_new_defaults_optional_fields(), test_finding_serde_roundtrip() (+4 more)

### Community 18 - "mod.rs"
Cohesion: 0.13
Nodes (15): [0.1.2] - 2026-04-05, [0.3.1] - 2026-04-19, [1.0.1] - 2026-05-25, [1.1.1] - 2026-07-17, [2.0.0] - 2026-07-25, Added, BREAKING, Changed (+7 more)

### Community 20 - "MAGI System Technical Documentation"
Cohesion: 0.40
Nodes (4): Exemption from CLAUDE.local.md §0.2 file-header rule, Local divergence from the pinned reference (F0, 2026-07-16), Regeneration, `src/prompts_md/` — Embedded prompt data

### Community 21 - "prelude.rs"
Cohesion: 0.08
Nodes (18): create_provider(), main(), parse_mode(), print_usage(), ProviderArgs, read_input(), Arc, Box (+10 more)

### Community 22 - "finding_id.rs"
Cohesion: 0.23
Nodes (13): D, de_category(), de_opt_file(), de_opt_line(), generate_finding_id(), normalize_category(), normalize_path(), Error (+5 more)

### Community 23 - ".cmp"
Cohesion: 0.24
Nodes (6): Display, Formatter, Ord, PartialOrd, Result, Severity

### Community 25 - "LlmProvider"
Cohesion: 0.19
Nodes (9): Instant, AbandonReason, ProviderError, Duration, Option, String, Vec, MockProvider (+1 more)

### Community 27 - "make_output"
Cohesion: 0.20
Nodes (9): AgentOutput, make_output(), Vec, test_agent_output_conditional_is_not_dissenting_from_approve_majority(), test_agent_output_effective_verdict_maps_conditional_to_approve(), test_agent_output_empty_findings_valid(), test_agent_output_is_dissenting_when_verdict_differs_from_majority(), test_agent_output_is_not_dissenting_when_verdict_matches_majority() (+1 more)

### Community 28 - "[0.5.0] - 2026-05-16"
Cohesion: 0.25
Nodes (8): [0.5.0] - 2026-05-16, Added, Backward compatibility, Changed (breaking), Documentation, Performance, Pre-merge gates (CLAUDE.local.md §6), Test count

### Community 30 - "[0.2.0] - 2026-04-18"
Cohesion: 0.29
Nodes (7): [0.2.0] - 2026-04-18, Added, Changed (breaking), Dependencies, Deprecated, Not included (deferred to v0.3.0), Security considerations

### Community 31 - "lib.rs"
Cohesion: 0.12
Nodes (26): AgentFactory, AgentName, Box, ComplexityGate, ConsensusConfig, ConsensusEngine, HashMap, PathBuf (+18 more)

### Community 32 - "[0.4.0] - 2026-05-16"
Cohesion: 0.29
Nodes (7): [0.4.0] - 2026-05-16, Added, Backward compatibility, Changed, Documentation, Performance, Test count

### Community 33 - "[0.3.0] - 2026-04-18"
Cohesion: 0.33
Nodes (6): [0.3.0] - 2026-04-18, Added, Changed (breaking), Dependencies, Not included (deferred beyond v0.3.0), Security considerations (MAGI R3 W8)

### Community 34 - "[0.6.0] - 2026-05-21"
Cohesion: 0.33
Nodes (6): [0.6.0] - 2026-05-21, Backward compatibility, Changed, Pre-merge gates (CLAUDE.local.md §6), Security, Test count

### Community 35 - "[1.0.0] - 2026-05-24"
Cohesion: 0.40
Nodes (5): [1.0.0] - 2026-05-24, Added, Changed (breaking), Notes, Security

### Community 36 - "[0.3.1] - 2026-04-19"
Cohesion: 0.19
Nodes (9): balthasar_prompt(), caspar_prompt(), embedded_prompt_for(), lookup_prompt(), melchior_prompt(), BTreeMap, Option, String (+1 more)

### Community 38 - "[1.1.1] - 2026-07-17"
Cohesion: 0.14
Nodes (24): Sized, build_user_prompt(), fixed_nonce(), Result, Self, Vec, test_build_user_prompt_accepts_empty_content(), test_build_user_prompt_all_5_unicode_separators_positive_neutralization() (+16 more)

### Community 39 - ".new"
Cohesion: 0.18
Nodes (16): dispatch_one_agent(), MagiConfig, Default, Duration, test_dispatch_one_agent_does_not_retry_on_auth_error(), test_dispatch_one_agent_does_not_retry_on_http_429(), test_dispatch_one_agent_does_not_retry_on_http_500(), test_dispatch_one_agent_does_not_retry_on_nested_session() (+8 more)

### Community 40 - "error.rs"
Cohesion: 0.24
Nodes (12): RoutingMockProvider, Default, HashMap, Mutex, Result, Self, String, Vec (+4 more)

### Community 41 - "magi-core"
Cohesion: 0.15
Nodes (13): Architecture, Changelog, Consensus Labels, Contribution, Example, Feature Flags, Features, Implementing a Custom Provider (+5 more)

### Community 42 - "normalize_newlines"
Cohesion: 0.24
Nodes (10): Cow, neutralize_headers(), normalize_newlines(), String, sanitize_error_for_retry_feedback(), strip_invisibles(), test_neutralize_headers_preserves_unmatched_lines_borrowed(), test_normalize_newlines_handles_empty_string() (+2 more)

### Community 43 - "MAGI System — Complete Technical Documentation"
Cohesion: 0.05
Nodes (37): 1.1 Context in the Series, 1.2 The Three Units, 1.3 Decision Mechanism, 1.4 The Philosophical Principle, 1.5 Why Structured Disagreement Works, 1. Origin: The MAGI Supercomputers from Evangelion, 2.1 Conceptual Mapping, 2.2 Why Three Perspectives and Not Two or Five (+29 more)

### Community 46 - "Quick Start"
Cohesion: 0.33
Nodes (6): Basic Usage, Cost Control with Complexity Gate, Custom System Prompts, Quick Start, Using the Built-in Claude CLI Provider, With Builder

### Community 50 - "FixedRng"
Cohesion: 0.22
Nodes (7): FastrandSource, FixedRng, RngLike, Send, test_fastrand_source_returns_distinct_values_across_calls(), test_fixed_rng_panics_when_exhausted(), VecDeque

### Community 54 - "backoff.rs"
Cohesion: 0.16
Nodes (26): FnMut, Option, fixed(), next_backoff(), parse_retry_after(), RetryAfter, RetryClass, Duration (+18 more)

### Community 66 - ".dispatch_with_retry"
Cohesion: 0.22
Nodes (6): AbortHandle, Agent, BTreeMap, BTreeSet, Drop, AbortGuard

### Community 76 - ".cmp"
Cohesion: 0.50
Nodes (3): Ordering, Option, Self

### Community 112 - "[1.1.0] - 2026-05-25"
Cohesion: 0.67
Nodes (3): [1.1.0] - 2026-05-25, Added, Notes

## Knowledge Gaps
- **153 isolated node(s):** `BREAKING`, `Added`, `Changed`, `Fixed`, `Changed` (+148 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **64 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `LlmProvider` connect `.new` to `Finding`, `provider.rs`, `openai_compat.rs`, `claude.rs`, `error.rs`, `claude_cli.rs`, `prelude.rs`, `LlmProvider`, `lib.rs`?**
  _High betweenness centrality (0.084) - this node is a cross-community bridge._
- **Why does `AgentName` connect `.new` to `reporting.rs`, `consensus.rs`, `[0.3.1] - 2026-04-19`, `schema.rs`, `error.rs`, `.cmp`, `.cmp`, `make_output`?**
  _High betweenness centrality (0.055) - this node is a cross-community bridge._
- **Why does `MagiBuilder` connect `lib.rs` to `orchestrator.rs`, `.dispatch_with_retry`, `Finding`, `.new`, `.new`?**
  _High betweenness centrality (0.043) - this node is a cross-community bridge._
- **What connects `BREAKING`, `Added`, `Changed` to the rest of the system?**
  _159 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `orchestrator.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.13911290322580644 - nodes in this community are weakly interconnected._
- **Should `reporting.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.07277834525025537 - nodes in this community are weakly interconnected._
- **Should `consensus.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.08691308691308691 - nodes in this community are weakly interconnected._