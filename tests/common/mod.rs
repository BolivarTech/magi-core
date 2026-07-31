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
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).ok()?;
                let head = String::from_utf8_lossy(&buf[..n]).into_owned();
                return Some((stream, head));
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
