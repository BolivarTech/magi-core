// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-07-25

//! Servidor HTTP minimo sobre `tokio::net::TcpListener` para tests de
//! integracion. NO es un mock server general: solo cubre los dos escenarios
//! que MS1 necesita (S11 y S16). Puerto efimero para evitar colisiones.

use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// Acepta una conexion, escribe status + headers validos y **nunca** el cuerpo.
/// Fuerza el camino de timeout TOTAL (un connect-timeout no disparia).
// Cada test de integracion incluye TODO este modulo pero usa un subconjunto; el
// binario que no usa esta funcion la veria como dead code (mismo motivo que
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

/// Responde `429` con el `Retry-After` dado en la primera peticion y `200` en
/// la segunda, para poder observar la espera intermedia.
// Sin caller todavia: su primer uso es el test S11 de la Tarea 9 (cableado
// end-to-end de `Retry-After`). `#[allow(dead_code)]` en vez de fabricar un
// caller falso, prohibido por CLAUDE.local.md §6.1.8 / spec R8.
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
