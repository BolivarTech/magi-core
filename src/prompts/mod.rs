// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05

pub mod balthasar;
pub mod caspar;
pub mod melchior;

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

// ── Legacy per-mode accessors (deprecated since 0.3.0) ───────────────────────

#[deprecated(since = "0.3.0", note = "use `melchior_prompt()` — mode-agnostic")]
#[doc(hidden)]
/// Returns Melchior's code-review system prompt (legacy per-mode accessor).
pub fn melchior_code_review() -> &'static str {
    melchior::prompt_for_mode(&crate::schema::Mode::CodeReview)
}

#[deprecated(since = "0.3.0", note = "use `melchior_prompt()` — mode-agnostic")]
#[doc(hidden)]
/// Returns Melchior's design system prompt (legacy per-mode accessor).
pub fn melchior_design() -> &'static str {
    melchior::prompt_for_mode(&crate::schema::Mode::Design)
}

#[deprecated(since = "0.3.0", note = "use `melchior_prompt()` — mode-agnostic")]
#[doc(hidden)]
/// Returns Melchior's analysis system prompt (legacy per-mode accessor).
pub fn melchior_analysis() -> &'static str {
    melchior::prompt_for_mode(&crate::schema::Mode::Analysis)
}

#[deprecated(since = "0.3.0", note = "use `balthasar_prompt()` — mode-agnostic")]
#[doc(hidden)]
/// Returns Balthasar's code-review system prompt (legacy per-mode accessor).
pub fn balthasar_code_review() -> &'static str {
    balthasar::prompt_for_mode(&crate::schema::Mode::CodeReview)
}

#[deprecated(since = "0.3.0", note = "use `balthasar_prompt()` — mode-agnostic")]
#[doc(hidden)]
/// Returns Balthasar's design system prompt (legacy per-mode accessor).
pub fn balthasar_design() -> &'static str {
    balthasar::prompt_for_mode(&crate::schema::Mode::Design)
}

#[deprecated(since = "0.3.0", note = "use `balthasar_prompt()` — mode-agnostic")]
#[doc(hidden)]
/// Returns Balthasar's analysis system prompt (legacy per-mode accessor).
pub fn balthasar_analysis() -> &'static str {
    balthasar::prompt_for_mode(&crate::schema::Mode::Analysis)
}

#[deprecated(since = "0.3.0", note = "use `caspar_prompt()` — mode-agnostic")]
#[doc(hidden)]
/// Returns Caspar's code-review system prompt (legacy per-mode accessor).
pub fn caspar_code_review() -> &'static str {
    caspar::prompt_for_mode(&crate::schema::Mode::CodeReview)
}

#[deprecated(since = "0.3.0", note = "use `caspar_prompt()` — mode-agnostic")]
#[doc(hidden)]
/// Returns Caspar's design system prompt (legacy per-mode accessor).
pub fn caspar_design() -> &'static str {
    caspar::prompt_for_mode(&crate::schema::Mode::Design)
}

#[deprecated(since = "0.3.0", note = "use `caspar_prompt()` — mode-agnostic")]
#[doc(hidden)]
/// Returns Caspar's analysis system prompt (legacy per-mode accessor).
pub fn caspar_analysis() -> &'static str {
    caspar::prompt_for_mode(&crate::schema::Mode::Analysis)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

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

    #[test]
    fn test_prompts_match_python_reference_sha256() {
        use sha2::{Digest, Sha256};

        let fixture = include_str!("../../tests/fixtures/magi_ref_prompts.sha256");
        let mut expected: std::collections::HashMap<&str, &str> =
            std::collections::HashMap::new();
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
                "{filename} content drifted from Python reference"
            );
        }
    }
}
