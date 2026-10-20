//! The NFS backend: a `Volume` over one export, through our own NFSv3 client.
//!
//! There is no OS mount under this: every listing, read, and write is an RPC on
//! a TCP connection this crate holds. The volume's root is the export, so the
//! paths it hands out are export-relative (`/Photos/a.jpg` under `/Public`), and
//! `cmdr_fs::volume::remote_paths::RemoteRoot` is the one translation to the app's
//! spelling (`nfs://nas.local:2049/Public/Photos/a.jpg`).
//!
//! Nothing here names the application. What the backend needs from it arrives
//! through the [`VolumeHost`] seams handed to [`connect_nfs_volume`].

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::ignore_poison::RwLockIgnorePoison;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::remote_paths::RemoteRoot;
use tokio_util::sync::CancellationToken;

use crate::errors::NfsConnectError;
use crate::params::NfsConnectionParams;

mod discovery;
mod volume_impl;

use discovery::resolve_export;
pub use discovery::{NfsIdentityGuess, NfsIdentitySource, guess_identity, list_exports};

/// A volume backed by one export on an NFS server.
pub struct NfsVolume {
    /// Display name, as the app chose to label the place.
    name: String,
    /// Both spellings of the export's root: the app's
    /// (`nfs://nas.local:2049/Public`) and the export-relative one (`/`).
    /// ❗ Every path translation goes through it, ❌ nothing here spells an app
    /// path by hand.
    root: RemoteRoot,
    inner: Arc<NfsVolumeInner>,
}

/// The connection-scoped half: what every instance of this volume shares.
struct NfsVolumeInner {
    /// The key every piece of durable per-volume state is filed under, minted
    /// from the RESOLVED export (`params.volume_id()`).
    volume_id: String,
    /// How to reach the export again, as the dial resolved it. Behind a lock
    /// because the auto-reconnect switch moves on the live volume.
    params: std::sync::RwLock<NfsConnectionParams>,
}

impl NfsVolume {
    /// The volume id every listing-cache lookup and connection event uses.
    pub fn volume_id(&self) -> &str {
        &self.inner.volume_id
    }

    /// The params this volume dials with, as the connect RESOLVED them: the real
    /// export, and the rest of a typed path as the start folder when none was
    /// given. ❗ What the caller saves, so a saved place never carries a folder
    /// in its export, and the id it registers under is this one's.
    pub fn params(&self) -> NfsConnectionParams {
        self.inner.params.read_ignore_poison().clone()
    }

    /// Another instance of this volume, named `name`, riding THIS instance's
    /// connection: what saving a name or start-folder edit to a connected place
    /// installs, with no redial. ❗ A changed uid or gid is a new connection,
    /// ❌ never this.
    pub fn sharing_connection(&self, name: &str) -> NfsVolume {
        NfsVolume {
            name: name.to_string(),
            root: RemoteRoot::new(self.root.app_root().to_string_lossy().into_owned(), Path::new("/")),
            inner: Arc::clone(&self.inner),
        }
    }

    /// Moves the place's "Reconnect automatically" switch on the live volume.
    pub fn set_auto_reconnect(&self, on: bool) {
        self.inner.params.write_ignore_poison().auto_reconnect = on;
    }
}

/// Opens an NFS export: dials the server, splits `params`' export into the real
/// export and a folder under it, mounts the export as the params' identity, and
/// proves the root handle.
///
/// ❗ **The split happens here, before an id exists.** A typed
/// `/share/Public/photos` resolves by longest match against the server's
/// exports (or, when it won't list them, by `MNT` on ever-shorter prefixes),
/// and the rest becomes the start folder when none was given. So the caller
/// registers under the returned volume's [`NfsVolume::volume_id`] and saves its
/// [`NfsVolume::params`], ❌ never the id of what it passed in. Cancelling
/// `cancel` ends the attempt where it stands and leaves ❗ nothing behind.
///
/// The client isn't built yet, so every dial answers a typed
/// [`NfsConnectError::Transport`].
pub async fn connect_nfs_volume(
    name: &str,
    params: NfsConnectionParams,
    host: VolumeHost,
    cancel: CancellationToken,
) -> Result<NfsVolume, NfsConnectError> {
    let resolved = resolve_export(&params.server, params.export(), params.identity, &cancel).await?;
    let _ = (name, resolved, host);
    Err(NfsConnectError::Transport("the NFS client isn't built yet".to_string()))
}
