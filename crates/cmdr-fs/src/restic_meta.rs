//! What a restic snapshot's folder row says about its backup, as data rather
//! than as a sentence.
//!
//! A snapshot folder's name carries only its time and short id. Which machine
//! it came from, its tags, and what it backed up ride on
//! [`FileEntry::restic_meta`](crate::entry::FileEntry::restic_meta) as
//! [`ResticSnapshotMeta`], and the frontend words them from the message catalog,
//! the way it words [`GitEntryMeta`](crate::git_meta::GitEntryMeta).
//!
//! ❌ Never add a field that carries a sentence. A field is the FACT (a hostname,
//! a tag, a path); the words around it live in
//! `apps/desktop/src/lib/intl/messages/*/fileExplorer.json`, keyed
//! `fileExplorer.restic.*`.

use serde::{Deserialize, Serialize};

/// The backup facts a restic snapshot records, for its folder row (the dated
/// one and `latest` alike).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResticSnapshotMeta {
    /// The machine the backup ran on, as restic recorded it. Empty when the
    /// snapshot recorded none.
    pub hostname: String,
    /// The snapshot's tags, sorted. Empty when it has none.
    pub tags: Vec<String>,
    /// The paths the backup was asked to save, sorted, as the backed-up machine
    /// spelled them.
    pub paths: Vec<String>,
}
