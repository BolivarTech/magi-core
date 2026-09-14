// Author: Julian Bolivar
// Version: 4.1.0
// Date: 2026-09-14

mod support;
use support::mock_server;

#[tokio::test]
async fn hanging_server_does_not_send_body() {
    let (url, handle) = mock_server::spawn_hanging_headers().await;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(300))
        .build()
        .expect("client");
    // `.send()` resolves as soon as the HEADERS arrive — the mock sends them immediately and
    // then hangs the body. To observe the TOTAL timeout (the failure mode that S16 pursues:
    // headers OK, body that never ends) you must **consume the body**, which is what the real
    // `complete()` does via `.text()`. Without reading the body, `.send()` returns
    // `Ok(200)` without a timeout.
    let err = client
        .get(&url)
        .send()
        .await
        .expect("headers llegan de inmediato")
        .text()
        .await
        .expect_err("la lectura del cuerpo debe dar timeout");
    assert!(err.is_timeout(), "expected timeout, got: {err}");
    handle.abort();
}
