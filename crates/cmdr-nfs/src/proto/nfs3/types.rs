//! NFSv3's types (RFC 1813 § 2), each with its decoder (and encoder where a
//! call sends it). ❗ `post_op_attr`, `wcc_data`, and the per-entry handle and
//! attributes of READDIRPLUS decode to `Option`, ❌ never to a placeholder.

use bytes::Bytes;

use crate::transport::CallError;
use crate::xdr::{Decoder, Encoder, XdrError};

/// `NFS3_FHSIZE`: the largest file handle.
pub(crate) const MAX_HANDLE: usize = 64;
/// The longest name or link target accepted from the wire. RFC 1813 sets no
/// limit; Linux stores 255 bytes per name and 4,095 per path.
pub(super) const MAX_NAME: usize = 4096;

/// An NFS file handle: opaque bytes the server minted, at most 64.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct FileHandle(pub(crate) Bytes);

/// `nfs_fh3` off the wire.
pub(crate) fn decode_handle(dec: &mut Decoder) -> Result<FileHandle, XdrError> {
    Ok(FileHandle(dec.opaque(MAX_HANDLE)?))
}

pub(super) fn encode_handle(enc: &mut Encoder, handle: &FileHandle) {
    enc.opaque(&handle.0);
}

/// `nfsstat3`, by number. `Ok` never reaches a caller as an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NfsStat3 {
    /// `NFS3_OK` (0).
    Ok,
    /// `NFS3ERR_PERM` (1): not the owner. knfsd answers it on every call of a
    /// `secure` export from a port past 1023.
    Perm,
    /// `NFS3ERR_NOENT` (2).
    NoEnt,
    /// `NFS3ERR_IO` (5).
    Io,
    /// `NFS3ERR_NXIO` (6).
    Nxio,
    /// `NFS3ERR_ACCES` (13).
    Access,
    /// `NFS3ERR_EXIST` (17).
    Exist,
    /// `NFS3ERR_XDEV` (18).
    Xdev,
    /// `NFS3ERR_NODEV` (19).
    NoDev,
    /// `NFS3ERR_NOTDIR` (20).
    NotDir,
    /// `NFS3ERR_ISDIR` (21).
    IsDir,
    /// `NFS3ERR_INVAL` (22).
    Inval,
    /// `NFS3ERR_FBIG` (27).
    FBig,
    /// `NFS3ERR_NOSPC` (28).
    NoSpc,
    /// `NFS3ERR_ROFS` (30).
    Rofs,
    /// `NFS3ERR_MLINK` (31).
    MLink,
    /// `NFS3ERR_NAMETOOLONG` (63).
    NameTooLong,
    /// `NFS3ERR_NOTEMPTY` (66).
    NotEmpty,
    /// `NFS3ERR_DQUOT` (69).
    DQuot,
    /// `NFS3ERR_STALE` (70): the handle no longer names anything.
    Stale,
    /// `NFS3ERR_REMOTE` (71).
    Remote,
    /// `NFS3ERR_BADHANDLE` (10001).
    BadHandle,
    /// `NFS3ERR_NOT_SYNC` (10002).
    NotSync,
    /// `NFS3ERR_BAD_COOKIE` (10003): a listing must restart.
    BadCookie,
    /// `NFS3ERR_NOTSUPP` (10004).
    NotSupp,
    /// `NFS3ERR_TOOSMALL` (10005).
    TooSmall,
    /// `NFS3ERR_SERVERFAULT` (10006).
    ServerFault,
    /// `NFS3ERR_BADTYPE` (10007).
    BadType,
    /// `NFS3ERR_JUKEBOX` (10008): try again later.
    Jukebox,
    /// A number RFC 1813 doesn't define.
    Unknown(u32),
}

impl NfsStat3 {
    /// The status for a number off the wire.
    pub(crate) fn from_number(number: u32) -> Self {
        match number {
            0 => Self::Ok,
            1 => Self::Perm,
            2 => Self::NoEnt,
            5 => Self::Io,
            6 => Self::Nxio,
            13 => Self::Access,
            17 => Self::Exist,
            18 => Self::Xdev,
            19 => Self::NoDev,
            20 => Self::NotDir,
            21 => Self::IsDir,
            22 => Self::Inval,
            27 => Self::FBig,
            28 => Self::NoSpc,
            30 => Self::Rofs,
            31 => Self::MLink,
            63 => Self::NameTooLong,
            66 => Self::NotEmpty,
            69 => Self::DQuot,
            70 => Self::Stale,
            71 => Self::Remote,
            10001 => Self::BadHandle,
            10002 => Self::NotSync,
            10003 => Self::BadCookie,
            10004 => Self::NotSupp,
            10005 => Self::TooSmall,
            10006 => Self::ServerFault,
            10007 => Self::BadType,
            10008 => Self::Jukebox,
            other => Self::Unknown(other),
        }
    }
}

/// Why an NFS procedure didn't answer what was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NfsError {
    /// The server answered with this status.
    Status(NfsStat3),
    /// The call itself failed: transport, RPC refusal, or a reply that
    /// wouldn't decode.
    Call(CallError),
    /// A listing page came back with no entries and no end: following it would
    /// ask for the same page forever.
    NoProgress,
}

impl From<CallError> for NfsError {
    fn from(err: CallError) -> Self {
        Self::Call(err)
    }
}

impl From<XdrError> for NfsError {
    fn from(err: XdrError) -> Self {
        Self::Call(CallError::Decode(err))
    }
}

/// `ftype3`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileType {
    /// `NF3REG`.
    Regular,
    /// `NF3DIR`.
    Directory,
    /// `NF3BLK`.
    BlockDevice,
    /// `NF3CHR`.
    CharDevice,
    /// `NF3LNK`.
    Symlink,
    /// `NF3SOCK`.
    Socket,
    /// `NF3FIFO`.
    Fifo,
}

/// `nfstime3`: seconds and nanoseconds since the Unix epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) struct NfsTime {
    /// Whole seconds.
    pub(crate) seconds: u32,
    /// The nanoseconds past them.
    pub(crate) nseconds: u32,
}

pub(super) fn decode_time(dec: &mut Decoder) -> Result<NfsTime, XdrError> {
    Ok(NfsTime {
        seconds: dec.u32()?,
        nseconds: dec.u32()?,
    })
}

pub(super) fn encode_time(enc: &mut Encoder, time: NfsTime) {
    enc.u32(time.seconds);
    enc.u32(time.nseconds);
}

/// `fattr3`: a file's attributes. ❗ Never follows a link: a symlink's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Fattr3 {
    /// The kind of file.
    pub(crate) file_type: FileType,
    /// The permission bits (and setuid, setgid, sticky).
    pub(crate) mode: u32,
    /// Hard links.
    pub(crate) nlink: u32,
    /// The owner.
    pub(crate) uid: u32,
    /// The group.
    pub(crate) gid: u32,
    /// The length in bytes.
    pub(crate) size: u64,
    /// The bytes it occupies on disk.
    pub(crate) used: u64,
    /// A device's major and minor numbers.
    pub(crate) rdev: (u32, u32),
    /// The file system it's on.
    pub(crate) fsid: u64,
    /// Its number on that file system.
    pub(crate) fileid: u64,
    /// Last read.
    pub(crate) atime: NfsTime,
    /// Last written.
    pub(crate) mtime: NfsTime,
    /// Last changed in any way, attributes included.
    pub(crate) ctime: NfsTime,
}

/// `fattr3` off the wire.
pub(crate) fn decode_fattr(dec: &mut Decoder) -> Result<Fattr3, XdrError> {
    let file_type = match dec.u32()? {
        1 => FileType::Regular,
        2 => FileType::Directory,
        3 => FileType::BlockDevice,
        4 => FileType::CharDevice,
        5 => FileType::Symlink,
        6 => FileType::Socket,
        7 => FileType::Fifo,
        value => return Err(XdrError::BadDiscriminant { what: "ftype3", value }),
    };
    Ok(Fattr3 {
        file_type,
        mode: dec.u32()?,
        nlink: dec.u32()?,
        uid: dec.u32()?,
        gid: dec.u32()?,
        size: dec.u64()?,
        used: dec.u64()?,
        rdev: (dec.u32()?, dec.u32()?),
        fsid: dec.u64()?,
        fileid: dec.u64()?,
        atime: decode_time(dec)?,
        mtime: decode_time(dec)?,
        ctime: decode_time(dec)?,
    })
}

pub(super) fn decode_post_op_attr(dec: &mut Decoder) -> Result<Option<Fattr3>, XdrError> {
    dec.optional(decode_fattr)
}

/// `wcc_attr`: the attributes a mutation saw before it ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WccAttr {
    /// The length.
    pub(crate) size: u64,
    /// Last written.
    pub(crate) mtime: NfsTime,
    /// Last changed.
    pub(crate) ctime: NfsTime,
}

/// `wcc_data`: an object's attributes either side of a mutation, each optional.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct WccData {
    /// Before the mutation.
    pub(crate) before: Option<WccAttr>,
    /// After it.
    pub(crate) after: Option<Fattr3>,
}

pub(super) fn decode_wcc(dec: &mut Decoder) -> Result<WccData, XdrError> {
    let before = dec.optional(|dec| {
        Ok(WccAttr {
            size: dec.u64()?,
            mtime: decode_time(dec)?,
            ctime: decode_time(dec)?,
        })
    })?;
    Ok(WccData {
        before,
        after: decode_post_op_attr(dec)?,
    })
}

/// How `sattr3` sets a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SetTime {
    /// Leave it.
    #[default]
    DontChange,
    /// The server's clock, now.
    ServerTime,
    /// This value.
    ClientTime(NfsTime),
}

/// `sattr3`: the attributes a SETATTR, CREATE, or MKDIR sets. `None` leaves one
/// as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Sattr3 {
    /// The permission bits.
    pub(crate) mode: Option<u32>,
    /// The owner.
    pub(crate) uid: Option<u32>,
    /// The group.
    pub(crate) gid: Option<u32>,
    /// The length (truncating or extending).
    pub(crate) size: Option<u64>,
    /// Last read.
    pub(crate) atime: SetTime,
    /// Last written.
    pub(crate) mtime: SetTime,
}

pub(super) fn encode_sattr(enc: &mut Encoder, sattr: &Sattr3) {
    for value in [sattr.mode, sattr.uid, sattr.gid] {
        enc.bool(value.is_some());
        if let Some(value) = value {
            enc.u32(value);
        }
    }
    enc.bool(sattr.size.is_some());
    if let Some(size) = sattr.size {
        enc.u64(size);
    }
    for time in [sattr.atime, sattr.mtime] {
        match time {
            SetTime::DontChange => enc.u32(0),
            SetTime::ServerTime => enc.u32(1),
            SetTime::ClientTime(time) => {
                enc.u32(2);
                encode_time(enc, time);
            }
        }
    }
}

/// `stable_how`: how durable a WRITE must be before it's answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Stable {
    /// May sit in the server's cache until a COMMIT.
    Unstable,
    /// The data is on disk; metadata may not be.
    DataSync,
    /// Data and metadata are on disk.
    FileSync,
}

impl Stable {
    pub(super) fn number(self) -> u32 {
        match self {
            Self::Unstable => 0,
            Self::DataSync => 1,
            Self::FileSync => 2,
        }
    }

    fn decode(dec: &mut Decoder) -> Result<Self, XdrError> {
        match dec.u32()? {
            0 => Ok(Self::Unstable),
            1 => Ok(Self::DataSync),
            2 => Ok(Self::FileSync),
            value => Err(XdrError::BadDiscriminant {
                what: "stable_how",
                value,
            }),
        }
    }
}

/// `writeverf3`: changes when the server reboots and loses uncommitted data.
pub(crate) type WriteVerifier = [u8; 8];

/// `createhow3`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CreateHow {
    /// Create, or open what's there (truncating if `size` says so).
    Unchecked(Sattr3),
    /// Create, and refuse with `EXIST` when the name is taken: the atomic
    /// claim.
    Guarded(Sattr3),
    /// Create with this verifier, so a resend recognizes its own file.
    Exclusive([u8; 8]),
}

/// LOOKUP's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LookupOk {
    /// The handle the name leads to.
    pub(crate) handle: FileHandle,
    /// Its attributes, when the server sent them.
    pub(crate) attributes: Option<Fattr3>,
    /// The directory's attributes, when the server sent them.
    pub(crate) dir_attributes: Option<Fattr3>,
}

/// ACCESS's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AccessOk {
    /// The object's attributes, when the server sent them.
    pub(crate) attributes: Option<Fattr3>,
    /// The subset of the asked bits the server grants.
    pub(crate) granted: u32,
}

/// The ACCESS bits (RFC 1813 § 3.3.4).
pub(crate) mod access {
    /// Read a file, or list a directory.
    pub(crate) const READ: u32 = 0x0001;
    /// Look up a name in a directory.
    pub(crate) const LOOKUP: u32 = 0x0002;
    /// Rewrite a file's data, or a directory's entries.
    pub(crate) const MODIFY: u32 = 0x0004;
    /// Write past a file's end, or add to a directory.
    pub(crate) const EXTEND: u32 = 0x0008;
    /// Remove a directory entry.
    pub(crate) const DELETE: u32 = 0x0010;
    /// Run a file.
    pub(crate) const EXECUTE: u32 = 0x0020;
}

/// READ's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReadOk {
    /// The file's attributes after the read, when the server sent them.
    pub(crate) attributes: Option<Fattr3>,
    /// The bytes, a slice of the reply record. ❗ May be shorter than asked
    /// with `eof` false: re-ask at the new offset.
    pub(crate) data: Bytes,
    /// The read reached the end of the file.
    pub(crate) eof: bool,
}

/// WRITE's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WriteOk {
    /// The file's attributes either side.
    pub(crate) wcc: WccData,
    /// Bytes written. ❗ May be fewer than sent: resend from the answered offset.
    pub(crate) count: u32,
    /// How durable they are.
    pub(crate) committed: Stable,
    /// The server's verifier, to compare with COMMIT's.
    pub(crate) verifier: WriteVerifier,
}

/// CREATE's and MKDIR's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CreatedOk {
    /// The new object's handle, when the server sent it (else LOOKUP it).
    pub(crate) handle: Option<FileHandle>,
    /// Its attributes, when the server sent them.
    pub(crate) attributes: Option<Fattr3>,
    /// The parent directory either side.
    pub(crate) dir_wcc: WccData,
}

/// RENAME's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RenameOk {
    /// The source directory either side.
    pub(crate) from_dir_wcc: WccData,
    /// The target directory either side.
    pub(crate) to_dir_wcc: WccData,
}

/// COMMIT's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CommitOk {
    /// The file either side.
    pub(crate) wcc: WccData,
    /// The server's verifier: equal to every covered WRITE's, or data was lost.
    pub(crate) verifier: WriteVerifier,
}

/// One READDIR entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DirEntry {
    /// The file's number.
    pub(crate) fileid: u64,
    /// Its name, as the server's bytes.
    pub(crate) name: Bytes,
}

/// One READDIRPLUS entry. The handle and attributes are each optional.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DirEntryPlus {
    /// The file's number.
    pub(crate) fileid: u64,
    /// Its name, as the server's bytes.
    pub(crate) name: Bytes,
    /// Its attributes, when the server sent them.
    pub(crate) attributes: Option<Fattr3>,
    /// Its handle, when the server sent it.
    pub(crate) handle: Option<FileHandle>,
}

/// One page of a READDIR or READDIRPLUS listing. ❗ `.` and `..` are already
/// dropped, but `next_cookie` still counts them: it's the last entry's cookie,
/// whatever its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DirPage<E> {
    /// The directory's attributes, when the server sent them.
    pub(crate) dir_attributes: Option<Fattr3>,
    /// The verifier the next page must echo.
    pub(crate) cookieverf: [u8; 8],
    /// The entries, minus `.` and `..`.
    pub(crate) entries: Vec<E>,
    /// Where the next page starts, or `None` when this page listed nothing.
    pub(crate) next_cookie: Option<u64>,
    /// The listing ended with this page.
    pub(crate) eof: bool,
}

/// FSSTAT's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FsStat {
    /// The file system's size in bytes.
    pub(crate) total_bytes: u64,
    /// Free bytes.
    pub(crate) free_bytes: u64,
    /// Free bytes this user may use.
    pub(crate) available_bytes: u64,
    /// Total file slots.
    pub(crate) total_files: u64,
    /// Free file slots.
    pub(crate) free_files: u64,
    /// Free file slots this user may use.
    pub(crate) available_files: u64,
}

/// FSINFO's answer: the server's preferred and largest transfer sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FsInfo {
    /// The largest READ the server takes.
    pub(crate) rtmax: u32,
    /// The READ size it prefers.
    pub(crate) rtpref: u32,
    /// The largest WRITE it takes.
    pub(crate) wtmax: u32,
    /// The WRITE size it prefers.
    pub(crate) wtpref: u32,
    /// The READDIR size it prefers.
    pub(crate) dtpref: u32,
    /// The largest file it stores.
    pub(crate) maxfilesize: u64,
    /// Its clock's granularity.
    pub(crate) time_delta: NfsTime,
    /// `FSF3_*` bits: links, symlinks, homogeneous, settable times.
    pub(crate) properties: u32,
}

/// PATHCONF's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PathConf {
    /// The most hard links one file may have.
    pub(crate) link_max: u32,
    /// The longest name, in bytes.
    pub(crate) name_max: u32,
    /// A name past `name_max` is refused, ❌ not cut.
    pub(crate) no_trunc: bool,
    /// Only a privileged user may change an owner.
    pub(crate) chown_restricted: bool,
    /// Names compare without case.
    pub(crate) case_insensitive: bool,
    /// Names keep the case they were made with.
    pub(crate) case_preserving: bool,
}

/// `diropargs3`: a directory and a name in it.
pub(super) fn encode_dirop(enc: &mut Encoder, dir: &FileHandle, name: &[u8]) {
    encode_handle(enc, dir);
    enc.opaque(name);
}

pub(super) fn is_dot_or_dot_dot(name: &[u8]) -> bool {
    name == b"." || name == b".."
}

/// GETATTR's success arm.
pub(crate) fn decode_getattr(dec: &mut Decoder) -> Result<Fattr3, XdrError> {
    decode_fattr(dec)
}

/// LOOKUP's success arm.
pub(crate) fn decode_lookup(dec: &mut Decoder) -> Result<LookupOk, XdrError> {
    Ok(LookupOk {
        handle: decode_handle(dec)?,
        attributes: decode_post_op_attr(dec)?,
        dir_attributes: decode_post_op_attr(dec)?,
    })
}

/// READ's success arm. `asked` caps the data: a server answering more than it
/// was asked for is broken.
pub(crate) fn decode_read(dec: &mut Decoder, asked: u32) -> Result<ReadOk, XdrError> {
    let attributes = decode_post_op_attr(dec)?;
    let _count = dec.u32()?;
    let eof = dec.bool()?;
    let data = dec.opaque(asked as usize)?;
    Ok(ReadOk { attributes, data, eof })
}

/// WRITE's success arm.
pub(crate) fn decode_write(dec: &mut Decoder) -> Result<WriteOk, XdrError> {
    Ok(WriteOk {
        wcc: decode_wcc(dec)?,
        count: dec.u32()?,
        committed: Stable::decode(dec)?,
        verifier: dec.fixed_array()?,
    })
}

/// CREATE's and MKDIR's success arm.
pub(crate) fn decode_created(dec: &mut Decoder) -> Result<CreatedOk, XdrError> {
    Ok(CreatedOk {
        handle: dec.optional(decode_handle)?,
        attributes: decode_post_op_attr(dec)?,
        dir_wcc: decode_wcc(dec)?,
    })
}

/// READDIR's success arm.
pub(crate) fn decode_readdir(dec: &mut Decoder) -> Result<DirPage<DirEntry>, XdrError> {
    let dir_attributes = decode_post_op_attr(dec)?;
    let cookieverf = dec.fixed_array()?;
    let mut entries = Vec::new();
    let mut next_cookie = None;
    while dec.bool()? {
        let fileid = dec.u64()?;
        let name = dec.opaque(MAX_NAME)?;
        next_cookie = Some(dec.u64()?);
        if !is_dot_or_dot_dot(&name) {
            entries.push(DirEntry { fileid, name });
        }
    }
    Ok(DirPage {
        dir_attributes,
        cookieverf,
        entries,
        next_cookie,
        eof: dec.bool()?,
    })
}

/// READDIRPLUS's success arm.
pub(crate) fn decode_readdirplus(dec: &mut Decoder) -> Result<DirPage<DirEntryPlus>, XdrError> {
    let dir_attributes = decode_post_op_attr(dec)?;
    let cookieverf = dec.fixed_array()?;
    let mut entries = Vec::new();
    let mut next_cookie = None;
    while dec.bool()? {
        let fileid = dec.u64()?;
        let name = dec.opaque(MAX_NAME)?;
        next_cookie = Some(dec.u64()?);
        let attributes = decode_post_op_attr(dec)?;
        let handle = dec.optional(decode_handle)?;
        if !is_dot_or_dot_dot(&name) {
            entries.push(DirEntryPlus {
                fileid,
                name,
                attributes,
                handle,
            });
        }
    }
    Ok(DirPage {
        dir_attributes,
        cookieverf,
        entries,
        next_cookie,
        eof: dec.bool()?,
    })
}

/// FSSTAT's success arm.
pub(crate) fn decode_fsstat(dec: &mut Decoder) -> Result<FsStat, XdrError> {
    let _attributes = decode_post_op_attr(dec)?;
    let stat = FsStat {
        total_bytes: dec.u64()?,
        free_bytes: dec.u64()?,
        available_bytes: dec.u64()?,
        total_files: dec.u64()?,
        free_files: dec.u64()?,
        available_files: dec.u64()?,
    };
    let _invarsec = dec.u32()?;
    Ok(stat)
}

/// FSINFO's success arm.
pub(crate) fn decode_fsinfo(dec: &mut Decoder) -> Result<FsInfo, XdrError> {
    let _attributes = decode_post_op_attr(dec)?;
    let rtmax = dec.u32()?;
    let rtpref = dec.u32()?;
    let _rtmult = dec.u32()?;
    let wtmax = dec.u32()?;
    let wtpref = dec.u32()?;
    let _wtmult = dec.u32()?;
    Ok(FsInfo {
        rtmax,
        rtpref,
        wtmax,
        wtpref,
        dtpref: dec.u32()?,
        maxfilesize: dec.u64()?,
        time_delta: decode_time(dec)?,
        properties: dec.u32()?,
    })
}

/// PATHCONF's success arm.
pub(crate) fn decode_pathconf(dec: &mut Decoder) -> Result<PathConf, XdrError> {
    let _attributes = decode_post_op_attr(dec)?;
    Ok(PathConf {
        link_max: dec.u32()?,
        name_max: dec.u32()?,
        no_trunc: dec.bool()?,
        chown_restricted: dec.bool()?,
        case_insensitive: dec.bool()?,
        case_preserving: dec.bool()?,
    })
}

/// COMMIT's success arm.
pub(crate) fn decode_commit(dec: &mut Decoder) -> Result<CommitOk, XdrError> {
    Ok(CommitOk {
        wcc: decode_wcc(dec)?,
        verifier: dec.fixed_array()?,
    })
}

/// Reads the `nfsstat3` a result opens with: the success arm's decoder, or the
/// status. A failure arm's body is left undecoded.
pub(crate) fn status_of(mut dec: Decoder) -> Result<Result<Decoder, NfsStat3>, XdrError> {
    match NfsStat3::from_number(dec.u32()?) {
        NfsStat3::Ok => Ok(Ok(dec)),
        status => Ok(Err(status)),
    }
}

#[cfg(test)]
#[path = "types_test.rs"]
mod types_test;
