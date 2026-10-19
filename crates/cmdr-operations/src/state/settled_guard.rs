//! The guard that tells the frontend an operation's background work is over.

use std::sync::Arc;

use super::super::host::OperationEventSink;
use super::super::types::{WriteOperationType, WriteSettledEvent};

/// RAII guard that emits exactly one `write-settled` event when dropped.
///
/// Place at the top of every write-op spawned task. Whatever way the task
/// exits — happy path, error path, cancel, or panic via `JoinError` propagation
/// — the guard's `Drop` impl fires, so the FE always learns "this op's
/// background work is fully torn down; the volume is ready for the next op."
///
/// The guard takes the same injected `Arc<dyn OperationEventSink>` the rest of
/// the pipeline uses (built at the IPC edge as `TauriEventSink`, or a
/// `CollectorEventSink` in tests), so the full spawn-task lifecycle runs with no
/// Tauri runtime under test.
pub(crate) struct WriteSettledGuard {
    inner: Option<WriteSettledGuardInner>,
}

struct WriteSettledGuardInner {
    sink: Arc<dyn OperationEventSink>,
    operation_id: String,
    operation_type: WriteOperationType,
    volume_id: Option<String>,
}

impl WriteSettledGuard {
    /// Builds a guard that emits `write-settled` via `sink.emit_settled(...)`
    /// on drop.
    pub(crate) fn new(
        sink: Arc<dyn OperationEventSink>,
        operation_id: impl Into<String>,
        operation_type: WriteOperationType,
        volume_id: Option<String>,
    ) -> Self {
        Self {
            inner: Some(WriteSettledGuardInner {
                sink,
                operation_id: operation_id.into(),
                operation_type,
                volume_id,
            }),
        }
    }
}

impl Drop for WriteSettledGuard {
    fn drop(&mut self) {
        let Some(inner) = self.inner.take() else { return };
        inner.sink.emit_settled(WriteSettledEvent {
            operation_id: inner.operation_id,
            operation_type: inner.operation_type,
            volume_id: inner.volume_id,
        });
    }
}
