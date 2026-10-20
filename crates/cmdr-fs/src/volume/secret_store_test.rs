//! What a sign-in that connected leaves in the store, by the Remember box.

use std::sync::Arc;

use super::*;
use crate::volume::host::credentials::{CredentialStore, InMemoryCredentials};

const SERVICE: &str = "https://nas.local:443";

fn host_over(store: &Arc<InMemoryCredentials>) -> VolumeHost {
    VolumeHost::builder()
        .credentials(Arc::clone(store) as Arc<dyn CredentialStore>)
        .build()
}

fn sign_in(secret: &str, remember: bool) -> SignIn {
    SignIn {
        username: "ada".to_string(),
        secret: secret.to_string(),
        remember,
    }
}

fn held(store: &InMemoryCredentials) -> Option<String> {
    store.credentials(SERVICE, Some("ada")).map(|held| held.secret)
}

/// ❗ A checked Remember box is a promise, so it seeds an empty store.
#[tokio::test]
async fn remember_on_seeds_an_empty_store() {
    let store = Arc::new(InMemoryCredentials::new());
    assert!(keep_signed_in_secret(&host_over(&store), SERVICE, &sign_in("new", true)).await);
    assert_eq!(held(&store), Some("new".to_string()));
}

#[tokio::test]
async fn remember_on_replaces_a_remembered_secret() {
    let store = Arc::new(InMemoryCredentials::new().with_entry(SERVICE, Some("ada"), "ada", "old"));
    assert!(keep_signed_in_secret(&host_over(&store), SERVICE, &sign_in("new", true)).await);
    assert_eq!(held(&store), Some("new".to_string()));
}

#[tokio::test]
async fn remember_off_keeps_a_remembered_secret_current() {
    let store = Arc::new(InMemoryCredentials::new().with_entry(SERVICE, Some("ada"), "ada", "old"));
    assert!(keep_signed_in_secret(&host_over(&store), SERVICE, &sign_in("new", false)).await);
    assert_eq!(held(&store), Some("new".to_string()));
}

/// ❌ Remember off never seeds: the sheet forgot the entry before it dialed.
#[tokio::test]
async fn remember_off_leaves_an_empty_store_empty() {
    let store = Arc::new(InMemoryCredentials::new());
    assert!(keep_signed_in_secret(&host_over(&store), SERVICE, &sign_in("new", false)).await);
    assert_eq!(held(&store), None);
}
