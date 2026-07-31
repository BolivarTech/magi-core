// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-07-30

//! Behavioural tests for the three response-body readers.
//!
//! # Why these exist
//!
//! The bound itself was covered by unit tests on the pure helpers (`push_within_cap`, `body_cap`,
//! `truncate_diagnostic`), but nothing exercised the readers that use them. That left the entire
//! `ResponseTooLarge` production path unconstructed by any test, the truncation marker on a body
//! cut short unverified, and the probe's fail-open degrade unexercised — while the changelog
//! stated flatly that response bodies are bounded.
//!
//! The three readers deliberately react differently to the same event, and that asymmetry is the
//! thing most at risk from a well-meaning "simplification":
//!
//! | Reader | Over the cap | Why |
//! |---|---|---|
//! | verdict | **fails** | a cut body loses its closing marker, and the parser would blame the *model* for our cut |
//! | diagnostic | **truncates and says so** | there is no verdict to falsify, and dropping a 500's body discards the reason it is read |
//! | probe | **degrades to `None`** | "no capability information" is a valid answer for a probe |
//!
//! No network: loopback only, OS-assigned ports, explicit deadlines.

#![cfg(feature = "openai-compat")]

use std::io::Write;
use std::net::TcpListener;

use magi_core::prelude::*;

mod common;
use common::{accept_one, hang_up};

/// The floor of the verdict cap. With the default `max_tokens` the derived cap never rises above
/// it, so this is what a body has to exceed.
///
/// Duplicated rather than imported: the production constant is `pub(crate)` and an integration
/// test compiles as a separate crate. If it ever drifts, the over-cap test stops failing and the
/// at-the-cap test starts — the pair is what makes drift visible rather than silent.
const VERDICT_CAP: usize = 1 << 20;

/// The diagnostic prefix cap — far smaller, because an error body is read by a human.
const DIAGNOSTIC_CAP: usize = 8 * 1024;

/// Size of each chunk when the server frames a body as `chunked`.
///
/// Named because the VALUE carries the intent: it has to be small enough that a body over the cap
/// takes several chunks, so the reader crosses the limit mid-stream instead of on its first read —
/// which is the branch a `Content-Length` body never reaches. A larger number would still pass the
/// test while quietly stopping it from testing that.
const CHUNK_BYTES: usize = 64 * 1024;

/// How the body is framed, which decides WHICH branch of the reader runs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Framing {
    /// `Content-Length`. The probe reader short-circuits on this before reading a byte.
    Length,
    /// `Transfer-Encoding: chunked`. No length is known up front, so the reader has to accumulate
    /// and hit the cap mid-stream — the branch a `Content-Length` test never reaches.
    ///
    /// Gated with the tests that use it: the probe reader is the only one with a `Content-Length`
    /// short-circuit to bypass, and it ships with the Ollama provider.
    #[cfg(feature = "ollama")]
    Chunked,
}

/// Serves ONE request with `status` and a body of `body_len` bytes, then hangs up cleanly.
///
/// Returns the address to point a client at, and a handle that must be joined so a server-side
/// failure surfaces instead of being swallowed.
fn serve_one_body(status: &str, body_len: usize) -> (String, std::thread::JoinHandle<()>) {
    serve_framed(status, vec![b'x'; body_len], Framing::Length)
}

fn serve_framed(
    status: &str,
    body: Vec<u8>,
    framing: Framing,
) -> (String, std::thread::JoinHandle<()>) {
    let body_len = body.len();
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr").to_string();
    let status = status.to_string();

    let handle = std::thread::spawn(move || {
        let (mut stream, _head) = accept_one(&listener).expect("a client connected in time");
        {
            let framing_header = match framing {
                Framing::Length => format!("Content-Length: {body_len}\r\n"),
                #[cfg(feature = "ollama")]
                Framing::Chunked => "Transfer-Encoding: chunked\r\n".to_string(),
            };
            let head = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n\
                         {framing_header}Connection: close\r\n\r\n"
            );
            if stream.write_all(head.as_bytes()).is_ok() {
                match framing {
                    Framing::Length => {
                        let _ = stream.write_all(&body);
                    }
                    #[cfg(feature = "ollama")]
                    Framing::Chunked => {
                        // Several chunks, so the cap is crossed part-way through the
                        // stream rather than on the very first read.
                        let chunk = CHUNK_BYTES;
                        let mut sent = 0;
                        while sent < body_len {
                            let n = chunk.min(body_len - sent);
                            let ok = stream
                                .write_all(format!("{n:x}\r\n").as_bytes())
                                .and_then(|_| stream.write_all(&body[sent..sent + n]))
                                .and_then(|_| stream.write_all(b"\r\n"))
                                .is_ok();
                            if !ok {
                                break;
                            }
                            sent += n;
                        }
                        let _ = stream.write_all(b"0\r\n\r\n");
                    }
                }
            }
        }
        hang_up(stream);
    });

    (addr, handle)
}

#[tokio::test]
async fn a_success_body_over_the_cap_fails_rather_than_arriving_truncated() {
    // The one reader that must NOT truncate. Handing a cut body to the verdict parser would strip
    // its closing marker, and the parser would report a truncated *model* output — blaming the
    // model for a cut this reader made, with a retry that can never fix it.
    let (addr, server) = serve_one_body("200 OK", VERDICT_CAP + 1);
    let provider =
        OpenAiCompatibleProvider::new(format!("http://{addr}/v1"), "m", None).expect("constructs");

    let err = provider
        .complete("sys", "usr", &CompletionConfig::default())
        .await
        .expect_err("an over-cap body must fail");
    server.join().expect("server thread");

    match err {
        // `..` is required, and that is the design rather than an inconvenience: the variant is
        // `#[non_exhaustive]`, so from outside the crate it can gain fields without breaking this.
        ProviderError::ResponseTooLarge { limit, .. } => {
            assert_eq!(
                limit, VERDICT_CAP,
                "the cap is reported, not left to be parsed"
            );
        }
        other => panic!("expected ResponseTooLarge, got {other:?}"),
    }
}

#[tokio::test]
async fn a_success_body_at_the_cap_is_read_whole() {
    // The boundary is `>`, not `>=`. Without this the previous test would also pass with an
    // off-by-one that rejected a body of exactly the permitted size.
    let (addr, server) = serve_one_body("200 OK", VERDICT_CAP);
    let provider =
        OpenAiCompatibleProvider::new(format!("http://{addr}/v1"), "m", None).expect("constructs");

    let err = provider
        .complete("sys", "usr", &CompletionConfig::default())
        .await
        .expect_err("the body is `xxxx…`, so it still fails — as a PARSE error");
    server.join().expect("server thread");

    assert!(
        !matches!(err, ProviderError::ResponseTooLarge { .. }),
        "a body exactly at the cap must be read, not rejected: {err:?}"
    );
}

#[tokio::test]
async fn an_error_body_over_the_cap_keeps_its_prefix_and_announces_the_cut() {
    // The opposite reaction, deliberately: an error body is diagnostic text. There is no verdict a
    // cut could falsify, and dropping a 500's body whole discards the only reason that error is
    // read at all.
    let (addr, server) = serve_one_body("500 Internal Server Error", DIAGNOSTIC_CAP * 4);
    let provider =
        OpenAiCompatibleProvider::new(format!("http://{addr}/v1"), "m", None).expect("constructs");

    let err = provider
        .complete("sys", "usr", &CompletionConfig::default())
        .await
        .expect_err("a 500 is an error");
    server.join().expect("server thread");

    let rendered = err.to_string();
    assert!(
        rendered.contains("truncated"),
        "a body cut short must say so, or its last byte reads as the server's last word: {rendered}"
    );
    assert!(
        rendered.contains("xxxx"),
        "and the prefix is kept — that is the point of not failing here: {rendered}"
    );
    assert!(
        rendered.len() < DIAGNOSTIC_CAP * 2,
        "the retained prefix is bounded, not the whole body: {} bytes",
        rendered.len()
    );
}

#[cfg(feature = "ollama")]
#[tokio::test]
async fn a_probe_body_over_the_cap_degrades_instead_of_failing() {
    // The third semantics. For a probe, "no capability information" is a VALID result, so an
    // over-cap body degrades rather than erroring — the fail-open behaviour rotation depends on.
    // Truncating would be worse here than anywhere else: a half-read JSON document does not parse,
    // so it would look like schema drift rather than an oversized response.
    use magi_core::rotation::ProviderProbe;

    let (addr, server) = serve_one_body("200 OK", VERDICT_CAP + 1);
    let provider = magi_core::providers::ollama::OllamaProvider::new(format!("http://{addr}"), "m")
        .expect("constructs");

    let window = provider.window().await;
    server.join().expect("server thread");

    assert!(
        matches!(window, Ok(None)),
        "an over-cap probe body degrades to `None`, it does not error: {window:?}"
    );
}

#[cfg(feature = "ollama")]
#[tokio::test]
async fn a_chunked_probe_body_degrades_from_the_streaming_branch() {
    // The sibling above is answered with `Content-Length`, so the reader rejects it before reading
    // a byte — the early check, not the accumulation loop. Without a length the reader has to
    // stream and notice the cap mid-body, which is the branch that actually bounds memory.
    //
    // The body is VALID, oversized JSON, and that is what makes this test discriminating. With a
    // junk body both outcomes look the same: over-cap degrades to `None`, and so does a body that
    // parsed to nothing. A first version of this test used junk and passed with the streaming
    // branch deliberately broken — the exact "green for the wrong reason" it was written against.
    // Parse it and the window is `Some(4096)`; bound it and it is `None`.
    use magi_core::rotation::ProviderProbe;

    let mut body = br#"{"model_info":{"llama.context_length":4096},"pad":""#.to_vec();
    body.resize(VERDICT_CAP + 1 - 2, b'p');
    body.extend_from_slice(br#""}"#);
    assert_eq!(body.len(), VERDICT_CAP + 1, "the body must exceed the cap");
    assert!(
        serde_json::from_slice::<serde_json::Value>(&body).is_ok(),
        "and it must be parseable, or this test cannot tell the two outcomes apart"
    );

    let (addr, server) = serve_framed("200 OK", body, Framing::Chunked);
    let provider = magi_core::providers::ollama::OllamaProvider::new(format!("http://{addr}"), "m")
        .expect("constructs");

    let window = provider.window().await;
    server.join().expect("server thread");

    assert!(
        matches!(window, Ok(None)),
        "the streaming branch must bound the body — a window here means it was read whole: {window:?}"
    );
}

#[tokio::test]
async fn a_verdict_body_that_is_not_utf8_fails_instead_of_being_mangled() {
    // Lossy conversion would turn each invalid byte into a 3-byte replacement character, so a
    // body that fitted the cap in bytes could leave this function as a String three times larger
    // — and the mangled text would then fail downstream as a *schema* error, blaming the model
    // for an encoding fault.
    let (addr, server) = serve_framed("200 OK", vec![0xff, 0xfe, 0xfd], Framing::Length);
    let provider =
        OpenAiCompatibleProvider::new(format!("http://{addr}/v1"), "m", None).expect("constructs");

    let err = provider
        .complete("sys", "usr", &CompletionConfig::default())
        .await
        .expect_err("invalid UTF-8 must not be silently replaced");
    server.join().expect("server thread");

    let rendered = err.to_string();
    assert!(
        rendered.contains("not valid UTF-8"),
        "the cause must name the encoding fault rather than a schema one: {rendered}"
    );
}

#[cfg(feature = "ollama")]
#[tokio::test]
async fn a_chunked_probe_body_under_the_cap_is_read_and_parsed() {
    // The positive half of the pair, and what makes the negative half mean something. On its own,
    // "over-cap chunked gives None" is satisfied by a reader that fails on ALL chunked responses.
    // This shows the same framing parses fine when it fits, so the `None` above is the cap talking
    // and not the transfer encoding.
    use magi_core::rotation::ProviderProbe;

    let body = br#"{"model_info":{"llama.context_length":4096}}"#.to_vec();
    let (addr, server) = serve_framed("200 OK", body, Framing::Chunked);
    let provider = magi_core::providers::ollama::OllamaProvider::new(format!("http://{addr}"), "m")
        .expect("constructs");

    let window = provider.window().await;
    server.join().expect("server thread");

    assert_eq!(
        window.expect("no transport error"),
        Some(4096),
        "a chunked body within the cap must be read and parsed"
    );
}
