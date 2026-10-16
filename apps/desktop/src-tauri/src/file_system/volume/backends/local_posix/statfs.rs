//! Reading what `statfs` and `getfsstat` hand back: the raw char arrays a mount
//! record carries.

/// Reads one NUL-terminated `statfs` char array as the UTF-8 it holds.
///
/// ❌ Not `c as u8 as char`, which is a latin-1 read: `f_mntonname` and
/// `f_mntfromname` carry real paths, so a mount at `/Volumes/公開` came back as
/// `/Volumes/å¬é`. Nothing downstream can recover from that — the mangled path
/// fails its own `statfs`, so the share resolves to no SMB volume at all and the
/// pane shows an unknown volume with a `path-volumes-å-é-…` id.
///
/// Lossy rather than strict: a mount name is whatever bytes the volume was
/// labelled with, and a replacement character in a display path beats losing the
/// mount.
pub(crate) fn statfs_string(field: &[libc::c_char]) -> String {
    let bytes: Vec<u8> = field.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}
