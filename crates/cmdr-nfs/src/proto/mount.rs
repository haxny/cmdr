//! MOUNT v3 (RFC 1813 Appendix I): turning an export path into the root file
//! handle, and asking which exports there are.

use std::time::Duration;

use bytes::Bytes;

use crate::proto::nfs3::{FileHandle, decode_handle};
use crate::proto::rpc::{Procedure, Program, Repeat};
use crate::transport::{CallError, RpcConnection};
use crate::xdr::{Decoder, XdrError};

/// MOUNT v3: program 100005, version 3.
pub(crate) const MOUNT_V3: Program = Program {
    number: 100_005,
    version: 3,
};

/// `MNTPATHLEN`: the longest export path.
pub(crate) const MAX_PATH: usize = 1024;
/// `MNTNAMLEN`: the longest group (client) name in an export's list.
const MAX_NAME: usize = 255;

const MNT: Procedure = Procedure {
    program: MOUNT_V3,
    number: 1,
    // A second MNT of the same path answers the same handle.
    repeat: Repeat::Safe,
};

const UMNT: Procedure = Procedure {
    program: MOUNT_V3,
    number: 3,
    repeat: Repeat::Safe,
};

const EXPORT: Procedure = Procedure {
    program: MOUNT_V3,
    number: 5,
    repeat: Repeat::Safe,
};

/// `mountstat3`: why `MNT` refused, by number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MountStat {
    /// `MNT3ERR_PERM` (1).
    Perm,
    /// `MNT3ERR_NOENT` (2): no such export (or path).
    NoEnt,
    /// `MNT3ERR_IO` (5).
    Io,
    /// `MNT3ERR_ACCES` (13): refused this client. knfsd answers it for a
    /// `secure` export asked from a port past 1023.
    Access,
    /// `MNT3ERR_NOTDIR` (20).
    NotDir,
    /// `MNT3ERR_INVAL` (22).
    Inval,
    /// `MNT3ERR_NAMETOOLONG` (63).
    NameTooLong,
    /// `MNT3ERR_NOTSUPP` (10004).
    NotSupp,
    /// `MNT3ERR_SERVERFAULT` (10006).
    ServerFault,
    /// A number RFC 1813 doesn't define.
    Unknown(u32),
}

impl MountStat {
    fn from_number(number: u32) -> Self {
        match number {
            1 => Self::Perm,
            2 => Self::NoEnt,
            5 => Self::Io,
            13 => Self::Access,
            20 => Self::NotDir,
            22 => Self::Inval,
            63 => Self::NameTooLong,
            10004 => Self::NotSupp,
            10006 => Self::ServerFault,
            other => Self::Unknown(other),
        }
    }
}

/// A successful `MNT`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Mounted {
    /// The export's root handle.
    pub(crate) root: FileHandle,
    /// The auth flavors the server accepts on this export, in its order of
    /// preference.
    pub(crate) auth_flavors: Vec<u32>,
}

/// `MNT` of `path`: the export's root handle, or the server's refusal.
pub(crate) async fn mnt(
    conn: &RpcConnection,
    path: &[u8],
    budget: Option<Duration>,
) -> Result<Result<Mounted, MountStat>, CallError> {
    let mut results = conn.call(MNT, |enc| enc.opaque(path), budget).await?;
    Ok(decode_mnt(&mut results)?)
}

/// `MNT`'s result.
pub(crate) fn decode_mnt(results: &mut Decoder) -> Result<Result<Mounted, MountStat>, XdrError> {
    let status = results.u32()?;
    if status != 0 {
        return Ok(Err(MountStat::from_number(status)));
    }
    let root = decode_handle(results)?;
    let count = results.array_len(usize::MAX, 4)?;
    let mut auth_flavors = Vec::with_capacity(count);
    for _ in 0..count {
        auth_flavors.push(results.u32()?);
    }
    Ok(Ok(Mounted { root, auth_flavors }))
}

/// `UMNT` of `path`: tells the server's mount table this client is done. Best
/// effort by nature: NFSv3 is stateless, and the table is only bookkeeping.
pub(crate) async fn umnt(conn: &RpcConnection, path: &[u8], budget: Option<Duration>) -> Result<(), CallError> {
    conn.call(UMNT, |enc| enc.opaque(path), budget).await?;
    Ok(())
}

/// `EXPORT`: every export path the server lists to this client, as raw bytes
/// (a name may not be UTF-8; the caller decides).
pub(crate) async fn export(conn: &RpcConnection, budget: Option<Duration>) -> Result<Vec<Bytes>, CallError> {
    let mut results = conn.call(EXPORT, |_| {}, budget).await?;
    Ok(decode_exports(&mut results)?)
}

/// `EXPORT`'s result: a linked list of `exportnode`s, each with a linked list
/// of group names this client doesn't need.
pub(crate) fn decode_exports(results: &mut Decoder) -> Result<Vec<Bytes>, XdrError> {
    let mut exports = Vec::new();
    while results.bool()? {
        exports.push(results.opaque(MAX_PATH)?);
        while results.bool()? {
            results.opaque(MAX_NAME)?;
        }
    }
    Ok(exports)
}
