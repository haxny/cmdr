//! NFSv3 (RFC 1813): the types and every procedure the volume uses, as typed
//! calls on one [`RpcConnection`].
//!
//! - ❗ **Optional everywhere.** `post_op_attr`, `wcc_data`, and a READDIRPLUS
//!   entry's handle and attributes may each be absent; they decode to `Option`,
//!   ❌ never to a placeholder.
//! - **Only the success arm is decoded.** A failure is its `nfsstat3`; the
//!   attributes a failure arm carries are skipped.
//! - **`NFS3ERR_JUKEBOX`** (the server is fetching the file from slow storage)
//!   is retried here after 1 s, up to three times, before it reaches the caller.
//! - **Budgets**: metadata and mutation calls carry the client's budget; READ,
//!   WRITE, and COMMIT carry none (silence is the signal, through the transport's
//!   ladder).
//!
//! `types.rs` holds the wire types and their codecs, `listing.rs` the whole-listing
//! loop; this file is the client, one typed method per procedure.

use std::time::Duration;

use bytes::Bytes;
use log::debug;

use crate::proto::rpc::{Procedure, Program, Repeat};
use crate::transport::{MAX_PAYLOAD, RpcConnection};
use crate::xdr::{Decoder, Encoder, XdrError};

mod listing;
mod types;

#[allow(
    unused_imports,
    reason = "what the volume's listing will name; drop this once the volume is wired"
)]
pub(crate) use listing::{ListedEntry, ListingEvent};
pub(crate) use types::*;

/// NFS v3: program 100003, version 3.
pub(crate) const NFS_V3: Program = Program {
    number: 100_003,
    version: 3,
};

/// How long a `JUKEBOX` answer waits before the call goes again.
const JUKEBOX_DELAY: Duration = Duration::from_secs(1);
/// How many times a `JUKEBOX` answer is retried before it's the caller's.
const JUKEBOX_RETRIES: u32 = 3;

mod procedures {
    use super::{NFS_V3, Procedure, Repeat};

    const fn safe(number: u32) -> Procedure {
        Procedure {
            program: NFS_V3,
            number,
            repeat: Repeat::Safe,
        }
    }

    const fn never(number: u32) -> Procedure {
        Procedure {
            program: NFS_V3,
            number,
            repeat: Repeat::Never,
        }
    }

    pub(super) const NULL: Procedure = safe(0);
    pub(super) const GETATTR: Procedure = safe(1);
    pub(super) const SETATTR: Procedure = never(2);
    pub(super) const LOOKUP: Procedure = safe(3);
    pub(super) const ACCESS: Procedure = safe(4);
    pub(super) const READLINK: Procedure = safe(5);
    pub(super) const READ: Procedure = safe(6);
    pub(super) const WRITE: Procedure = safe(7);
    pub(super) const CREATE: Procedure = never(8);
    pub(super) const MKDIR: Procedure = never(9);
    pub(super) const REMOVE: Procedure = never(12);
    pub(super) const RMDIR: Procedure = never(13);
    pub(super) const RENAME: Procedure = never(14);
    pub(super) const READDIR: Procedure = safe(16);
    pub(super) const READDIRPLUS: Procedure = safe(17);
    pub(super) const FSSTAT: Procedure = safe(18);
    pub(super) const FSINFO: Procedure = safe(19);
    pub(super) const PATHCONF: Procedure = safe(20);
    pub(super) const COMMIT: Procedure = safe(21);
}

/// Typed NFSv3 calls on one connection. Cheap to clone.
#[derive(Clone)]
pub(crate) struct Nfs3Client {
    conn: RpcConnection,
    /// The budget every metadata and mutation call carries. READ, WRITE, and
    /// COMMIT carry none.
    budget: Option<Duration>,
}

impl Nfs3Client {
    /// Calls over `conn`, metadata and mutations capped at `budget`.
    pub(crate) fn new(conn: RpcConnection, budget: Option<Duration>) -> Self {
        Self { conn, budget }
    }

    /// The connection under this client: its liveness, its reserved port.
    pub(crate) fn connection(&self) -> &RpcConnection {
        &self.conn
    }

    /// One procedure, retried on `JUKEBOX`, decoded by `ok` on success.
    async fn run<T>(
        &self,
        procedure: Procedure,
        budget: Option<Duration>,
        args: impl Fn(&mut Encoder),
        ok: impl FnOnce(&mut Decoder) -> Result<T, XdrError>,
    ) -> Result<T, NfsError> {
        let mut retries = 0;
        loop {
            let results = self.conn.call(procedure, &args, budget).await?;
            match status_of(results)? {
                Ok(mut dec) => return Ok(ok(&mut dec)?),
                Err(NfsStat3::Jukebox) if retries < JUKEBOX_RETRIES => {
                    retries += 1;
                    debug!(target: "nfs", "procedure {} answered JUKEBOX; asking again in {JUKEBOX_DELAY:?}", procedure.number);
                    tokio::time::sleep(JUKEBOX_DELAY).await;
                }
                Err(status) => return Err(NfsError::Status(status)),
            }
        }
    }

    /// `NULL`: does the NFS program answer at all?
    pub(crate) async fn null(&self) -> Result<(), NfsError> {
        self.conn.call(procedures::NULL, |_| {}, self.budget).await?;
        Ok(())
    }

    /// `GETATTR`.
    pub(crate) async fn getattr(&self, handle: &FileHandle) -> Result<Fattr3, NfsError> {
        self.run(
            procedures::GETATTR,
            self.budget,
            |enc| encode_handle(enc, handle),
            decode_getattr,
        )
        .await
    }

    /// `SETATTR`, refused with `NOT_SYNC` when `guard_ctime` is given and the
    /// object's `ctime` no longer matches it.
    pub(crate) async fn setattr(
        &self,
        handle: &FileHandle,
        sattr: &Sattr3,
        guard_ctime: Option<NfsTime>,
    ) -> Result<WccData, NfsError> {
        self.run(
            procedures::SETATTR,
            self.budget,
            |enc| {
                encode_handle(enc, handle);
                encode_sattr(enc, sattr);
                enc.bool(guard_ctime.is_some());
                if let Some(ctime) = guard_ctime {
                    encode_time(enc, ctime);
                }
            },
            decode_wcc,
        )
        .await
    }

    /// `LOOKUP` of `name` in `dir`. ❗ Doesn't follow a link at the end.
    pub(crate) async fn lookup(&self, dir: &FileHandle, name: &[u8]) -> Result<LookupOk, NfsError> {
        self.run(
            procedures::LOOKUP,
            self.budget,
            |enc| encode_dirop(enc, dir, name),
            decode_lookup,
        )
        .await
    }

    /// `ACCESS`: which of the [`access`] bits in `asked` this identity has.
    pub(crate) async fn access(&self, handle: &FileHandle, asked: u32) -> Result<AccessOk, NfsError> {
        self.run(
            procedures::ACCESS,
            self.budget,
            |enc| {
                encode_handle(enc, handle);
                enc.u32(asked);
            },
            |dec| {
                Ok(AccessOk {
                    attributes: decode_post_op_attr(dec)?,
                    granted: dec.u32()?,
                })
            },
        )
        .await
    }

    /// `READLINK`: a symlink's target, as the server's bytes.
    pub(crate) async fn readlink(&self, handle: &FileHandle) -> Result<Bytes, NfsError> {
        self.run(
            procedures::READLINK,
            self.budget,
            |enc| encode_handle(enc, handle),
            |dec| {
                let _attributes = decode_post_op_attr(dec)?;
                dec.opaque(MAX_NAME)
            },
        )
        .await
    }

    /// `READ` of up to `count` bytes at `offset` (capped at 1 MiB).
    pub(crate) async fn read(&self, handle: &FileHandle, offset: u64, count: u32) -> Result<ReadOk, NfsError> {
        let count = count.min(MAX_PAYLOAD);
        self.run(
            procedures::READ,
            None,
            |enc| {
                encode_handle(enc, handle);
                enc.u64(offset);
                enc.u32(count);
            },
            |dec| decode_read(dec, count),
        )
        .await
    }

    /// `WRITE` of `data` at `offset`. The caller keeps `data` within the
    /// server's `wtmax`.
    pub(crate) async fn write(
        &self,
        handle: &FileHandle,
        offset: u64,
        stable: Stable,
        data: &[u8],
    ) -> Result<WriteOk, NfsError> {
        let count = u32::try_from(data.len()).unwrap_or(u32::MAX);
        self.run(
            procedures::WRITE,
            None,
            |enc| {
                encode_handle(enc, handle);
                enc.u64(offset);
                enc.u32(count);
                enc.u32(stable.number());
                enc.opaque(data);
            },
            decode_write,
        )
        .await
    }

    /// `CREATE` of `name` in `dir`.
    pub(crate) async fn create(&self, dir: &FileHandle, name: &[u8], how: &CreateHow) -> Result<CreatedOk, NfsError> {
        self.run(
            procedures::CREATE,
            self.budget,
            |enc| {
                encode_dirop(enc, dir, name);
                match how {
                    CreateHow::Unchecked(sattr) => {
                        enc.u32(0);
                        encode_sattr(enc, sattr);
                    }
                    CreateHow::Guarded(sattr) => {
                        enc.u32(1);
                        encode_sattr(enc, sattr);
                    }
                    CreateHow::Exclusive(verifier) => {
                        enc.u32(2);
                        enc.fixed_opaque(verifier);
                    }
                }
            },
            decode_created,
        )
        .await
    }

    /// `MKDIR` of `name` in `dir`.
    pub(crate) async fn mkdir(&self, dir: &FileHandle, name: &[u8], sattr: &Sattr3) -> Result<CreatedOk, NfsError> {
        self.run(
            procedures::MKDIR,
            self.budget,
            |enc| {
                encode_dirop(enc, dir, name);
                encode_sattr(enc, sattr);
            },
            decode_created,
        )
        .await
    }

    /// `REMOVE` of the non-directory `name` in `dir`.
    pub(crate) async fn remove(&self, dir: &FileHandle, name: &[u8]) -> Result<WccData, NfsError> {
        self.run(
            procedures::REMOVE,
            self.budget,
            |enc| encode_dirop(enc, dir, name),
            decode_wcc,
        )
        .await
    }

    /// `RMDIR` of the empty directory `name` in `dir`.
    pub(crate) async fn rmdir(&self, dir: &FileHandle, name: &[u8]) -> Result<WccData, NfsError> {
        self.run(
            procedures::RMDIR,
            self.budget,
            |enc| encode_dirop(enc, dir, name),
            decode_wcc,
        )
        .await
    }

    /// `RENAME`. ❗ Replaces an existing target (a directory only when it's
    /// empty).
    pub(crate) async fn rename(
        &self,
        from_dir: &FileHandle,
        from_name: &[u8],
        to_dir: &FileHandle,
        to_name: &[u8],
    ) -> Result<RenameOk, NfsError> {
        self.run(
            procedures::RENAME,
            self.budget,
            |enc| {
                encode_dirop(enc, from_dir, from_name);
                encode_dirop(enc, to_dir, to_name);
            },
            |dec| {
                Ok(RenameOk {
                    from_dir_wcc: decode_wcc(dec)?,
                    to_dir_wcc: decode_wcc(dec)?,
                })
            },
        )
        .await
    }

    /// One `READDIR` page of about `count` bytes from `cookie`.
    pub(crate) async fn readdir(
        &self,
        dir: &FileHandle,
        cookie: u64,
        cookieverf: [u8; 8],
        count: u32,
    ) -> Result<DirPage<DirEntry>, NfsError> {
        self.run(
            procedures::READDIR,
            self.budget,
            |enc| {
                encode_handle(enc, dir);
                enc.u64(cookie);
                enc.fixed_opaque(&cookieverf);
                enc.u32(count);
            },
            decode_readdir,
        )
        .await
    }

    /// One `READDIRPLUS` page of about `maxcount` bytes from `cookie`.
    pub(crate) async fn readdirplus(
        &self,
        dir: &FileHandle,
        cookie: u64,
        cookieverf: [u8; 8],
        maxcount: u32,
    ) -> Result<DirPage<DirEntryPlus>, NfsError> {
        let maxcount = maxcount.min(MAX_PAYLOAD);
        self.run(
            procedures::READDIRPLUS,
            self.budget,
            |enc| {
                encode_handle(enc, dir);
                enc.u64(cookie);
                enc.fixed_opaque(&cookieverf);
                // `dircount` (names and cookies only) and `maxcount` (the whole
                // reply): the same number, the shape the Linux client sends.
                enc.u32(maxcount);
                enc.u32(maxcount);
            },
            decode_readdirplus,
        )
        .await
    }

    /// `FSSTAT`.
    pub(crate) async fn fsstat(&self, handle: &FileHandle) -> Result<FsStat, NfsError> {
        self.run(
            procedures::FSSTAT,
            self.budget,
            |enc| encode_handle(enc, handle),
            decode_fsstat,
        )
        .await
    }

    /// `FSINFO`.
    pub(crate) async fn fsinfo(&self, handle: &FileHandle) -> Result<FsInfo, NfsError> {
        self.run(
            procedures::FSINFO,
            self.budget,
            |enc| encode_handle(enc, handle),
            decode_fsinfo,
        )
        .await
    }

    /// `PATHCONF`.
    pub(crate) async fn pathconf(&self, handle: &FileHandle) -> Result<PathConf, NfsError> {
        self.run(
            procedures::PATHCONF,
            self.budget,
            |enc| encode_handle(enc, handle),
            decode_pathconf,
        )
        .await
    }

    /// `COMMIT` of `count` bytes at `offset` (0 and 0: the whole file).
    pub(crate) async fn commit(&self, handle: &FileHandle, offset: u64, count: u32) -> Result<CommitOk, NfsError> {
        self.run(
            procedures::COMMIT,
            None,
            |enc| {
                encode_handle(enc, handle);
                enc.u64(offset);
                enc.u32(count);
            },
            decode_commit,
        )
        .await
    }
}

#[cfg(test)]
#[path = "client_test.rs"]
mod client_test;
