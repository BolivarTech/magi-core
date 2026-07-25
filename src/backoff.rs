// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-07-25

//! Pure, total helpers for retry backoff and `Retry-After` parsing. No I/O, no
//! clock, no global state. Every function here is total: never panics, never
//! returns `Err`.

use std::time::Duration;

/// The result of parsing one or more `Retry-After` header values.
///
/// `pub(crate)`: returned by `parse_retry_after`, which is itself `pub(crate)`,
/// so it is never exposed on the public surface (no `#[non_exhaustive]` needed —
/// in-crate the exhaustive `match` is what we want).
// No consumer yet: the retry loop (Task 7) is the first to `match` on this and
// call `parse_retry_after`. `#[allow(dead_code)]` instead of fabricating a fake
// caller (forbidden by CLAUDE.local.md §6.1.8 / spec R8).
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RetryAfter {
    /// No header, or the header explicitly means "use our own formula"
    /// (missing, `0`, or `cap == ZERO`). The caller falls back to the formula.
    Absent,
    /// A valid delta-seconds value we accept (`<= cap`). Honor it.
    Honor(Duration),
    /// A valid value that exceeds `cap`. The caller must abandon retrying.
    TooLong {
        /// What the server requested.
        requested: Duration,
    },
    /// A header was present but could not be interpreted (date form, discordant
    /// merged segments, garbage). The caller must abandon retrying.
    Unintelligible {
        /// The raw value(s) received, for diagnostics.
        raw: String,
    },
}

/// Parse the raw `Retry-After` header value(s) against our acceptance cap.
///
/// This function is total: it never panics and never returns an error.
///
/// # Parameters
///
/// * `values` — the raw header value(s) received for `Retry-After`.
/// * `cap` — the maximum duration we are willing to honor. If zero, the header
///   is ignored entirely.
///
/// # Returns
///
/// A `RetryAfter` variant describing how the caller should proceed: `Absent` to
/// fall back to a formula, `Honor(Duration)` to wait exactly that long, `TooLong`
/// if the server asked for more than `cap`, or `Unintelligible` if a header was
/// present but could not be parsed.
// Wired into the retry loop in Task 7; unused until then (see the enum note).
#[allow(dead_code)]
pub(crate) fn parse_retry_after(values: &[String], cap: Duration) -> RetryAfter {
    if cap.is_zero() {
        return RetryAfter::Absent;
    }
    if values.is_empty() {
        return RetryAfter::Absent;
    }

    for raw in values {
        let t = raw.trim();
        if t.is_empty() {
            continue;
        }

        let parsed = if t.contains(',') {
            let mut segments = t.split(',').map(str::trim);
            let first = match segments.next() {
                Some(f) => f,
                None => continue,
            };
            let first_n = match first.parse::<u64>() {
                Ok(n) => n,
                Err(_) => continue,
            };

            let mut all_equal = true;
            for seg in segments {
                if seg.is_empty() {
                    all_equal = false;
                    break;
                }
                match seg.parse::<u64>() {
                    Ok(n) if n == first_n => {}
                    _ => {
                        all_equal = false;
                        break;
                    }
                }
            }

            if all_equal { Some(first_n) } else { None }
        } else {
            t.parse::<u64>().ok()
        };

        if let Some(n) = parsed {
            if n == 0 {
                return RetryAfter::Absent;
            }

            let requested = Duration::from_secs(n);
            if n > cap.as_secs() {
                return RetryAfter::TooLong { requested };
            }
            return RetryAfter::Honor(requested);
        }
    }

    RetryAfter::Unintelligible {
        raw: values.join(", "),
    }
}

/// Failure class for backoff-policy purposes — the discriminant of
/// [`crate::error::ProviderError`] **without its data**.
///
/// Exists because `ProviderError::Http` carries `status` and `body`: there is no
/// way to write "any `Http`" as a value to configure `RetryConfig::flat_classes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RetryClass {
    /// Client-side timeout.
    Timeout,
    /// Network failure (DNS, connection refused, reset).
    Network,
    /// HTTP response with an error status.
    Http,
    /// Authentication failure.
    Auth,
    /// CLI subprocess failure.
    Process,
    /// Nested session detected.
    NestedSession,
    /// Deliberate abandonment of retrying.
    RetryAbandoned,
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAP: Duration = Duration::from_secs(300);

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_parse_retry_after_decision_table() {
        assert_eq!(parse_retry_after(&[], CAP), RetryAfter::Absent);
        assert_eq!(parse_retry_after(&v(&["0"]), CAP), RetryAfter::Absent);
        // First valid wins, skipping malformed values.
        assert_eq!(
            parse_retry_after(&v(&["banana", "12"]), CAP),
            RetryAfter::Honor(Duration::from_secs(12))
        );
        assert_eq!(
            parse_retry_after(&v(&["12", "30"]), CAP),
            RetryAfter::Honor(Duration::from_secs(12)),
            "first valid wins; the second is not looked at"
        );
        assert_eq!(
            parse_retry_after(&v(&["5", "60"]), CAP),
            RetryAfter::Honor(Duration::from_secs(5)),
            "two valid headers: the first wins"
        );
        assert!(matches!(
            parse_retry_after(&v(&["banana", "fecha"]), CAP),
            RetryAfter::Unintelligible { .. }
        ));
        assert_eq!(
            parse_retry_after(&v(&["12"]), CAP),
            RetryAfter::Honor(Duration::from_secs(12))
        );
        assert_eq!(
            parse_retry_after(&v(&[" 12 "]), CAP),
            RetryAfter::Honor(Duration::from_secs(12))
        );
        assert_eq!(
            parse_retry_after(&v(&["007"]), CAP),
            RetryAfter::Honor(Duration::from_secs(7))
        );
        assert_eq!(
            parse_retry_after(&v(&["600"]), CAP),
            RetryAfter::TooLong {
                requested: Duration::from_secs(600)
            }
        );
        // First VALID wins even when it exceeds the cap; we do NOT fall to the second.
        assert_eq!(
            parse_retry_after(&v(&["600", "60"]), CAP),
            RetryAfter::TooLong {
                requested: Duration::from_secs(600)
            },
            "first valid wins even above cap"
        );
        // Benign proxy merge, concordant segments -> honored (C1.2).
        assert_eq!(
            parse_retry_after(&v(&["12, 12"]), CAP),
            RetryAfter::Honor(Duration::from_secs(12))
        );
        // Discordant merged segments -> we do not guess (C1.1).
        assert!(matches!(
            parse_retry_after(&v(&["12, 30"]), CAP),
            RetryAfter::Unintelligible { .. }
        ));
        // Date form, garbage, negative, empty, overflow -> abort.
        for raw in [
            "Sun, 06 Nov 1994 08:49:37 GMT",
            "banana",
            "-5",
            "",
            "99999999999999999999999",
        ] {
            assert!(
                matches!(
                    parse_retry_after(&v(&[raw]), CAP),
                    RetryAfter::Unintelligible { .. }
                ),
                "raw {raw:?} must be Unintelligible"
            );
        }
    }

    #[test]
    fn test_parse_retry_after_cap_zero_ignores_header() {
        let vals = vec!["12".to_string()];
        assert_eq!(parse_retry_after(&vals, Duration::ZERO), RetryAfter::Absent);
    }

    #[test]
    fn test_parse_retry_after_never_panics() {
        // "18446744073709551616" = u64::MAX + 1: exercises the real u64 overflow
        // path (a text literal "u64::MAX" would only test "non-numeric").
        let overflow = (u64::MAX as u128 + 1).to_string();
        for raw in [
            "",
            " ",
            ",",
            "12,",
            ",12",
            "\0",
            &overflow,
            &"9".repeat(400),
        ] {
            let _ = parse_retry_after(&[raw.to_string()], CAP);
        }
    }
}
