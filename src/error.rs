// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05

use std::time::{Duration, Instant};
use thiserror::Error;

/// The reason `RetryProvider` deliberately stopped retrying.
///
/// Exists so that giving up is **not a silent decision**: the consumer can
/// distinguish, by `match`, an abandonment of our own from a server error.
///
/// # Stability
///
/// `#[non_exhaustive]` on the enum **and on its struct-like variants**, in
/// symmetry with [`ProviderError`] (A1/A2): the enum-level attribute only
/// enables adding new variants; the per-variant attribute enables adding new
/// **fields** (e.g. a field on `RetryAfterTooLong`) without breaking. Consumers
/// use `_ => ...` when matching and `..` when destructuring.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AbandonReason {
    /// The server asked to wait longer than we accept (`retry_after_cap`).
    #[non_exhaustive]
    RetryAfterTooLong {
        /// What the server requested.
        requested: Duration,
        /// Our limit.
        cap: Duration,
    },
    /// A `Retry-After` was present but could not be interpreted (date form,
    /// discordant segments, garbage). We do not guess: we give up.
    #[non_exhaustive]
    RetryAfterUnintelligible {
        /// The raw value received, for diagnostics. When an intermediary merged
        /// repeated headers, this may be a comma-joined string.
        raw: String,
    },
    /// `operation_budget` was exhausted before `max_retries` was consumed.
    #[non_exhaustive]
    OperationBudgetExhausted {
        /// Time elapsed since the first attempt.
        elapsed: Duration,
        /// The configured budget.
        budget: Duration,
    },
}

/// Errors originating from LLM provider implementations.
///
/// Each variant represents a distinct failure mode that providers
/// can encounter when communicating with LLM backends.
///
/// # Stability
///
/// `#[non_exhaustive]` on the enum **and on its struct-like variants**:
/// consumers must include a `_ => ...` arm when matching, and use `..` when
/// destructuring a variant. This allows adding variants and fields in future
/// minor releases without breaking.
#[derive(Debug, Clone, Error)]
#[non_exhaustive]
pub enum ProviderError {
    /// HTTP response with a non-success status code.
    #[error("http error {status}: {body}")]
    #[non_exhaustive]
    Http {
        /// HTTP status code.
        status: u16,
        /// Response body text.
        body: String,
        /// **Raw** values of the `Retry-After` header, in order of arrival.
        /// Empty means the server did not send it. It is a `Vec` because HTTP
        /// allows the header to be **repeated** and the HTTP spec requires
        /// keeping the **first valid** one, skipping malformed ones — a single
        /// `String` could not represent that. Interpreting them is
        /// `RetryProvider`'s job, since only it knows the configured cap.
        retry_after_raw: Vec<String>,
        /// Instant at which the response **headers** were received. This is the
        /// epoch against which a honored `retry_after` is discounted.
        received_at: Option<Instant>,
    },

    /// Network-level failure (DNS, connection refused, etc.).
    #[error("network error: {message}")]
    #[non_exhaustive]
    Network {
        /// Description of the network failure.
        message: String,
    },

    /// Provider did not respond within the allowed time.
    #[error("timeout: {message}")]
    #[non_exhaustive]
    Timeout {
        /// Description of the timeout condition.
        message: String,
    },

    /// Authentication or authorization failure.
    #[error("auth error: {message}")]
    #[non_exhaustive]
    Auth {
        /// Description of the authentication failure.
        message: String,
    },

    /// CLI subprocess provider failed.
    #[error("process error (exit_code={exit_code:?}): {stderr}")]
    #[non_exhaustive]
    Process {
        /// Exit code of the child process, if available.
        exit_code: Option<i32>,
        /// Standard error output from the child process.
        stderr: String,
    },

    /// Detected nested session (e.g., `CLAUDECODE` env var present).
    #[error("nested session detected: cannot launch CLI provider from within an existing session")]
    NestedSession,

    /// Response body exceeded the size this crate is willing to buffer.
    ///
    /// **Not a transport failure**: the server answered fine, it answered *too much*. Condemnation
    /// is mage-local and it never counts toward the endpoint-down latch — a content failure does
    /// not license run-wide consequences.
    ///
    /// # Remediation depends on the cause
    ///
    /// - The **endpoint** returns oversized junk regardless of input → lower `max_tokens`, or drop
    ///   that endpoint from the pool.
    /// - The **input** legitimately demands a long answer → **raise** `max_tokens` (which raises
    ///   this cap, since it is derived) or split the input. Lowering it here makes things worse.
    #[error("response body exceeded {limit} bytes")]
    #[non_exhaustive]
    ResponseTooLarge {
        /// The cap that was exceeded, so a reader never has to parse the message.
        limit: usize,
    },

    /// `RetryProvider` stopped retrying by its own decision.
    #[error("retry abandoned after {attempts} attempt(s): {reason:?}")]
    #[non_exhaustive]
    RetryAbandoned {
        /// Why it was abandoned.
        reason: AbandonReason,
        /// Attempts made before abandoning.
        attempts: u32,
    },

    /// Failure reported by an [`LlmProvider`] implemented OUTSIDE this crate.
    ///
    /// [`LlmProvider`]: crate::provider::LlmProvider
    ///
    /// Build it with [`ProviderError::external`] — the only constructor reachable from another
    /// crate, and deliberately the only one.
    ///
    /// # The message is third-party text, and this crate cannot redact it
    ///
    /// It travels into the report like any other error message, but this crate does not author it
    /// and **cannot** clean it: recognising a secret inside arbitrary prose is not something a
    /// library can do. **Do not put credentials in it.** The cap below limits the blast radius; it
    /// does not prevent a leak.
    ///
    /// # It names a shape; the core decides the consequences
    ///
    /// An external crate says *what kind* of failure happened. Whether that is retried, and
    /// whether it condemns a lineage, stays here — see [`ExternalErrorKind`].
    #[error("external provider error ({kind:?}): {message}")]
    #[non_exhaustive]
    External {
        /// Third-party diagnostic text, capped and marked when cut.
        message: String,
        /// The SHAPE of the failure.
        kind: ExternalErrorKind,
    },
}

/// Marker appended when text is cut, so a truncated message never reads as a complete one.
///
/// Lives here rather than beside the retry machinery because this is the foundation layer: the
/// error type must not import from the layers above it, and both of the crate's truncation sites
/// need this constant. It sat in `provider.rs` briefly and made `error.rs` depend upwards.
pub(crate) const TRUNCATION_MARKER: &str = " … (truncated)";

/// Upper bound, in bytes, for the text an external provider may attach to a failure.
pub const MAX_EXTERNAL_MESSAGE_BYTES: usize = 400;

/// How an external provider's failure should be treated.
///
/// Deliberately **coarser** than [`ProviderError`]: a third party names the shape of its failure,
/// and this crate keeps ownership of retry classification and lineage condemnation. That split is
/// the whole design — it is why an external provider can fail in a typed way without acquiring the
/// ability to decide what happens next.
///
/// # There is no `Schema` variant, on purpose
///
/// A provider returns a `String`; it does not validate verdicts. Schema failures belong to this
/// crate's own path, and letting a third party claim them would misattribute the adherence
/// telemetry that exists to answer *which model stops following the contract*.
///
/// # When all three seats share one external backend
///
/// Each seat condemns the lineage **locally** and rotates on its own; the endpoint-down latch
/// never fires, because a third-party backend's failure says nothing this crate can verify about
/// the lineages the *other* seats are using. The waste is **bounded** — by each mage's rotation
/// cap and by the timeouts you configured — so the run finishes and degrades honestly rather than
/// hanging.
///
/// The worse case is not a clean outage but an **intermittent** backend: each seat rotates, finds
/// it healthy again, and drains budget in small steps without ever reaching a fast fail. If your
/// seats share a backend, the lineage diversity that rotation promises does not exist — and that
/// is a configuration decision this crate cannot infer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExternalErrorKind {
    /// Connection-level failure reaching the backend.
    Network,
    /// The backend did not answer in time.
    Timeout,
    /// Rejected credentials or insufficient permissions.
    Auth,
    /// Rate limited. Retryable, but the scheduling stays here: this carries no `Retry-After`.
    RateLimit,
    /// The backend failed on its own side — the 5xx shape.
    ServerError,
    /// Anything else. **Not** retryable: an escape hatch must not silently buy retries.
    Other,
}

impl ProviderError {
    /// Builds a failure from an [`LlmProvider`] implemented outside this crate.
    ///
    /// [`LlmProvider`]: crate::provider::LlmProvider
    ///
    /// # Parameters
    /// - `message`: third-party diagnostic text. Truncated to [`MAX_EXTERNAL_MESSAGE_BYTES`] with
    ///   a visible marker. **Must not contain credentials** — see [`ProviderError::External`].
    /// - `kind`: the shape of the failure. It does **not** decide the consequences.
    ///
    /// # Returns
    /// A [`ProviderError::External`]. Never fails, never panics — including on multi-byte input,
    /// where the cut lands on a character boundary.
    ///
    /// # Why this exists at all
    ///
    /// Every variant of this enum is `#[non_exhaustive]`, so none can be built with a struct
    /// expression from another crate. Without a constructor an external provider could *compile*
    /// but could not **fail in a typed way** — which pushed implementors toward lying with an
    /// unrelated variant or panicking. `#[non_exhaustive]` and this constructor are a pair;
    /// either alone is broken.
    ///
    /// # Examples
    ///
    /// ```
    /// use magi_core::error::{ExternalErrorKind, ProviderError};
    ///
    /// let err = ProviderError::external("backend unreachable", ExternalErrorKind::Network);
    /// assert!(err.to_string().contains("backend unreachable"));
    /// ```
    pub fn external(message: impl Into<String>, kind: ExternalErrorKind) -> Self {
        let raw: String = message.into();
        let message = if raw.len() <= MAX_EXTERNAL_MESSAGE_BYTES {
            raw
        } else {
            // The marker is paid for INSIDE the budget. A cap that its own suffix can push past
            // is a cap that lies about its name — and this one is quoted in the rustdoc as a
            // bound, so it has to hold literally.
            let budget = MAX_EXTERNAL_MESSAGE_BYTES.saturating_sub(TRUNCATION_MARKER.len());
            // `floor_char_boundary`, never a raw slice: the budget lands mid-character for any
            // multi-byte text, and slicing there panics.
            let cut = raw.floor_char_boundary(budget);
            format!("{}{TRUNCATION_MARKER}", &raw[..cut])
        };
        Self::External { message, kind }
    }
}

/// Unified error type for the magi-core crate.
///
/// All public APIs return `Result<T, MagiError>`. This enum unifies
/// provider errors, validation failures, and I/O errors into a single type.
///
/// Marked `#[non_exhaustive]` (added v0.5.0) so future variants can be
/// introduced in minor releases without breaking downstream exhaustive
/// matchers. Consumers MUST include a `_ => ...` arm when matching.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum MagiError {
    /// Invalid input or schema violation.
    #[error("validation error: {0}")]
    Validation(String),

    /// Wraps a provider-specific error.
    #[error(transparent)]
    Provider(#[from] ProviderError),

    /// Fewer agents completed successfully than the minimum threshold.
    #[error("insufficient agents: {succeeded} succeeded, {required} required")]
    InsufficientAgents {
        /// Number of agents that completed successfully.
        succeeded: usize,
        /// Minimum number of agents required.
        required: usize,
    },

    /// JSON deserialization failure.
    #[error("deserialization error: {0}")]
    Deserialization(String),

    /// Content exceeds configured maximum input size.
    #[error("input too large: {size} bytes exceeds maximum of {max} bytes")]
    InputTooLarge {
        /// Actual size of the input in bytes.
        size: usize,
        /// Maximum allowed size in bytes.
        max: usize,
    },

    /// Input rejected by invariant check (e.g., prompt nonce collision).
    #[error("invalid input: {reason}")]
    InvalidInput {
        /// Description of the invariant violation.
        reason: String,
    },

    /// **v0.5.0** — Input did not pass the caller-supplied complexity gate.
    /// No LLM dispatch occurred. The caller's predicate is `bool`-returning;
    /// the `reason` string in this variant is **library-synthesized**, not
    /// caller-supplied — see field doc below for the format.
    ///
    /// The variant itself is `#[non_exhaustive]` so future fields (e.g.,
    /// structured `content_len: usize`, `mode: Mode`) can be added without
    /// breaking match patterns. Match using `..` rest pattern.
    ///
    /// See [`MagiBuilder::with_complexity_gate`](crate::orchestrator::MagiBuilder::with_complexity_gate).
    #[error("skipped by complexity gate: {reason}")]
    #[non_exhaustive]
    SkippedByComplexityGate {
        /// Library-synthesized description of the skip, currently in the
        /// format `"complexity gate rejected: mode={mode}, content_len={N}"`
        /// where `content_len` is the byte length (not UTF-8 char count).
        ///
        /// **The exact format is NOT part of the SemVer contract.** It may
        /// change between minor releases to add more diagnostic context.
        /// Treat this string as human-readable log output only. For
        /// structured logging on skip rate, count occurrences of the
        /// variant itself; do not parse this field. Future versions may
        /// expose `content_len` / `mode` as structured fields on this
        /// variant (enabled by `#[non_exhaustive]`).
        reason: String,
    },

    /// Filesystem I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Endpoint-down fast-fail: two distinct lineages failed at the
    /// connection level, so no endpoint is reachable. The run aborts **before**
    /// consensus rather than degrade. Additive (enabled by `#[non_exhaustive]`).
    #[error("endpoint down: no lineage reachable ({})", .lineages.iter().map(|l| l.as_str()).collect::<Vec<_>>().join(", "))]
    EndpointDown {
        /// The connection-failed lineages that tripped the fast-fail.
        lineages: Vec<crate::rotation::Lineage>,
    },

    /// A resolvable system prompt violates the verdict-marker contract.
    ///
    /// Returned by `MagiBuilder::build()` **before any provider is resolved**, and by
    /// [`crate::prompts::validate_prompt`]. It does **not** trigger retry or rotation:
    /// a stale prompt is not fixed by asking the model again. It is a sibling of the
    /// validation path, not a child of it.
    ///
    /// Additive via the enum's `#[non_exhaustive]`; the variant is `#[non_exhaustive]`
    /// too, so match with `..`.
    // `AgentName` has no `Display` (only `display_name()`), so the format string calls
    // it explicitly. Both `agent` and `mode` are optional in the rendering, so the
    // message never claims an owner it does not have.
    #[error("prompt contract violated{}{}: {reason}",
            agent.map_or_else(|| " (unassigned prompt)".to_string(),
                              |a| format!(" for {}", a.display_name())),
            mode.map_or_else(String::new, |m| format!(" (mode {m:?})")))]
    #[non_exhaustive]
    PromptContract {
        /// Whose prompt, when known.
        ///
        /// `Some` on the path that resolves prompts — `build()` always knows the seat,
        /// and a message that cannot say which of the three files to open is not
        /// actionable. `None` for [`crate::prompts::validate_prompt`], where the
        /// **consumer** hands over a loose string and has not chosen a seat yet.
        ///
        /// # Why this is optional, and not always known
        ///
        /// The obvious shape is a plain `AgentName`, reasoning *"no prompt is
        /// ownerless"*. That holds for prompts the crate **resolves**; it does not hold
        /// for a consumer validating a string in their own test suite. With a required
        /// field, that path would have to name a mage — and would name the **wrong**
        /// one: someone checking their Caspar prompt would read *"Melchior"*. An
        /// actively misleading error is worse than an honest `None`,
        /// so the field is optional. Use
        /// [`crate::prompts::validate_prompt_for`] when the seat IS known.
        agent: Option<crate::schema::AgentName>,
        /// `Some` for a per-mode override; `None` for a mode-agnostic override, an
        /// embedded prompt, or a consumer-supplied string.
        mode: Option<crate::schema::Mode>,
        /// Which rule was violated, and how to check it before deploying.
        reason: String,
    },
}

impl From<serde_json::Error> for MagiError {
    fn from(err: serde_json::Error) -> Self {
        MagiError::Deserialization(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- MS2: EndpointDown variant --

    #[test]
    fn test_endpoint_down_variant_carries_lineages() {
        use crate::rotation::Lineage;
        let e = MagiError::EndpointDown {
            lineages: vec![Lineage::from("a"), Lineage::from("b")],
        };
        assert!(matches!(e, MagiError::EndpointDown { .. }));
        assert!(e.to_string().contains("endpoint")); // Display mentions endpoint-down
    }

    // -- ProviderError Display tests --

    /// ProviderError::Http contains status code and body in Display output.
    #[test]
    fn test_provider_error_http_display_contains_status_and_body() {
        let err = ProviderError::Http {
            status: 500,
            body: "Internal Server Error".to_string(),
            retry_after_raw: vec![],
            received_at: None,
        };
        let display = format!("{err}");
        assert!(
            display.contains("500"),
            "Display should contain status code"
        );
        assert!(
            display.contains("Internal Server Error"),
            "Display should contain body"
        );
    }

    /// ProviderError::Network includes message in Display.
    #[test]
    fn test_provider_error_network_display_contains_message() {
        let err = ProviderError::Network {
            message: "connection refused".to_string(),
        };
        let display = format!("{err}");
        assert!(
            display.contains("connection refused"),
            "Display should contain message"
        );
    }

    /// ProviderError::Timeout includes message in Display.
    #[test]
    fn test_provider_error_timeout_display_contains_message() {
        let err = ProviderError::Timeout {
            message: "exceeded 30s".to_string(),
        };
        let display = format!("{err}");
        assert!(
            display.contains("exceeded 30s"),
            "Display should contain message"
        );
    }

    /// ProviderError::Auth includes message in Display.
    #[test]
    fn test_provider_error_auth_display_contains_message() {
        let err = ProviderError::Auth {
            message: "invalid api key".to_string(),
        };
        let display = format!("{err}");
        assert!(
            display.contains("invalid api key"),
            "Display should contain message"
        );
    }

    /// ProviderError::Process includes exit_code and stderr in Display.
    #[test]
    fn test_provider_error_process_display_includes_exit_code_and_stderr() {
        let err = ProviderError::Process {
            exit_code: Some(1),
            stderr: "segfault".to_string(),
        };
        let display = format!("{err}");
        assert!(display.contains("1"), "Display should contain exit code");
        assert!(
            display.contains("segfault"),
            "Display should contain stderr"
        );
    }

    /// ProviderError::Process with no exit code still displays stderr.
    #[test]
    fn test_provider_error_process_display_none_exit_code() {
        let err = ProviderError::Process {
            exit_code: None,
            stderr: "killed".to_string(),
        };
        let display = format!("{err}");
        assert!(display.contains("killed"), "Display should contain stderr");
    }

    /// ProviderError::NestedSession has a meaningful Display.
    #[test]
    fn test_provider_error_nested_session_display() {
        let err = ProviderError::NestedSession;
        let display = format!("{err}");
        assert!(!display.is_empty(), "Display should not be empty");
    }

    // -- v2.0: Http carries Retry-After; RetryAbandoned reports its reason --

    /// Http carries the raw `Retry-After` header values and the receipt instant.
    #[test]
    fn test_http_variant_carries_retry_after_and_receipt() {
        let now = std::time::Instant::now();
        let err = ProviderError::Http {
            status: 429,
            body: "rate limited".to_string(),
            retry_after_raw: vec!["12".to_string()],
            received_at: Some(now),
        };
        match err {
            ProviderError::Http {
                retry_after_raw, ..
            } => {
                assert_eq!(retry_after_raw, vec!["12".to_string()]);
            }
            _ => panic!("expected Http variant"),
        }
    }

    /// RetryAbandoned's Display names the attempt count (budget exhaustion).
    #[test]
    fn test_retry_abandoned_reports_budget_exhaustion() {
        let err = ProviderError::RetryAbandoned {
            reason: AbandonReason::OperationBudgetExhausted {
                elapsed: std::time::Duration::from_secs(600),
                budget: std::time::Duration::from_secs(600),
            },
            attempts: 2,
        };
        let msg = err.to_string();
        assert!(
            msg.contains('2'),
            "message must name the attempt count: {msg}"
        );
    }

    /// RetryAbandoned's Display mentions retry for a too-long `Retry-After`.
    #[test]
    fn test_retry_abandoned_reports_retry_after_too_long() {
        let err = ProviderError::RetryAbandoned {
            reason: AbandonReason::RetryAfterTooLong {
                requested: std::time::Duration::from_secs(600),
                cap: std::time::Duration::from_secs(300),
            },
            attempts: 1,
        };
        assert!(err.to_string().contains("retry"), "{err}");
    }

    // -- MagiError Display tests --

    /// MagiError::Validation contains descriptive message.
    #[test]
    fn test_magi_error_validation_contains_message() {
        let err = MagiError::Validation("confidence out of range".to_string());
        let display = format!("{err}");
        assert!(
            display.contains("confidence out of range"),
            "Display should contain validation message"
        );
    }

    /// MagiError::InsufficientAgents formats succeeded and required in Display.
    #[test]
    fn test_magi_error_insufficient_agents_formats_counts() {
        let err = MagiError::InsufficientAgents {
            succeeded: 1,
            required: 2,
        };
        let display = format!("{err}");
        assert!(
            display.contains("1"),
            "Display should contain succeeded count"
        );
        assert!(
            display.contains("2"),
            "Display should contain required count"
        );
    }

    /// MagiError::InputTooLarge formats size and max in Display.
    #[test]
    fn test_magi_error_input_too_large_formats_size_and_max() {
        let err = MagiError::InputTooLarge {
            size: 2_000_000,
            max: 1_048_576,
        };
        let display = format!("{err}");
        assert!(
            display.contains("2000000"),
            "Display should contain actual size"
        );
        assert!(
            display.contains("1048576"),
            "Display should contain max size"
        );
    }

    // -- From impls --

    /// From<ProviderError> for MagiError wraps correctly into Provider variant.
    #[test]
    fn test_from_provider_error_wraps_into_magi_error_provider() {
        let pe = ProviderError::Timeout {
            message: "timed out".to_string(),
        };
        let me: MagiError = pe.into();
        assert!(
            matches!(me, MagiError::Provider(_)),
            "Should wrap into Provider variant"
        );
    }

    /// From<serde_json::Error> for MagiError produces Deserialization variant.
    #[test]
    fn test_from_serde_json_error_produces_deserialization_variant() {
        let result: Result<String, _> = serde_json::from_str("not json");
        let serde_err = result.unwrap_err();
        let me: MagiError = serde_err.into();
        assert!(
            matches!(me, MagiError::Deserialization(_)),
            "Should produce Deserialization variant"
        );
    }

    /// From<std::io::Error> for MagiError produces Io variant.
    #[test]
    fn test_from_io_error_produces_io_variant() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let me: MagiError = io_err.into();
        assert!(matches!(me, MagiError::Io(_)), "Should produce Io variant");
    }

    // -- external provider errors (Eje C) --

    fn external_message(err: &ProviderError) -> String {
        match err {
            ProviderError::External { message, .. } => message.clone(),
            other => panic!("expected External, got {other:?}"),
        }
    }

    #[test]
    fn a_short_external_message_survives_untouched() {
        let err = ProviderError::external("backend unreachable", ExternalErrorKind::Network);
        assert_eq!(external_message(&err), "backend unreachable");
        assert!(
            !err.to_string().contains("truncated"),
            "nothing was cut, so nothing may claim it was"
        );
    }

    #[test]
    fn an_oversized_external_message_is_cut_and_says_so() {
        let err = ProviderError::external(
            "x".repeat(MAX_EXTERNAL_MESSAGE_BYTES * 2),
            ExternalErrorKind::Other,
        );
        let message = external_message(&err);
        assert!(
            message.len() <= MAX_EXTERNAL_MESSAGE_BYTES,
            "a cap its own marker can push past is a cap that lies about its name: {}",
            message.len()
        );
        assert!(
            message.contains("truncated"),
            "a cut message must never read as a complete one"
        );
    }

    #[test]
    fn truncation_never_splits_a_multibyte_character() {
        // The cut lands mid-character unless it is moved to a boundary — and slicing a `String`
        // there panics. Three-byte characters do not divide the cap evenly, which is the point.
        let err = ProviderError::external(
            "\u{4f60}".repeat(MAX_EXTERNAL_MESSAGE_BYTES),
            ExternalErrorKind::ServerError,
        );
        let message = external_message(&err);
        assert!(message.len() <= MAX_EXTERNAL_MESSAGE_BYTES);
        assert!(message.contains("truncated"));
    }

    #[test]
    fn a_message_of_exactly_the_cap_is_kept_whole() {
        // The boundary the comparison turns on. One byte either way changes the answer, and an
        // off-by-one here would either truncate a message that fit or let one through that did
        // not — the two failures a cap exists to prevent, in opposite directions.
        for (len, truncated) in [
            (MAX_EXTERNAL_MESSAGE_BYTES - 1, false),
            (MAX_EXTERNAL_MESSAGE_BYTES, false),
            (MAX_EXTERNAL_MESSAGE_BYTES + 1, true),
        ] {
            let err = ProviderError::external("x".repeat(len), ExternalErrorKind::Other);
            let message = external_message(&err);
            assert!(
                message.len() <= MAX_EXTERNAL_MESSAGE_BYTES,
                "a {len}-byte message must never exceed the cap"
            );
            assert_eq!(
                message.contains("truncated"),
                truncated,
                "a {len}-byte message against a {MAX_EXTERNAL_MESSAGE_BYTES}-byte cap"
            );
        }
    }

    #[test]
    fn truncation_never_splits_a_four_byte_character() {
        // Four-byte characters divide the budget differently from three-byte ones, and an
        // implementation that walked back a fixed number of bytes would pass the three-byte case
        // and split this one. Emoji are the common carrier of them in real diagnostic text.
        let err = ProviderError::external(
            "\u{1f600}".repeat(MAX_EXTERNAL_MESSAGE_BYTES),
            ExternalErrorKind::ServerError,
        );
        let message = external_message(&err);
        assert!(message.len() <= MAX_EXTERNAL_MESSAGE_BYTES);
        assert!(message.contains("truncated"));
        // The real assertion: the surviving prefix is still valid text. A split codepoint would
        // have panicked at the slice, so reaching here with intact characters is the guarantee.
        assert!(message.chars().any(|c| c == '\u{1f600}'));
    }

    #[test]
    fn the_kind_reaches_the_rendered_message() {
        // The shape is diagnostic: a reader of `failed_agents` must be able to tell an auth
        // failure from an outage without the third party having spelled it out in prose.
        let err = ProviderError::external("nope", ExternalErrorKind::Auth);
        assert!(err.to_string().contains("Auth"), "{err}");
    }
}
