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

/// The uid and gid the add form fills "Act as user ID" and "Group ID" with.
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
