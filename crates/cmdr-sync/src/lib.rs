// The lint set this crate is held to lives in the workspace root's
// `[workspace.lints]`, opted into by `Cargo.toml`'s `lints.workspace = true`.
#![warn(unused_crate_dependencies)]
#![warn(unreachable_pub)]
#![deny(missing_docs)]

//! Synchronize folders, the pure part: what a volume can do, how names pair up
//! across volumes, and the one answer to "is this copy newer?" that Compare
//! folders (⇧F2) and Synchronize folders share.
//!
//! No `tauri`, no I/O. Rationale and tables: `DETAILS.md`.

mod caps;
mod compare;
mod content;
mod control;
mod filter;
mod key;
mod scan;
mod snapshot;

pub use caps::{BackendFacts, SyncCaps, backend_facts};
pub use compare::{
    Category, CompareBy, CompareRules, ContentState, EntryFacts, EntryKind, HOUR_SHIFT_SECS, Rung,
    SAME_TIME_TOLERANCE_SECS, Verdict, classify,
};
pub use content::{ContentJob, ContentProgress, compare_contents, content_workers};
pub use control::SyncControl;
pub use filter::{ALWAYS_EXCLUDED, Admission, DEFAULT_EXCLUDES, JUNK_MASKS, MaskError, MaskFilter};
pub use key::{EntryKey, NameRule};
pub use scan::{ScanError, ScanOptions, ScanProgress, Side, scan};
pub use snapshot::{Entry, Snapshot, SubtreeFacts, UnreadableReason};

#[cfg(test)]
mod caps_test;
#[cfg(test)]
mod compare_test;
#[cfg(test)]
mod content_test;
#[cfg(test)]
mod control_test;
#[cfg(test)]
mod filter_test;
#[cfg(test)]
mod key_test;
#[cfg(test)]
mod scan_test;
