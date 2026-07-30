// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-07-30

//! Anchors of UPSTREAM behaviour. These do not test this crate's logic.
//!
//! The redaction design rests on what the HTTP client does across a redirect. That behaviour is
//! internal to that crate, so nothing but a test keeps it from changing under us silently.
//!
//! Two of these anchor a guarantee the client gives us (`Authorization` is stripped across origins
//! but kept within one). The third anchors the opposite: a leak the client's **default** has, which
//! this crate closes by configuration. Both directions have to be pinned — a guarantee that quietly
//! disappears and a mitigation quietly deleted as "unnecessary" fail exactly the same way.
//!
//! # If this file goes red, read this before touching anything
//!
//! 1. **What broke?** Something in the HTTP client changed — check the lockfile diff first.
//! 2. **False positive:** its parser got stricter and rejected our hand-written response. Fix the
//!    fixture bytes. Nothing about this crate's guarantees changed.
//! 3. **True positive:** credentials are no longer stripped across origins. **The redaction
//!    guarantee stops holding** — redaction has to move to before the request is built. That is a
//!    design change, not a test fix.
//! 4. **The `Referer` case is red in the safe direction:** if the default stopped leaking, the
//!    mitigation is merely redundant, not wrong. Leave it in place — a default that changed once
//!    can change back.
//!
//! Treating (3) as (2) buries a security hole. When in doubt, assume (3).
//!
//! No network: everything is loopback, with the OS assigning ports.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

/// Generous on purpose: two orders of magnitude above a loopback handshake, so exceeding it means
/// something is actually broken rather than that the runner was busy. If this ever flakes, raise
/// the deadline — do not add retries, which start averaging away a real failure.
const DEADLINE: Duration = Duration::from_secs(10);

/// Bounds the post-response drain. Short on purpose: by then the peer has been told the connection
/// closes, so reaching EOF is the expected case and this only caps the pathological one.
const DRAIN: Duration = Duration::from_millis(500);

/// `Connection: close` is load-bearing, not decoration. Without it the client keeps the socket in
/// its pool and reuses it for the redirect — but this server serves exactly one request and then
/// hangs up, so the reused socket is already dead.
const OK_RESPONSE: &str = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

/// Minimal but VALID: CRLF terminators and an explicit length, not a fragment that relies on the
/// parser being lenient.
fn redirect_to(url: &str) -> String {
    format!(
        "HTTP/1.1 302 Found\r\nLocation: {url}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
}

/// Accepts one connection, returns the request head, and replies with `response`.
///
/// Non-blocking with an explicit deadline: a plain `accept()` blocks forever, which would hang CI —
/// the worst possible failure mode for a security tripwire, because nobody can tell a hang from a
/// dead runner.
fn serve_once(listener: &TcpListener, response: &str) -> Option<String> {
    listener.set_nonblocking(true).ok()?;
    let start = Instant::now();
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                // MUST come before the first read, and is not redundant with the timeout below.
                // On Windows an accepted socket INHERITS the listener's non-blocking mode (on Linux
                // it does not), and the listener is non-blocking so that the accept loop can honour
                // a deadline. Left inherited, a read issued before the request bytes land returns
                // `WouldBlock` — which is not an error, but reads exactly like one, aborting this
                // handler. The visible symptom appeared on the *next* listener, which then waited
                // out the full deadline for a connection the client had already given up on.
                stream.set_nonblocking(false).ok()?;
                stream.set_read_timeout(Some(DEADLINE)).ok()?;
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).ok()?;
                let head = String::from_utf8_lossy(&buf[..n]).into_owned();
                stream.write_all(response.as_bytes()).ok()?;
                let _ = stream.flush();
                // Hang up CLEANLY. Dropping a socket that still has unread bytes makes the OS send
                // RST instead of FIN, and a client that takes an RST mid-response reports a
                // connection error rather than the 302 it had already received — so it never
                // follows the redirect, the next origin is never reached, and the test times out
                // blaming the wrong thing. Half-close to send a real FIN, then read to EOF so
                // nothing is left pending, and only then drop.
                let _ = stream.shutdown(std::net::Shutdown::Write);
                let _ = stream.set_read_timeout(Some(DRAIN));
                while matches!(stream.read(&mut buf), Ok(k) if k > 0) {}
                return Some(head);
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

#[tokio::test]
async fn authorization_is_stripped_across_origins() {
    let first = TcpListener::bind("127.0.0.1:0").expect("bind");
    let second = TcpListener::bind("127.0.0.1:0").expect("bind");
    let target = format!("http://{}/next", second.local_addr().expect("addr"));
    let src = format!(
        "http://user:pass@{}/start",
        first.local_addr().expect("addr")
    );

    let handle = std::thread::spawn(move || {
        let _ = serve_once(&first, &redirect_to(&target));
        serve_once(&second, OK_RESPONSE)
    });

    let _ = reqwest::Client::new().get(&src).send().await;
    let seen = handle
        .join()
        .expect("server thread")
        .expect("the second origin was reached");

    assert!(
        !seen.to_lowercase().contains("authorization"),
        "credentials crossed to another origin:\n{seen}"
    );
}

#[tokio::test]
async fn authorization_survives_a_same_origin_redirect() {
    // Without this case the pair would also pass if the client stripped ALWAYS — which would break
    // authentication instead of protecting it. This is what makes the tests discriminating rather
    // than merely confirmatory.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let src = format!("http://user:pass@{addr}/start");
    let target = format!("http://{addr}/next");

    let handle = std::thread::spawn(move || {
        let _ = serve_once(&listener, &redirect_to(&target));
        serve_once(&listener, OK_RESPONSE)
    });

    let _ = reqwest::Client::new().get(&src).send().await;
    let seen = handle
        .join()
        .expect("server thread")
        .expect("the second request was served");

    assert!(
        seen.to_lowercase().contains("authorization"),
        "a same-origin redirect must keep the header:\n{seen}"
    );
}

#[tokio::test]
async fn the_default_client_does_leak_a_query_secret_through_referer() {
    // Not a bug report against the HTTP client — documented behaviour, and the reason this crate
    // configures its clients the way it does.
    //
    // On a redirect the default client sets `Referer` to the ORIGINAL url, query string included.
    // The `Location` genuinely does replace the url, which is what an earlier version of this
    // design concluded — and that conclusion was wrong, because it looked only at the url and not
    // at the headers. For an endpoint authenticated by a query parameter, this hands the
    // credential to whatever origin the redirect points at.
    //
    // This test exists so the mitigation below can never be removed as "unnecessary".
    let first = TcpListener::bind("127.0.0.1:0").expect("bind");
    let second = TcpListener::bind("127.0.0.1:0").expect("bind");
    let target = format!("http://{}/next", second.local_addr().expect("addr"));
    let src = format!(
        "http://{}/start?key=q3rySecret",
        first.local_addr().expect("addr")
    );

    let handle = std::thread::spawn(move || {
        let _ = serve_once(&first, &redirect_to(&target));
        serve_once(&second, OK_RESPONSE)
    });

    let _ = reqwest::Client::new().get(&src).send().await;
    let seen = handle
        .join()
        .expect("server thread")
        .expect("the second origin was reached");

    assert!(
        seen.to_lowercase().contains("referer"),
        "if the default stopped sending Referer, this crate's mitigation could be revisited:\n{seen}"
    );
    assert!(
        seen.contains("q3rySecret"),
        "and the Referer is what carries the query secret:\n{seen}"
    );
}

#[tokio::test]
async fn a_client_configured_the_way_this_crate_does_it_leaks_nothing() {
    // The mitigation, proven rather than asserted: `referer(false)` is what every provider in this
    // crate builds its client with.
    let first = TcpListener::bind("127.0.0.1:0").expect("bind");
    let second = TcpListener::bind("127.0.0.1:0").expect("bind");
    let target = format!("http://{}/next", second.local_addr().expect("addr"));
    let src = format!(
        "http://{}/start?key=q3rySecret",
        first.local_addr().expect("addr")
    );

    let handle = std::thread::spawn(move || {
        let _ = serve_once(&first, &redirect_to(&target));
        serve_once(&second, OK_RESPONSE)
    });

    let client = reqwest::Client::builder()
        .referer(false)
        .build()
        .expect("client");
    let _ = client.get(&src).send().await;
    let seen = handle
        .join()
        .expect("server thread")
        .expect("the second origin was reached");

    assert!(
        !seen.contains("q3rySecret"),
        "the query secret reached a different origin:\n{seen}"
    );
}
