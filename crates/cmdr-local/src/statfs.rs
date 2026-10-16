//! Reading what `statfs` and `getfsstat` hand back: the raw char arrays a mount
//! record carries, and the mount a path lives on.

use std::path::Path;

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
pub fn statfs_string(field: &[libc::c_char]) -> String {
    let bytes: Vec<u8> = field.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The mount a path lives on, as one `statfs` reports it.
pub struct MountPointInfo {
    /// Where it's mounted, firmlink-normalized (`/System/Volumes/Data` reads as `/`).
    pub mount_point: String,
    /// `f_fstypename`, for example `apfs` or `smbfs`.
    pub fs_type: String,
    /// `f_mntfromname`: a device node (`/dev/disk3s1`) for a disk or disk image,
    /// a URL-ish string for a share, whatever a FUSE or cloud mount chose.
    pub source: String,
}

/// The mount `path` lives on, from one `statfs`, plus the mount's source.
///
/// On APFS firmlinks, normalizes `/System/Volumes/Data` to `/` (because
/// `statfs("/Users/foo")` returns `/System/Volumes/Data` on modern macOS).
///
/// If `statfs` fails (ENOENT for a deleted directory), walks up parent
/// directories until one succeeds. Returns `None` only if even `/` fails.
pub fn get_mount_info(path: &str) -> Option<MountPointInfo> {
    use std::ffi::CString;

    let mut current = path.to_string();
    loop {
        if let Ok(c_path) = CString::new(current.as_str()) {
            let mut stat: std::mem::MaybeUninit<libc::statfs> = std::mem::MaybeUninit::uninit();
            // SAFETY: `c_path` is a valid NUL-terminated C string from `current`, and `stat` is an
            // uninitialized but correctly-typed `libc::statfs` out-buffer the kernel fills on success.
            let result = unsafe { libc::statfs(c_path.as_ptr(), stat.as_mut_ptr()) };
            if result == 0 {
                // SAFETY: `statfs` returned 0, so the kernel fully initialized `stat`.
                let stat = unsafe { stat.assume_init() };

                let mount_point = statfs_string(&stat.f_mntonname);
                let fs_type = statfs_string(&stat.f_fstypename);
                let source = statfs_string(&stat.f_mntfromname);

                // APFS firmlink normalization: /System/Volumes/Data → /
                let mount_point = if mount_point == "/System/Volumes/Data" {
                    "/".to_string()
                } else {
                    mount_point
                };

                return Some(MountPointInfo {
                    mount_point,
                    fs_type,
                    source,
                });
            }
        }

        // Walk up to parent on failure (handles deleted directories)
        if current == "/" || current.is_empty() {
            return None;
        }
        current = Path::new(&current)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        if current.is_empty() {
            current = "/".to_string();
        }
    }
}
