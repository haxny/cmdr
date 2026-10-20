//! Why a connection attempt didn't produce a working volume, and how the
//! protocol's numbers map onto the `Volume` vocabulary.

use cmdr_fs::volume::VolumeError;

use crate::proto::nfs3::{NfsError, NfsStat3};
use crate::proto::rpc::Rejection;
use crate::transport::CallError;

/// Why dialing an NFS server, listing its exports, or mounting one didn't work.
///
/// ❗ Every variant comes from a NUMBER the server sent (`mountstat3`,
/// `accept_stat`, `reject_stat`, `auth_stat`) or from the transport, ❌ never
/// from a message. The `String`s are log-only diagnostics, ❌ never shown and
/// never branched on.
#[derive(Debug)]
pub enum NfsConnectError {
    /// The server couldn't be reached: DNS, refused, no route, or a connection
    /// that broke during the dial.
    Unreachable(String),
    /// The dial didn't finish inside its budget.
    TimedOut,
    /// The server offers no NFSv3 over TCP (portmap lists none, or a call
    /// answered `PROG_MISMATCH`). ❌ Never a fallback to another version.
    NoNfsV3,
    /// No export by that path on this server (`MNT3ERR_NOENT`).
    ExportNotFound,
    /// The server refused this client the export (`MNT3ERR_ACCES`, an `AUTH_*`
    /// rejection, or a mount whose flavors don't include `AUTH_SYS`), while the
    /// request came from a reserved port.
    ExportRefused,
    /// The server won't list its exports to this client (MOUNT's `EXPORT`
    /// refused). ❗ Distinct from [`Unreachable`](Self::Unreachable): the server
    /// is there, and an export typed by hand may still mount.
    ListingRefused,
    /// The export's refusal, while this process couldn't bind a source port below
    /// 1024: a `secure` export refuses every client that doesn't, so the fix is
    /// the port, ❌ not the export.
    ReservedPortRequired,
    /// Anything else: a reply that wouldn't decode, a status no other variant
    /// covers.
    Transport(String),
    /// The user called it off. Nothing is wrong with the server.
    Cancelled,
}

/// Turns an NFS procedure's failure into the `Volume` vocabulary, for an
/// operation on `path`.
///
/// ❗ **`path` is the payload, ❌ not context.** `NotFound`, `PermissionDenied`,
/// and `AlreadyExists` carry the path the user acted on, never server text:
/// the transfer layer renders it as a filename.
pub(crate) fn map_nfs_error(err: &NfsError, path: &str) -> VolumeError {
    match err {
        NfsError::Status(status) => map_nfs_status(*status, path),
        NfsError::Call(call) => map_call_error(call, path),
        NfsError::NoProgress => io_error("a listing page came back empty without ending", libc::EIO),
    }
}

/// `nfsstat3` → `VolumeError`, by number. `JUKEBOX` reaching here has already
/// been retried three times.
pub(crate) fn map_nfs_status(status: NfsStat3, path: &str) -> VolumeError {
    let located = || path.to_string();
    match status {
        NfsStat3::NoEnt | NfsStat3::Stale | NfsStat3::BadHandle => VolumeError::NotFound(located()),
        NfsStat3::Access => VolumeError::PermissionDenied {
            path: located(),
            raw_os_error: Some(libc::EACCES),
        },
        // "Not the owner", or a `secure` export refusing a non-reserved port.
        // ❌ Not `EPERM`, which the transfer layer reads as macOS's own lock.
        NfsStat3::Perm => VolumeError::PermissionDenied {
            path: located(),
            raw_os_error: None,
        },
        NfsStat3::Exist => VolumeError::AlreadyExists(located()),
        NfsStat3::NotDir => VolumeError::NotADirectory(located()),
        NfsStat3::IsDir => VolumeError::IsADirectory(located()),
        NfsStat3::NotEmpty => io_error(&format!("{path} isn't empty"), libc::ENOTEMPTY),
        NfsStat3::NoSpc | NfsStat3::DQuot => VolumeError::StorageFull {
            message: format!("{status:?}: {path}"),
        },
        NfsStat3::Rofs => VolumeError::ReadOnly(located()),
        NfsStat3::NameTooLong => VolumeError::InvalidName(format!("{status:?}: {path}")),
        NfsStat3::Jukebox => io_error("the server kept answering JUKEBOX", libc::EAGAIN),
        NfsStat3::NotSupp | NfsStat3::BadType => VolumeError::NotSupported,
        NfsStat3::Io | NfsStat3::ServerFault => io_error(&format!("{status:?}"), libc::EIO),
        NfsStat3::Nxio => io_error("NXIO", libc::ENXIO),
        NfsStat3::Xdev => io_error("XDEV", libc::EXDEV),
        NfsStat3::NoDev => io_error("NODEV", libc::ENODEV),
        NfsStat3::Inval => io_error("INVAL", libc::EINVAL),
        NfsStat3::FBig => io_error("FBIG", libc::EFBIG),
        NfsStat3::MLink => io_error("MLINK", libc::EMLINK),
        NfsStat3::Remote => io_error("REMOTE", libc::EREMOTE),
        // `NOT_SYNC` (a guarded SETATTR's ctime moved), `BAD_COOKIE` (a listing
        // past its restarts), `TOOSMALL`, and numbers RFC 1813 doesn't define.
        NfsStat3::NotSync | NfsStat3::BadCookie | NfsStat3::TooSmall | NfsStat3::Unknown(_) => VolumeError::IoError {
            message: format!("{status:?}"),
            raw_os_error: None,
        },
        // A success is never an error; keep the match exhaustive anyway.
        NfsStat3::Ok => io_error("NFS3_OK reported as a failure", libc::EIO),
    }
}

/// A call that never got an `nfsstat3` → `VolumeError`.
pub(crate) fn map_call_error(err: &CallError, path: &str) -> VolumeError {
    match err {
        // A transport fault the reconnect cycle recovers from: retiring is the
        // connection's business, and a fresh one answers the retry.
        CallError::Disconnected(why) => VolumeError::DeviceDisconnected(why.clone()),
        CallError::Retired => VolumeError::DeviceDisconnected("the connection was retired".to_string()),
        CallError::TimedOut => VolumeError::ConnectionTimeout(path.to_string()),
        // ❗ A CREATE or REMOVE that may or may not have happened: the caller
        // must look before it decides, so it's the timeout errno, ❌ not a
        // guess at the outcome.
        CallError::Unanswered => io_error("the server never answered a call that can't be resent", libc::ETIMEDOUT),
        CallError::Denied(Rejection::Auth(_)) => VolumeError::PermissionDenied {
            path: path.to_string(),
            raw_os_error: None,
        },
        CallError::Denied(Rejection::RpcMismatch { .. }) | CallError::NotAccepted(_) => VolumeError::IoError {
            message: format!("{err:?}"),
            raw_os_error: Some(libc::EPROTO),
        },
        CallError::Decode(why) => VolumeError::IoError {
            message: format!("a reply wouldn't decode: {why:?}"),
            raw_os_error: Some(libc::EBADMSG),
        },
    }
}

fn io_error(message: &str, errno: i32) -> VolumeError {
    VolumeError::IoError {
        message: message.to_string(),
        raw_os_error: Some(errno),
    }
}

#[cfg(test)]
#[path = "errors_test.rs"]
mod errors_test;
