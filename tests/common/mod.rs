// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-07-31

//! Loopback server primitives shared by the integration tests that need one.
//!
//! # Why this module exists
//!
//! Two test files each grew their own hand-rolled one-shot HTTP server, and the duplicated part
//! was **the subtle part**: the two portability fixes below. Duplicated tricky code diverges, and
//! it diverges in the tricky place — one copy gets a fix and the other keeps the bug, with the
//! symptom appearing as a flaky test nobody trusts.
//!
//! What is NOT shared is response composition. Each test knows what it needs to send; only the
//! socket handling is common.
//!
//! # The two fixes, both paid for the hard way
//!
//! 1. **On Windows an accepted socket INHERITS the listener's non-blocking mode** (on Linux it does
//!    not). The listener must be non-blocking so `accept` can honour a deadline, so without an
//!    explicit reset a read issued before the request bytes land returns `WouldBlock` — not an
//!    error, but it reads like one, and the handler aborts.
//! 2. **Dropping a socket with unread bytes makes the OS send RST instead of FIN.** A client that
//!    takes an RST mid-response reports a connection error rather than the response it already
//!    received, so it never follows a redirect. Half-close, drain to EOF, then drop.
//!
//! Both failed on the FIRST listener while the symptom appeared on the SECOND, which waited out
//! its whole deadline for a connection the client had already abandoned. If a test using this
//! starts failing at exactly the deadline, look here before suspecting the HTTP client.

// Each test binary that declares `mod common;` compiles the whole module and uses a subset of it,
// so items unused by one binary are expected rather than dead. Scoped to this shared helper.
#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

/// Generous on purpose: two orders of magnitude above a loopback exchange, so exceeding it means
/// something is actually broken rather than that the runner was busy. If a test using this flakes
/// in CI, raise the deadline — do not add retries, which start averaging away a real failure.
pub const DEADLINE: Duration = Duration::from_secs(10);

/// Bounds the post-response drain. Short because the peer has been told the connection closes, so
/// reaching EOF is the expected case and this only caps the pathological one.
pub const DRAIN: Duration = Duration::from_millis(500);

/// Ceiling on the accumulated request head, so a peer that never sends the terminator cannot grow
/// the buffer without bound. Far above any request these tests provoke.
const MAX_HEAD: usize = 64 * 1024;

/// True once the accumulated bytes contain the end of the HTTP header block.
///
/// Only the CRLF form is accepted: these servers talk to one client, which speaks it. A bare-LF
/// request would time out rather than be misread, which is the direction to fail in.
fn ends_header(buf: &[u8]) -> bool {
    buf.windows(4).any(|w| w == b"\r\n\r\n")
}

/// Binds a loopback listener on an OS-assigned port.
///
/// Port 0 on purpose: a fixed port collides with the environment and with other tests.
pub fn bind_loopback() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr").to_string();
    (listener, addr)
}

/// Accepts one connection within [`DEADLINE`], reads the request head, and hands the caller a
/// writable stream plus what it read.
///
/// Returns `None` if no client connected before the deadline. A plain blocking `accept()` would
/// hang the whole CI job instead — the worst failure mode for a security tripwire, because nobody
/// can tell a hang from a dead runner.
pub fn accept_one(listener: &TcpListener) -> Option<(std::net::TcpStream, String)> {
    listener.set_nonblocking(true).ok()?;
    let start = Instant::now();
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                // Fix 1. MUST come before the first read.
                stream.set_nonblocking(false).ok()?;
                stream.set_read_timeout(Some(DEADLINE)).ok()?;
                // Reads until the header terminator, NOT once. A single `read` returns whatever
                // one TCP segment happened to carry, so a request split across segments could
                // arrive with `Authorization` in the part not yet read — and the assertion that
                // the header is ABSENT would then pass while it was merely late. A security test
                // that can pass without seeing the evidence is worse than no test.
                let mut head = Vec::new();
                let mut buf = [0u8; 4096];
                while !ends_header(&head) && head.len() < MAX_HEAD {
                    match stream.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => head.extend_from_slice(&buf[..n]),
                        Err(_) => break,
                    }
                }
                return Some((stream, String::from_utf8_lossy(&head).into_owned()));
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if start.elapsed() > DEADLINE {
                    return None;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => return None,
        }
    }
}

/// Closes a served connection cleanly: half-close, drain to EOF, then drop.
///
/// Fix 2. Skipping this turns a delivered response into a connection error at the client.
pub fn hang_up(mut stream: std::net::TcpStream) {
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Write);
    let _ = stream.set_read_timeout(Some(DRAIN));
    let mut buf = [0u8; 4096];
    while matches!(stream.read(&mut buf), Ok(n) if n > 0) {}
}

// ---------------------------------------------------------------------------
// MS2 — shared retry-budget helpers (Task 1a)
//
// Five helpers are shared across five MS2 tasks. They live here rather than being written
// wherever they are first needed, because a helper created twice is how two of them diverge.
// ---------------------------------------------------------------------------

/// Collects the `tracing::warn!` messages emitted while `f` runs.
///
/// # Why this exists rather than a crate API
///
/// The crate reports dangerous configuration through `tracing`, and this milestone adds no
/// public surface for reading it back. There is no `Magi::warnings()`.
///
/// # Scope — it does NOT see everything, and that is stated rather than discovered
///
/// `tracing::subscriber::with_default` is **thread-local** and scoped to the closure. Warnings
/// emitted from a spawned task (`tokio::spawn` runs on another runtime thread) are NOT captured,
/// nor is anything outside `f`.
///
/// That covers exactly the case it is used for: every warning this milestone verifies is emitted
/// from `RetryConfig::dangerous_settings`, which runs synchronously inside
/// `RetryProvider::with_config` on the calling thread. Saying so keeps someone from reusing it
/// for a warning emitted mid-run and reading the empty result as silence.
///
/// # Isolation
///
/// `with_default` rather than `set_global_default`: the latter is a `OnceCell`, so under
/// `cargo test` (one process, many threads) one test would read another's warnings. Thread-local
/// installation makes that impossible. Under `cargo nextest` each test is its own process anyway.
///
/// # No new dependency
///
/// Implemented against `tracing` alone. `tracing-subscriber` is not a dependency of this crate,
/// and adding one for a test helper would break the milestone's zero-new-dependencies rule.
#[cfg(feature = "test-utils")]
pub fn captured_warnings<F: FnOnce()>(f: F) -> Vec<String> {
    use std::sync::{Arc, Mutex};
    use tracing::field::{Field, Visit};
    use tracing::span::{Attributes, Id, Record};
    use tracing::{Event, Level, Metadata, Subscriber};

    #[derive(Default)]
    struct Collector {
        lines: Arc<Mutex<Vec<String>>>,
    }

    /// Pulls the formatted body out of an event's fields.
    ///
    /// `warn!("{x}")` lands in the `message` field; a structured `warn!(a = 1)` does not, so
    /// every field is recorded and the message is preferred when present.
    #[derive(Default)]
    struct Grab {
        message: Option<String>,
        others: Vec<String>,
    }

    impl Visit for Grab {
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            let rendered = format!("{value:?}");
            if field.name() == "message" {
                self.message = Some(rendered);
            } else {
                self.others.push(format!("{}={}", field.name(), rendered));
            }
        }
    }

    impl Subscriber for Collector {
        fn enabled(&self, metadata: &Metadata<'_>) -> bool {
            *metadata.level() <= Level::WARN
        }
        fn new_span(&self, _: &Attributes<'_>) -> Id {
            Id::from_u64(1)
        }
        fn record(&self, _: &Id, _: &Record<'_>) {}
        fn record_follows_from(&self, _: &Id, _: &Id) {}
        fn event(&self, event: &Event<'_>) {
            if *event.metadata().level() > Level::WARN {
                return;
            }
            let mut grab = Grab::default();
            event.record(&mut grab);
            let line = grab.message.unwrap_or_else(|| grab.others.join(" "));
            if let Ok(mut guard) = self.lines.lock() {
                guard.push(line);
            }
        }
        fn enter(&self, _: &Id) {}
        fn exit(&self, _: &Id) {}
    }

    let lines: Arc<Mutex<Vec<String>>> = Arc::default();
    let collector = Collector {
        lines: Arc::clone(&lines),
    };
    tracing::subscriber::with_default(collector, f);
    lines.lock().map(|g| g.clone()).unwrap_or_default()
}

/// The client timeout the hanging seat runs under. Public so the absorbed-retry assertion can
/// derive its threshold instead of repeating a number that would decouple in silence.
#[cfg(all(feature = "test-utils", feature = "openai-compat"))]
pub const HANGING_SEAT_CLIENT_TIMEOUT: Duration = Duration::from_millis(300);

#[cfg(all(feature = "test-utils", feature = "openai-compat"))]
/// A run whose Caspar seat talks to a backend that answers headers and never a body.
///
/// The hanging provider is WRAPPED in a `RetryProvider`, which absorbs its own retries — so
/// from the orchestrator's side this is **one** call and therefore **one** completion record.
/// The wrapping is what makes the claim testable: unwrapped, one record would follow from one
/// call whether retries were absorbed or not. That is the point: it is the counter-example to
/// the corrective-retry helper below, which produces two.
///
/// # Feature gate, stated rather than discovered
///
/// Needs `openai-compat` for a real HTTP provider and `test-utils` for the trio builders. The
/// hanging server itself is `support::mock_server`, already used by two integration suites.
pub async fn run_against_a_hanging_backend()
-> Result<magi_core::reporting::MagiReport, magi_core::error::MagiError> {
    use magi_core::prelude::*;
    use magi_core::rotation::Lineage;
    use magi_core::test_support::{Beh, ScriptProvider};
    use std::sync::Arc;
    use std::time::Duration;

    #[path = "../support/mock_server.rs"]
    mod mock_server;

    let (url, handle) = mock_server::spawn_hanging_headers().await;

    // A short client timeout: the property is that the seat is lost to a hang, not how long a
    // test is willing to sit still for it.
    let hanging =
        OpenAiCompatibleProvider::with_timeout(url, "m-hanging", None, HANGING_SEAT_CLIENT_TIMEOUT)
            .expect("the hanging provider builds");

    // WRAPPED in a `RetryProvider`, and that is load-bearing rather than decoration: without it
    // the single record would come from a single call and the test would pass identically
    // whether or not retries are absorbed — proving nothing about the mechanism it names.
    // `MagiBuilder::build` does not wrap providers itself, which its own rustdoc states.
    // Fields over `Default`: `RetryConfig` is `#[non_exhaustive]`, so the struct literal does not
    // compile from OUTSIDE the crate — which is the 2.0 migration pattern its rustdoc documents,
    // and a reminder that the in-crate form is not available here.
    let mut retry_cfg = RetryConfig::default();
    // Small enough that the absorbed retries cost milliseconds, not the shipped seconds.
    retry_cfg.base_delay = Duration::from_millis(1);
    let retrying: Arc<dyn LlmProvider> = Arc::new(RetryProvider::with_config(
        Arc::new(hanging) as Arc<dyn LlmProvider>,
        retry_cfg,
    ));

    let magi =
        MagiBuilder::new(ScriptProvider::new("m-default", vec![Beh::Ok]) as Arc<dyn LlmProvider>)
            .with_timeout(Duration::from_secs(5))
            .with_agent(AgentName::Caspar, retrying, Lineage::new("deepseek"))
            .build()
            .expect("the hanging trio builds");

    let out = magi.analyze(&Mode::CodeReview, "fn main() {}").await;
    handle.abort();
    out
}

/// A run where one seat's FIRST response fails the schema and its corrective retry succeeds.
///
/// # Why this shape, and not a transport hang
///
/// These are **two calls made by the orchestrator**, both visible from outside, against the
/// **same** model. A transport retry would not do: that one happens inside `RetryProvider`,
/// which hands back a single result, so from here it is indistinguishable from one call.
///
/// That distinction is the whole content of the MS1xMS2 cross-milestone criterion: completion
/// telemetry records per ATTEMPT, not per model.
///
/// The pool is empty on purpose — the corrective retry must succeed against the same model, so a
/// rotation would prove something else.
#[cfg(feature = "test-utils")]
pub async fn run_where_the_first_response_fails_schema_and_the_retry_succeeds()
-> Result<magi_core::reporting::MagiReport, magi_core::error::MagiError> {
    use magi_core::prelude::*;
    use magi_core::test_support::{Beh, ScriptProvider, build_trio_with_caspar};

    // `ScriptProvider` consumes behaviours by call index and repeats the last one, so this is
    // exactly "first call bad, every later call good".
    let caspar = ScriptProvider::new("m-deepseek", vec![Beh::BadJson, Beh::Ok]);
    let magi = build_trio_with_caspar(caspar, vec![]);
    magi.analyze(&Mode::CodeReview, "fn main() {}").await
}
