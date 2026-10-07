#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! What a supervisor of a local `pando serve` needs: the API token from `GET /api/v1/token` and
//! trusting the server's private CA.

use axum::Router;
use axum::routing::get;
use pando::{Error, PandoClient, PandoConfig};
use serde_json::json;

async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn fetch_api_token_reads_the_loopback_token_endpoint() {
    let app = Router::new().route(
        "/api/v1/token",
        get(|| async { axum::Json(json!({"token": "abc123"})) }),
    );
    let client = PandoClient::new(PandoConfig::new(serve(app).await)).unwrap();
    let token = client.fetch_api_token().await.unwrap();
    assert_eq!(token.expose(), "abc123");
    assert_eq!(format!("{token:?}"), "Token(<redacted>)");
}

#[tokio::test]
async fn fetch_api_token_maps_an_unauthorised_answer() {
    let app = Router::new().route(
        "/api/v1/token",
        get(|| async { (axum::http::StatusCode::UNAUTHORIZED, "no") }),
    );
    let client = PandoClient::new(PandoConfig::new(serve(app).await)).unwrap();
    assert!(matches!(
        client.fetch_api_token().await,
        Err(Error::Unauthorized)
    ));
}

#[test]
fn an_invalid_root_certificate_is_a_config_error() {
    let cfg = PandoConfig::new("https://127.0.0.1:1").with_root_certificate_pem(
        b"-----BEGIN CERTIFICATE-----\nnot base64!\n-----END CERTIFICATE-----\n".to_vec(),
    );
    assert!(matches!(PandoClient::new(cfg), Err(Error::Config(_))));
}

#[test]
fn no_root_certificate_keeps_the_system_roots() {
    assert!(PandoClient::new(PandoConfig::new("https://127.0.0.1:1")).is_ok());
}
