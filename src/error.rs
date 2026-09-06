// Author: Julian Bolivar
// Version: 4.0.0
// Date: 2026-08-23

use std::fmt;
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

/// The phrase that marks the branch where the cap IS the fix.
///
/// A constant rather than a literal repeated in three files, because the wording is
/// declared non-contractual in the migration guide and was nonetheless pinned by raw
/// substrings in the tests of two providers -- so rewording it for clarity broke tests
/// that were meant to assert the BRANCH, not the sentence. The message is composed from
/// these and the tests assert these, which is what makes the pin structural.
pub(crate) const REMEDY_PRESCRIBES: &str = "configurable via";

/// The phrase that marks the branch where the backend named a reason and it is not the budget.
pub(crate) const REMEDY_RULED_OUT: &str = "does not address";

/// The phrase that marks the branch where neither direction is supportable.
pub(crate) const REMEDY_UNKNOWN: &str = "cannot be told";

/// The half of the empty-completion message that depends on what stopped the model.
///
/// The cap is stated either way, because "what budget was in force" is an
/// observation. What is conditional is the PRESCRIPTION: telling an operator to
/// raise `max_tokens` when the backend said it refused sends them to change a
/// number that had nothing to do with it -- the same shape of misdiagnosis as the
/// `http error 0` this release removed, one layer in.
///
/// The third branch is the one that took two rounds to get right. Saying "not the
/// budget" for a reason this crate does not interpret asserts a negative it cannot
/// support. So the unknown case says what is true -- that it cannot be told -- and
/// the vendor vocabularies were widened until only genuinely novel values land
/// there.
///
/// **The prescribing branch is hedged, and it has to be.** `Length` covers two
/// conditions with OPPOSITE remedies -- an undersized output budget and a prompt
/// that filled the context window -- because that is how both wires report them.
/// Prescribing the cap unconditionally would have been the same defect as the one
/// above, one branch over: correct for half the inputs and harmful for the rest.
///
/// # Parameters
///
/// * `bearing` -- from `CompletionTelemetry::budget_bearing`.
///
/// # Returns
///
/// The sentence tail, already carrying its leading punctuation.
fn empty_completion_remedy(
    bearing: crate::provider::BudgetBearing,
    prompt_tokens: Option<u32>,
) -> String {
    use crate::provider::BudgetBearing as B;
    match bearing {
        B::MayExplain => {
            // The measurement is PRINTED, not merely named. Telling a reader to compare
            // `prompt_tokens` against the context window while withholding the number is
            // advice they cannot act on from the message they are holding.
            let measured = match prompt_tokens {
                Some(n) => format!("The prompt measured {n} tokens: if that is"),
                None => "The prompt was not measured; if it is".to_string(),
            };
            format!(
                ", {REMEDY_PRESCRIBES} `CompletionConfig::max_tokens`. {measured} already near the model's context window, it is the input that has to shrink and raising the cap will not help -- both causes arrive under the same termination and this crate does not guess between them."
            )
        }
        B::RuledOut => format!(
            ", but the termination the backend reported is not the budget running out, so raising `CompletionConfig::max_tokens` {REMEDY_RULED_OUT} this."
        ),
        B::Unknown => format!(
            ", and the termination the backend reported is not one this crate interprets, so whether the budget was reached {REMEDY_UNKNOWN} from it."
        ),
    }
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

    /// The endpoint answered, but what it sent does not satisfy the response contract.
    ///
    /// **Mage-local, and the reason is that a lineage is not an endpoint**: in the usual
    /// deployment all three mages reach the SAME backend with DIFFERENT lineages, so condemning
    /// one lineage run-wide would not shield the others from a misbehaving endpoint — it would
    /// pay the cost of the condemnation without buying its protection. Where the scope of a
    /// fault is in doubt, this crate condemns mage-local, because the error is asymmetric:
    /// condemning run-wide when it should have been local takes a HEALTHY candidate away from
    /// the other two seats, while the reverse only costs each seat one attempt discovering the
    /// same thing.
    ///
    /// # Where `Http { status: 0 }` went
    ///
    /// Until `4.0.0` an unreadable body became an `Http` error carrying a synthetic status of
    /// zero — a contract failure wearing an HTTP error's clothes, which is how it inherited
    /// run-wide semantics it was never entitled to. There is no synthetic status any more, and
    /// `Http.status` now only ever holds a real one.
    #[error("response contract violated: {reason}{}", crate::error::suffix(.detail))]
    #[non_exhaustive]
    ResponseContract {
        /// Which part of the contract was not met.
        reason: ResponseContractCause,
        /// Which operation and endpoint it happened on, redacted, when the caller knew.
        ///
        /// Empty when there is nothing to add: a body that parsed and carried no message
        /// describes itself, and the seat is already named where the failure is reported.
        ///
        /// It exists because the variant that replaced the synthetic HTTP status inherited none
        /// of its `body`, and one of the cases routed here is a redirect-policy failure — where
        /// the whole diagnostic value is WHICH endpoint refused. `3.1.0` spent a milestone
        /// establishing that a redacted URL must still say which endpoint failed; dropping it
        /// here would have taken that back for one path without anyone deciding to.
        ///
        /// Composed by the crate's single transport-error mapper, which is the only place that
        /// renders an endpoint at all — so whatever appears here is redacted by construction.
        ///
        /// **Bounded**, at [`MAX_CONTRACT_DETAIL_CHARS`]. It reaches the serialized report, which
        /// this crate has already identified as its most-shared and least-inspected channel, and
        /// every other outside-authored text that gets there is capped.
        ///
        /// Where the bound actually comes from, stated precisely because the obvious phrasing
        /// is wrong: `#[non_exhaustive]` stops only ANOTHER crate from writing the literal.
        /// Inside this one it does not, and most sites do write it — with an **empty** detail,
        /// because their cause is already typed and free text would restate it untyped. An
        /// empty string needs no bound. Every site that passes real text goes through the
        /// crate-private `response_contract` constructor, which is what caps it. (Named, not
        /// linked: it is `pub(crate)`, and public docs cannot link to a private item.)
        detail: String,
    },

    /// The model produced no usable content.
    ///
    /// Commonly the model spent its entire output budget in a reasoning channel and emitted
    /// nothing: HTTP 200, `finish_reason` of `length`, empty content.
    ///
    /// **Mage-local**: the endpoint answered perfectly — it answered *nothing*. That says
    /// nothing about what a DIFFERENT mage would get from the same lineage.
    ///
    /// # Not retried
    ///
    /// Retrying with the same budget reproduces the failure by construction, and that is
    /// measured, not assumed.
    #[error(
        "empty completion: the model returned no content (termination: {:?}). \
         The output budget in force was {cap} tokens{}",
        .telemetry.finish,
        empty_completion_remedy(.telemetry.budget_bearing(), .telemetry.prompt_tokens)
    )]
    #[non_exhaustive]
    EmptyCompletion {
        /// What the provider measured about the completion that came back empty.
        ///
        /// It carries the termination reason, the token counts and -- the reason this is a
        /// whole telemetry rather than a lone reason -- the **reasoning measurement**. That is
        /// the exact failure this release exists to diagnose: the model spent its entire output
        /// budget reasoning and emitted nothing. Reporting the cut without the number that
        /// explains it leaves the operator with "empty, budget 16384" and no evidence of where
        /// the budget went, which is the blindness this milestone set out to end.
        ///
        /// It is the same type the success path returns, so
        /// [`CompletionRecord::from_telemetry`](crate::reporting::CompletionRecord::from_telemetry)
        /// fills a record identically either way: one shape, not two that can drift.
        telemetry: crate::provider::CompletionTelemetry,
        /// The output budget in force for the completion that came back empty.
        ///
        /// It exists so the MESSAGE can name the number the operator has to change. It is not
        /// the source of [`crate::reporting::CompletionRecord::cap`], which is set by the caller
        /// for the same reason stated there — a provider does not know what it was given, so a
        /// record that read the budget off a response would be reporting the provider's opinion
        /// of it. The two are the same number by construction, and the caller owns it.
        cap: u32,
    },

    /// The backend accepted the request and generated nothing at all.
    ///
    /// **This is a defect in `magi-core`, not a failure of the model**, and it does not rotate:
    /// rotating would reproduce our own bad request against every seat in turn. The orchestrator
    /// raises it to [`MagiError`] and aborts the run.
    ///
    /// # The message separates what was OBSERVED from what is INFERRED
    ///
    /// The observation is that the token counters are **absent** — absent, not zero — which is
    /// the discriminant. The inference is the cause, and it rests on a single captured case, so
    /// it is stated as a hypothesis. A second cause producing the same footprint would not make
    /// the observation wrong; it would make the hypothesis wrong, and whoever finds it needs to
    /// see which was which.
    ///
    /// # Known false positive
    ///
    /// `done_reason: "load"` literally means the model was loading, so a transient cold start is
    /// the most plausible way to see this footprint without anyone having written a bad request.
    /// A cold start was captured and does **not** match — it answers `stop`, with content and
    /// with counters present — but that is one observation, not a proof of exclusivity.
    #[error(
        "no generation - token counters absent (termination: {done_reason:?}); the known cause \
         of this is a request without `messages`, which points at a defect in magi-core"
    )]
    #[non_exhaustive]
    NoGeneration {
        /// The termination reason observed, when the backend reported one.
        done_reason: Option<crate::provider::FinishReason>,
    },
}

/// Renders an optional diagnostic tail, so an empty one adds no punctuation.
///
/// A free function rather than an `Option<String>` field: the empty case is common and an
/// `Option` would make every construction site write `None` for it, which reads as a decision
/// where there was none.
pub(crate) fn suffix(detail: &str) -> String {
    if detail.is_empty() {
        String::new()
    } else {
        format!(" ({detail})")
    }
}

/// Which part of a provider's response contract was not met.
///
/// One variant per sub-case, inside a single [`ProviderError::ResponseContract`], because the
/// unit of separation in [`ProviderError`] is the **consequence** and all three of these share it.
/// Splitting them into sibling error variants would force the orchestrator's classifier to nest
/// a match to reach the same answer — and this crate has already paid for a nested match that
/// stole the outer one's state.
///
/// # Migrating from a synthetic `Http { status: 0 }`
///
/// This example is a **doctest**, so it is compiled: the same shape written into
/// `docs/migration-v4.0.md` is prose that nothing checks, and one snippet there had already
/// gone stale against a field rename before anyone noticed. Whatever a reader copies should
/// have been compiled at least once.
///
/// ```
/// use magi_core::prelude::{ProviderError, ResponseContractCause};
///
/// fn is_a_contract_failure(err: &ProviderError) -> bool {
///     match err {
///         ProviderError::ResponseContract { reason, .. } => match reason {
///             ResponseContractCause::Unreadable => true,
///             ResponseContractCause::NoMessage => true,
///             ResponseContractCause::RedirectRefused => true,
///             // `#[non_exhaustive]`: a cause added later lands here rather than
///             // failing to compile for every consumer.
///             _ => true,
///         },
///         _ => false,
///     }
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResponseContractCause {
    /// The body could not be read as the format the provider speaks.
    ///
    /// Retryable, and it is the only one of these that is: a body can be unreadable because it
    /// was **truncated in transit** — a connection cut mid-response, a proxy that clipped it —
    /// and asking again can genuinely return something different.
    Unreadable,

    /// The body parsed, but carried no message for this crate to read.
    ///
    /// Named for what is missing rather than for the wire's field name: `{"choices": []}` is
    /// perfectly VALID JSON that simply does not carry what the contract promises, so a name
    /// saying "malformed" would be false for this half of the family. And `choices` is OpenAI's
    /// vocabulary, while this type is public and this crate is provider-agnostic.
    NoMessage,

    /// The endpoint answered with a redirect chain this crate would not follow.
    ///
    /// Not retryable: the same request follows the same chain and fails the same way, so a retry
    /// only spends budget.
    ///
    /// # Why it lives among the contract causes and not on its own
    ///
    /// The unit of separation in [`ProviderError`] is the **consequence**, and this shares its
    /// consequence exactly with [`Self::NoMessage`] — mage-local, never retried. A sibling error
    /// variant would force the orchestrator's classifier to reach the same answer through a
    /// second arm, and this crate has already paid for a classifier that grew one match too many.
    ///
    /// # What it must never be
    ///
    /// [`ProviderError::Network`], whose connection class feeds the endpoint-down latch: two of
    /// these would abort a whole run over a misconfigured redirect chain seen by one seat. Until
    /// `4.0.0` this case borrowed the synthetic zero status for exactly that reason, which is how
    /// a routing decision ended up encoded in a number that was never an HTTP status.
    RedirectRefused,
}

impl fmt::Display for ResponseContractCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unreadable => "the response body could not be read",
            Self::NoMessage => "the response carried no message",
            Self::RedirectRefused => "the response was a redirect chain this crate will not follow",
        })
    }
}

/// Marker appended when text is cut, so a truncated message never reads as a complete one.
///
/// Lives here rather than beside the retry machinery because this is the foundation layer: the
/// error type must not import from the layers above it, and both of the crate's truncation sites
/// need this constant. It sat in `provider.rs` briefly and made `error.rs` depend upwards.
pub(crate) const TRUNCATION_MARKER: &str = " … (truncated)";

/// Upper bound, in bytes, for the text an external provider may attach to a failure.
pub const MAX_EXTERNAL_MESSAGE_BYTES: usize = 400;

// The cap must leave room for the marker, or the budget subtraction below it underflows to zero
// and the truncated message would be the marker alone — a diagnostic that says only that there
// was one. The relation was load-bearing and implicit, held by the happy accident that 400 sits
// far above the marker's 16 BYTES (its ellipsis is multi-byte, so that is not its 14 characters —
// and bytes are what `len()` returns and what the subtraction spends). Stated here it becomes a
// compile error rather than a surprise, which is the move this milestone makes wherever it can.
const _: () = assert!(MAX_EXTERNAL_MESSAGE_BYTES > TRUNCATION_MARKER.len());

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
/// The worst case is not a clean outage but an **intermittent** backend: each seat rotates, finds
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

/// Upper bound on [`ProviderError::ResponseContract`]'s `detail`, in characters.
///
/// Chosen to match the reasoning already written for the rotation detail: long enough for a
/// composed message naming an operation and a redacted endpoint, short enough that a hostile or
/// merely verbose backend cannot inflate a report through it.
pub const MAX_CONTRACT_DETAIL_CHARS: usize = 256;

impl ProviderError {
    /// Builds a [`ProviderError::ResponseContract`] with its `detail` bounded.
    ///
    /// # Why a constructor rather than a convention
    ///
    /// The field's invariant used to be prose — *"composed by the single transport-error mapper,
    /// so it is redacted by construction"*. That was true of every site that existed, which is
    /// not the same as being true. A bound stated only in a doc comment is enforced by whoever
    /// reads it, and this text reaches the serialized report.
    ///
    /// Truncation is on a character boundary and never panics.
    ///
    /// # `pub(crate)`, and the redaction gate is what said so
    ///
    /// It was written `pub` and the CI rule refused it: `error.rs` holds exactly ONE public
    /// door, and whatever that door builds is what the outside world can build. A contract
    /// failure is this crate's own reading of a response — an external provider claiming one
    /// would be asserting something only the parser can know. `external` stays the only public
    /// constructor, which is the shape `3.1.0` settled on.
    /// Gated with its only caller, the shared transport mapper, which exists only when an
    /// HTTP provider is compiled in. Without the gate the default feature set fails on an
    /// unused function -- and silencing that with an attribute would have kept a
    /// constructor alive in a build where nothing can construct.
    #[cfg(any(feature = "claude-api", feature = "openai-compat"))]
    pub(crate) fn response_contract(
        reason: ResponseContractCause,
        detail: impl Into<String>,
    ) -> Self {
        let mut detail = detail.into();
        if detail.chars().count() > MAX_CONTRACT_DETAIL_CHARS {
            let cut = detail
                .char_indices()
                .nth(MAX_CONTRACT_DETAIL_CHARS)
                .map_or(detail.len(), |(i, _)| i);
            detail.truncate(cut);
        }
        Self::ResponseContract { reason, detail }
    }

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
    /// Every struct-like variant of this enum is `#[non_exhaustive]`, so none of them can be
    /// built with a struct expression from another crate. (`NestedSession` is a bare unit variant
    /// and is constructible, which helps nobody: it names a condition only this crate
    /// detects.) Without a constructor an external provider could *compile*
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

    /// A defect in **this crate**, detected at run time. Aborts the run.
    ///
    /// # Why this is not a `failed_agents` entry
    ///
    /// `failed_agents` is where model failures land every day. A bug of ours filed there is
    /// invisible in the noise of the normal, and the operator goes and looks at the model. This
    /// project has already paid that bill once, on a defect that masqueraded as a provider error
    /// and cost a great deal to identify as local.
    ///
    /// # The message separates OBSERVATION from HYPOTHESIS, and so does this type
    ///
    /// The two are distinct **fields**, not two halves of one sentence, so the distinction
    /// survives however the text is later formatted. What was measured is a fact; what caused it
    /// is an inference drawn from a single captured case.
    #[error("defect in magi-core, run aborted: {observation}; {hypothesis} (agent {}, model {model})", .agent.display_name())]
    #[non_exhaustive]
    CrateDefect {
        /// What was MEASURED, with no causal claim attached.
        observation: String,
        /// The known cause, offered as a hypothesis. Text this crate authors, never the wire's.
        hypothesis: &'static str,
        /// The seat that hit it.
        agent: crate::schema::AgentName,
        /// The model in force when it happened.
        model: String,
        /// The seats that had already been **joined** when the abort was reached.
        ///
        /// # It is a hint, not a census — and the distinction is why it is named this way
        ///
        /// Membership is decided by dispatch and join ORDER, not by who answered: a seat that
        /// had produced its verdict but had not yet been joined is absent from this list. So it
        /// cannot be read as *did this hit every seat, or one?* — an earlier version of this
        /// documentation claimed exactly that, and the code never supported it.
        ///
        /// What it is good for is the opposite direction: a NON-empty list proves other seats
        /// got that far, which is what tells you the defect is not simply "the run never
        /// started". Empty is the common case, because the discriminant is that no generation
        /// happened, so the backend answers in fractions of a second.
        joined_before_abort: Vec<crate::schema::AgentName>,
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

    // ---- Task 3b: the three response-contract variants ----

    #[test]
    fn the_unit_of_separation_is_the_consequence_not_the_case() {
        // One variant per family, with a typed reason inside for the sub-cases. A single
        // `Contract { reason }` spanning all three families would force the orchestrator's
        // classifier to NEST a match to decide the consequence — and this crate already paid for
        // that: one of the 3.1.0 defects was a nested match that stole the outer one's state.
        let a = ProviderError::ResponseContract {
            reason: ResponseContractCause::NoMessage,
            detail: String::new(),
        };
        let b = ProviderError::ResponseContract {
            reason: ResponseContractCause::Unreadable,
            detail: String::new(),
        };
        assert_eq!(
            std::mem::discriminant(&a),
            std::mem::discriminant(&b),
            "both sub-cases share one variant because they share one consequence"
        );
    }

    #[test]
    fn empty_choices_is_valid_json_so_the_name_must_not_say_malformed() {
        // `{"choices": []}` parses perfectly; it simply does not carry what the contract
        // promises. A name saying "malformed" would be false for half the family. And
        // `NoMessage`, not `NoChoices`: `choices` is OpenAI vocabulary and this type is PUBLIC.
        let n = format!("{:?}", ResponseContractCause::NoMessage);
        assert!(!n.to_lowercase().contains("malformed"), "{n}");
        assert!(!n.to_lowercase().contains("choices"), "{n}");
    }

    #[test]
    fn no_generation_carries_the_observed_reason_and_nothing_else() {
        // The counters are ABSENT — that IS the discriminant. Inventing a zero for them would
        // assert a measurement that never happened.
        let e = ProviderError::NoGeneration {
            done_reason: Some(crate::provider::FinishReason::Load),
        };
        assert!(!format!("{e:?}").contains("tokens"));
    }

    #[test]
    fn an_empty_completion_names_the_cap_and_says_it_is_configurable() {
        // The whole point of the diagnosis axis: the message has to contain its own fix. An
        // operator reading "http error 0" went and looked at a network that answered 200.
        let rendered = ProviderError::EmptyCompletion {
            telemetry: crate::provider::CompletionTelemetry::unmeasured()
                .with_finish(crate::provider::FinishReason::Length),
            cap: 4096,
        }
        .to_string();
        assert!(rendered.contains("4096"), "{rendered}");
        assert!(rendered.contains("max_tokens"), "{rendered}");
        assert!(
            !rendered.to_lowercase().contains("http"),
            "a content failure must not render as a transport one: {rendered}"
        );
    }

    #[test]
    fn an_empty_completion_prescribes_the_cap_only_where_the_cap_is_the_fix() {
        // BOTH messages state the budget in force, because "what was the budget" is an
        // observation. What must not survive a refusal is the PRESCRIPTION: sending an
        // operator to raise `max_tokens` when the backend said it refused is the same shape
        // of misdiagnosis as the `http error 0` this release removed, one layer down -- and
        // it reached here through the one lane the termination narrowing does not cover, a
        // text-only response that came back blank.
        //
        // Three groups, not two. The unknown group is the correction of a first attempt that
        // put every unrecognised reason on the "not the budget" side, which asserted a
        // negative from an uninterpreted string -- the same defect, sign flipped.
        let render = |f: Option<FinishReason>| {
            let mut t = crate::provider::CompletionTelemetry::unmeasured();
            if let Some(f) = f {
                t = t.with_finish(f);
            }
            ProviderError::EmptyCompletion {
                telemetry: t,
                cap: 16_384,
            }
            .to_string()
        };

        // Unknown is NOT evidence that the budget was untouched, so absent prescribes too.
        for may in [None, Some(FinishReason::Length)] {
            let s = render(may);
            assert!(s.contains("16384"), "{s}");
            assert!(
                s.contains("not measured"),
                "the sentence names the prompt as the other cause, so it must say whether the prompt was measured: {s}"
            );
            assert!(
                s.contains(REMEDY_PRESCRIBES),
                "the budget can explain this, so the message must carry its own fix: {s}"
            );
            assert!(!s.contains(REMEDY_RULED_OUT), "{s}");
            assert!(!s.contains(REMEDY_UNKNOWN), "{s}");
        }

        // Reasons this crate INTERPRETS, and neither of them is the budget.
        for ruled_out in [FinishReason::Stop, FinishReason::Load] {
            let s = render(Some(ruled_out));
            assert!(
                s.contains("16384"),
                "the budget in force stays an observation: {s}"
            );
            assert!(
                s.contains(REMEDY_RULED_OUT),
                "prescribing the cap for a reason the backend named is a misdiagnosis: {s}"
            );
            assert!(
                !s.contains(REMEDY_PRESCRIBES),
                "prescription survived a termination that rules it out: {s}"
            );
        }

        // A reason this crate does NOT interpret -- one no vendor publishes, which is what
        // this branch is for now that both published vocabularies are translated. It could
        // be anything, a new way of saying the room ran out included, so claiming the budget
        // was not involved would be inventing evidence in the other direction.
        let s = render(Some(FinishReason::Other(
            "a_reason_invented_after_this_was_written".to_string(),
        )));
        assert!(s.contains("16384"), "{s}");
        assert!(
            s.contains(REMEDY_UNKNOWN),
            "an uninterpreted reason supports neither direction: {s}"
        );
        assert!(!s.contains(REMEDY_PRESCRIBES), "{s}");
        assert!(!s.contains(REMEDY_RULED_OUT), "{s}");

        // The number is PRINTED when it exists. Naming `prompt_tokens` as the thing to
        // compare while withholding it is advice the reader cannot act on.
        let measured = ProviderError::EmptyCompletion {
            telemetry: crate::provider::CompletionTelemetry::unmeasured()
                .with_finish(FinishReason::Length)
                .with_prompt_tokens(63_924),
            cap: 16_384,
        }
        .to_string();
        assert!(
            measured.contains("63924"),
            "the prompt measurement must appear in the sentence that tells you to check it: {measured}"
        );
    }

    #[test]
    fn a_crate_defect_keeps_the_observation_apart_from_the_hypothesis() {
        // Separate FIELDS, not two halves of one sentence, so the distinction survives however
        // the text is later formatted. The attribution rests on a single captured case; whoever
        // meets a second cause with the same footprint has to be able to see which was which.
        let e = MagiError::CrateDefect {
            observation: "no generation - token counters absent".to_string(),
            hypothesis: "the known cause is a request without `messages`",
            agent: crate::schema::AgentName::Caspar,
            model: "glm-5.2:cloud".to_string(),
            // Empty is the honest value here: nothing had answered when a scripted defect fires.
            joined_before_abort: Vec::new(),
        };
        let rendered = e.to_string();
        assert!(rendered.contains("no generation - token counters absent"));
        assert!(rendered.contains("the known cause is"));
        assert!(
            rendered.contains("magi-core"),
            "the category has to be legible, or the bug hides in the noise of ordinary model \
             failures: {rendered}"
        );
    }
    use super::*;
    use crate::provider::FinishReason;

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
    // ---------------------------------------------------------------------
    // Task 14 — the empty-completion error names the budget that cut it.
    // ---------------------------------------------------------------------

    #[test]
    fn the_empty_completion_error_carries_its_own_fix() {
        // A-4. Until `4.0.0` an operator saw "transport" here and went to look at a
        // network that had answered HTTP 200 perfectly. The message has to name the
        // number that cut the completion AND say that the number is theirs to change,
        // or it diagnoses without being actionable.
        let e = ProviderError::EmptyCompletion {
            telemetry: crate::provider::CompletionTelemetry::unmeasured()
                .with_finish(crate::provider::FinishReason::Length),
            cap: 16_384,
        };
        let s = e.to_string();
        assert!(s.contains("16384"), "it must name the cap that cut it: {s}");
        assert!(
            s.contains("max_tokens"),
            "and that the cap is configurable: {s}"
        );
        assert!(
            !s.to_lowercase().contains("http"),
            "it is not an HTTP failure, and saying so would send the reader to the wrong place: {s}"
        );
    }

    #[test]
    fn a_genuine_empty_and_a_budget_empty_read_differently() {
        // The two need opposite remedies — raise the budget, or look at the model —
        // so a message that renders them identically has diagnosed nothing.
        let a = ProviderError::EmptyCompletion {
            telemetry: crate::provider::CompletionTelemetry::unmeasured()
                .with_finish(crate::provider::FinishReason::Stop),
            cap: 16_384,
        };
        let b = ProviderError::EmptyCompletion {
            telemetry: crate::provider::CompletionTelemetry::unmeasured()
                .with_finish(crate::provider::FinishReason::Length),
            cap: 16_384,
        };
        assert_ne!(a.to_string(), b.to_string());
    }

    #[test]
    fn a_contract_failure_can_still_say_which_endpoint_it_happened_on() {
        // The property `3.1.0` spent a milestone establishing: a redacted URL must still name the
        // endpoint that failed. Deleting the synthetic HTTP status took its `body` with it, and
        // one of the cases routed to the replacement is a redirect-policy failure — where WHICH
        // endpoint refused is the entire diagnostic. Review caught it going silently missing.
        let with = ProviderError::ResponseContract {
            reason: ResponseContractCause::RedirectRefused,
            detail: "POST https://api.example.com/v1?key=[REDACTED]: too many redirects".into(),
        };
        let rendered = with.to_string();
        assert!(rendered.contains("api.example.com"), "{rendered}");
        assert!(rendered.contains("redirect"), "{rendered}");

        // And a cause that describes itself adds no punctuation for a detail it does not have.
        let without = ProviderError::ResponseContract {
            reason: ResponseContractCause::NoMessage,
            detail: String::new(),
        };
        assert!(
            !without.to_string().contains("()"),
            "an empty detail must not render as empty parentheses: {without}"
        );
    }

    #[test]
    fn the_shipped_messages_carry_no_space_runs_from_a_joined_source_line() {
        // A run of interior spaces is the fingerprint of a source string that was
        // wrapped across lines and re-joined without the indentation being stripped:
        // the SOURCE reads fine and the RENDERED message does not. These strings reach
        // `failed_agents` and the serialized report — the most-shared, least-inspected
        // channel this crate has — so the defect ships even though nothing fails.
        //
        // Asserted over the rendered text of the variants this milestone added rather
        // than by scanning the source: a scan would have to reproduce Rust's literal
        // rules to avoid flagging ordinary indentation, and a check that is harder to
        // read than the thing it guards is the one that gets deleted.
        let rendered = [
            ProviderError::EmptyCompletion {
                telemetry: crate::provider::CompletionTelemetry::unmeasured()
                    .with_finish(crate::provider::FinishReason::Length),
                cap: 16_384,
            }
            .to_string(),
            ProviderError::NoGeneration {
                done_reason: Some(crate::provider::FinishReason::Load),
            }
            .to_string(),
            ProviderError::ResponseContract {
                reason: ResponseContractCause::NoMessage,
                detail: String::new(),
            }
            .to_string(),
        ];
        for s in rendered {
            assert!(
                !s.contains("   "),
                "a rendered message must not carry a run of spaces: {s:?}"
            );
        }
    }
    /// The detail is BOUNDED, and not merely documented as bounded.
    #[cfg(any(feature = "claude-api", feature = "openai-compat"))]
    #[test]
    fn a_contract_detail_is_capped_at_the_declared_length() {
        let long = "x".repeat(MAX_CONTRACT_DETAIL_CHARS * 4);
        let err = ProviderError::response_contract(ResponseContractCause::RedirectRefused, long);
        let ProviderError::ResponseContract { detail, .. } = &err else {
            panic!("the constructor must build its own variant");
        };
        assert_eq!(detail.chars().count(), MAX_CONTRACT_DETAIL_CHARS);
    }

    /// A shorter detail is kept whole: the cap truncates, it does not pad or normalise.
    #[cfg(any(feature = "claude-api", feature = "openai-compat"))]
    #[test]
    fn a_contract_detail_under_the_cap_is_untouched() {
        let err = ProviderError::response_contract(
            ResponseContractCause::RedirectRefused,
            "redirect refused by policy for https://gw.example.com/v1",
        );
        let ProviderError::ResponseContract { detail, .. } = &err else {
            panic!("the constructor must build its own variant");
        };
        assert_eq!(
            detail,
            "redirect refused by policy for https://gw.example.com/v1"
        );
    }

    /// Truncation lands on a CHARACTER boundary and never panics.
    ///
    /// A composed message can carry a redacted endpoint with non-ASCII in its path, and cutting
    /// by byte offset there is a panic, not a truncation. This crate has already shipped one
    /// release for a slicing bug of exactly that shape.
    #[cfg(any(feature = "claude-api", feature = "openai-compat"))]
    #[test]
    fn a_multi_byte_contract_detail_is_cut_without_panicking() {
        let long = "é".repeat(MAX_CONTRACT_DETAIL_CHARS * 2);
        let err = ProviderError::response_contract(ResponseContractCause::Unreadable, long);
        let ProviderError::ResponseContract { detail, .. } = &err else {
            panic!("the constructor must build its own variant");
        };
        assert_eq!(detail.chars().count(), MAX_CONTRACT_DETAIL_CHARS);
        assert!(detail.chars().all(|c| c == 'é'));
    }

    // ---------------------------------------------------------------------
    // Task 1 (R-11) — a single implementation of marker truncation.
    // ---------------------------------------------------------------------

    #[test]
    fn mark_within_cap_cuts_on_a_char_boundary_and_reserves_the_marker() {
        // Multi-byte on purpose: the historical defect was slicing mid-codepoint.
        let text = "áéíóú".repeat(50);
        let out = mark_within_cap(&text, 20);
        assert!(
            out.len() <= 20,
            "output must fit the cap, got {}",
            out.len()
        );
        assert!(out.ends_with(TRUNCATION_MARKER));
        assert!(text.starts_with(out.trim_end_matches(TRUNCATION_MARKER)));
    }

    #[test]
    fn mark_within_cap_marks_even_when_the_text_fits() {
        // THE HELPER HAS NO EARLY-RETURN, and this test is where that decision is pinned.
        // An earlier version asserted `== text` -- meaning it returned the input
        // intact when it fit-- and that CONTRADICTED the corrected contract: `mark_truncated`
        // needs the marker whether it fits or not, because it marks a PARTIAL read, not a cut
        // by cap. With the early-return inside the helper, that site lost the signal.
        //
        // The three call sites that DO want the intact input keep their own `if`
        // before calling; the helper has ONE single semantics.
        let text = "short";
        let out = mark_within_cap(text, 4096);
        assert_eq!(out, format!("{text}{TRUNCATION_MARKER}"));
        assert!(out.len() <= 4096);
    }

    #[test]
    fn mark_within_cap_survives_a_cap_smaller_than_the_marker() {
        // The helper is NEW and generic: its four call sites today have constants
        // that make this impossible, but the next caller brings its own cap. Subtracting
        // the marker length from a smaller cap would underflow.
        let text = "some long text that will not fit";
        // THE EXPECTED VALUES OF THIS TEST ARE DERIVED FROM THESE EXACT BYTES. Pinned
        // so that changing the marker fails HERE, with an instruction, instead of
        // failing three asserts below with a mystery. NOT loosening them: a bound
        // is what made the old contract vacuous.
        assert_eq!(
            TRUNCATION_MARKER, " … (truncated)",
            "if the marker changes, RECOMPUTE the expected values below -- do not relax them"
        );
        // THAT IS U+2026 HORIZONTAL ELLIPSIS, ONE codepoint of THREE bytes -- not three
        // ASCII dots (error.rs:470, read). The whole arithmetic below depends on it:
        // with "..." the three dots would be three 1-byte chars and a cut at 2 would
        // yield " ." instead of " ". This is also WHY `floor_char_boundary` is
        // load-bearing here rather than decorative.
        let out = mark_within_cap(text, 2);
        // EXACT VALUE, not a bound. `out.len() <= 2` plus `starts_with` satisfies them
        // BOTH " " AND "" -- meaning the test passed with the correct implementation
        // and with the wrong one, and could not catch the arithmetic error that a version
        // earlier of this contract had written.
        //
        // MEASURED on the real marker " … (truncated)" (error.rs:470): byte 0 is a
        // space and bytes 1..4 are the SINGLE codepoint U+2026, so `floor_char_boundary(2)`
        // cannot cut inside it, gives 1, and the result is
        // ONE SPACE. Empty only with cap == 0.
        assert_eq!(
            out, " ",
            "cap=2 cuts the `...` in half: only the space survives"
        );
        // The marker trimming also goes through `floor_char_boundary`: a
        // raw `&TRUNCATION_MARKER[..cap]` PANICS, which is the same class of byte-index cut
        // that already cost a release to this project.
    }

    #[test]
    fn mark_within_cap_at_exactly_the_marker_length() {
        // THE boundary: one byte less falls into the case above, one more leaves room for
        // content. Here the budget for text is exactly zero.
        let out = mark_within_cap("some text", TRUNCATION_MARKER.len());
        assert_eq!(out, TRUNCATION_MARKER);
    }

    #[test]
    fn mark_within_cap_returns_empty_for_a_zero_cap() {
        assert_eq!(mark_within_cap("anything", 0), "");
    }

    #[test]
    fn mark_within_cap_marks_an_empty_input_too() {
        // THERE IS NO EMPTY-INPUT CARVE-OUT, and this is the correction of a defect that
        // a previous round of this plan introduced. It said that "" should come out "" without
        // marker -- "nothing to truncate" -- and that CHANGES THE OUTPUT of `mark_truncated`:
        // today, with budget 8176 and a cut of 0, it returns the marker. VERIFIED against
        // the tree, not reasoned.
        //
        // And Task 1 is a REFACTOR whose verification is "the suite stays green", which CANNOT
        // see that change unless there is a test for empty partial body. A
        // output change invisible to its own verification bucket is the worst
        // form of this defect.
        //
        // The other THREE sites are not affected: each has its own
        // `if len <= cap { return intacto }` --error.rs:604, provider.rs:1474,
        // truncate_diagnostic:295-- so the empty input never reaches the helper.
        assert_eq!(mark_within_cap("", 4096), TRUNCATION_MARKER);
    }

    // -- Step 4: the three boundary cases the helper's own tests above do not pivot on --

    #[test]
    fn mark_within_cap_at_budget_minus_one_stays_uncut() {
        // Pivot is `budget = cap - TRUNCATION_MARKER.len()`, NOT `cap`: with no early-return,
        // a text of length `budget - 1` is one byte SHORT OF THE BUDGET the marker reserves,
        // not "one byte from the cap" -- so it survives whole and the output lands at
        // `cap - 1`, not `cap`.
        let cap = 50;
        let budget = cap - TRUNCATION_MARKER.len();
        let text = "x".repeat(budget - 1);
        let out = mark_within_cap(&text, cap);
        assert_eq!(out, format!("{text}{TRUNCATION_MARKER}"), "must not be cut");
        assert_eq!(out.len(), cap - 1);
    }

    #[test]
    fn mark_within_cap_at_the_budget_stays_uncut() {
        // Exactly at the budget: still no cut, and the output now lands at `cap` exactly.
        let cap = 50;
        let budget = cap - TRUNCATION_MARKER.len();
        let text = "x".repeat(budget);
        let out = mark_within_cap(&text, cap);
        assert_eq!(out, format!("{text}{TRUNCATION_MARKER}"), "must not be cut");
        assert_eq!(out.len(), cap);
    }

    #[test]
    fn mark_within_cap_one_over_the_budget_cuts_by_one_byte() {
        // One byte past the budget is where the cut finally starts, one byte at a time --
        // not at `cap`, which the two tests above already occupy without triggering it.
        let cap = 50;
        let budget = cap - TRUNCATION_MARKER.len();
        let text = "x".repeat(budget + 1);
        let out = mark_within_cap(&text, cap);
        assert_eq!(
            out,
            format!("{}{TRUNCATION_MARKER}", &text[..budget]),
            "must be cut by exactly one byte"
        );
        assert_eq!(out.len(), cap);
    }

    // -- Step 4: the call-site test observable only from error.rs's own guard --

    #[test]
    fn an_empty_external_message_is_kept_empty() {
        // The call site's OWN guard decides this, not the helper: `external` keeps its
        // `if raw.len() <= MAX_EXTERNAL_MESSAGE_BYTES` before ever calling `mark_within_cap`,
        // so an empty message never reaches the helper and comes back untouched -- unlike
        // `mark_truncated`, which has no such guard and would mark even this.
        let err = ProviderError::external("", ExternalErrorKind::Other);
        assert_eq!(external_message(&err), "");
        assert!(!err.to_string().contains("truncated"));
    }
}
