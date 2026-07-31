// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-07-30

//! Authority over a provider's URL.
//!
//! # The two rules this module exists to enforce
//!
//! 1. **A secret is never stored as a `String`.** The parsed URL lives here and nowhere else, so a
//!    future `derive(Debug)`, `format!`, or logging call on a provider cannot leak it.
//! 2. **A foreign error's `Display`/`Debug` is never interpolated.** The HTTP client's error text
//!    embeds the URL; messages are composed from our own redacted rendering plus the source chain.
//!
//! # What is redacted, and what is not
//!
//! Userinfo (user and password) and **every query value** are replaced; query parameter **names**,
//! scheme, host, port, path and fragment survive — an error that does not say *where* it failed is
//! useless, and half the value of these messages is telling you that you pointed at the wrong
//! endpoint.
//!
//! All query values are redacted rather than a curated list of "secret-looking" names: enumerating
//! names means the next name nobody enumerated leaks. This over-redacts benign parameters (an API
//! version reads as redacted), which is accepted — over-neutralizing is cosmetic, a sanitizer that
//! can be walked past is not.
//!
//! Path and fragment are **out of scope**: no known LLM API authenticates there, and redacting the
//! path would destroy the only diagnostic the message carries.
//!
//! # Two things reviewers keep flagging here, both verified and both fine
//!
//! * **`let`-chains and `floor_char_boundary`** need Rust 1.88 and 1.91 respectively. The crate's
//!   `rust-version` is **1.91**, so both are within the declared floor rather than ahead of it.
//! * **The transport-error rules are gated on `any(claude-api, openai-compat)`**, which looks like
//!   it omits Ollama. It does not: `ollama = ["openai-compat"]`, so enabling Ollama enables the
//!   gate. `cargo clippy --all-targets --features ollama` is clean, which is the empirical form of
//!   the same statement.

use std::fmt;

use crate::error::ProviderError;

/// Replacement for redacted values.
///
/// URL-safe on purpose: a bracketed form would be percent-encoded when the query is rewritten,
/// rendering as `%5B…%5D` and reading like part of the original value.
pub(crate) const REDACTED_PLACEHOLDER: &str = "REDACTED";

/// A provider base URL that can only ever be shown redacted.
///
/// `Debug` is implemented by hand — **never derived**: a derive would print the inner URL, turning
/// the type that closes the leak into the one that opens it. `Hash` is deliberately absent so
/// nobody uses a credential-bearing URL as a map key. Nothing here is serializable: a secret must
/// have no path to a persisted artifact.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ProviderUrl {
    inner: reqwest::Url,
}

impl ProviderUrl {
    /// Parses and validates a base URL (scheme restricted to http/https).
    ///
    /// # Errors
    ///
    /// [`ProviderError::Network`] on a malformed URL or an unsupported scheme.
    ///
    /// The error message **never contains the raw input** — only the parser's payload-free
    /// description or the scheme name. This is the one path no automated check can guard: this
    /// module is the one place allowed to interpolate a parse error, and the redacting type does
    /// not exist yet at that point.
    pub(crate) fn parse(raw: &str) -> Result<Self, ProviderError> {
        let parsed = reqwest::Url::parse(raw).map_err(|e| ProviderError::Network {
            // `e` is a parse error whose variants carry no payload — it cannot echo `raw`.
            message: format!("invalid base_url: {e}"),
        })?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(ProviderError::Network {
                message: format!(
                    "invalid base_url scheme: {} (expected http/https)",
                    parsed.scheme()
                ),
            });
        }
        Ok(Self {
            inner: strip_trailing_slash(parsed),
        })
    }

    /// The only visible rendering of a URL in this crate.
    ///
    /// Built with the URL type's own APIs rather than string concatenation, so percent-encoding
    /// stays the library's problem and the output is the same URL with its secrets struck out.
    pub(crate) fn redacted(&self) -> String {
        let mut url = self.inner.clone();
        if !url.username().is_empty() {
            // Infallible for the http/https schemes `parse` admits; ignoring the result keeps the
            // function total, which matters because it runs while building an error message.
            let _ = url.set_username(REDACTED_PLACEHOLDER);
        }
        if url.password().is_some() {
            let _ = url.set_password(Some(REDACTED_PLACEHOLDER));
        }
        if url.query().is_some() {
            let names: Vec<String> = url.query_pairs().map(|(k, _)| k.into_owned()).collect();
            let mut pairs = url.query_pairs_mut();
            pairs.clear();
            for name in &names {
                pairs.append_pair(name, REDACTED_PLACEHOLDER);
            }
            drop(pairs);
        }
        url.to_string()
    }

    /// Appends path segments, **preserving query and fragment**.
    ///
    /// `Url::join` is wrong here and the mistake is silent: relative resolution replaces the last
    /// segment and **drops the query** — with query-based authentication that discards the
    /// credential, turning a leak into an inexplicable 401.
    ///
    /// Segments are `&'static str` on purpose: a runtime-built `String` — the only vehicle by which
    /// a `..` could arrive — does not compile.
    fn join_path(&self, segs: &[&'static str]) -> reqwest::Url {
        let mut url = self.inner.clone();
        if let Ok(mut path) = url.path_segments_mut() {
            // Drops the empty segment a trailing slash leaves behind, so `/v1` and `/v1/`
            // produce the same endpoint with a single separator.
            path.pop_if_empty();
            for seg in segs {
                path.push(seg);
            }
        }
        url
    }

    /// Derives a new authority with `segs` appended to the path, **keeping everything else** —
    /// credentials, query and fragment included.
    ///
    /// # Parameters
    /// - `segs`: compile-time path segments, same contract as the request builder.
    ///
    /// # Why this exists rather than composing a string
    ///
    /// A caller that needs a sub-path is otherwise tempted to write `format!("{base}/v1")`, and
    /// that is **silently destructive**: `Display` on this type is the *redacted* rendering, so the
    /// string it produces has the real credentials replaced by the placeholder — an inexplicable
    /// 401 — and carries the normalising trailing slash, giving `//v1`.
    ///
    /// That is not hypothetical. It shipped in the Ollama provider and reached code review: the
    /// type that exists to make the leak impossible was being round-tripped through the one method
    /// that rewrites the secret. Deriving authority-to-authority removes the string entirely.
    /// Only the Ollama provider needs this today, so it is gated there: an item compiled into
    /// a feature set that never calls it is dead code under that feature set, and silencing
    /// that with an allow would hide the next one that is genuinely dead.
    #[cfg(feature = "ollama")]
    pub(crate) fn with_segments(&self, segs: &[&'static str]) -> Self {
        Self {
            inner: self.join_path(segs),
        }
    }

    /// Whether the path's last non-empty segment is `seg`.
    ///
    /// # Parameters
    /// - `seg`: the segment to test for, a compile-time constant like every other segment here.
    ///
    /// Returns a `bool` rather than the segment itself on purpose: handing out a `&str` from this
    /// module is the shape the redaction rules forbid, and a caller only ever needs the answer.
    ///
    /// **Case-sensitive, deliberately.** URL paths are case-sensitive, and a server that publishes
    /// `/v1` does not answer at `/V1`; folding the comparison would make this report a match for a
    /// path the daemon would 404. So `…/V1` is treated as an ordinary path segment.
    #[cfg(feature = "ollama")]
    pub(crate) fn ends_with_segment(&self, seg: &'static str) -> bool {
        self.inner
            // The empty-segment filter is belt-and-braces: `parse` canonicalises the trailing
            // slash away, so this only matters for an authority built some other way.
            .path_segments()
            .and_then(|mut segments| segments.rfind(|s| !s.is_empty()))
            .is_some_and(|last| last == seg)
    }

    /// Derives the authority one path level up, keeping everything else.
    ///
    /// At the root this is a no-op — there is nothing above it — which is the right answer rather
    /// than an error: the caller is asking "what is above this", and for a root the honest reply
    /// is "the same place".
    #[cfg(feature = "ollama")]
    pub(crate) fn parent(&self) -> Self {
        let mut url = self.inner.clone();
        if let Ok(mut path) = url.path_segments_mut() {
            path.pop_if_empty();
            path.pop();
        }
        Self { inner: url }
    }

    /// Builds a request against this URL. The raw URL never leaves this module.
    pub(crate) fn request(
        &self,
        client: &reqwest::Client,
        method: reqwest::Method,
        segs: &[&'static str],
    ) -> ProviderRequest {
        ProviderRequest {
            inner: client.request(method, self.join_path(segs)),
            redacted_url: self.redacted(),
        }
    }
}

/// Drops the empty segment ONE trailing slash leaves behind, so `…/v1` and `…/v1/` become the
/// same value.
///
/// One, not all: `…/v1//` keeps an empty segment and still compares unequal. That is cosmetic —
/// the request builder pops another, so the endpoint is identical either way — and it is left
/// alone rather than looped, because a repeated empty segment is a malformed URL the caller should
/// see rather than have quietly repaired.
///
/// Canonicalising at construction is what lets equality mean what it looks like. The endpoints
/// were already identical either way — the request builder pops the empty segment too — but the
/// stored authorities were not, so two providers pointed at the same daemon compared unequal.
fn strip_trailing_slash(mut url: reqwest::Url) -> reqwest::Url {
    if let Ok(mut path) = url.path_segments_mut() {
        path.pop_if_empty();
    }
    url
}

/// Floor for the response-body cap: 1 MiB.
pub(crate) const MAX_RESPONSE_BODY_BYTES: usize = 1 << 20;

/// Worst-case bytes per output token.
///
/// A heuristic, and deliberately generous: fully escaped CJK costs 6 bytes per character and CJK
/// tokenizers yield roughly 1-1.5 characters per token, so ~6-9 bytes/token. 16 keeps about 2x
/// headroom over that worst case. It is a **ratio**, which ages far slower than an absolute size:
/// it changes when tokenization changes, not when a vendor doubles its context window.
///
/// # Measured, not assumed — 2026-07-30
///
/// Response-body bytes divided by the model's own reported completion tokens, over a CJK-heavy
/// prompt chosen to push toward the escaped worst case rather than measure easy English. All eight
/// models this project routes through were measured, across eight distinct vendors:
///
/// | Model | bytes/token |
/// |---|---|
/// | `nemotron-3-super` | 4.05 |
/// | `gpt-oss:120b` | 4.37 |
/// | `gemma4` | 4.76 |
/// | `qwen3.5:397b` | 4.82 |
/// | `kimi-k2.6` | 4.90 |
/// | `glm-5.2` | 4.96 |
/// | `minimax-m3` | 5.12 |
/// | `deepseek-v4-pro` | **5.51** ← worst |
///
/// The acceptance criterion was fixed **before** measuring, so the number could not be fitted to
/// the result: pass if the worst observation is at most half the ceiling. Observed worst is 5.51
/// against a bound of 8.00, so 16 stands with ~2.9x headroom.
///
/// Re-measure when the model pool turns over. This is a ratio, so it survives a vendor doubling a
/// context window; what would move it is a change in how text is tokenized or encoded.
pub(crate) const BYTES_PER_TOKEN_CEILING: usize = 16;

/// Ceiling applied to `max_tokens` before deriving the cap.
///
/// Clamping the input is what keeps the defense bounded from ABOVE. Saturating the product instead
/// would yield `usize::MAX` for an absurd `max_tokens` — a limit that disappears exactly when it is
/// most needed, which is the worst possible failure for a cap.
pub(crate) const MAX_CAPPABLE_TOKENS: usize = 1_048_576;

/// Cap for the diagnostic prefix of an error body.
pub(crate) const MAX_ERROR_BODY_PREFIX_BYTES: usize = 8 * 1024;

/// Appends `chunk` to `acc` unless doing so would EXCEED `cap` (strictly greater), in which case it
/// returns `false` and leaves `acc` untouched.
///
/// Pure and unit-testable on purpose: the streaming readers around it need a live server to
/// exercise, so this is where the memory bound is actually proven.
pub(crate) fn push_within_cap(acc: &mut Vec<u8>, chunk: &[u8], cap: usize) -> bool {
    // `checked_add` guards the (practically impossible, but defensive) usize overflow of
    // `acc.len() + chunk.len()`; an overflow is treated as over-cap.
    match acc.len().checked_add(chunk.len()) {
        Some(total) if total <= cap => {
            acc.extend_from_slice(chunk);
            true
        }
        _ => false,
    }
}

/// The response-body cap, bounded in BOTH directions.
fn body_cap(max_tokens: u32) -> usize {
    let clamped = (max_tokens as usize).min(MAX_CAPPABLE_TOKENS);
    MAX_RESPONSE_BODY_BYTES.max(clamped * BYTES_PER_TOKEN_CEILING)
}

/// Appends the truncation marker, trimming the text first so the total stays within the cap.
///
/// Cutting on a character boundary, never a byte index: this crate has already shipped a release
/// to fix an offset landing inside a codepoint, and a server's error body is the likeliest place
/// for unexpected multi-byte text.
fn mark_truncated(text: &str) -> String {
    let budget = MAX_ERROR_BODY_PREFIX_BYTES.saturating_sub(crate::error::TRUNCATION_MARKER.len());
    // `floor_char_boundary` already clamps past the end, so the `.min` was noise.
    let cut = text.floor_char_boundary(budget);
    format!("{}{}", &text[..cut], crate::error::TRUNCATION_MARKER)
}

/// Truncates diagnostic text at a character boundary, announcing the cut.
fn truncate_diagnostic(raw: &str) -> String {
    if raw.len() <= MAX_ERROR_BODY_PREFIX_BYTES {
        return raw.to_string();
    }
    // Reserve the marker inside the budget, and cut on a character boundary: a byte-index cut is
    // the bug class that already cost this project a release.
    let budget = MAX_ERROR_BODY_PREFIX_BYTES.saturating_sub(crate::error::TRUNCATION_MARKER.len());
    let cut = raw.floor_char_boundary(budget);
    format!("{}{}", &raw[..cut], crate::error::TRUNCATION_MARKER)
}

/// A request that cannot be printed.
///
/// The client's own request builder implements `Debug` and prints the URL, so handing it out would
/// leak the query through any `{:?}`. This wrapper deliberately has no `Debug`.
pub(crate) struct ProviderRequest {
    inner: reqwest::RequestBuilder,
    redacted_url: String,
}

/// A response that cannot be printed. Same reason: the client's response prints its URL too.
pub(crate) struct ProviderResponse {
    inner: reqwest::Response,
    redacted_url: String,
}

impl ProviderRequest {
    /// Attaches a JSON body.
    pub(crate) fn json<T: serde::Serialize + ?Sized>(mut self, body: &T) -> Self {
        self.inner = self.inner.json(body);
        self
    }

    /// Attaches a header.
    pub(crate) fn header(mut self, name: &'static str, value: String) -> Self {
        self.inner = self.inner.header(name, value);
        self
    }

    /// Sends the request, converting any client failure into a composed, redacted error.
    ///
    /// # Errors
    /// [`ProviderError::Timeout`] when the client timeout fired, [`ProviderError::Network`]
    /// otherwise. The message never contains the URL's secrets.
    pub(crate) async fn send(self) -> Result<ProviderResponse, ProviderError> {
        let redacted_url = self.redacted_url;
        match self.inner.send().await {
            Ok(inner) => Ok(ProviderResponse {
                inner,
                redacted_url,
            }),
            // One shared mapper: this module does not build the variant either.
            Err(e) => Err(crate::provider::to_provider_error(
                "request failed",
                &redacted_url,
                &e,
            )),
        }
    }
}

impl ProviderResponse {
    /// HTTP status code.
    pub(crate) fn status(&self) -> u16 {
        self.inner.status().as_u16()
    }

    /// Raw `Retry-After` header values, in arrival order.
    pub(crate) fn retry_after_raw(&self) -> Vec<String> {
        // `get_all`, not `get`: the header may repeat and the first VALID one wins.
        self.inner
            .headers()
            .get_all(reqwest::header::RETRY_AFTER)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .map(str::to_owned)
            .collect()
    }

    /// Reads a body that carries a **verdict**. Over the cap it **fails**.
    ///
    /// It must not truncate: a cut body loses its closing marker, and the verdict parser would
    /// report a truncated *model* output — blaming the model for a cut this reader made, with a
    /// retry that can never fix it.
    ///
    /// # Errors
    /// [`ProviderError::ResponseTooLarge`] over the cap; [`ProviderError::Network`] or
    /// [`ProviderError::Timeout`] if the body cannot be read.
    pub(crate) async fn read_verdict_body(
        mut self,
        max_tokens: u32,
    ) -> Result<String, ProviderError> {
        let cap = body_cap(max_tokens);
        let mut acc: Vec<u8> = Vec::new();
        loop {
            match self.inner.chunk().await {
                // Same pure helper as the probe reader, so the bound is computed in ONE place and
                // stays unit-testable. Only the reaction to hitting it differs, which is the whole
                // difference between these two readers.
                Ok(Some(chunk)) if push_within_cap(&mut acc, &chunk, cap) => {}
                Ok(Some(_)) => return Err(ProviderError::ResponseTooLarge { limit: cap }),
                Ok(None) => break,
                Err(e) => {
                    return Err(crate::provider::to_provider_error(
                        "failed to read response body",
                        &self.redacted_url,
                        &e,
                    ));
                }
            }
        }
        Ok(String::from_utf8_lossy(&acc).into_owned())
    }

    /// Reads a **probe** body: bounded, and degrading to `None` on any problem.
    ///
    /// A third semantics on purpose, and the asymmetry is the point. For a probe, "no capability
    /// information" is a **valid result** — so an over-cap or unreadable body degrades rather than
    /// failing, exactly as it did before this type existed. Truncating would be worse here than in
    /// the diagnostic case: a half-read JSON document does not parse, so a truncated probe body
    /// would look like schema drift instead of an oversized response.
    ///
    /// A `Content-Length` already over the cap is rejected early, compared in `u64` so there is no
    /// `usize` truncation on 32-bit.
    #[cfg(feature = "ollama")]
    pub(crate) async fn read_probe_body(mut self, cap: usize) -> Option<Vec<u8>> {
        if let Some(len) = self.inner.content_length()
            && len > cap as u64
        {
            return None;
        }
        let mut acc: Vec<u8> = Vec::new();
        loop {
            match self.inner.chunk().await {
                // The bound lives in a PURE helper so it stays unit-testable: the HTTP path here
                // cannot be exercised without a server, and inlining the accumulation would have
                // quietly traded away the only coverage the cap logic has.
                Ok(Some(chunk)) if push_within_cap(&mut acc, &chunk, cap) => {}
                Ok(Some(_)) => return None, // over cap (or overflow) → degrade, never OOM
                Ok(None) => return Some(acc),
                Err(_) => return None,
            }
        }
    }

    /// Reads a body that carries **diagnostics**. Over the cap it truncates and says so.
    ///
    /// Asymmetric with [`Self::read_verdict_body`] on purpose, and the difference lives in the
    /// names so that a future "simplification" unifying them has to delete a function a reviewer
    /// can see. Truncating is safe here: there is no verdict a cut could falsify, and dropping a
    /// `500` body whole would discard the only reason that error is read.
    pub(crate) async fn read_diagnostic_body(mut self) -> String {
        let mut acc: Vec<u8> = Vec::new();
        let mut partial = false;
        loop {
            match self.inner.chunk().await {
                // Bounded through the same pure helper as its two siblings, so the cap is tested
                // BEFORE the bytes are taken. Appending first and checking afterwards let a single
                // oversized chunk land in memory whole — the one asymmetry among the three readers
                // that had no reason behind it.
                Ok(Some(chunk))
                    if push_within_cap(&mut acc, &chunk, MAX_ERROR_BODY_PREFIX_BYTES) => {}
                // Over the cap, or the read failed part-way. Both leave a PREFIX, and both must
                // say so — a body cut short that reads as complete makes its last word look like
                // the server's final one.
                Ok(Some(_)) | Err(_) => {
                    partial = true;
                    break;
                }
                Ok(None) => break,
            }
        }
        // Arbitrary server text: lossy conversion keeps the diagnostic instead of turning the
        // server's error into ours.
        let text = truncate_diagnostic(&String::from_utf8_lossy(&acc));
        // `truncate_diagnostic` only marks what IT cut. Rejecting an over-cap chunk leaves `acc`
        // below the limit, so without this the cut would go unannounced — which is the failure
        // this reader is supposed to be immune to.
        //
        // The marker is paid for INSIDE the budget, not appended on top of it. Appending after the
        // fact let the result reach `MAX_ERROR_BODY_PREFIX_BYTES + marker`, so the constant named
        // as the cap was not the cap — a bound that its own annotation can push past is not a
        // bound, and this one is quoted as one in the changelog.
        if partial && !text.ends_with(crate::error::TRUNCATION_MARKER) {
            return mark_truncated(&text);
        }
        text
    }
}

impl fmt::Debug for ProviderUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderUrl")
            .field("url", &self.redacted())
            .finish()
    }
}

impl fmt::Display for ProviderUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.redacted())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacted_hides_userinfo_and_keeps_host() {
        let u = ProviderUrl::parse("http://alice:s3cret@example.com:8080/v1").expect("parses");
        let out = u.redacted();
        assert!(!out.contains("alice"), "username leaked: {out}");
        assert!(!out.contains("s3cret"), "password leaked: {out}");
        assert!(out.contains("example.com"), "host must survive: {out}");
        assert!(out.contains("8080"), "port must survive: {out}");
        assert!(out.contains("/v1"), "path must survive: {out}");
    }

    #[test]
    fn redacted_hides_all_query_values_and_keeps_names() {
        let u = ProviderUrl::parse("http://h/v1?api-version=2024-02-01&key=q3rySecret")
            .expect("parses");
        let out = u.redacted();
        assert!(!out.contains("q3rySecret"), "query secret leaked: {out}");
        assert!(
            !out.contains("2024-02-01"),
            "all query values are redacted: {out}"
        );
        assert!(out.contains("api-version"), "param NAMES survive: {out}");
        assert!(out.contains("key"), "param NAMES survive: {out}");
    }

    #[test]
    fn redacted_placeholder_is_url_safe_and_not_percent_encoded() {
        let u = ProviderUrl::parse("http://h/v1?key=S").expect("parses");
        let out = u.redacted();
        assert!(
            out.contains(REDACTED_PLACEHOLDER),
            "placeholder present: {out}"
        );
        assert!(
            !out.contains('%'),
            "placeholder must not be percent-encoded: {out}"
        );
    }

    #[test]
    fn debug_and_display_are_both_redacted() {
        let u = ProviderUrl::parse("http://alice:s3cret@h/v1").expect("parses");
        assert!(!format!("{u:?}").contains("s3cret"), "Debug leaked");
        assert!(!format!("{u}").contains("s3cret"), "Display leaked");
    }

    #[test]
    fn parse_rejects_non_http_scheme_without_echoing_the_input() {
        let err = ProviderUrl::parse("ftp://alice:s3cret@h/v1").expect_err("scheme rejected");
        let msg = err.to_string();
        assert!(
            !msg.contains("alice") && !msg.contains("s3cret"),
            "parse error leaked: {msg}"
        );
        assert!(
            msg.contains("ftp"),
            "the scheme itself is safe context: {msg}"
        );
    }

    #[test]
    fn parse_error_never_echoes_the_raw_input() {
        let err = ProviderUrl::parse("not a url at all ?key=s3cret").expect_err("malformed");
        let msg = err.to_string();
        assert!(
            !msg.contains("s3cret"),
            "malformed-input error leaked: {msg}"
        );
    }

    #[cfg(feature = "ollama")]
    #[test]
    fn ends_with_segment_is_case_sensitive_and_ignores_a_trailing_slash() {
        let f = |raw: &str| {
            ProviderUrl::parse(raw)
                .expect("parses")
                .ends_with_segment("v1")
        };
        assert!(f("http://h/v1"));
        assert!(
            f("http://h/v1/"),
            "the trailing slash is canonicalised away first"
        );
        assert!(f("http://h/ollama/v1"));
        assert!(
            !f("http://h/V1"),
            "case-sensitive on purpose: a server at /v1 404s at /V1"
        );
        assert!(!f("http://h/v10"));
        assert!(!f("http://h"), "a root has no segment to match");
    }

    #[cfg(feature = "ollama")]
    #[test]
    fn parent_climbs_one_level_and_stops_at_the_root() {
        let p = |raw: &str| ProviderUrl::parse(raw).expect("parses").parent();
        assert_eq!(
            p("http://h/a/b"),
            ProviderUrl::parse("http://h/a").expect("parses")
        );
        assert_eq!(
            p("http://h/v1"),
            ProviderUrl::parse("http://h").expect("parses")
        );
        // The documented no-op. It is the branch a refactor is most likely to break, and the
        // honest answer for "what is above the root" is "the same place", not an error.
        assert_eq!(
            p("http://h"),
            ProviderUrl::parse("http://h").expect("parses")
        );
    }

    #[cfg(feature = "ollama")]
    #[test]
    fn parent_keeps_the_credentials_the_query_and_the_fragment() {
        assert_eq!(
            ProviderUrl::parse("http://u:p@h/base/v1?key=S#frag")
                .expect("parses")
                .parent(),
            ProviderUrl::parse("http://u:p@h/base?key=S#frag").expect("parses")
        );
    }

    /// The memory bound lives in this pure helper precisely so it can be proven without a server,
    /// and until now the proof lived in another module gated on a feature the helper does not need
    /// — so under `--features openai-compat` the function ran with no coverage at all.
    #[test]
    fn push_within_cap_admits_exactly_up_to_the_cap() {
        let mut acc = Vec::new();
        assert!(
            push_within_cap(&mut acc, b"", 4),
            "an empty chunk always fits"
        );
        assert!(push_within_cap(&mut acc, b"abc", 4));
        assert!(
            push_within_cap(&mut acc, b"d", 4),
            "a chunk that exactly fills it fits"
        );
        assert_eq!(acc, b"abcd");
    }

    #[test]
    fn push_within_cap_rejects_one_byte_over_and_leaves_the_buffer_untouched() {
        // Leaving `acc` alone on rejection is what lets the diagnostic reader keep a coherent
        // prefix instead of a partly-appended chunk.
        let mut acc = b"abcd".to_vec();
        assert!(!push_within_cap(&mut acc, b"e", 4));
        assert_eq!(
            acc, b"abcd",
            "a rejected chunk must not be partially applied"
        );
    }

    #[test]
    fn push_within_cap_rejects_a_single_oversized_chunk_from_empty() {
        // The out-of-memory case: the first chunk alone exceeds the cap.
        let mut acc = Vec::new();
        assert!(!push_within_cap(&mut acc, &[0u8; 9], 8));
        assert!(acc.is_empty());
    }

    #[test]
    fn push_within_cap_admits_a_chunk_when_the_cap_is_unbounded() {
        // Named for what it actually checks. An earlier version of this test claimed to exercise
        // the `checked_add` overflow guard and then asserted the opposite of what it observed —
        // the name said "over cap", the assertion said it fits, and the assertion was right.
        //
        // The overflow branch is **unreachable with real buffers**: it would need `acc` and
        // `chunk` to sum past `usize::MAX`, which no allocation can reach. It stays because it
        // decides the ANSWER if it ever could — over cap, never under — and because saturating
        // there would silently say "fits", which is the wrong direction for a bound. That is a
        // property of the code, not something a test can demonstrate, so no test pretends to.
        let mut acc = Vec::new();
        assert!(push_within_cap(&mut acc, b"x", usize::MAX));
        assert_eq!(acc, b"x");
    }

    #[test]
    fn a_partial_read_stays_within_the_cap_including_its_marker() {
        // The bound the marker used to break: appending it after truncation pushed the result to
        // cap + marker, so the constant named as the cap was not the cap.
        let text = "y".repeat(MAX_ERROR_BODY_PREFIX_BYTES);
        let marked = mark_truncated(&text);
        assert!(
            marked.len() <= MAX_ERROR_BODY_PREFIX_BYTES,
            "marked length {} exceeds the cap {MAX_ERROR_BODY_PREFIX_BYTES}",
            marked.len()
        );
        assert!(marked.ends_with(crate::error::TRUNCATION_MARKER));
    }

    #[test]
    fn marking_a_short_text_does_not_pad_it() {
        let marked = mark_truncated("brief");
        assert_eq!(marked, format!("brief{}", crate::error::TRUNCATION_MARKER));
    }

    #[test]
    fn marking_never_splits_a_multibyte_character() {
        // Three-byte characters do not divide the budget evenly, which is the point: a byte-index
        // cut here would panic, and a server's error body is arbitrary text.
        let text = "\u{4f60}".repeat(MAX_ERROR_BODY_PREFIX_BYTES);
        let marked = mark_truncated(&text);
        assert!(marked.len() <= MAX_ERROR_BODY_PREFIX_BYTES);
        assert!(marked.ends_with(crate::error::TRUNCATION_MARKER));
    }

    #[test]
    fn parse_canonicalizes_the_trailing_slash() {
        // Equality has to mean "the same daemon". Before this, two providers pointed at one
        // endpoint compared unequal purely on how the caller had spelled it.
        for (with, without) in [
            ("http://h:11434/v1/", "http://h:11434/v1"),
            ("http://h:11434/", "http://h:11434"),
            ("http://h/a/b/", "http://h/a/b"),
        ] {
            assert_eq!(
                ProviderUrl::parse(with).expect("parses"),
                ProviderUrl::parse(without).expect("parses"),
                "{with} and {without} name the same place"
            );
        }
    }

    #[test]
    fn canonicalizing_does_not_touch_the_query_or_the_fragment() {
        let u = ProviderUrl::parse("http://h/v1/?key=S#frag").expect("parses");
        assert_eq!(
            u,
            ProviderUrl::parse("http://h/v1?key=S#frag").expect("parses")
        );
    }

    #[test]
    fn parse_normalizes_dot_segments() {
        let u = ProviderUrl::parse("http://h/v1/../admin").expect("parses");
        assert!(u.redacted().contains("/admin"));
        assert!(!u.redacted().contains(".."));
    }

    #[test]
    #[cfg(feature = "ollama")]
    fn with_segments_keeps_the_real_credentials() {
        // Equality here compares the FULL url, credentials included — which is exactly why this
        // test can prove they survived without ever printing them. Composing the sub-path as a
        // string instead would have gone through the redacted rendering and produced the literal
        // placeholder as a username.
        let derived = ProviderUrl::parse("http://alice:s3cret@h:11434")
            .expect("parses")
            .with_segments(&["v1"]);
        let expected = ProviderUrl::parse("http://alice:s3cret@h:11434/v1").expect("parses");
        assert_eq!(derived, expected);
    }

    #[test]
    #[cfg(feature = "ollama")]
    fn with_segments_produces_one_separator_not_two() {
        // The other half of the string-composition bug: `Display` normalises an empty path to
        // `/`, so `format!("{base}/v1")` yields `//v1`.
        for raw in ["http://h:11434", "http://h:11434/"] {
            let derived = ProviderUrl::parse(raw)
                .expect("parses")
                .with_segments(&["v1"]);
            assert_eq!(
                derived,
                ProviderUrl::parse("http://h:11434/v1").expect("parses"),
                "from {raw}"
            );
        }
    }

    #[test]
    #[cfg(feature = "ollama")]
    fn with_segments_keeps_the_query_and_the_fragment() {
        let derived = ProviderUrl::parse("http://h/base?key=S#frag")
            .expect("parses")
            .with_segments(&["v1"]);
        assert_eq!(
            derived,
            ProviderUrl::parse("http://h/base/v1?key=S#frag").expect("parses")
        );
    }

    #[test]
    fn join_path_preserves_query_and_appends_segments() {
        let u = ProviderUrl::parse("http://h/v1?key=S").expect("parses");
        let full = u.join_path(&["chat", "completions"]);
        assert_eq!(full.path(), "/v1/chat/completions");
        assert_eq!(full.query(), Some("key=S"));
    }

    #[test]
    fn join_path_is_idempotent_over_trailing_slash() {
        let a = ProviderUrl::parse("http://h/v1")
            .expect("parses")
            .join_path(&["chat"]);
        let b = ProviderUrl::parse("http://h/v1/")
            .expect("parses")
            .join_path(&["chat"]);
        assert_eq!(a.as_str(), b.as_str());
        assert!(
            !a.path().contains("//"),
            "no double separator: {}",
            a.path()
        );
    }

    #[test]
    fn join_path_preserves_fragment() {
        let u = ProviderUrl::parse("http://h/v1#frag").expect("parses");
        assert_eq!(u.join_path(&["chat"]).fragment(), Some("frag"));
    }

    #[test]
    fn body_cap_is_bounded_below_and_above() {
        assert_eq!(
            body_cap(0),
            MAX_RESPONSE_BODY_BYTES,
            "floor applies to a tiny max_tokens"
        );
        assert_eq!(
            body_cap(128_000),
            128_000 * BYTES_PER_TOKEN_CEILING,
            "derived once above the floor"
        );
        assert_eq!(
            body_cap(u32::MAX),
            MAX_CAPPABLE_TOKENS * BYTES_PER_TOKEN_CEILING,
            "an absurd max_tokens must NOT remove the defense"
        );
    }

    #[test]
    fn diagnostic_truncation_is_announced_and_utf8_safe() {
        let raw = "\u{f1}".repeat(MAX_ERROR_BODY_PREFIX_BYTES);
        let out = truncate_diagnostic(&raw);
        assert!(
            out.len() <= MAX_ERROR_BODY_PREFIX_BYTES,
            "capped: {}",
            out.len()
        );
        assert!(out.contains("truncated"), "the cut is announced");
    }

    #[test]
    fn diagnostic_under_the_cap_is_untouched() {
        let out = truncate_diagnostic("upstream said no");
        assert_eq!(out, "upstream said no");
    }

    #[tokio::test]
    async fn send_composes_a_redacted_error_on_connection_failure() {
        let url =
            ProviderUrl::parse("http://alice:s3cret@127.0.0.1:1/v1?key=q3ry").expect("parses");
        let client = reqwest::Client::builder().build().expect("client");
        // `expect_err` would require the Ok type to be `Debug` — and `ProviderResponse`
        // deliberately is not. The invariant has teeth: the test bends, not the type.
        let err = match url
            .request(&client, reqwest::Method::POST, &["chat", "completions"])
            .send()
            .await
        {
            Ok(_) => panic!("expected a connection failure against port 1"),
            Err(e) => e,
        };
        let msg = err.to_string();
        assert!(!msg.contains("alice"), "username leaked: {msg}");
        assert!(!msg.contains("s3cret"), "password leaked: {msg}");
        assert!(!msg.contains("q3ry"), "query secret leaked: {msg}");
        assert!(msg.contains("127.0.0.1"), "host must survive: {msg}");
    }
}
