//! What a batch rename tells its engine as it goes: each hop announced before it
//! lands and journaled the moment it has, through the batch's own scope.

use std::path::Path;
use std::sync::Arc;

use super::super::super::scope::OpScope;
use super::{BulkRenameOutcome, BulkRenameRow};
use cmdr_operation_log::types::{EntryType, ItemOutcome};

/// Journals what the batch actually did to the filesystem, as it happens.
///
/// **Every hop journals the moment it lands; rows that never moved journal once the
/// run settles.** A crash or force-quit mid-batch has to leave rollback units for the
/// renames that already happened, or the files sit under their new names with an empty
/// journal and nothing for undo to reverse. A row that never moved carries no such
/// debt: nothing on disk depends on it, so its `Skipped`/`Failed` row can wait for the
/// end of the run.
pub(super) struct BulkRenameRecorder {
    /// The batch's scope, whose journal every row lands in.
    scope: Arc<OpScope>,
    volume_id: String,
}

impl BulkRenameRecorder {
    pub(super) fn new(scope: Arc<OpScope>, volume_id: String) -> Self {
        Self { scope, volume_id }
    }

    /// Announces both ends of a hop before the rename that makes it.
    pub(super) fn note_write(&self, from: &Path, to: &Path) {
        self.scope.host().announce(from);
        self.scope.host().announce(to);
    }

    /// One completed filesystem hop, recorded the moment it lands.
    ///
    /// A rotation's temp hop is a hop like any other and gets its own row, so a crash
    /// mid-rotation leaves the file findable by name and reversible in the ordinary
    /// reverse-order replay. That is why a two-file swap journals three rows.
    ///
    /// ❌ Never track a rotation temp in `in_flight_temps` instead. That ledger's
    /// next-launch sweep DELETES what it holds, which is right for the half-written
    /// `.cmdr-tmp-*` partials it exists for and catastrophic here: a rotation temp
    /// holds a COMPLETE user file under a private name, so sweeping it destroys data.
    pub(super) fn record_hop(&self, row: &BulkRenameRow, from: &Path, to: &Path) {
        let (size, mtime) = row.expected_fingerprint.journal_snapshot();
        super::super::super::journal::record_volume_leaf(
            &self.scope,
            EntryType::File,
            &self.volume_id,
            from,
            Some((&self.volume_id, to)),
            size,
            mtime,
            false,
            ItemOutcome::Done,
        );
    }

    /// The rows that never moved, recorded once the run has settled. Journaling a skip
    /// is what keeps the operation log honest about the batch the user approved: before
    /// this, a skipped row was dropped entirely and the log claimed a smaller batch than
    /// the one that ran.
    pub(super) fn record_unmoved(&self, rows: &[BulkRenameRow], outcomes: &[BulkRenameOutcome]) {
        for (row, outcome) in rows.iter().zip(outcomes.iter().copied()) {
            let item_outcome = match outcome {
                // A row that landed already has its own hop row (or three, for a rotation).
                BulkRenameOutcome::Done => continue,
                BulkRenameOutcome::Skipped => ItemOutcome::Skipped,
                BulkRenameOutcome::Failed => ItemOutcome::Failed,
            };
            let (size, mtime) = row.expected_fingerprint.journal_snapshot();
            super::super::super::journal::record_volume_leaf(
                &self.scope,
                EntryType::File,
                &self.volume_id,
                &row.source,
                // The destination it was MEANT to reach, so the log can say what was
                // skipped. Undo never reverses a non-`Done` row (`restore_move` refuses
                // one), so naming a destination here can't move anything.
                Some((&self.volume_id, &row.destination)),
                size,
                mtime,
                false,
                item_outcome,
            );
        }
    }
}
