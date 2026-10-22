//! What this backend brings to the shared lifecycle
//! (`cmdr_fs::volume::http_session`, whose header has the rules): the client is
//! one NFS connection plus the export's root handle, the probe is RPC `NULL` on
//! a fresh connection, and there is no secret.
//!
//! - ❗ **No secret, so no credential states.** `credential_key` is `None`, the
//!   dial answers only [`DialRefusal::Transient`] (a refused export or a missing
//!   reserved port on a REDIAL keeps backing off), and nothing here ever parks
//!   the volume in `NeedsCredentials`: that would publish `NeedsSignIn`, a
//!   sign-in view with nothing to type.
//! - **A redial is a whole dial**: portmap, `MNT`, `FSINFO`, `GETATTR`. v3 is
//!   stateless and an old root handle would usually still work, but a server
//!   that restarted may have moved its ports (portmap answers anew), and a
//!   stale root handle needs a fresh `MNT` anyway. A few small calls per
//!   reconnect buy both for free.

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::http_session::{CredentialKey, DialRefusal, HttpBackend, HttpSession, SessionClient};
use cmdr_fs::volume::liveness::Liveness;
use cmdr_fs::volume::{Retirement, Retires, VolumeError};
use tokio_util::sync::CancellationToken;

use super::NfsVolumeInner;
use crate::dial::{self, DialOptions, MountedExport};
use crate::errors::NfsConnectError;
use crate::params::NfsConnectionParams;
use crate::proto::nfs3::{FileHandle, FsInfo, NFS_V3, Nfs3Client};
use crate::proto::rpc::Credential;
use crate::transport::{self, Probe};

/// One live connection to the export: what the session holds as its client.
pub(crate) struct NfsClient {
    /// Typed NFSv3 calls on the connection.
    pub(crate) nfs: Nfs3Client,
    /// The export's root handle, as this connection's `MNT` answered it.
    pub(crate) root: FileHandle,
    /// The server's transfer sizes.
    pub(crate) fsinfo: FsInfo,
    /// The silence ladder's question: RPC `NULL` on a FRESH connection.
    probe: Probe,
    /// Where `UMNT` goes when the volume leaves, and as whom. `None` for a
    /// cell's fake, which keeps no mount table.
    umnt: Option<UmntTarget>,
}

/// Where and how to tell the server's mount table this client is done.
#[derive(Clone)]
pub(crate) struct UmntTarget {
    mount_at: SocketAddr,
    credential: Credential,
    export: String,
}

impl NfsClient {
    /// The client a real dial produced.
    pub(crate) fn from_mounted(mounted: MountedExport) -> Self {
        let nfs_at = mounted.addresses.nfs;
        Self {
            nfs: mounted.nfs,
            root: mounted.root,
            fsinfo: mounted.fsinfo,
            probe: Arc::new(move || Box::pin(transport::ping(nfs_at, NFS_V3))),
            umnt: Some(UmntTarget {
                mount_at: mounted.addresses.mount,
                credential: mounted.credential,
                export: mounted.export,
            }),
        }
    }

    /// A client over a connection a cell built, with `probe` as its silence
    /// question.
    #[cfg(test)]
    pub(crate) fn over_connection(nfs: Nfs3Client, root: FileHandle, fsinfo: FsInfo, probe: Probe) -> Self {
        Self {
            nfs,
            root,
            fsinfo,
            probe,
            umnt: None,
        }
    }

    /// Where this connection's `UMNT` goes.
    pub(crate) fn umnt_target(&self) -> Option<UmntTarget> {
        self.umnt.clone()
    }

    /// Closes the connection now: every waiting call answers at once.
    pub(crate) fn close(&self) {
        self.nfs.connection().close();
    }
}

impl SessionClient for NfsClient {
    fn liveness(&self) -> &Arc<Liveness> {
        self.nfs.connection().liveness()
    }

    fn ping(&self) -> impl Future<Output = bool> + Send {
        (self.probe)()
    }
}

/// What a typed export path resolved to on the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Resolved {
    /// The export, canonical.
    pub(crate) export: String,
    /// The export-relative folder the rest of the typed path names (`/photos`),
    /// or `None` when the path was the export itself.
    pub(crate) folder: Option<String>,
}

type DialFuture = Pin<Box<dyn Future<Output = Result<(NfsClient, Resolved), NfsConnectError>> + Send>>;
type DialFn = dyn Fn(NfsConnectionParams, CancellationToken) -> DialFuture + Send + Sync;

/// How this volume reaches its export: the real dial over TCP, or a cell's
/// fake. Both the first connect and every redial go through it.
#[derive(Clone)]
pub(crate) struct Dialer {
    options: DialOptions,
    dial: Arc<DialFn>,
}

impl Dialer {
    /// The real dial ([`dial::mount`]) under `options`.
    pub(crate) fn over_tcp(options: DialOptions) -> Self {
        Self {
            options,
            dial: Arc::new(move |params: NfsConnectionParams, cancel: CancellationToken| {
                Box::pin(async move {
                    let mounted =
                        dial::mount(&params.server, params.export(), params.identity, &options, &cancel).await?;
                    let resolved = Resolved {
                        export: mounted.export.clone(),
                        folder: mounted.folder.clone(),
                    };
                    Ok((NfsClient::from_mounted(mounted), resolved))
                })
            }),
        }
    }

    /// A dial a cell scripts, under `options`.
    #[cfg(test)]
    pub(crate) fn scripted(
        options: DialOptions,
        dial: impl Fn(NfsConnectionParams, CancellationToken) -> DialFuture + Send + Sync + 'static,
    ) -> Self {
        Self {
            options,
            dial: Arc::new(dial),
        }
    }

    /// The budgets this dialer runs under, and the transport's ladder.
    pub(crate) fn options(&self) -> &DialOptions {
        &self.options
    }

    /// The first dial, which also splits the typed export from a folder.
    pub(crate) async fn first(
        &self,
        params: &NfsConnectionParams,
        cancel: &CancellationToken,
    ) -> Result<(NfsClient, Resolved), NfsConnectError> {
        (self.dial)(params.clone(), cancel.clone()).await
    }
}

impl Retires for NfsVolumeInner {
    fn retirement(&self) -> &Retirement {
        self.session.connection().retirement()
    }
}

impl HttpBackend for NfsVolumeInner {
    type Client = NfsClient;

    const BACKEND: &'static str = "nfs";

    fn session(&self) -> &HttpSession<Self> {
        &self.session
    }

    fn host(&self) -> &VolumeHost {
        &self.host
    }

    /// ❗ `AUTH_SYS` carries a uid, nothing to keep: no secret, no store read.
    fn credential_key(&self) -> Option<CredentialKey> {
        None
    }

    async fn dial(&self, _secret: &str) -> Result<NfsClient, DialRefusal> {
        let params = self.params();
        match self.dialer.first(&params, &CancellationToken::new()).await {
            Ok((client, _)) => {
                // A fresh mount may hand out a new root handle, and nothing
                // cached under the old one can be trusted to match it.
                self.handles.clear();
                *self.umnt.lock_ignore_poison() = client.umnt.clone();
                Ok(client)
            }
            Err(err) => Err(DialRefusal::Transient(self.redial_failure(err))),
        }
    }
}

impl NfsVolumeInner {
    /// A redial's failure in the `Volume` vocabulary. ❗ Every arm is
    /// transient: an export refused on a redial (a NAS still booting, an
    /// `/etc/exports` edit) keeps backing off, ❌ never asks a person to sign
    /// in.
    fn redial_failure(&self, err: NfsConnectError) -> VolumeError {
        let id = || self.volume_id.clone();
        match err {
            NfsConnectError::Cancelled => VolumeError::Cancelled(id().into()),
            NfsConnectError::TimedOut => VolumeError::ConnectionTimeout(id().into()),
            NfsConnectError::ExportNotFound => VolumeError::NotFound(id()),
            NfsConnectError::ExportRefused | NfsConnectError::ReservedPortRequired => VolumeError::PermissionDenied {
                path: id(),
                raw_os_error: None,
            },
            NfsConnectError::Unreachable(why) | NfsConnectError::Transport(why) => {
                VolumeError::DeviceDisconnected(why.into())
            }
            NfsConnectError::NoNfsV3 | NfsConnectError::ListingRefused => VolumeError::DeviceDisconnected(id().into()),
        }
    }

    /// The volume is leaving the registry: no more reconnects, the connection
    /// goes (it closes when the last call riding it ends), and the server's
    /// mount table hears `UMNT`, best effort.
    pub(super) fn leave(self: &Arc<Self>) {
        self.unmount();
        let Some(target) = self.umnt.lock_ignore_poison().take() else {
            return;
        };
        let options = *self.dialer.options();
        self.host.runtime().spawn(async move {
            dial::unmount(target.mount_at, &target.export, target.credential, &options).await;
        });
    }
}
