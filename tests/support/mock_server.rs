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
