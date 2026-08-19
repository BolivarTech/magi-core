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
//!
//! # Fixes from review round 1 (Critical: `forward_buffered` fabricated a
//! response)
//!
//! `forward_buffered` used to return `(u16, Vec<u8>)` and its `Err` arm
//! returned an empty `Vec` on a failed body read. The caller then
//! unconditionally called `with_recorded_response`, which unconditionally
//! sets `response_recorded = true` — so a read failure was recorded as a
//! genuine empty `200`, indistinguishable from a backend that truly answered
//! nothing. It now returns `(u16, Option<Vec<u8>>)`; `None` routes the
//! caller to `with_status_only` instead, so a failed read is recorded as
//! "nothing recorded," never as "recorded, and it was empty." See
//! [`RequestRecord::response_recorded`]'s fix note and the test
//! `a_broken_response_read_is_not_recorded_as_an_empty_answer`.
//!
//! # Fixes from review round 2 (the same defect, on the REQUEST side)
//!
//! The response half of that fix landed while the request half kept the bug: a
//! failed `body.collect()` in [`SpyProxy::handle`] became `Bytes::new()` and was
//! FORWARDED, so the backend received an empty request the client never sent,
//! and the transparency comparison could then blame the crate for a body the
//! HARNESS substituted. Nothing is forwarded now; see the `Err` arm in `handle`
//! and `an_unreadable_request_is_not_forwarded_as_an_empty_one`.
//!
//! # Fixes from review round 3 (the same defect once more, in the FALLBACKS)
//!
//! Five response builders ended in
//! `unwrap_or_else(|_| hyper::Response::new(empty_body()))`, and
//! `hyper::Response::new` defaults to **`200`** — so a builder failure answered
//! the crate with SUCCESS over a failure, which is the harness fabricating the
//! exact outcome it exists to catch. Four of the five cannot fail today (their
//! statuses are compile-time constants or `StatusCode` values round-tripped
//! from a real response), and that is not a reason to leave them aimed the
//! wrong way: the fifth — the injected stand-in, whose status is a `u16` the
//! caller chooses — is reachable right now through [`SpyProxy::set_injection`],
//! and the other four become reachable the moment someone edits a status. They
//! all route through [`build_failed`] now; see
//! `a_response_the_proxy_cannot_build_is_an_error_not_a_fabricated_success`.

use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::TryStreamExt;
use http_body_util::{BodyExt, Full, StreamBody};
use hyper::body::Bytes;

/// The whole injection surface. **Defined in Task 4 even though Task 5 is what
/// fills it**: `SpyProxy` names it in a field, so a definition that arrived
/// later would not compile.
///
/// ONE variant, not a family. `FailModel` covers rotation and degradation,
/// which is the only injection any scenario in this release asks for. There is
/// no "fail the Nth request", no delays, no partial rewriting — and no fixture
/// replay either: a `ReplayModel` variant was specified, written, and then
/// removed, because nothing in any of the five milestone plans constructs one.
/// It would have shipped an untested match arm whose fixture read swallowed its
/// own error, and this enum is private to an unpublished binary, so adding the
/// variant the day a replay scenario exists costs exactly what keeping it costs
/// now — and buys certainty about its shape instead of guessing at it.
#[derive(Clone, Debug)]
pub enum Injection {
    FailModel { model: String, status: u16 },
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

/// Status the proxy answers with when it could not BUILD the response that
/// would have carried an UPSTREAM exchange back to the crate.
///
/// A `502` and deliberately not a `200`: the proxy did talk to the backend and
/// then failed to relay what came of it, which is a gateway failure and exactly
/// what the crate can classify as one.
const RELAY_BUILD_FAILED_STATUS: hyper::StatusCode = hyper::StatusCode::BAD_GATEWAY;

/// Status the proxy answers with when it could not BUILD a response of its
/// OWN — an injected stand-in, or a refusal about a request it could not read.
///
/// A `500` and **not** a `502`, and the distinction is the point: no upstream
/// leg exists in these exchanges, so `502` would claim a conversation with the
/// backend that never happened and send an investigation looking for it. `500`
/// says "the server side of this hop failed", which is true — the proxy IS the
/// server here.
const LOCAL_BUILD_FAILED_STATUS: hyper::StatusCode = hyper::StatusCode::INTERNAL_SERVER_ERROR;

/// The body sent with either build-failure status. Names the proxy as the party
/// that failed, so the line is legible in a captured trace on its own.
const BUILD_FAILED_BODY: &[u8] = b"spy proxy: could not build the response it meant to send";

/// The fallback for a response the proxy meant to build and could not.
///
/// # Why this exists at all
///
/// Five call sites used to end in
/// `unwrap_or_else(|_| hyper::Response::new(empty_body()))`, and
/// `hyper::Response::new` **defaults to `200`** — so a builder failure answered
/// the crate with SUCCESS over a failure. That is the harness fabricating the
/// one outcome it exists to catch, and it does not stop being wrong for being
/// unreachable today: an unreachable fallback is a reachable one after the next
/// edit, and it would then be pointing the wrong way with nothing to catch it.
///
/// # Parameters
///
/// * `status` — [`RELAY_BUILD_FAILED_STATUS`] when the failure is about
///   relaying an upstream exchange, [`LOCAL_BUILD_FAILED_STATUS`] when the
///   proxy was answering on its own behalf. The call sites choose; this
///   function does not guess.
///
/// # Errors
///
/// **None, by construction.** It uses `Response::new` plus `status_mut` rather
/// than `Response::builder()`: the builder is precisely what failed on the way
/// in, and a fallback that can fail the same way as the thing it is falling
/// back from is not a fallback. A `StatusCode` is already valid — there is no
/// conversion left to reject — and [`fixed`] is infallible.
///
/// # Complexity
///
/// `O(n)` in [`BUILD_FAILED_BODY`]'s length, which is a fixed short constant.
fn build_failed(status: hyper::StatusCode) -> hyper::Response<ProxyBody> {
    let mut resp = hyper::Response::new(fixed(BUILD_FAILED_BODY));
    *resp.status_mut() = status;
    resp
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

/// Absolute floor for [`max_recorded_body`], regardless of how small
/// `payload_target_bytes` is: `1 MiB`, written as its own definition rather
/// than pulling in a units crate for one number.
const MIN_RECORDED_BODY_CAP: usize = 1 << 20;

/// The status the proxy relays when it cannot reach the upstream at all — a
/// transport failure between the proxy and the backend, never something the
/// backend itself returned. Named so a reader does not have to reverse the
/// meaning of a bare `502` out of the code that sends it.
const UPSTREAM_UNREACHABLE_STATUS: u16 = 502;

/// The status the proxy answers with when it could not read the request body it
/// was handed — see [`SpyProxy::handle`] for why nothing is forwarded in that
/// case.
///
/// **`5xx` and deliberately not `4xx`.** For this exchange the proxy IS the
/// server, and the party that failed is the proxy; a `4xx` would tell the crate
/// its own request was malformed, which is a claim the harness has no basis for
/// and which points an investigation straight at the crate — the misattribution
/// this whole harness exists to prevent. The crate reads a `5xx` as the server
/// side failing, which is true, and [`SpyProxy::is_degraded`] is what tells the
/// scenarios to report SKIP rather than either verdict.
const REQUEST_UNREADABLE_STATUS: u16 = 500;

/// The body sent with [`REQUEST_UNREADABLE_STATUS`]. Says who failed, so the
/// line is legible in a captured trace without cross-referencing the status.
const REQUEST_UNREADABLE_BODY: &[u8] = b"spy proxy: could not read the request body";

/// Headers that belong to ONE hop of a connection and must never be relayed to
/// the next one, in the lowercase form `hyper` and `reqwest` both hand back.
///
/// Two of these corrupt framing rather than merely being untidy:
/// `transfer-encoding` describes how the body was framed on the connection the
/// proxy read it from, and both directions here are re-framed by the library
/// that writes them, so relaying it end to end announces an encoding that is
/// not the one on the wire. `connection` names further headers that are
/// themselves per-hop, so passing it on can make the far end treat an
/// end-to-end header as disposable.
///
/// The list is RFC 9110's set of connection-specific fields plus `proxy-*`; it
/// is a closed set and does not grow with what the harness happens to see.
const HOP_BY_HOP_HEADERS: [&str; 8] = [
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

/// Whether `name` is a per-hop header that must be dropped rather than relayed.
///
/// # Parameters
///
/// * `name` — a header name as `hyper`/`reqwest` expose it, already lowercase.
///
/// # Complexity
///
/// `O(h)` over the eight names in [`HOP_BY_HOP_HEADERS`].
fn is_hop_by_hop(name: &str) -> bool {
    HOP_BY_HOP_HEADERS.contains(&name)
}

/// Cap on the recorded copy of a request body, **derived from the configured
/// payload target** instead of hardcoded: the whole point of the large-payload
/// scenario is that the body is big, and a fixed cap that someone raises the
/// target past would start truncating the very record the scenario reads.
///
/// Twice the target leaves room for the system prompt and the JSON envelope.
/// **The FORWARDED body is never truncated** — only the copy we keep.
fn max_recorded_body(payload_target_bytes: usize) -> usize {
    (payload_target_bytes * 2).max(MIN_RECORDED_BODY_CAP)
}

/// One request as the proxy saw it, and what came back.
///
/// **The request body is identified by its HASH, never stored.** A `method` and
/// a `body` field lived here for one milestone with no reader outside
/// `#[cfg(test)]` code, on the argument that a later milestone would want them;
/// that is API surface without a consumer, and a test is not one. Adding them
/// the day something reads them costs exactly what keeping them costs now, and
/// buys certainty about the shape that reader actually needs.
#[derive(Clone, Debug)]
pub struct RequestRecord {
    /// The request's path component only — no query string, no host. This is
    /// what [`RECORDED_RESPONSE_PATHS`] matches against.
    pub path: String,
    /// **Computed over the FULL body, whatever the recording cap says.** If it
    /// were computed over a capped prefix, `S2b` — which compares transparency
    /// by checksum — would fail on the large payload because of the cap,
    /// accusing the crate of something the harness did. This hash IS the
    /// record of what the request carried.
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
    ///
    /// # Fix note (review round 1)
    ///
    /// This field exists to prevent exactly one lie: a probe-path response
    /// whose body could not be fully read back must NEVER be recorded as
    /// `response_recorded: true` with an empty body — that would be
    /// indistinguishable from a backend that genuinely answered empty, which
    /// is the exact class of failure this harness exists to catch. The
    /// caller in `handle` routes a failed read to
    /// [`with_status_only`](RequestRecord::with_status_only) instead of
    /// [`with_recorded_response`](RequestRecord::with_recorded_response), so
    /// `response_recorded` stays `false` for that case.
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
    /// The hash is over the FULL body, whatever the recording cap says: `S2b`
    /// compares transparency by checksum, and a hash of a capped prefix would
    /// fail it on the large payload because of the cap — blaming the crate for
    /// something the harness did.
    pub fn record_of(body: &[u8], path: &str) -> RequestRecord {
        RequestRecord {
            path: path.to_string(),
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

    /// Completes a record whose response was STREAMED through, recording the
    /// status WITHOUT a body and leaving `response_recorded` false.
    ///
    /// **`response_recorded` means "the response BODY was buffered", not "a
    /// response arrived".** The status recorded here is the real one. Only the
    /// two probe paths buffer a body; everything else streams, so a completion's
    /// record legitimately carries a true status and `response_recorded: false`.
    /// A review round read the field the other way and proposed requiring it
    /// before trusting the status — which would have turned the happy-path
    /// scenario red on every live run.
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
    /// The upstream client, built ONCE at [`start`](SpyProxy::start) and reused
    /// by every forward.
    ///
    /// `forward` used to call `reqwest::Client::new()` per request, which throws
    /// away the connection pool the client exists to hold: every forwarded
    /// request paid a fresh TCP connect, and the real runs forward one per
    /// completion, per probe, per retry and per rotation. It also made each
    /// request open its own socket against the backend, which is a load pattern
    /// the crate never produces on its own — so the harness would have been
    /// measuring itself into the picture.
    ///
    /// `reqwest::Client` is internally reference-counted and `Clone` is cheap,
    /// so sharing it through `SpyProxy`'s own `Clone` (the serving task holds
    /// one) needs no `Arc` of our own.
    ///
    /// It carries a TOTAL request timeout, given to
    /// [`start`](SpyProxy::start) by the caller — see that function for where
    /// the value comes from and why the proxy must not be the component
    /// without a bound.
    client: reqwest::Client,
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
    /// Test-only: production reads a WINDOW of records, never all of them, so
    /// that a scenario sees its own run's traffic and not the previous run's.
    #[cfg(test)]
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

    /// Poisons the INJECTION lock specifically — as opposed to
    /// [`poison_for_test`](SpyProxy::poison_for_test), which poisons the
    /// separate request registry. The two mutexes recover independently, so
    /// proving one recovers says nothing about the other; this exists to
    /// prove the injection half on its own. A real panic while holding the
    /// guard, exactly like `poison_for_test`, not a flag standing in for one.
    #[cfg(test)]
    pub fn poison_injection_for_test(&self) {
        let injection = Arc::clone(&self.injection);
        let _ = std::thread::spawn(move || {
            let _g = injection.lock().expect("not poisoned yet");
            panic!("deliberate: poisons the injection mutex");
        })
        .join(); // Err(_) — the panic is expected
        debug_assert!(
            self.injection.lock().is_err(),
            "the injection lock must now be poisoned"
        );
    }

    /// A watermark into the shared registry. Runs share ONE proxy, so a
    /// snapshot taken after run 3 would hand run 3's assertions the records of
    /// runs 1 and 2 as well — and an assertion counting requests would be
    /// right about a set nobody asked about.
    ///
    /// Recovers a poisoned lock via `into_inner()` **and** marks `degraded`,
    /// exactly like [`push`](SpyProxy::push) and
    /// [`records_since`](SpyProxy::records_since). It used to answer `0` on a
    /// poisoned lock, which is the worst possible answer: `0` is a VALID
    /// watermark meaning "the registry was empty", so `records_since(0)` then
    /// hands the run every EARLIER run's records — the precise confusion this
    /// pair exists to prevent — while `is_degraded()` kept reporting clean and
    /// the assertions reading that foreign traffic could still report `Pass`.
    pub fn mark(&self) -> usize {
        match self.records.lock() {
            Ok(g) => g.len(),
            Err(p) => {
                self.degraded
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                p.into_inner().len()
            }
        }
    }

    /// Everything recorded SINCE `mark`. Taken once, when the run ends.
    ///
    /// Recovers a poisoned lock the same way its siblings do, `degraded`
    /// included: recovering the records while staying silent about the
    /// poisoning would let a run report clean over a registry that may be
    /// missing entries.
    pub fn records_since(&self, mark: usize) -> Vec<RequestRecord> {
        match self.records.lock() {
            Ok(g) => g.get(mark..).unwrap_or(&[]).to_vec(),
            Err(p) => {
                self.degraded
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                p.into_inner().get(mark..).unwrap_or(&[]).to_vec()
            }
        }
    }

    /// Sets the rule THROUGH the shared cell, so the already-spawned serving
    /// task sees it. Setting a field on `self` here would be invisible to
    /// that task, because `start()` handed the task a `clone()` of `SpyProxy`
    /// taken before this call — only the `Arc<Mutex<..>>` is shared, not the
    /// struct itself.
    ///
    /// `&self`, not `self` — a `Runner` changes this PER RUN on a proxy that
    /// is already shared by `Arc` with the serving task, so a consuming
    /// method would have no `Self` to hand back. `Option`, because a run
    /// without injection must be able to CLEAR a previous run's rule, or the
    /// next run inherits a failure nobody asked it for.
    ///
    /// Recovers a poisoned lock via `into_inner()` **and** marks `degraded`,
    /// so this side agrees with [`injected_response`](SpyProxy::injected_response):
    /// both halves of the injection path report the same poisoning instead
    /// of one of them staying silent about it.
    pub fn set_injection(&self, inj: Option<Injection>) {
        match self.injection.lock() {
            Ok(mut g) => *g = inj,
            Err(p) => {
                self.degraded
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                *p.into_inner() = inj;
            }
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

    /// Binds on an ephemeral port and serves for the rest of the process —
    /// there is no `Drop` impl, no abort handle, and no cancellation. The
    /// accept loop and every per-connection task it spawns keep running
    /// until the process itself exits.
    ///
    /// **That is deliberate, not an omission.** `magi-smoke` starts exactly
    /// ONE `SpyProxy` per invocation (`raise_proxy` in the preflight,
    /// `Runner::new` takes that single instance), and every scenario in the
    /// run shares it — [`mark`](SpyProxy::mark) and
    /// [`records_since`](SpyProxy::records_since) exist precisely because
    /// scenarios take turns on one proxy rather than each getting its own.
    /// The binary is a short-lived CLI that runs its scenarios and exits;
    /// at that point the OS reclaims the listener and every spawned task,
    /// the same trade-off the test-time `testkit::spawn_echo_server`
    /// already makes for its own fixture server. A cancellation handle would
    /// be API surface with no caller: nothing in this milestone ever needs
    /// to stop a proxy mid-process, only to let the process end.
    ///
    /// **Everything goes through here**: there is no with-proxy and
    /// without-proxy mode, so a scenario that bypassed it would observe
    /// nothing.
    ///
    /// # Parameters
    ///
    /// * `upstream` — base URL of the real backend every request is forwarded
    ///   to.
    /// * `payload_target_bytes` — sizes the per-record body cap (see
    ///   [`max_recorded_body`]).
    /// * `upstream_timeout` — TOTAL bound on one forwarded request, from
    ///   connect to the last body byte.
    ///
    /// # The upstream bound, and why it is passed in
    ///
    /// A backend that accepts a connection and then answers nothing would hang
    /// the forward. The run's own budget does eventually cut it, so nothing
    /// hangs forever — but the proxy would be the only component in the path
    /// with no bound of its own, and that is the component that ends up blamed
    /// for a backend fault.
    ///
    /// The value is DERIVED, not invented:
    /// [`Config::longest_backend_budget`](crate::config::Config::longest_backend_budget)
    /// is what the production caller passes, so the proxy's bound is always at
    /// least as long as the longest run it can be serving. That ordering is
    /// the point — if the proxy cut first, a slow-but-legal backend would
    /// surface as a proxy error, and a proxy error is a HARNESS fault reported
    /// as "cannot test" rather than as a verdict about the crate.
    pub async fn start(
        upstream: String,
        payload_target_bytes: usize,
        upstream_timeout: Duration,
    ) -> Result<Self, std::io::Error> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let base_url = format!("http://{}", listener.local_addr()?);
        // Built here rather than per request (see the `client` field), and
        // through `builder().build()` rather than `Client::new()` so a client
        // that cannot be constructed becomes this function's error instead of a
        // panic inside a dependency.
        let client = reqwest::Client::builder()
            .timeout(upstream_timeout)
            .build()
            .map_err(std::io::Error::other)?;
        let this = Self {
            base_url,
            records: Arc::new(Mutex::new(Vec::new())),
            degraded: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            injection: Arc::new(Mutex::new(None)),
            record_cap: max_recorded_body(payload_target_bytes),
            client,
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
            // The request could not be read in full. **Nothing is forwarded.**
            //
            // This used to substitute `Bytes::new()` and forward that, so the
            // backend received a request the client never sent — an EMPTY body
            // in place of whatever was on the wire. Everything downstream then
            // described that fabrication: the record's hash identified a body
            // nobody sent, and the transparency scenario, which compares the
            // proxied request against a direct one by checksum, would have
            // reported a difference the CRATE never introduced. A proxy that
            // cannot read its input has exactly three honest options —
            // fail, drop, or read it correctly — and forwarding something
            // else is not among them.
            //
            // The failure is made visible on both sides instead of being
            // swallowed: `degraded` is latched, which routes every assertion
            // that reads the registry to SKIP, and the client is answered with
            // [`REQUEST_UNREADABLE_STATUS`] naming the proxy as the party that
            // failed.
            //
            // **No record is pushed, deliberately.** Every field of one would be
            // a claim about a request that was never fully received:
            // `body_sha256` is computed over the FULL body precisely so the
            // transparency comparison can trust it, and hashing a truncated
            // prefix would put a confident, wrong checksum into the registry —
            // the same class of lie as recording an unreadable response as an
            // empty one (see [`RequestRecord::response_recorded`]). Absence
            // here is not silence: `degraded` is the signal, and it is louder
            // than a row.
            Err(_) => {
                self.degraded
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                return Ok(hyper::Response::builder()
                    .status(REQUEST_UNREADABLE_STATUS)
                    .body(fixed(REQUEST_UNREADABLE_BODY))
                    // LOCAL, not relay: nothing was forwarded here, so a 502
                    // would name an upstream leg that never existed.
                    .unwrap_or_else(|_| build_failed(LOCAL_BUILD_FAILED_STATUS)));
            }
        };
        let rec = RequestRecord::record_of(&bytes, &path);

        // ONE push per request, AFTER the answer exists, so both halves land
        // in the same record. This connection owns `rec` until then — no
        // second lock, no index into a shared Vec, and no chance of
        // completing someone else's row.
        if let Some((status, payload)) = self.injected_response(&bytes) {
            self.push(rec.with_recorded_response(status, &payload, self.record_cap));
            return Ok(hyper::Response::builder()
                .status(status)
                .body(fixed(&payload))
                // The ONE site of the five that a caller can actually reach:
                // `status` comes from the configured `Injection`, and a `u16`
                // outside `100..1000` is not a `StatusCode`. LOCAL, because an
                // injection is the harness standing in FOR the backend — no
                // forward was attempted, so there is no gateway leg to blame.
                //
                // `degraded` is latched for the same reason the unreadable-body
                // path above latches it: without that, a harness defect arrives
                // at the crate as a plain server error and a scenario can go RED
                // for something the proxy did. Degraded routes it to SKIP.
                .unwrap_or_else(|_| {
                    self.degraded
                        .store(true, std::sync::atomic::Ordering::SeqCst);
                    build_failed(LOCAL_BUILD_FAILED_STATUS)
                }));
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
            // `None` means the body read failed: record NOTHING was
            // recorded (`with_status_only`), never an invented empty
            // response (`with_recorded_response(status, &[], ..)` would
            // look exactly like a backend that genuinely answered empty).
            let (rec, body) = match out {
                Some(body) => (
                    rec.with_recorded_response(status, &body, self.record_cap),
                    body,
                ),
                // The status is real; the BODY could not be read. Recording the
                // status alone already stops it being reported as a genuine
                // empty answer — but a scenario comparing response bytes would
                // still see nothing and blame the crate. Marking the proxy
                // degraded is what lets that scenario SKIP instead, and the
                // request-body path two matches above already does exactly this.
                None => {
                    self.degraded
                        .store(true, std::sync::atomic::Ordering::SeqCst);
                    (rec.with_status_only(status), Vec::new())
                }
            };
            self.push(rec);
            return Ok(hyper::Response::builder()
                .status(status)
                .body(fixed(&body))
                // RELAY: this response is the upstream's, buffered on the way
                // through, so failing to build it is a failure to relay.
                .unwrap_or_else(|_| build_failed(RELAY_BUILD_FAILED_STATUS)));
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
    ///
    /// Returns `(status, None)` when the body could not be read back in
    /// full — the status IS real (headers already arrived), but there is no
    /// genuine body to hand the caller. `None`, not `Some(Vec::new())`: an
    /// empty `Vec` would be indistinguishable from a backend that truly
    /// answered with an empty body, and the caller must be able to tell
    /// "nothing was recorded" from "an empty response was recorded" — see
    /// the fix note on [`RequestRecord::response_recorded`].
    async fn forward_buffered(
        &self,
        method: &str,
        path: &str,
        headers: hyper::HeaderMap,
        body: Bytes,
        upstream: &str,
    ) -> (u16, Option<Vec<u8>>) {
        let resp = self.forward(method, path, headers, body, upstream).await;
        let status = resp.status().as_u16();
        match resp.into_body().collect().await {
            Ok(c) => (status, Some(c.to_bytes().to_vec())),
            // Could not read it back: report the status and NOTHING else.
            // The CALLER must route this to `with_status_only`, never to
            // `with_recorded_response` with an empty body — that would
            // record a genuine-looking empty answer for a read that failed.
            Err(_) => (status, None),
        }
    }

    /// **Returns `(status, body)`, NOT a `hyper::Response`** — the injection
    /// logic has no business knowing the server's types.
    ///
    /// Returns the canned response when the request body names the injected
    /// model, or `None` to forward. Matching on the body's `model` field is
    /// what makes injection surgical: rotation needs the FIRST candidate to
    /// fail and the second to go through, so a matcher that fired on every
    /// request would fail that scenario silently.
    ///
    /// A poisoned lock is recovered the same way `push`/`records` recover
    /// theirs: `into_inner()` gets the rule back instead of treating
    /// poisoning as "no injection", and `degraded` is set. Poisoning is
    /// sticky — every later `lock()` on this mutex keeps failing — so
    /// treating it as silent "forward everything" would make a
    /// rotation scenario stop injecting for the rest of the run while
    /// `is_degraded()` kept reporting clean. That is the harness lying about
    /// what it did.
    fn injected_response(&self, body: &[u8]) -> Option<(u16, Vec<u8>)> {
        let guard = match self.injection.lock() {
            Ok(g) => g,
            Err(p) => {
                self.degraded
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                p.into_inner()
            }
        };
        let inj = guard.as_ref()?;
        let text = String::from_utf8_lossy(body);
        match inj {
            Injection::FailModel { model, status } if names_model(&text, model) => {
                Some((*status, Vec::new()))
            }
            _ => None,
        }
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
        let url = format!("{}{}", upstream.trim_end_matches('/'), path);
        let m = reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::POST);

        let mut req = self.client.request(m, &url).body(body.to_vec());
        for (name, value) in headers.iter() {
            // Hop-by-hop headers must not be forwarded (see
            // [`HOP_BY_HOP_HEADERS`]); this list used to name only `connection`,
            // leaving `transfer-encoding`, `te`, `upgrade` and their kin to be
            // relayed end to end.
            //
            // `host` and `content-length` are dropped for a different reason
            // and are NOT hop-by-hop: `host` would still point at the proxy,
            // and `content-length` describes the body as it arrived, which
            // `reqwest` recomputes for the request it is about to write.
            if is_hop_by_hop(name.as_str()) || matches!(name.as_str(), "host" | "content-length") {
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
                    // The RESPONSE path had no filter at all, so every per-hop
                    // header the upstream set was relayed to the crate. The one
                    // that actually corrupts rather than merely misinforms is
                    // `transfer-encoding`: the body below is re-framed by
                    // `hyper` as it writes it, so announcing the encoding of the
                    // connection it was READ from describes a framing that is
                    // not the one being sent.
                    //
                    // `content-length` is kept: it is end-to-end, it is the same
                    // number for the same body, and the stream forwards exactly
                    // those bytes.
                    if is_hop_by_hop(n.as_str()) {
                        continue;
                    }
                    out = out.header(n.as_str(), v.as_bytes());
                }
                // RELAY: the upstream answered and this is the response
                // carrying its answer back, headers included.
                out.body(boxed(StreamBody::new(stream)))
                    .unwrap_or_else(|_| build_failed(RELAY_BUILD_FAILED_STATUS))
            }
            // RELAY on both halves: the build that failed here was itself the
            // report of an upstream failure, so the fallback keeps saying
            // "upstream" rather than changing the story.
            Err(_) => hyper::Response::builder()
                .status(UPSTREAM_UNREACHABLE_STATUS)
                .body(fixed(b"upstream unreachable"))
                .unwrap_or_else(|_| build_failed(RELAY_BUILD_FAILED_STATUS)),
        }
    }
}

/// Whether a raw request body names `model` in its top-level `"model"` field.
///
/// A body that fails to parse as JSON, or that has no `model` field, is
/// treated as a non-match rather than an error: a malformed body is the
/// SUT's problem to surface, not the proxy's business to reject, and
/// `injected_response` already forwards a `None` as "no injection applies".
fn names_model(body_text: &str, model: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(body_text)
        .ok()
        .and_then(|v| v.get("model").and_then(|m| m.as_str().map(str::to_owned)))
        .is_some_and(|m| m == model)
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

    /// Upstream bound for the proxies these tests raise.
    ///
    /// Deliberately far longer than anything a local fixture server takes, so
    /// it never becomes the reason a test fails: what these tests are about is
    /// the proxy's recording and injection, never its timing. Production
    /// derives its value from the configured budgets instead — see
    /// [`SpyProxy::start`].
    const TEST_UPSTREAM_TIMEOUT: Duration = Duration::from_secs(30);

    #[test]
    fn an_absurd_payload_target_does_not_overflow_the_recorded_body_cap() {
        // `max_recorded_body` computes `payload_target_bytes * 2`, and that field
        // has only a MINIMUM bound in `Config`, so a huge-but-legal value
        // overflows here: a panic in debug builds, and in release a silent wrap
        // to a cap SMALLER than the payload it exists to hold — which truncates
        // the very record the large-payload scenario reads.
        //
        // The sibling of the same defect in `config::validate_probe_window` was
        // the one reported. Fixing the reported instance and leaving this one is
        // how the class has survived every previous round.
        for absurd in [usize::MAX, usize::MAX / 2 + 1] {
            let cap = max_recorded_body(absurd);
            assert!(
                cap >= MIN_RECORDED_BODY_CAP,
                "a cap that wrapped is below even the absolute floor: {cap}"
            );
        }
        // The other side, so the fix cannot be "always answer the floor": a
        // target the arithmetic can hold must still set the cap.
        assert_eq!(
            max_recorded_body(MIN_RECORDED_BODY_CAP),
            2 * MIN_RECORDED_BODY_CAP
        );
    }

    #[tokio::test]
    async fn records_the_full_request_body_by_hash_not_just_the_path() {
        // The record identifies the WHOLE body, not merely the envelope: the
        // hash is taken over every byte that went on the wire, so a proxy that
        // recorded only the path — or that hashed a capped prefix — fails here.
        let upstream = crate::testkit::spawn_echo_server().await;
        let proxy = SpyProxy::start(upstream.url(), 250_000, TEST_UPSTREAM_TIMEOUT)
            .await
            .expect("proxy bind");
        // A literal body, so the exact bytes hashed are known to the test
        // rather than dependent on how `serde_json` orders a map.
        const BODY: &str = r#"{"model":"m","stream":false}"#;
        let client = reqwest::Client::new();
        client
            .post(format!("{}/api/chat", proxy.base_url()))
            .body(BODY)
            .send()
            .await
            .unwrap();

        let rec = &proxy.records()[0];
        assert_eq!(rec.path, "/api/chat");
        assert_eq!(
            rec.body_sha256,
            sha256_hex(BODY.as_bytes()),
            "the record must identify the full request body, not just its path"
        );
    }

    /// A `u16` that is not a status code: `StatusCode` accepts `100..1000`, so
    /// `hyper::Response::builder().status(0)` stores an error the following
    /// `.body(..)` hands back. It is the one input that reaches a build-failure
    /// fallback without editing the proxy, because it is the only status of the
    /// five that a CALLER supplies rather than the code.
    const UNBUILDABLE_STATUS: u16 = 0;

    #[tokio::test]
    async fn a_response_the_proxy_cannot_build_is_an_error_not_a_fabricated_success() {
        // The fallback used to be `hyper::Response::new(empty_body())`, which is
        // a **200**: a proxy that could not build its response answered SUCCESS
        // over a failure — the harness fabricating the one outcome it exists to
        // catch, in the component whose whole contract is never to invent a
        // verdict.
        //
        // Driven through `set_injection`, the proxy's own public API, with a
        // status no `StatusCode` can hold. Nothing about the request is
        // special: the builder is what fails.
        let upstream = crate::testkit::spawn_echo_server().await;
        let proxy = SpyProxy::start(upstream.url(), 250_000, TEST_UPSTREAM_TIMEOUT)
            .await
            .expect("proxy bind");
        proxy.set_injection(Some(Injection::FailModel {
            model: "bad".into(),
            status: UNBUILDABLE_STATUS,
        }));

        let r = reqwest::Client::new()
            .post(format!("{}/api/chat", proxy.base_url()))
            .body(r#"{"model":"bad"}"#)
            .send()
            .await
            .expect("the proxy must still ANSWER: a build failure is a status, not a hang");

        assert_ne!(
            r.status().as_u16(),
            200,
            "a response the proxy could not build must never come back as success"
        );
        assert_eq!(
            r.status().as_u16(),
            LOCAL_BUILD_FAILED_STATUS.as_u16(),
            "an injection is the harness standing in for the backend, so the failure is \
             LOCAL: no forward was attempted and a 502 would name a gateway leg that \
             never existed"
        );
        assert!(
            proxy.is_degraded(),
            "and it must SAY so, or a harness defect reaches the crate as a plain server \
             error and a scenario goes red for something the proxy did"
        );
    }

    #[tokio::test]
    async fn a_poisoned_registry_does_not_report_a_zero_watermark() {
        // `0` is a VALID watermark meaning "the registry was empty", so
        // answering `0` on a poisoned lock hands the next run every earlier
        // run's records — the exact confusion `mark`/`records_since` exist to
        // prevent — and does it while `is_degraded()` reports clean.
        let upstream = crate::testkit::spawn_echo_server().await;
        let proxy = SpyProxy::start(upstream.url(), 250_000, TEST_UPSTREAM_TIMEOUT)
            .await
            .expect("proxy bind");
        let client = reqwest::Client::new();
        client
            .post(format!("{}/api/chat", proxy.base_url()))
            .body("{}")
            .send()
            .await
            .unwrap();
        assert!(
            !proxy.is_degraded(),
            "nothing has gone wrong yet, so the flag must still be clean"
        );

        proxy.poison_for_test(); // a real poisoning, not a flag

        assert_eq!(
            proxy.mark(),
            1,
            "a poisoned registry must recover the real watermark, never fall back to 0"
        );
        assert!(
            proxy.is_degraded(),
            "a poisoned registry must be visible on the degraded flag, or the run \
             reports clean over a registry that may be missing entries"
        );
    }

    #[tokio::test]
    async fn a_poisoned_registry_makes_records_since_report_degraded_too() {
        // Its sibling `push` already marked `degraded`; `records_since`
        // recovered the records but said nothing, so a run whose registry was
        // poisoned only between the push and the read stayed silent about it.
        let upstream = crate::testkit::spawn_echo_server().await;
        let proxy = SpyProxy::start(upstream.url(), 250_000, TEST_UPSTREAM_TIMEOUT)
            .await
            .expect("proxy bind");
        let client = reqwest::Client::new();
        client
            .post(format!("{}/api/chat", proxy.base_url()))
            .body("{}")
            .send()
            .await
            .unwrap();
        assert!(!proxy.is_degraded());

        proxy.poison_for_test();

        assert_eq!(proxy.records_since(0).len(), 1, "the records are recovered");
        assert!(
            proxy.is_degraded(),
            "and the poisoning is reported rather than swallowed"
        );
    }

    #[tokio::test]
    async fn a_poisoned_registry_degrades_but_the_proxy_keeps_serving() {
        // A partial registry degrades the assertions that read it; a dead
        // proxy fails EVERY scenario. Failing forward and saying so is the
        // right direction.
        let upstream = crate::testkit::spawn_echo_server().await;
        let proxy = SpyProxy::start(upstream.url(), 250_000, TEST_UPSTREAM_TIMEOUT)
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

    #[tokio::test]
    async fn fail_model_only_affects_the_named_model() {
        let upstream = crate::testkit::spawn_echo_server().await;
        let proxy = SpyProxy::start(upstream.url(), 250_000, TEST_UPSTREAM_TIMEOUT)
            .await
            .expect("proxy bind");
        proxy.set_injection(Some(Injection::FailModel {
            model: "bad".into(),
            status: 500,
        }));
        let c = reqwest::Client::new();
        let bad = c
            .post(format!("{}/api/chat", proxy.base_url()))
            .body(r#"{"model":"bad"}"#)
            .send()
            .await
            .unwrap();
        let good = c
            .post(format!("{}/api/chat", proxy.base_url()))
            .body(r#"{"model":"good"}"#)
            .send()
            .await
            .unwrap();
        assert_eq!(bad.status(), 500);
        assert_eq!(
            good.status(),
            200,
            "injection must be surgical: rotation needs the second \
             candidate to go through"
        );
    }

    #[tokio::test]
    async fn a_poisoned_injection_lock_keeps_injecting_and_marks_degraded() {
        // Poisoning is STICKY: once the injection mutex is poisoned, every
        // later `lock()` on it keeps failing. `injected_response` and
        // `set_injection` must both recover the rule (so a rotation scenario
        // does not silently stop injecting mid-run) AND mark `degraded` (so
        // the run does not report clean while a lock was poisoned).
        let upstream = crate::testkit::spawn_echo_server().await;
        let proxy = SpyProxy::start(upstream.url(), 250_000, TEST_UPSTREAM_TIMEOUT)
            .await
            .expect("proxy bind");
        proxy.set_injection(Some(Injection::FailModel {
            model: "bad".into(),
            status: 500,
        }));
        proxy.poison_injection_for_test(); // a real poisoning, not a flag

        let bad = reqwest::Client::new()
            .post(format!("{}/api/chat", proxy.base_url()))
            .body(r#"{"model":"bad"}"#)
            .send()
            .await
            .unwrap();

        assert_eq!(
            bad.status(),
            500,
            "a poisoned injection lock must still recover the rule and \
             inject, not silently start forwarding"
        );
        assert!(
            proxy.is_degraded(),
            "a poisoned injection lock must be visible on the degraded \
             flag, or the run reports clean while a lock was poisoned"
        );
    }

    #[tokio::test]
    async fn a_broken_response_read_is_not_recorded_as_an_empty_answer() {
        // The upstream promises a 1000-byte body and delivers 10, then closes
        // the connection: `forward_buffered`'s body read must fail. Before
        // the fix, that failure was recorded as a genuine empty response
        // (`response_recorded: true`, empty body) — indistinguishable from a
        // backend that truly answered with nothing.
        let upstream = crate::testkit::spawn_truncating_server().await;
        let proxy = SpyProxy::start(upstream.url(), 250_000, TEST_UPSTREAM_TIMEOUT)
            .await
            .expect("proxy bind");
        let client = reqwest::Client::new();
        // `/api/tags` is a RECORDED_RESPONSE_PATHS entry, so this goes
        // through `forward_buffered`, not the streaming path.
        let _ = client
            .post(format!("{}/api/tags", proxy.base_url()))
            .body("{}")
            .send()
            .await;

        let rec = &proxy.records()[0];
        assert_eq!(rec.path, "/api/tags");
        assert!(
            !rec.response_recorded,
            "a failed body read must be recorded as NOTHING recorded, \
             never as a genuine (empty) response"
        );
        assert!(
            proxy.is_degraded(),
            "a proxy that could not read a response body must SAY so: without \
             this a scenario comparing response bytes sees nothing and blames \
             the crate for what the proxy failed to do"
        );
    }

    #[tokio::test]
    async fn an_unreadable_request_is_not_forwarded_as_an_empty_one() {
        // The client promises 1000 body bytes and sends 10, then closes its
        // write half: the proxy's `body.collect()` must fail. Before the fix it
        // substituted an EMPTY body and forwarded that, so the backend received
        // a request the client never sent — and the transparency scenario, which
        // compares proxied against direct by checksum, could report a difference
        // the crate never introduced.
        //
        // The load-bearing assertion is the one the proxy's own registry cannot
        // make: that the upstream received NOTHING.
        let upstream = crate::testkit::spawn_echo_server().await;
        let proxy = SpyProxy::start(upstream.url(), 250_000, TEST_UPSTREAM_TIMEOUT)
            .await
            .expect("proxy bind");
        assert_eq!(upstream.received(), 0, "nothing has been sent yet");

        crate::testkit::send_truncated_request(&proxy.base_url(), "/api/chat").await;

        assert_eq!(
            upstream.received(),
            0,
            "a request the proxy could not read must not reach the backend at all: \
             forwarding a substitute for it is the one option that is wrong"
        );
        assert!(
            proxy.is_degraded(),
            "and the failure must be visible rather than silent, or a run reports clean \
             over a request it mishandled"
        );
        assert!(
            proxy.records().is_empty(),
            "no record either: every field of one would describe a request that was never \
             fully received, and `body_sha256` in particular is what the transparency \
             comparison trusts"
        );
    }

    #[tokio::test]
    async fn hop_by_hop_headers_are_not_relayed_in_either_direction() {
        // Relaying `transfer-encoding` is the one that corrupts rather than
        // merely misinforms: both directions are re-framed by the library that
        // writes them, so announcing the encoding of the connection the message
        // was READ from describes framing that is not the one being sent.
        //
        // Asserted on the classifier, per name, rather than through a live
        // exchange: `hyper` and `reqwest` both strip and regenerate these
        // themselves, so a round trip would go green whether the filter existed
        // or not — which is precisely the mechanism-that-reports-success shape
        // this milestone keeps producing.
        for name in HOP_BY_HOP_HEADERS {
            assert!(is_hop_by_hop(name), "{name} must be filtered");
        }
        // End-to-end headers must survive, or the forward stops being verbatim
        // and the transparency claim stops being checkable.
        for name in ["content-type", "authorization", "accept", "user-agent"] {
            assert!(
                !is_hop_by_hop(name),
                "{name} is end-to-end and must be forwarded untouched"
            );
        }
        // `content-length` is NOT hop-by-hop, and the distinction is
        // load-bearing: the request path drops it for its own reason (the body
        // is rewritten by `reqwest`), while the response path keeps it.
        assert!(!is_hop_by_hop("content-length"));
    }

    #[tokio::test]
    async fn by_default_it_forwards_and_injects_nothing() {
        let upstream = crate::testkit::spawn_echo_server().await;
        let proxy = SpyProxy::start(upstream.url(), 250_000, TEST_UPSTREAM_TIMEOUT)
            .await
            .expect("proxy bind");
        let r = reqwest::Client::new()
            .post(format!("{}/api/chat", proxy.base_url()))
            .body("{}")
            .send()
            .await
            .unwrap();
        assert_eq!(
            r.status(),
            200,
            "there must never be a mock in front of the happy path"
        );
    }

    /// Shorter than [`crate::testkit::stub_that_is_always_slow`]'s delay, so
    /// the bound is what ends the forward and the stub never gets to answer.
    const SHORT_UPSTREAM_TIMEOUT: Duration = Duration::from_millis(100);

    /// Comfortably above [`SHORT_UPSTREAM_TIMEOUT`] and comfortably below the
    /// stub's own delay, so the assertion distinguishes "the bound cut it"
    /// from "the stub eventually answered" without racing a loaded machine.
    const CUT_BEFORE: Duration = Duration::from_millis(400);

    #[tokio::test]
    async fn a_backend_that_never_answers_is_cut_by_the_proxys_own_bound() {
        // The proxy used to build its forward client with no timeout at all:
        // an upstream that accepts a connection and then says nothing would
        // hold the forward open until the RUN's budget expired. Nothing hung
        // forever, but the proxy was the one component in the path without a
        // bound of its own — and an unbounded harness component is what ends
        // up wearing a backend fault.
        //
        // Asserting the 502 alone would pass just as well against a proxy with
        // no timeout, because the stub does answer in the end. The ELAPSED
        // time is what separates the two.
        let upstream = crate::testkit::stub_that_is_always_slow().await;
        let proxy = SpyProxy::start(upstream.url(), 250_000, SHORT_UPSTREAM_TIMEOUT)
            .await
            .expect("proxy bind");
        let started = std::time::Instant::now();
        let r = reqwest::Client::new()
            .post(format!("{}/api/chat", proxy.base_url()))
            .body("{}")
            .send()
            .await
            .expect(
                "the proxy must ANSWER, not hang: a cut forward is a 502, not a dropped \
                     connection",
            );
        let elapsed = started.elapsed();
        assert_eq!(
            r.status(),
            UPSTREAM_UNREACHABLE_STATUS,
            "a forward the proxy cut short is an upstream problem the crate can classify"
        );
        assert!(
            elapsed < CUT_BEFORE,
            "the proxy's own bound must be what ends the forward, but it took {elapsed:?} — \
             which is the stub answering, not the timeout firing"
        );
    }
}
