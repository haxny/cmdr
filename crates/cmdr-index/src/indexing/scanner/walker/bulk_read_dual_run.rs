//! The dual-run guard for moving the index onto `cmdr_fs::attr_buf`: the
//! hand-rolled `parse_entry` and `decode_batch` read the same real kernel
//! buffers, and must agree on every entry.

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use super::*;

/// Every raw `getattrlistbulk` batch the kernel writes for `path` under the
/// index's request: `(buffer, record count)`.
fn capture_batches(path: &Path) -> Vec<(Vec<u8>, usize)> {
    let c_path = CString::new(path.as_os_str().as_bytes()).unwrap();
    // SAFETY: `c_path` is a valid NUL-terminated C string that outlives the call.
    let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDONLY) };
    assert!(fd >= 0, "open {}", path.display());
    let dir = DirFd(fd);
    let mut attrlist = REQUEST.attrlist();
    let mut batches = Vec::new();
    loop {
        let mut buf = vec![0u8; ATTR_BUF_SIZE];
        // SAFETY: as in `bulk_read_dir_inner`: a valid dir fd, a well-formed
        // `attrlist`, and a writable buffer of `ATTR_BUF_SIZE` bytes.
        let n = unsafe {
            libc::getattrlistbulk(
                dir.0,
                (&mut attrlist as *mut libc::attrlist).cast::<libc::c_void>(),
                buf.as_mut_ptr().cast::<libc::c_void>(),
                ATTR_BUF_SIZE,
                FSOPT_NOFOLLOW | FSOPT_PACK_INVAL_ATTRS,
            )
        };
        assert!(n >= 0, "getattrlistbulk on {}", path.display());
        if n == 0 {
            return batches;
        }
        batches.push((buf, n.unsigned_abs() as usize));
    }
}

/// `parse_entry` driven exactly as `bulk_read_dir_inner`'s batch loop drives it.
fn parse_entry_batch(buf: &[u8], count: usize, dir: &Path, out: &mut Vec<RawDirEntry>) -> usize {
    let mut unusable = 0usize;
    let mut base = 0usize;
    for _ in 0..count {
        let (entry_len, parsed) = parse_entry(&buf[base..], dir);
        match parsed {
            Some(entry) => out.push(entry),
            None => unusable += 1,
        }
        if entry_len == 0 {
            break;
        }
        base += entry_len;
        if base >= buf.len() {
            break;
        }
    }
    unusable
}

/// The hand-rolled offset table and `attr_buf` read the same real buffers to
/// the same entries: the rich tree `bulk_matches_symlink_metadata` checks
/// (plus a fifo, whose record packs the file group), `/usr/bin`, and `/`
/// (firmlinks, symlinks, mount points).
#[test]
fn attr_buf_decodes_real_buffers_exactly_as_parse_entry_does() {
    let request = REQUEST.attrlist();
    assert_eq!(
        (request.commonattr, request.dirattr, request.fileattr),
        (
            ATTR_CMN_RETURNED_ATTRS | ATTR_CMN_NAME | ATTR_CMN_OBJTYPE | ATTR_CMN_MODTIME | ATTR_CMN_FILEID,
            0,
            ATTR_FILE_LINKCOUNT | ATTR_FILE_ALLOCSIZE | ATTR_FILE_DATALENGTH,
        ),
        "attr_buf asks for exactly what the offset table reads"
    );

    let tree = tempfile::tempdir().expect("temp dir");
    let root = tree.path();
    std::fs::write(root.join("empty.bin"), b"").unwrap();
    std::fs::write(root.join("small.txt"), b"hello world").unwrap();
    std::fs::write(root.join("larger.bin"), vec![7u8; 40_000]).unwrap();
    std::fs::write(root.join("náïve-unïcode.txt"), b"unicode name").unwrap();
    std::fs::create_dir(root.join("subdir")).unwrap();
    std::fs::create_dir(root.join("empty_dir")).unwrap();
    std::os::unix::fs::symlink(root.join("small.txt"), root.join("link.txt")).unwrap();
    std::os::unix::fs::symlink(root.join("gone"), root.join("broken.txt")).unwrap();
    std::fs::hard_link(root.join("small.txt"), root.join("small_alias.txt")).unwrap();
    let fifo = CString::new(root.join("pipe").as_os_str().as_bytes()).unwrap();
    // SAFETY: `fifo` is a valid NUL-terminated C string that outlives the call.
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o644) }, 0, "mkfifo");

    for dir in [root, Path::new("/usr/bin"), Path::new("/")] {
        let batches = capture_batches(dir);
        assert!(!batches.is_empty(), "{} has entries", dir.display());
        for (buf, count) in batches {
            let (mut old, mut new) = (Vec::new(), Vec::new());
            let old_unusable = parse_entry_batch(&buf, count, dir, &mut old);
            let new_unusable = decode_batch(&buf, count, dir, &mut new);
            assert_eq!(new_unusable, old_unusable, "unusable count in {}", dir.display());
            assert_eq!(old_unusable, 0, "no real record is unusable in {}", dir.display());
            assert_eq!(new.len(), old.len(), "entry count in {}", dir.display());
            for (a, b) in old.iter().zip(&new) {
                assert_eq!(a.path, b.path);
                assert_eq!(a.file_type, b.file_type, "type of {}", a.path.display());
                assert_eq!(
                    a.stat
                        .map(|s| (s.logical_size, s.physical_size, s.modified_at, s.inode, s.nlink)),
                    b.stat
                        .map(|s| (s.logical_size, s.physical_size, s.modified_at, s.inode, s.nlink)),
                    "stat of {}",
                    a.path.display()
                );
            }
        }
    }
}
