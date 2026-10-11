//! What a copy carries from its source beyond the bytes and the mtime: the
//! creation date, the mode, the user flags, and the extended attributes.
//!
//! ❗ No mtime and no atime here. A stream's `modified_at` is the ONE source of
//! a copy's mtime, so two getters can never disagree; a local copy hands both
//! dates to [`super::apply_staged`] beside this set ([`super::StagedTimes`]).

use std::ffi::OsString;
use std::fs::{File, Metadata};
use std::path::Path;
use std::time::SystemTime;

use super::UserFlags;

/// The xattr macOS stamps on every file a process writes. The kernel ignores a
/// write of it (verified on macOS 27.0, `xattr -w` then `-p` reads back the
/// writer's own value, 2026-10-11), so copying it costs a read and a write for
/// nothing: a copy never carries it.
const PROVENANCE_XATTR: &str = "com.apple.provenance";

/// One extended attribute: its name and its whole value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Xattr {
    /// The full name, namespace included (`com.apple.metadata:_kMDItemUserTags`,
    /// `user.app`).
    pub name: OsString,
    /// The value, opaque bytes.
    pub value: Vec<u8>,
}

/// The source's metadata a copy puts on what it writes, apart from the dates
/// the stream already owns.
///
/// The flags travel here but go on only after the landing rename
/// ([`super::LandedMetadata`]): Locked refuses that rename, and the whole set
/// lands together so Hidden can't race ahead of it.
// DEFAULT-OK: the empty set is "the source carried nothing beyond its bytes",
// which is what a source that can't read any of it honestly reports.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CarriedMetadata {
    /// The creation (birth) date.
    pub created: Option<SystemTime>,
    /// The permission bits, setuid/setgid/sticky included (`0o7777`).
    pub mode: Option<u32>,
    /// The user flags Finder shows.
    pub flags: UserFlags,
    /// The extended attributes, provenance excluded; on Linux only `user.*`.
    pub xattrs: Vec<Xattr>,
}

impl CarriedMetadata {
    /// What the local file `file` carries, read off the handle the copy already
    /// holds and the `meta` its `fstat` returned: no path lookup, and no call at
    /// all for a class the stat already answers. A file with no xattrs costs one
    /// `flistxattr`.
    pub fn of_local_file(file: &File, meta: &Metadata) -> Self {
        use xattr::FileExt as _;
        let xattrs = match file.list_xattr() {
            Ok(names) => names
                .filter(carries)
                .filter_map(|name| match file.get_xattr(&name) {
                    Ok(Some(value)) => Some(Xattr { name, value }),
                    Ok(None) => None,
                    Err(e) => {
                        log::debug!(target: "metadata", "couldn't read xattr {name:?}: {e}");
                        None
                    }
                })
                .collect(),
            Err(e) => {
                log::debug!(target: "metadata", "couldn't list a source's xattrs: {e}");
                Vec::new()
            }
        };
        Self {
            xattrs,
            ..Self::of_stat(meta)
        }
    }

    /// What the symlink at `path` carries ITSELF (never its target's), given the
    /// `lstat` the caller took: its creation date and its own xattrs. No mode
    /// (a link's is meaningless) and no flags.
    pub fn of_local_link(path: &Path, meta: &Metadata) -> Self {
        // `xattr::list` and `xattr::get` don't follow a link (`XATTR_NOFOLLOW`).
        let xattrs = match xattr::list(path) {
            Ok(names) => names
                .filter(carries)
                .filter_map(|name| match xattr::get(path, &name) {
                    Ok(Some(value)) => Some(Xattr { name, value }),
                    _ => None,
                })
                .collect(),
            Err(_) => Vec::new(),
        };
        Self {
            created: meta.created().ok(),
            mode: None,
            flags: UserFlags::NONE,
            xattrs,
        }
    }

    /// The classes a stat answers: creation date, mode, and (macOS) flags.
    fn of_stat(meta: &Metadata) -> Self {
        use std::os::unix::fs::PermissionsExt as _;
        Self {
            created: meta.created().ok(),
            mode: Some(meta.permissions().mode() & 0o7777),
            flags: flags_of(meta),
            xattrs: Vec::new(),
        }
    }
}

#[cfg(target_os = "macos")]
fn flags_of(meta: &Metadata) -> UserFlags {
    use std::os::macos::fs::MetadataExt as _;
    UserFlags::from_bsd(meta.st_flags())
}

#[cfg(not(target_os = "macos"))]
fn flags_of(_meta: &Metadata) -> UserFlags {
    UserFlags::NONE
}

/// Whether a copy carries the xattr `name`. On macOS everything but provenance;
/// on Linux only `user.*`: `system.posix_acl_*` is the ACL (`exacl` carries it),
/// and `trusted.*` and `security.*` need privileges a desktop app doesn't have.
fn carries(name: &OsString) -> bool {
    if cfg!(target_os = "macos") {
        name != PROVENANCE_XATTR
    } else {
        name.as_encoded_bytes().starts_with(b"user.")
    }
}
