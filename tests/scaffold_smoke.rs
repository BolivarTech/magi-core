// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-07-25

mod support;
use support::mock_server;

#[tokio::test]
async fn hanging_server_does_not_send_body() {
    let (url, handle) = mock_server::spawn_hanging_headers().await;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(300))
        .build()
        .expect("client");
    // `.send()` resuelve apenas llegan las CABECERAS — el mock las manda al
    // instante y despues cuelga el cuerpo. Para observar el timeout TOTAL (el
    // modo de fallo que S16 persigue: headers OK, cuerpo que nunca termina) hay
    // que **consumir el cuerpo**, que es lo que hace el `complete()` real via
    // `.text()`. Sin leer el cuerpo, `.send()` retorna `Ok(200)` sin timeout.
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
