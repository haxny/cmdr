//! Pure type mapping: NFS attributes into `FileEntry` and `EntryKind`, and an
//! `FSSTAT` answer into `SpaceInfo`.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{EntryKind, SpaceInfo};

use crate::proto::nfs3::{Fattr3, FileType, FsStat, NfsTime};

/// Builds a [`FileEntry`] from one `fattr3`.
///
/// ❗ `app_path` is the APP spelling, prefix and all, ❌ never the
/// export-relative path: a pane hands back what a listing gave it.
///
/// - `permissions` carries the `S_IF*` type bits beside the mode, the way
///   SFTP's mapping does, so a walker can skip a FIFO or a device unopened.
/// - `owner` and `group` are the numeric ids: nothing here can resolve a
///   server's uid to a name, and the number is what "Act as user ID" speaks.
/// - No `created_at`: v3 has no creation date, and repeating the mtime would
///   call it a birth date.
pub(super) fn attrs_to_file_entry(name: &str, app_path: &str, attrs: &Fattr3) -> FileEntry {
    let is_directory = attrs.file_type == FileType::Directory;
    let is_symlink = attrs.file_type == FileType::Symlink;
    let mut entry = FileEntry::new(name.to_string(), app_path.to_string(), is_directory, is_symlink);
    entry.size = (!is_directory).then_some(attrs.size);
    entry.physical_size = (!is_directory).then_some(attrs.used);
    entry.modified_at = Some(u64::from(attrs.mtime.seconds));
    entry.permissions = type_bits(attrs.file_type) | (attrs.mode & 0o7777);
    entry.owner = attrs.uid.to_string();
    entry.group = attrs.gid.to_string();
    entry
}

/// What `attrs` describes, a link as the link.
pub(super) fn entry_kind(attrs: &Fattr3) -> EntryKind {
    match attrs.file_type {
        FileType::Directory => EntryKind::Directory,
        FileType::Symlink => EntryKind::Symlink,
        _ => EntryKind::File,
    }
}

/// An NFS time as a `SystemTime`.
pub(super) fn system_time(time: NfsTime) -> SystemTime {
    UNIX_EPOCH + Duration::new(u64::from(time.seconds), time.nseconds.min(999_999_999))
}

/// The room on the export's filesystem, as `FSSTAT` reports it for this
/// identity: `abytes` is what THIS user may still write (a quota, root's
/// reserve), and what's stored is everything that isn't free. ❗ So `used` is
/// NOT the complement of `available`, which is why this builds
/// [`SpaceInfo::Bounded`] itself. Every number comes off the wire, so each is
/// clamped rather than trusted.
pub(super) fn fsstat_to_space_info(stat: &FsStat) -> SpaceInfo {
    let total_bytes = stat.total_bytes;
    SpaceInfo::Bounded {
        total_bytes,
        available_bytes: stat.available_bytes.min(total_bytes),
        used_bytes: total_bytes.saturating_sub(stat.free_bytes),
    }
}

/// The `st_mode` file-type bits for `file_type`.
fn type_bits(file_type: FileType) -> u32 {
    match file_type {
        FileType::Regular => 0o100_000,
        FileType::Directory => 0o040_000,
        FileType::BlockDevice => 0o060_000,
        FileType::CharDevice => 0o020_000,
        FileType::Symlink => 0o120_000,
        FileType::Socket => 0o140_000,
        FileType::Fifo => 0o010_000,
    }
}

#[cfg(test)]
#[path = "mapping_test.rs"]
mod mapping_test;
