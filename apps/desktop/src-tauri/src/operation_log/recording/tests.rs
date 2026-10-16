//! The `operationLog.enabled` switch, driven through the same free functions the
//! write pipeline calls, over a real temp-DB `WriterJournal`. Each test holds the
//! one `TestJournalGuard`, which also resets the switch on both ends, so a test
//! that turns recording off can't leak it into the next.

use std::sync::Arc;

use super::set_enabled;
use crate::file_system::VolumeManager;
use crate::operation_log::capture::{FinalizeInputs, WriterJournal};
use crate::operation_log::rollback::{RollbackRefusal, rollback_operation};
use crate::operation_log::store::{open_read_connection, operation_log_db_path, read_operation, read_operation_items};
use crate::operation_log::types::{EntryType, ExecutionStatus, Initiator, ItemOutcome, OpKind, RollbackState, RowRole};
use crate::operation_log::writer::{JournalItem, OpenOperation, OperationLogWriter};
use crate::operation_log::{TestJournalGuard, journal_finalize, journal_open, journal_record_items};

struct Rig {
    _dir: tempfile::TempDir,
    writer: OperationLogWriter,
    path: std::path::PathBuf,
    _guard: TestJournalGuard,
}

impl Rig {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = operation_log_db_path(dir.path());
        let writer = OperationLogWriter::spawn(&path).expect("spawn writer");
        let guard = TestJournalGuard::install(Arc::new(WriterJournal::new(writer.clone())));
        Self {
            _dir: dir,
            writer,
            path,
            _guard: guard,
        }
    }

    fn row_count(&self, op_id: &str) -> Option<usize> {
        self.writer.flush_blocking().expect("flush");
        let conn = open_read_connection(&self.path).expect("read conn");
        read_operation(&conn, op_id).expect("read")?;
        Some(read_operation_items(&conn, op_id, 100).expect("items").len())
    }

    fn rollback_state(&self, op_id: &str) -> Option<RollbackState> {
        self.writer.flush_blocking().expect("flush");
        let conn = open_read_connection(&self.path).expect("read conn");
        read_operation(&conn, op_id)
            .expect("read")
            .map(|row| row.rollback_state)
    }
}

fn open(op_id: &str) {
    journal_open(OpenOperation {
        op_id: op_id.to_string(),
        kind: OpKind::Copy,
        initiator: Initiator::User,
        source_volume_id: Some("root".to_string()),
        dest_volume_id: Some("root".to_string()),
        item_count: 2,
        started_at: 1,
        rolls_back_op_id: None,
        execution_status: ExecutionStatus::Running,
    });
}

fn record(op_id: &str, name: &str) {
    journal_record_items(
        op_id,
        vec![JournalItem {
            seq: 0,
            entry_type: EntryType::File,
            row_role: RowRole::RollbackUnit,
            source_volume_id: "root".to_string(),
            source_dir: "/src".to_string(),
            source_name: name.to_string(),
            dest_volume_id: Some("root".to_string()),
            dest_dir: Some("/dst".to_string()),
            dest_name: Some(name.to_string()),
            size: Some(10),
            mtime: Some(100),
            outcome: ItemOutcome::Done,
            overwrote: false,
        }],
    );
}

fn finalize(op_id: &str) -> u64 {
    journal_finalize(
        op_id,
        FinalizeInputs {
            execution_status: ExecutionStatus::Done,
            kind: OpKind::Copy,
            archive_subkind: None,
            net_new: false,
            ended_at: 2,
            item_count: None,
            items_done: 2,
            bytes_total: 20,
            dev_summary: None,
        },
    )
    .rollback_unit_rows
}

fn run_whole(op_id: &str) -> u64 {
    open(op_id);
    record(op_id, "a.txt");
    record(op_id, "b.txt");
    finalize(op_id)
}

#[test]
fn an_operation_run_while_off_writes_no_rows() {
    let rig = Rig::new();
    set_enabled(false);

    assert_eq!(run_whole("op-off"), 0);
    assert_eq!(rig.row_count("op-off"), None, "no header and no item rows");
}

#[test]
fn turning_recording_back_on_journals_the_next_operation() {
    let rig = Rig::new();
    set_enabled(false);
    run_whole("op-off");
    set_enabled(true);

    assert_eq!(run_whole("op-on"), 2);
    assert_eq!(rig.row_count("op-on"), Some(2));
    assert_eq!(rig.row_count("op-off"), None);
}

#[test]
fn an_operation_already_recording_when_switched_off_is_journaled_whole() {
    let rig = Rig::new();
    open("op-mid");
    record("op-mid", "a.txt");
    set_enabled(false);
    record("op-mid", "b.txt");

    assert_eq!(finalize("op-mid"), 2);
    assert_eq!(rig.row_count("op-mid"), Some(2), "every item, never half");
    assert_eq!(rig.rollback_state("op-mid"), Some(RollbackState::Rollbackable));
}

#[test]
fn an_operation_started_while_off_stays_unjournaled_when_switched_on() {
    let rig = Rig::new();
    set_enabled(false);
    open("op-late");
    record("op-late", "a.txt");
    set_enabled(true);
    record("op-late", "b.txt");

    assert_eq!(finalize("op-late"), 0);
    assert_eq!(rig.row_count("op-late"), None, "no rows at all, never a tail");
}

#[test]
fn an_operation_run_while_off_is_not_offered_for_undo() {
    let rig = Rig::new();
    set_enabled(false);
    run_whole("op-off");

    let refusal = rollback_operation(&VolumeManager::new(), &rig.writer, "op-off", |_| Ok(())).unwrap_err();
    assert_eq!(refusal, RollbackRefusal::UnknownOperation);
}

#[test]
fn history_recorded_before_switching_off_stays_undoable() {
    let rig = Rig::new();
    run_whole("op-before");
    set_enabled(false);

    assert_eq!(rig.row_count("op-before"), Some(2));
    assert_eq!(rig.rollback_state("op-before"), Some(RollbackState::Rollbackable));
}
