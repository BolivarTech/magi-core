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
    pub(crate) fn parse(_raw: &str) -> Result<Self, ProviderError> {
        todo!("Task 1 Green")
    }

    /// The only visible rendering of a URL in this crate.
    ///
    /// Built with the URL type's own APIs rather than string concatenation, so percent-encoding
    /// stays the library's problem and the output is the same URL with its secrets struck out.
    pub(crate) fn redacted(&self) -> String {
        todo!("Task 1 Green")
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
        assert!(out.contains(REDACTED_PLACEHOLDER), "placeholder present: {out}");
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
        assert!(msg.contains("ftp"), "the scheme itself is safe context: {msg}");
    }

    #[test]
    fn parse_error_never_echoes_the_raw_input() {
        let err = ProviderUrl::parse("not a url at all ?key=s3cret").expect_err("malformed");
        let msg = err.to_string();
        assert!(!msg.contains("s3cret"), "malformed-input error leaked: {msg}");
    }

    #[test]
    fn parse_normalizes_dot_segments() {
        let u = ProviderUrl::parse("http://h/v1/../admin").expect("parses");
        assert!(u.redacted().contains("/admin"));
        assert!(!u.redacted().contains(".."));
    }
}
