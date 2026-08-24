// Author: Julian Bolivar
// Version: 4.0.0
// Date: 2026-08-23

//! Native Ollama provider (feature `ollama`).
//!
//! Speaks the **native** API for everything: `POST {base}/api/chat` for
//! [`LlmProvider::complete`] and the native probe endpoints for [`ProviderProbe`]:
//!
//! # Why native, and why unconditionally
//!
//! Until `4.0.0` completions went through the OpenAI-compatible path
//! (`{base}/v1/chat/completions`). That path cannot honour a reasoning control:
//! `think: false` was measured **accepted with HTTP 200 and inert** there, while the
//! native endpoint made the same payload converge in 602 tokens instead of burning
//! 32 768 and returning nothing. A knob that appears to work is worse than no knob.
//!
//! The routing is unconditional rather than switchable because a second mode shipped
//! into a public surface costs another major to remove — so it does not get removed.
//! `/v1` is also Ollama's own experimental compatibility layer; `/api/chat` is its
//! primary API.
//!
//! - **window** ← `POST {base}/api/show` → `model_info` → first `*.context_length`
//!   (the key is architecture-prefixed and NOT derivable from `details.family`, so
//!   it is scanned, never built).
//! - **digest** ← `GET {base}/api/tags` → the model's manifest SHA256 (64-char
//!   lowercase hex, no `sha256:` prefix). `/api/show` has NO digest field.
//!
//! # Probe transport errors are classified like any other, since 3.1.0
//!
//! The probe used to map **every** transport failure to `Network`, which is the connection class:
//! a probe that merely timed out counted toward the endpoint-down latch while a completion that
//! timed out did not. Both now go through the shared mapper, so a timeout reads as `Timeout` — it
//! still condemns the lineage run-wide, it just no longer fast-fails the whole run on a slow
//! daemon. That asymmetry was the accident; this is the deliberate part.
//!
//! Both bodies are untrusted, so each read is bounded by `MAX_SHOW_BODY_BYTES`
//! (`ProviderResponse::read_probe_body`); an over-cap or malformed body degrades the probe to `None`
//! (fail-open, trusted by lineage) rather than erroring — only a transport failure
//! surfaces a [`ProviderError`]. HTTP-thin, no new dependencies (`reqwest` is
//! already pulled by the `openai-compat` feature this one enables).

use std::time::{Duration, Instant};

use async_trait::async_trait;

use crate::error::{ProviderError, ResponseContractCause};
use crate::provider::{Completion, CompletionConfig, DEFAULT_CLIENT_TIMEOUT, LlmProvider};
use crate::providers::ollama_wire::{NativeMessage, NativeRequest, NativeResponse, native_error};
use crate::providers::provider_url::ProviderUrl;
use crate::rotation::ProviderProbe;

/// Native Ollama provider: `/api/chat` completions + `/api/show` + `/api/tags`
/// probe. Construct with [`OllamaProvider::new`].
pub struct OllamaProvider {
    /// The URL authority — never a `String`, so a reverse proxy's credentials in front of Ollama
    /// cannot leak through `Debug` or an error message.
    base_url: ProviderUrl,
    client: reqwest::Client,
    /// The model tag, passed through unchanged. Held here since `4.0.0`; before that it lived
    /// inside the wrapped OpenAI-compatible provider, which no longer exists.
    model: String,
}

impl OllamaProvider {
    /// Creates a provider for an Ollama daemon. Keyless — Ollama needs no bearer token.
    ///
    /// # Parameters
    /// - `base_url`: **either** the daemon root (`http://localhost:11434`) **or** the same URL
    ///   with the legacy `/v1` suffix. Both are accepted and both produce the same native
    ///   `/api/*` endpoints; nothing this provider sends addresses `/v1`.
    ///
    ///   That concerns the URL you CONFIGURE, not what the endpoint serves. A gateway exposing
    ///   only the OpenAI-compatible surface has no `/api/chat` to answer and stops working with
    ///   this provider; point `OpenAiCompatibleProvider` at it instead, which is the documented
    ///   path for that shape.
    /// - `model`: the model tag, passed through unchanged.
    ///
    /// # Errors
    /// [`ProviderError::Network`] on an invalid `base_url`/scheme or client build.
    ///
    /// # Why both spellings are accepted
    ///
    /// Ollama serves its OpenAI-compatible API under `/v1` and its native API under `/api`. Since
    /// `4.0.0` this provider uses **only** the native one — for completions as well as for the
    /// window and digest measurements — so a `/v1` suffix is **normalised away** rather than
    /// used. It is still accepted because an existing configuration should keep working, and
    /// because the sibling [`OpenAiCompatibleProvider`] does take its URL that way.
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
    /// | `http://localhost:11434/v1` | `…/api/chat` | `…/api/show`, `…/api/tags` |
    /// | `http://localhost:11434` | `…/api/chat` | `…/api/show`, `…/api/tags` |
    /// | `https://gw.example.com/ollama/v1` | `…/ollama/api/chat` | `…/ollama/api/*` |
    ///
    /// Since `4.0.0` a `/v1` spelling is accepted and then **normalised away**: nothing this
    /// provider talks to lives under `/v1` any more. The spelling is still accepted because
    /// consumers reach for it, and breaking them for a reason that is ours would be the wrong
    /// trade.
    ///
    /// # The one deployment this reads wrong
    ///
    /// A daemon whose root genuinely ends in a segment named `v1` — say
    /// `https://gw/tenants/v1` — is taken for the OpenAI prefix, so **both** families are looked
    /// for one level too high.
    ///
    /// It does not degrade quietly, but be precise about which half is the alarm:
    ///
    /// | Channel | What happens |
    /// |---|---|
    /// | **Completions** | 404 → `ProviderError::Http { status: 404 }`, which is **run-wide**: the lineage is condemned for every seat, not just this one. That did NOT change in `4.0.0` — a real HTTP status still means transport. Only the CONTRACT failures became mage-local. **This is the loud one.** |
    /// | **Probe** | 404 → `Ok(None)`, by design. It is fail-open: an unmeasurable window is a valid result, so nothing refuses here. |
    ///
    /// The probe's silence surfaces only as the report's *estimated window* note — a disclosure,
    /// not a refusal. A strict context guard does **not** catch this either: it is off by default,
    /// and it filters rotation *candidates*, never the primary this constructor produces.
    ///
    /// Pass such a root through [`OpenAiCompatibleProvider`] instead, which does no probing.
    pub fn new(
        base_url: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self, ProviderError> {
        Self::with_timeout(base_url, model, DEFAULT_CLIENT_TIMEOUT)
    }

    /// Like [`new`](Self::new) but bounds the HTTP client this type builds with `timeout`, which
    /// covers completions and the probe alike — they share it.
    ///
    /// [`new`](Self::new) delegates here with [`DEFAULT_CLIENT_TIMEOUT`], so the default is
    /// unchanged: 300 s, which is generous on purpose because a local daemon may be loading a
    /// model from cold on the first call.
    ///
    /// The timeout covers the entire request, from send to the last body byte. It said "both
    /// clients" until `4.0.0`, which was true while this type wrapped an `OpenAiCompatibleProvider`
    /// for completions and kept its own for the probe; that wrapper is gone and there is one.
    /// `Duration::MAX` means "no timeout" and is dangerous for the same reason it is on the
    /// sibling provider: a model that hangs while generating hangs forever. Nothing validates
    /// the value here.
    ///
    /// # Why this exists
    ///
    /// A consumer that derives its per-agent timeouts from a single ceiling needs the client
    /// timeout to fit under it — see the layering section on
    /// [`RetryConfig`](crate::provider::RetryConfig). It gives the current form and explains why
    /// the older
    /// `operation_budget + client_timeout <= MagiConfig::timeout` is left unsatisfied by the
    /// shipped defaults **on purpose**: since `4.0.0` the chain is bounded by an attempt count
    /// and the budget is a backstop, so summing the two would buy a ceiling the chain cannot
    /// reach. Without this constructor such a consumer had to give
    /// up this type entirely for completions, and this is the only provider here that can also
    /// probe — so a capability ended up dictating a provider. The sibling HTTP providers already
    /// offered the same knob; this one was the outlier.
    pub fn with_timeout(
        base_url: impl Into<String>,
        model: impl Into<String>,
        timeout: Duration,
    ) -> Result<Self, ProviderError> {
        let given = ProviderUrl::parse(&base_url.into())?;
        // A `/v1` spelling is still ACCEPTED — users reach for it because the sibling provider
        // takes its URL that way — but it is now normalised AWAY: every endpoint this provider
        // uses hangs off the daemon root. Rejecting the spelling instead would break consumers
        // for a reason that is ours, not theirs.
        let base = if given.ends_with_segment(OPENAI_COMPAT_PREFIX) {
            given.parent()
        } else {
            given
        };
        let client = reqwest::Client::builder()
            .timeout(timeout)
            // Referer OFF — see the note in the OpenAI-compatible provider: on a redirect the
            // client would send the ORIGINAL url, query string included, to the target origin.
            .referer(false)
            .build()
            .map_err(|e| crate::provider::client_build_error(&e))?;
        Ok(Self {
            base_url: base,
            client,
            model: model.into(),
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
    /// Completes over Ollama's **native** `POST {base}/api/chat`, always.
    ///
    /// # Errors
    /// - [`ProviderError::Timeout`] if the request exceeds the total client timeout.
    /// - [`ProviderError::Network`] on connection failures.
    /// - [`ProviderError::Auth`] on 401/403 — the daemon itself is keyless, but this
    ///   provider is explicitly built to sit behind a reverse proxy, which may not be.
    /// - [`ProviderError::Http`] on any **other** non-2xx response, carrying the **real**
    ///   status — a missing model answers `404` with `{"error": "..."}`, which is nothing
    ///   like the OpenAI-compatible error shape.
    /// - [`ProviderError::ResponseTooLarge`] when the body exceeds the cap derived from
    ///   `max_tokens`. It fails rather than truncating: a cut body loses its closing marker.
    /// - [`ProviderError::ResponseContract`] when the response did not meet the contract —
    ///   `Unreadable`, `NoMessage`, or `RedirectRefused` — and
    ///   [`ProviderError::EmptyCompletion`] when the model produced no usable content. Both
    ///   are **mage-local**: no lineage is condemned run-wide.
    /// - [`ProviderError::NoGeneration`] on the **full three-signal footprint**: both token
    ///   counters absent (absent, not zero), empty content, **and** `done_reason` exactly
    ///   `load`. Anything short of all three is [`ProviderError::EmptyCompletion`] instead —
    ///   `tests/fixtures/ec/native-unload-empty-messages.json` meets the first three under
    ///   `unload` and takes that safer path. The narrowing is deliberate: this footprint is a
    ///   defect of THIS crate rather than a failure of the model, so the orchestrator raises it
    ///   and aborts the run instead of rotating — rotating would reproduce it at every seat —
    ///   and an irreversible consequence is owed a precise trigger, not one symptom.
    async fn complete(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        config: &CompletionConfig,
    ) -> Result<Completion, ProviderError> {
        // Built from the authority this provider already holds, NEVER from a formatted string:
        // `Display` on a `ProviderUrl` is the REDACTED rendering, so composing a url by hand
        // would send the literal placeholder where a reverse proxy's credentials belong — a
        // silent 401 — and would step outside the type that owns redaction. That guarantee is
        // structural and it is inherited by using the type, not by repeating its code.
        //
        // `request` also applies `.referer(false)` and maps transport errors with the redacted
        // url. A hand-built client here would lose all three properties of 3.1.0 at once, and
        // `ci/check_redaction.sh` would go red for exactly that reason.
        let response = self
            .base_url
            .request(&self.client, reqwest::Method::POST, &["api", "chat"])
            .json(&NativeRequest::new(
                &self.model,
                vec![
                    NativeMessage::system(system_prompt),
                    NativeMessage::user(user_prompt),
                ],
                config.max_tokens,
                config.temperature,
                config.reasoning,
            ))
            .send()
            .await?;

        // Captured on the SAME beat as the status and BEFORE the body is read, exactly as the
        // compat sibling does: `send` resolves on the headers, so this is the moment the
        // response arrived. Review found this path was dropping both, which silently turned a
        // gateway's `429` with a `Retry-After` into blind exponential backoff — a live
        // regression for anyone moving from `/v1` to native against a cloud tag.
        let received_at = Instant::now();
        let status = response.status();
        let retry_after_raw = response.retry_after_raw();

        if !(200..300).contains(&status) {
            // The error branch reads a DIAGNOSTIC body, which truncates. Reading it through the
            // verdict reader instead made an oversized error page fail as `ResponseTooLarge` —
            // mage-local and non-retryable — losing the status entirely, so a `503` behind a
            // chatty proxy stopped looking like transport.
            let body = response.read_diagnostic_body().await;
            return Err(native_error(status, &body, retry_after_raw, received_at));
        }

        // The success branch keeps the verdict reader: over the cap it FAILS rather than
        // truncating, because a cut body loses its closing marker and the parser would blame the
        // model for a cut this reader made.
        let body = response.read_verdict_body(config.max_tokens).await?;
        let native: NativeResponse =
            serde_json::from_str(&body).map_err(|_| ProviderError::ResponseContract {
                reason: ResponseContractCause::Unreadable,
                detail: String::new(),
            })?;
        native.into_completion(config.max_tokens, config.reasoning_trace)
    }

    fn name(&self) -> &str {
        "ollama"
    }

    fn model(&self) -> &str {
        self.model.as_str()
    }
}

#[async_trait]
impl ProviderProbe for OllamaProvider {
    /// This provider always knows: it probes `/api/show` and `/api/tags` for the exact model
    /// its completions half serves, so it can answer and let the preflight check the
    /// correspondence rather than take it on trust.
    fn declared_model(&self) -> Option<&str> {
        Some(self.model.as_str())
    }

    async fn window(&self) -> Result<Option<usize>, ProviderError> {
        let resp = self
            .base_url
            .request(&self.client, reqwest::Method::POST, &["api", "show"])
            .json(&serde_json::json!({ "model": self.model.as_str() }))
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
                        model = self.model.as_str(),
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
                let digest = Self::parse_tags_digest(&body, &self.model);
                if digest.is_none() {
                    tracing::warn!(
                        model = self.model.as_str(),
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
        // Since 4.0.0 there is no `/v1` endpoint left to check: BOTH spellings normalise to the
        // daemon root, and every endpoint — completions included — hangs off it.
        let expected_root = ProviderUrl::parse("http://localhost:11434").expect("parses");

        for raw in [
            "http://localhost:11434/v1",
            "http://localhost:11434/v1/",
            "http://localhost:11434",
            "http://localhost:11434/",
        ] {
            let p = OllamaProvider::new(raw, "qwen3:8b").expect("constructs");
            assert_eq!(p.base_url, expected_root, "root from {raw}");
        }
    }

    /// A reverse proxy that mounts Ollama under a prefix keeps it, from either spelling — the
    /// `/v1` and `/api` families stay siblings under that prefix rather than jumping to the origin.
    #[test]
    fn a_mounted_prefix_survives_both_spellings() {
        let expected_root = ProviderUrl::parse("https://gw.example.com/ollama").expect("parses");

        for raw in [
            "https://gw.example.com/ollama/v1",
            "https://gw.example.com/ollama",
        ] {
            let p = OllamaProvider::new(raw, "qwen3:8b").expect("constructs");
            assert_eq!(p.base_url, expected_root, "root from {raw}");
        }
    }

    /// The regression this test exists for: the authority must keep the REAL credentials, not the
    /// redaction placeholder. Equality compares the full url, so this proves it without printing
    /// anything — and a failure prints the redacted form.
    ///
    /// It used to check the wrapped provider's `/v1` base. With the wrapper gone there is one
    /// authority left, and it is the one every endpoint is built from — so the same property is
    /// now checked where it actually lives.
    #[test]
    fn construction_keeps_the_real_credentials_on_the_authority() {
        let p = OllamaProvider::new("http://alice:s3cret@localhost:11434", "qwen3:8b")
            .expect("constructs");
        assert_eq!(
            p.base_url,
            ProviderUrl::parse("http://alice:s3cret@localhost:11434").expect("parses")
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

    /// Starts a TCP listener that accepts connections and never responds, holding each
    /// accepted stream alive for a few seconds instead of dropping it. Dropping the
    /// stream immediately would close the connection and the client would fail instantly
    /// for the WRONG reason — a test that passes because the peer hung up proves nothing
    /// about client-side timeouts. Returns the port the listener is bound to.
    fn start_unresponsive_server() -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
        let port = listener.local_addr().expect("read assigned port").port();
        std::thread::spawn(move || {
            while let Ok((stream, _addr)) = listener.accept() {
                // Keep the stream alive without writing anything so the client-side
                // timeout, not a connection reset, is what ends the call.
                std::thread::sleep(Duration::from_secs(5));
                drop(stream);
            }
        });
        port
    }

    #[test]
    fn test_declared_model_answers_the_model_it_probes_for() {
        // The provider is the one production probe that CAN answer, so the preflight's
        // correspondence check only has teeth if this reports the same model the
        // completions half serves — not the URL, not a label.
        let p = OllamaProvider::new("http://127.0.0.1:11434", "qwen3:8b").expect("constructs");
        assert_eq!(ProviderProbe::declared_model(&p), Some("qwen3:8b"));
        assert_eq!(ProviderProbe::declared_model(&p), Some(p.model.as_str()));
    }

    #[test]
    fn test_new_keeps_the_default_client_timeout() {
        // Asserts through `Debug`, which reqwest renders including the client's total
        // timeout — but WITHOUT pinning its rendering. The comparison is what carries the
        // claim: `new` must be indistinguishable from an explicit default, and
        // distinguishable from a different timeout. If a future reqwest stopped surfacing
        // the timeout at all, the second assertion fails rather than passing vacuously,
        // which is the property that makes this cheap test honest instead of decorative.
        let base = "http://127.0.0.1:1";
        let by_new = OllamaProvider::new(base, "m").expect("constructs");
        let by_default =
            OllamaProvider::with_timeout(base, "m", DEFAULT_CLIENT_TIMEOUT).expect("constructs");
        let by_short = OllamaProvider::with_timeout(base, "m", Duration::from_millis(200))
            .expect("constructs");

        assert_eq!(
            format!("{:?}", by_new.client),
            format!("{:?}", by_default.client),
            "new must still build the client with DEFAULT_CLIENT_TIMEOUT"
        );
        assert_ne!(
            format!("{:?}", by_new.client),
            format!("{:?}", by_short.client),
            "a different timeout must be observable, or the assertion above proves nothing"
        );
    }

    #[tokio::test]
    async fn test_with_timeout_bounds_a_completion_that_never_answers() {
        let port = start_unresponsive_server();
        let provider = OllamaProvider::with_timeout(
            format!("http://127.0.0.1:{port}"),
            "m",
            Duration::from_millis(200),
        )
        .expect("construct provider");

        let outcome = tokio::time::timeout(
            Duration::from_secs(3),
            provider.complete("s", "u", &CompletionConfig::default()),
        )
        .await;

        // The outer deadline must NOT be what ends this call — the provider's own timeout
        // should return first.
        let inner_result = outcome.expect("provider call should return before the outer deadline");

        // A server that never answers cannot produce a completion.
        assert!(inner_result.is_err());
    }

    #[tokio::test]
    async fn test_with_timeout_bounds_a_probe_that_never_answers() {
        let port = start_unresponsive_server();
        let provider = OllamaProvider::with_timeout(
            format!("http://127.0.0.1:{port}"),
            "m",
            Duration::from_millis(200),
        )
        .expect("construct provider");

        let outcome = tokio::time::timeout(Duration::from_secs(3), provider.window()).await;

        // Only the deadline matters here, not how the probe resolved: the probe is
        // fail-open by design, so an unanswered one may legitimately degrade to `Ok(None)`
        // instead of `Err`. This proves the call RETURNED within the bounded timeout, not
        // which variant it returned — hence the deliberately discarded inner result.
        let _inner = outcome.expect("probe call should return before the outer deadline");
    }
}
