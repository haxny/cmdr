//! The lifecycle for a backend with no secret at all (NFS's `AUTH_SYS`): it
//! dials unattended with nothing to offer, and never rests in
//! `NeedsCredentials`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::*;
use crate::volume::Retirement;
use crate::volume::host;
use crate::volume::host::credentials::CredentialStore;

struct NoSecretClient {
    liveness: Arc<Liveness>,
}

impl SessionClient for NoSecretClient {
    fn liveness(&self) -> &Arc<Liveness> {
        &self.liveness
    }

    async fn ping(&self) -> bool {
        true
    }
}

struct NoSecretBackend {
    session: HttpSession<NoSecretBackend>,
    host: VolumeHost,
    /// Every secret a dial was handed.
    dialed_with: Mutex<Vec<String>>,
}

impl Retires for NoSecretBackend {
    fn retirement(&self) -> &Retirement {
        self.session.connection().retirement()
    }
}

impl HttpBackend for NoSecretBackend {
    type Client = NoSecretClient;

    const BACKEND: &'static str = "test";

    fn session(&self) -> &HttpSession<Self> {
        &self.session
    }

    fn host(&self) -> &VolumeHost {
        &self.host
    }

    fn credential_key(&self) -> Option<CredentialKey> {
        None
    }

    async fn dial(&self, secret: &str) -> Result<NoSecretClient, DialRefusal> {
        self.dialed_with.lock().unwrap().push(secret.to_string());
        Ok(NoSecretClient {
            liveness: Arc::new(Liveness::new()),
        })
    }
}

/// A store that counts every read, so a cell can say none happened.
#[derive(Default)]
struct CountingStore {
    reads: AtomicUsize,
}

impl CredentialStore for CountingStore {
    fn credentials(&self, _service: &str, _scope: Option<&str>) -> Option<host::credentials::StoredCredentials> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        None
    }

    fn save_credentials(
        &self,
        _service: &str,
        _scope: Option<&str>,
        _credentials: &host::credentials::StoredCredentials,
    ) -> Result<(), host::credentials::CredentialsNotStored> {
        Ok(())
    }

    fn forget_credentials(
        &self,
        _service: &str,
        _scope: Option<&str>,
    ) -> Result<(), host::credentials::CredentialsNotForgotten> {
        Ok(())
    }
}

fn backend(store: Arc<CountingStore>) -> Arc<NoSecretBackend> {
    let host = VolumeHost::builder().credentials(store).build();
    Arc::new_cyclic(|me| NoSecretBackend {
        session: HttpSession::new(me, "test-volume", &host, None, true),
        host,
        dialed_with: Mutex::new(Vec::new()),
    })
}

#[tokio::test]
async fn a_backend_with_no_secret_redials_unattended_without_reading_the_store() {
    let store = Arc::new(CountingStore::default());
    let backend = backend(Arc::clone(&store));

    backend.attempt_reconnect().await.expect("a no-secret backend redials");

    assert_eq!(*backend.dialed_with.lock().unwrap(), vec![String::new()]);
    assert_eq!(
        store.reads.load(Ordering::SeqCst),
        0,
        "nothing to read, so no Keychain prompt"
    );
    assert_eq!(backend.session().connection().state(), HttpConnectionState::Connected);
}

#[tokio::test]
async fn a_backend_with_no_secret_can_always_reconnect_unattended_while_its_switch_is_on() {
    let store = Arc::new(CountingStore::default());
    let backend = backend(Arc::clone(&store));

    assert_eq!(backend.unattended_reconnect().await, UnattendedReconnect::Possible);
    backend.set_auto_reconnect(false);
    assert_eq!(backend.unattended_reconnect().await, UnattendedReconnect::SwitchOff);
    assert_eq!(store.reads.load(Ordering::SeqCst), 0);
}

#[test]
fn a_session_takes_the_backends_own_silence_ladder() {
    let store = Arc::new(CountingStore::default());
    let host = VolumeHost::builder().credentials(store).build();
    let quick = Timings {
        quiet: std::time::Duration::from_secs(5),
        probe_budget: std::time::Duration::from_secs(5),
        unanswered_limit: 2,
    };
    let backend: Arc<NoSecretBackend> = Arc::new_cyclic(|me| NoSecretBackend {
        session: HttpSession::new(me, "test-volume", &host, None, true).with_silence_timings(quick),
        host: host.clone(),
        dialed_with: Mutex::new(Vec::new()),
    });
    assert_eq!(backend.session().silence_timings(), quick);
}
