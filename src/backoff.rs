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
///
/// Derives `Copy` on purpose: it is a **data-less discriminant** (it mirrors
/// `ProviderError`'s variants *without* their data). A future variant that needed
/// to carry data would not belong here — that data lives on `ProviderError`, and
/// `RetryClass` would still only mirror the discriminant.
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
    /// Response body over the buffering cap. Distinct from `Http`: nothing about the status was
    /// wrong, and its condemnation scope is mage-local rather than run-wide.
    ResponseTooLarge,
    /// Failure reported by a provider implemented outside this crate.
    ///
    /// One class for all of them, regardless of the shape the third party declared: the shape
    /// informs retryability, but backoff policy stays configurable per-class by THIS crate's
    /// vocabulary, not by a vocabulary an external crate can extend.
    External,
}

/// Positive jitter added to a honored `Retry-After`, so that several clients
/// receiving the same value do not retry in lockstep.
///
/// Fixed and **not configurable**: 1 s is enough to separate the three mages of
/// a MAGI run and is negligible against waits measured in seconds. If the crate
/// ever coordinated dozens of clients, it would become configurable **then**.
pub const RETRY_AFTER_JITTER: Duration = Duration::from_secs(1);

/// Compute how long to wait before the next attempt.
///
/// # Policy
///
/// - **`retry_after` present** → wins over the formula, plus **positive** jitter
///   bounded by [`RETRY_AFTER_JITTER`]. We never wait **less** than requested.
/// - **Class in `flat_classes`** (`Timeout`, `Network` by default) → **constant**
///   wait of `base`: waiting progressively longer does not fix a network
///   partition or a downed host, it only prolongs the failure. *Do not
///   "simplify" it to exponential: the asymmetry is deliberate.*
/// - **Otherwise** → exponential `base * 2^attempt`, bounded by `cap`.
/// - On the formula (both paths) **full jitter** is applied: `uniform(0, backoff)`.
///
/// # `base = 0` disables TWO protections, not one
///
/// Since the wait is `rand * backoff`, with `base = 0` the product is always
/// zero: the pause **and** the desynchronization both vanish. Several clients
/// failing at once return to the lockstep the jitter exists to avoid. It is
/// legitimate and preserved (a burst **bounded** to `max_retries + 1` requests),
/// but it is an opt-out of both guarantees, not a plain "no wait". The
/// constructor warns about it via `tracing`.
///
/// # Elapsed-time adjustment scope
///
/// This function does **not** discount `now - received_at`; only the `retry_after`
/// path does that, in the retry loop. Formula backoffs are counted from the moment
/// of deciding — there is no external instant to refer to. A deliberate asymmetry.
///
/// # Parameters
/// - `attempt`: **0-based retry index** (0 = first retry after the initial
///   request failed). The initial request does not go through here.
/// - `rand`: randomness source returning `f64` in `[0.0, 1.0]`. If it yields
///   something out of contract it is **sanitized** (see below); never panics.
///
/// # Returns
/// A `Duration` in `[0, cap]` (or `[retry_after, retry_after + JITTER]` when the
/// server is honored). **Never panics, never overflows** (saturating arithmetic).
pub(crate) fn next_backoff(
    attempt: u32,
    class: RetryClass,
    base: Duration,
    cap: Duration,
    flat_classes: &[RetryClass],
    retry_after: Option<Duration>,
    rand: &mut impl FnMut() -> f64,
) -> Duration {
    if let Some(asked) = retry_after {
        let factor = sanitize(rand());
        let wait = asked.saturating_add(RETRY_AFTER_JITTER.mul_f64(factor));
        // RETRY_AFTER_SOFT_CEILING invariant: the final wait may exceed the
        // requested value by at most RETRY_AFTER_JITTER, never more — and never less.
        debug_assert!(
            wait >= asked && wait <= asked.saturating_add(RETRY_AFTER_JITTER),
            "soft ceiling violated: asked={asked:?} wait={wait:?}"
        );
        return wait;
    }
    let raw = if flat_classes.contains(&class) {
        base
    } else {
        base.saturating_mul(2u32.saturating_pow(attempt.min(31)))
    };
    let bounded = raw.min(cap);
    let factor = sanitize(rand());
    // Saturating scale (P1/P4): `Duration::mul_f64` PANICS when `bounded =
    // Duration::MAX` and `factor` is near 1.0 — `from_secs_f64` overflows the
    // `u64` of seconds. Verified empirically. `try_from_secs_f64` returns `Err`
    // at that edge instead of panicking; `unwrap_or(bounded)` falls to the ceiling
    // (already <= cap), and `.min(cap)` closes P3 by construction against any
    // floating-point rounding.
    Duration::try_from_secs_f64(bounded.as_secs_f64() * factor)
        .unwrap_or(bounded)
        .min(cap)
}

/// Sanitize the source value into the range `[0.0, 1.0]`.
///
/// **The order matters and cannot be inverted:** the finitude check runs
/// **before** the `clamp`, because `f64::clamp` **returns `NaN` for `NaN`** —
/// clamping alone is not enough, and a `Duration` scaled by `NaN` is garbage or
/// a panic.
fn sanitize(v: f64) -> f64 {
    if !v.is_finite() {
        return 0.0;
    }
    let clamped = v.clamp(0.0, 1.0);
    // POST-condition (defensive), NOT a pre-condition on `v`: asserting
    // `(0.0..=1.0).contains(&v)` on the RAW value would panic in debug on a finite
    // out-of-range input (1.5, -0.1) — verified empirically — and that would break
    // `test_out_of_contract_source_never_panics` under nextest (which runs in
    // debug), violating P1. Totality is absolute: we assert the sanitized output,
    // which never fails; an out-of-contract input is silently clamped.
    debug_assert!(
        (0.0..=1.0).contains(&clamped),
        "sanitize broken: clamped={clamped}"
    );
    clamped
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

    // -- next_backoff / jitter / sanitize --

    fn fixed(v: f64) -> impl FnMut() -> f64 {
        move || v
    }

    #[test]
    fn test_exponential_growth_is_capped() {
        let mut r = fixed(1.0); // max jitter: the wait is the full backoff
        let cap = Duration::from_secs(60);
        let w = next_backoff(
            10,
            RetryClass::Http,
            Duration::from_secs(1),
            cap,
            &[],
            None,
            &mut r,
        );
        assert_eq!(w, cap, "2^10 s must be bounded to cap");
    }

    #[test]
    fn test_flat_path_does_not_grow() {
        let mut r = fixed(1.0);
        let flat = [RetryClass::Network];
        let base = Duration::from_secs(2);
        let a = next_backoff(
            0,
            RetryClass::Network,
            base,
            Duration::from_secs(60),
            &flat,
            None,
            &mut r,
        );
        let b = next_backoff(
            5,
            RetryClass::Network,
            base,
            Duration::from_secs(60),
            &flat,
            None,
            &mut r,
        );
        assert_eq!(a, b, "the flat path does not grow with attempt");
    }

    #[test]
    fn test_full_jitter_spans_whole_interval() {
        let base = Duration::from_secs(4);
        let cap = Duration::from_secs(60);
        let mut lo = fixed(0.0);
        let mut hi = fixed(1.0);
        let w_lo = next_backoff(0, RetryClass::Http, base, cap, &[], None, &mut lo);
        let w_hi = next_backoff(0, RetryClass::Http, base, cap, &[], None, &mut hi);
        assert_eq!(w_lo, Duration::ZERO);
        assert_eq!(w_hi, base);
    }

    #[test]
    fn test_two_clients_desynchronize_on_exponential_path() {
        // S3: the point of the jitter is NOT the range, it is that two clients
        // failing at once do NOT retry together.
        let (base, cap) = (Duration::from_secs(8), Duration::from_secs(60));
        let mut a = fixed(0.2);
        let mut b = fixed(0.9);
        let wa = next_backoff(1, RetryClass::Http, base, cap, &[], None, &mut a);
        let wb = next_backoff(1, RetryClass::Http, base, cap, &[], None, &mut b);
        assert_ne!(wa, wb, "two clients with distinct sources must separate");
    }

    #[test]
    fn test_two_clients_desynchronize_on_flat_path() {
        // S4: the flat path needs it MORE than the exponential — without growth
        // to separate them, they would retry in lockstep indefinitely.
        let flat = [RetryClass::Network];
        let (base, cap) = (Duration::from_secs(8), Duration::from_secs(60));
        let mut a = fixed(0.2);
        let mut b = fixed(0.9);
        let wa = next_backoff(3, RetryClass::Network, base, cap, &flat, None, &mut a);
        let wb = next_backoff(3, RetryClass::Network, base, cap, &flat, None, &mut b);
        assert_ne!(wa, wb, "the flat path must desynchronize too");
    }

    #[test]
    fn test_retry_after_zero_path_desynchronizes_deterministically() {
        // S14 (intersection C2 + B3 + formula path): a `Retry-After: 0` parses as
        // absent (C2) and `next_backoff` receives `retry_after = None`, falling to
        // the formula WITH full jitter. Two clients with distinct sources diverge.
        let (base, cap) = (Duration::from_secs(1), Duration::from_secs(60));
        let mut a = fixed(0.25);
        let mut b = fixed(0.75);
        let wa = next_backoff(0, RetryClass::Http, base, cap, &[], None, &mut a);
        let wb = next_backoff(0, RetryClass::Http, base, cap, &[], None, &mut b);
        assert_ne!(
            wa, wb,
            "Retry-After: 0 must fall to the jittered formula, not to lockstep"
        );
    }

    #[test]
    fn test_retry_after_wins_and_gets_positive_only_jitter() {
        let asked = Duration::from_secs(12);
        let mut hi = fixed(1.0);
        let w = next_backoff(
            0,
            RetryClass::Http,
            Duration::from_secs(1),
            Duration::from_secs(60),
            &[],
            Some(asked),
            &mut hi,
        );
        assert!(w >= asked, "never before what was requested: {w:?}");
        assert!(w <= asked + RETRY_AFTER_JITTER, "jitter bounded: {w:?}");
    }

    #[test]
    fn test_out_of_contract_source_never_panics_and_stays_in_range() {
        let base = Duration::from_secs(4);
        let cap = Duration::from_secs(60);
        for v in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -0.1,
            1.5,
            f64::MAX,
        ] {
            let mut r = fixed(v);
            let w = next_backoff(0, RetryClass::Http, base, cap, &[], None, &mut r);
            assert!(w <= base, "out of range for v={v}: {w:?}");
        }
    }

    #[test]
    fn test_nan_is_sanitized_before_clamp() {
        // If the clamp ran first, NaN would survive and the wait would be garbage.
        let mut r = fixed(f64::NAN);
        let w = next_backoff(
            0,
            RetryClass::Http,
            Duration::from_secs(4),
            Duration::from_secs(60),
            &[],
            None,
            &mut r,
        );
        assert_eq!(w, Duration::ZERO, "NaN must be treated as 0.0");
    }

    #[test]
    fn test_duration_max_does_not_panic_and_stays_capped() {
        // Explicit guard for the `Duration::mul_f64` panic path: with
        // base=cap=Duration::MAX and rand=1.0 the saturating `try_from_secs_f64`
        // must return a bounded value, never panic (the boundary table covers this
        // in a sweep; this pins it as a dedicated, named regression test).
        let mut r = fixed(1.0);
        let w = next_backoff(
            u32::MAX,
            RetryClass::Http,
            Duration::MAX,
            Duration::MAX,
            &[],
            None,
            &mut r,
        );
        assert!(w <= Duration::MAX);
    }

    #[test]
    fn test_base_zero_yields_zero_wait() {
        let mut r = fixed(1.0);
        let w = next_backoff(
            3,
            RetryClass::Http,
            Duration::ZERO,
            Duration::from_secs(60),
            &[],
            None,
            &mut r,
        );
        assert_eq!(w, Duration::ZERO);
    }

    #[test]
    fn test_boundary_table_never_panics() {
        // §9 requires the COMPLETE table: all four axes, including the out-of-
        // contract `rand` values (B5).
        const RANDS: [f64; 8] = [
            0.0,
            0.5,
            1.0, // in contract: the three edges
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY, // out: non-finite
            -0.1,              // out: finite below (spec §9)
            1.5,               // out: finite above
        ];
        for &rv in &RANDS {
            for attempt in [0u32, 1, 31, 32, u32::MAX] {
                for base in [
                    Duration::ZERO,
                    Duration::from_nanos(1),
                    Duration::from_secs(1),
                    Duration::MAX,
                ] {
                    for cap in [Duration::ZERO, Duration::from_secs(60), Duration::MAX] {
                        let mut r = fixed(rv);
                        // P1: no panic (reaching here proves it).
                        let w =
                            next_backoff(attempt, RetryClass::Http, base, cap, &[], None, &mut r);
                        // P3, no escapes: the ceiling is UNCONDITIONAL.
                        assert!(
                            w <= cap,
                            "P3 broken: rand={rv}, attempt={attempt}, base={base:?}, cap={cap:?} -> {w:?}"
                        );
                        // P6: same source and same args => same result.
                        let mut r2 = fixed(rv);
                        let w2 =
                            next_backoff(attempt, RetryClass::Http, base, cap, &[], None, &mut r2);
                        assert_eq!(w, w2, "P6 broken (determinism)");
                    }
                }
            }
        }
    }

    #[test]
    fn test_boundary_table_honors_retry_after_exactly() {
        // P5, with the same sweep: a honored `retry_after` wins over the formula
        // and only admits the bounded POSITIVE jitter (B2).
        for &rv in &[0.0f64, 0.5, 1.0, f64::NAN] {
            for base in [Duration::ZERO, Duration::from_secs(1), Duration::MAX] {
                let mut r = fixed(rv);
                let asked = Duration::from_secs(12);
                let w = next_backoff(
                    0,
                    RetryClass::Http,
                    base,
                    Duration::from_secs(60),
                    &[],
                    Some(asked),
                    &mut r,
                );
                assert!(w >= asked, "never less than requested: {w:?}");
                assert!(
                    w <= asked + RETRY_AFTER_JITTER,
                    "soft ceiling broken: {w:?}"
                );
            }
        }
    }
}
