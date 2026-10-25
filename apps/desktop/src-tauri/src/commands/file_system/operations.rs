//! Tauri commands for the operation manager: the queue's membership and
//! lifecycle (list, cancel, pause, resume, reorder, dismiss, retry) and the live
//! transfer tree. They mirror the frontend's `tauri-commands/operations.ts`.

use crate::operations_host::operations;
use cmdr_operation_log::types::OperationId;
use cmdr_operations::{
    OperationSnapshot, Operations, PauseAllOutcome, PauseOutcome, QueueMoveDirection, QueueMoveOutcome, RetryOutcome,
    TransferTreeView, TransferTreeViewError, TransferTreeViewRequest, WriteOperationError, WriteProgressEvent,
};

/// Returns the thin operation registry snapshot (membership + lifecycle
/// status) for the queue window. Live per-row progress comes from the separate
/// `write-progress` stream; this snapshot stays thin.
#[tauri::command]
#[specta::specta]
pub fn list_operations() -> Vec<OperationSnapshot> {
    operations().map(Operations::list_operations).unwrap_or_default()
}

/// The newest `write-progress` tick of every live operation that has emitted
/// one, for a queue window that opens late. A paused operation emits nothing,
/// so this is the only way that window can draw its bars and tree before the
/// next Resume. Read once at mount; the event stream takes over from there.
/// `async` so it never answers on the main thread: it takes each operation's
/// progress lock, which an emitting driver holds for a moment at a time.
#[tauri::command]
#[specta::specta]
pub async fn latest_operation_progress() -> Vec<WriteProgressEvent> {
    operations()
        .map(Operations::latest_operation_progress)
        .unwrap_or_default()
}

/// One window of an operation's live transfer tree, for the progress dialog and
/// the queue. A view calls it on mount and again on each `write-progress` tick
/// of its operation, with its own hand toggles and scroll position; the
/// backend keeps no per-view state. `ready: false` while the operation scans.
///
/// `async` so it never answers on the main thread: a window costs microseconds,
/// but it takes the tree's lock, which a driver settling a whole subtree at once
/// (a folder renamed in one call) holds for as long as that subtree is big.
#[tauri::command]
#[specta::specta]
pub async fn get_transfer_tree_view(
    operation_id: String,
    request: TransferTreeViewRequest,
) -> Result<TransferTreeView, TransferTreeViewError> {
    operations()
        .map_err(|_| TransferTreeViewError::UnknownOperation)?
        .transfer_trees()
        .view(&OperationId::from(operation_id), &request)
}

/// The progress dialog showing this operation's tree closed. A settled
/// operation keeps its tree for that dialog's last pulls; this lets it go
/// before its safety TTL does. A no-op for a running operation.
#[tauri::command]
#[specta::specta]
pub fn release_transfer_tree(operation_id: String) {
    if let Ok(engine) = operations() {
        engine.transfer_trees().release(&OperationId::from(operation_id));
    }
}

/// Cancels one operation, keeping already-copied files. A Queued op is dropped
/// without ever spawning; a Running/Paused op routes through the existing
/// keep-partials cancel path.
#[tauri::command]
#[specta::specta]
pub fn cancel_operation(operation_id: String) {
    if let Ok(engine) = operations() {
        engine.cancel_operation(&OperationId::from(operation_id));
    }
}

/// Cancels several operations (keep-partials each). Backs the queue window's
/// "Cancel selected".
#[tauri::command]
#[specta::specta]
pub fn cancel_operations(operation_ids: Vec<String>) {
    if let Ok(engine) = operations() {
        let operation_ids: Vec<OperationId> = operation_ids.into_iter().map(OperationId::from).collect();
        engine.cancel_operations(&operation_ids);
    }
}

/// Pauses one Running operation. It parks at the next between-files boundary and
/// its lifecycle status flips to `paused` in `operations-changed`. A paused op
/// keeps holding its lane slots. Pausing a Queued/Done op is a no-op.
///
/// Returns what actually happened, so a caller never has to assume it worked.
#[tauri::command]
#[specta::specta]
pub fn pause_operation(operation_id: String) -> PauseOutcome {
    operations().map_or(PauseOutcome::NotApplicable, |engine| {
        engine.pause_operation(&OperationId::from(operation_id))
    })
}

/// Resumes one paused operation: it continues from where it parked and its
/// status flips back to `running`. Resuming a non-paused op is a no-op.
///
/// Returns what actually happened, like [`pause_operation`].
#[tauri::command]
#[specta::specta]
pub fn resume_operation(operation_id: String) -> PauseOutcome {
    operations().map_or(PauseOutcome::NotApplicable, |engine| {
        engine.resume_operation(&OperationId::from(operation_id))
    })
}

/// Pauses every currently-running operation. Backs the queue window's global
/// "Pause all".
///
/// Returns the per-outcome counts for the whole sweep, so a caller never has to
/// assume it worked. An empty running set and three parked copies are different
/// answers.
#[tauri::command]
#[specta::specta]
pub fn pause_all() -> PauseAllOutcome {
    operations().map(Operations::pause_all).unwrap_or_default()
}

/// Resumes every currently-paused operation. Backs "Resume all".
///
/// Returns the sweep's counts, like [`pause_all`].
#[tauri::command]
#[specta::specta]
pub fn resume_all() -> PauseAllOutcome {
    operations().map(Operations::resume_all).unwrap_or_default()
}

/// Moves a waiting operation one place earlier or later in line (the queue
/// window's ⌥↑ / ⌥↓). Only waiting operations move; running and paused ones
/// keep their place.
///
/// Returns where the operation ended up, so the window announces its real
/// place in line.
#[tauri::command]
#[specta::specta]
pub fn move_queued_operation(operation_id: String, direction: QueueMoveDirection) -> QueueMoveOutcome {
    operations().map_or(QueueMoveOutcome::NotQueued, |engine| {
        engine.move_queued_operation(&OperationId::from(operation_id), direction)
    })
}

/// Puts a waiting operation at `position` in line (1-based, counting waiting
/// operations only): the drop of a drag in the queue window. Only waiting
/// operations move; running and paused ones keep their place.
///
/// Returns where the operation ended up, so the window announces its real
/// place in line.
#[tauri::command]
#[specta::specta]
pub fn place_queued_operation(operation_id: String, position: u32) -> QueueMoveOutcome {
    operations().map_or(QueueMoveOutcome::NotQueued, |engine| {
        engine.place_queued_operation(&OperationId::from(operation_id), position)
    })
}

/// Drops one retained failure from the snapshot (the queue row's Dismiss, and
/// the foreground error dialog's close path). Dismissal is always explicit: a
/// failure never expires on its own.
#[tauri::command]
#[specta::specta]
pub fn dismiss_failed_operation(operation_id: String) {
    if let Ok(engine) = operations() {
        engine.dismiss_failed_operation(&OperationId::from(operation_id));
    }
}

/// Starts a retained failure again as a new operation (the queue row's Retry).
/// The failed row is dropped once the new operation starts; a refused start
/// comes back as the typed error and leaves the row. `NothingToRetry` for an id
/// with no failure, no way to start again, or a Retry already starting.
#[tauri::command]
#[specta::specta]
pub async fn retry_failed_operation(operation_id: String) -> Result<RetryOutcome, WriteOperationError> {
    // Before `install` no failure is retained, so there's nothing to retry.
    match operations() {
        Ok(engine) => engine.retry_failed_operation(&OperationId::from(operation_id)).await,
        Err(_) => Ok(RetryOutcome::NothingToRetry),
    }
}

/// Drops every retained failure. Backs the queue window's "Dismiss all".
#[tauri::command]
#[specta::specta]
pub fn dismiss_all_failed_operations() {
    if let Ok(engine) = operations() {
        engine.dismiss_all_failed_operations();
    }
}
