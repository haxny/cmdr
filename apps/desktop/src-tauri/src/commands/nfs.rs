//! The IPC surface for NFS servers: what the add form asks before a place
//! exists. Connecting goes through the protocol-agnostic `commands::servers`
//! facade.
//!
//! Pass-throughs to `cmdr_nfs`. ❌ **No result here is a string**: the form
//! branches on "the server won't list its exports" (a typed export still works)
//! against "nothing answered", and a message can't carry that.

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use cmdr_nfs::{NfsConnectError, NfsIdentity, NfsIdentitySource, NfsServer};

// ============================================================================
// The wire vocabulary
// ============================================================================

/// What asking a server for its exports produced.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case", tag = "outcome", rename_all_fields = "camelCase")]
pub enum NfsExportList {
    /// The exports the server lists, each canonical (`/Public`, `/volume1/data`).
    Listed {
        /// Possibly empty: a server may export nothing to this Mac.
        exports: Vec<String>,
    },
    /// The server is there but won't list its exports to this Mac. ❗ Not a
    /// dead end: an export typed by hand may still mount.
    ListingRefused,
    /// The server speaks no NFSv3 over TCP.
    NoNfsV3,
    /// Nothing answered inside the dial budget.
    TimedOut,
    /// No route, refused, DNS, or a reply that made no sense.
    Unreachable,
}

/// Where a guessed identity came from, so the form can say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum NfsIdentitySourceWire {
    /// The most common non-zero owner of the files at the export's root.
    MostCommonOwner,
    /// The owner of the export's root folder itself.
    RootOwner,
    /// This Mac's own uid and gid: nothing there had a non-zero owner, or the
    /// export couldn't be read to say.
    ThisMac,
}

/// The uid and gid the add form fills "User ID" and "Group ID" with.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GuessedNfsIdentity {
    /// The user id.
    pub uid: u32,
    /// The group id.
    pub gid: u32,
    /// Why these.
    pub source: NfsIdentitySourceWire,
}

impl From<NfsIdentitySource> for NfsIdentitySourceWire {
    fn from(source: NfsIdentitySource) -> Self {
        match source {
            NfsIdentitySource::MostCommonOwner => Self::MostCommonOwner,
            NfsIdentitySource::RootOwner => Self::RootOwner,
            NfsIdentitySource::ThisMac => Self::ThisMac,
        }
    }
}

// ============================================================================
// Commands
// ============================================================================

/// The exports the server at `host` (and `port`, when the address named one)
/// lists.
#[tauri::command]
#[specta::specta]
pub async fn list_nfs_exports(host: String, port: Option<u16>) -> NfsExportList {
    let server = NfsServer::new(&host, port);
    match cmdr_nfs::list_exports(&server, CancellationToken::new()).await {
        Ok(exports) => NfsExportList::Listed { exports },
        Err(
            NfsConnectError::ListingRefused | NfsConnectError::ExportRefused | NfsConnectError::ReservedPortRequired,
        ) => NfsExportList::ListingRefused,
        Err(NfsConnectError::NoNfsV3) => NfsExportList::NoNfsV3,
        Err(NfsConnectError::TimedOut) => NfsExportList::TimedOut,
        Err(
            NfsConnectError::Unreachable(_)
            | NfsConnectError::Transport(_)
            | NfsConnectError::ExportNotFound
            | NfsConnectError::Cancelled,
        ) => NfsExportList::Unreachable,
    }
}

/// Who the files in the export `export_path` on `host` belong to, as a uid and
/// gid to act as. (`export_path`, ❌ not `export`: the binding's parameter would
/// be a reserved word in TypeScript.)
///
/// ❗ Always answers: an export that can't be read to say falls back to this
/// Mac's own uid and gid, and the connect reports its own refusal.
#[tauri::command]
#[specta::specta]
pub async fn guess_nfs_identity(host: String, port: Option<u16>, export_path: String) -> GuessedNfsIdentity {
    let this_mac = this_mac_identity();
    let server = NfsServer::new(&host, port);
    let guess = cmdr_nfs::guess_identity(&server, &export_path, this_mac, CancellationToken::new()).await;
    let (identity, source) = match guess {
        Ok(guess) => (guess.identity, guess.source.into()),
        Err(_) => (this_mac, NfsIdentitySourceWire::ThisMac),
    };
    GuessedNfsIdentity {
        uid: identity.uid,
        gid: identity.gid,
        source,
    }
}

/// This process's own uid and gid.
fn this_mac_identity() -> NfsIdentity {
    // SAFETY: `getuid` and `getgid` take no arguments, can't fail, and touch no
    // memory of ours.
    let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };
    NfsIdentity { uid, gid }
}

/// One saved NFS place as the edit sheet reads it: the stored entry plus the
/// volume id its row and its switcher entry carry, so a caller matches by id
/// rather than re-deriving one.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SavedNfsPlace {
    /// The place's volume id (`cmdr_fs::volume::nfs_volume_id`).
    pub volume_id: String,
    /// The server, as the person typed it.
    pub host: String,
    /// The NFS port the address named, or `null` for the portmapper's.
    pub port: Option<u16>,
    /// The export, canonical (`/volume1/data`).
    pub export: String,
    /// The user id every request acts as.
    pub uid: u32,
    /// The group id every request acts as.
    pub gid: u32,
    /// The name a person gave its SERVER, empty when nobody did. ❗ The
    /// server's, ❌ never the place's: an export reads as its own path.
    pub display_name: String,
    /// Where a pane lands when the place is opened, export-relative, or `null`
    /// for the export's root.
    pub start_folder: Option<String>,
    /// The place's "Reconnect automatically" switch.
    pub auto_reconnect: bool,
    /// Whether it shows in the volume switcher.
    pub pinned: bool,
}

/// Every NFS place the user has saved, for an edit sheet that has to show (and
/// resend) the uid, gid, and start folder the servers listing doesn't carry.
#[tauri::command]
#[specta::specta]
pub fn get_known_nfs_places() -> Vec<SavedNfsPlace> {
    crate::network::nfs_known_places::all()
        .into_iter()
        .map(|entry| SavedNfsPlace {
            volume_id: entry.volume_id(),
            display_name: crate::network::nfs_known_places::server_name(&entry),
            host: entry.host,
            port: entry.port,
            export: entry.export,
            uid: entry.uid,
            gid: entry.gid,
            start_folder: entry.start_folder,
            auto_reconnect: entry.auto_reconnect,
            pinned: entry.pinned,
        })
        .collect()
}
