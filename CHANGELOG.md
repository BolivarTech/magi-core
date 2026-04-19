# Changelog

All notable changes to `magi-core` are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-04-18

### Changed (breaking)

- **Claude `opus` alias** now resolves to `claude-opus-4-7` (was `claude-opus-4-6`).
- **`Condition.condition`** is now sourced from `AgentOutput.summary` instead of
  `AgentOutput.recommendation`. Conditions are intended as short one-line blocking
  statements; full recommendations remain in the separate `recommendations` map.
- **`Validator`**: new `validate_mut(&self, &mut AgentOutput) -> Result<(), MagiError>`
  method. The orchestrator pipeline switched to use it so parsed agent outputs now
  flow through consensus with titles already cleaned in-place.
- **Consensus deduplication** no longer collapses interior whitespace. Titles
  differing by internal spacing (e.g., `"SQL injection"` vs `"SQL  injection"`)
  are now treated as distinct findings — aligned with Python MAGI 2.1.3. Dedup
  key uses NFKC normalization + full Unicode case-folding (`caseless` crate)
  instead of `to_lowercase()`.
- **`MagiConfig::max_input_len` default** raised from 1 MB (`1_048_576`) to
  4 MB (`4 * 1024 * 1024`). Consumers exposing the library to untrusted input
  should lower it via `MagiBuilder::with_max_input_len`. Full 10 MB alignment
  with Python is deferred to v0.3.0 pending allocation audit.
- **Report output (markdown)** changes:
  - `## Consensus Summary` section removed. Consumers parsing the rendered
    markdown should read `consensus.majority_summary` from the JSON instead.
  - Dissent section renders one line per dissenter with the `summary` field
    only (no `reasoning` paragraph). The `reasoning` field remains in JSON
    output.
  - Findings section renders one line per finding with fixed-width marker (5)
    and severity (14) columns. No indented detail paragraph; detail remains
    in JSON.
  - `GO WITH CAVEATS` consensus label now includes split count:
    `GO WITH CAVEATS (2-1)`.
  - `majority_summary` entries prefixed with agent display name:
    `"Melchior: <summary> | Balthasar: <summary>"`.
- **Banner rendering**: agent labels now column-aligned to the longest label
  so verdicts start at the same column. Labels that exceed the inner width (50)
  are truncated with `"..."` while preserving the verdict suffix.

### Security considerations

- **`max_input_len` default raised from 1 MB to 4 MB.** Consumers that expose
  `analyze()` to untrusted input should explicitly lower this via
  `MagiBuilder::with_max_input_len(1_048_576)` or similar. See `docs/migration-v0.2.md`
  for the allocation-envelope analysis (peak ≈ 5× content size during the 3-agent
  parallel dispatch; 4 MB default produces ~20 MB peak).
- **`Validator::validate_mut` silently rewrites `Finding.title` in place.** The
  orchestrator pipeline now uses `validate_mut`, so `MagiReport.agents[i].findings[j].title`
  reflects the *cleaned* form (NFKC-ready, invisible-char-stripped) rather than
  the raw LLM output. Consumers that need the raw form must preserve it before
  passing to `Magi::analyze`.

### Added

- **`clean_title`** public function in `validate` module: strips invisible
  Unicode characters and normalizes control whitespace (tabs, newlines, etc.)
  to a single space, matching Python MAGI 2.1.3 semantics.
- **`ReportConfig::new_checked`** constructor that validates ASCII on all
  `agent_titles` values, returning `Result<Self, ReportError>`.
- **`ReportError`** enum for structured reporting errors
  (`NonAsciiTitle { agent, field, value }` variant).
- **`BANNER_WIDTH`** and **`BANNER_INNER`** public constants on `reporting`
  module.
- **`DEFAULT_MAX_INPUT_LEN`** public constant on `orchestrator` module.
- **78 new unit tests** covering zero-width handling, NFKC+casefold, banner
  alignment, fit_content edge cases, dedup ordering, and more. Total test
  count: 250 (up from 172).

### Deprecated

- **`Finding::stripped_title`** is now `#[deprecated(since = "0.2.0")]`. The
  method still exists as a shim over `validate::clean_title`, but with a
  **different character coverage** than v0.1.x — it now strips the Python
  MAGI `_ZERO_WIDTH_RE` set (U+200B-U+200F, U+2028-U+202F, U+2060-U+206F,
  U+FEFF, U+00AD) instead of the v0.1.x `ZERO_WIDTH_PATTERN` set (which
  covered Arabic/Syriac/Mongolian format marks). See
  `docs/migration-v0.2.md` for the full comparison. The method will be
  removed in v0.3.0.

### Dependencies

- New: `unicode-normalization = "~0.1.24"` (NFKC for dedup key).
- New: `caseless = "~0.2.2"` (full Unicode case-folding for dedup key,
  equivalent to Python `str.casefold()`).

### Not included (deferred to v0.3.0)

- **Prompt architecture consolidation** (9 prompt files → 3 mode-agnostic +
  prompt-injection hardening). Tracked in
  `planning/claude-plan-tdd-v0.3-prompts.md`.

## [0.1.2] - 2026-04-05

- Initial public release. 172 tests. MAGI review STRONG GO (unanimous, round 3).
