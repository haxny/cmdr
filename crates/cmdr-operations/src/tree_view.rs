//! What `get_transfer_tree_view` answers: one operation's transfer tree, as the
//! window of rows one view asked for (`transfer_tree/view.rs`).
//!
//! A separate leaf because it looks the operation up in the manager and the
//! state registry, which both sit above `transfer_tree`; inside it, the lookup
//! would weld the tree into the state's cycle.

use serde::Serialize;

use super::Operations;
use super::transfer_tree::{TransferTreeView, TransferTreeViewRequest};
use super::types::{ProgressAxis, WriteOperationType};
use cmdr_operation_log::types::OperationId;

/// Why there's no tree view to answer with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransferTreeViewError {
    /// No operation by that id: it never existed, or it has settled and its
    /// tree was released or expired (`finished_trees.rs`).
    UnknownOperation,
    /// An operation that never builds a tree: a delete, a trash, a rename, an
    /// edit inside an existing archive.
    NotATransfer,
}

impl Operations {
    /// The engine's operation trees: a window of one, and letting a settled
    /// one go.
    pub fn transfer_trees(&self) -> TransferTrees<'_> {
        TransferTrees { engine: self }
    }
}

/// What a view asks of the engine's transfer trees (`Operations::transfer_trees`).
pub struct TransferTrees<'a> {
    engine: &'a Operations,
}

impl TransferTrees<'_> {
    /// The window `request` asks for, of the operation's tree.
    ///
    /// `ready: false` while the operation is queued or scanning (no tree yet), so
    /// a view that attaches early gets an answer instead of an error. The axis is
    /// the bottleneck the operation's last `write-progress` carried, `Bytes` before
    /// there was one. A settled operation answers from its kept tree until
    /// [`release`](Self::release) or its TTL (`finished_trees.rs`).
    pub fn view(
        &self,
        operation_id: &OperationId,
        request: &TransferTreeViewRequest,
    ) -> Result<TransferTreeView, TransferTreeViewError> {
        let manager = self.engine.manager();
        // A settled operation's kept tree first: the manager hands it over
        // BEFORE it drops the record, so one of the two always answers.
        if let Some(view) = manager.finished_trees().view(operation_id, request) {
            return Ok(view);
        }
        let operation_type = manager
            .operation_type_of(operation_id)
            .ok_or(TransferTreeViewError::UnknownOperation)?;
        if !builds_a_tree(operation_type) {
            return Err(TransferTreeViewError::NotATransfer);
        }
        let Some(state) = self.engine.registry().get(operation_id) else {
            // Admitted but not yet started: nothing scanned, nothing to show.
            return Ok(TransferTreeView::not_ready(ProgressAxis::Bytes));
        };
        let axis = state
            .last_progress()
            .and_then(|event| event.bottleneck)
            .unwrap_or(ProgressAxis::Bytes);
        Ok(state.transfer_tree.view(request, axis))
    }

    /// The dialog showing `operation_id`'s tree closed: a settled operation's
    /// kept tree goes now rather than at its TTL (`finished_trees.rs`). A no-op
    /// for a running operation, whose tree is its own until it settles.
    pub fn release(&self, operation_id: &OperationId) {
        self.engine.manager().finished_trees().release(operation_id);
    }
}

/// Copy, move, and compress build one; nothing else does.
fn builds_a_tree(operation_type: WriteOperationType) -> bool {
    matches!(
        operation_type,
        WriteOperationType::Copy | WriteOperationType::Move | WriteOperationType::Compress
    )
}
