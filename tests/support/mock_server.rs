// Author: Julian Bolivar
// Version: 4.0.0
// Date: 2026-08-23

//! Minimal HTTP server over `tokio::net::TcpListener` for integration tests. NOT a general
//! mock server: it only covers the two scenarios that MS1 needs (`S11` and `S16`). Ephemeral
//! port to avoid collisions.

use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// Accepts a connection, writes valid status + headers and **never** the body.
/// Forces the TOTAL timeout path (a connect-timeout would not trigger it).
// Each integration test includes ALL this module but uses a subset; the binary that
// does not use this function would see it as dead code (same reason as
// `spawn_429_with_retry_after`).
#[allow(dead_code)]
pub async fn spawn_hanging_headers() -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local addr");
    let handle = tokio::spawn(async move {
        if let Ok((mut sock, _)) = listener.accept().await {
            let mut buf = [0u8; 1024];
            let _ = sock.read(&mut buf).await;
            let _ = sock
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n")
                .await;
            std::future::pending::<()>().await;
        }
    });
    (format!("http://{addr}"), handle)
}

/// Responds with `429` with the given `Retry-After` on the first request and `200` on the
/// second, so we can observe the intermediate wait.
// No caller yet: its first use is the S11 test of Task 9 (end-to-end wiring of
// `Retry-After`). `#[allow(dead_code)]` instead of fabricating a fake caller,
// which the project's rules forbid.
#[allow(dead_code)]
pub async fn spawn_429_with_retry_after(value: &str) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local addr");
    let value = Arc::new(value.to_string());
    let handle = tokio::spawn(async move {
        let mut served = 0u32;
        while let Ok((mut sock, _)) = listener.accept().await {
            let mut buf = [0u8; 4096];
            let _ = sock.read(&mut buf).await;
            let response = if served == 0 {
                format!(
                    "HTTP/1.1 429 Too Many Requests\r\nRetry-After: {}\r\nContent-Length: 0\r\n\r\n",
                    value
                )
            } else {
                let body = r#"{"choices":[{"message":{"content":"ok"}}]}"#;
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                )
            };
            let _ = sock.write_all(response.as_bytes()).await;
            served += 1;
        }
    });
    (format!("http://{addr}"), handle)
}

/// What a captured request carried: the path it was sent to, and its JSON body.
///
/// Written in English, unlike the two servers above, because §0.2 asks for it in `tests/` too —
/// they predate the rule and are left alone rather than rewritten in an unrelated task.
#[allow(dead_code)]
pub struct CapturedRequest {
    pub path: String,
    pub body: serde_json::Value,
}

/// Serves one canned JSON response and RECORDS the request that asked for it.
///
/// Exists because the assertion that matters for the native endpoint is not "the response was
/// parsed" but "the request went to `/api/chat` with `stream: false`" — and neither is
/// observable from the response. The two servers above answer without looking at what arrived.
///
/// Reads exactly `Content-Length` bytes of body rather than one `read` call: a body split across
/// TCP segments would otherwise be captured truncated and the JSON parse would fail for a reason
/// that has nothing to do with the code under test.
#[allow(dead_code)]
pub async fn spawn_capturing(
    status: u16,
    response_body: &str,
) -> (
    String,
    Arc<std::sync::Mutex<Option<CapturedRequest>>>,
    JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local addr");
    let captured = Arc::new(std::sync::Mutex::new(None));
    let sink = Arc::clone(&captured);
    let body = Arc::new(response_body.to_string());
    let handle = tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            let mut raw: Vec<u8> = Vec::new();
            let mut chunk = [0u8; 4096];
            // Headers first: read until the blank line that ends them.
            let head_end = loop {
                match sock.read(&mut chunk).await {
                    Ok(0) | Err(_) => break None,
                    Ok(n) => {
                        raw.extend_from_slice(&chunk[..n]);
                        if let Some(p) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                            break Some(p + 4);
                        }
                    }
                }
            };
            if let Some(head_end) = head_end {
                let head = String::from_utf8_lossy(&raw[..head_end]).to_string();
                let path = head
                    .lines()
                    .next()
                    .and_then(|l| l.split_whitespace().nth(1))
                    .unwrap_or("")
                    .to_string();
                let want: usize = head
                    .lines()
                    .find_map(|l| {
                        l.strip_prefix("content-length: ")
                            .or_else(|| l.strip_prefix("Content-Length: "))
                    })
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(0);
                while raw.len() - head_end < want {
                    match sock.read(&mut chunk).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => raw.extend_from_slice(&chunk[..n]),
                    }
                }
                // A TRAP for any absence assertion downstream, and it is left here rather
                // than made fallible because the alternative is worse: `Null.get("x")` is
                // `None`, so "the field is absent" holds identically for a body that never
                // parsed. Tests asserting absence must first assert the body is an OBJECT --
                // see `the_default_reasoning_control_puts_no_think_field_on_the_wire`.
                let parsed =
                    serde_json::from_slice(&raw[head_end..]).unwrap_or(serde_json::Value::Null);
                if let Ok(mut slot) = sink.lock() {
                    *slot = Some(CapturedRequest { path, body: parsed });
                }
            }
            let response = format!(
                "HTTP/1.1 {} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                status,
                body.len(),
                body
            );
            let _ = sock.write_all(response.as_bytes()).await;
        }
    });
    (format!("http://{addr}"), captured, handle)
}

/// `429` on the first request and a **hang** on every one after it, counting what it served.
///
/// Exists because neither server above can produce a class TRANSITION: one hangs always, the
/// other never hangs. The property under test is that the attempt cap follows the class of the
/// error that JUST happened, so a chain has to change class mid-flight.
///
/// # Three return values where the others return two, deliberately
///
/// The counter IS the observable: it is what distinguishes "abandoned on the `Timeout` cap" (2)
/// from "on the general cap" (4). Normalising this to `(String, JoinHandle<()>)` would leave the
/// test with no way to tell those apart, which is the whole property.
///
/// # The `429` carries no `Retry-After`, on purpose
///
/// With the header the chain would honour the server's requested wait and the test would be
/// measuring a server backoff instead of the per-class cap. Without it the backoff is the one in
/// `RetryConfig`, which the test pins to 1 ms.
///
/// # The counter increments after READING the request, before hanging
///
/// Incrementing when responding would never count the request that hangs, so the test would read
/// `1` where there were `2` — the helper would report success for an abandonment that did not
/// happen.
#[allow(dead_code)]
pub async fn spawn_429_then_hang() -> (
    String,
    JoinHandle<()>,
    Arc<std::sync::Mutex<Vec<&'static str>>>,
) {
    use std::sync::Mutex;

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local addr");
    // What was SERVED, in order, rather than how many requests arrived. The count alone cannot
    // show a class TRANSITION: two hangs would also produce two requests and a final `Timeout`,
    // so a chain that never saw the 429 was indistinguishable from one that did — and the
    // transition is the whole property.
    let served_log: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&served_log);

    let handle = tokio::spawn(async move {
        let mut served = 0u32;
        while let Ok((mut sock, _)) = listener.accept().await {
            let log = Arc::clone(&log);
            let hang = served > 0;
            // Each connection in its OWN task: a hung connection served inline never returns, so
            // the loop would never call `accept()` again and the log would stop at two entries
            // whatever attempt cap was in force. Found by mutation, not by review.
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let _ = sock.read(&mut buf).await;
                // Recorded after reading and before hanging: a hung request is still a request
                // the chain spent an attempt on.
                if let Ok(mut g) = log.lock() {
                    g.push(if hang { "hang" } else { "429" });
                }
                // CRLF, like the two servers above it. A bare LF is tolerated by most clients but
                // is not HTTP, and a fixture that frames differently from its siblings becomes
                // the pattern the next one is copied from.
                if hang {
                    let headers = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 512\r\nConnection: close\r\n\r\n";
                    let _ = sock.write_all(headers.as_bytes()).await;
                    std::future::pending::<()>().await;
                } else {
                    // `Connection: close` is load-bearing, not tidiness. Under HTTP/1.1 its
                    // absence advertises keep-alive, so the client may pool this socket — and the
                    // task ends right after, closing it. The next attempt then fails as `Network`
                    // instead of reaching the hang, and the class-transition test flakes into the
                    // wrong error class: worse than no test on that property.
                    let resp = "HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                    let _ = sock.write_all(resp.as_bytes()).await;
                }
            });
            served += 1;
        }
    });
    (format!("http://{addr}"), handle, served_log)
}
