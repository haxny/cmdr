//! Reaching a backend's secret store without stalling every other volume.
//!
//! ❗ **Every call here runs on a blocking task.** The store can put a Keychain
//! prompt in front of a read or a write, and a modal dialog on the async runtime
//! holds the whole runtime, every other volume's listings and transfers with
//! it. A backend that reaches `host.credentials()` directly from an async fn is
//! one user click away from freezing the app.

use crate::volume::host::VolumeHost;
use crate::volume::host::credentials::StoredCredentials;

/// The account's stored secret, or `None` when nothing is stored (and when the
/// store didn't answer at all, which sends the caller to the same place: ask).
pub async fn stored_secret(host: &VolumeHost, service: &str, username: &str) -> Option<StoredCredentials> {
    let host = host.clone();
    let service = service.to_string();
    let scope = username.to_string();
    tokio::task::spawn_blocking(move || host.credentials().credentials(&service, Some(&scope)))
        .await
        .ok()
        .flatten()
}

/// Whether a secret is stored, without reading it out.
pub async fn has_stored_secret(host: &VolumeHost, service: &str, username: &str) -> bool {
    stored_secret(host, service, username).await.is_some()
}

/// Drops the account's stored secret, answering whether the store let it go
/// (nothing stored counts as gone). Callers that hold the secret in memory drop
/// that copy themselves, whatever this answers.
pub async fn forget_stored_secret(host: &VolumeHost, service: &str, username: &str) -> bool {
    let host = host.clone();
    let service = service.to_string();
    let scope = username.to_string();
    let forgotten =
        tokio::task::spawn_blocking(move || host.credentials().forget_credentials(&service, Some(&scope))).await;
    matches!(forgotten, Ok(Ok(())))
}

/// Brings a REMEMBERED secret up to date with the one the user just typed.
///
/// ❗ Call it only once that secret CONNECTED. Called before the dial, a typo
/// replaced the secret that worked, and the dial then failed (#530).
///
/// ❗ Refreshes, ❌ never seeds: a store with nothing in it is the user having
/// said no to remembering, and writing one anyway would put a password in the
/// Keychain they declined to leave there. The read and the write ride ONE
/// blocking task, so a store that prompts asks once.
///
/// Answers whether the new secret landed. A `false` costs only the "silent next
/// time" guarantee, ❌ never the connection that earned it, so callers log it
/// rather than failing.
pub async fn refresh_remembered_secret(host: &VolumeHost, service: &str, username: &str, secret: &str) -> bool {
    let host = host.clone();
    let service = service.to_string();
    let scope = username.to_string();
    let stored = StoredCredentials {
        username: username.to_string(),
        secret: secret.to_string(),
    };
    let written = tokio::task::spawn_blocking(move || {
        let store = host.credentials();
        if store.credentials(&service, Some(&scope)).is_none() {
            return Ok(());
        }
        store.save_credentials(&service, Some(&scope), &stored)
    })
    .await;
    matches!(written, Ok(Ok(())))
}

/// What a person typed into a sign-in sheet to mend a volume: the account, its
/// secret, and where they left the Remember box.
///
/// One payload rather than three positional arguments, so a `remember` can't
/// land in the wrong slot. ❌ No `Debug`: the secret must never reach a log.
#[derive(Clone)]
pub struct SignIn {
    /// The account. A backend whose volume id IS the account refuses another.
    pub username: String,
    /// The password, passphrase, or secret key, as typed.
    pub secret: String,
    /// The sheet's Remember box: whether the person wants this secret kept.
    pub remember: bool,
}

/// How a sign-in that CONNECTED left its secret. ❗ The connection stands either
/// way; this is only what the person has to be told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignedIn {
    /// The Remember box was on and the store wouldn't keep the secret (Deny on
    /// the Keychain prompt, a locked keychain, no secret service), so the next
    /// reconnect asks again unless the person tries once more.
    pub remember_refused: bool,
}

impl SignedIn {
    /// The answer for `sign_in` once the store said whether it `kept` the
    /// secret. Only a checked box can be refused: with it off there was no
    /// promise to break.
    pub fn after(sign_in: &SignIn, kept: bool) -> Self {
        Self {
            remember_refused: sign_in.remember && !kept,
        }
    }

    /// A sign-in that found a live session and wrote nothing.
    pub const NOTHING_FILED: Self = Self {
        remember_refused: false,
    };
}

/// Files the secret a sign-in CONNECTED with, the way the Remember box asks.
///
/// - **On**: saved, seeding an empty store; a checked box is a promise.
/// - **Off**: [`refresh_remembered_secret`], so an entry still there is kept
///   current and ❌ an empty store stays empty (the sheet forgot the entry
///   before it dialed).
///
/// ❗ Call it only once the secret connected (#530). Answers whether the store
/// took it; a `false` is logged, ❌ never a failed connection.
pub async fn keep_signed_in_secret(host: &VolumeHost, service: &str, sign_in: &SignIn) -> bool {
    if !sign_in.remember {
        return refresh_remembered_secret(host, service, &sign_in.username, &sign_in.secret).await;
    }
    let host = host.clone();
    let service = service.to_string();
    let stored = StoredCredentials {
        username: sign_in.username.clone(),
        secret: sign_in.secret.clone(),
    };
    let written = tokio::task::spawn_blocking(move || {
        host.credentials()
            .save_credentials(&service, Some(&stored.username), &stored)
    })
    .await;
    matches!(written, Ok(Ok(())))
}

#[cfg(test)]
#[path = "secret_store_test.rs"]
mod secret_store_test;
