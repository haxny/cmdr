//! The terminal side-effect matrix: per outcome, which app side effects fire and
//! in what order relative to the emit (`terminal_effects.rs`), which the sink
//! adapter (`sinks.rs`) runs.
//!
//! The retained-failure half through the production effects lives with the
//! other retained-failure pins (`manager/tests/failures.rs`), which own a
//! private manager.

use std::sync::Mutex;

use super::analytics::{CompletionCategory, completion_category};
use super::terminal_effects::{self, AppTerminalEffects, TerminalEffects};
use crate::file_system::write_operations::{
    CancelRollback, WriteCancelledEvent, WriteCompleteEvent, WriteErrorEvent, WriteOperationError, WriteOperationType,
    WriteSettledEvent,
};
use crate::mcp::terminal_ops::{self, TerminalStatus};
use cmdr_fs::ignore_poison::IgnorePoison;

/// One thing a terminal event did, as the recorder saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Fired {
    Analytics(Option<CompletionCategory>),
    Terminal(TerminalStatus),
    Warned,
    Retained,
    SpaceHint,
    Emitted,
}

/// Records every effect in call order; the emit closure appends `Emitted`.
#[derive(Default)]
struct Recorder(Mutex<Vec<Fired>>);

impl Recorder {
    fn push(&self, fired: Fired) {
        self.0.lock_ignore_poison().push(fired);
    }

    fn fired(&self) -> Vec<Fired> {
        self.0.lock_ignore_poison().clone()
    }
}

impl TerminalEffects for Recorder {
    fn completion_analytics(&self, event: &WriteCompleteEvent) {
        self.push(Fired::Analytics(completion_category(event.operation_type)));
    }
    fn record_terminal(&self, _operation_id: &str, _operation_type: WriteOperationType, status: TerminalStatus) {
        self.push(Fired::Terminal(status));
    }
    fn warn_failure(&self, _event: &WriteErrorEvent) {
        self.push(Fired::Warned);
    }
    fn retain_failure(&self, _event: &WriteErrorEvent) {
        self.push(Fired::Retained);
    }
    fn expect_space_change(&self) {
        self.push(Fired::SpaceHint);
    }
}

const PROGRESSED: [WriteOperationType; 6] = [
    WriteOperationType::Copy,
    WriteOperationType::Move,
    WriteOperationType::Delete,
    WriteOperationType::Trash,
    WriteOperationType::ArchiveEdit,
    WriteOperationType::Compress,
];

fn complete_event(operation_id: &str, operation_type: WriteOperationType) -> WriteCompleteEvent {
    WriteCompleteEvent {
        operation_id: operation_id.to_string(),
        operation_type,
        files_processed: 3,
        files_skipped: 0,
        bytes_processed: 30,
        appeared_during_move: None,
        top_level_skipped: None,
        refused: None,
        metadata_left_behind: None,
        durability_unconfirmed: None,
    }
}

fn cancelled_event(operation_id: &str) -> WriteCancelledEvent {
    WriteCancelledEvent {
        operation_id: operation_id.to_string(),
        operation_type: WriteOperationType::Copy,
        files_processed: 1,
        rollback: CancelRollback::none(),
    }
}

fn error_event(operation_id: &str) -> WriteErrorEvent {
    WriteErrorEvent::new(
        operation_id.to_string(),
        WriteOperationType::Copy,
        WriteOperationError::IoError {
            path: "/Users/me/a.txt".to_string(),
            message: "disk went away".to_string(),
        },
    )
}

#[test]
fn complete_fires_its_category_then_the_ring_then_the_emit_for_every_progressed_op() {
    let expected_category = [
        CompletionCategory::FileTransfer,
        CompletionCategory::FileTransfer,
        CompletionCategory::Delete,
        CompletionCategory::Delete,
        CompletionCategory::ArchiveEdit,
        CompletionCategory::ArchiveEdit,
    ];
    for (operation_type, category) in PROGRESSED.into_iter().zip(expected_category) {
        let fx = Recorder::default();
        terminal_effects::complete(&fx, complete_event("op-complete", operation_type), |_| {
            fx.push(Fired::Emitted);
        });
        assert_eq!(
            fx.fired(),
            vec![
                Fired::Analytics(Some(category)),
                Fired::Terminal(TerminalStatus::Completed),
                Fired::Emitted
            ],
            "write-complete for {operation_type:?}"
        );
    }
}

#[test]
fn instant_ops_have_no_completion_category() {
    for operation_type in [
        WriteOperationType::Rename,
        WriteOperationType::CreateFolder,
        WriteOperationType::CreateFile,
    ] {
        assert_eq!(completion_category(operation_type), None, "{operation_type:?}");
    }
}

#[test]
fn cancel_fires_the_ring_then_the_emit_and_nothing_else() {
    let fx = Recorder::default();
    terminal_effects::cancelled(&fx, cancelled_event("op-cancel"), |_| fx.push(Fired::Emitted));
    assert_eq!(
        fx.fired(),
        vec![Fired::Terminal(TerminalStatus::Cancelled), Fired::Emitted]
    );
}

#[test]
fn error_fires_the_ring_the_warning_and_the_retained_failure_before_the_emit() {
    let fx = Recorder::default();
    terminal_effects::error(&fx, error_event("op-error"), |_| fx.push(Fired::Emitted));
    assert_eq!(
        fx.fired(),
        vec![
            Fired::Terminal(TerminalStatus::Failed),
            Fired::Warned,
            Fired::Retained,
            Fired::Emitted
        ]
    );
}

#[test]
fn settle_fires_the_space_hint_then_the_emit() {
    let fx = Recorder::default();
    let event = WriteSettledEvent {
        operation_id: "op-settle".to_string(),
        operation_type: WriteOperationType::Delete,
        volume_id: None,
    };
    terminal_effects::settled(&fx, event, |_| fx.push(Fired::Emitted));
    assert_eq!(fx.fired(), vec![Fired::SpaceHint, Fired::Emitted]);
}

/// The production effects land the MCP ring entry before the frontend hears,
/// which is what lets `await operation_complete` answer the instant the event
/// arrives. Unique ids: the ring is process-global.
#[test]
fn the_production_ring_already_holds_the_outcome_when_the_emit_runs() {
    let fx = AppTerminalEffects {
        operations: Some(crate::file_system::write_operations::installed()),
    };
    let complete_id = format!("terminal-ring-complete-{}", uuid::Uuid::new_v4());
    let mut seen = None;
    terminal_effects::complete(&fx, complete_event(&complete_id, WriteOperationType::Copy), |event| {
        seen = terminal_ops::lookup(&event.operation_id).map(|op| op.status)
    });
    assert_eq!(seen, Some(TerminalStatus::Completed));

    let cancel_id = format!("terminal-ring-cancel-{}", uuid::Uuid::new_v4());
    let mut seen = None;
    terminal_effects::cancelled(&fx, cancelled_event(&cancel_id), |event| {
        seen = terminal_ops::lookup(&event.operation_id).map(|op| op.status);
    });
    assert_eq!(seen, Some(TerminalStatus::Cancelled));
}
