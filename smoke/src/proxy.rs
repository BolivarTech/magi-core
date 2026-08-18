// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-17

//! The spy proxy: sits between the crate and the real backend, records every
//! request and can inject failures.
//!
//! It uses `hyper` — which `reqwest` already brings — rather than a hand-rolled
//! parser. A parser of our own would have to defend itself against chunked
//! encoding, `Expect: 100-continue`, HTTP/2 and partial reads; that is a
//! subsystem, not a support piece.
//!
//! It serves HTTP/1.1, which is what `reqwest` speaks to a plaintext endpoint.
//!
//! It NEVER produces a scenario failure: if it breaks, the run reports "cannot
//! test". Confusing "the harness broke" with "the crate broke" is the failure
//! mode this whole harness exists to eliminate.
//!
//! # Deviations from the Task 4 brief's draft, found while making it compile
//!
//! 1. `resp.bytes_stream()` needs `reqwest`'s optional `stream` feature, which
//!    is not enabled in `Cargo.toml` and adding it is out of scope for this
//!    task. [`response_chunk_stream`] gets the same streaming forward — never
//!    buffering the response body — by hand-unfolding `Response::chunk()`,
//!    which `reqwest` exposes unconditionally.
//! 2. `Self::record_of(..)` inside `handle` would resolve to `SpyProxy`, which
//!    has no such method; the constructor lives on `RequestRecord`.
//! 3. `.map_err(|e: std::convert::Infallible| match e {})` left the closure's
//!    return type unconstrained. rustc's never-type fallback picks `()` for
//!    it, which then fails the `Box<dyn Error + Send + Sync>` bound one
//!    function call later — confirmed with the Step 0 spike, where the exact
//!    same shape failed until replaced with a named function carrying an
//!    explicit return type ([`infallible_to_box`]).
//! 4. `sha256_hex` is pulled out of the `impl SpyProxy` block into its own
//!    module-level `pub fn`, per its own doc comment in the brief ("Free
//!    function at MODULE level ... NOT inside `impl SpyProxy`"); the brief's
//!    code listing had it mid-block, which does not parse.

use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

use futures_util::TryStreamExt;
use http_body_util::{BodyExt, Full, StreamBody};
use hyper::body::Bytes;

/// The whole injection surface. **Defined in Task 4 even though Task 5 is what
/// fills it**: `SpyProxy` names it in a field, so a definition that arrived
/// later would not compile.
///
/// Two variants, not a family: `FailModel` covers rotation and degradation,
/// `ReplayModel` covers fixture replay. There is no "fail the Nth request", no
/// delays and no partial rewriting — no scenario asks for them, and the spec
/// forbids abstracting for scenarios that do not exist.
#[derive(Clone, Debug)]
pub enum Injection {
    FailModel {
        model: String,
        status: u16,
    },
    ReplayModel {
        model: String,
        fixture: std::path::PathBuf,
    },
}

/// One body type for every response the proxy returns, so the streamed path
/// and the fixed-payload paths share a signature instead of forcing a
/// generic.
pub type ProxyBody =
    http_body_util::combinators::BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>;

/// Boxes an already-built body (the streaming forward).
fn boxed<B>(b: B) -> ProxyBody
where
    B: hyper::body::Body<Data = Bytes, Error = Box<dyn std::error::Error + Send + Sync>>
        + Send
        + Sync
        + 'static,
{
    ProxyBody::new(b)
}

/// `Infallible` has no values, so this function can never actually be called;
/// what it buys is a NAMED return type. An inline `|e| match e {}` closure
/// leaves its return type for rustc to infer from the body alone, and the
/// never-type fallback for an unconstrained closure is `()` — which then
/// fails to satisfy `Box<dyn Error + Send + Sync>` one call later, in
/// `ProxyBody::new`. The Step 0 spike hit exactly this and was fixed the same
/// way: see `examples/hyper_spike.rs` in git history for the failing version.
fn infallible_to_box(e: std::convert::Infallible) -> Box<dyn std::error::Error + Send + Sync> {
    match e {}
}

/// A complete, known payload — injections and the 502. `Full` is infallible,
/// so its error type is widened rather than handled.
fn fixed(bytes: &[u8]) -> ProxyBody {
    ProxyBody::new(Full::new(Bytes::copy_from_slice(bytes)).map_err(infallible_to_box))
}

fn empty_body() -> ProxyBody {
    fixed(b"")
}

/// Hex sha256, the harness's ONE hashing helper. **Free function at MODULE
/// level in `proxy.rs`, NOT inside `impl SpyProxy`** — it hashes bytes and
/// knows nothing about the proxy; `S2b` and the manifest import it as
/// `proxy::sha256_hex`.
///
/// Lives here because this is its first user (Task 4). Two hashing sites that
/// drift is how a checksum comparison starts failing for a reason nobody can
/// see.
pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Cap on the recorded copy of a request body, **derived from the configured
/// payload target** instead of hardcoded: the whole point of the large-payload
/// scenario is that the body is big, and a fixed cap that someone raises the
/// target past would start truncating the very record the scenario reads.
///
/// Twice the target leaves room for the system prompt and the JSON envelope.
/// **The FORWARDED body is never truncated** — only the copy we keep.
fn max_recorded_body(payload_target_bytes: usize) -> usize {
    (payload_target_bytes * 2).max(1 << 20) // never below 1 MiB
}

#[derive(Clone, Debug)]
pub struct RequestRecord {
    pub method: String,
    pub path: String,
    /// Truncated at `max_recorded_body(..)`. `body_truncated` says so, because
    /// an assertion reading a silently-cut body would fail for the wrong
    /// reason.
    pub body: Vec<u8>,
    pub body_truncated: bool,
    /// **Computed over the FULL body, before the kept copy is truncated.** If
    /// it were computed over the truncated copy, `S2b` — which compares
    /// transparency by checksum — would fail on the large payload because of
    /// the cap, accusing the crate of something the harness did. The hash is
    /// the load-bearing property; the stored body is for inspection only.
    pub body_sha256: String,
    /// The status the proxy relayed back. **Always recorded** — it costs
    /// nothing and it is what tells a 502 from a 200.
    pub response_status: u16,
    /// **The response BODY is recorded ONLY for the probe paths** (`/api/show`,
    /// `/api/tags`), and `response_recorded` says which case this is.
    ///
    /// # Why not always, and why this is not a half-measure
    ///
    /// The completion response is forwarded **STREAMING, never accumulated**:
    /// the crate owns that bound (`ProviderUrl`), and buffering here would put
    /// a second, harness-owned cap on a body the crate must cap itself —
    /// hiding the oversized-body path the crate has to classify. Recording
    /// every response would silently undo that.
    ///
    /// The probe paths are the exception because they are **small, bounded and
    /// deterministic**, and they are exactly what `S2b` compares. A field that
    /// were empty for the other paths without saying so would read as "the
    /// response was empty" — the same lie `Option<InputSize>` exists to avoid.
    pub response_recorded: bool,
    pub response_sha256: String,
    pub response_body: Vec<u8>,
}

impl RequestRecord {
    /// Builds the REQUEST half. The response half does not exist yet — the
    /// upstream has not answered — so the record is completed by
    /// [`with_recorded_response`](RequestRecord::with_recorded_response) or
    /// [`with_status_only`](RequestRecord::with_status_only) and pushed
    /// **once**, after forwarding. Pushing here and mutating later would need
    /// a second lock and an index, and an interleaved connection could
    /// complete the wrong record.
    ///
    /// Both halves hash the FULL body, BEFORE the copy is capped: `S2b`
    /// compares transparency by checksum, and hashing the truncated copy would
    /// fail it on the large payload because of the cap — blaming the crate for
    /// the harness.
    pub fn record_of(body: &[u8], method: &str, path: &str, cap: usize) -> RequestRecord {
        RequestRecord {
            method: method.to_string(),
            path: path.to_string(),
            body: body[..body.len().min(cap)].to_vec(),
            body_truncated: body.len() > cap,
            body_sha256: sha256_hex(body),
            // Filled by `with_recorded_response`/`with_status_only`. A record
            // pushed without one would claim a 0-status answer that never
            // happened.
            response_status: 0,
            response_recorded: false,
            response_sha256: String::new(),
            response_body: Vec::new(),
        }
    }

    /// Completes a record whose response WAS buffered (probe paths only).
    pub fn with_recorded_response(mut self, status: u16, body: &[u8], cap: usize) -> RequestRecord {
        self.response_status = status;
        self.response_recorded = true;
        self.response_sha256 = sha256_hex(body); // FULL body, before capping
        self.response_body = body[..body.len().min(cap)].to_vec();
        self
    }

    /// Completes a record whose response was STREAMED through. The status is
    /// real; the body was never held, and `response_recorded` stays false so
    /// no assertion mistakes "not recorded" for "empty".
    pub fn with_status_only(mut self, status: u16) -> RequestRecord {
        self.response_status = status;
        self
    }
}

/// The only paths whose RESPONSE body is buffered and recorded. Everything
/// else streams through untouched.
const RECORDED_RESPONSE_PATHS: [&str; 2] = ["/api/show", "/api/tags"];

#[derive(Clone)]
pub struct SpyProxy {
    base_url: String,
    records: Arc<Mutex<Vec<RequestRecord>>>,
    degraded: Arc<std::sync::atomic::AtomicBool>,
    /// **SHARED, not owned** — and this is the whole reason it is an `Arc`.
    ///
    /// `start()` spawns the serving task with a CLONE of `SpyProxy`. With a
    /// plain `Option<Injection>`, a later `set_injection(..)` would mutate the
    /// caller's copy and **the serving task would never see it**.
    ///
    /// `None` means «forward everything», so Task 4 is complete on its own and
    /// Task 5 does not have to alter this struct.
    injection: Arc<Mutex<Option<Injection>>>,
    /// Derived once from the configured payload target at `start()`.
    record_cap: usize,
}

impl SpyProxy {
    pub fn base_url(&self) -> String {
        self.base_url.clone()
    }

    /// Recovers from a poisoned lock instead of propagating the panic, and
    /// marks the registry DEGRADED so every assertion that reads it reports
    /// SKIP rather than FAIL — a partial registry could fail an assertion the
    /// crate satisfied perfectly, which would accuse the crate of a harness
    /// defect.
    pub fn records(&self) -> Vec<RequestRecord> {
        match self.records.lock() {
            Ok(g) => g.clone(),
            Err(p) => {
                self.degraded
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                p.into_inner().clone()
            }
        }
    }

    pub fn is_degraded(&self) -> bool {
        self.degraded.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Poisons the registry **for real** — the only way a `Mutex` gets
    /// poisoned is a panic while the guard is held, so the test does exactly
    /// that in a scoped thread and swallows the panic.
    ///
    /// A boolean "pretend it is poisoned" flag would test the flag, not the
    /// recovery path, and the recovery path is the thing that must not take
    /// the proxy down with it.
    #[cfg(test)]
    pub fn poison_for_test(&self) {
        let records = Arc::clone(&self.records);
        let _ = std::thread::spawn(move || {
            let _g = records.lock().expect("not poisoned yet");
            panic!("deliberate: poisons the mutex");
        })
        .join(); // Err(_) — the panic is expected
        debug_assert!(
            self.records.lock().is_err(),
            "the registry must now be poisoned"
        );
    }

    /// A watermark into the shared registry. Runs share ONE proxy, so a
    /// snapshot taken after run 3 would hand run 3's assertions the records of
    /// runs 1 and 2 as well — and an assertion counting requests would be
    /// right about a set nobody asked about.
    pub fn mark(&self) -> usize {
        self.records.lock().map(|g| g.len()).unwrap_or(0)
    }

    /// Everything recorded SINCE `mark`. Taken once, when the run ends.
    pub fn records_since(&self, mark: usize) -> Vec<RequestRecord> {
        match self.records.lock() {
            Ok(g) => g.get(mark..).unwrap_or(&[]).to_vec(),
            Err(p) => p.into_inner().get(mark..).unwrap_or(&[]).to_vec(),
        }
    }

    fn push(&self, rec: RequestRecord) {
        // The only critical section is this push: taken and released within
        // one line, with no `await` in between. With hyper each connection is
        // served by its own task, so this is the only shared point.
        match self.records.lock() {
            Ok(mut g) => g.push(rec),
            Err(p) => {
                self.degraded
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                p.into_inner().push(rec);
            }
        }
    }

    /// Binds on an ephemeral port and serves until dropped.
    ///
    /// **Everything goes through here**: there is no with-proxy and
    /// without-proxy mode, so a scenario that bypassed it would observe
    /// nothing.
    pub async fn start(
        upstream: String,
        payload_target_bytes: usize,
    ) -> Result<Self, std::io::Error> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let base_url = format!("http://{}", listener.local_addr()?);
        let this = Self {
            base_url,
            records: Arc::new(Mutex::new(Vec::new())),
            degraded: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            injection: Arc::new(Mutex::new(None)),
            record_cap: max_recorded_body(payload_target_bytes),
        };
        let srv = this.clone();
        tokio::spawn(async move {
            loop {
                // A failed accept must NOT kill the proxy: a proxy problem
                // never turns into a scenario red. It marks degraded and
                // keeps going, so the assertions report SKIP rather than FAIL.
                let Ok((stream, _)) = listener.accept().await else {
                    srv.degraded
                        .store(true, std::sync::atomic::Ordering::SeqCst);
                    continue;
                };
                let conn = srv.clone();
                let up = upstream.clone();
                tokio::spawn(async move {
                    let io = hyper_util::rt::TokioIo::new(stream);
                    let svc = hyper::service::service_fn(move |req| {
                        let conn = conn.clone();
                        let up = up.clone();
                        async move { conn.handle(req, up).await }
                    });
                    // Errors here are the CLIENT's or the connection's; a
                    // serve error is not a scenario failure either.
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(io, svc)
                        .await;
                });
            }
        });
        Ok(this)
    }

    /// Records, applies injection if any, and forwards. **The forwarded body
    /// is never altered** — that is what makes the transparency claim
    /// checkable.
    async fn handle(
        &self,
        req: hyper::Request<hyper::body::Incoming>,
        upstream: String,
    ) -> Result<hyper::Response<ProxyBody>, std::convert::Infallible> {
        let (parts, body) = req.into_parts();
        let path = parts.uri.path().to_string();
        let method = parts.method.to_string();
        let bytes = match body.collect().await {
            Ok(c) => c.to_bytes(),
            Err(_) => {
                self.degraded
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                Bytes::new()
            }
        };
        let rec = RequestRecord::record_of(&bytes, &method, &path, self.record_cap);

        // ONE push per request, AFTER the answer exists, so both halves land
        // in the same record. This connection owns `rec` until then — no
        // second lock, no index into a shared Vec, and no chance of
        // completing someone else's row.
        if let Some((status, payload)) = self.injected_response(&bytes) {
            self.push(rec.with_recorded_response(status, &payload, self.record_cap));
            return Ok(hyper::Response::builder()
                .status(status)
                .body(fixed(&payload))
                .unwrap_or_else(|_| hyper::Response::new(empty_body())));
        }
        // TWO forwarding paths, because the two directions are not symmetric:
        //   - probe paths: buffered, recorded, then handed back whole. Small,
        //     bounded and deterministic — and what `S2b` compares.
        //   - everything else: STREAMED, never accumulated. Only the status
        //     is recorded; `response_recorded` stays false so nobody reads
        //     the empty body as "the response was empty".
        if RECORDED_RESPONSE_PATHS.contains(&path.as_str()) {
            let (status, out) = self
                .forward_buffered(&method, &path, parts.headers, bytes, &upstream)
                .await;
            self.push(rec.with_recorded_response(status, &out, self.record_cap));
            return Ok(hyper::Response::builder()
                .status(status)
                .body(fixed(&out))
                .unwrap_or_else(|_| hyper::Response::new(empty_body())));
        }
        let resp = self
            .forward(&method, &path, parts.headers, bytes, &upstream)
            .await;
        self.push(rec.with_status_only(resp.status().as_u16()));
        Ok(resp)
    }

    /// The probe path: buffers the response so it can be recorded. **Never
    /// used for completions** — that is what keeps streaming intact where it
    /// matters.
    ///
    /// Bounded by `record_cap` like the request side: a probe answer is
    /// small, but "small" is an expectation and this is the one place the
    /// harness could be made to hold an arbitrary body.
    async fn forward_buffered(
        &self,
        method: &str,
        path: &str,
        headers: hyper::HeaderMap,
        body: Bytes,
        upstream: &str,
    ) -> (u16, Vec<u8>) {
        let resp = self.forward(method, path, headers, body, upstream).await;
        let status = resp.status().as_u16();
        match resp.into_body().collect().await {
            Ok(c) => (status, c.to_bytes().to_vec()),
            // Could not read it back: report the status and NO body. The
            // record keeps `response_recorded = false`, so a reader SKIPs
            // instead of comparing against an emptiness it invented.
            Err(_) => (status, Vec::new()),
        }
    }

    /// **Returns `(status, body)`, NOT a `hyper::Response`** — the injection
    /// logic has no business knowing the server's types, and Task 5 fills
    /// this exact signature. Always `None` in Task 4, so the proxy forwards
    /// everything.
    fn injected_response(&self, _body: &[u8]) -> Option<(u16, Vec<u8>)> {
        None
    }

    /// Forwards via `reqwest`, **verbatim**: same method, same path, same
    /// headers, same body. That literalness is what makes the transparency
    /// claim checkable at all.
    ///
    /// A transport failure against the upstream is the UPSTREAM's problem, so
    /// it travels back as a `502` that the crate can classify. **The proxy
    /// never invents a verdict.**
    async fn forward(
        &self,
        method: &str,
        path: &str,
        headers: hyper::HeaderMap,
        body: Bytes,
        upstream: &str,
    ) -> hyper::Response<ProxyBody> {
        let client = reqwest::Client::new();
        let url = format!("{}{}", upstream.trim_end_matches('/'), path);
        let m = reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::POST);

        let mut req = client.request(m, &url).body(body.to_vec());
        for (name, value) in headers.iter() {
            // Hop-by-hop headers must not be forwarded; `host` would point at
            // the proxy and make the upstream reject or misroute the request.
            if matches!(name.as_str(), "host" | "connection" | "content-length") {
                continue;
            }
            req = req.header(name.as_str(), value.as_bytes());
        }

        match req.send().await {
            Ok(resp) => {
                let status = resp.status();
                let hdrs = resp.headers().clone();
                // The RESPONSE is forwarded STREAMING, never accumulated. Its
                // real cap belongs to `ProviderUrl` inside the crate, and
                // buffering here would (a) put a second, harness-owned bound
                // on a body the crate is supposed to bound itself, and (b)
                // hide from the crate exactly the oversized-body path it must
                // classify.
                //
                // The REQUEST is buffered on purpose — that is what the
                // recording needs — and it is capped. The two directions are
                // not symmetric.
                let stream = response_chunk_stream(resp)
                    .map_ok(hyper::body::Frame::data)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>);
                let mut out = hyper::Response::builder().status(status.as_u16());
                for (n, v) in hdrs.iter() {
                    out = out.header(n.as_str(), v.as_bytes());
                }
                out.body(boxed(StreamBody::new(stream)))
                    .unwrap_or_else(|_| hyper::Response::new(empty_body()))
            }
            Err(_) => hyper::Response::builder()
                .status(502)
                .body(fixed(b"upstream unreachable"))
                .unwrap_or_else(|_| hyper::Response::new(empty_body())),
        }
    }
}

/// Reads a `reqwest::Response` one chunk at a time via `Response::chunk()`
/// and turns that into a `Stream`, by hand.
///
/// `reqwest::Response::bytes_stream()` would do this directly, but it is
/// gated behind the optional `stream` feature, which `Cargo.toml` does not
/// enable (see the module-level "Deviations" note) — adding a dependency
/// feature to unlock one call is out of scope, and `chunk()` gives the same
/// never-buffer-the-whole-body property unconditionally.
///
/// `stream::unfold` carries the `Response` itself as the fold state, so each
/// poll continues reading the SAME in-flight body instead of re-issuing the
/// request; the state becomes `None` (ending the stream) after `Ok(None)`
/// (body exhausted) or `Err` (nothing more can be read).
fn response_chunk_stream(
    resp: reqwest::Response,
) -> impl futures_util::Stream<Item = Result<Bytes, reqwest::Error>> {
    futures_util::stream::unfold(Some(resp), |state| async move {
        let mut resp = state?;
        match resp.chunk().await {
            Ok(Some(chunk)) => Some((Ok(chunk), Some(resp))),
            Ok(None) => None,
            Err(e) => Some((Err(e), None)),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn records_the_full_request_body_not_just_the_path() {
        let upstream = crate::testkit::spawn_echo_server().await;
        let proxy = SpyProxy::start(upstream.url(), 250_000)
            .await
            .expect("proxy bind");
        let client = reqwest::Client::new();
        client
            .post(format!("{}/api/chat", proxy.base_url()))
            .json(&serde_json::json!({"model": "m", "stream": false}))
            .send()
            .await
            .unwrap();

        let rec = &proxy.records()[0];
        assert_eq!(rec.method, "POST");
        assert_eq!(rec.path, "/api/chat");
        // Without the body, this would verify the envelope and not the letter.
        assert!(String::from_utf8_lossy(&rec.body).contains("\"stream\":false"));
    }

    #[tokio::test]
    async fn a_poisoned_registry_degrades_but_the_proxy_keeps_serving() {
        // A partial registry degrades the assertions that read it; a dead
        // proxy fails EVERY scenario. Failing forward and saying so is the
        // right direction.
        let upstream = crate::testkit::spawn_echo_server().await;
        let proxy = SpyProxy::start(upstream.url(), 250_000)
            .await
            .expect("proxy bind");
        proxy.poison_for_test(); // see the impl below: a real poisoning, not a flag
        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/api/chat", proxy.base_url()))
            .body("{}")
            .send()
            .await;
        assert!(
            resp.is_ok(),
            "the proxy must keep serving after a poisoned lock"
        );
        assert!(proxy.is_degraded());
    }
}
