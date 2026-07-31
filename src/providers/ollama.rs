// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-07-26

//! Native Ollama provider (feature `ollama`).
//!
//! Reuses the OpenAI-compatible completions path (`{base}/v1/chat/completions`)
//! for [`LlmProvider::complete`] and ADDS the native probe endpoints for
//! [`ProviderProbe`]:
//!
//! - **window** ← `POST {base}/api/show` → `model_info` → first `*.context_length`
//!   (the key is architecture-prefixed and NOT derivable from `details.family`, so
//!   it is scanned, never built).
//! - **digest** ← `GET {base}/api/tags` → the model's manifest SHA256 (64-char
//!   lowercase hex, no `sha256:` prefix). `/api/show` has NO digest field.
//!
//! Both bodies are untrusted, so each read is bounded by `MAX_SHOW_BODY_BYTES`
//! (`cap_body`); an over-cap or malformed body degrades the probe to `None`
//! (fail-open, trusted by lineage) rather than erroring — only a transport failure
//! surfaces a [`ProviderError`]. HTTP-thin, no new dependencies (`reqwest` is
//! already pulled by the `openai-compat` feature this one enables).

use async_trait::async_trait;

use crate::error::ProviderError;
use crate::provider::{CompletionConfig, DEFAULT_CLIENT_TIMEOUT, LlmProvider};
use crate::providers::openai_compat::OpenAiCompatibleProvider;
use crate::providers::provider_url::ProviderUrl;
use crate::rotation::ProviderProbe;

/// Native Ollama provider: OpenAI-compatible completions + `/api/show` +
/// `/api/tags` probe. Construct with [`OllamaProvider::new`].
pub struct OllamaProvider {
    inner: OpenAiCompatibleProvider,
    /// The URL authority — never a `String`, so a reverse proxy's credentials in front of Ollama
    /// cannot leak through `Debug` or an error message.
    base_url: ProviderUrl,
    client: reqwest::Client,
}

impl OllamaProvider {
    /// Creates a provider for an Ollama daemon. Keyless — Ollama needs no bearer token.
    ///
    /// # Parameters
    /// - `base_url`: **either** the OpenAI-compatible endpoint (`http://localhost:11434/v1`)
    ///   **or** the daemon root (`http://localhost:11434`). Both are accepted and both produce
    ///   the same endpoints.
    /// - `model`: the model tag, passed through unchanged.
    ///
    /// # Errors
    /// [`ProviderError::Network`] on an invalid `base_url`/scheme or client build.
    ///
    /// # Why both spellings are accepted
    ///
    /// Ollama serves its OpenAI-compatible API under `/v1` and its native API under `/api`, and
    /// this provider needs both — the second is what measures the context window and the weights
    /// digest. Only one of the two has to be given, since they are siblings.
    ///
    /// Earlier versions took the daemon root only, and that surprised people: the sibling
    /// [`OpenAiCompatibleProvider`] takes its URL **with** `/v1`, so the same-looking parameter
    /// meant different things in the same crate. Users were not guessing wrong — they were
    /// applying the convention from the provider next door. Accepting both removes the choice
    /// rather than documenting it harder.
    ///
    /// [`OpenAiCompatibleProvider`]: crate::providers::openai_compat::OpenAiCompatibleProvider
    ///
    /// | Given | Completions | Probe |
    /// |---|---|---|
    /// | `http://localhost:11434/v1` | `…/v1/chat/completions` | `…/api/show`, `…/api/tags` |
    /// | `http://localhost:11434` | `…/v1/chat/completions` | `…/api/show`, `…/api/tags` |
    /// | `https://gw.example.com/ollama/v1` | `…/ollama/v1/chat/completions` | `…/ollama/api/*` |
    ///
    /// # The one deployment this reads wrong
    ///
    /// A daemon whose root genuinely ends in a segment named `v1` — say
    /// `https://gw/tenants/v1` — is taken for the OpenAI prefix, so the probe is looked for one
    /// level too high. It fails **loudly**: the probe 404s, the context window comes back
    /// unmeasured, and a strict context guard refuses the model rather than running blind. Pass
    /// such a root through [`OpenAiCompatibleProvider`] instead, which does no probing.
    pub fn new(
        base_url: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self, ProviderError> {
        let given = ProviderUrl::parse(&base_url.into())?;
        // Whichever spelling arrived, normalise to the pair this provider needs.
        let (completions, base) = if given.ends_with_segment(OPENAI_COMPAT_PREFIX) {
            (given.clone(), given.parent())
        } else {
            (given.with_segments(&[OPENAI_COMPAT_PREFIX]), given)
        };
        // Passed as an authority, NEVER as `format!("{base}/v1")`: `Display` here is the redacted
        // rendering, so composing a string would hand the inner provider the literal placeholder
        // in place of real credentials — a silent 401 — plus a doubled path separator.
        let inner = OpenAiCompatibleProvider::from_authority(
            completions,
            model,
            None,
            DEFAULT_CLIENT_TIMEOUT,
        )?;
        let client = reqwest::Client::builder()
            .timeout(DEFAULT_CLIENT_TIMEOUT)
            // Referer OFF — see the note in the OpenAI-compatible provider: on a redirect the
            // client would send the ORIGINAL url, query string included, to the target origin.
            .referer(false)
            .build()
            .map_err(|e| crate::provider::client_build_error(&e))?;
        Ok(Self {
            inner,
            base_url: base,
            client,
        })
    }

    /// Extracts the context window from a `/api/show` JSON body. Scans the
    /// `model_info` object for the FIRST key ending in `.context_length` whose
    /// value is a POSITIVE integer, returning it as `usize`. The key is
    /// architecture-prefixed and NOT derivable from `details.family`, so we scan
    /// the suffix rather than build the key. Total: any malformed input → `None`.
    pub(crate) fn parse_show_window(body: &str) -> Option<usize> {
        let value = serde_json::from_str::<serde_json::Value>(body).ok()?;
        let model_info = value.get("model_info")?.as_object()?;
        model_info.iter().find_map(|(key, value)| {
            if key.ends_with(".context_length") {
                value.as_u64().filter(|&n| n > 0).map(|n| n as usize)
            } else {
                None
            }
        })
    }

    /// Extracts the manifest digest for `model` from a `/api/tags` JSON body:
    /// finds the `models[]` entry whose `name` equals `model` and returns its
    /// `digest` IFF it is a 64-char lowercase-hex string. Total: absent model,
    /// missing/short/non-hex digest, or malformed JSON → `None`.
    pub(crate) fn parse_tags_digest(body: &str, model: &str) -> Option<String> {
        let value = serde_json::from_str::<serde_json::Value>(body).ok()?;
        let models = value.get("models")?.as_array()?;
        models
            .iter()
            .find(|entry| entry.get("name").and_then(|n| n.as_str()) == Some(model))
            .and_then(|entry| {
                let digest = entry.get("digest")?.as_str()?;
                if digest.len() == 64
                    && digest
                        .chars()
                        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
                {
                    Some(digest.to_string())
                } else {
                    None
                }
            })
    }
}

/// Hard cap on an untrusted Ollama response body (`/api/show`, `/api/tags`), in
/// bytes. A real body is a few KB; anything larger is rejected (probe → `None`)
/// rather than accumulated, preventing memory exhaustion.
pub(crate) const MAX_SHOW_BODY_BYTES: usize = 1 << 20; // 1 MiB

/// Path segment under which Ollama serves its OpenAI-compatible API, alongside the native `/api`.
const OPENAI_COMPAT_PREFIX: &str = "v1";

#[async_trait]
impl LlmProvider for OllamaProvider {
    async fn complete(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        config: &CompletionConfig,
    ) -> Result<String, ProviderError> {
        self.inner
            .complete(system_prompt, user_prompt, config)
            .await
    }

    fn name(&self) -> &str {
        "ollama"
    }

    fn model(&self) -> &str {
        self.inner.model()
    }
}

#[async_trait]
impl ProviderProbe for OllamaProvider {
    async fn window(&self) -> Result<Option<usize>, ProviderError> {
        let resp = self
            .base_url
            .request(&self.client, reqwest::Method::POST, &["api", "show"])
            .json(&serde_json::json!({ "model": self.inner.model() }))
            .send()
            .await?;
        // A non-2xx status carries no usable probe body → degrade to `None`
        // (fail-open) without reading it, rather than parse an error page.
        if !(200..300).contains(&resp.status()) {
            return Ok(None);
        }
        match resp.read_probe_body(MAX_SHOW_BODY_BYTES).await {
            Some(bytes) => {
                let body = String::from_utf8_lossy(&bytes);
                let window = Self::parse_show_window(&body);
                if window.is_none() {
                    tracing::warn!(
                        model = self.inner.model(),
                        "/api/show returned no *.context_length key (schema drift or absent)"
                    );
                }
                Ok(window)
            }
            None => Ok(None),
        }
    }

    async fn digest(&self) -> Result<Option<String>, ProviderError> {
        let resp = self
            .base_url
            .request(&self.client, reqwest::Method::GET, &["api", "tags"])
            .send()
            .await?;
        if !(200..300).contains(&resp.status()) {
            return Ok(None);
        }
        match resp.read_probe_body(MAX_SHOW_BODY_BYTES).await {
            Some(bytes) => {
                let body = String::from_utf8_lossy(&bytes);
                let digest = Self::parse_tags_digest(&body, self.inner.model());
                if digest.is_none() {
                    tracing::warn!(
                        model = self.inner.model(),
                        "/api/tags: model not listed; digest unresolved (trusted by lineage)"
                    );
                }
                Ok(digest)
            }
            None => Ok(None),
        }
    }
}

#[cfg(all(test, feature = "ollama"))]
mod tests {
    use super::*;

    /// Every spelling a user might reasonably pass lands on the SAME pair of endpoints.
    ///
    /// The `/v1` forms are the ones users actually reach for, because the sibling
    /// OpenAI-compatible provider takes its URL that way — accepting only the root is what made
    /// the two providers disagree about the same-looking parameter.
    ///
    /// The root forms also cover the string-composition bug that produced `//v1`: the redacted
    /// rendering normalises an empty path to `/`.
    #[test]
    fn every_accepted_spelling_yields_the_same_endpoints() {
        let expected_completions = ProviderUrl::parse("http://localhost:11434/v1").expect("parses");
        let expected_root = ProviderUrl::parse("http://localhost:11434").expect("parses");

        for raw in [
            "http://localhost:11434/v1",
            "http://localhost:11434/v1/",
            "http://localhost:11434",
            "http://localhost:11434/",
        ] {
            let p = OllamaProvider::new(raw, "qwen3:8b").expect("constructs");
            assert_eq!(
                *p.inner.base(),
                expected_completions,
                "completions from {raw}"
            );
            assert_eq!(p.base_url, expected_root, "probe root from {raw}");
        }
    }

    /// A reverse proxy that mounts Ollama under a prefix keeps it, from either spelling — the
    /// `/v1` and `/api` families stay siblings under that prefix rather than jumping to the origin.
    #[test]
    fn a_mounted_prefix_survives_both_spellings() {
        let expected_completions =
            ProviderUrl::parse("https://gw.example.com/ollama/v1").expect("parses");
        let expected_root = ProviderUrl::parse("https://gw.example.com/ollama").expect("parses");

        for raw in [
            "https://gw.example.com/ollama/v1",
            "https://gw.example.com/ollama",
        ] {
            let p = OllamaProvider::new(raw, "qwen3:8b").expect("constructs");
            assert_eq!(
                *p.inner.base(),
                expected_completions,
                "completions from {raw}"
            );
            assert_eq!(p.base_url, expected_root, "probe root from {raw}");
        }
    }

    /// The regression this pair exists for: the inner provider must receive the REAL credentials,
    /// not the redaction placeholder. Equality compares the full url, so this proves it without
    /// printing anything — and a failure prints the redacted form.
    #[test]
    fn new_gives_the_inner_provider_the_real_credentials() {
        let p = OllamaProvider::new("http://alice:s3cret@localhost:11434", "qwen3:8b")
            .expect("constructs");
        assert_eq!(
            *p.inner.base(),
            ProviderUrl::parse("http://alice:s3cret@localhost:11434/v1").expect("parses")
        );
    }

    /// The probe endpoints must keep the credentials too, and must not inherit the `/v1` that
    /// belongs to the completions side — from either spelling.
    #[test]
    fn the_probe_authority_keeps_the_credentials_and_stays_at_the_root() {
        let expected = ProviderUrl::parse("http://alice:s3cret@localhost:11434").expect("parses");
        for raw in [
            "http://alice:s3cret@localhost:11434",
            "http://alice:s3cret@localhost:11434/v1",
        ] {
            let p = OllamaProvider::new(raw, "qwen3:8b").expect("constructs");
            assert_eq!(p.base_url, expected, "from {raw}");
        }
    }

    /// The documented misread, pinned so it is a known consequence rather than a surprise: a root
    /// that genuinely ends in `v1` is taken for the OpenAI prefix.
    #[test]
    fn a_root_that_really_ends_in_v1_is_read_as_the_prefix() {
        let p = OllamaProvider::new("https://gw/tenants/v1", "qwen3:8b").expect("constructs");
        assert_eq!(
            p.base_url,
            ProviderUrl::parse("https://gw/tenants").expect("parses"),
            "the probe looks one level up — documented, and it fails loudly with a 404"
        );
    }

    #[test]
    fn test_parse_show_window_scans_arch_prefixed_context_length() {
        let body = r#"{"model_info":{"gemma4.context_length":262144,"gemma4.attention.head_count":16},"details":{"family":"gemma4"}}"#;
        assert_eq!(OllamaProvider::parse_show_window(body), Some(262144));
    }

    #[test]
    fn test_parse_show_window_malformed_is_none_never_panics() {
        for body in [
            "",
            "not json",
            "{}",
            r#"{"model_info":{}}"#,
            r#"{"model_info":{"x.context_length":"NaN"}}"#,
            r#"{"model_info":{"x.context_length":-5}}"#,
        ] {
            assert!(OllamaProvider::parse_show_window(body).is_none());
        }
    }

    #[test]
    fn test_parse_tags_digest_finds_by_name() {
        let body = r#"{"models":[
            {"name":"other:1b","digest":"1111111111111111111111111111111111111111111111111111111111111111","size":1},
            {"name":"gemma4:12b","digest":"4eb23ef187e2c5462566d6a1d3bbbc2f1346d0b4327cbb66d58fffbcc9b2b05c","size":7556508396}
        ]}"#;
        assert_eq!(
            OllamaProvider::parse_tags_digest(body, "gemma4:12b").as_deref(),
            Some("4eb23ef187e2c5462566d6a1d3bbbc2f1346d0b4327cbb66d58fffbcc9b2b05c")
        );
    }

    #[test]
    fn test_parse_tags_digest_absent_or_malformed_is_none_never_panics() {
        let present = r#"{"models":[{"name":"other:1b","digest":"2222222222222222222222222222222222222222222222222222222222222222","size":1}]}"#;
        assert_eq!(
            OllamaProvider::parse_tags_digest(present, "gemma4:12b"),
            None
        );
        for bad in [
            "",
            "not json",
            "{}",
            r#"{"models":[]}"#,
            r#"{"models":[{"name":"gemma4:12b"}]}"#,
        ] {
            assert!(OllamaProvider::parse_tags_digest(bad, "gemma4:12b").is_none());
        }
    }

    #[test]
    fn test_push_within_cap_bounds_accumulator() {
        let mut acc = Vec::new();
        assert!(crate::providers::provider_url::push_within_cap(
            &mut acc,
            b"ab",
            MAX_SHOW_BODY_BYTES
        ));
        assert!(crate::providers::provider_url::push_within_cap(
            &mut acc,
            b"cd",
            MAX_SHOW_BODY_BYTES
        ));
        assert_eq!(acc, b"abcd");
        // A chunk that would exceed the cap is rejected and leaves `acc` untouched.
        let big = vec![0u8; MAX_SHOW_BODY_BYTES];
        assert!(!crate::providers::provider_url::push_within_cap(
            &mut acc,
            &big,
            MAX_SHOW_BODY_BYTES
        ));
        assert_eq!(acc, b"abcd");
        // A single oversized chunk from empty is rejected (no OOM).
        let mut empty = Vec::new();
        let one_big = vec![0u8; MAX_SHOW_BODY_BYTES + 1];
        assert!(!crate::providers::provider_url::push_within_cap(
            &mut empty,
            &one_big,
            MAX_SHOW_BODY_BYTES
        ));
        assert!(empty.is_empty());
    }
}
