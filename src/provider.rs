// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05

use crate::backoff::RetryClass;
use crate::error::{AbandonReason, ProviderError};
use crate::schema::Mode;
use std::sync::Arc;
use std::time::Duration;

/// Configuration for LLM completion requests.
///
/// Controls parameters like token limits and sampling temperature.
/// Marked `#[non_exhaustive]` to allow adding fields in future versions
/// without breaking downstream crates.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct CompletionConfig {
    /// Maximum number of tokens in the LLM response.
    pub max_tokens: u32,
    /// Sampling temperature (0.0 = deterministic).
    pub temperature: f64,
}

impl Default for CompletionConfig {
    fn default() -> Self {
        Self {
            max_tokens: 4096,
            temperature: 0.0,
        }
    }
}

/// Abstraction for LLM backends.
///
/// Any LLM provider (Claude, Gemini, OpenAI, local models) implements this
/// trait. Uses `async-trait` because native async traits in Rust do not yet
/// support `dyn Trait` dispatch, which is required for `Arc<dyn LlmProvider>`
/// with `tokio::spawn`.
///
/// The `Send + Sync` bounds are required because `Arc<dyn LlmProvider>` is
/// shared across `tokio::spawn` tasks.
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    /// Sends a completion request to the LLM provider.
    ///
    /// # Parameters
    /// - `system_prompt`: The system-level instruction for the LLM.
    /// - `user_prompt`: The user's input content.
    /// - `config`: Completion parameters (max_tokens, temperature).
    ///
    /// # Returns
    /// The LLM's text response, or a `ProviderError` on failure.
    async fn complete(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        config: &CompletionConfig,
    ) -> Result<String, ProviderError>;

    /// Returns the provider's name (e.g., "claude", "claude-cli", "openai").
    fn name(&self) -> &str;

    /// Returns the model identifier (e.g., "claude-sonnet-4-6").
    fn model(&self) -> &str;
}

/// Resolves a short Claude model alias to a full model identifier.
///
/// Used by both `ClaudeProvider` (HTTP) and `ClaudeCliProvider` (subprocess).
/// Other providers (Gemini, OpenAI) should implement their own alias resolvers.
///
/// # Aliases
///
/// - `"sonnet"` → `"claude-sonnet-4-6"`
/// - `"opus"` → `"claude-opus-4-7"`
/// - `"haiku"` → `"claude-haiku-4-5-20251001"`
/// - Any string containing `"claude-"` passes through as-is
///
/// # Errors
///
/// Returns `ProviderError::Auth` if the alias is unknown.
///
/// # Examples
///
/// ```
/// use magi_core::provider::resolve_claude_alias;
///
/// assert_eq!(resolve_claude_alias("opus").unwrap(), "claude-opus-4-7");
/// assert_eq!(resolve_claude_alias("claude-custom").unwrap(), "claude-custom");
/// assert!(resolve_claude_alias("unknown").is_err());
/// ```
pub fn resolve_claude_alias(model: &str) -> Result<String, ProviderError> {
    match model {
        "sonnet" => Ok("claude-sonnet-4-6".to_string()),
        "opus" => Ok("claude-opus-4-7".to_string()),
        "haiku" => Ok("claude-haiku-4-5-20251001".to_string()),
        m if m.contains("claude-") => Ok(m.to_string()),
        _ => Err(ProviderError::Auth {
            message: format!("unknown model alias: {model}"),
        }),
    }
}

/// Resolves the default model short-name (`"opus"`, `"sonnet"`, `"haiku"`)
/// recommended for the given analysis mode.
///
/// Mirrors Python's `MODE_DEFAULT_MODELS` (MAGI@v2.2.8 `models.py:58-62`).
/// As of v0.4.0 all three modes default to `"opus"` per Python parity.
/// Pair with [`resolve_claude_alias`] to obtain the full model id:
///
/// ```
/// use magi_core::provider::{default_model_for_mode, resolve_claude_alias};
/// use magi_core::schema::Mode;
///
/// let alias = default_model_for_mode(Mode::Analysis);
/// let model_id = resolve_claude_alias(alias).unwrap();
/// assert_eq!(model_id, "claude-opus-4-7");
/// ```
///
/// # Arguments
///
/// * `mode` — The analysis mode whose default model alias to return.
///
/// # Returns
///
/// The short alias name (always `"opus"` in v0.4.0). Future versions may
/// route different modes to different defaults without breaking this API.
pub fn default_model_for_mode(mode: Mode) -> &'static str {
    match mode {
        Mode::CodeReview => "opus",
        Mode::Design => "opus",
        Mode::Analysis => "opus",
    }
}

/// Named default values (no magic numbers).
const DEFAULT_MAX_RETRIES: u32 = 3;
const DEFAULT_BASE_DELAY: Duration = Duration::from_secs(1);
const DEFAULT_CAP: Duration = Duration::from_secs(60);
const DEFAULT_RETRY_AFTER_CAP: Duration = Duration::from_secs(300);
const DEFAULT_OPERATION_BUDGET: Duration = Duration::from_secs(600);

/// Default **total** request timeout for the HTTP providers.
///
/// Total, not connect or read: the failure mode that matters — a model that
/// accepts the connection, returns headers, and then **hangs generating** — is
/// caught by no connect-timeout, and a read-timeout resets on each byte. 300 s is
/// generous for an LLM completion; who needs more uses the provider's
/// `with_timeout` constructor. `Duration::MAX` means "no timeout".
pub const DEFAULT_CLIENT_TIMEOUT: Duration = Duration::from_secs(300);

/// Retry configuration for [`RetryProvider`].
///
/// Set **only at construction** and then immutable: there is no public way to
/// mutate it afterwards, so that the dangerous-configuration warnings cannot be
/// evaded.
///
/// `RetryConfig` is `#[non_exhaustive]`: build it from [`Default`] and then
/// adjust fields (the struct-literal `RetryConfig { .. }` does not compile
/// outside the crate — that is the 2.0 migration pattern).
///
/// ```
/// use magi_core::prelude::*;
/// use std::time::Duration;
///
/// let mut cfg = RetryConfig::default();
/// cfg.max_retries = 5;
/// cfg.cap = Duration::from_secs(30);
///
/// assert_eq!(cfg.max_retries, 5);
/// assert_eq!(cfg.cap, Duration::from_secs(30));
/// // The rest of the defaults still apply.
/// assert_eq!(cfg.base_delay, Duration::from_secs(1));
/// ```
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct RetryConfig {
    /// Maximum retries after the first failure.
    pub max_retries: u32,
    /// Base wait. `ZERO` disables **both the pause and the jitter** — see warnings.
    pub base_delay: Duration,
    /// Ceiling for our own backoff. Mandatory: there is no "no ceiling".
    pub cap: Duration,
    /// Maximum `Retry-After` we accept obeying; exceeding it abandons.
    ///
    /// The `Retry-After` header is parsed in **whole seconds** (RFC 7231
    /// delta-seconds), so this cap is compared at whole-second granularity: a
    /// **sub-second** `retry_after_cap` (e.g. 500 ms) rounds down to 0 s, which
    /// makes **every** positive `Retry-After` (>= 1 s) exceed it and abandon.
    /// Use whole-second values; `ZERO` is the explicit "ignore the header" opt-out.
    pub retry_after_cap: Duration,
    /// Hard cap on the total retry time. `Duration::MAX` disables it.
    ///
    /// `Duration::ZERO` is **not** the opt-out: since the check is reactive
    /// (`elapsed >= budget` before each attempt), a zero budget is already met
    /// on the first check and yields **zero retries** — it behaves like
    /// `max_retries = 0`, not "start now". It is legitimate but almost always a
    /// mistake, and unlike the other zero-valued settings it emits no warning of
    /// its own — the symptom shows up at runtime. For "no cap" use `Duration::MAX`.
    pub operation_budget: Duration,
    /// Classes that use **flat** backoff instead of exponential.
    pub flat_classes: Vec<RetryClass>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: DEFAULT_MAX_RETRIES,
            base_delay: DEFAULT_BASE_DELAY,
            cap: DEFAULT_CAP,
            retry_after_cap: DEFAULT_RETRY_AFTER_CAP,
            operation_budget: DEFAULT_OPERATION_BUDGET,
            flat_classes: vec![RetryClass::Timeout, RetryClass::Network],
        }
    }
}

impl RetryConfig {
    /// Returns the detected dangerous combinations, in human-readable text.
    ///
    /// **We warn, we do not correct**: bounding the consumer's configuration
    /// "for their own good" would change behavior silently — exactly the problem
    /// we want to avoid.
    pub(crate) fn dangerous_settings(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.base_delay.is_zero() {
            out.push(format!(
                "base_delay = 0: {} consecutive retries with no pause and NO JITTER",
                self.max_retries
            ));
        }
        if self.retry_after_cap.is_zero() {
            // Same principle as the other two zeros: it SILENTLY disables honoring
            // `Retry-After` entirely — not even an unintelligible header aborts.
            // Legitimate (it is the explicit opt-out) but almost always a mistake.
            out.push(
                "retry_after_cap = 0: the `Retry-After` header is ignored entirely".to_string(),
            );
        }
        if self.cap.is_zero() {
            out.push("cap = 0: every wait is zero, no pause and NO JITTER".to_string());
        }
        if self.retry_after_cap > self.operation_budget {
            // The budget is checked reactively (before each attempt), so a honored
            // `Retry-After` sleep of up to `retry_after_cap` is NOT interrupted. If
            // that cap exceeds the whole budget, a single honored wait can overrun
            // the budget by the difference. Legitimate but almost always a mistake.
            out.push(format!(
                "retry_after_cap ({:?}) > operation_budget ({:?}): a single honored Retry-After can overrun the budget",
                self.retry_after_cap, self.operation_budget
            ));
        }
        out
    }

    // NOTE: there is no `dangerous_settings_with_timeout`. See the retry loop:
    // the "budget < timeout" condition is detected by its **runtime symptom**,
    // not by comparing config (the trait does not expose the wrapped timeout).
}

/// Opt-in retry wrapper for any `LlmProvider`.
///
/// # Concurrency
///
/// Shared across tasks via `Arc` and `complete` takes `&self`: it has **no
/// interior mutable state** — no RNG, no counters. All per-attempt state lives
/// on the stack. *Putting the RNG in a field would force a `Mutex` and serialize
/// concurrent callers exactly where the jitter exists to make them independent.*
///
/// Implements `LlmProvider` itself, making it transparent to consumers.
pub struct RetryProvider {
    inner: Arc<dyn LlmProvider>,
    config: RetryConfig,
}

impl RetryProvider {
    /// Creates a `RetryProvider` with the default configuration.
    ///
    /// # Parameters
    /// - `inner`: The provider to wrap with retry logic.
    pub fn new(inner: Arc<dyn LlmProvider>) -> Self {
        Self::with_config(inner, RetryConfig::default())
    }

    /// Creates a `RetryProvider` with explicit configuration.
    ///
    /// Emits a `tracing` warning if the combination is dangerous (an internal
    /// `dangerous_settings` check). The warning is emitted **once per
    /// constructed provider**: since the config is immutable, no `AtomicBool` or
    /// rate-limit is needed.
    ///
    /// # Parameters
    /// - `inner`: The provider to wrap with retry logic.
    /// - `config`: The retry configuration.
    pub fn with_config(inner: Arc<dyn LlmProvider>, config: RetryConfig) -> Self {
        for warning in config.dangerous_settings() {
            tracing::warn!(target: "magi_core::retry", "{warning}");
        }
        Self { inner, config }
    }

    /// Read-only access to the effective configuration.
    pub fn config(&self) -> &RetryConfig {
        &self.config
    }
}

/// Determines whether a `ProviderError` is transient and should be retried.
///
/// Retryable errors:
/// - `Timeout`: Provider did not respond in time.
/// - `Network`: DNS, connection refused, etc.
/// - `Http` with a transient status (408, 429, 500, 502, 503, 504). The three
///   new 5xx cover local server cold-start; 408 is a server-side request timeout.
///
/// Non-retryable errors:
/// - `Auth`: Invalid credentials won't become valid on retry.
/// - `Process`: CLI subprocess failure.
/// - `NestedSession`: Structural environment issue.
/// - `Http` with any other status code (e.g., 400, 403, 404).
fn is_retryable(error: &ProviderError) -> bool {
    match error {
        ProviderError::Timeout { .. } | ProviderError::Network { .. } => true,
        ProviderError::Http { status, .. } => TRANSIENT_STATUSES.contains(status),
        // Exhaustive on purpose — the catch-all this replaced would have silently classified any
        // new variant as "never retry", which for a transient shape is the wrong answer and would
        // fail no test. A new variant must now break the build until someone decides.
        ProviderError::Auth { .. }
        | ProviderError::Process { .. }
        | ProviderError::NestedSession
        | ProviderError::RetryAbandoned { .. }
        // A server that sent an oversized body will send it again: retrying spends budget the
        // rotation needs to try a DIFFERENT lineage.
        | ProviderError::ResponseTooLarge { .. } => false,
    }
}

/// Marker appended when text is cut, so a truncated message never reads as a complete one.
pub(crate) const TRUNCATION_MARKER: &str = " … (truncated)";

/// Upper bound for a composed transport error message, in bytes.
pub(crate) const MAX_TRANSPORT_MESSAGE_BYTES: usize = 2000;

/// Builds a provider error message from parts this crate controls.
///
/// # Why compose instead of interpolating the client's error
///
/// An HTTP client's error text embeds the URL it was given, which may carry credentials. Composing
/// from an already-redacted rendering plus the *causes* keeps every bit of diagnostic value and
/// drops exactly the part that can leak.
///
/// # Why the order matters
///
/// Operation and endpoint come first, so when the cap bites it eats the tail of the cause chain —
/// the least critical part — and never the endpoint, which is the first thing a reader needs.
pub(crate) fn compose_transport_message(op: &str, redacted_url: &str, cause_chain: &str) -> String {
    let head = format!("{op} for {redacted_url}");
    if cause_chain.is_empty() {
        return head;
    }
    let full = format!("{head}: {cause_chain}");
    if full.len() <= MAX_TRANSPORT_MESSAGE_BYTES {
        return full;
    }
    // Reserve the marker inside the budget: a cap its own suffix can exceed lies about its name.
    let budget = MAX_TRANSPORT_MESSAGE_BYTES.saturating_sub(TRUNCATION_MARKER.len());
    let cut = full.floor_char_boundary(budget);
    format!("{}{TRUNCATION_MARKER}", &full[..cut])
}

/// Joins an error's `source()` chain, **starting at the first source**.
///
/// The top-level error is skipped on purpose: an HTTP client's `Display` interpolates the URL. Its
/// causes (transport, I/O) describe the failure without it.
///
/// Lives here rather than in the provider-URL module because it takes `&dyn Error` — it touches no
/// HTTP type — and it must be reachable when only the Claude feature is enabled, which does not
/// compile that module. Duplicating it would put two definitions on the one path that produces
/// error text.
pub(crate) fn cause_chain(e: &dyn std::error::Error) -> String {
    let mut parts = Vec::new();
    let mut cur = e.source();
    while let Some(c) = cur {
        parts.push(c.to_string());
        cur = c.source();
    }
    parts.join(": ")
}

/// Describes a deserialization failure.
///
/// Typed on purpose: it accepts **only** a serde error, so it is structurally impossible to feed it
/// a network error whose text embeds a URL. That is what lets the CI check forbid interpolating an
/// error inside provider code without carving out an exception — the safe case has a name.
pub(crate) fn describe_parse_error(e: &serde_json::Error) -> String {
    e.to_string()
}

/// Builds the error for a client that could not be constructed.
///
/// Separate from [`to_provider_error`] because no request exists yet — there is no URL to redact
/// and nothing to compose against. It lives here, with the other constructors, so that **no
/// provider file builds a transport variant at all**: that is what lets the CI check be a flat
/// prohibition instead of a rule with exceptions.
#[cfg(any(feature = "claude-api", feature = "openai-compat"))]
pub(crate) fn client_build_error(e: &reqwest::Error) -> ProviderError {
    ProviderError::Network {
        message: format!("failed to build HTTP client: {}", cause_chain(e)),
    }
}

/// Turns a transport failure into this crate's error type.
///
/// **This is the only place a transport [`ProviderError`] is built from a client error.** That is
/// what lets the rule "provider files never construct transport errors" be unconditional — including
/// for a provider whose URL is a constant with no secret. A conditional rule is one someone applies
/// wrong.
#[cfg(any(feature = "claude-api", feature = "openai-compat"))]
pub(crate) fn to_provider_error(op: &str, redacted_url: &str, e: &reqwest::Error) -> ProviderError {
    let message = compose_transport_message(op, redacted_url, &cause_chain(e));
    if e.is_timeout() {
        ProviderError::Timeout { message }
    } else {
        ProviderError::Network { message }
    }
}

/// HTTP statuses considered transient (worth retrying).
const TRANSIENT_STATUSES: &[u16] = &[408, 429, 500, 502, 503, 504];

/// Maps a [`ProviderError`] to its [`RetryClass`].
///
/// # Compiler-enforced synchronization
///
/// The `match` is exhaustive on purpose. `ProviderError` is `#[non_exhaustive]`
/// for external consumers, but **not within the crate**: adding a variant
/// **breaks compilation here** until it is mapped. Without this, a new class
/// would silently fall into the wrong backoff path.
pub(crate) fn classify(err: &ProviderError) -> RetryClass {
    match err {
        ProviderError::Timeout { .. } => RetryClass::Timeout,
        ProviderError::Network { .. } => RetryClass::Network,
        ProviderError::Http { .. } => RetryClass::Http,
        ProviderError::Auth { .. } => RetryClass::Auth,
        ProviderError::Process { .. } => RetryClass::Process,
        ProviderError::NestedSession => RetryClass::NestedSession,
        ProviderError::RetryAbandoned { .. } => RetryClass::RetryAbandoned,
        ProviderError::ResponseTooLarge { .. } => RetryClass::ResponseTooLarge,
    }
}

#[async_trait::async_trait]
impl LlmProvider for RetryProvider {
    async fn complete(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        config: &CompletionConfig,
    ) -> Result<String, ProviderError> {
        let started = std::time::Instant::now();
        let mut last_error: Option<ProviderError> = None;

        for attempt in 0..=self.config.max_retries {
            // Reactive budget check, before each new attempt.
            if attempt > 0 {
                let elapsed = started.elapsed();
                if elapsed >= self.config.operation_budget {
                    // If the budget is exhausted already on the first check, a
                    // single attempt consumed it whole: almost always
                    // `operation_budget < provider timeout`. Detected by SYMPTOM,
                    // not by comparing config: the `LlmProvider` trait does not
                    // expose the wrapped timeout, so a construction-time comparison
                    // would be unreachable code.
                    if attempt == 1 {
                        tracing::warn!(
                            target: "magi_core::retry",
                            ?elapsed,
                            budget = ?self.config.operation_budget,
                            "operation_budget exhausted by a SINGLE attempt: no retry will ever happen. Is the budget smaller than one attempt plus its backoff (e.g. the provider timeout)?"
                        );
                    }
                    tracing::warn!(
                        target: "magi_core::retry",
                        ?elapsed,
                        budget = ?self.config.operation_budget,
                        attempts = attempt,
                        "operation budget exhausted; abandoning retries"
                    );
                    return Err(ProviderError::RetryAbandoned {
                        reason: AbandonReason::OperationBudgetExhausted {
                            elapsed,
                            budget: self.config.operation_budget,
                        },
                        attempts: attempt,
                    });
                }
            }

            let err = match self
                .inner
                .complete(system_prompt, user_prompt, config)
                .await
            {
                Ok(response) => return Ok(response),
                Err(e) => e,
            };

            if !is_retryable(&err) || attempt == self.config.max_retries {
                return Err(err);
            }

            // Interpret the Retry-After HERE: the only point that knows the
            // configured cap. A present but non-honorable header ABANDONS.
            let retry_after = match &err {
                ProviderError::Http {
                    retry_after_raw,
                    received_at,
                    ..
                } => match crate::backoff::parse_retry_after(
                    retry_after_raw.as_slice(),
                    self.config.retry_after_cap,
                ) {
                    crate::backoff::RetryAfter::Absent => None,
                    crate::backoff::RetryAfter::Honor(asked) => {
                        // C3.1: discount the time elapsed since the headers were
                        // received, with SATURATING subtraction (never negative).
                        Some(match received_at {
                            Some(t) => asked.saturating_sub(t.elapsed()),
                            None => asked,
                        })
                    }
                    crate::backoff::RetryAfter::TooLong { requested } => {
                        tracing::warn!(
                            target: "magi_core::retry",
                            ?requested,
                            cap = ?self.config.retry_after_cap,
                            "server asked to wait longer than retry_after_cap; abandoning"
                        );
                        return Err(ProviderError::RetryAbandoned {
                            reason: AbandonReason::RetryAfterTooLong {
                                requested,
                                cap: self.config.retry_after_cap,
                            },
                            attempts: attempt + 1,
                        });
                    }
                    crate::backoff::RetryAfter::Unintelligible { raw } => {
                        tracing::warn!(
                            target: "magi_core::retry",
                            raw = %raw,
                            "Retry-After present but uninterpretable; abandoning"
                        );
                        return Err(ProviderError::RetryAbandoned {
                            reason: AbandonReason::RetryAfterUnintelligible { raw },
                            attempts: attempt + 1,
                        });
                    }
                },
                _ => None,
            };

            let mut rand = || fastrand::f64();
            let wait = crate::backoff::next_backoff(
                attempt,
                classify(&err),
                self.config.base_delay,
                self.config.cap,
                &self.config.flat_classes,
                retry_after,
                &mut rand,
            );

            tracing::debug!(
                target: "magi_core::retry",
                attempt,
                ?wait,
                honored_retry_after = retry_after.is_some(),
                "transient error; backing off before retry"
            );
            last_error = Some(err);
            tokio::time::sleep(wait).await;
        }

        // Unreachable by construction: the loop is only left AFTER at least one
        // failed attempt, and every failure assigns `last_error`. The
        // `debug_assert!` makes it visible in dev if someone restructures the
        // loop; in release it degrades to an honest error rather than panicking,
        // because a library must not tear down the consumer's process for its own
        // bug (§Error handling: `panic!` only for the unrecoverable).
        debug_assert!(
            last_error.is_some(),
            "the loop exited without recording any error: check the exit condition"
        );
        Err(last_error.unwrap_or(ProviderError::Network {
            message: "retry loop ended without an attempt".to_string(),
        }))
    }

    fn name(&self) -> &str {
        self.inner.name()
    }

    fn model(&self) -> &str {
        self.inner.model()
    }
}

#[cfg(test)]
mod message_composition_tests {
    use super::*;

    #[test]
    fn an_oversized_response_is_not_retryable_and_has_its_own_class() {
        let e = ProviderError::ResponseTooLarge { limit: 1024 };
        assert!(
            !is_retryable(&e),
            "a server that sent 1 MiB will send it again"
        );
        assert_eq!(classify(&e), RetryClass::ResponseTooLarge);
    }

    #[test]
    fn compose_puts_operation_and_url_first_and_truncates_the_cause_tail() {
        let long_cause = "x".repeat(MAX_TRANSPORT_MESSAGE_BYTES * 2);
        let msg = compose_transport_message("request failed", "http://h/v1", &long_cause);
        assert!(
            msg.len() <= MAX_TRANSPORT_MESSAGE_BYTES,
            "capped: {}",
            msg.len()
        );
        assert!(msg.starts_with("request failed"), "operation first: {msg}");
        assert!(
            msg.contains("http://h/v1"),
            "endpoint survives truncation: {msg}"
        );
        assert!(msg.contains("truncated"), "the cut is announced: {msg}");
    }

    #[test]
    fn compose_does_not_truncate_when_under_the_cap() {
        let msg = compose_transport_message("request failed", "http://h/v1", "connection refused");
        assert!(msg.contains("connection refused"));
        assert!(
            !msg.contains("truncated"),
            "no marker when nothing was cut: {msg}"
        );
    }

    #[test]
    fn compose_never_panics_on_multibyte_boundaries() {
        let cause = "ñ".repeat(MAX_TRANSPORT_MESSAGE_BYTES * 2);
        let msg = compose_transport_message("op", "http://h", &cause);
        assert!(msg.len() <= MAX_TRANSPORT_MESSAGE_BYTES);
    }

    #[test]
    fn cause_chain_skips_the_top_level_error() {
        use std::fmt;

        #[derive(Debug)]
        struct Top;
        #[derive(Debug)]
        struct Inner;
        impl fmt::Display for Top {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "for url (http://u:p@h)")
            }
        }
        impl fmt::Display for Inner {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "connection refused")
            }
        }
        impl std::error::Error for Inner {}
        impl std::error::Error for Top {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&Inner)
            }
        }

        let chain = cause_chain(&Top);
        assert!(
            chain.contains("connection refused"),
            "sources kept: {chain}"
        );
        assert!(
            !chain.contains("http://u:p@h"),
            "top-level Display excluded: {chain}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    // -- Test providers for the retry loop (Task 7) --

    /// Always fails with the given error. Counts invocations.
    struct FailingProvider {
        error: ProviderError,
        calls: AtomicUsize,
    }

    impl FailingProvider {
        fn new(error: ProviderError) -> Self {
            Self {
                error,
                calls: AtomicUsize::new(0),
            }
        }
        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for FailingProvider {
        async fn complete(
            &self,
            _s: &str,
            _u: &str,
            _c: &CompletionConfig,
        ) -> Result<String, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Err(self.error.clone())
        }
        fn name(&self) -> &str {
            "failing"
        }
        fn model(&self) -> &str {
            "failing"
        }
    }

    /// Like `FailingProvider`, but **delays** before failing: used to exhaust the
    /// `operation_budget` with a controlled number of attempts. Also records the
    /// peak concurrency observed (deterministic, no timing asserts).
    struct SlowFailingProvider {
        delay: Duration,
        calls: AtomicUsize,
        in_flight: AtomicUsize,
        peak_in_flight: AtomicUsize,
    }

    impl SlowFailingProvider {
        fn new(delay: Duration) -> Self {
            Self {
                delay,
                calls: AtomicUsize::new(0),
                in_flight: AtomicUsize::new(0),
                peak_in_flight: AtomicUsize::new(0),
            }
        }
        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
        fn peak_in_flight(&self) -> usize {
            self.peak_in_flight.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for SlowFailingProvider {
        async fn complete(
            &self,
            _s: &str,
            _u: &str,
            _c: &CompletionConfig,
        ) -> Result<String, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak_in_flight.fetch_max(now, Ordering::SeqCst);
            tokio::time::sleep(self.delay).await;
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            Err(ProviderError::Network {
                message: "slow fail".to_string(),
            })
        }
        fn name(&self) -> &str {
            "slow-failing"
        }
        fn model(&self) -> &str {
            "slow-failing"
        }
    }

    /// Returns a 429 with the given `Retry-After` headers and, after `fail_times`
    /// failures, responds with success. The only mock that exercises the
    /// `Retry-After` honouring path end to end.
    struct RetryAfterProvider {
        headers: Vec<String>,
        fail_times: usize,
        calls: AtomicUsize,
    }

    impl RetryAfterProvider {
        fn new(headers: Vec<String>, fail_times: usize) -> Self {
            Self {
                headers,
                fail_times,
                calls: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for RetryAfterProvider {
        async fn complete(
            &self,
            _s: &str,
            _u: &str,
            _c: &CompletionConfig,
        ) -> Result<String, ProviderError> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            if n >= self.fail_times {
                return Ok("ok".to_string());
            }
            Err(ProviderError::Http {
                status: 429,
                body: String::new(),
                retry_after_raw: self.headers.clone(),
                received_at: Some(Instant::now()),
            })
        }
        fn name(&self) -> &str {
            "retry-after"
        }
        fn model(&self) -> &str {
            "retry-after"
        }
    }

    #[test]
    fn test_is_retryable_covers_the_eight_transient_cases() {
        for status in [408u16, 429, 500, 502, 503, 504] {
            assert!(
                is_retryable(&ProviderError::Http {
                    status,
                    body: String::new(),
                    retry_after_raw: vec![],
                    received_at: None
                }),
                "status {status} must be transient"
            );
        }
        assert!(is_retryable(&ProviderError::Timeout {
            message: String::new()
        }));
        assert!(is_retryable(&ProviderError::Network {
            message: String::new()
        }));
    }

    #[test]
    fn test_is_retryable_rejects_non_transient() {
        for status in [400u16, 403, 404] {
            assert!(!is_retryable(&ProviderError::Http {
                status,
                body: String::new(),
                retry_after_raw: vec![],
                received_at: None
            }));
        }
        assert!(!is_retryable(&ProviderError::Auth {
            message: String::new()
        }));
    }

    #[tokio::test]
    async fn test_max_retries_zero_does_not_retry() {
        let inner = Arc::new(FailingProvider::new(ProviderError::Network {
            message: "fail".into(),
        }));
        let p = RetryProvider::with_config(
            inner.clone(),
            RetryConfig {
                max_retries: 0,
                ..Default::default()
            },
        );
        let _ = p.complete("s", "u", &CompletionConfig::default()).await;
        assert_eq!(inner.calls(), 1, "only the initial request");
    }

    #[tokio::test]
    async fn test_base_zero_with_three_retries_emits_exactly_four_requests() {
        // S2 / B7 end-to-end: `base_delay = 0` does not sleep, but the burst is
        // BOUNDED to `max_retries + 1`.
        let inner = Arc::new(FailingProvider::new(ProviderError::Network {
            message: "fail".into(),
        }));
        let p = RetryProvider::with_config(
            inner.clone(),
            RetryConfig {
                base_delay: Duration::ZERO,
                max_retries: 3,
                ..Default::default()
            },
        );
        let _ = p.complete("s", "u", &CompletionConfig::default()).await;
        assert_eq!(inner.calls(), 4, "1 initial + 3 retries, no infinite loop");
    }

    #[tokio::test]
    async fn test_budget_exhaustion_abandons_with_typed_reason() {
        let inner = Arc::new(SlowFailingProvider::new(Duration::from_millis(50)));
        let p = RetryProvider::with_config(
            inner,
            RetryConfig {
                operation_budget: Duration::from_millis(10),
                base_delay: Duration::ZERO,
                ..Default::default()
            },
        );
        let err = p
            .complete("s", "u", &CompletionConfig::default())
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                ProviderError::RetryAbandoned {
                    reason: AbandonReason::OperationBudgetExhausted { .. },
                    ..
                }
            ),
            "expected budget abandonment, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_retry_after_beyond_cap_abandons_with_typed_reason() {
        let inner = Arc::new(RetryAfterProvider::new(vec!["600".to_string()], 1));
        let p = RetryProvider::new(inner);
        let err = p
            .complete("s", "u", &CompletionConfig::default())
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                ProviderError::RetryAbandoned {
                    reason: AbandonReason::RetryAfterTooLong { .. },
                    ..
                }
            ),
            "{err}"
        );
    }

    #[tokio::test]
    async fn test_single_attempt_budget_exhaustion_is_announced() {
        // Budget smaller than one attempt's duration: NEVER a retry.
        let inner = Arc::new(SlowFailingProvider::new(Duration::from_millis(80)));
        let provider = RetryProvider::with_config(
            inner.clone(),
            RetryConfig {
                operation_budget: Duration::from_millis(10),
                ..Default::default()
            },
        );
        let err = provider
            .complete("s", "u", &CompletionConfig::default())
            .await
            .unwrap_err();
        assert_eq!(
            inner.calls(),
            1,
            "one attempt: the budget cuts before the second"
        );
        assert!(
            matches!(
                err,
                ProviderError::RetryAbandoned {
                    reason: AbandonReason::OperationBudgetExhausted { .. },
                    attempts: 1,
                }
            ),
            "{err}"
        );
    }

    #[tokio::test]
    async fn test_operation_budget_zero_yields_single_attempt() {
        // E3.1 exact edge: `operation_budget = ZERO` -> `elapsed >= 0` is met on
        // the first check -> ZERO retries, behaves like `max_retries = 0`.
        let inner = Arc::new(FailingProvider::new(ProviderError::Network {
            message: "x".into(),
        }));
        let provider = RetryProvider::with_config(
            inner.clone(),
            RetryConfig {
                operation_budget: Duration::ZERO,
                ..Default::default()
            },
        );
        let _ = provider
            .complete("s", "u", &CompletionConfig::default())
            .await
            .unwrap_err();
        assert_eq!(inner.calls(), 1, "budget ZERO: one attempt, no retries");
    }

    #[tokio::test]
    async fn test_honored_retry_after_can_overrun_a_small_budget() {
        // retry_after_cap (5s) > operation_budget (50ms): the reactive budget does
        // NOT clamp the honored `Retry-After` sleep, so a single honored wait
        // overruns the budget and the NEXT reactive check abandons. Pins the
        // documented "reactive, not a hard cap" behavior (and the config that the
        // new dangerous_settings warning flags).
        let inner = Arc::new(RetryAfterProvider::new(vec!["1".to_string()], 1));
        let cfg = RetryConfig {
            operation_budget: Duration::from_millis(50),
            retry_after_cap: Duration::from_secs(5),
            ..Default::default()
        };
        let p = RetryProvider::with_config(inner, cfg);
        let start = Instant::now();
        let err = p
            .complete("s", "u", &CompletionConfig::default())
            .await
            .unwrap_err();
        let elapsed = start.elapsed();
        assert!(
            matches!(
                err,
                ProviderError::RetryAbandoned {
                    reason: AbandonReason::OperationBudgetExhausted { .. },
                    ..
                }
            ),
            "{err}"
        );
        assert!(
            elapsed >= Duration::from_secs(1),
            "the ~1s honored wait overran the 50ms budget: {elapsed:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_shared_provider_does_not_serialize_callers() {
        // Three concurrent tasks over ONE shared RetryProvider behind Arc. A peak
        // `>= 2` proves it did NOT serialize callers (an internal mutex would give
        // peak 1). Not a formal proof of no interior mutability (R10 is structural,
        // held by type review); asserted `>= 2` (robust) not `== 3`.
        let inner_probe = Arc::new(SlowFailingProvider::new(Duration::from_millis(50)));
        let provider = Arc::new(RetryProvider::with_config(
            Arc::clone(&inner_probe) as Arc<dyn LlmProvider>,
            RetryConfig {
                max_retries: 0,
                ..Default::default()
            },
        ));

        let mut handles = Vec::new();
        for _ in 0..3 {
            let p = Arc::clone(&provider);
            handles.push(tokio::spawn(async move {
                let _ = p.complete("s", "u", &CompletionConfig::default()).await;
            }));
        }
        for h in handles {
            h.await.expect("task joined");
        }

        assert!(
            inner_probe.peak_in_flight() >= 2,
            "the shared provider serialized callers: concurrency peak = {}",
            inner_probe.peak_in_flight()
        );
    }

    #[test]
    fn test_retry_config_defaults() {
        let c = RetryConfig::default();
        assert_eq!(c.max_retries, 3);
        assert_eq!(c.base_delay, Duration::from_secs(1));
        assert_eq!(c.cap, Duration::from_secs(60));
        assert_eq!(c.retry_after_cap, Duration::from_secs(300));
        assert_eq!(c.operation_budget, Duration::from_secs(600));
        assert_eq!(
            c.flat_classes,
            vec![RetryClass::Timeout, RetryClass::Network]
        );
    }

    #[test]
    fn test_dangerous_config_is_announced_for_zero_base_delay() {
        let cfg = RetryConfig {
            base_delay: Duration::ZERO,
            ..Default::default()
        };
        let warnings = cfg.dangerous_settings();
        assert!(
            warnings.iter().any(|w| w.contains("base_delay")),
            "{warnings:?}"
        );
    }

    #[test]
    fn test_dangerous_config_is_announced_for_zero_cap() {
        let cfg = RetryConfig {
            cap: Duration::ZERO,
            ..Default::default()
        };
        let warnings = cfg.dangerous_settings();
        assert!(warnings.iter().any(|w| w.contains("cap")), "{warnings:?}");
    }

    #[test]
    fn test_dangerous_config_is_announced_for_zero_retry_after_cap() {
        // F2: the THREE zeros that silently disable a protection all warn.
        let cfg = RetryConfig {
            retry_after_cap: Duration::ZERO,
            ..Default::default()
        };
        let warnings = cfg.dangerous_settings();
        assert!(
            warnings.iter().any(|w| w.contains("retry_after_cap")),
            "{warnings:?}"
        );
    }

    #[test]
    fn test_dangerous_config_is_announced_for_retry_after_cap_over_budget() {
        // default retry_after_cap (300s) > operation_budget here (100s): a honored
        // Retry-After can overrun the budget (reactive check does not clamp sleeps).
        let cfg = RetryConfig {
            operation_budget: Duration::from_secs(100),
            ..Default::default()
        };
        let warnings = cfg.dangerous_settings();
        assert!(
            warnings.iter().any(|w| w.contains("overrun")),
            "{warnings:?}"
        );
    }

    #[test]
    fn test_default_config_is_silent() {
        assert!(RetryConfig::default().dangerous_settings().is_empty());
    }

    /// classify maps every ProviderError variant to its RetryClass.
    #[test]
    fn test_classify_maps_every_variant() {
        assert_eq!(
            classify(&ProviderError::Timeout {
                message: "t".into()
            }),
            RetryClass::Timeout
        );
        assert_eq!(
            classify(&ProviderError::Network {
                message: "n".into()
            }),
            RetryClass::Network
        );
        assert_eq!(
            classify(&ProviderError::Http {
                status: 503,
                body: String::new(),
                retry_after_raw: vec![],
                received_at: None,
            }),
            RetryClass::Http
        );
        assert_eq!(
            classify(&ProviderError::Auth {
                message: "a".into()
            }),
            RetryClass::Auth
        );
        assert_eq!(
            classify(&ProviderError::Process {
                exit_code: Some(1),
                stderr: "p".into(),
            }),
            RetryClass::Process
        );
        assert_eq!(
            classify(&ProviderError::NestedSession),
            RetryClass::NestedSession
        );
        assert_eq!(
            classify(&ProviderError::RetryAbandoned {
                reason: crate::error::AbandonReason::OperationBudgetExhausted {
                    elapsed: Duration::ZERO,
                    budget: Duration::ZERO,
                },
                attempts: 0,
            }),
            RetryClass::RetryAbandoned
        );
    }

    // -- default_model_for_mode tests (T02) --

    /// Default model for code-review mode is opus per Python v2.2.8 MODE_DEFAULT_MODELS.
    #[test]
    fn test_default_model_for_mode_code_review_is_opus() {
        assert_eq!(default_model_for_mode(Mode::CodeReview), "opus");
    }

    /// Default model for design mode is opus per Python v2.2.8 MODE_DEFAULT_MODELS.
    #[test]
    fn test_default_model_for_mode_design_is_opus() {
        assert_eq!(default_model_for_mode(Mode::Design), "opus");
    }

    /// Default model for analysis mode is opus per Python v2.2.8 MODE_DEFAULT_MODELS.
    /// Note: Python v2.2.3 had sonnet here briefly, reverted to opus in v2.2.5
    /// due to output-length structural failures. See `models.py:39-50` in upstream.
    #[test]
    fn test_default_model_for_mode_analysis_is_opus() {
        assert_eq!(default_model_for_mode(Mode::Analysis), "opus");
    }

    /// Pairing with resolve_claude_alias yields the full model id.
    #[test]
    fn test_default_model_for_mode_composes_with_resolve_claude_alias() {
        let alias = default_model_for_mode(Mode::Analysis);
        let id = resolve_claude_alias(alias).expect("opus alias resolves");
        assert_eq!(id, "claude-opus-4-7");
    }

    /// Manual mock provider for testing.
    struct MockProvider {
        provider_name: String,
        provider_model: String,
        responses: std::sync::Mutex<Vec<Result<String, ProviderError>>>,
        call_count: AtomicU32,
    }

    impl MockProvider {
        fn new(name: &str, model: &str) -> Self {
            Self {
                provider_name: name.to_string(),
                provider_model: model.to_string(),
                responses: std::sync::Mutex::new(Vec::new()),
                call_count: AtomicU32::new(0),
            }
        }

        fn with_responses(
            name: &str,
            model: &str,
            responses: Vec<Result<String, ProviderError>>,
        ) -> Self {
            // Reverse so we can pop from the end (FIFO order)
            let mut reversed = responses;
            reversed.reverse();
            Self {
                provider_name: name.to_string(),
                provider_model: model.to_string(),
                responses: std::sync::Mutex::new(reversed),
                call_count: AtomicU32::new(0),
            }
        }

        fn call_count(&self) -> u32 {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for MockProvider {
        async fn complete(
            &self,
            _system_prompt: &str,
            _user_prompt: &str,
            _config: &CompletionConfig,
        ) -> Result<String, ProviderError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            let mut responses = self.responses.lock().unwrap();
            if let Some(result) = responses.pop() {
                result
            } else {
                Ok("default response".to_string())
            }
        }

        fn name(&self) -> &str {
            &self.provider_name
        }

        fn model(&self) -> &str {
            &self.provider_model
        }
    }

    // -- CompletionConfig tests --

    /// CompletionConfig::default has max_tokens=4096, temperature=0.0.
    #[test]
    fn test_completion_config_default_values() {
        let config = CompletionConfig::default();
        assert_eq!(config.max_tokens, 4096);
        assert!((config.temperature - 0.0).abs() < f64::EPSILON);
    }

    /// CompletionConfig is #[non_exhaustive] — verify Default works and fields accessible.
    #[test]
    fn test_completion_config_is_non_exhaustive() {
        let config = CompletionConfig::default();
        assert_eq!(config.max_tokens, 4096);
        assert!((config.temperature).abs() < f64::EPSILON);
    }

    // -- RetryProvider delegation tests --

    /// RetryProvider wraps inner provider and delegates name().
    #[tokio::test]
    async fn test_retry_provider_delegates_name() {
        let mock = Arc::new(MockProvider::new("test-provider", "test-model"));
        let retry = RetryProvider::new(mock);
        assert_eq!(retry.name(), "test-provider");
    }

    /// RetryProvider wraps inner provider and delegates model().
    #[tokio::test]
    async fn test_retry_provider_delegates_model() {
        let mock = Arc::new(MockProvider::new("test-provider", "test-model"));
        let retry = RetryProvider::new(mock);
        assert_eq!(retry.model(), "test-model");
    }

    // -- RetryProvider retry behavior --

    /// RetryProvider retries on ProviderError::Timeout up to max_retries.
    #[tokio::test]
    async fn test_retry_provider_retries_on_timeout() {
        let mock = Arc::new(MockProvider::with_responses(
            "p",
            "m",
            vec![
                Err(ProviderError::Timeout {
                    message: "t1".into(),
                }),
                Err(ProviderError::Timeout {
                    message: "t2".into(),
                }),
                Ok("success".into()),
            ],
        ));
        let retry = RetryProvider::with_config(
            mock.clone(),
            RetryConfig {
                max_retries: 3,
                base_delay: Duration::from_millis(1),
                ..Default::default()
            },
        );
        let config = CompletionConfig::default();
        let result = retry.complete("sys", "usr", &config).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
        assert_eq!(mock.call_count(), 3);
    }

    /// RetryProvider retries on ProviderError::Http with status 500.
    #[tokio::test]
    async fn test_retry_provider_retries_on_http_500() {
        let mock = Arc::new(MockProvider::with_responses(
            "p",
            "m",
            vec![
                Err(ProviderError::Http {
                    status: 500,
                    body: "err".into(),
                    retry_after_raw: vec![],
                    received_at: None,
                }),
                Ok("ok".into()),
            ],
        ));
        let retry = RetryProvider::with_config(
            mock.clone(),
            RetryConfig {
                max_retries: 3,
                base_delay: Duration::from_millis(1),
                ..Default::default()
            },
        );
        let config = CompletionConfig::default();
        let result = retry.complete("sys", "usr", &config).await;
        assert!(result.is_ok());
        assert_eq!(mock.call_count(), 2);
    }

    /// RetryProvider retries on ProviderError::Http with status 429.
    #[tokio::test]
    async fn test_retry_provider_retries_on_http_429() {
        let mock = Arc::new(MockProvider::with_responses(
            "p",
            "m",
            vec![
                Err(ProviderError::Http {
                    status: 429,
                    body: "rate limit".into(),
                    retry_after_raw: vec![],
                    received_at: None,
                }),
                Ok("ok".into()),
            ],
        ));
        let retry = RetryProvider::with_config(
            mock.clone(),
            RetryConfig {
                max_retries: 3,
                base_delay: Duration::from_millis(1),
                ..Default::default()
            },
        );
        let config = CompletionConfig::default();
        let result = retry.complete("sys", "usr", &config).await;
        assert!(result.is_ok());
        assert_eq!(mock.call_count(), 2);
    }

    /// RetryProvider retries on ProviderError::Network.
    #[tokio::test]
    async fn test_retry_provider_retries_on_network() {
        let mock = Arc::new(MockProvider::with_responses(
            "p",
            "m",
            vec![
                Err(ProviderError::Network {
                    message: "dns".into(),
                }),
                Ok("ok".into()),
            ],
        ));
        let retry = RetryProvider::with_config(
            mock.clone(),
            RetryConfig {
                max_retries: 3,
                base_delay: Duration::from_millis(1),
                ..Default::default()
            },
        );
        let config = CompletionConfig::default();
        let result = retry.complete("sys", "usr", &config).await;
        assert!(result.is_ok());
        assert_eq!(mock.call_count(), 2);
    }

    /// RetryProvider does NOT retry on ProviderError::Auth.
    #[tokio::test]
    async fn test_retry_provider_does_not_retry_on_auth() {
        let mock = Arc::new(MockProvider::with_responses(
            "p",
            "m",
            vec![Err(ProviderError::Auth {
                message: "bad key".into(),
            })],
        ));
        let retry = RetryProvider::with_config(
            mock.clone(),
            RetryConfig {
                max_retries: 3,
                base_delay: Duration::from_millis(1),
                ..Default::default()
            },
        );
        let config = CompletionConfig::default();
        let result = retry.complete("sys", "usr", &config).await;
        assert!(result.is_err());
        assert_eq!(mock.call_count(), 1);
    }

    /// RetryProvider does NOT retry on ProviderError::Process.
    #[tokio::test]
    async fn test_retry_provider_does_not_retry_on_process() {
        let mock = Arc::new(MockProvider::with_responses(
            "p",
            "m",
            vec![Err(ProviderError::Process {
                exit_code: Some(1),
                stderr: "fail".into(),
            })],
        ));
        let retry = RetryProvider::with_config(
            mock.clone(),
            RetryConfig {
                max_retries: 3,
                base_delay: Duration::from_millis(1),
                ..Default::default()
            },
        );
        let config = CompletionConfig::default();
        let result = retry.complete("sys", "usr", &config).await;
        assert!(result.is_err());
        assert_eq!(mock.call_count(), 1);
    }

    /// RetryProvider does NOT retry on ProviderError::NestedSession.
    #[tokio::test]
    async fn test_retry_provider_does_not_retry_on_nested_session() {
        let mock = Arc::new(MockProvider::with_responses(
            "p",
            "m",
            vec![Err(ProviderError::NestedSession)],
        ));
        let retry = RetryProvider::with_config(
            mock.clone(),
            RetryConfig {
                max_retries: 3,
                base_delay: Duration::from_millis(1),
                ..Default::default()
            },
        );
        let config = CompletionConfig::default();
        let result = retry.complete("sys", "usr", &config).await;
        assert!(result.is_err());
        assert_eq!(mock.call_count(), 1);
    }

    /// RetryProvider does NOT retry on ProviderError::Http with 4xx (except 429).
    #[tokio::test]
    async fn test_retry_provider_does_not_retry_on_http_4xx() {
        let mock = Arc::new(MockProvider::with_responses(
            "p",
            "m",
            vec![Err(ProviderError::Http {
                status: 403,
                body: "forbidden".into(),
                retry_after_raw: vec![],
                received_at: None,
            })],
        ));
        let retry = RetryProvider::with_config(
            mock.clone(),
            RetryConfig {
                max_retries: 3,
                base_delay: Duration::from_millis(1),
                ..Default::default()
            },
        );
        let config = CompletionConfig::default();
        let result = retry.complete("sys", "usr", &config).await;
        assert!(result.is_err());
        assert_eq!(mock.call_count(), 1);
    }

    /// RetryProvider returns last error after exhausting retries.
    #[tokio::test]
    async fn test_retry_provider_returns_last_error_after_exhausting_retries() {
        let mock = Arc::new(MockProvider::with_responses(
            "p",
            "m",
            vec![
                Err(ProviderError::Timeout {
                    message: "t1".into(),
                }),
                Err(ProviderError::Timeout {
                    message: "t2".into(),
                }),
                Err(ProviderError::Timeout {
                    message: "t3".into(),
                }),
            ],
        ));
        // max_retries=2 means 1 initial + 2 retries = 3 total attempts
        let retry = RetryProvider::with_config(
            mock.clone(),
            RetryConfig {
                max_retries: 2,
                base_delay: Duration::from_millis(1),
                ..Default::default()
            },
        );
        let config = CompletionConfig::default();
        let result = retry.complete("sys", "usr", &config).await;
        assert!(result.is_err());
        assert_eq!(mock.call_count(), 3);
        match result.unwrap_err() {
            ProviderError::Timeout { message } => assert_eq!(message, "t3"),
            other => panic!("expected Timeout, got: {other}"),
        }
    }

    /// RetryProvider returns success on first successful retry.
    #[tokio::test]
    async fn test_retry_provider_returns_success_on_first_retry() {
        let mock = Arc::new(MockProvider::with_responses(
            "p",
            "m",
            vec![
                Err(ProviderError::Timeout {
                    message: "t1".into(),
                }),
                Ok("recovered".into()),
            ],
        ));
        let retry = RetryProvider::with_config(
            mock.clone(),
            RetryConfig {
                max_retries: 3,
                base_delay: Duration::from_millis(1),
                ..Default::default()
            },
        );
        let config = CompletionConfig::default();
        let result = retry.complete("sys", "usr", &config).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "recovered");
        assert_eq!(mock.call_count(), 2);
    }

    /// RetryProvider default config: 3 retries, 1s delay.
    #[test]
    fn test_retry_provider_default_config() {
        let mock = Arc::new(MockProvider::new("p", "m"));
        let retry = RetryProvider::new(mock);
        assert_eq!(retry.config().max_retries, 3);
        assert_eq!(retry.config().base_delay, Duration::from_secs(1));
    }

    #[test]
    fn test_resolve_claude_alias_opus_returns_claude_opus_4_7() {
        let result = resolve_claude_alias("opus").unwrap();
        assert_eq!(result, "claude-opus-4-7");
    }

    #[test]
    fn test_resolve_claude_alias_sonnet_returns_claude_sonnet_4_6() {
        let result = resolve_claude_alias("sonnet").unwrap();
        assert_eq!(result, "claude-sonnet-4-6");
    }

    #[test]
    fn test_resolve_claude_alias_haiku_returns_claude_haiku_4_5_20251001() {
        let result = resolve_claude_alias("haiku").unwrap();
        assert_eq!(result, "claude-haiku-4-5-20251001");
    }

    /// Consumers who pinned "claude-opus-4-6" from v0.1.x get the string passed through
    /// unchanged — backward compatibility for callers that already resolved the alias.
    #[test]
    fn test_resolve_claude_alias_accepts_literal_claude_opus_4_6_passthrough() {
        // Consumers may have pinned the string "claude-opus-4-6" from v0.1.x;
        // the resolver must pass any string containing "claude-" through unchanged.
        assert_eq!(
            resolve_claude_alias("claude-opus-4-6").unwrap(),
            "claude-opus-4-6"
        );
    }
}
