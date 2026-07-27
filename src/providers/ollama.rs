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
use crate::rotation::ProviderProbe;

/// Native Ollama provider: OpenAI-compatible completions + `/api/show` +
/// `/api/tags` probe. Construct with [`OllamaProvider::new`].
pub struct OllamaProvider {
    inner: OpenAiCompatibleProvider,
    base_url: String,
    client: reqwest::Client,
}

impl OllamaProvider {
    /// Creates a provider for an Ollama daemon at `base_url` (e.g.
    /// `http://localhost:11434`). Completions target `{base}/v1`; the probe uses
    /// the native `{base}/api/*` endpoints. Keyless (Ollama needs no bearer token).
    ///
    /// # Errors
    /// [`ProviderError::Network`] on an invalid `base_url`/scheme or client build.
    pub fn new(
        base_url: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self, ProviderError> {
        let base = base_url.into().trim_end_matches('/').to_string();
        let inner = OpenAiCompatibleProvider::new(format!("{base}/v1"), model, None)?;
        let client = reqwest::Client::builder()
            .timeout(DEFAULT_CLIENT_TIMEOUT)
            .build()
            .map_err(|e| ProviderError::Network {
                message: format!("failed to build HTTP client: {e}"),
            })?;
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

/// Appends `chunk` to `acc` unless doing so would EXCEED `cap` (strictly greater),
/// in which case it returns `false` and leaves `acc` unchanged. Called per network
/// chunk by [`read_capped`], so the accumulator never grows past `cap` — the
/// streaming memory bound for untrusted probe bodies. Pure and unit-testable.
pub(crate) fn push_within_cap(acc: &mut Vec<u8>, chunk: &[u8], cap: usize) -> bool {
    if acc.len() + chunk.len() > cap {
        return false;
    }
    acc.extend_from_slice(chunk);
    true
}

/// Reads a probe response body bounded by [`MAX_SHOW_BODY_BYTES`], CHUNK BY CHUNK,
/// so it never buffers more than the cap even when `Content-Length` is absent or
/// lies. A `Content-Length` already over the cap is rejected early, compared in
/// `u64` (no `usize` truncation on 32-bit). Any body-read problem degrades to
/// `None` (fail-open) — only the caller's `send()` surfaces a transport error.
///
/// `reqwest::Response::chunk()` is an inherent async method, so the streaming
/// bound needs NO `futures_util` dependency (the crate adds none). The HTTP path
/// itself is not unit-tested (accepted gap D-2); the bound logic is
/// ([`push_within_cap`]).
async fn read_capped(mut resp: reqwest::Response) -> Option<Vec<u8>> {
    if let Some(len) = resp.content_length()
        && len > MAX_SHOW_BODY_BYTES as u64
    {
        return None;
    }
    let mut acc: Vec<u8> = Vec::new();
    loop {
        match resp.chunk().await {
            Ok(Some(chunk)) => {
                if !push_within_cap(&mut acc, &chunk, MAX_SHOW_BODY_BYTES) {
                    return None; // over-cap → degrade (fail-open), never OOM
                }
            }
            Ok(None) => return Some(acc),
            Err(_) => return None, // body-read error → degrade (fail-open)
        }
    }
}

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
        let url = format!("{}/api/show", self.base_url);
        let resp = self
            .client
            .post(&url)
            .json(&serde_json::json!({ "model": self.inner.model() }))
            .send()
            .await
            .map_err(|e| ProviderError::Network {
                message: format!("/api/show request failed: {e}"),
            })?;
        match read_capped(resp).await {
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
        let url = format!("{}/api/tags", self.base_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| ProviderError::Network {
                message: format!("/api/tags request failed: {e}"),
            })?;
        match read_capped(resp).await {
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
        assert!(push_within_cap(&mut acc, b"ab", MAX_SHOW_BODY_BYTES));
        assert!(push_within_cap(&mut acc, b"cd", MAX_SHOW_BODY_BYTES));
        assert_eq!(acc, b"abcd");
        // A chunk that would exceed the cap is rejected and leaves `acc` untouched.
        let big = vec![0u8; MAX_SHOW_BODY_BYTES];
        assert!(!push_within_cap(&mut acc, &big, MAX_SHOW_BODY_BYTES));
        assert_eq!(acc, b"abcd");
        // A single oversized chunk from empty is rejected (no OOM).
        let mut empty = Vec::new();
        let one_big = vec![0u8; MAX_SHOW_BODY_BYTES + 1];
        assert!(!push_within_cap(&mut empty, &one_big, MAX_SHOW_BODY_BYTES));
        assert!(empty.is_empty());
    }
}
