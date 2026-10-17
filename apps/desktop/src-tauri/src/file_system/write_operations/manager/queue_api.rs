//! What the queue window, the MCP `queue` tool, and the volume guards ask an
//! engine about its operations: list them, cancel, pause, resume, reorder,
//! dismiss a failure, and whether any still needs a volume. Each is an
//! [`Operations`] method over the engine's manager and registry; the free
//! functions of the same names are the installed engine's, for callers outside
//! the engine.

use super::super::Operations;
use super::{OperationSnapshot, PauseAllOutcome, PauseOutcome, QueueMoveDirection, QueueMoveOutcome};

impl Operations {
    /// The thin registry snapshot (membership + lifecycle status) for the queue
    /// window. Backs the `list_operations` IPC command.
    pub(crate) fn list_operations(&self) -> Vec<OperationSnapshot> {
        self.manager().list()
    }

    /// Whether an operation still needs `volume_id`: one the manager holds,
    /// Queued, Running, or Paused, or an external transfer (a drag-out) reading
    /// from it.
    ///
    /// ❗ Wider than the busy set Eject reads (`busy_volume_ids`, Running and
    /// Paused only): a Queued op hasn't touched the device yet, but it names the
    /// volume and will set out for it, so anything that retires the volume's id
    /// for good (a saved server moving to a new address) has to wait for it too.
    pub(crate) fn operations_need_volume(&self, volume_id: &str) -> bool {
        // Asked one after the other, ❌ never nested: the busy set takes the status
        // cache lock, and `get_operation_status` orders the two the other way.
        self.manager().names_volume(volume_id) || self.status().busy_volume_ids().iter().any(|id| id == volume_id)
    }

    /// Cancels one operation, keeping already-copied files (the existing
    /// `rollback=false` path). A Queued op is dropped from the registry without
    /// ever spawning; a Running/Paused op routes through the intent state
    /// machine. Backs the `cancel_operation(id)` IPC command.
    pub(crate) fn cancel_operation(&self, operation_id: &str) {
        if !self.manager().cancel_if_queued(operation_id) {
            self.cancel_write_operation(operation_id, false);
        }
    }

    /// Cancels several operations (keep-partials each). Backs the
    /// `cancel_operations(ids)` IPC command (the queue window's "Cancel
    /// selected").
    pub(crate) fn cancel_operations(&self, operation_ids: &[String]) {
        for id in operation_ids {
            self.cancel_operation(id);
        }
    }

    /// Pauses one Running operation: parks it at its next between-files
    /// boundary and flips its `LifecycleStatus` to `Paused` (re-emitting
    /// `operations-changed`). A paused op keeps its lane slots. Pausing a Queued
    /// op is a v1 no-op (it isn't touching a device yet — it stays Queued and
    /// admits normally when its lanes free); pausing a Done/absent op is a
    /// no-op. Backs `pause_operation(id)`.
    ///
    /// The [`PauseOutcome`] rides all the way out to the IPC command and the MCP
    /// `queue` tool: a surface that reports a pause it didn't get sends its user
    /// (or its agent) off believing the device is free.
    pub(crate) fn pause_operation(&self, operation_id: &str) -> PauseOutcome {
        // Flip the live gate (so the driver parks) and the record status (so the
        // UI shows Paused). `set_paused` only flips a Running record, so a Queued
        // op's gate is intentionally left untouched: parking a not-yet-spawned op
        // would do nothing and risk a Paused-but-Queued limbo.
        let outcome = self.manager().set_paused(operation_id, true);
        if outcome == PauseOutcome::Applied {
            self.pause_write_operation(operation_id);
        }
        outcome
    }

    /// Resumes one Paused operation: clears its gate (waking the parked driver)
    /// and flips its `LifecycleStatus` back to `Running`. No admission pass — it
    /// never freed its lanes. Resuming a non-paused op is a no-op. Backs
    /// `resume_operation(id)`, and reports what it did for the same reason
    /// [`Self::pause_operation`] does.
    pub(crate) fn resume_operation(&self, operation_id: &str) -> PauseOutcome {
        let outcome = self.manager().set_paused(operation_id, false);
        if outcome == PauseOutcome::Applied {
            self.resume_write_operation(operation_id);
        }
        outcome
    }

    /// Pauses every currently-Running operation. Backs `pause_all` (the queue
    /// window's global Pause all). Snapshots the running set first so the
    /// iteration is stable.
    ///
    /// Reports the whole sweep as a [`PauseAllOutcome`], for the reason
    /// [`Self::pause_operation`] reports its own: an empty running set, a scan
    /// still walking, and three parked copies are three different answers, and a
    /// caller that can't tell them apart says "paused" to all three.
    pub(crate) fn pause_all(&self) -> PauseAllOutcome {
        self.manager()
            .running_ids()
            .iter()
            .map(|id| self.pause_operation(id))
            .collect()
    }

    /// Resumes every currently-Paused operation. Backs `resume_all` (Resume
    /// all), and reports the sweep the way [`Self::pause_all`] does.
    pub(crate) fn resume_all(&self) -> PauseAllOutcome {
        self.manager()
            .paused_ids()
            .iter()
            .map(|id| self.resume_operation(id))
            .collect()
    }

    /// Moves a waiting operation one place earlier or later in line, so it's
    /// admitted sooner or later than the waiting operations around it. Running
    /// and paused operations keep their place. Backs `move_queued_operation(id,
    /// direction)`, the queue window's ⌥↑ / ⌥↓. Reports what it did, so the
    /// window announces the operation's real place rather than the one it asked
    /// for.
    pub(crate) fn move_queued_operation(&self, operation_id: &str, direction: QueueMoveDirection) -> QueueMoveOutcome {
        self.manager().move_queued(operation_id, direction)
    }

    /// Puts a waiting operation at `position` (1-based, counting waiting
    /// operations only), shifting the ones in between. Backs
    /// `place_queued_operation(id, position)`, the queue window's
    /// drag-to-reorder.
    pub(crate) fn place_queued_operation(&self, operation_id: &str, position: u32) -> QueueMoveOutcome {
        self.manager().place_queued(operation_id, position)
    }

    /// Drops one retained failure and re-broadcasts the snapshot. Backs
    /// `dismiss_failed_operation(id)`: the queue row's Dismiss button, and the
    /// foreground error dialog's close path for the operation it was showing.
    /// Dismissal is ALWAYS explicit — ❌ never a timer, a window close, or the
    /// next operation starting. Unknown id: a no-op.
    pub(crate) fn dismiss_failed_operation(&self, operation_id: &str) {
        self.manager().dismiss_failure(operation_id);
    }

    /// Drops every retained failure and re-broadcasts the snapshot. Backs
    /// `dismiss_all_failed_operations` (the queue toolbar's "Dismiss all").
    pub(crate) fn dismiss_all_failed_operations(&self) {
        self.manager().dismiss_all_failures();
    }
}

// The installed engine's, by free function, for callers outside the engine.
// Before `install` each answers what an engine with no operations would.
// shim: removed in M7a

/// [`Operations::list_operations`](Operations::list_operations) on the installed engine.
pub fn list_operations() -> Vec<OperationSnapshot> {
    super::super::installed_for("list_operations")
        .map(Operations::list_operations)
        .unwrap_or_default()
}

/// [`Operations::operations_need_volume`](Operations::operations_need_volume) on the installed engine.
pub fn operations_need_volume(volume_id: &str) -> bool {
    super::super::installed_for("operations_need_volume").is_some_and(|ops| ops.operations_need_volume(volume_id))
}

/// [`Operations::cancel_operation`](Operations::cancel_operation) on the installed engine.
pub fn cancel_operation(operation_id: &str) {
    if let Some(ops) = super::super::installed_for("cancel_operation") {
        ops.cancel_operation(operation_id);
    }
}

/// [`Operations::cancel_operations`](Operations::cancel_operations) on the installed engine.
pub fn cancel_operations(operation_ids: &[String]) {
    if let Some(ops) = super::super::installed_for("cancel_operations") {
        ops.cancel_operations(operation_ids);
    }
}

/// [`Operations::pause_operation`](Operations::pause_operation) on the installed engine.
pub fn pause_operation(operation_id: &str) -> PauseOutcome {
    super::super::installed_for("pause_operation")
        .map_or(PauseOutcome::NotApplicable, |ops| ops.pause_operation(operation_id))
}

/// [`Operations::resume_operation`](Operations::resume_operation) on the installed engine.
pub fn resume_operation(operation_id: &str) -> PauseOutcome {
    super::super::installed_for("resume_operation")
        .map_or(PauseOutcome::NotApplicable, |ops| ops.resume_operation(operation_id))
}

/// [`Operations::pause_all`](Operations::pause_all) on the installed engine.
pub fn pause_all() -> PauseAllOutcome {
    super::super::installed_for("pause_all")
        .map(Operations::pause_all)
        .unwrap_or_default()
}

/// [`Operations::resume_all`](Operations::resume_all) on the installed engine.
pub fn resume_all() -> PauseAllOutcome {
    super::super::installed_for("resume_all")
        .map(Operations::resume_all)
        .unwrap_or_default()
}

/// [`Operations::move_queued_operation`](Operations::move_queued_operation) on the installed engine.
pub fn move_queued_operation(operation_id: &str, direction: QueueMoveDirection) -> QueueMoveOutcome {
    super::super::installed_for("move_queued_operation").map_or(QueueMoveOutcome::NotQueued, |ops| {
        ops.move_queued_operation(operation_id, direction)
    })
}

/// [`Operations::place_queued_operation`](Operations::place_queued_operation) on the installed engine.
pub fn place_queued_operation(operation_id: &str, position: u32) -> QueueMoveOutcome {
    super::super::installed_for("place_queued_operation").map_or(QueueMoveOutcome::NotQueued, |ops| {
        ops.place_queued_operation(operation_id, position)
    })
}

/// [`Operations::dismiss_failed_operation`](Operations::dismiss_failed_operation) on the installed engine.
pub fn dismiss_failed_operation(operation_id: &str) {
    if let Some(ops) = super::super::installed_for("dismiss_failed_operation") {
        ops.dismiss_failed_operation(operation_id);
    }
}

/// [`Operations::dismiss_all_failed_operations`](Operations::dismiss_all_failed_operations) on the installed engine.
pub fn dismiss_all_failed_operations() {
    if let Some(ops) = super::super::installed_for("dismiss_all_failed_operations") {
        ops.dismiss_all_failed_operations();
    }
}
