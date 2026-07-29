// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05

//! Compile-time embedded system prompts for the three agents.
//!
//! **Public since MS3.** The module was crate-internal until `3.0.0`; it is now `pub`
//! for three reasons that reinforce each other:
//!
//! 1. `MagiBuilder::build()` **enforces** the verdict-marker contract, so a consumer
//!    writing a custom prompt has to be able to see the canonical shape. Without this,
//!    the guard is a wall with no door.
//! 2. The built-in prompts **are** the migration path — always in sync via
//!    `include_str!`, so there is no fourth copy of the canonical section to drift.
//!    Start from [`caspar_prompt`] and edit, or copy its `## Output format` verbatim.
//! 3. [`validate_prompt`] must be real public API for the strict marker predicate to
//!    have a production consumer at all.
//!
//! Visibility is deliberately **narrow**: the three accessors and `validate_prompt` are
//! public; `lookup_prompt` and `embedded_prompt_for` stay `pub(crate)`.
//!
//! Being public also makes doctests possible here for the first time — a doctest is
//! compiled as an *external* crate, so it could not reach a private item before.

use std::collections::BTreeMap;

use crate::error::MagiError;
use crate::schema::{AgentName, Mode};
use crate::verdict_markers::{is_exact_marker_line, locate_block};

// ── Mode-agnostic accessors (v0.3.0) ─────────────────────────────────────────

/// Returns the consolidated, mode-agnostic system prompt for Melchior (Scientist).
///
/// This prompt is loaded at compile time from `prompts_md/melchior.md` and is
/// used by [`crate::agent::Agent`] when no custom prompt is configured.
pub fn melchior_prompt() -> &'static str {
    include_str!("../prompts_md/melchior.md")
}

/// Returns the consolidated, mode-agnostic system prompt for Balthasar (Pragmatist).
///
/// This prompt is loaded at compile time from `prompts_md/balthasar.md` and is
/// used by [`crate::agent::Agent`] when no custom prompt is configured.
pub fn balthasar_prompt() -> &'static str {
    include_str!("../prompts_md/balthasar.md")
}

/// Returns the consolidated, mode-agnostic system prompt for Caspar (Critic).
///
/// This prompt is loaded at compile time from `prompts_md/caspar.md` and is
/// used by [`crate::agent::Agent`] when no custom prompt is configured.
pub fn caspar_prompt() -> &'static str {
    include_str!("../prompts_md/caspar.md")
}

/// Returns the compiled-in system prompt for the given agent name.
///
/// Shared by [`crate::agent::Agent::new`] and [`lookup_prompt`]
/// to avoid duplicate `match` arms. Any change to the embedded prompt mapping
/// must be made here only.
///
/// # Parameters
/// - `name`: The agent whose embedded prompt to retrieve.
pub(crate) fn embedded_prompt_for(name: AgentName) -> &'static str {
    match name {
        AgentName::Melchior => melchior_prompt(),
        AgentName::Balthasar => balthasar_prompt(),
        AgentName::Caspar => caspar_prompt(),
    }
}

// ── Prompt resolution ─────────────────────────────────────────────────────────

/// Resolves the system prompt for an agent given a mode and the overrides map.
///
/// Priority order:
/// 1. Mode-specific override: `(agent, Some(mode))`
/// 2. Mode-agnostic override: `(agent, None)`
/// 3. Compiled-in embedded default for the agent
///
/// # Parameters
/// - `agent`: Which MAGI agent (Melchior, Balthasar, Caspar).
/// - `mode`: The current analysis mode.
/// - `overrides`: Map of custom prompt overrides keyed by `(AgentName, Option<Mode>)`.
///
/// # Returns
/// A string slice of the resolved prompt (borrowed from the map or `'static` from embedded).
pub(crate) fn lookup_prompt(
    agent: AgentName,
    mode: Mode,
    overrides: &BTreeMap<(AgentName, Option<Mode>), String>,
) -> &str {
    if let Some(s) = overrides.get(&(agent, Some(mode))) {
        return s.as_str();
    }
    if let Some(s) = overrides.get(&(agent, None)) {
        return s.as_str();
    }
    embedded_prompt_for(agent)
}

/// Checks that `prompt` satisfies the verdict-marker contract.
///
/// This is the **same function** `MagiBuilder::build()` runs, so what it accepts is
/// exactly what `build()` accepts. Call it from your own test suite before deploying a
/// custom prompt, instead of discovering the problem when `build()` returns `Err`.
///
/// # What it checks (MS3 T1 — minimal)
///
/// Exactly **one** `<MAGI_VERDICT>` line and **one** `</MAGI_VERDICT>` line, in that
/// order, judged with the **strict** predicate: in a file you ship, an invisible
/// character inside a marker line is corruption, not tolerance owed.
///
/// A leading BOM is tolerated: it is an artefact of the file's **encoding**, not a
/// property of a line, so it is resolved here — before anything is compared. Resolving
/// it in the comparator is the mistake the reference implementation made (a false FATAL
/// aborted a run) before moving it to the encoding layer.
///
/// # Errors
///
/// [`MagiError::PromptContract`] naming the rule that was violated.
///
/// # Examples
///
/// ```
/// use magi_core::prompts::validate_prompt;
/// use magi_core::verdict_markers::{VERDICT_CLOSE, VERDICT_OPEN};
///
/// // Each marker alone on its own line, exactly once, wrapping a NON-JSON placeholder.
/// let mine = format!(
///     "You are Caspar.\n\n## Output format\n{VERDICT_OPEN}\n{{ ...your 7-key JSON object... }}\n{VERDICT_CLOSE}"
/// );
/// assert!(validate_prompt(&mine).is_ok());
///
/// // A legacy prompt with no marker block is rejected — and `build()` would reject it
/// // too, which is the point of checking here first.
/// assert!(validate_prompt("You are Caspar. Reply with only a JSON object.").is_err());
/// ```
pub fn validate_prompt(prompt: &str) -> Result<(), MagiError> {
    let body = prompt.strip_prefix('\u{feff}').unwrap_or(prompt);
    locate_block(body, is_exact_marker_line)
        .map(|_| ())
        .map_err(|e| MagiError::PromptContract {
            // DEBT, not design — T5 must eliminate this. `validate_prompt(&str)` cannot
            // know the seat, so naming one here risks naming the WRONG one: a consumer
            // validating their Caspar prompt would read "melchior". T5 adds the internal
            // path that DOES know agent and mode, and the acceptance condition is that
            // the public path never names a wrong agent.
            agent: AgentName::Melchior,
            mode: None,
            reason: format!("{e}; verify with prompts::validate_prompt before deploying"),
        })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests_verdict_contract {
    use super::*;
    use crate::verdict_markers::{VERDICT_CLOSE, VERDICT_OPEN};

    /// NOT vacuous: at T1 the prompts still carry NO markers (the re-pin is T4), so the
    /// contract must REJECT them. T4 flips this assertion — and that flip is how the
    /// re-pin PROVES it installed the markers, rather than asserting it in prose.
    #[test]
    fn test_shipped_prompts_do_not_yet_satisfy_the_contract_before_the_repin() {
        for p in [melchior_prompt(), balthasar_prompt(), caspar_prompt()] {
            assert!(
                matches!(validate_prompt(p), Err(MagiError::PromptContract { .. })),
                "pre-repin prompts carry no markers; T4 flips this"
            );
        }
    }

    #[test]
    fn test_validate_prompt_rejects_prose_without_markers() {
        assert!(matches!(
            validate_prompt("You are Melchior. Respond with JSON."),
            Err(MagiError::PromptContract { .. })
        ));
    }

    #[test]
    fn test_validate_prompt_accepts_exactly_one_ordered_pair() {
        let ok = format!("intro\n{VERDICT_OPEN}\n{{ ...slot... }}\n{VERDICT_CLOSE}\nfin");
        validate_prompt(&ok).expect("one ordered pair must validate");
    }

    #[test]
    fn test_validate_prompt_rejects_two_pairs() {
        let two = format!("{VERDICT_OPEN}\na\n{VERDICT_CLOSE}\n{VERDICT_OPEN}\nb\n{VERDICT_CLOSE}");
        assert!(matches!(
            validate_prompt(&two),
            Err(MagiError::PromptContract { .. })
        ));
    }

    #[test]
    fn test_validate_prompt_tolerates_a_leading_bom() {
        // R3 — the BOM is resolved in the encoding layer, not in the comparator.
        let p = format!("\u{feff}intro\n{VERDICT_OPEN}\n{{ ...slot... }}\n{VERDICT_CLOSE}");
        validate_prompt(&p).expect("a leading BOM must not fail the contract");
    }

    #[test]
    fn test_validate_prompt_uses_the_strict_predicate_on_our_own_files() {
        // E21 — a zero-width inside a marker line is CORRUPTION in a file we ship, even
        // though the same line in model output would be accepted (E9).
        let corrupt = format!("<MAGI_\u{200b}VERDICT>\nx\n{VERDICT_CLOSE}");
        assert!(matches!(
            validate_prompt(&corrupt),
            Err(MagiError::PromptContract { .. })
        ));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_v3_prompts_contain_calibration_markers() {
        let melchior = include_str!("../prompts_md/melchior.md");
        let balthasar = include_str!("../prompts_md/balthasar.md");
        let caspar = include_str!("../prompts_md/caspar.md");
        for p in [melchior, balthasar, caspar] {
            assert!(
                p.contains("Finding calibration"),
                "missing calibration section"
            );
            assert!(!p.contains('\r'), "CRLF detected — must be LF");
            assert!(!p.starts_with('\u{feff}'), "BOM detected — must be no-BOM");
        }
        assert!(
            caspar.contains("Critic's override"),
            "caspar missing override"
        );
    }

    /// F0 fabrication-echo hardening: the worked example embedded in each
    /// prompt must never carry an `approve` verdict. A model that echoes the
    /// example verbatim would otherwise fabricate a clean `approve` in the
    /// adversarial seat — the worst silent failure the system can produce.
    /// The example uses `conditional` instead (echo → GO WITH CAVEATS,
    /// visible), matching the Python MAGI plugin's v5.1.0+ prompts.
    #[test]
    fn test_worked_examples_do_not_ship_an_approve_verdict() {
        let prompts = [
            ("melchior.md", include_str!("../prompts_md/melchior.md")),
            ("balthasar.md", include_str!("../prompts_md/balthasar.md")),
            ("caspar.md", include_str!("../prompts_md/caspar.md")),
        ];
        for (name, p) in prompts {
            // Whitespace-normalized so a re-pinned prompt cannot evade the
            // check via `"verdict":"approve"` / `"verdict" : "approve"`
            // spellings (during a re-pin the SHA fixture is regenerated too,
            // leaving this property test as the only guard).
            let flat: String = p.chars().filter(|c| !c.is_whitespace()).collect();
            assert!(
                !flat.contains(r#""verdict":"approve""#),
                "{name}: worked example carries an echo-fabricable approve verdict"
            );
            assert!(
                flat.contains(r#""verdict":"conditional""#),
                "{name}: worked example must use the conditional verdict"
            );
        }
    }
}

#[cfg(test)]
mod tests_v0_3 {
    use super::*;

    #[test]
    fn test_melchior_prompt_is_non_empty() {
        assert!(!melchior_prompt().is_empty());
    }

    #[test]
    fn test_balthasar_prompt_is_non_empty() {
        assert!(!balthasar_prompt().is_empty());
    }

    #[test]
    fn test_caspar_prompt_is_non_empty() {
        assert!(!caspar_prompt().is_empty());
    }

    #[test]
    fn test_three_prompts_are_distinct() {
        assert_ne!(melchior_prompt(), balthasar_prompt());
        assert_ne!(balthasar_prompt(), caspar_prompt());
        assert_ne!(melchior_prompt(), caspar_prompt());
    }

    /// The fixture is generated from the pinned Python reference blobs with
    /// the declared local divergences applied (`DIVERGENCES` in
    /// `tests/fixtures/_magi_ref.py`) — see the fixture header.
    #[test]
    fn test_prompts_match_pinned_reference_sha256() {
        use sha2::{Digest, Sha256};

        let fixture = include_str!("../../tests/fixtures/magi_ref_prompts.sha256");
        let mut expected: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
        for line in fixture.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.splitn(2, "  ").collect();
            assert_eq!(parts.len(), 2, "bad fixture line: {line}");
            expected.insert(parts[1].trim(), parts[0].trim());
        }

        for (filename, content) in [
            ("melchior.md", melchior_prompt()),
            ("balthasar.md", balthasar_prompt()),
            ("caspar.md", caspar_prompt()),
        ] {
            let expected_hash = expected
                .get(filename)
                .unwrap_or_else(|| panic!("no fixture entry for {filename}"));
            let actual_hash = format!("{:x}", Sha256::digest(content.as_bytes()));
            assert_eq!(
                &actual_hash, expected_hash,
                "{filename} content drifted from the pinned reference (see the \
                 fixture header for the documented local divergence)"
            );
        }
    }
}
