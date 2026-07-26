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

/// Folds an untrusted body's byte chunks into a `Vec<u8>`, returning `None` the
/// moment the accumulated length would EXCEED `cap` (strictly greater). A body
/// within `cap` returns `Some(bytes)`. Keeps the size bound unit-testable without
/// a live server.
pub(crate) fn cap_body<C: AsRef<[u8]>>(
    chunks: impl IntoIterator<Item = C>,
    cap: usize,
) -> Option<Vec<u8>> {
    let mut acc = Vec::new();
    for chunk in chunks {
        let chunk = chunk.as_ref();
        if acc.len() + chunk.len() > cap {
            return None;
        }
        acc.extend_from_slice(chunk);
    }
    Some(acc)
}

/// Reads a probe response body with the [`MAX_SHOW_BODY_BYTES`] bound. A
/// `Content-Length` over the cap is rejected before reading; the full read is
/// then re-checked through [`cap_body`]. Any body-read problem degrades to `None`
/// (fail-open) — only the caller's `send()` surfaces a transport error.
///
/// Note: streaming the body chunk-by-chunk would need `futures_util::StreamExt`, a
/// new dependency the crate forbids ("cero dependencias nuevas"). For the local
/// Ollama daemon target, the `Content-Length` pre-check plus `cap_body` bounds
/// memory adequately; the HTTP path itself is not unit-tested (accepted gap D-2).
async fn read_capped(resp: reqwest::Response) -> Option<Vec<u8>> {
    if let Some(len) = resp.content_length()
        && len as usize > MAX_SHOW_BODY_BYTES
    {
        return None;
    }
    let bytes = resp.bytes().await.ok()?;
    cap_body([bytes], MAX_SHOW_BODY_BYTES)
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
    fn test_cap_body_rejects_oversized_stream() {
        // `cap_body` is generic over `AsRef<[u8]>`, so it accepts plain slices here
        // and reqwest's `bytes::Bytes` on the real path — no `Bytes` type needed.
        let small: Vec<&[u8]> = vec![b"ab", b"cd"];
        assert_eq!(
            cap_body(small, MAX_SHOW_BODY_BYTES).as_deref(),
            Some(&b"abcd"[..])
        );
        let big: Vec<Vec<u8>> = vec![vec![0u8; MAX_SHOW_BODY_BYTES], b"x".to_vec()];
        assert_eq!(cap_body(big, MAX_SHOW_BODY_BYTES), None);
        let one_big: Vec<Vec<u8>> = vec![vec![0u8; MAX_SHOW_BODY_BYTES + 1]];
        assert_eq!(cap_body(one_big, MAX_SHOW_BODY_BYTES), None);
    }
}
