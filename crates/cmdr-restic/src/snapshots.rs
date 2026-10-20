//! A repo's snapshots as folders: one per snapshot, plus `latest`.

use rustic_core::jiff::Timestamp;
use rustic_core::jiff::tz::TimeZone;
use rustic_core::repofile::SnapshotFile;
use rustic_core::{Id, TreeId};
use std::time::SystemTime;

/// The folder name that always means the newest snapshot. A dated name starts
/// with a digit, so the two can never collide.
pub(crate) const LATEST_FOLDER_NAME: &str = "latest";

/// How many hex digits of the id a folder name carries, unless another snapshot
/// shares them. Restic's own short id is eight.
pub(crate) const SHORT_ID_LEN: usize = 8;

/// One snapshot, as a browser shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Snapshot {
    /// The snapshot's full id, 64 hex digits.
    pub(crate) id: String,
    /// When the backup ran: the folder's Modified date.
    pub(crate) time: SystemTime,
    /// The root of the backed-up tree.
    pub(crate) tree: TreeId,
    /// The bytes the backup processed, from the snapshot's summary. `None` for a
    /// snapshot without one (restic before 0.17 wrote none), ❌ never a made-up 0.
    pub(crate) size: Option<u64>,
    /// Its folder name: local time plus the short id, for example
    /// `2026-10-12 21.00.03 (a1b2c3d4)`. Unique within its list, one valid path
    /// component, and sorting by it sorts by time.
    pub(crate) folder_name: String,
}

/// Every snapshot in a repo, oldest first.
#[derive(Debug, Clone, Default)]
pub(crate) struct SnapshotList {
    snapshots: Vec<Snapshot>,
}

impl SnapshotList {
    /// The snapshots, oldest first.
    pub(crate) fn snapshots(&self) -> &[Snapshot] {
        &self.snapshots
    }

    /// The newest snapshot. Two taken at the same instant tie-break by id, so the
    /// answer never flips between refreshes.
    pub(crate) fn latest(&self) -> Option<&Snapshot> {
        self.snapshots.last()
    }

    /// The one snapshot whose id starts with `short_id`, the suffix of a dated
    /// folder name. `None` when none does, and when SEVERAL do: a name saved before
    /// a newer backup shared its prefix resolves to nothing, ❌ never to the wrong
    /// snapshot.
    pub(crate) fn by_short_id(&self, short_id: &str) -> Option<&Snapshot> {
        let mut matching = self.snapshots.iter().filter(|s| s.id.starts_with(short_id));
        let found = matching.next()?;
        matching.next().is_none().then_some(found)
    }

    /// The snapshot with this full id.
    pub(crate) fn by_id(&self, id: &str) -> Option<&Snapshot> {
        self.snapshots.iter().find(|s| s.id == id)
    }
}

/// A snapshot as read from the repo, before it's named.
pub(crate) struct RawSnapshot {
    pub(crate) id: Id,
    pub(crate) time: Timestamp,
    pub(crate) tree: TreeId,
    pub(crate) size: Option<u64>,
}

impl From<&SnapshotFile> for RawSnapshot {
    fn from(file: &SnapshotFile) -> Self {
        Self {
            id: *file.id,
            time: file.time.timestamp(),
            tree: file.tree,
            size: file.summary.as_ref().map(|summary| summary.total_bytes_processed),
        }
    }
}

/// Names and orders the snapshots in `files`, with local times in the system's
/// zone.
pub(crate) fn snapshot_list_of(files: &[SnapshotFile]) -> SnapshotList {
    snapshot_list(files.iter().map(RawSnapshot::from).collect(), &TimeZone::system())
}

/// Names and orders `raw`, with local times in `tz` (the host passes the
/// system's zone; tests pin one).
///
/// Order is by time, then id. A name is the local second plus the shortest id
/// prefix (eight digits at least) that no other snapshot in the repo starts
/// with, so names stay unique even when two backups start together or a DST
/// fall-back repeats an hour, and the suffix alone finds its snapshot. Sorting by
/// name matches the order to the second; within one second it's by id, and in a
/// repeated DST hour the two passes interleave by name.
pub(crate) fn snapshot_list(mut raw: Vec<RawSnapshot>, tz: &TimeZone) -> SnapshotList {
    raw.sort_by(|a, b| a.time.cmp(&b.time).then(a.id.cmp(&b.id)));
    let hexes: Vec<String> = raw.iter().map(|s| (*s.id.to_hex()).to_string()).collect();
    let mut sorted: Vec<&str> = hexes.iter().map(String::as_str).collect();
    sorted.sort_unstable();

    let snapshots = raw
        .into_iter()
        .zip(&hexes)
        .map(|(s, hex)| {
            let stamp = tz.to_datetime(s.time).strftime("%Y-%m-%d %H.%M.%S");
            let short_id = &hex[..unique_prefix_len(hex, &sorted)];
            Snapshot {
                folder_name: format!("{stamp} ({short_id})"),
                id: hex.clone(),
                time: SystemTime::from(s.time),
                tree: s.tree,
                size: s.size,
            }
        })
        .collect();
    SnapshotList { snapshots }
}

/// The shortest prefix of `id`, [`SHORT_ID_LEN`] digits or more, that no other id
/// in `sorted` (every id in the repo, sorted) starts with. Only the ids that
/// clash grow. The ids that share the longest prefix with `id` are its
/// neighbors in sort order, so two comparisons decide it.
fn unique_prefix_len(id: &str, sorted: &[&str]) -> usize {
    let shared = |other: &str| id.bytes().zip(other.bytes()).take_while(|(a, b)| a == b).count();
    let at = sorted.partition_point(|other| *other < id);
    let before = at.checked_sub(1).and_then(|i| sorted.get(i));
    let after = sorted[at..].iter().find(|other| **other != id);
    let longest_shared = before
        .into_iter()
        .chain(after)
        .map(|other| shared(other))
        .max()
        .unwrap_or(0);
    (longest_shared + 1).clamp(SHORT_ID_LEN, id.len())
}

#[cfg(test)]
#[path = "snapshots_tests.rs"]
mod tests;
