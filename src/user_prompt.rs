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

// Placeholder to keep Cow import used until helpers land in T05-T08.
#[allow(dead_code)]
fn _placeholder_uses_cow() -> Cow<'static, str> {
    Cow::Borrowed("")
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
}
