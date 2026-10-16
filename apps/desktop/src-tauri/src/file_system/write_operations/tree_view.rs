//! What `get_transfer_tree_view` answers: one operation's transfer tree, as the
//! window of rows one view asked for (`transfer_tree/view.rs`).
//!
//! A separate leaf because it looks the operation up in the manager and the
//! state registry, which both sit above `transfer_tree`; inside it, the lookup
//! would weld the tree into the state's cycle.

use serde::Serialize;

use super::manager;
use super::state::operation_states;
use super::transfer_tree::{TransferTreeView, TransferTreeViewRequest};
use super::types::{ProgressAxis, WriteOperationType};

/// Why there's no tree view to answer with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum TransferTreeViewError {
    /// No operation by that id: it never existed, or it has settled and its
    /// tree went with it.
    UnknownOperation,
    /// An operation that never builds a tree: a delete, a trash, a rename, an
    /// edit inside an existing archive.
    NotATransfer,
}

/// The window `request` asks for, of the operation's tree.
///
/// `ready: false` while the operation is queued or scanning (no tree yet), so
/// a view that attaches early gets an answer instead of an error. The axis is
/// the bottleneck the operation's last `write-progress` carried, `Bytes` before
/// there was one.
pub(crate) fn transfer_tree_view(
    operation_id: &str,
    request: &TransferTreeViewRequest,
) -> Result<TransferTreeView, TransferTreeViewError> {
    let operation_type = manager::manager()
        .operation_type_of(operation_id)
        .ok_or(TransferTreeViewError::UnknownOperation)?;
    if !builds_a_tree(operation_type) {
        return Err(TransferTreeViewError::NotATransfer);
    }
    let Some(state) = operation_states().get(operation_id) else {
        // Admitted but not yet started: nothing scanned, nothing to show.
        return Ok(TransferTreeView::not_ready(ProgressAxis::Bytes));
    };
    let axis = state
        .last_progress()
        .and_then(|event| event.bottleneck)
        .unwrap_or(ProgressAxis::Bytes);
    Ok(state.transfer_tree.view(request, axis))
}

/// Copy, move, and compress build one; nothing else does.
fn builds_a_tree(operation_type: WriteOperationType) -> bool {
    matches!(
        operation_type,
        WriteOperationType::Copy | WriteOperationType::Move | WriteOperationType::Compress
    )
}
