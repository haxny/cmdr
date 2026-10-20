//! What the add form asks a server before a place exists: its exports, and who
//! the files in one belong to.

use tokio_util::sync::CancellationToken;

use crate::errors::NfsConnectError;
use crate::params::{NfsIdentity, NfsServer};

/// Who the add form should act as on an export, and why that one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NfsIdentityGuess {
    /// The uid and gid to fill the form with.
    pub identity: NfsIdentity,
    /// Where they came from, so the form can say.
    pub source: NfsIdentitySource,
}

/// Where an [`NfsIdentityGuess`] came from, best evidence first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NfsIdentitySource {
    /// The most common non-zero owner among the export root's entries.
    MostCommonOwner,
    /// No entry had a non-zero owner, but the export root itself does.
    RootOwner,
    /// Everything there is owned by uid 0, or there's nothing there: the
    /// caller's own fallback, this Mac's uid and gid.
    ThisMac,
}

/// The export paths `server` lists (MOUNT's `EXPORT`), each canonical
/// (`cmdr_fs::volume::canonical_nfs_export`). A name that isn't UTF-8 is left
/// out.
///
/// ❗ A server may refuse to list ([`NfsConnectError::ListingRefused`]) and still
/// mount an export typed by hand, so a refusal here is ❌ never a reason to
/// refuse the place.
///
/// The client isn't built yet, so every call answers a typed
/// [`NfsConnectError::Transport`].
pub async fn list_exports(server: &NfsServer, cancel: CancellationToken) -> Result<Vec<String>, NfsConnectError> {
    let _ = server;
    if cancel.is_cancelled() {
        return Err(NfsConnectError::Cancelled);
    }
    Err(NfsConnectError::Transport("the NFS client isn't built yet".to_string()))
}

/// Who owns the files in `export`, from one `MNT` and one `READDIRPLUS` of its
/// root as `this_mac`: what the add form fills "Act as user ID" and "Group ID"
/// with. Answers `this_mac` itself ([`NfsIdentitySource::ThisMac`]) when nothing
/// there has a non-zero owner.
///
/// The client isn't built yet, so every call answers a typed
/// [`NfsConnectError::Transport`].
pub async fn guess_identity(
    server: &NfsServer,
    export: &str,
    this_mac: NfsIdentity,
    cancel: CancellationToken,
) -> Result<NfsIdentityGuess, NfsConnectError> {
    let _ = (server, export, this_mac);
    if cancel.is_cancelled() {
        return Err(NfsConnectError::Cancelled);
    }
    Err(NfsConnectError::Transport("the NFS client isn't built yet".to_string()))
}
