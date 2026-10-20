//! A repo's snapshots as folders: one per snapshot, plus `latest`.

use rustic_core::Id;
use rustic_core::jiff::Timestamp;
use rustic_core::jiff::tz::TimeZone;
use std::collections::HashMap;
use std::time::SystemTime;

/// The folder name that always means the newest snapshot. A dated name starts
/// with a digit, so the two can never collide.
pub const LATEST_FOLDER_NAME: &str = "latest";

/// One snapshot, as a browser shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// The snapshot's full id, 64 hex digits.
    pub id: String,
    /// When the backup ran.
    pub time: SystemTime,
    /// The machine that made it.
    pub hostname: String,
    /// The paths it backed up, as restic recorded them.
    pub paths: Vec<String>,
    /// Its tags, if any.
    pub tags: Vec<String>,
    /// Its folder name: local time plus the short id, for example
    /// `2026-10-12 21.00.03 (a1b2c3d4)`. Unique within its list, one valid path
    /// component, and sorting by it sorts by time.
    pub folder_name: String,
}

/// Every snapshot in a repo, oldest first.
#[derive(Debug, Clone, Default)]
pub struct SnapshotList {
    snapshots: Vec<Snapshot>,
}

impl SnapshotList {
    /// The snapshots, oldest first.
    pub fn snapshots(&self) -> &[Snapshot] {
        &self.snapshots
    }

    /// The newest snapshot, which the `latest` folder shows. Two taken at the same
    /// instant tie-break by id, so `latest` never flips between refreshes.
    pub fn latest(&self) -> Option<&Snapshot> {
        self.snapshots.last()
    }

    /// The snapshot a folder name stands for: a dated name, or
    /// [`LATEST_FOLDER_NAME`].
    pub fn find(&self, folder_name: &str) -> Option<&Snapshot> {
        if folder_name == LATEST_FOLDER_NAME {
            return self.latest();
        }
        self.snapshots.iter().find(|s| s.folder_name == folder_name)
    }
}

/// How many hex digits of the id a folder name carries, unless two snapshots in
/// the same second share them. Restic's own short id is eight.
const SHORT_ID_LEN: usize = 8;

/// A snapshot as read from the repo, before it's named.
pub(crate) struct RawSnapshot {
    pub(crate) id: Id,
    pub(crate) time: Timestamp,
    pub(crate) hostname: String,
    pub(crate) paths: Vec<String>,
    pub(crate) tags: Vec<String>,
}

/// Names and orders `raw`, with local times in `tz` (the host passes the
/// system's zone; tests pin one).
///
/// Order is by time, then id. A name is the local second plus the shortest id
/// prefix (eight digits at least) that tells apart every snapshot sharing that
/// second, so names stay unique even when two backups start together or a DST
/// fall-back repeats an hour. Sorting by name matches the order to the second;
/// within one second it's by id, and in a repeated DST hour the two passes
/// interleave by name.
pub(crate) fn snapshot_list(mut raw: Vec<RawSnapshot>, tz: &TimeZone) -> SnapshotList {
    raw.sort_by(|a, b| a.time.cmp(&b.time).then(a.id.cmp(&b.id)));
    let stamps: Vec<String> = raw
        .iter()
        .map(|s| tz.to_datetime(s.time).strftime("%Y-%m-%d %H.%M.%S").to_string())
        .collect();
    let hexes: Vec<String> = raw.iter().map(|s| (*s.id.to_hex()).to_string()).collect();

    let mut by_stamp: HashMap<&str, Vec<&str>> = HashMap::new();
    for (stamp, hex) in stamps.iter().zip(&hexes) {
        by_stamp.entry(stamp).or_default().push(hex);
    }
    let short_ids: Vec<String> = stamps
        .iter()
        .zip(&hexes)
        .map(|(stamp, hex)| hex[..unique_prefix_len(hex, &by_stamp[stamp.as_str()])].to_string())
        .collect();

    let snapshots = raw
        .into_iter()
        .zip(stamps.iter().zip(hexes).zip(short_ids))
        .map(|(s, ((stamp, hex), short_id))| Snapshot {
            folder_name: format!("{stamp} ({short_id})"),
            id: hex,
            time: SystemTime::from(s.time),
            hostname: s.hostname,
            paths: s.paths,
            tags: s.tags,
        })
        .collect();
    SnapshotList { snapshots }
}

/// The shortest prefix of `id`, [`SHORT_ID_LEN`] digits or more, that no other id
/// in `same_second` starts with. Only the ids that clash grow.
fn unique_prefix_len(id: &str, same_second: &[&str]) -> usize {
    let longest_shared = same_second
        .iter()
        .filter(|other| **other != id)
        .map(|other| id.bytes().zip(other.bytes()).take_while(|(a, b)| a == b).count())
        .max()
        .unwrap_or(0);
    (longest_shared + 1).clamp(SHORT_ID_LEN, id.len())
}

#[cfg(test)]
#[path = "snapshots_tests.rs"]
mod tests;
