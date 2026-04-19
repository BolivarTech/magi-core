// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-18

//! User prompt construction with defense-in-depth against injection.
//!
//! Single entry point `build_user_prompt` sanitizes `content`, generates
//! a per-request nonce, and wraps the result in `---BEGIN USER CONTEXT
//! <nonce>---` / `---END USER CONTEXT <nonce>---` delimiters.
//!
//! See `sbtdd/spec-behavior.md` §5 and
//! `docs/adr/001-prompt-injection-threat-model.md` for threat model and
//! algorithmic specification.

use std::borrow::Cow;
use std::sync::LazyLock;

use regex::Regex;

use crate::validate::INVISIBLE_AND_SEPARATOR_RE;

/// Compiled regex matching all Unicode line separators except `\n`.
///
/// `\r\n` is listed before `\r` so the CRLF pair is consumed as a unit
/// (leftmost-first alternation in the regex engine).
static NEWLINE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new("\r\n|\r|\u{000B}|\u{000C}|\u{0085}|\u{2028}|\u{2029}")
        .expect("NEWLINE_RE is a valid regex")
});

/// Converts all Unicode line separators to `\n`.
///
/// Recognized separators: `\r\n`, `\r`, U+000B (VT), U+000C (FF),
/// U+0085 (NEL), U+2028 (LS), U+2029 (PS). Returns `Cow::Borrowed`
/// when no non-LF separator is present (no allocation needed).
///
/// # Arguments
///
/// * `s` — Input string slice to normalize.
///
/// # Returns
///
/// `Cow<'_, str>` — borrowed if unchanged, owned if any separator was replaced.
#[allow(dead_code)]
fn normalize_newlines(s: &str) -> Cow<'_, str> {
    NEWLINE_RE.replace_all(s, "\n")
}

/// Removes invisible and Unicode separator characters from `s`.
///
/// Delegates to [`crate::validate::INVISIBLE_AND_SEPARATOR_RE`], which covers:
/// zero-width spaces, bidi marks, line/paragraph separators
/// (U+2028..U+202F), word joiner and related formatting controls
/// (U+2060..U+206F), BOM (U+FEFF), and soft hyphen (U+00AD).
///
/// Returns `Cow::Borrowed` when no invisible characters are present
/// (no allocation). Returns `Cow::Owned` when at least one character
/// is removed.
///
/// # Arguments
///
/// * `s` — Input string slice to sanitize.
///
/// # Returns
///
/// `Cow<'_, str>` — borrowed if unchanged, owned if any character was removed.
#[allow(dead_code)]
fn strip_invisibles(s: &str) -> Cow<'_, str> {
    INVISIBLE_AND_SEPARATOR_RE.replace_all(s, "")
}

/// Abstraction over a `u128` random-number source.
///
/// `Send` is required so `Box<dyn RngLike + Send>` can cross threads via
/// the MagiBuilder `with_rng_source` API.
pub(crate) trait RngLike: Send {
    fn next_u128(&mut self) -> u128;
}

pub(crate) struct FastrandSource;

impl RngLike for FastrandSource {
    fn next_u128(&mut self) -> u128 {
        fastrand::u128(..)
    }
}

#[cfg(test)]
pub(crate) struct FixedRng {
    values: std::collections::VecDeque<u128>,
}

#[cfg(test)]
impl FixedRng {
    /// Creates a `FixedRng` that yields `values` in submission order (FIFO).
    pub(crate) fn new(values: Vec<u128>) -> Self {
        Self {
            values: values.into(),
        }
    }
}

#[cfg(test)]
impl RngLike for FixedRng {
    fn next_u128(&mut self) -> u128 {
        self.values.pop_front().expect("FixedRng exhausted")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fastrand_source_returns_distinct_values_across_calls() {
        let mut rng = FastrandSource;
        let a = rng.next_u128();
        let b = rng.next_u128();
        let c = rng.next_u128();
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert_ne!(a, c);
    }

    #[test]
    fn test_fixed_rng_returns_values_in_submission_order_fifo() {
        let mut rng = FixedRng::new(vec![0x1, 0x2, 0x3]);
        assert_eq!(rng.next_u128(), 0x1);
        assert_eq!(rng.next_u128(), 0x2);
        assert_eq!(rng.next_u128(), 0x3);
    }

    #[test]
    #[should_panic(expected = "FixedRng exhausted")]
    fn test_fixed_rng_panics_when_exhausted() {
        let mut rng = FixedRng::new(vec![0x1]);
        rng.next_u128();
        rng.next_u128();
    }

    // --- normalize_newlines tests ---

    #[test]
    fn test_normalize_newlines_collapses_crlf_pair_to_lf() {
        assert_eq!(normalize_newlines("a\r\nb"), "a\nb");
    }

    #[test]
    fn test_normalize_newlines_converts_lone_cr_to_lf() {
        assert_eq!(normalize_newlines("a\rb"), "a\nb");
    }

    #[test]
    fn test_normalize_newlines_converts_vertical_tab_to_lf() {
        assert_eq!(normalize_newlines("a\u{000B}b"), "a\nb");
    }

    #[test]
    fn test_normalize_newlines_converts_form_feed_to_lf() {
        assert_eq!(normalize_newlines("a\u{000C}b"), "a\nb");
    }

    #[test]
    fn test_normalize_newlines_converts_nel_to_lf() {
        assert_eq!(normalize_newlines("a\u{0085}b"), "a\nb");
    }

    #[test]
    fn test_normalize_newlines_converts_line_separator_to_lf() {
        assert_eq!(normalize_newlines("a\u{2028}b"), "a\nb");
    }

    #[test]
    fn test_normalize_newlines_converts_paragraph_separator_to_lf() {
        assert_eq!(normalize_newlines("a\u{2029}b"), "a\nb");
    }

    #[test]
    fn test_normalize_newlines_preserves_existing_lf_borrows() {
        let out = normalize_newlines("a\nb");
        assert_eq!(out, "a\nb");
        assert!(matches!(out, Cow::Borrowed(_)), "no-op case should borrow");
    }

    #[test]
    fn test_normalize_newlines_handles_mixed_separators() {
        assert_eq!(
            normalize_newlines("one\r\ntwo\rthree\u{2028}four\u{0085}five\nsix"),
            "one\ntwo\nthree\nfour\nfive\nsix"
        );
    }

    #[test]
    fn test_normalize_newlines_handles_empty_string() {
        let out = normalize_newlines("");
        assert_eq!(out, "");
        assert!(matches!(out, Cow::Borrowed(_)));
    }

    // --- strip_invisibles tests ---

    #[test]
    fn test_strip_invisibles_removes_zwsp() {
        assert_eq!(strip_invisibles("a\u{200b}b"), "ab");
    }

    #[test]
    fn test_strip_invisibles_removes_bom() {
        assert_eq!(strip_invisibles("a\u{feff}b"), "ab");
    }

    #[test]
    fn test_strip_invisibles_removes_bidi_marks() {
        assert_eq!(strip_invisibles("a\u{200e}b\u{202d}c"), "abc");
    }

    #[test]
    fn test_strip_invisibles_removes_soft_hyphen() {
        assert_eq!(strip_invisibles("a\u{00ad}b"), "ab");
    }

    #[test]
    fn test_strip_invisibles_preserves_regular_text() {
        let out = strip_invisibles("hello world");
        assert_eq!(out, "hello world");
        assert!(matches!(out, Cow::Borrowed(_)), "no-op case should borrow");
    }

    #[test]
    fn test_strip_invisibles_preserves_ascii_whitespace() {
        assert_eq!(strip_invisibles("a b\tc\nd"), "a b\tc\nd");
    }

    #[test]
    fn test_strip_invisibles_handles_word_joiner_range() {
        // U+2060 is in the U+2060-U+206F range and should be stripped.
        assert_eq!(strip_invisibles("a\u{2060}b"), "ab");
    }
}
