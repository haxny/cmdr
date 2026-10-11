//! A fresh compress's transfer-tree rows, driven by the producer's progress
//! ticks (`fresh_zip.rs::produce`). Built and fed by `fresh_compress.rs`.
//!
//! `produce` writes the plan's entries one at a time, in plan order, and every
//! tick says how many it has finished, so the entry in flight is always the
//! `entries_done`th: one active row at a time, its bytes the source bytes read
//! since the entry before it finished. Deriving rows from the ticks keeps the
//! producer free of any tree vocabulary, and like every report it can't reach
//! back into the compress (`transfer_tree/DETAILS.md` § "Observation only,
//! structurally").

use std::sync::Mutex;
use std::sync::atomic::Ordering;

use super::super::transfer_tree::{NodeId, TreeLeaf, TreeReporter};
use super::fresh_zip::FreshZipProgress;
use crate::ignore_poison::IgnorePoison;

/// The compress's rows: each plan entry's, and the one in flight.
pub(super) struct CompressRows {
    tree: TreeReporter,
    /// Each plan entry's row, and whether it's a folder.
    entries: Vec<(NodeId, bool)>,
    current: Mutex<InFlightEntry>,
}

/// The entry `produce` is on.
// DEFAULT-OK: entry 0, nothing read yet, no row open: where a producer starts.
#[derive(Default)]
struct InFlightEntry {
    index: usize,
    /// Source bytes done when this entry started.
    started_at: u64,
    row: Option<TreeLeaf>,
}

impl CompressRows {
    pub(super) fn new(tree: TreeReporter, entries: Vec<(NodeId, bool)>) -> Self {
        Self {
            tree,
            entries,
            current: Mutex::default(),
        }
    }

    /// One producer tick: settles every entry it says is finished, then opens
    /// and advances the row of the one in flight.
    pub(super) fn on_tick(&self, tick: FreshZipProgress) {
        let mut current = self.current.lock_ignore_poison();
        while current.index < tick.entries_done {
            match (self.entries.get(current.index), current.row.take()) {
                (_, Some(row)) => row.complete(),
                (Some(&(folder, true)), None) => self.tree.folder_in_place(Some(folder)),
                // A file finished before any chunk of it reported (an empty one).
                (Some(&(file, false)), None) => self.tree.leaf(Some(file)).complete(),
                (None, None) => {}
            }
            current.index += 1;
            current.started_at = tick.source_bytes_done;
        }
        if current.row.is_none()
            && let Some(&(file, false)) = self.entries.get(current.index)
        {
            current.row = Some(self.tree.leaf(Some(file)));
        }
        let read = tick.source_bytes_done.saturating_sub(current.started_at);
        if let Some(row) = &current.row {
            row.bytes().store(read, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
#[path = "fresh_rows_tests.rs"]
mod tests;
