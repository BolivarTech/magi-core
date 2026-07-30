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

/// Fingerprint of the worked example every shipped prompt carries: its `summary`.
///
/// The three prompts share this value **verbatim**; they differ only in `reasoning` and
/// in the finding `detail`. Together with [`ECHO_CANARY_RECOMMENDATION`] it identifies an
/// output that is the *example* rather than an analysis.
///
/// # ⚠ Requires BOTH values to match
///
/// With one alone the false positive stops being theoretical. Even with both, a genuine
/// verdict whose `summary` is exactly `"One-line verdict"` **and** whose `recommendation`
/// is exactly `"What you recommend"` would be rejected — accepted, because it demands
/// simultaneous exact equality on two free-prose fields and the cost is **one retry**,
/// not a lost verdict. The direction of failure is the safe one.
///
/// # This is the SECOND line of defence, and it does not cover custom prompts
///
/// The fingerprint is of *our* example. A consumer's custom prompt has its own, and if
/// their model echoes it this check will not catch it. What protects them is structural:
/// the guard rejects any prompt whose delimited block is fabricable, and the empty-slot
/// pattern the migration guide prescribes puts their example outside the markers. Made
/// explicit rather than extended, because a canary that only half covers gives a feeling
/// of coverage where there is none.
pub(crate) const ECHO_CANARY_SUMMARY: &str = "One-line verdict";

/// Fingerprint of the worked example every shipped prompt carries: its `recommendation`.
/// See [`ECHO_CANARY_SUMMARY`] — both must match for the canary to fire.
pub(crate) const ECHO_CANARY_RECOMMENDATION: &str = "What you recommend";

/// Checks that `prompt` satisfies the verdict-marker contract.
///
/// This is the **same function** `MagiBuilder::build()` runs, so what it accepts is
/// exactly what `build()` accepts. Call it from your own test suite before deploying a
/// custom prompt, instead of discovering the problem when `build()` returns `Err`.
///
/// # What it checks
///
/// 1. Exactly **one** `<MAGI_VERDICT>` line and **one** `</MAGI_VERDICT>` line, in that
///    order, judged with the **strict** predicate: in a file you ship, an invisible
///    character inside a marker line is corruption, not tolerance owed.
/// 2. **Nothing fabricable between them** — the delimited block must NOT deserialize as
///    a complete verdict. A prompt whose marker block is a valid 7-key object is a
///    *fabrication template*: an agent that echoes its own instructions emits a clean
///    verdict nobody formed. Put the worked example **outside** the markers and leave a
///    non-JSON placeholder inside, the way the built-in prompts do.
///
/// The block is obtained through the **same** `locate_block` the parser uses — same line
/// splitter, same fence stripping — so a fence cannot hide a fabricable object from this
/// check while the parser would still accept it.
///
/// A leading BOM is tolerated: it is an artefact of the file's **encoding**, not a
/// property of a line, so it is resolved here — before anything is compared. Resolving
/// it in the comparator is the mistake the reference implementation made (a false FATAL
/// aborted a run) before moving it to the encoding layer.
///
/// **Exactly one** leading BOM is stripped, not a run of them. One is what an encoder
/// emits; a second is corruption, and corruption in a file we ship should be **visible**.
/// The direction is safe either way: the extra BOM stays on the first marker line, the
/// strict predicate rejects it, and `build()` fails loudly with the offending prompt
/// named — never silently accepting a prompt whose bytes we could not account for.
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
    validate_prompt_for(None, None, prompt)
}

/// Same check as [`validate_prompt`], for a caller that **knows the seat**.
///
/// This is the form `MagiBuilder::build()` uses, so its error names the agent (and the
/// mode, for a per-mode override) and a reader knows which of the three files to open.
/// Consumers can use it too when they already know which mage a prompt is for.
///
/// # Errors
///
/// [`MagiError::PromptContract`] identifying the prompt, the rule violated, and how to
/// check it before deploying.
pub fn validate_prompt_for(
    agent: Option<AgentName>,
    mode: Option<Mode>,
    prompt: &str,
) -> Result<(), MagiError> {
    // The BOM is an artefact of the FILE's encoding, not a property of a line, so it is
    // resolved HERE — before anything is compared. Resolving it in the comparator is the
    // mistake the reference made (a false FATAL aborted a run) before moving it to the
    // encoding layer.
    let body = prompt.strip_prefix('\u{feff}').unwrap_or(prompt);

    let contract = |reason: String| MagiError::PromptContract {
        agent,
        mode,
        reason,
    };

    let block = locate_block(body, is_exact_marker_line).map_err(|e| {
        contract(format!(
            "{e}. Start from prompts::caspar_prompt() (or copy its `## Output format` \
             section verbatim) and check with prompts::validate_prompt before deploying"
        ))
    })?;

    // NOTHING FABRICABLE between the markers. The block is obtained from the SAME
    // `locate_block` the parser uses — same line splitter, same fence stripping — so the
    // guard sees exactly what the parser would see. With two separate implementations, a
    // 7-key object hidden inside a fence passed the guard while the parser, which does
    // strip fences, would have accepted it: a fabrication vector inside the guard meant
    // to close it.
    if serde_json::from_str::<crate::schema::AgentOutput>(block).is_ok() {
        return Err(contract(
            "the block between the verdict markers deserializes as a complete verdict, \
             so an agent echoing its instructions would fabricate one. Put the worked \
             example OUTSIDE the markers and leave a non-JSON placeholder inside, as \
             prompts::caspar_prompt() does"
                .to_string(),
        ));
    }
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests_verdict_contract {
    use super::*;
    use crate::verdict_markers::{VERDICT_CLOSE, VERDICT_OPEN};

    /// The re-pin's own witness (MS3 T4). Until the re-pin, this test asserted the
    /// INVERSE — that the prompts did NOT satisfy the contract, because they carried no
    /// markers. Flipping it is how the re-pin **proves** it installed the contract,
    /// instead of asserting it in prose.
    ///
    /// It also subsumes "exactly one ordered strict pair": `locate_block` with the strict
    /// predicate succeeds only on exactly one ordered pair, so a second assertion for
    /// that would be the same check under another name.
    ///
    /// Note this uses the STRICT predicate, so it doubles as E21 over our own files: an
    /// invisible inside a marker line here is corruption and fails, even though the same
    /// line in model output would be accepted.
    #[test]
    fn test_shipped_prompts_satisfy_the_verdict_marker_contract() {
        for (name, p) in [
            ("melchior.md", melchior_prompt()),
            ("balthasar.md", balthasar_prompt()),
            ("caspar.md", caspar_prompt()),
        ] {
            validate_prompt(p)
                .unwrap_or_else(|e| panic!("{name}: re-pin must install the contract: {e}"));
        }
    }

    /// MANDATORY ANCHOR (R13). Without it, editing a prompt without updating the canary
    /// leaves the canary comparing against text nobody emits: a **silent fail-open**.
    ///
    /// Now asserted against the **production constants**, so it proves two things at
    /// once: that the shipped prompts contain the fingerprint, and that the constants and
    /// the prompts have not drifted apart. Either half alone leaves the canary able to
    /// compare against text nobody emits.
    #[test]
    fn test_echo_canary_values_are_present_in_every_shipped_prompt() {
        for (name, p) in [
            ("melchior.md", melchior_prompt()),
            ("balthasar.md", balthasar_prompt()),
            ("caspar.md", caspar_prompt()),
        ] {
            assert!(
                p.contains(ECHO_CANARY_SUMMARY),
                "{name}: canary summary value absent"
            );
            assert!(
                p.contains(ECHO_CANARY_RECOMMENDATION),
                "{name}: canary recommendation value absent"
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

    /// The complete 7-key object — a prompt with this between its markers is a
    /// fabrication template: an agent echoing its instructions emits a clean verdict.
    fn fabricable_object() -> &'static str {
        r#"{"agent":"caspar","verdict":"approve","confidence":0.9,"summary":"s",
           "reasoning":"r","findings":[],"recommendation":"rec"}"#
    }

    #[test]
    fn test_validate_prompt_rejects_a_fabricable_block() {
        // E19
        let p = format!("{VERDICT_OPEN}\n{}\n{VERDICT_CLOSE}", fabricable_object());
        assert!(matches!(
            validate_prompt(&p),
            Err(MagiError::PromptContract { .. })
        ));
    }

    #[test]
    fn test_validate_prompt_rejects_a_fabricable_block_inside_a_fence() {
        // E19b — THE EVASION VECTOR the gate found. The guard applies the SAME
        // `strip_fence` as the parser, so a fence cannot hide a fabricable object. With
        // two separate implementations this prompt PASSED the guard (serde choked on the
        // fence) while the parser, which strips fences, would have accepted it.
        let p = format!(
            "{VERDICT_OPEN}\n```json\n{}\n```\n{VERDICT_CLOSE}",
            fabricable_object()
        );
        assert!(
            matches!(validate_prompt(&p), Err(MagiError::PromptContract { .. })),
            "a fence must not hide a fabricable object from the guard"
        );
    }

    #[test]
    fn test_validate_prompt_counts_markers_in_a_cr_only_prompt() {
        // E19c — inherits the R5 splitter via `locate_block`; `str::lines()` would count
        // zero markers here and wave the prompt through.
        let p = format!("intro\r{VERDICT_OPEN}\r{{ ...slot... }}\r{VERDICT_CLOSE}\rfin");
        validate_prompt(&p).expect("a CR-only prompt must validate");
    }

    #[test]
    fn test_unassigned_validation_does_not_name_a_mage() {
        // The debt T1 left behind, killed. `validate_prompt(&str)` cannot know the seat,
        // so its error must not claim one: a consumer checking their Caspar prompt used
        // to read "Melchior". An actively misleading error is worse than an honest
        // "unassigned" (E20b).
        let err = validate_prompt("no markers here").unwrap_err();
        let msg = err.to_string();
        assert!(matches!(err, MagiError::PromptContract { agent: None, .. }));
        for mage in ["Melchior", "Balthasar", "Caspar"] {
            assert!(
                !msg.contains(mage),
                "must not claim a seat it does not know: {msg}"
            );
        }
        assert!(msg.contains("unassigned"), "{msg}");
    }

    #[test]
    fn test_seat_aware_validation_names_the_agent_and_mode() {
        // The path `build()` uses: the seat IS known, so the message says which of the
        // three files to open (E20b).
        let err =
            validate_prompt_for(Some(AgentName::Caspar), Some(Mode::Design), "nope").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Caspar"), "{msg}");
        assert!(msg.contains("Design"), "{msg}");
    }

    #[test]
    fn test_the_error_is_actionable_on_its_own() {
        // E20b — a consumer must be able to fix it without opening documentation: which
        // rule broke, and how to check before deploying.
        let msg = validate_prompt("legacy prompt").unwrap_err().to_string();
        assert!(msg.contains("no verdict markers found"), "the rule: {msg}");
        assert!(msg.contains("validate_prompt"), "how to check: {msg}");
        assert!(msg.contains("caspar_prompt"), "where to start: {msg}");
    }

    #[test]
    fn test_guard_and_parser_agree_on_the_same_block() {
        // E19d — the guard is a FAITHFUL simulation of the parser. Whatever the strict
        // path accepts, the permissive path must see the SAME content; the only
        // admissible difference is that strict REJECTS invisibles.
        let fenced = format!("{VERDICT_OPEN}\n```json\n{{\"a\":1}}\n```\n{VERDICT_CLOSE}");
        assert_eq!(
            crate::verdict_markers::extract(&fenced).unwrap(),
            "{\"a\":1}",
            "the parser strips the fence"
        );
        // And the guard, seeing that same stripped content, judges it non-fabricable.
        validate_prompt(&fenced).expect("`{\"a\":1}` is not a 7-key verdict");
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
