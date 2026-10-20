//! The server half of ONC RPC framing, for in-process fakes: read a call off
//! a stream, answer it (or don't, or answer garbage). The transport cells
//! script it one call at a time; a fake NFS server loops over it and
//! dispatches by procedure.

use bytes::Bytes;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

use crate::proto::rpc::REPLY;
use crate::transport::{DEFAULT_RECORD_CAP, frame_record, read_record};
use crate::xdr::{Decoder, Encoder, XdrError};

/// One decoded call, as a server sees it.
pub(crate) struct CallFrame {
    /// The call's XID, which its reply must echo.
    pub(crate) xid: u32,
    /// The program number.
    pub(crate) program: u32,
    /// The program version.
    pub(crate) version: u32,
    /// The procedure number.
    pub(crate) procedure: u32,
    /// The credential's flavor (0 `AUTH_NONE`, 1 `AUTH_SYS`).
    pub(crate) credential_flavor: u32,
    /// The credential's body.
    pub(crate) credential: Bytes,
    /// The procedure's arguments, undecoded.
    pub(crate) args: Decoder,
}

/// Decodes a call record (RFC 5531 § 9).
pub(crate) fn decode_call(record: Bytes) -> Result<CallFrame, XdrError> {
    let mut dec = Decoder::new(record);
    let xid = dec.u32()?;
    let msg_type = dec.u32()?;
    if msg_type != 0 {
        return Err(XdrError::BadDiscriminant {
            what: "msg_type",
            value: msg_type,
        });
    }
    let _rpc_version = dec.u32()?;
    let program = dec.u32()?;
    let version = dec.u32()?;
    let procedure = dec.u32()?;
    let credential_flavor = dec.u32()?;
    let credential = dec.opaque(400)?;
    let _verifier_flavor = dec.u32()?;
    let _verifier = dec.opaque(400)?;
    Ok(CallFrame {
        xid,
        program,
        version,
        procedure,
        credential_flavor,
        credential,
        args: dec,
    })
}

/// A framed `MSG_ACCEPTED` + `SUCCESS` reply to `xid`, its results written by
/// `results`.
pub(crate) fn success_reply(xid: u32, results: impl FnOnce(&mut Encoder)) -> Bytes {
    frame_record(|enc| {
        accepted_header(enc, xid);
        enc.u32(0);
        results(enc);
    })
}

/// A framed `MSG_ACCEPTED` reply with `accept_stat` (and the version range,
/// for `PROG_MISMATCH`).
pub(crate) fn not_accepted_reply(xid: u32, accept_stat: u32, mismatch: Option<(u32, u32)>) -> Bytes {
    frame_record(|enc| {
        accepted_header(enc, xid);
        enc.u32(accept_stat);
        if let Some((low, high)) = mismatch {
            enc.u32(low);
            enc.u32(high);
        }
    })
}

/// A framed `MSG_DENIED` + `AUTH_ERROR` reply with `auth_stat`.
pub(crate) fn auth_denied_reply(xid: u32, auth_stat: u32) -> Bytes {
    frame_record(|enc| {
        enc.u32(xid);
        enc.u32(REPLY);
        enc.u32(1);
        enc.u32(1);
        enc.u32(auth_stat);
    })
}

fn accepted_header(enc: &mut Encoder, xid: u32) {
    enc.u32(xid);
    enc.u32(REPLY);
    enc.u32(0);
    // An `AUTH_NONE` verifier.
    enc.u32(0);
    enc.opaque(&[]);
}

/// One accepted client connection, server side.
pub(crate) struct ServerConn<S> {
    stream: S,
}

impl<S: AsyncRead + AsyncWrite + Unpin> ServerConn<S> {
    /// Wraps an accepted stream.
    pub(crate) fn new(stream: S) -> Self {
        Self { stream }
    }

    /// The next call, or `None` once the client closed (or sent something that
    /// isn't a call).
    pub(crate) async fn next_call(&mut self) -> Option<CallFrame> {
        let record = read_record(&mut self.stream, DEFAULT_RECORD_CAP, &|| {}).await.ok()?;
        decode_call(record).ok()
    }

    /// Writes `bytes` as they stand: a framed reply, or deliberate garbage.
    pub(crate) async fn send(&mut self, bytes: &[u8]) {
        // A client that's gone is the cell's own business to assert on.
        let _ = self.stream.write_all(bytes).await;
    }

    /// Answers `xid` with `SUCCESS` and the given results.
    pub(crate) async fn reply(&mut self, xid: u32, results: impl FnOnce(&mut Encoder)) {
        let reply = success_reply(xid, results);
        self.send(&reply).await;
    }
}

/// Server-side encoders for the NFSv3 types a fake answers with: the mirror of
/// `nfs3`'s decoders.
pub(crate) mod nfs3_wire {
    use crate::proto::nfs3::{Fattr3, FileHandle, FileType, NfsTime};
    use crate::xdr::Encoder;

    /// `ftype3`'s number.
    pub(crate) fn file_type_number(file_type: FileType) -> u32 {
        match file_type {
            FileType::Regular => 1,
            FileType::Directory => 2,
            FileType::BlockDevice => 3,
            FileType::CharDevice => 4,
            FileType::Symlink => 5,
            FileType::Socket => 6,
            FileType::Fifo => 7,
        }
    }

    fn time(enc: &mut Encoder, time: NfsTime) {
        enc.u32(time.seconds);
        enc.u32(time.nseconds);
    }

    /// `fattr3`.
    pub(crate) fn fattr(enc: &mut Encoder, attrs: &Fattr3) {
        enc.u32(file_type_number(attrs.file_type));
        enc.u32(attrs.mode);
        enc.u32(attrs.nlink);
        enc.u32(attrs.uid);
        enc.u32(attrs.gid);
        enc.u64(attrs.size);
        enc.u64(attrs.used);
        enc.u32(attrs.rdev.0);
        enc.u32(attrs.rdev.1);
        enc.u64(attrs.fsid);
        enc.u64(attrs.fileid);
        time(enc, attrs.atime);
        time(enc, attrs.mtime);
        time(enc, attrs.ctime);
    }

    /// `post_op_attr`.
    pub(crate) fn post_op_attr(enc: &mut Encoder, attrs: Option<&Fattr3>) {
        enc.bool(attrs.is_some());
        if let Some(attrs) = attrs {
            fattr(enc, attrs);
        }
    }

    /// `nfs_fh3`.
    pub(crate) fn handle(enc: &mut Encoder, handle: &FileHandle) {
        enc.opaque(&handle.0);
    }

    /// `post_op_fh3`.
    pub(crate) fn post_op_fh(enc: &mut Encoder, handle: Option<&FileHandle>) {
        enc.bool(handle.is_some());
        if let Some(handle) = handle {
            self::handle(enc, handle);
        }
    }

    /// A plain file or folder's attributes, the rest zeroed: what most fakes
    /// need.
    pub(crate) fn simple_attrs(file_type: FileType, fileid: u64, size: u64) -> Fattr3 {
        Fattr3 {
            file_type,
            mode: 0o755,
            nlink: 1,
            uid: 1000,
            gid: 1000,
            size,
            used: size,
            rdev: (0, 0),
            fsid: 1,
            fileid,
            atime: NfsTime::default(),
            mtime: NfsTime::default(),
            ctime: NfsTime::default(),
        }
    }
}
