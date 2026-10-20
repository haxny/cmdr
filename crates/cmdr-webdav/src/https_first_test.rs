//! Which address an `http` add dials (#540): its `https` twin when that
//! answers, else the address as typed.

use std::sync::atomic::{AtomicBool, Ordering};

use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use url::Url;

use super::prefer_https_with;
use crate::params::WebdavConnectionParams;

fn params(url: &str) -> WebdavConnectionParams {
    let mut params = WebdavConnectionParams::new(Url::parse(url).expect("a valid test URL"), "ada", "/Photos");
    params.auto_reconnect = false;
    params
}

#[tokio::test]
async fn an_http_address_moves_to_https_when_that_answers() {
    let chosen = prefer_https_with(params("http://nas.local/dav/"), |url| async move {
        assert_eq!(url.as_str(), "https://nas.local/dav/", "the probe asks the twin");
        true
    })
    .await;

    assert_eq!(chosen.base_url.as_str(), "https://nas.local/dav/");
    assert_eq!(chosen.username, "ada");
    assert_eq!(chosen.remote_root.to_string_lossy(), "/Photos");
    assert!(!chosen.auto_reconnect, "the person's switches come along");
}

#[tokio::test]
async fn an_http_address_is_dialed_as_typed_when_https_does_not_answer() {
    let chosen = prefer_https_with(params("http://nas.local:5005/dav/"), |_| async { false }).await;
    assert_eq!(chosen.base_url.as_str(), "http://nas.local:5005/dav/");
}

#[tokio::test]
async fn an_https_address_asks_nothing() {
    let asked = AtomicBool::new(false);
    let chosen = prefer_https_with(params("https://cloud.example.com/dav/"), |_| {
        asked.store(true, Ordering::Relaxed);
        async { true }
    })
    .await;
    assert_eq!(chosen.base_url.as_str(), "https://cloud.example.com/dav/");
    assert!(!asked.load(Ordering::Relaxed));
}

// ── The real probe's "no" answers ────────────────────────────────────

/// ❗ The commonest NAS case: `http://nas:5005` has no TLS on that port. A TLS
/// hello to a plain HTTP server must read as "doesn't answer", fast.
#[tokio::test]
async fn a_plain_http_port_does_not_answer_over_tls() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let _ = socket
                .write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await;
        }
    });

    let chosen = params(&format!("http://127.0.0.1:{port}/dav/"))
        .preferring_https(&CancellationToken::new())
        .await;

    assert_eq!(chosen.base_url.scheme(), "http");
}

#[tokio::test]
async fn a_closed_port_does_not_answer_over_tls() {
    let chosen = params("http://127.0.0.1:1/dav/")
        .preferring_https(&CancellationToken::new())
        .await;
    assert_eq!(chosen.base_url.scheme(), "http");
}
