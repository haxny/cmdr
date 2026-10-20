//! The store key: two accounts on one server never share an entry.

use url::Url;

use super::WebdavConnectionParams;

fn params(url: &str, user: &str) -> WebdavConnectionParams {
    WebdavConnectionParams::new(Url::parse(url).expect("a valid test URL"), user, "/")
}

#[test]
fn an_http_address_has_an_https_twin_on_the_default_port() {
    let twin = params("http://nas.local/dav/", "ada")
        .https_twin()
        .expect("http has a twin");
    assert_eq!(twin.base_url.as_str(), "https://nas.local/dav/");
    assert_eq!(twin.credential_service(), "https://nas.local:443");
}

#[test]
fn an_explicit_port_stays_on_the_https_twin() {
    let twin = params("http://nas.local:5005/dav/", "ada")
        .https_twin()
        .expect("http has a twin");
    assert_eq!(twin.base_url.as_str(), "https://nas.local:5005/dav/");
}

#[test]
fn an_https_address_has_no_twin() {
    assert!(params("https://nas.local/dav/", "ada").https_twin().is_none());
}

#[test]
fn https_without_a_port_keys_on_443() {
    assert_eq!(
        params("https://cloud.example.com/remote.php/dav", "ada").credential_service(),
        "https://cloud.example.com:443"
    );
}

#[test]
fn http_without_a_port_keys_on_80() {
    assert_eq!(
        params("http://nas.local/dav/", "ada").credential_service(),
        "http://nas.local:80"
    );
}

#[test]
fn an_explicit_port_is_kept() {
    assert_eq!(
        params("http://127.0.0.1:13480/dav/", "ada").credential_service(),
        "http://127.0.0.1:13480"
    );
    assert_eq!(params("http://127.0.0.1:13480/dav/", "ada").port(), 13480);
}

#[test]
fn two_accounts_on_one_server_share_the_service_but_not_the_scope() {
    let ada = params("https://cloud.example.com/dav/", "ada");
    let bob = params("https://cloud.example.com/dav/", "bob");
    assert_eq!(ada.credential_service(), bob.credential_service());
    assert_ne!(
        ada.username, bob.username,
        "the scope is what keeps their entries apart"
    );
}

#[test]
fn the_base_url_always_ends_in_a_slash() {
    assert_eq!(
        params("https://cloud.example.com/remote.php/dav", "ada")
            .base_url
            .path(),
        "/remote.php/dav/"
    );
    assert_eq!(params("https://cloud.example.com", "ada").base_url.path(), "/");
}
