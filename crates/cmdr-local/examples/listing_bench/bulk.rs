//! The prototype `getattrlistbulk` lister, plus the two per-path probes the
//! benchmark prices against it (`getattrlist` for the added date, `getxattr` for
//! Finder's last-used date). Benchmark-only code: nothing here ships.

use std::ffi::{CString, OsStr};
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use cmdr_fs::entry::{FileEntry, get_group_name, get_owner_name, is_hidden_by_name};
use cmdr_fs::volume::host::listings::ListingHost;
use cmdr_local::reading;

/// `ATTR_CMN_ERROR` from `<sys/attr.h>`; `libc` doesn't carry it.
const ATTR_CMN_ERROR: u32 = 0x2000_0000;

/// `SF_FIRMLINK` from `<sys/stat.h>`: the `st_flags` bit a firmlink carries.
const SF_FIRMLINK: u32 = 0x0080_0000;

/// How many entries the lister re-stat'ed because they were mount points or
/// firmlinks (reported by `parity`).
pub static FALLBACKS: AtomicUsize = AtomicUsize::new(0);

/// `enum vtype` from `<sys/vnode.h>`.
const VREG: u32 = 1;
const VDIR: u32 = 2;
const VBLK: u32 = 3;
const VCHR: u32 = 4;
const VLNK: u32 = 5;
const VSOCK: u32 = 6;
const VFIFO: u32 = 7;

/// `u32` length + `attribute_set_t` (five `u32` bitmaps).
const ENTRY_HEADER_LEN: usize = 24;

/// The Finder "Last opened" xattr. 16 bytes: a little-endian `struct timespec`
/// (`i64` seconds, `i64` nanoseconds).
pub const LAST_USED_XATTR: &[u8] = b"com.apple.lastuseddate#PS\0";

#[derive(Clone, Copy)]
pub struct BulkOptions {
    pub added_time: bool,
    pub last_used_xattr: bool,
    pub buf_bytes: usize,
}

/// Lists `path` with `getattrlistbulk`, building the same `FileEntry` the
/// current `list_directory_core` builds, plus `added_at` (and `opened_at` from
/// the xattr when asked).
pub fn list(path: &Path, listings: &dyn ListingHost, opts: BulkOptions) -> io::Result<Vec<FileEntry>> {
    let c_path = CString::new(path.as_os_str().as_bytes())?;
    // SAFETY: `c_path` is a valid NUL-terminated string for the call's duration.
    let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let result = list_fd(fd, path, opts);
    // SAFETY: `fd` is the descriptor `open` returned above, closed exactly once.
    unsafe { libc::close(fd) };
    let mut entries = result?;
    listings.sort_listing(&mut entries);
    Ok(entries)
}

fn attr_list(opts: BulkOptions) -> libc::attrlist {
    let mut common = libc::ATTR_CMN_RETURNED_ATTRS
        | ATTR_CMN_ERROR
        | libc::ATTR_CMN_NAME
        | libc::ATTR_CMN_OBJTYPE
        | libc::ATTR_CMN_CRTIME
        | libc::ATTR_CMN_MODTIME
        | libc::ATTR_CMN_OWNERID
        | libc::ATTR_CMN_GRPID
        | libc::ATTR_CMN_ACCESSMASK
        | libc::ATTR_CMN_FLAGS
        | libc::ATTR_CMN_FILEID;
    if opts.added_time {
        common |= libc::ATTR_CMN_ADDEDTIME;
    }
    libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: common,
        volattr: 0,
        dirattr: libc::ATTR_DIR_MOUNTSTATUS,
        fileattr: libc::ATTR_FILE_LINKCOUNT | libc::ATTR_FILE_DATALENGTH | libc::ATTR_FILE_ALLOCSIZE,
        forkattr: 0,
    }
}

/// Prints the returned-attribute bitmaps and raw bytes of the first few
/// entries, for debugging a filesystem whose packing surprises the parser.
pub fn dump(path: &Path, max: usize) -> io::Result<()> {
    let file = fs::File::open(path)?;
    let mut al = attr_list(BulkOptions {
        added_time: true,
        last_used_xattr: false,
        buf_bytes: 0,
    });
    let mut buf = vec![0u64; 8192];
    // SAFETY: `al` is a valid `attrlist`, `buf` is writable for 64 KiB.
    let n = unsafe {
        libc::getattrlistbulk(
            std::os::fd::AsRawFd::as_raw_fd(&file),
            (&raw mut al).cast(),
            buf.as_mut_ptr().cast(),
            65536,
            u64::from(libc::FSOPT_PACK_INVAL_ATTRS),
        )
    };
    // SAFETY: viewing the initialized buffer as bytes.
    let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), 65536) };
    let mut off = 0;
    for _ in 0..(n.max(0) as usize).min(max) {
        let len = read_u32(bytes, off)? as usize;
        let e = &bytes[off..off + len];
        println!(
            "len={len} common={:#010x} dir={:#x} file={:#x}",
            read_u32(e, 4)?,
            read_u32(e, 12)?,
            read_u32(e, 16)?
        );
        let hex: Vec<String> = e
            .chunks(4)
            .map(|c| c.iter().map(|b| format!("{b:02x}")).collect())
            .collect();
        println!("  {}", hex.join(" "));
        off += len;
    }
    Ok(())
}

fn list_fd(fd: libc::c_int, dir: &Path, opts: BulkOptions) -> io::Result<Vec<FileEntry>> {
    let mut al = attr_list(opts);
    // `u64` backing only so the allocation is 8-aligned; every read below is
    // unaligned anyway, since the kernel packs to 4-byte boundaries.
    let mut buf = vec![0u64; opts.buf_bytes / 8];
    let buf_len = buf.len() * 8;
    let mut entries = Vec::new();
    loop {
        // SAFETY: `al` is a valid `attrlist`, and `buf` is writable for `buf_len` bytes.
        let n = unsafe {
            libc::getattrlistbulk(
                fd,
                (&raw mut al).cast(),
                buf.as_mut_ptr().cast(),
                buf_len,
                u64::from(libc::FSOPT_PACK_INVAL_ATTRS),
            )
        };
        if n < 0 {
            let err = io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            return Err(err);
        }
        if n == 0 {
            break;
        }
        // SAFETY: the kernel wrote up to `buf_len` bytes into `buf`; viewing them as bytes is sound.
        let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), buf_len) };
        let mut offset = 0usize;
        for _ in 0..n {
            let len = read_u32(bytes, offset)? as usize;
            if len < ENTRY_HEADER_LEN || offset + len > bytes.len() {
                return Err(io::Error::other(format!("implausible entry length {len} at {offset}")));
            }
            entries.push(parse_entry(&bytes[offset..offset + len], dir, opts)?);
            offset += len;
        }
    }
    Ok(entries)
}

/// One decoded entry, before it becomes a `FileEntry`.
struct Raw<'a> {
    error: u32,
    name: &'a [u8],
    obj_type: Option<u32>,
    crtime: Option<i64>,
    modtime: Option<i64>,
    uid: u32,
    gid: u32,
    mask: u32,
    flags: u32,
    file_id: u64,
    added: Option<i64>,
    mount_status: u32,
    link_count: u32,
    data_len: u64,
    alloc_size: u64,
}

fn parse_entry(e: &[u8], dir: &Path, opts: BulkOptions) -> io::Result<FileEntry> {
    let common = read_u32(e, 4)?;
    let mut cur = ENTRY_HEADER_LEN;
    let mut raw = Raw {
        error: 0,
        name: &[],
        obj_type: None,
        crtime: None,
        modtime: None,
        uid: 0,
        gid: 0,
        mask: 0,
        flags: 0,
        file_id: 0,
        added: None,
        mount_status: 0,
        link_count: 0,
        data_len: 0,
        alloc_size: 0,
    };
    // `ATTR_CMN_ERROR` is packed straight after the returned-attributes set,
    // ahead of the name (getattrlistbulk(2)); everything else in bitmap order.
    raw.error = take_u32(e, &mut cur)?;
    {
        let ref_pos = cur;
        let off = read_u32(e, cur)? as i32;
        let len = read_u32(e, cur + 4)? as usize;
        cur += 8;
        let start = usize::try_from(ref_pos as i64 + i64::from(off)).map_err(io::Error::other)?;
        // `attr_length` counts the trailing NUL.
        let end = start + len.saturating_sub(1);
        raw.name = e
            .get(start..end)
            .ok_or_else(|| io::Error::other("name runs past entry"))?;
    }
    if raw.error != 0 {
        // What else an errored entry packs isn't specified; the name is all we use.
        return Ok(to_file_entry(&raw, dir, opts));
    }
    // With FSOPT_PACK_INVAL_ATTRS every REQUESTED attribute is packed, an
    // invalid one as zeros with its returned bit cleared. So the cursor walks
    // the requested set, and the returned bit only says whether the value is
    // real. (Skipping by the returned set misreads every field after an
    // unsupported one: ADDEDTIME on SMB/exFAT/FAT shifted sizes by 16 bytes.)
    let valid = |bit: u32| common & bit != 0;
    let obj_type = take_u32(e, &mut cur)?;
    raw.obj_type = valid(libc::ATTR_CMN_OBJTYPE).then_some(obj_type);
    let crtime = take_timespec_secs(e, &mut cur)?;
    raw.crtime = valid(libc::ATTR_CMN_CRTIME).then_some(crtime);
    let modtime = take_timespec_secs(e, &mut cur)?;
    raw.modtime = valid(libc::ATTR_CMN_MODTIME).then_some(modtime);
    raw.uid = take_u32(e, &mut cur)?;
    raw.gid = take_u32(e, &mut cur)?;
    raw.mask = take_u32(e, &mut cur)?;
    raw.flags = take_u32(e, &mut cur)?;
    raw.file_id = take_u64(e, &mut cur)?;
    if opts.added_time {
        let added = take_timespec_secs(e, &mut cur)?;
        raw.added = valid(libc::ATTR_CMN_ADDEDTIME).then_some(added);
    }
    // The dir group is packed for directories only, the file group for
    // everything else (verified on APFS and smbfs: a file entry carries no
    // mount-status bytes, a dir entry no link count or sizes).
    if obj_type == VDIR {
        raw.mount_status = take_u32(e, &mut cur)?;
    } else {
        raw.link_count = take_u32(e, &mut cur)?;
        // Bitmap order: ALLOCSIZE (0x4) packs before DATALENGTH (0x200).
        raw.alloc_size = take_u64(e, &mut cur)?;
        raw.data_len = take_u64(e, &mut cur)?;
    }
    Ok(to_file_entry(&raw, dir, opts))
}

fn to_file_entry(raw: &Raw<'_>, dir: &Path, opts: BulkOptions) -> FileEntry {
    let name = OsStr::from_bytes(raw.name).to_string_lossy().to_string();
    let full_path = dir.join(OsStr::from_bytes(raw.name));
    let path_str = full_path.to_string_lossy().to_string();
    let obj_type = raw.obj_type.unwrap_or(0);
    let is_symlink = obj_type == VLNK;

    if raw.error != 0 || raw.obj_type.is_none() {
        // The current path's "couldn't stat it" shape.
        return FileEntry {
            icon_id: if is_symlink { "symlink-broken" } else { "file" }.to_string(),
            extended_metadata_loaded: true,
            ..FileEntry::new(name, path_str, false, is_symlink)
        };
    }

    let secs = |t: Option<i64>| t.and_then(|s| u64::try_from(s).ok());

    // A mount point or a firmlink comes back with the attributes of the
    // directory it COVERS (the stub on the system volume), while `lstat`
    // reports what's mounted / linked there. Re-stat just those, the way the
    // current lister does every entry.
    if obj_type == VDIR && (raw.mount_status & libc::DIR_MNTSTATUS_MNTPOINT != 0 || raw.flags & SF_FIRMLINK != 0) {
        FALLBACKS.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut entry) = reading::get_single_entry(&full_path) {
            entry.added_at = if opts.added_time { secs(raw.added) } else { None };
            return entry;
        }
    }

    let target_is_dir = is_symlink && fs::metadata(&full_path).map(|m| m.is_dir()).unwrap_or(false);
    let is_dir = obj_type == VDIR || target_is_dir;
    let is_file = obj_type == VREG;

    let opened_at = if opts.last_used_xattr && is_file {
        read_last_used(&full_path)
    } else {
        None
    };

    FileEntry {
        size: is_file.then_some(raw.data_len),
        physical_size: is_file.then_some(raw.alloc_size),
        inode: (is_file && raw.link_count > 1).then_some(raw.file_id),
        modified_at: secs(raw.modtime),
        created_at: secs(raw.crtime),
        added_at: if opts.added_time { secs(raw.added) } else { None },
        opened_at,
        permissions: (raw.mask & 0o7777) | type_bits(obj_type),
        owner: get_owner_name(raw.uid),
        group: get_group_name(raw.gid),
        is_hidden: is_hidden_by_name(&name) || raw.flags & libc::UF_HIDDEN != 0,
        ..FileEntry::new(name, path_str, is_dir, is_symlink)
    }
}

/// The `S_IFMT` bits `st_mode` carries for an object type, so `permissions`
/// matches what `Metadata::permissions().mode()` returns.
fn type_bits(obj_type: u32) -> u32 {
    u32::from(match obj_type {
        VREG => libc::S_IFREG,
        VDIR => libc::S_IFDIR,
        VLNK => libc::S_IFLNK,
        VBLK => libc::S_IFBLK,
        VCHR => libc::S_IFCHR,
        VSOCK => libc::S_IFSOCK,
        VFIFO => libc::S_IFIFO,
        _ => 0,
    })
}

/// Reads `com.apple.lastuseddate#PS` without following symlinks. `None` when
/// absent (`ENOATTR`) or malformed.
pub fn read_last_used(path: &Path) -> Option<u64> {
    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut buf = [0u8; 16];
    // SAFETY: both strings are NUL-terminated; `buf` is writable for 16 bytes.
    let n = unsafe {
        libc::getxattr(
            c_path.as_ptr(),
            LAST_USED_XATTR.as_ptr().cast(),
            buf.as_mut_ptr().cast(),
            buf.len(),
            0,
            libc::XATTR_NOFOLLOW,
        )
    };
    if n != 16 {
        return None;
    }
    u64::try_from(i64::from_le_bytes(buf[..8].try_into().ok()?)).ok()
}

/// One `getattrlist(ATTR_CMN_ADDEDTIME)` by path, without following symlinks:
/// the cheap add-on to the current lister.
pub fn added_time_by_path(path: &Path) -> Option<u64> {
    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut al = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: libc::ATTR_CMN_RETURNED_ATTRS | libc::ATTR_CMN_ADDEDTIME,
        volattr: 0,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    let mut buf = [0u64; 8];
    // SAFETY: `al` is a valid `attrlist`, `buf` is writable for 64 bytes.
    let rc = unsafe {
        libc::getattrlist(
            c_path.as_ptr(),
            (&raw mut al).cast(),
            buf.as_mut_ptr().cast(),
            64,
            libc::FSOPT_NOFOLLOW | libc::FSOPT_PACK_INVAL_ATTRS,
        )
    };
    if rc != 0 {
        return None;
    }
    // SAFETY: viewing the 64 initialized bytes of `buf` as bytes.
    let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), 64) };
    let common = read_u32(bytes, 4).ok()?;
    if common & libc::ATTR_CMN_ADDEDTIME == 0 {
        return None;
    }
    let mut cur = ENTRY_HEADER_LEN;
    take_timespec_secs(bytes, &mut cur)
        .ok()
        .and_then(|s| u64::try_from(s).ok())
}

/// What a volume says about the attributes this lister relies on: whether
/// `getattrlist` is native (`VOL_CAP_INT_ATTRLIST`), and whether
/// `ATTR_CMN_ADDEDTIME` is valid / native per `ATTR_VOL_ATTRIBUTES`.
pub struct VolumeAttrSupport {
    pub attrlist_native: bool,
    pub addedtime_valid: bool,
    pub addedtime_native: bool,
    pub flags_valid: bool,
}

pub fn volume_attr_support(path: &Path) -> io::Result<VolumeAttrSupport> {
    let c_path = CString::new(path.as_os_str().as_bytes())?;
    let mut al = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: 0,
        volattr: libc::ATTR_VOL_INFO | libc::ATTR_VOL_CAPABILITIES | libc::ATTR_VOL_ATTRIBUTES,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    let mut buf = [0u64; 64];
    // SAFETY: `al` is a valid `attrlist`, `buf` is writable for 512 bytes.
    let rc = unsafe { libc::getattrlist(c_path.as_ptr(), (&raw mut al).cast(), buf.as_mut_ptr().cast(), 512, 0) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: viewing the 512 initialized bytes of `buf` as bytes.
    let b = unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), 512) };
    // u32 length, then vol_capabilities_attr_t (capabilities[4] + valid[4], u32 each),
    // then vol_attributes_attr_t (validattr + nativeattr, attribute_set_t each).
    let caps_off = 4;
    let interfaces_valid = read_u32(b, caps_off + 16 + 4 * libc::VOL_CAPABILITIES_INTERFACES)?;
    let interfaces = read_u32(b, caps_off + 4 * libc::VOL_CAPABILITIES_INTERFACES)?;
    let attrs_off = caps_off + 32;
    let valid_common = read_u32(b, attrs_off)?;
    let native_common = read_u32(b, attrs_off + 20)?;
    Ok(VolumeAttrSupport {
        attrlist_native: interfaces_valid & libc::VOL_CAP_INT_ATTRLIST != 0
            && interfaces & libc::VOL_CAP_INT_ATTRLIST != 0,
        addedtime_valid: valid_common & libc::ATTR_CMN_ADDEDTIME != 0,
        addedtime_native: native_common & libc::ATTR_CMN_ADDEDTIME != 0,
        flags_valid: valid_common & libc::ATTR_CMN_FLAGS != 0,
    })
}

fn read_u32(b: &[u8], at: usize) -> io::Result<u32> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes(s.try_into().expect("4 bytes")))
        .ok_or_else(|| io::Error::other("truncated u32"))
}

fn take_u32(b: &[u8], cur: &mut usize) -> io::Result<u32> {
    let v = read_u32(b, *cur)?;
    *cur += 4;
    Ok(v)
}

fn take_u64(b: &[u8], cur: &mut usize) -> io::Result<u64> {
    let v = b
        .get(*cur..*cur + 8)
        .map(|s| u64::from_le_bytes(s.try_into().expect("8 bytes")))
        .ok_or_else(|| io::Error::other("truncated u64"))?;
    *cur += 8;
    Ok(v)
}

/// A packed `struct timespec` (16 bytes on 64-bit), seconds part only.
fn take_timespec_secs(b: &[u8], cur: &mut usize) -> io::Result<i64> {
    let secs = take_u64(b, cur)? as i64;
    *cur += 8;
    Ok(secs)
}
