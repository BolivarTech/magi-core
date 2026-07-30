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
        Ok(Self { inner: parsed })
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

    /// Reads the whole body.
    ///
    /// Preserves today's behaviour; the two bounded, asymmetric readers replace it in the
    /// response-cap task.
    ///
    /// # Errors
    /// [`ProviderError::Network`] or [`ProviderError::Timeout`] if the body cannot be read.
    pub(crate) async fn text(self) -> Result<String, ProviderError> {
        let redacted_url = self.redacted_url;
        self.inner.text().await.map_err(|e| {
            crate::provider::to_provider_error("failed to read response body", &redacted_url, &e)
        })
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

    #[test]
    fn parse_normalizes_dot_segments() {
        let u = ProviderUrl::parse("http://h/v1/../admin").expect("parses");
        assert!(u.redacted().contains("/admin"));
        assert!(!u.redacted().contains(".."));
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
