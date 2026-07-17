// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05

use std::collections::BTreeMap;

use crate::schema::{AgentName, Mode};

// ── Mode-agnostic accessors (v0.3.0) ─────────────────────────────────────────

/// Returns the consolidated, mode-agnostic system prompt for Melchior (Scientist).
///
/// This prompt is loaded at compile time from `prompts_md/melchior.md` and is
/// used by [`crate::agent::Agent`] when no custom prompt is configured.
///
/// # Example
/// ```
/// let prompt = magi_core::prompts::melchior_prompt();
/// assert!(!prompt.is_empty());
/// ```
pub fn melchior_prompt() -> &'static str {
    include_str!("../prompts_md/melchior.md")
}

/// Returns the consolidated, mode-agnostic system prompt for Balthasar (Pragmatist).
///
/// This prompt is loaded at compile time from `prompts_md/balthasar.md` and is
/// used by [`crate::agent::Agent`] when no custom prompt is configured.
///
/// # Example
/// ```
/// let prompt = magi_core::prompts::balthasar_prompt();
/// assert!(!prompt.is_empty());
/// ```
pub fn balthasar_prompt() -> &'static str {
    include_str!("../prompts_md/balthasar.md")
}

/// Returns the consolidated, mode-agnostic system prompt for Caspar (Critic).
///
/// This prompt is loaded at compile time from `prompts_md/caspar.md` and is
/// used by [`crate::agent::Agent`] when no custom prompt is configured.
///
/// # Example
/// ```
/// let prompt = magi_core::prompts::caspar_prompt();
/// assert!(!prompt.is_empty());
/// ```
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

// ── Tests ─────────────────────────────────────────────────────────────────────

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
