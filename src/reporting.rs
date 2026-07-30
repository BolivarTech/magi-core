// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fmt::Write;

use crate::consensus::{Condition, ConsensusResult, DedupFinding, Dissent};
use crate::rotation::AgentRotation;
use crate::schema::{AgentName, AgentOutput, Mode};
use crate::verdict_markers::ExtractionFailureCause;

/// Error returned by `ReportConfig::new_checked` when validation fails.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReportError {
    /// An `agent_titles` value contains non-ASCII characters.
    ///
    /// The field name is either `"display_name"` or `"title"`,
    /// and `agent` identifies which agent's entry failed validation.
    #[non_exhaustive]
    NonAsciiTitle {
        /// The agent whose title failed validation.
        agent: AgentName,
        /// Which field: `"display_name"` or `"title"`.
        field: &'static str,
        /// The invalid value.
        value: String,
    },
    /// `banner_width` is below the minimum required for meaningful rendering.
    ///
    /// The minimum is 8: one `|` border + 6 content bytes + one `|` border.
    #[non_exhaustive]
    BannerTooSmall {
        /// The requested (invalid) width.
        requested: usize,
        /// The minimum accepted width.
        minimum: usize,
    },
}

impl fmt::Display for ReportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReportError::NonAsciiTitle {
                agent,
                field,
                value,
            } => write!(
                f,
                "agent_titles[{:?}].{} contains non-ASCII characters: {:?}",
                agent, field, value
            ),
            ReportError::BannerTooSmall { requested, minimum } => write!(
                f,
                "banner_width {requested} is below the minimum of {minimum}"
            ),
        }
    }
}

/// Default total width of the ASCII banner in bytes, including the two border `|` characters.
///
/// Equals [`BANNER_INNER`] + 2. Assumes ASCII content for correct visual alignment.
pub const BANNER_WIDTH: usize = 52;

/// Default inner width of the ASCII banner in bytes (between the two border `|` characters).
///
/// Equals [`BANNER_WIDTH`] - 2. All content lines are padded or truncated to exactly this width.
pub const BANNER_INNER: usize = BANNER_WIDTH - 2;

/// Left-justified width of the severity icon column (e.g., `[!!!]`, `[!!]`, `[i]`).
const FINDING_MARKER_WIDTH: usize = 5;

/// Fits `content` into exactly `width` bytes, preserving `preserve_suffix` when truncating.
///
/// # Preconditions (ASCII-only input)
///
/// - `content` and `preserve_suffix` must be ASCII.
///   `debug_assert!(content.is_ascii() && preserve_suffix.is_ascii())`
/// - `width > 0`.
///   `debug_assert!(width > 0)`
/// - `content` should end with `preserve_suffix` when Step 3 runs — but this is a
///   *caller expectation*, **not** an unchecked assumption. It is enforced at runtime by
///   `strip_suffix`: a violation logs a `tracing::warn!` and degrades to the Step 2 tail
///   cut, so the result is always a meaningful truncation of the real input.
///   *(This bullet used to say the prefix would be "arbitrary" and that a `debug_assert!`
///   guarded it. Both were the defect: an assert is absent in release, and "arbitrary" was
///   understating it — the function appended a suffix the content never carried. MAGI R2,
///   Melchior `[WARNING]`.)*
///   `debug_assert!(content.ends_with(preserve_suffix))` — checked at Step 3 entry.
///
/// # Post-condition
///
/// The byte length of the result is:
/// - `content.len()` when `content.len() <= width` (no truncation).
/// - `<= width` when `width >= 4` and truncation occurs. When `content` is ASCII the result is
///   exactly `width` bytes; for non-ASCII input `floor_char_boundary` may snap the cutoff to a
///   slightly smaller value (graceful degradation, no panic).
/// - May exceed `width` when `width < 4` (documented edge case: cannot fit one char + `"..."`).
///   Callers should ensure `width >= 4` for sensible truncation.
///
/// # Algorithm
///
/// 1. If `content.len() <= width`, return `content` unchanged.
/// 2. Fallback (tail-cut) applies when `preserve_suffix` is empty or
///    `preserve_suffix.len() + 3 >= width`:
///    `cutoff = max(1, width.saturating_sub(3))`, return `content[..cutoff] + "..."`.
/// 3. Otherwise prefix-truncate with suffix protected:
///    `prefix_budget = width - 3 - preserve_suffix.len()`,
///    return `prefix_source[..prefix_budget] + "..." + preserve_suffix`.
///
/// Truncates `content` from the tail and appends `ellipsis`, cutting only on a char
/// boundary.
///
/// Extracted because [`fit_content`] reaches it from two places — Step 2, and Step 3's
/// precondition fallback — and two copies of a truncation rule is one copy too many: the
/// day one grows a guard the other silently keeps the old behavior.
///
/// Total: `floor_char_boundary` cannot return an offset inside a codepoint, and the
/// `max(1)` keeps the cut positive when `width < ellipsis.len()`.
fn tail_cut(content: &str, width: usize, ellipsis: &str) -> String {
    let cutoff = (width.saturating_sub(ellipsis.len())).max(1);
    let safe_cutoff = content.floor_char_boundary(cutoff);
    format!("{}{}", &content[..safe_cutoff], ellipsis)
}

/// # Panics
///
/// Never. Every cut goes through `floor_char_boundary` or `strip_suffix`, so no byte
/// offset can land inside a codepoint. A violated precondition degrades to the Step 2
/// tail cut and emits a `tracing::warn!`; it does not panic and it does not invent text.
/// *(`3.0.0` shipped with this section saying the function panics in release when the
/// Step 3 precondition was broken. MAGI flagged that a `debug_assert!` leaves release
/// builds unguarded, so the guard became a real one.)*
fn fit_content(content: &str, width: usize, preserve_suffix: &str) -> String {
    debug_assert!(content.is_ascii() && preserve_suffix.is_ascii());
    debug_assert!(width > 0);
    debug_assert!(
        width >= 4,
        "fit_content requires width >= 4 for sensible truncation; got {}",
        width
    );

    const ELLIPSIS: &str = "...";

    // Step 1: no truncation needed
    if content.len() <= width {
        return content.to_string();
    }

    // Step 2: fallback tail-cut when no suffix or suffix + ellipsis fills width
    if preserve_suffix.is_empty() || preserve_suffix.len() + ELLIPSIS.len() >= width {
        return tail_cut(content, width, ELLIPSIS);
    }

    // Step 3: prefix-truncate with suffix protected.
    //
    // `strip_suffix` is what enforces the precondition — NOT a `debug_assert!`, which is
    // what this used to be. The old form lopped `preserve_suffix.len()` bytes off the tail
    // unconditionally, so a release build with the invariant broken would **fabricate**:
    // append a suffix the content never carried. Slicing at that offset could also land
    // inside a codepoint and panic — in a formatter, on the report that IS the product.
    // Degrading to Step 2's tail-cut is honest about what it dropped, and total.
    let Some(prefix_source) = content.strip_suffix(preserve_suffix) else {
        tracing::warn!(
            target: "magi_core::reporting",
            suffix = preserve_suffix,
            "fit_content: content does not end with preserve_suffix; \
             falling back to a tail cut rather than inventing the suffix"
        );
        return tail_cut(content, width, ELLIPSIS);
    };
    let prefix_budget = width - ELLIPSIS.len() - preserve_suffix.len();
    let safe_prefix_budget = prefix_source.floor_char_boundary(prefix_budget);
    format!(
        "{}{}{}",
        &prefix_source[..safe_prefix_budget],
        ELLIPSIS,
        preserve_suffix
    )
}

/// Left-justified width of the markdown severity label column (e.g., `**[CRITICAL]**`).
///
/// `**[CRITICAL]**` = 14 chars (fits exactly).
/// `**[WARNING]**`  = 13 chars (1 trailing space added by padding).
/// `**[INFO]**`     = 10 chars (4 trailing spaces added by padding).
const FINDING_SEVERITY_WIDTH: usize = 14;

/// Configuration for the report formatter.
///
/// Controls banner width and agent display names/titles.
///
/// # ASCII Constraint
///
/// The fixed-width banner guarantee (`banner_width` bytes per line) assumes
/// all displayed content is ASCII. Agent titles, verdict labels, and consensus
/// strings are ASCII by default. If `agent_titles` contains multi-byte UTF-8
/// characters, banner lines will have correct byte length but may appear
/// visually misaligned in terminals because multi-byte characters can occupy
/// more than one display column.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct ReportConfig {
    /// Total width of the ASCII banner in bytes, including border characters
    /// (default: 52). Equals character count for ASCII content.
    pub banner_width: usize,
    /// Maps agent name to (display_name, title) for report display.
    /// Values should be ASCII for correct banner alignment.
    pub agent_titles: BTreeMap<AgentName, (String, String)>,
}

/// Formats consensus results into ASCII banners and markdown reports.
///
/// Generates fixed-width ASCII banners (exactly 52 characters wide per line)
/// and full markdown reports from agent outputs and consensus results.
/// The reporting module is pure string formatting -- no async, no I/O.
pub struct ReportFormatter {
    config: ReportConfig,
    banner_inner: usize,
}

/// Final output struct returned by the orchestrator's `analyze()` method.
///
/// Contains all analysis data plus the formatted report string.
/// Serializes to JSON matching the Python original format.
///
/// **v0.4.0:** added `#[derive(Deserialize)]` for backward-compatible
/// loading of v0.3.x JSON (the missing `retried_agents` key defaults to
/// `BTreeSet::new()`).
///
/// **v1.0.0:** marked `#[non_exhaustive]` so downstream crates cannot
/// exhaustively match or construct this struct directly; new fields may
/// be added in minor versions without breaking changes.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MagiReport {
    /// The successful agent outputs used in analysis.
    pub agents: Vec<AgentOutput>,
    /// The computed consensus result.
    pub consensus: ConsensusResult,
    /// The ASCII banner string.
    pub banner: String,
    /// The full markdown report string.
    pub report: String,
    /// True if fewer than 3 agents succeeded.
    pub degraded: bool,
    /// Agents that failed, mapped to their failure reason
    /// (e.g., `"parse: no valid JSON"`, `"validation: confidence out of range"`).
    pub failed_agents: BTreeMap<AgentName, String>,
    /// Agents whose first attempt failed schema/parse validation and that
    /// were retried once. Included in JSON only if non-empty.
    ///
    /// Composes with `failed_agents` for two derived cohorts:
    /// - `retried_agents - failed_agents.keys()` → "retry recovered"
    /// - `retried_agents ∩ failed_agents.keys()` → "retry also failed"
    ///
    /// Python parity: `run_magi.py:485, 631-632` (v2.2.0 telemetry).
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub retried_agents: BTreeSet<AgentName>,

    /// **MS2** — per-agent rotation telemetry, populated for EVERY agent (successful
    /// or failed). Each entry carries `model_configured`/`model_used` and the ordered
    /// `chain` of hops (empty when the agent did not rotate). Always present in JSON.
    ///
    /// **Accepted limitation — telemetry on panic after rotation.** If an agent
    /// rotated one or more times and then its task **panicked** (or was cancelled),
    /// its entry shows the pre-seed (empty `chain`, `model_used == model_configured`),
    /// NOT the hops it made before panicking — that in-flight chain lived on the
    /// panicked task's stack and is unrecoverable. Capturing it would require a
    /// shared mutable telemetry sink (a second lock outside the registry), which the
    /// single-lock concurrency design deliberately forbids. The panic itself is the
    /// headline (it surfaces in `failed_agents`); the exact pre-panic rotation trail
    /// of a *failed* mage is a minor diagnostic loss, accepted by design.
    #[serde(default)]
    pub rotations: BTreeMap<AgentName, AgentRotation>,

    /// **MS3** — per-agent record of every output rejected before it could enter
    /// consensus, in the order the attempts happened.
    ///
    /// # Always present, seeded for every dispatched agent
    ///
    /// An empty `Vec` means that seat was clean, and saying so is the point: the field's
    /// job is to **certify** that all three agents adhered to the verdict-marker
    /// contract. A certificate that disappears exactly when everything went well is not
    /// a certificate — it is the absence of a claim. Hence `#[serde(default)]` with **no**
    /// `skip_serializing_if`, unlike `retried_agents`, which it resembles structurally
    /// but not in purpose.
    ///
    /// # Read it JOINED with `rotations`, on `AgentName`
    ///
    /// The two are halves of one story: *why* a mage suffered, and *where* it ended up.
    /// Together they answer the question MS2 and MS3 only make answerable in combination
    /// — *did this seat fail extraction, rotate because of it, and which model did it
    /// finish on?* That join is why both fields follow the same presence rule.
    ///
    /// # Records, not counts — because the model is the actionable dimension
    ///
    /// A `BTreeMap<cause, usize>` would say *"Caspar had two `MissingMarkers`"*, but with
    /// rotation that seat may have tried two or three different models. The question that
    /// decides something is **which MODEL fails to adhere**, because that is what gets
    /// dropped from the pool. Counts erase exactly that attribution, and per-seat or
    /// per-cause totals are trivially derivable from these records anyway.
    ///
    /// # An empty `Vec` means "no EXTRACTION failure", not "no failure"
    ///
    /// Read alone, an empty vector looks like a clean seat. It is narrower than that: it
    /// certifies only that nothing failed *extraction*. A mage that panicked, timed out,
    /// or died on transport never produced output to extract from, so its vector is empty
    /// while the run is degraded. `failed_agents` is what says whether the seat produced a
    /// verdict at all; this field says why the text it produced was rejected, if it was.
    /// Both are needed, and reading either in isolation gives a confident wrong answer.
    /// *(MAGI S3, Caspar `[INFO]`.)*
    #[serde(default)]
    pub extraction_failures: BTreeMap<AgentName, Vec<ExtractionFailure>>,
}

/// Maps an [`ExtractionFailureCause`] to a short human label for the
/// `## Extraction Failures` section.
///
/// `Other` is the enum's own catch-all for a cause a NEWER crate version produced — never
/// constructed here, but the `_` arm is required because the enum is `#[non_exhaustive]`,
/// and `"other"` is the truthful label for something this version cannot name.
fn cause_label(cause: ExtractionFailureCause) -> &'static str {
    match cause {
        ExtractionFailureCause::MissingMarkers => "missing markers",
        ExtractionFailureCause::Unterminated => "unterminated block",
        ExtractionFailureCause::Ambiguous => "ambiguous markers",
        ExtractionFailureCause::InvalidJson => "invalid JSON",
        ExtractionFailureCause::Schema => "schema mismatch",
        ExtractionFailureCause::EchoedExample => "echoed example",
        ExtractionFailureCause::AgentIdentity => "agent identity mismatch",
        _ => "other",
    }
}

/// One rejected agent output, attributed to the **model** that produced it.
///
/// `#[non_exhaustive]` like every public struct since 1.0.0 (ADR 005): it can gain fields
/// in a minor without breaking consumers.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractionFailure {
    /// The model id that produced the rejected output — **not** the declared lineage.
    ///
    /// Recorded from the agent's rotation state **at the moment of the failure**, before
    /// any rotation mutates it. Reading it later would attribute a pre-rotation failure
    /// to the model that had not run yet, i.e. the telemetry would say the opposite of
    /// the truth about the only question it exists to answer.
    pub model: String,
    /// `1` for the first attempt with **this model**, `2` for the corrective retry with
    /// the **same** model.
    ///
    /// It **restarts at 1 after a rotation**: the retry-then-rotate cycle is per-model, so
    /// a seat that fails twice on `M1`, rotates, and fails once on `M2` yields
    /// `[{M1,1}, {M1,2}, {M2,1}]` — never `{M2,3}`. A global counter would make the
    /// reading *"this model failed on its very first attempt"* unrecoverable, and that
    /// reading is what the field is for.
    pub attempt: u8,
    /// Why the output was rejected.
    pub cause: ExtractionFailureCause,
}

impl ReportConfig {
    /// Minimum `banner_width` accepted by [`ReportConfig::new_checked`].
    ///
    /// Allows `|` + 6 content bytes + `|` = 8 bytes minimum for any meaningful banner.
    pub const MIN_BANNER_WIDTH: usize = 8;

    /// Creates a `ReportConfig` with validated `banner_width` and ASCII `agent_titles`.
    ///
    /// Returns `Err(ReportError::BannerTooSmall)` if `banner_width < 8`.
    /// Returns `Err(ReportError::NonAsciiTitle)` if any display name or title in
    /// `agent_titles` contains non-ASCII characters.
    ///
    /// This allows `fit_content` to assume ASCII without run-time panic and
    /// ensures the banner is at least minimally renderable.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut titles = BTreeMap::new();
    /// titles.insert(AgentName::Melchior, ("Melchior".to_string(), "Scientist".to_string()));
    /// let config = ReportConfig::new_checked(52, titles)?;
    /// ```
    pub fn new_checked(
        banner_width: usize,
        agent_titles: BTreeMap<AgentName, (String, String)>,
    ) -> Result<Self, ReportError> {
        Self::check_parts(banner_width, &agent_titles)?;
        Ok(Self {
            banner_width,
            agent_titles,
        })
    }

    /// THE validity rule for a report config — one definition, two callers.
    ///
    /// [`ReportConfig::new_checked`] and [`ReportFormatter::with_config`] used to carry
    /// byte-identical copies of this. Two copies of a validation rule is one too many:
    /// the day one grows a check, the other keeps accepting what the first now rejects,
    /// and the gap shows up as a malformed banner rather than an error. MAGI S2,
    /// Balthasar `[INFO]`.
    ///
    /// # Errors
    ///
    /// [`ReportError::BannerTooSmall`] when `banner_width < MIN_BANNER_WIDTH`;
    /// [`ReportError::NonAsciiTitle`] when any display name or title is not ASCII —
    /// the banner's byte-per-line invariant assumes ASCII content.
    pub(crate) fn check_parts(
        banner_width: usize,
        agent_titles: &BTreeMap<AgentName, (String, String)>,
    ) -> Result<(), ReportError> {
        if banner_width < Self::MIN_BANNER_WIDTH {
            return Err(ReportError::BannerTooSmall {
                requested: banner_width,
                minimum: Self::MIN_BANNER_WIDTH,
            });
        }
        for (agent, (display_name, title)) in agent_titles {
            if !display_name.is_ascii() {
                return Err(ReportError::NonAsciiTitle {
                    agent: *agent,
                    field: "display_name",
                    value: display_name.clone(),
                });
            }
            if !title.is_ascii() {
                return Err(ReportError::NonAsciiTitle {
                    agent: *agent,
                    field: "title",
                    value: title.clone(),
                });
            }
        }
        Ok(())
    }
}

impl Default for ReportConfig {
    fn default() -> Self {
        let mut agent_titles = BTreeMap::new();
        agent_titles.insert(
            AgentName::Melchior,
            ("Melchior".to_string(), "Scientist".to_string()),
        );
        agent_titles.insert(
            AgentName::Balthasar,
            ("Balthasar".to_string(), "Pragmatist".to_string()),
        );
        agent_titles.insert(
            AgentName::Caspar,
            ("Caspar".to_string(), "Critic".to_string()),
        );
        Self {
            banner_width: 52,
            agent_titles,
        }
    }
}

impl ReportFormatter {
    /// Internal infallible constructor — assumes `config` is already validated.
    fn from_valid_config(config: ReportConfig) -> Self {
        let banner_inner = config.banner_width - 2;
        Self {
            config,
            banner_inner,
        }
    }

    /// Creates a new formatter with default configuration.
    ///
    /// Infallible because [`ReportConfig::default`] always produces a valid
    /// configuration with an 8-byte-or-larger banner width and ASCII agent titles.
    pub fn new() -> Self {
        Self::from_valid_config(ReportConfig::default())
    }

    /// Creates a new formatter with a validated custom configuration.
    ///
    /// Re-runs the same ASCII and minimum-width checks as
    /// [`ReportConfig::new_checked`], so callers who mutate a `ReportConfig`
    /// after construction (e.g., via `default()` + field assignment) cannot
    /// bypass validation.
    ///
    /// # Errors
    ///
    /// Returns [`ReportError::BannerTooSmall`] if `config.banner_width < 8`.
    /// Returns [`ReportError::NonAsciiTitle`] if any agent title contains
    /// non-ASCII characters.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let cfg = ReportConfig::default();
    /// let fmt = ReportFormatter::with_config(cfg).expect("default config is valid");
    /// ```
    pub fn with_config(config: ReportConfig) -> Result<Self, ReportError> {
        // One definition of validity, shared with `ReportConfig::new_checked` — see
        // `check_parts` for why this is not duplicated here.
        ReportConfig::check_parts(config.banner_width, &config.agent_titles)?;
        Ok(Self::from_valid_config(config))
    }

    /// Generates the fixed-width ASCII verdict banner with column-aligned agent labels.
    ///
    /// Every line is exactly `banner_width` (52) characters. Agent labels are
    /// left-justified to the same width (`max_label_len`), so verdict suffixes
    /// start at the same column for all agents. When content overflows the inner
    /// width, the label is ellipsized while the verdict suffix is preserved intact.
    ///
    /// Structure:
    /// ```text
    /// +==================================================+
    /// |          MAGI SYSTEM -- VERDICT                  |
    /// +==================================================+
    /// |  Melchior (Scientist):  APPROVE (90%)            |
    /// |  Balthasar (Pragmatist): APPROVE (85%)           |
    /// |  Caspar (Critic):        APPROVE (78%)           |
    /// +==================================================+
    /// |  CONSENSUS: GO WITH CAVEATS (2-1)                |
    /// +==================================================+
    /// ```
    pub fn format_banner(&self, agents: &[AgentOutput], consensus: &ConsensusResult) -> String {
        let mut out = String::new();
        let sep = self.format_separator();

        // Step 1: compute per-agent labels and max label length
        let labels: Vec<String> = agents
            .iter()
            .map(|a| {
                let (display_name, title) = self.agent_display(&a.agent);
                format!("{} ({}):", display_name, title)
            })
            .collect();
        let max_label_len = labels.iter().map(|l| l.chars().count()).max().unwrap_or(0);

        writeln!(out, "{}", sep).ok();
        writeln!(
            out,
            "{}",
            self.format_line("        MAGI SYSTEM -- VERDICT")
        )
        .ok();
        writeln!(out, "{}", sep).ok();

        // Step 2: render each agent line with aligned label
        for (agent, label) in agents.iter().zip(labels.iter()) {
            let pct = (agent.confidence * 100.0).round() as u32;
            let verdict_suffix = format!(" {} ({}%)", agent.verdict, pct);
            let content = format!("  {:<max_label_len$}{}", label, verdict_suffix);
            let fitted = fit_content(&content, self.banner_inner, &verdict_suffix);
            writeln!(out, "|{:<width$}|", fitted, width = self.banner_inner).ok();
        }

        // Step 3: consensus line (no suffix protection)
        writeln!(out, "{}", sep).ok();
        let consensus_content = format!("  CONSENSUS: {}", consensus.consensus);
        let fitted_consensus = fit_content(&consensus_content, self.banner_inner, "");
        writeln!(
            out,
            "|{:<width$}|",
            fitted_consensus,
            width = self.banner_inner
        )
        .ok();
        write!(out, "{}", sep).ok();

        out
    }

    /// Generates the pre-analysis initialization banner.
    ///
    /// Shows mode, model, and timeout in a fixed-width ASCII box.
    pub fn format_init_banner(&self, mode: &Mode, model: &str, timeout_secs: u64) -> String {
        let mut out = String::new();
        let sep = self.format_separator();

        writeln!(out, "{}", sep).ok();
        writeln!(
            out,
            "{}",
            self.format_line("        MAGI SYSTEM -- INITIALIZING")
        )
        .ok();
        writeln!(out, "{}", sep).ok();
        writeln!(
            out,
            "{}",
            self.format_line(&format!("  Mode:     {}", mode))
        )
        .ok();
        writeln!(
            out,
            "{}",
            self.format_line(&format!("  Model:    {}", model))
        )
        .ok();
        writeln!(
            out,
            "{}",
            self.format_line(&format!("  Timeout:  {}s", timeout_secs))
        )
        .ok();
        write!(out, "{}", sep).ok();

        out
    }

    /// Generates the full markdown report (banner + all sections).
    ///
    /// Concatenates sections in order: banner, key findings, dissenting opinion,
    /// conditions for approval, recommended actions.
    /// Optional sections are omitted entirely when their data is absent.
    pub fn format_report(&self, agents: &[AgentOutput], consensus: &ConsensusResult) -> String {
        // Delegates to the rotation-aware assembler with NO rotation data, so the
        // output is byte-identical to the pre-MS2 format (empty rotations →
        // `## Model Rotations` omitted; `estimated == false` → no note).
        self.format_report_with_rotations(agents, consensus, &BTreeMap::new(), false)
    }

    /// **MS2** — Like [`format_report`](Self::format_report) but inserts the rotation
    /// telemetry sections after the banner: `## Model Rotations` (when any agent
    /// rotated) and the run-level "estimated" honesty note (when `estimated`). The
    /// ENTIRE report is assembled here in the formatter, so callers never couple to
    /// the banner/section layout (no external string-splicing). Additive:
    /// `format_report` is unchanged and delegates here with empty inputs.
    pub fn format_report_with_rotations(
        &self,
        agents: &[AgentOutput],
        consensus: &ConsensusResult,
        rotations: &BTreeMap<AgentName, AgentRotation>,
        estimated: bool,
    ) -> String {
        // Delegates with no extraction telemetry, so its own signature stays untouched.
        // MS2 established this shape when `format_report` began delegating here; adding a
        // parameter instead would be a second public break in a release whose only
        // intended one is the prompt guard.
        self.format_report_with_telemetry(agents, consensus, rotations, estimated, &BTreeMap::new())
    }

    /// **MS3** — like [`format_report_with_rotations`](Self::format_report_with_rotations)
    /// but also renders `## Extraction Failures`.
    ///
    /// Each telemetry section is empty when there is nothing to report, so a clean run
    /// still produces byte-identical output to a report from before either feature
    /// existed. The whole report is assembled here, so callers never couple to the
    /// section layout by splicing strings themselves.
    pub fn format_report_with_telemetry(
        &self,
        agents: &[AgentOutput],
        consensus: &ConsensusResult,
        rotations: &BTreeMap<AgentName, AgentRotation>,
        estimated: bool,
        extraction_failures: &BTreeMap<AgentName, Vec<ExtractionFailure>>,
    ) -> String {
        let mut out = String::new();

        // 1. Banner
        out.push_str(&self.format_banner(agents, consensus));
        out.push('\n');

        // 1b. MS2 rotation sections (after the banner, before Key Findings). Each is
        //     empty when there was no rotation / no estimated model, preserving the
        //     byte-identical 2.0.x output on a plain run.
        out.push_str(&self.format_model_rotations(rotations));
        out.push_str(&self.format_estimated_note(estimated));
        out.push_str(&self.format_extraction_failures(extraction_failures));

        // 2. Key Findings (optional)
        if !consensus.findings.is_empty() {
            out.push_str(&self.format_findings(&consensus.findings));
        }

        // 3. Dissenting Opinion (optional)
        if !consensus.dissent.is_empty() {
            out.push_str(&self.format_dissent(&consensus.dissent));
        }

        // 4. Conditions for Approval (optional)
        if !consensus.conditions.is_empty() {
            out.push_str(&self.format_conditions(&consensus.conditions));
        }

        // 5. Recommended Actions
        out.push_str(&self.format_recommendations(&consensus.recommendations));

        out
    }

    /// Formats the `## Extraction Failures` section, or returns an empty string when every
    /// dispatched agent's `Vec` is empty (every seat's verdict was extracted cleanly).
    ///
    /// Renders one line per rejected attempt, in `failures`' `BTreeMap` order (i.e.
    /// [`AgentName`] order) and then `Vec` order within an agent — that order is
    /// chronological (first attempt, corrective retry, then any post-rotation attempts) and
    /// must be preserved, since it is what lets a reader see *when* a model stopped
    /// adhering, not merely that it did:
    ///
    /// ```text
    /// ## Extraction Failures
    ///
    /// ✗ Caspar: deepseek (attempt 1) — missing markers
    /// ✗ Caspar: deepseek (attempt 2) — unterminated block
    /// ✗ Caspar: glm (attempt 1) — schema mismatch
    /// ```
    ///
    /// Before this section existed, an agent whose corrective retry recovered showed up in
    /// `retried_agents` with the cause invisible — a reader knew a seat had struggled, not
    /// why or with which model. Attributing by `model` rather than merely by seat is the
    /// point: with per-agent rotation a single seat can burn through several models, and the
    /// question that decides whether a model stays in the pool is *"which MODEL fails to
    /// adhere?"*, not *"which seat suffered?"*.
    ///
    /// Returning an empty string on a clean run keeps the output byte-identical to a
    /// pre-sentinel report, mirroring
    /// [`format_model_rotations`](Self::format_model_rotations)'s rule. Note the map is
    /// ALWAYS populated with one entry per agent, so `failures.is_empty()` would be the
    /// wrong test — an empty `Vec` is a present, positive certificate.
    pub fn format_extraction_failures(
        &self,
        failures: &BTreeMap<AgentName, Vec<ExtractionFailure>>,
    ) -> String {
        let has_failures = failures.values().any(|v| !v.is_empty());
        if !has_failures {
            return String::new();
        }

        let mut out = String::new();
        writeln!(out, "## Extraction Failures\n").ok();
        for (agent, agent_failures) in failures {
            for failure in agent_failures {
                writeln!(
                    out,
                    "\u{2717} {}: {} (attempt {}) \u{2014} {}",
                    self.agent_display(agent).0,
                    failure.model,
                    failure.attempt,
                    cause_label(failure.cause)
                )
                .ok();
            }
        }
        writeln!(out).ok();
        out
    }

    /// Formats the `## Model Rotations` section, or returns an empty string
    /// when no agent rotated (every `AgentRotation.chain` is empty).
    ///
    /// Renders one line per completed rotation hop, in `agents`' `BTreeMap`
    /// order (i.e. [`AgentName`] order) and then chain order within an agent:
    ///
    /// ```text
    /// ## Model Rotations
    ///
    /// ⟲ Caspar rotated: deepseek → glm (transport)
    /// ```
    ///
    /// Returning an empty string when nothing rotated keeps `format_report`'s
    /// output byte-identical to pre-rotation (2.0.x) reports for the common
    /// case where no fallback was ever needed.
    pub fn format_model_rotations(&self, rotations: &BTreeMap<AgentName, AgentRotation>) -> String {
        let has_rotation = rotations.values().any(|rot| !rot.chain.is_empty());
        if !has_rotation {
            return String::new();
        }

        let mut out = String::new();
        writeln!(out, "## Model Rotations\n").ok();
        for (agent, rot) in rotations {
            for event in &rot.chain {
                writeln!(
                    out,
                    "\u{27F2} {} rotated: {} \u{2192} {} ({})",
                    self.agent_display(agent).0,
                    event.from(),
                    event.to(),
                    event.kind()
                )
                .ok();
            }
        }
        writeln!(out).ok();
        out
    }

    /// Run-level honesty disclosure (R19): when any surviving mage ran on an
    /// ESTIMATED (unmeasured) context window, returns a one-line note; otherwise
    /// `""`. The word "estimated" is the auditable signal.
    pub fn format_estimated_note(&self, estimated: bool) -> String {
        if estimated {
            "> Note: at least one surviving model ran on an estimated (unmeasured) \
             context window.\n\n"
                .to_string()
        } else {
            String::new()
        }
    }

    /// Generates the separator line: `+` + `=` * inner + `+`.
    fn format_separator(&self) -> String {
        format!("+{}+", "=".repeat(self.banner_inner))
    }

    /// Generates a content line: `|` + content padded to inner width + `|`.
    ///
    /// Content is left-aligned. If content exceeds inner width, it is truncated.
    fn format_line(&self, content: &str) -> String {
        if content.len() > self.banner_inner {
            let boundary = content.floor_char_boundary(self.banner_inner);
            format!(
                "|{:<width$}|",
                &content[..boundary],
                width = self.banner_inner
            )
        } else {
            format!("|{:<width$}|", content, width = self.banner_inner)
        }
    }

    /// Returns `(display_name, title)` for the given agent.
    ///
    /// Looks up in `config.agent_titles` first, falls back to
    /// `AgentName::display_name()` and `AgentName::title()`.
    fn agent_display(&self, name: &AgentName) -> (&str, &str) {
        if let Some((display_name, title)) = self.config.agent_titles.get(name) {
            (display_name.as_str(), title.as_str())
        } else {
            (name.display_name(), name.title())
        }
    }

    /// Formats the key findings section.
    ///
    /// Renders one line per finding in the Python MAGI reference layout:
    /// ```text
    /// {marker:<5} {severity_label:<14} {title} _(from {sources})_
    /// ```
    /// The `detail` field is intentionally excluded from the markdown output.
    /// It remains accessible via the `ConsensusResult::findings[].detail` field
    /// in the JSON-serialized `MagiReport`.
    fn format_findings(&self, findings: &[DedupFinding]) -> String {
        let mut out = String::new();
        writeln!(out, "\n## Key Findings\n").ok();
        for finding in findings {
            let sources = finding
                .sources
                .iter()
                .map(|s| self.agent_display(s).0)
                .collect::<Vec<_>>()
                .join(", ");
            let severity_label = format!("**[{}]**", finding.severity);
            writeln!(
                out,
                "{:<marker_w$} {:<sev_w$} {} _(from {})_",
                finding.severity.icon(),
                severity_label,
                finding.title,
                sources,
                marker_w = FINDING_MARKER_WIDTH,
                sev_w = FINDING_SEVERITY_WIDTH,
            )
            .ok();
        }
        writeln!(out).ok();
        out
    }

    /// Formats the dissenting opinion section.
    ///
    /// Emits one line per dissenter: `**Name (Title)**: <summary>`.
    /// The `reasoning` field is intentionally excluded — it is preserved
    /// in the JSON output (`Dissent` struct) but not rendered in the report.
    fn format_dissent(&self, dissent: &[Dissent]) -> String {
        let mut out = String::new();
        writeln!(out, "\n## Dissenting Opinion\n").ok();
        for d in dissent {
            let (name, title) = self.agent_display(&d.agent);
            writeln!(out, "**{} ({})**: {}", name, title, d.summary).ok();
        }
        writeln!(out).ok();
        out
    }

    /// Formats the conditions for approval section.
    fn format_conditions(&self, conditions: &[Condition]) -> String {
        let mut out = String::new();
        writeln!(out, "\n## Conditions for Approval\n").ok();
        for c in conditions {
            let (display_name, _) = self.agent_display(&c.agent);
            writeln!(out, "- **{}**: {}", display_name, c.condition).ok();
        }
        writeln!(out).ok();
        out
    }

    /// Formats the recommended actions section.
    fn format_recommendations(&self, recommendations: &BTreeMap<AgentName, String>) -> String {
        let mut out = String::new();
        writeln!(out, "\n## Recommended Actions\n").ok();
        for (name, rec) in recommendations {
            let (display_name, title) = self.agent_display(name);
            writeln!(out, "- **{}** ({}): {}", display_name, title, rec).ok();
        }
        out
    }
}

impl Default for ReportFormatter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consensus::*;
    use crate::rotation::{Lineage, RotationEvent, RotationKind};
    use crate::schema::*;

    /// Helper: build a minimal AgentOutput for testing.
    fn make_agent(
        name: AgentName,
        verdict: Verdict,
        confidence: f64,
        summary: &str,
        reasoning: &str,
        recommendation: &str,
    ) -> AgentOutput {
        AgentOutput {
            agent: name,
            verdict,
            confidence,
            summary: summary.to_string(),
            reasoning: reasoning.to_string(),
            findings: vec![],
            recommendation: recommendation.to_string(),
        }
    }

    #[test]
    fn test_no_rotation_report_section_is_empty() {
        // S16
        let f = ReportFormatter::new();
        let mut rots = std::collections::BTreeMap::new();
        rots.insert(
            AgentName::Caspar,
            AgentRotation {
                model_configured: "deepseek".into(),
                model_used: "deepseek".into(),
                chain: vec![],
                ran_unmeasured: false,
            },
        );
        let out = f.format_model_rotations(&rots);
        assert!(out.is_empty());
        assert!(!out.contains("## Model Rotations"));
    }

    #[test]
    fn test_single_hop_rotation_line_format() {
        // S15
        let f = ReportFormatter::new();
        let mut rots = std::collections::BTreeMap::new();
        rots.insert(
            AgentName::Caspar,
            AgentRotation {
                model_configured: "deepseek".into(),
                model_used: "glm".into(),
                chain: vec![RotationEvent::new(
                    Lineage::new("deepseek"),
                    Lineage::new("glm"),
                    "glm".into(),
                    RotationKind::Transport,
                    String::new(),
                )],
                ran_unmeasured: false,
            },
        );
        let out = f.format_model_rotations(&rots);
        assert!(out.contains("## Model Rotations"));
        assert!(
            out.contains("\u{27F2} Caspar rotated: deepseek \u{2192} glm (transport)"),
            "got: {out}"
        );
    }

    #[test]
    fn test_multi_hop_two_lines_in_order() {
        // S17
        let f = ReportFormatter::new();
        let mut rots = std::collections::BTreeMap::new();
        rots.insert(
            AgentName::Caspar,
            AgentRotation {
                model_configured: "deepseek".into(),
                model_used: "nemotron".into(),
                chain: vec![
                    RotationEvent::new(
                        Lineage::new("deepseek"),
                        Lineage::new("gpt-oss"),
                        "gpt-oss".into(),
                        RotationKind::Schema,
                        String::new(),
                    ),
                    RotationEvent::new(
                        Lineage::new("gpt-oss"),
                        Lineage::new("nemotron"),
                        "nemotron".into(),
                        RotationKind::Transport,
                        String::new(),
                    ),
                ],
                ran_unmeasured: false,
            },
        );
        let out = f.format_model_rotations(&rots);
        let idx1 = out.find("deepseek \u{2192} gpt-oss").expect("first hop");
        let idx2 = out.find("gpt-oss \u{2192} nemotron").expect("second hop");
        assert!(idx1 < idx2, "hops must be in chain order");
    }

    /// Helper: build a minimal ConsensusResult for testing.
    fn make_consensus(
        label: &str,
        verdict: Verdict,
        confidence: f64,
        score: f64,
        agents: &[&AgentOutput],
    ) -> ConsensusResult {
        let mut votes = BTreeMap::new();
        let mut recommendations = BTreeMap::new();
        for a in agents {
            votes.insert(a.agent, a.verdict);
            recommendations.insert(a.agent, a.recommendation.clone());
        }

        let majority_summary = agents
            .iter()
            .filter(|a| a.effective_verdict() == verdict.effective())
            .map(|a| format!("{}: {}", a.agent.display_name(), a.summary))
            .collect::<Vec<_>>()
            .join(" | ");

        ConsensusResult {
            consensus: label.to_string(),
            consensus_verdict: verdict,
            confidence,
            score,
            agent_count: agents.len(),
            votes,
            majority_summary,
            dissent: vec![],
            findings: vec![],
            conditions: vec![],
            recommendations,
        }
    }

    // -- BDD Scenario 15: banner width --

    /// All banner lines are exactly 52 characters wide.
    #[test]
    fn test_banner_lines_are_exactly_52_chars_wide() {
        let m = make_agent(
            AgentName::Melchior,
            Verdict::Approve,
            0.9,
            "Good",
            "R",
            "Rec",
        );
        let b = make_agent(
            AgentName::Balthasar,
            Verdict::Conditional,
            0.85,
            "Ok",
            "R",
            "Rec",
        );
        let c = make_agent(AgentName::Caspar, Verdict::Reject, 0.78, "Bad", "R", "Rec");
        let agents = vec![m.clone(), b.clone(), c.clone()];
        let consensus = make_consensus(
            "GO WITH CAVEATS",
            Verdict::Approve,
            0.85,
            0.33,
            &[&m, &b, &c],
        );

        let formatter = ReportFormatter::new();
        let banner = formatter.format_banner(&agents, &consensus);

        for line in banner.lines() {
            if !line.is_empty() {
                assert_eq!(line.len(), 52, "Line is not 52 chars: '{}'", line);
            }
        }
    }

    /// Banner with long consensus label still fits 52 chars.
    #[test]
    fn test_banner_with_long_content_fits_52_chars() {
        let m = make_agent(AgentName::Melchior, Verdict::Approve, 0.9, "S", "R", "Rec");
        let b = make_agent(
            AgentName::Balthasar,
            Verdict::Approve,
            0.85,
            "S",
            "R",
            "Rec",
        );
        let c = make_agent(AgentName::Caspar, Verdict::Approve, 0.95, "S", "R", "Rec");
        let agents = vec![m.clone(), b.clone(), c.clone()];
        let consensus = make_consensus("STRONG GO", Verdict::Approve, 0.9, 1.0, &[&m, &b, &c]);

        let formatter = ReportFormatter::new();
        let banner = formatter.format_banner(&agents, &consensus);

        for line in banner.lines() {
            if !line.is_empty() {
                assert_eq!(line.len(), 52, "Line is not 52 chars: '{}'", line);
            }
        }
    }

    // -- BDD Scenario 16: report sections --

    /// Report with mixed consensus contains 4 markdown headers (no Consensus Summary).
    #[test]
    fn test_report_with_mixed_consensus_contains_all_headers() {
        let m = make_agent(
            AgentName::Melchior,
            Verdict::Approve,
            0.9,
            "Good code",
            "Solid",
            "Merge",
        );
        let b = make_agent(
            AgentName::Balthasar,
            Verdict::Conditional,
            0.85,
            "Needs work",
            "Issues",
            "Fix first",
        );
        let c = make_agent(
            AgentName::Caspar,
            Verdict::Reject,
            0.78,
            "Problems",
            "Risky",
            "Reject",
        );
        let agents = vec![m.clone(), b.clone(), c.clone()];

        let mut consensus = make_consensus(
            "GO WITH CAVEATS",
            Verdict::Approve,
            0.85,
            0.33,
            &[&m, &b, &c],
        );
        consensus.dissent = vec![Dissent {
            agent: AgentName::Caspar,
            summary: "Problems found".to_string(),
            reasoning: "Risk is too high".to_string(),
        }];
        consensus.conditions = vec![Condition {
            agent: AgentName::Balthasar,
            condition: "Fix first".to_string(),
        }];
        consensus.findings = vec![DedupFinding {
            severity: Severity::Warning,
            title: "Test finding".to_string(),
            detail: "Detail here".to_string(),
            sources: vec![AgentName::Melchior, AgentName::Caspar],
            file: None,
            line: None,
            category: Category::Other,
            id: None,
        }];

        let formatter = ReportFormatter::new();
        let report = formatter.format_report(&agents, &consensus);

        assert!(
            !report.contains("## Consensus Summary"),
            "Consensus Summary must not appear"
        );
        assert!(report.contains("## Key Findings"), "Missing Key Findings");
        assert!(
            report.contains("## Dissenting Opinion"),
            "Missing Dissenting Opinion"
        );
        assert!(
            report.contains("## Conditions for Approval"),
            "Missing Conditions"
        );
        assert!(
            report.contains("## Recommended Actions"),
            "Missing Recommended Actions"
        );
    }

    /// Report does not contain the "## Consensus Summary" heading.
    #[test]
    fn test_report_does_not_contain_consensus_summary_heading() {
        let m = make_agent(
            AgentName::Melchior,
            Verdict::Approve,
            0.9,
            "Good",
            "R",
            "Merge",
        );
        let b = make_agent(
            AgentName::Balthasar,
            Verdict::Approve,
            0.85,
            "Good",
            "R",
            "Merge",
        );
        let c = make_agent(
            AgentName::Caspar,
            Verdict::Approve,
            0.95,
            "Good",
            "R",
            "Merge",
        );
        let agents = vec![m.clone(), b.clone(), c.clone()];
        let consensus = make_consensus("STRONG GO", Verdict::Approve, 0.9, 1.0, &[&m, &b, &c]);

        let formatter = ReportFormatter::new();
        let report = formatter.format_report(&agents, &consensus);

        assert!(!report.contains("## Consensus Summary"));
    }

    /// Report section order: banner ("+====") before any optional sections,
    /// with no "## Consensus Summary" between them.
    #[test]
    fn test_report_section_order_banner_then_findings_or_dissent_or_conditions_or_actions() {
        let m = make_agent(
            AgentName::Melchior,
            Verdict::Approve,
            0.9,
            "Good code",
            "Solid",
            "Merge",
        );
        let b = make_agent(
            AgentName::Balthasar,
            Verdict::Conditional,
            0.85,
            "Needs work",
            "Issues",
            "Fix first",
        );
        let c = make_agent(
            AgentName::Caspar,
            Verdict::Reject,
            0.78,
            "Problems",
            "Risky",
            "Reject",
        );
        let agents = vec![m.clone(), b.clone(), c.clone()];

        let mut consensus = make_consensus(
            "GO WITH CAVEATS",
            Verdict::Approve,
            0.85,
            0.33,
            &[&m, &b, &c],
        );
        consensus.findings = vec![DedupFinding {
            severity: Severity::Warning,
            title: "Test finding".to_string(),
            detail: "Detail here".to_string(),
            sources: vec![AgentName::Melchior],
            file: None,
            line: None,
            category: Category::Other,
            id: None,
        }];
        consensus.dissent = vec![Dissent {
            agent: AgentName::Caspar,
            summary: "Problems found".to_string(),
            reasoning: "Risk is too high".to_string(),
        }];
        consensus.conditions = vec![Condition {
            agent: AgentName::Balthasar,
            condition: "Fix first".to_string(),
        }];

        let formatter = ReportFormatter::new();
        let report = formatter.format_report(&agents, &consensus);

        // No Consensus Summary anywhere
        assert!(!report.contains("## Consensus Summary"));

        // Banner border must appear before all section headings
        let banner_pos = report.find("+====").expect("banner border not found");
        let actions_pos = report
            .find("## Recommended Actions")
            .expect("Recommended Actions not found");
        let findings_pos = report
            .find("## Key Findings")
            .expect("Key Findings not found");
        let dissent_pos = report
            .find("## Dissenting Opinion")
            .expect("Dissenting Opinion not found");
        let conditions_pos = report
            .find("## Conditions for Approval")
            .expect("Conditions not found");

        assert!(
            banner_pos < findings_pos,
            "banner must come before Key Findings"
        );
        assert!(
            banner_pos < dissent_pos,
            "banner must come before Dissenting Opinion"
        );
        assert!(
            banner_pos < conditions_pos,
            "banner must come before Conditions"
        );
        assert!(
            banner_pos < actions_pos,
            "banner must come before Recommended Actions"
        );

        // Section order: findings < dissent < conditions < actions
        assert!(
            findings_pos < dissent_pos,
            "Key Findings must come before Dissenting Opinion"
        );
        assert!(
            dissent_pos < conditions_pos,
            "Dissenting Opinion must come before Conditions"
        );
        assert!(
            conditions_pos < actions_pos,
            "Conditions must come before Recommended Actions"
        );
    }

    /// Report without dissent omits "## Dissenting Opinion".
    #[test]
    fn test_report_without_dissent_omits_dissent_section() {
        let m = make_agent(
            AgentName::Melchior,
            Verdict::Approve,
            0.9,
            "Good",
            "R",
            "Merge",
        );
        let b = make_agent(
            AgentName::Balthasar,
            Verdict::Approve,
            0.85,
            "Good",
            "R",
            "Merge",
        );
        let c = make_agent(
            AgentName::Caspar,
            Verdict::Approve,
            0.95,
            "Good",
            "R",
            "Merge",
        );
        let agents = vec![m.clone(), b.clone(), c.clone()];
        let consensus = make_consensus("STRONG GO", Verdict::Approve, 0.9, 1.0, &[&m, &b, &c]);

        let formatter = ReportFormatter::new();
        let report = formatter.format_report(&agents, &consensus);

        assert!(!report.contains("## Dissenting Opinion"));
    }

    /// Report without conditions omits "## Conditions for Approval".
    #[test]
    fn test_report_without_conditions_omits_conditions_section() {
        let m = make_agent(
            AgentName::Melchior,
            Verdict::Approve,
            0.9,
            "Good",
            "R",
            "Merge",
        );
        let b = make_agent(
            AgentName::Balthasar,
            Verdict::Approve,
            0.85,
            "Good",
            "R",
            "Merge",
        );
        let c = make_agent(
            AgentName::Caspar,
            Verdict::Approve,
            0.95,
            "Good",
            "R",
            "Merge",
        );
        let agents = vec![m.clone(), b.clone(), c.clone()];
        let consensus = make_consensus("STRONG GO", Verdict::Approve, 0.9, 1.0, &[&m, &b, &c]);

        let formatter = ReportFormatter::new();
        let report = formatter.format_report(&agents, &consensus);

        assert!(!report.contains("## Conditions for Approval"));
    }

    /// Report without findings omits "## Key Findings".
    #[test]
    fn test_report_without_findings_omits_findings_section() {
        let m = make_agent(
            AgentName::Melchior,
            Verdict::Approve,
            0.9,
            "Good",
            "R",
            "Merge",
        );
        let b = make_agent(
            AgentName::Balthasar,
            Verdict::Approve,
            0.85,
            "Good",
            "R",
            "Merge",
        );
        let c = make_agent(
            AgentName::Caspar,
            Verdict::Approve,
            0.95,
            "Good",
            "R",
            "Merge",
        );
        let agents = vec![m.clone(), b.clone(), c.clone()];
        let consensus = make_consensus("STRONG GO", Verdict::Approve, 0.9, 1.0, &[&m, &b, &c]);

        let formatter = ReportFormatter::new();
        let report = formatter.format_report(&agents, &consensus);

        assert!(!report.contains("## Key Findings"));
    }

    // -- Banner formatting --

    /// format_banner generates correct ASCII art structure.
    #[test]
    fn test_format_banner_has_correct_structure() {
        let m = make_agent(AgentName::Melchior, Verdict::Approve, 0.9, "S", "R", "Rec");
        let b = make_agent(AgentName::Balthasar, Verdict::Reject, 0.7, "S", "R", "Rec");
        let c = make_agent(AgentName::Caspar, Verdict::Reject, 0.8, "S", "R", "Rec");
        let agents = vec![m.clone(), b.clone(), c.clone()];
        let consensus = make_consensus("HOLD (2-1)", Verdict::Reject, 0.7, -0.33, &[&m, &b, &c]);

        let formatter = ReportFormatter::new();
        let banner = formatter.format_banner(&agents, &consensus);

        assert!(banner.contains("MAGI SYSTEM -- VERDICT"));
        assert!(banner.contains("Melchior (Scientist)"));
        assert!(banner.contains("APPROVE"));
        assert!(banner.contains("CONSENSUS:"));
        assert!(banner.contains("HOLD (2-1)"));
    }

    /// format_init_banner shows mode, model, timeout.
    #[test]
    fn test_format_init_banner_shows_mode_model_timeout() {
        let formatter = ReportFormatter::new();
        let banner = formatter.format_init_banner(&Mode::CodeReview, "claude-sonnet", 300);

        assert!(banner.contains("code-review"), "Missing mode");
        assert!(banner.contains("claude-sonnet"), "Missing model");
        assert!(banner.contains("300"), "Missing timeout");

        for line in banner.lines() {
            if !line.is_empty() {
                assert_eq!(line.len(), 52, "Init banner line not 52 chars: '{}'", line);
            }
        }
    }

    /// Separator line is "+" + "=" * 50 + "+".
    #[test]
    fn test_separator_format() {
        let formatter = ReportFormatter::new();
        let banner = formatter.format_init_banner(&Mode::Analysis, "test", 60);
        let sep = format!("+{}+", "=".repeat(50));

        assert!(banner.contains(&sep), "Missing separator line");
        assert_eq!(sep.len(), 52);
    }

    /// Agent line shows "Name (Title):" aligned to max label width, then " VERDICT (NN%)".
    ///
    /// With default config:
    /// - "Melchior (Scientist):"   = 21 chars → padded to 23 → 2 trailing spaces before verdict
    /// - "Balthasar (Pragmatist):" = 23 chars → max, no padding
    /// - "Caspar (Critic):"        = 17 chars → padded to 23 → 6 trailing spaces before verdict
    ///
    /// Content = "  {label:<23}{verdict_suffix}" where verdict_suffix starts with one space:
    /// - Melchior: "  Melchior (Scientist):   APPROVE (90%)"  (2+21+2 padding+1 from suffix)
    /// - Caspar:   "  Caspar (Critic):         APPROVE (78%)" (2+17+6 padding+1 from suffix)
    #[test]
    fn test_agent_line_format() {
        let m = make_agent(AgentName::Melchior, Verdict::Approve, 0.9, "S", "R", "Rec");
        let b = make_agent(
            AgentName::Balthasar,
            Verdict::Approve,
            0.85,
            "S",
            "R",
            "Rec",
        );
        let c = make_agent(AgentName::Caspar, Verdict::Approve, 0.78, "S", "R", "Rec");
        let agents = vec![m.clone(), b.clone(), c.clone()];
        let consensus = make_consensus("STRONG GO", Verdict::Approve, 0.9, 1.0, &[&m, &b, &c]);

        let formatter = ReportFormatter::new();
        let banner = formatter.format_banner(&agents, &consensus);

        // "Melchior (Scientist):" = 21 chars, padded to 23 (max): 2 extra spaces.
        // verdict_suffix starts with 1 space → total 3 spaces between colon and APPROVE.
        assert!(banner.contains("Melchior (Scientist):   APPROVE (90%)"));
        // "Caspar (Critic):" = 17 chars, padded to 23: 6 extra spaces.
        // verdict_suffix starts with 1 space → total 7 spaces between colon and APPROVE.
        assert!(banner.contains("Caspar (Critic):        APPROVE (78%)"));
    }

    // -- Report content sections --

    /// Findings section shows icon + severity + title + sources (detail is excluded from markdown).
    #[test]
    fn test_findings_section_format() {
        let m = make_agent(
            AgentName::Melchior,
            Verdict::Approve,
            0.9,
            "Good",
            "R",
            "Merge",
        );
        let agents = vec![m.clone()];
        let mut consensus = make_consensus("GO (1-0)", Verdict::Approve, 0.9, 1.0, &[&m]);
        consensus.findings = vec![DedupFinding {
            severity: Severity::Critical,
            title: "SQL injection risk".to_string(),
            detail: "User input not sanitized".to_string(),
            sources: vec![AgentName::Melchior, AgentName::Caspar],
            file: None,
            line: None,
            category: Category::Other,
            id: None,
        }];

        let formatter = ReportFormatter::new();
        let report = formatter.format_report(&agents, &consensus);

        assert!(report.contains("[!!!]"), "Missing critical icon");
        assert!(report.contains("[CRITICAL]"), "Missing severity label");
        assert!(report.contains("SQL injection risk"), "Missing title");
        assert!(report.contains("Melchior"), "Missing source agent");
        assert!(report.contains("Caspar"), "Missing source agent");
        // detail is preserved in JSON but not rendered in markdown
        assert!(
            !report.contains("User input not sanitized"),
            "Detail must not appear in markdown report"
        );
    }

    /// Dissent section shows agent name and summary; reasoning is not rendered.
    #[test]
    fn test_dissent_section_format() {
        let m = make_agent(
            AgentName::Melchior,
            Verdict::Approve,
            0.9,
            "Good",
            "R",
            "Merge",
        );
        let c = make_agent(
            AgentName::Caspar,
            Verdict::Reject,
            0.8,
            "Bad",
            "Too risky",
            "Reject",
        );
        let agents = vec![m.clone(), c.clone()];
        let mut consensus = make_consensus("GO (1-1)", Verdict::Approve, 0.8, 0.0, &[&m, &c]);
        consensus.dissent = vec![Dissent {
            agent: AgentName::Caspar,
            summary: "Too many issues".to_string(),
            reasoning: "The code has critical flaws".to_string(),
        }];

        let formatter = ReportFormatter::new();
        let report = formatter.format_report(&agents, &consensus);

        assert!(report.contains("Caspar"), "Missing dissenting agent name");
        assert!(report.contains("Critic"), "Missing dissenting agent title");
        assert!(
            report.contains("Too many issues"),
            "Missing dissent summary"
        );
        // Reasoning is preserved in the Dissent struct (JSON) but not rendered in the report.
        assert!(
            !report.contains("The code has critical flaws"),
            "Dissent reasoning must not appear in the rendered report"
        );
    }

    /// Conditions section shows bulleted list with agent names.
    #[test]
    fn test_conditions_section_format() {
        let m = make_agent(
            AgentName::Melchior,
            Verdict::Approve,
            0.9,
            "Good",
            "R",
            "Merge",
        );
        let b = make_agent(
            AgentName::Balthasar,
            Verdict::Conditional,
            0.85,
            "Ok",
            "R",
            "Fix tests",
        );
        let agents = vec![m.clone(), b.clone()];
        let mut consensus =
            make_consensus("GO WITH CAVEATS", Verdict::Approve, 0.85, 0.75, &[&m, &b]);
        consensus.conditions = vec![Condition {
            agent: AgentName::Balthasar,
            condition: "Fix tests first".to_string(),
        }];

        let formatter = ReportFormatter::new();
        let report = formatter.format_report(&agents, &consensus);

        assert!(
            report.contains("- **Balthasar**:"),
            "Missing bullet with agent name"
        );
        assert!(report.contains("Fix tests first"), "Missing condition text");
    }

    /// Recommendations section shows per-agent recommendations.
    #[test]
    fn test_recommendations_section_format() {
        let m = make_agent(
            AgentName::Melchior,
            Verdict::Approve,
            0.9,
            "Good",
            "R",
            "Merge immediately",
        );
        let b = make_agent(
            AgentName::Balthasar,
            Verdict::Approve,
            0.85,
            "Good",
            "R",
            "Ship it",
        );
        let agents = vec![m.clone(), b.clone()];
        let consensus = make_consensus("GO (2-0)", Verdict::Approve, 0.9, 1.0, &[&m, &b]);

        let formatter = ReportFormatter::new();
        let report = formatter.format_report(&agents, &consensus);

        assert!(
            report.contains("Merge immediately"),
            "Missing Melchior recommendation"
        );
        assert!(
            report.contains("Ship it"),
            "Missing Balthasar recommendation"
        );
    }

    /// Agent display falls back to AgentName methods when not in config.
    #[test]
    fn test_agent_display_fallback_to_agent_name_methods() {
        let config = ReportConfig {
            banner_width: 52,
            agent_titles: BTreeMap::new(),
        };
        let formatter = ReportFormatter::with_config(config).unwrap();

        let m = make_agent(AgentName::Melchior, Verdict::Approve, 0.9, "S", "R", "Rec");
        let agents = vec![m.clone()];
        let consensus = make_consensus("GO (1-0)", Verdict::Approve, 0.9, 1.0, &[&m]);

        let banner = formatter.format_banner(&agents, &consensus);
        assert!(
            banner.contains("Melchior"),
            "Should use AgentName::display_name()"
        );
    }

    // -- MagiReport tests --

    /// MagiReport serializes to JSON.
    #[test]
    fn test_magi_report_serializes_to_json() {
        let m = make_agent(
            AgentName::Melchior,
            Verdict::Approve,
            0.9,
            "Good",
            "R",
            "Merge",
        );
        let agents = vec![m.clone()];
        let consensus = make_consensus("GO (1-0)", Verdict::Approve, 0.9, 1.0, &[&m]);

        let report = MagiReport {
            agents,
            consensus,
            banner: "banner".to_string(),
            report: "report".to_string(),
            degraded: false,
            failed_agents: BTreeMap::new(),
            retried_agents: BTreeSet::new(),
            rotations: BTreeMap::new(),
            extraction_failures: BTreeMap::new(),
        };

        let json = serde_json::to_string(&report).expect("serialize");
        assert!(json.contains("\"consensus\""));
        assert!(json.contains("\"agents\""));
        assert!(json.contains("\"degraded\""));
    }

    /// degraded=false when all 3 agents succeed.
    #[test]
    fn test_magi_report_not_degraded_with_three_agents() {
        let m = make_agent(AgentName::Melchior, Verdict::Approve, 0.9, "S", "R", "Rec");
        let b = make_agent(
            AgentName::Balthasar,
            Verdict::Approve,
            0.85,
            "S",
            "R",
            "Rec",
        );
        let c = make_agent(AgentName::Caspar, Verdict::Approve, 0.95, "S", "R", "Rec");
        let agents = vec![m.clone(), b.clone(), c.clone()];
        let consensus = make_consensus("STRONG GO", Verdict::Approve, 0.9, 1.0, &[&m, &b, &c]);

        let report = MagiReport {
            agents,
            consensus,
            banner: String::new(),
            report: String::new(),
            degraded: false,
            failed_agents: BTreeMap::new(),
            retried_agents: BTreeSet::new(),
            rotations: BTreeMap::new(),
            extraction_failures: BTreeMap::new(),
        };

        assert!(!report.degraded);
        assert!(report.failed_agents.is_empty());
    }

    // -- T04: retried_agents tests --

    /// Default-constructed MagiReport has empty retried_agents BTreeSet.
    #[test]
    fn test_magi_report_retried_agents_default_empty() {
        let m = make_agent(AgentName::Melchior, Verdict::Approve, 0.9, "S", "R", "Rec");
        let agents = vec![m.clone()];
        let consensus = make_consensus("GO (1-0)", Verdict::Approve, 0.9, 1.0, &[&m]);
        let report = MagiReport {
            agents,
            consensus,
            banner: String::new(),
            report: String::new(),
            degraded: false,
            failed_agents: BTreeMap::new(),
            retried_agents: BTreeSet::new(),
            rotations: BTreeMap::new(),
            extraction_failures: BTreeMap::new(),
        };
        assert!(report.retried_agents.is_empty());
    }

    /// Empty retried_agents is omitted from JSON serialization.
    #[test]
    fn test_magi_report_skip_serializing_empty_retried_agents() {
        let m = make_agent(AgentName::Melchior, Verdict::Approve, 0.9, "S", "R", "Rec");
        let agents = vec![m.clone()];
        let consensus = make_consensus("GO (1-0)", Verdict::Approve, 0.9, 1.0, &[&m]);
        let report = MagiReport {
            agents,
            consensus,
            banner: String::new(),
            report: String::new(),
            degraded: false,
            failed_agents: BTreeMap::new(),
            retried_agents: BTreeSet::new(),
            rotations: BTreeMap::new(),
            extraction_failures: BTreeMap::new(),
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            !json.contains("retried_agents"),
            "empty retried_agents must be omitted; got: {json}"
        );
    }

    /// Non-empty retried_agents serializes in alphabetical agent-name order.
    #[test]
    fn test_magi_report_serializes_non_empty_retried_agents_alphabetically() {
        let m = make_agent(AgentName::Melchior, Verdict::Approve, 0.9, "S", "R", "Rec");
        let agents = vec![m.clone()];
        let consensus = make_consensus("GO (1-0)", Verdict::Approve, 0.9, 1.0, &[&m]);
        let mut retried = BTreeSet::new();
        retried.insert(AgentName::Melchior);
        retried.insert(AgentName::Balthasar);
        retried.insert(AgentName::Caspar);
        let report = MagiReport {
            agents,
            consensus,
            banner: String::new(),
            report: String::new(),
            degraded: false,
            failed_agents: BTreeMap::new(),
            retried_agents: retried,
            rotations: BTreeMap::new(),
            extraction_failures: BTreeMap::new(),
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains(r#""retried_agents":["balthasar","caspar","melchior"]"#),
            "alphabetical order required; got: {json}"
        );
    }

    /// BDD-12: `retried_agents` MUST NOT appear in the markdown report
    /// rendered by `format_report` (Python parity: telemetry stays JSON-only).
    /// The current architecture makes this structurally true — `format_report`
    /// only receives `(&[AgentOutput], &ConsensusResult)` and has no
    /// visibility into the telemetry field. This test is a regression guard:
    /// if a future change adds a parameter or threads `retried_agents` into
    /// the formatter, the markdown rendering must continue to omit the
    /// field name and value strings.
    #[test]
    fn test_magi_report_retried_agents_not_rendered_in_markdown() {
        let m = make_agent(AgentName::Melchior, Verdict::Approve, 0.9, "S", "R", "Rec");
        let b = make_agent(
            AgentName::Balthasar,
            Verdict::Approve,
            0.85,
            "S",
            "R",
            "Rec",
        );
        let c = make_agent(AgentName::Caspar, Verdict::Approve, 0.95, "S", "R", "Rec");
        let agents = vec![m.clone(), b.clone(), c.clone()];
        let consensus = make_consensus("STRONG GO", Verdict::Approve, 0.9, 1.0, &[&m, &b, &c]);

        let mut retried = BTreeSet::new();
        retried.insert(AgentName::Melchior);
        retried.insert(AgentName::Caspar);

        let formatter = ReportFormatter::new();
        let markdown = formatter.format_report(&agents, &consensus);
        let banner = formatter.format_banner(&agents, &consensus);

        // Build a report so we can also inspect the `report.report` field
        // path (which is what consumers see in stdout/CLI output).
        let report = MagiReport {
            agents,
            consensus,
            banner: banner.clone(),
            report: markdown.clone(),
            degraded: false,
            failed_agents: BTreeMap::new(),
            retried_agents: retried,
            rotations: BTreeMap::new(),
            extraction_failures: BTreeMap::new(),
        };

        // The field name must NOT leak into the human-facing render. The
        // word "retried" anywhere in the markdown would be a regression.
        assert!(
            !report.report.to_lowercase().contains("retried"),
            "markdown report must NOT contain 'retried' — got:\n{}",
            report.report
        );
        assert!(
            !report.banner.to_lowercase().contains("retried"),
            "banner must NOT contain 'retried' — got:\n{}",
            report.banner
        );
        assert!(
            !markdown.to_lowercase().contains("retried"),
            "format_report output must NOT contain 'retried' — got:\n{markdown}"
        );
    }

    /// v0.3.1 JSON fixture (no retried_agents key) deserializes with the
    /// field defaulted to empty. Backward-compatibility contract.
    ///
    /// Fixture capture path: C — constructed from v0.4 with
    /// retried_agents=BTreeSet::new(), serialized form is byte-identical
    /// to what v0.3.1 produced for the same MagiReport shape (since
    /// skip_serializing_if omits the empty field). See MAGI R2 W2/W7/W10.
    #[test]
    fn test_magi_report_deserialize_v03_fixture_defaults_retried_agents_empty() {
        let json = include_str!("../tests/fixtures/magi_report_v0_3_1.json");
        let report: MagiReport = serde_json::from_str(json)
            .expect("v0.3.1 JSON must deserialize cleanly into v0.4 MagiReport");
        assert!(
            report.retried_agents.is_empty(),
            "absent retried_agents must default to empty"
        );
        // Sanity: other fields populate correctly.
        assert!(report.degraded);
        assert!(!report.failed_agents.is_empty());
        assert_eq!(report.agents.len(), 2);
    }

    /// degraded=true with failed_agents populated when agent fails.
    #[test]
    fn test_magi_report_degraded_with_failed_agents() {
        let m = make_agent(AgentName::Melchior, Verdict::Approve, 0.9, "S", "R", "Rec");
        let b = make_agent(
            AgentName::Balthasar,
            Verdict::Approve,
            0.85,
            "S",
            "R",
            "Rec",
        );
        let agents = vec![m.clone(), b.clone()];
        let consensus = make_consensus("GO (2-0)", Verdict::Approve, 0.9, 1.0, &[&m, &b]);

        let report = MagiReport {
            agents,
            consensus,
            banner: String::new(),
            report: String::new(),
            degraded: true,
            failed_agents: BTreeMap::from([(AgentName::Caspar, "timeout".to_string())]),
            retried_agents: BTreeSet::new(),
            rotations: BTreeMap::new(),
            extraction_failures: BTreeMap::new(),
        };

        assert!(report.degraded);
        assert_eq!(report.failed_agents.len(), 1);
        assert!(report.failed_agents.contains_key(&AgentName::Caspar));
    }

    // -- S07: Dissent rendered as single line (summary-only) --

    /// Two dissenters produce exactly two `**Name (Title)**:` header lines.
    #[test]
    fn test_dissent_shows_one_line_per_dissenter() {
        let formatter = ReportFormatter::new();
        let dissent = vec![
            Dissent {
                agent: AgentName::Caspar,
                summary: "Summary for Caspar".to_string(),
                reasoning: "Reasoning for Caspar that is long and detailed".to_string(),
            },
            Dissent {
                agent: AgentName::Balthasar,
                summary: "Summary for Balthasar".to_string(),
                reasoning: "Reasoning for Balthasar that is very lengthy".to_string(),
            },
        ];

        let output = formatter.format_dissent(&dissent);

        // Count lines matching the "**Name (Title)**:" pattern
        let header_lines: Vec<&str> = output
            .lines()
            .filter(|l| l.starts_with("**") && l.contains(")**:"))
            .collect();
        assert_eq!(
            header_lines.len(),
            2,
            "Expected exactly 2 dissenter header lines, got {}: {:?}",
            header_lines.len(),
            header_lines
        );
    }

    /// Each dissenter line contains the summary text but NOT the reasoning text.
    #[test]
    fn test_dissent_line_contains_summary_not_reasoning() {
        let formatter = ReportFormatter::new();
        let dissent = vec![Dissent {
            agent: AgentName::Caspar,
            summary: "Unique summary text here".to_string(),
            reasoning: "Unique reasoning text should not appear".to_string(),
        }];

        let output = formatter.format_dissent(&dissent);

        assert!(
            output.contains("Unique summary text here"),
            "Output must contain the summary"
        );
        assert!(
            !output.contains("Unique reasoning text should not appear"),
            "Output must NOT contain the reasoning"
        );
    }

    /// The dissent section ends with a blank line after the last dissenter's line.
    #[test]
    fn test_dissent_section_has_blank_line_after() {
        let formatter = ReportFormatter::new();
        let dissent = vec![Dissent {
            agent: AgentName::Caspar,
            summary: "Some summary".to_string(),
            reasoning: "Some reasoning".to_string(),
        }];

        let output = formatter.format_dissent(&dissent);

        // The output must end with "\n\n" (dissenter line + trailing blank line)
        assert!(
            output.ends_with("\n\n"),
            "Dissent section must end with a blank line (\\n\\n), got: {:?}",
            output
        );
    }

    // -- S08: Finding line layout (single line, fixed-width columns, no detail) --

    /// Finding detail text must not appear in the rendered findings section.
    ///
    /// The detail field is preserved in the JSON output (`DedupFinding::detail`)
    /// but is intentionally excluded from the markdown report.
    #[test]
    fn test_findings_line_does_not_contain_detail_text() {
        let formatter = ReportFormatter::new();
        let findings = vec![DedupFinding {
            severity: Severity::Critical,
            title: "SQL injection in query builder".to_string(),
            detail: "UNIQUE_DETAIL_SENTINEL_XYZ".to_string(),
            sources: vec![AgentName::Melchior],
            file: None,
            line: None,
            category: Category::Other,
            id: None,
        }];

        let output = formatter.format_findings(&findings);

        assert!(
            !output.contains("UNIQUE_DETAIL_SENTINEL_XYZ"),
            "Detail text must not appear in the markdown findings output"
        );
    }

    /// Marker column is exactly 5 characters, left-justified.
    ///
    /// The first token before the separator space must be exactly 5 bytes
    /// (the icon padded to `FINDING_MARKER_WIDTH`).
    #[test]
    fn test_findings_line_marker_column_is_5_chars_left_justified() {
        let formatter = ReportFormatter::new();
        let findings = vec![
            DedupFinding {
                severity: Severity::Critical,
                title: "Critical finding".to_string(),
                detail: "detail".to_string(),
                sources: vec![AgentName::Melchior],
                file: None,
                line: None,
                category: Category::Other,
                id: None,
            },
            DedupFinding {
                severity: Severity::Warning,
                title: "Warning finding".to_string(),
                detail: "detail".to_string(),
                sources: vec![AgentName::Balthasar],
                file: None,
                line: None,
                category: Category::Other,
                id: None,
            },
            DedupFinding {
                severity: Severity::Info,
                title: "Info finding".to_string(),
                detail: "detail".to_string(),
                sources: vec![AgentName::Caspar],
                file: None,
                line: None,
                category: Category::Other,
                id: None,
            },
        ];

        let output = formatter.format_findings(&findings);

        for line in output.lines() {
            if line.starts_with('[') {
                // The marker column occupies positions 0..5 (5 chars), then a space at index 5.
                let marker_col = &line[..5];
                assert_eq!(
                    marker_col.len(),
                    5,
                    "Marker column must be 5 chars; got {:?} in line {:?}",
                    marker_col,
                    line
                );
                assert_eq!(
                    line.chars().nth(5),
                    Some(' '),
                    "Column 5 must be a space separator; got {:?} in line {:?}",
                    line.chars().nth(5),
                    line
                );
            }
        }
    }

    /// Severity label column is exactly 14 characters wide (padded with trailing spaces).
    ///
    /// The severity label token (chars 6..20) must be exactly 14 bytes,
    /// with the markdown-decorated label left-justified inside that width.
    #[test]
    fn test_findings_line_severity_label_column_is_14_chars_left_justified() {
        let formatter = ReportFormatter::new();
        let findings = vec![
            DedupFinding {
                severity: Severity::Critical,
                title: "A".to_string(),
                detail: "d".to_string(),
                sources: vec![AgentName::Melchior],
                file: None,
                line: None,
                category: Category::Other,
                id: None,
            },
            DedupFinding {
                severity: Severity::Warning,
                title: "B".to_string(),
                detail: "d".to_string(),
                sources: vec![AgentName::Balthasar],
                file: None,
                line: None,
                category: Category::Other,
                id: None,
            },
            DedupFinding {
                severity: Severity::Info,
                title: "C".to_string(),
                detail: "d".to_string(),
                sources: vec![AgentName::Caspar],
                file: None,
                line: None,
                category: Category::Other,
                id: None,
            },
        ];

        let output = formatter.format_findings(&findings);

        for line in output.lines() {
            if line.starts_with('[') {
                // Layout: [marker:5] [space] [severity_label:14] [space] ...
                // Positions: 0-4 = marker (5 chars), 5 = space, 6-19 = severity (14 chars), 20 = space
                assert!(
                    line.len() >= 21,
                    "Line too short to contain marker+severity columns: {:?}",
                    line
                );
                let severity_col = &line[6..20];
                assert_eq!(
                    severity_col.len(),
                    14,
                    "Severity label column must be 14 chars; got {:?} in line {:?}",
                    severity_col,
                    line
                );
                assert_eq!(
                    line.chars().nth(20),
                    Some(' '),
                    "Column 20 must be a space separator after severity; got {:?} in line {:?}",
                    line.chars().nth(20),
                    line
                );
            }
        }
    }

    /// Full finding line matches the Python MAGI reference layout byte-for-byte.
    ///
    /// Format: `{marker:<5} {severity_label:<14} {title} _(from {sources})_`
    #[test]
    fn test_findings_line_matches_python_layout_exactly() {
        let formatter = ReportFormatter::new();
        let findings = vec![
            DedupFinding {
                severity: Severity::Critical,
                title: "Test title".to_string(),
                detail: "ignored detail".to_string(),
                sources: vec![AgentName::Melchior, AgentName::Caspar],
                file: None,
                line: None,
                category: Category::Other,
                id: None,
            },
            DedupFinding {
                severity: Severity::Warning,
                title: "Missing retry logic".to_string(),
                detail: "ignored detail".to_string(),
                sources: vec![AgentName::Balthasar],
                file: None,
                line: None,
                category: Category::Other,
                id: None,
            },
            DedupFinding {
                severity: Severity::Info,
                title: "Consider timeout".to_string(),
                detail: "ignored detail".to_string(),
                sources: vec![AgentName::Caspar],
                file: None,
                line: None,
                category: Category::Other,
                id: None,
            },
        ];

        let output = formatter.format_findings(&findings);

        // Exact expected lines per Python MAGI reference layout
        let expected_critical = "[!!!] **[CRITICAL]** Test title _(from Melchior, Caspar)_";
        let expected_warning = "[!!]  **[WARNING]**  Missing retry logic _(from Balthasar)_";
        let expected_info = "[i]   **[INFO]**     Consider timeout _(from Caspar)_";

        assert!(
            output.contains(expected_critical),
            "Critical line does not match Python layout.\nExpected: {:?}\nOutput:\n{}",
            expected_critical,
            output
        );
        assert!(
            output.contains(expected_warning),
            "Warning line does not match Python layout.\nExpected: {:?}\nOutput:\n{}",
            expected_warning,
            output
        );
        assert!(
            output.contains(expected_info),
            "Info line does not match Python layout.\nExpected: {:?}\nOutput:\n{}",
            expected_info,
            output
        );
    }

    // -- S10: fit_content helper --

    /// fit_content returns input unchanged when input length <= width.
    #[test]
    fn test_fit_content_returns_input_when_shorter_than_width() {
        assert_eq!(fit_content("hello", 10, ""), "hello");
        assert_eq!(fit_content("hi", 10, "lo"), "hi");
    }

    /// fit_content returns input unchanged when input length exactly equals width.
    #[test]
    fn test_fit_content_returns_input_when_exactly_width() {
        assert_eq!(fit_content("hello", 5, ""), "hello");
        assert_eq!(fit_content("abcde", 5, "lo"), "abcde");
    }

    /// fit_content preserves the suffix and ellipsizes the prefix when prefix overflows.
    #[test]
    fn test_fit_content_preserves_suffix_when_prefix_overflows() {
        // content = "abcdefghij" (10), width = 8, preserve_suffix = "hij"
        // prefix_budget = 8 - 3 - 3 = 2
        // prefix_source = "abcdefg" (content minus last 3 chars)
        // prefix_source[..2] = "ab"
        // result = "ab...hij" (8 chars)
        assert_eq!(fit_content("abcdefghij", 8, "hij"), "ab...hij");
    }

    /// fit_content falls back to tail-cut when no suffix is given.
    #[test]
    fn test_fit_content_falls_back_to_tail_cut_when_no_suffix() {
        // content = "abcdefghij" (10), width = 6, preserve_suffix = ""
        // fallback: cutoff = max(1, 6-3) = 3, result = "abc..."
        assert_eq!(fit_content("abcdefghij", 6, ""), "abc...");
    }

    /// fit_content falls back to tail-cut when suffix + ellipsis >= width.
    #[test]
    fn test_fit_content_falls_back_to_tail_cut_when_suffix_plus_ellipsis_exceeds_width() {
        // preserve_suffix = "xy" (2), ELLIPSIS = 3 bytes, total = 5 = width
        // condition: len(preserve_suffix) + 3 >= width  →  2 + 3 >= 5  → true → fallback
        // cutoff = max(1, 5-3) = 2, result = "ab..."
        assert_eq!(fit_content("abcdefghij", 5, "xy"), "ab...");
    }

    /// fit_content never appends a suffix the content did not carry.
    ///
    /// MAGI S2, Melchior `[WARNING]`: Step 3's precondition (`content` ends with
    /// `preserve_suffix`) was only a `debug_assert!`, so release builds took the branch
    /// regardless. Two consequences, and the quiet one is worse: it **fabricates** — it
    /// lops `preserve_suffix.len()` bytes off the tail and appends the suffix, so
    /// `("abcdefghij", 9, "xyz")` used to render `abc...xyz`, inventing a `xyz` that was
    /// never in the input. With non-ASCII content that same byte offset can also land
    /// inside a codepoint and panic — in a formatter, on the report that IS the product.
    ///
    /// A library must not invent text. When the invariant does not hold the function
    /// degrades to the same tail-cut Step 2 uses, which is honest about what it dropped.
    #[test]
    fn test_fit_content_does_not_fabricate_a_suffix_the_content_lacks() {
        // width 9: suffix(3) + ellipsis(3) < 9, so Step 2 does not catch it and Step 3
        // runs — with a `preserve_suffix` that is NOT a suffix of `content`.
        let result = fit_content("abcdefghij", 9, "xyz");
        assert!(
            !result.ends_with("xyz"),
            "fit_content invented a suffix absent from the input: {result:?}"
        );
        assert_eq!(result, "abcdef...", "expected the honest tail-cut");
    }

    /// fit_content ellipsis is exactly three dots.
    #[test]
    fn test_fit_content_ellipsis_is_exactly_three_dots() {
        let result = fit_content("abcdefghij", 6, "");
        assert!(
            result.ends_with("..."),
            "Expected ellipsis '...', got: {:?}",
            result
        );
        let ellipsis_start = result.len() - 3;
        assert_eq!(&result[ellipsis_start..], "...");
    }

    /// fit_content resulting byte length equals width when truncation occurs.
    #[test]
    fn test_fit_content_resulting_length_equals_width_when_truncated() {
        // All cases where content.len() > width must produce exactly width bytes
        // (except width < 4 edge case documented below)
        for w in 4..=20usize {
            let content = "a".repeat(w + 5);
            let result = fit_content(&content, w, "");
            assert_eq!(
                result.len(),
                w,
                "Expected result length {w} for width={w}, got {} from {:?}",
                result.len(),
                result
            );
        }
        // With suffix
        let result = fit_content("abcdefghij", 8, "hij");
        assert_eq!(
            result.len(),
            8,
            "Expected 8, got {}: {:?}",
            result.len(),
            result
        );
    }

    /// fit_content boundary at width=1: Python-literal fallback produces "a..." (4 bytes).
    ///
    /// For width < 4, the result length exceeds `width` (cannot fit ellipsis + 1 char).
    /// This is an accepted edge case documented in the spec — a literal port of Python behavior.
    /// Callers should ensure `width >= 4` for sensible truncation.
    ///
    /// This test is skipped in debug builds because `fit_content` fires a `debug_assert!(width >= 4)`
    /// to catch unintended callers in dev/test environments. The width=1 path is only reachable in
    /// release mode where the assert is compiled out.
    #[test]
    #[cfg(not(debug_assertions))]
    fn test_fit_content_boundary_width_1() {
        // width=1: cutoff = max(1, 1.saturating_sub(3)) = max(1, 0) = 1
        // result = "a..." (4 bytes, exceeds width — documented edge case)
        let result = fit_content("abc", 1, "");
        assert_eq!(
            result, "a...",
            "Expected 'a...' for width=1, got: {:?}",
            result
        );
    }

    // -- S10: Banner column alignment --

    /// All agent labels are left-aligned to the same column width (max_label_len).
    #[test]
    fn test_banner_labels_are_column_aligned_to_max_label_len() {
        // Default config: Melchior (Scientist): = 21, Balthasar (Pragmatist): = 23, Caspar (Critic): = 17
        // max_label_len = 23
        // After alignment: all verdict suffixes start at the same column
        let m = make_agent(AgentName::Melchior, Verdict::Approve, 0.9, "S", "R", "Rec");
        let b = make_agent(
            AgentName::Balthasar,
            Verdict::Approve,
            0.85,
            "S",
            "R",
            "Rec",
        );
        let c = make_agent(AgentName::Caspar, Verdict::Approve, 0.78, "S", "R", "Rec");
        let agents = vec![m.clone(), b.clone(), c.clone()];
        let consensus = make_consensus("STRONG GO", Verdict::Approve, 0.9, 1.0, &[&m, &b, &c]);

        let formatter = ReportFormatter::new();
        let banner = formatter.format_banner(&agents, &consensus);

        // Find the agent content lines (not separator, not header, not consensus)
        // Each agent line starts with "|  " and contains a verdict.
        // The verdict suffix " APPROVE (NN%)" must start at the same column in all lines.
        let agent_lines: Vec<&str> = banner
            .lines()
            .filter(|l| l.starts_with('|') && l.contains("APPROVE") && !l.contains("CONSENSUS"))
            .collect();

        assert_eq!(agent_lines.len(), 3, "Expected 3 agent lines");

        // Find position of " APPROVE" in each line — must be the same for all
        let verdict_positions: Vec<usize> = agent_lines
            .iter()
            .map(|l| l.find(" APPROVE").expect("APPROVE not found"))
            .collect();

        let first_pos = verdict_positions[0];
        for (i, &pos) in verdict_positions.iter().enumerate() {
            assert_eq!(
                pos, first_pos,
                "Agent line {i} has APPROVE at column {pos}, expected {first_pos}\nLines: {agent_lines:?}"
            );
        }
    }

    /// When a label is so long that the rendered line would overflow, the label is ellipsized
    /// but the verdict suffix is preserved intact.
    #[test]
    fn test_banner_verdict_preserved_when_label_exceeds_width() {
        // Use a very long title that would push the line over banner_inner (50 chars)
        let mut config = ReportConfig::default();
        config.agent_titles.insert(
            AgentName::Balthasar,
            (
                "Balthasar".to_string(),
                "Very Long Pragmatist Title Indeed Here".to_string(),
            ),
        );
        let formatter = ReportFormatter::with_config(config).unwrap();

        let b = make_agent(
            AgentName::Balthasar,
            Verdict::Approve,
            0.85,
            "S",
            "R",
            "Rec",
        );
        let agents = vec![b.clone()];
        let consensus = make_consensus("GO (1-0)", Verdict::Approve, 0.85, 1.0, &[&b]);

        let banner = formatter.format_banner(&agents, &consensus);

        // All lines must still be exactly 52 chars
        for line in banner.lines() {
            if !line.is_empty() {
                assert_eq!(line.len(), 52, "Line is not 52 chars: {:?}", line);
            }
        }

        // The verdict suffix must appear intact in the banner
        let verdict_suffix = " APPROVE (85%)";
        assert!(
            banner.contains(verdict_suffix),
            "Verdict suffix {:?} must be preserved in banner:\n{}",
            verdict_suffix,
            banner
        );
    }

    /// The consensus line for GO WITH CAVEATS includes the split count (e.g., "GO WITH CAVEATS (2-1)").
    #[test]
    fn test_banner_consensus_line_includes_split_for_go_with_caveats() {
        let m = make_agent(AgentName::Melchior, Verdict::Approve, 0.9, "S", "R", "Rec");
        let b = make_agent(AgentName::Balthasar, Verdict::Approve, 0.8, "S", "R", "Rec");
        let c = make_agent(AgentName::Caspar, Verdict::Reject, 0.75, "S", "R", "Rec");
        let agents = vec![m.clone(), b.clone(), c.clone()];
        // The consensus label must include the split — this is produced by the consensus engine (S05)
        let consensus = make_consensus(
            "GO WITH CAVEATS (2-1)",
            Verdict::Approve,
            0.8,
            0.33,
            &[&m, &b, &c],
        );

        let formatter = ReportFormatter::new();
        let banner = formatter.format_banner(&agents, &consensus);

        assert!(
            banner.contains("GO WITH CAVEATS (2-1)"),
            "Banner consensus line must include the split count: {banner}"
        );

        // All lines must be exactly 52 chars
        for line in banner.lines() {
            if !line.is_empty() {
                assert_eq!(line.len(), 52, "Line is not 52 chars: {:?}", line);
            }
        }
    }

    /// All lines of format_banner output are exactly banner_width (52) bytes.
    ///
    /// Re-verifies the existing invariant after alignment changes.
    #[test]
    fn test_banner_all_lines_are_exactly_banner_width() {
        let m = make_agent(
            AgentName::Melchior,
            Verdict::Approve,
            0.9,
            "Good",
            "R",
            "Rec",
        );
        let b = make_agent(
            AgentName::Balthasar,
            Verdict::Conditional,
            0.85,
            "Ok",
            "R",
            "Rec",
        );
        let c = make_agent(AgentName::Caspar, Verdict::Reject, 0.78, "Bad", "R", "Rec");
        let agents = vec![m.clone(), b.clone(), c.clone()];
        let consensus = make_consensus(
            "GO WITH CAVEATS (2-1)",
            Verdict::Approve,
            0.85,
            0.33,
            &[&m, &b, &c],
        );

        let formatter = ReportFormatter::new();
        let banner = formatter.format_banner(&agents, &consensus);

        for line in banner.lines() {
            if !line.is_empty() {
                assert_eq!(line.len(), 52, "Line is not 52 chars: {:?}", line);
            }
        }
    }

    /// Agent names in JSON are lowercase.
    #[test]
    fn test_magi_report_json_agent_names_lowercase() {
        let m = make_agent(AgentName::Melchior, Verdict::Approve, 0.9, "S", "R", "Rec");
        let agents = vec![m.clone()];
        let consensus = make_consensus("GO (1-0)", Verdict::Approve, 0.9, 1.0, &[&m]);

        let report = MagiReport {
            agents,
            consensus,
            banner: String::new(),
            report: String::new(),
            degraded: false,
            failed_agents: BTreeMap::new(),
            retried_agents: BTreeSet::new(),
            rotations: BTreeMap::new(),
            extraction_failures: BTreeMap::new(),
        };

        let json = serde_json::to_string(&report).expect("serialize");
        assert!(
            json.contains("\"melchior\""),
            "Agent name should be lowercase in JSON"
        );
        assert!(
            !json.contains("\"Melchior\""),
            "Agent name should NOT be capitalized in JSON"
        );
    }

    /// consensus.confidence is rounded to 2 decimals.
    #[test]
    fn test_magi_report_confidence_rounded() {
        let m = make_agent(AgentName::Melchior, Verdict::Approve, 0.9, "S", "R", "Rec");
        let agents = vec![m.clone()];
        let mut consensus = make_consensus("GO (1-0)", Verdict::Approve, 0.86, 1.0, &[&m]);
        consensus.confidence = 0.8567;

        let report = MagiReport {
            agents,
            consensus,
            banner: String::new(),
            report: String::new(),
            degraded: false,
            failed_agents: BTreeMap::new(),
            retried_agents: BTreeSet::new(),
            rotations: BTreeMap::new(),
            extraction_failures: BTreeMap::new(),
        };

        // Confidence rounding is done by the consensus engine, not by MagiReport.
        // Here we verify the field value is preserved as-is during serialization.
        let json = serde_json::to_string(&report).expect("serialize");
        assert!(
            json.contains("0.8567"),
            "Confidence should be serialized as-is (rounding is consensus engine's job)"
        );
    }

    // -- ReportConfig::new_checked tests --

    /// ReportConfig::new_checked accepts all ASCII titles.
    #[test]
    fn test_new_checked_accepts_all_ascii_titles() {
        let mut agent_titles = BTreeMap::new();
        agent_titles.insert(
            AgentName::Melchior,
            ("Melchior".to_string(), "Scientist".to_string()),
        );
        agent_titles.insert(
            AgentName::Balthasar,
            ("Balthasar".to_string(), "Pragmatist".to_string()),
        );
        agent_titles.insert(
            AgentName::Caspar,
            ("Caspar".to_string(), "Critic".to_string()),
        );

        let result = ReportConfig::new_checked(52, agent_titles);
        assert!(result.is_ok(), "Should accept all ASCII titles");
        let config = result.unwrap();
        assert_eq!(config.banner_width, 52);
    }

    /// ReportConfig::new_checked rejects non-ASCII display_name.
    #[test]
    fn test_new_checked_rejects_non_ascii_display_name() {
        let mut agent_titles = BTreeMap::new();
        agent_titles.insert(
            AgentName::Melchior,
            ("Mélchior".to_string(), "Scientist".to_string()),
        );

        let result = ReportConfig::new_checked(52, agent_titles);
        assert!(result.is_err(), "Should reject non-ASCII display_name");
        let err = result.unwrap_err();
        let ReportError::NonAsciiTitle {
            agent,
            field,
            value,
            ..
        } = err
        else {
            panic!("expected NonAsciiTitle, got {err:?}");
        };
        assert_eq!(agent, AgentName::Melchior);
        assert_eq!(field, "display_name");
        assert_eq!(value, "Mélchior");
    }

    /// ReportConfig::new_checked rejects non-ASCII title field.
    #[test]
    fn test_new_checked_rejects_non_ascii_title_field() {
        let mut agent_titles = BTreeMap::new();
        agent_titles.insert(
            AgentName::Balthasar,
            ("Balthasar".to_string(), "Pragmátist".to_string()),
        );

        let result = ReportConfig::new_checked(52, agent_titles);
        assert!(result.is_err(), "Should reject non-ASCII title");
        let err = result.unwrap_err();
        let ReportError::NonAsciiTitle {
            agent,
            field,
            value,
            ..
        } = err
        else {
            panic!("expected NonAsciiTitle, got {err:?}");
        };
        assert_eq!(agent, AgentName::Balthasar);
        assert_eq!(field, "title");
        assert_eq!(value, "Pragmátist");
    }

    // -- ReportConfig::new_checked rejects banner_width too small --

    #[test]
    fn test_new_checked_rejects_banner_width_too_small() {
        let titles = BTreeMap::new();
        for width in [0usize, 1, 4, 7] {
            let result = ReportConfig::new_checked(width, titles.clone());
            assert!(result.is_err(), "banner_width={width} should be rejected");
            assert_eq!(
                result.unwrap_err(),
                ReportError::BannerTooSmall {
                    requested: width,
                    minimum: ReportConfig::MIN_BANNER_WIDTH,
                },
                "wrong error variant for banner_width={width}"
            );
        }
    }

    #[test]
    fn test_new_checked_accepts_banner_width_at_minimum() {
        let titles = BTreeMap::new();
        assert!(
            ReportConfig::new_checked(ReportConfig::MIN_BANNER_WIDTH, titles).is_ok(),
            "banner_width == MIN_BANNER_WIDTH should be accepted"
        );
    }

    // -- ReportFormatter::with_config re-validation tests (Fix 1) --

    /// with_config rejects banner_width below minimum even when set via field mutation.
    #[test]
    fn test_with_config_rejects_banner_width_too_small() {
        // Construct directly to bypass new_checked validation and test that with_config
        // catches the invalid value at formatter construction time.
        let cfg = ReportConfig {
            banner_width: 1,
            ..ReportConfig::default()
        };
        match ReportFormatter::with_config(cfg) {
            Err(ReportError::BannerTooSmall { requested, minimum }) => {
                assert_eq!(requested, 1);
                assert_eq!(minimum, ReportConfig::MIN_BANNER_WIDTH);
            }
            Err(other) => panic!("expected BannerTooSmall, got {other:?}"),
            Ok(_) => panic!("with_config must re-validate banner_width"),
        }
    }

    /// with_config rejects non-ASCII agent title set after default construction.
    #[test]
    fn test_with_config_rejects_non_ascii_agent_title() {
        let mut titles = BTreeMap::new();
        titles.insert(
            AgentName::Melchior,
            ("Ménagère".to_string(), "Scientist".to_string()),
        );
        let cfg = ReportConfig {
            banner_width: 52,
            agent_titles: titles,
        };
        match ReportFormatter::with_config(cfg) {
            Err(ReportError::NonAsciiTitle { agent, field, .. }) => {
                assert_eq!(agent, AgentName::Melchior);
                assert_eq!(field, "display_name");
            }
            Err(other) => panic!("expected NonAsciiTitle, got {other:?}"),
            Ok(_) => panic!("with_config must reject non-ASCII titles"),
        }
    }

    // -- T6: MagiReport non_exhaustive + structured-field guard tests --

    /// Regression guard: format_findings must NOT render file, category, or id
    /// in the markdown output. These are JSON-only telemetry fields.
    ///
    /// If this test fails, format_findings has started leaking structured
    /// fields into the report — fix that before proceeding.
    #[test]
    fn test_report_markdown_omits_structured_finding_fields() {
        let f = DedupFinding {
            severity: Severity::Warning,
            title: "T".into(),
            detail: "D".into(),
            sources: vec![AgentName::Melchior],
            file: Some("src/secret_path.rs".into()),
            line: Some(42),
            category: Category::LogicError,
            id: Some("abc123def4567890".into()),
        };
        let md = ReportFormatter::new().format_findings(std::slice::from_ref(&f));
        assert!(
            !md.contains("src/secret_path.rs"),
            "format_findings must not render `file` — got:\n{md}"
        );
        assert!(
            !md.contains("category"),
            "format_findings must not render `category` — got:\n{md}"
        );
        assert!(
            !md.contains("abc123def4567890"),
            "format_findings must not render `id` — got:\n{md}"
        );
    }

    /// Backward-compat guard: a v0.3.1-era JSON finding (no file/line/category
    /// keys) deserializes with the new fields at their serde defaults:
    /// `file == None`, `line == None`, `category == Category::Other`.
    ///
    /// Uses an inline JSON snippet rather than the existing fixture (which has
    /// `findings: []`) so we can exercise a non-empty findings array without
    /// modifying the shared fixture.
    #[test]
    fn test_dedup_finding_legacy_json_defaults_structured_fields() {
        // Simulate a DedupFinding serialized by v0.3.1 (no file/line/category/id keys).
        let json = r#"{
            "severity": "warning",
            "title": "Legacy finding",
            "detail": "Some detail",
            "sources": ["melchior"]
        }"#;
        let finding: DedupFinding =
            serde_json::from_str(json).expect("legacy DedupFinding JSON must deserialize cleanly");
        assert_eq!(finding.file, None, "absent `file` must default to None");
        assert_eq!(finding.line, None, "absent `line` must default to None");
        assert_eq!(
            finding.category,
            Category::Other,
            "absent `category` must default to Category::Other"
        );
        assert_eq!(finding.id, None, "absent `id` must default to None");
        assert_eq!(finding.title, "Legacy finding");
    }
}
