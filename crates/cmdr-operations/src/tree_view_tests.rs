//! A finished operation's tree stays answerable while its dialog shows it.
//!
//! #702: a server-side copy of 36 files settled in well under a second, before
//! the progress dialog's throttled pull came back, and the operation's tree
//! went with it. The dialog kept the rows it had pulled first: every file
//! waiting, at 0 bytes, under a "Copied 36 files" title.

use std::path::{Path, PathBuf};
use std::time::Duration;

use cmdr_operation_log::types::OperationId;

use super::manager::{ManagedTaskGuard, OperationDescriptor, OperationSummaryText};
use super::testing::TestOperations;
use super::transfer_tree::test_support::english;
use super::transfer_tree::{TransferTreeViewRequest, TreeLanding, TreeRowState, from_sources};
use super::tree_view::TransferTreeViewError;
use super::types::{RollbackSupport, WriteOperationType};
use cmdr_fs::testing::wait_until_async;

const WAIT: Duration = Duration::from_secs(5);

/// Runs a copy of `/a.txt` and `/b.txt` that lands both files and settles, the
/// way a real one does, with nobody having viewed its tree yet.
async fn run_copy_to_settle(engine: &TestOperations, id: &str) -> OperationId {
    let op = OperationId::from(id);
    let state = engine.new_state();
    let sources = [PathBuf::from("/a.txt"), PathBuf::from("/b.txt")];
    let tree = from_sources(&sources, |_| false, &english());
    state.transfer_tree.install(tree, TreeLanding::nowhere());
    let descriptor = OperationDescriptor {
        operation_id: op.clone(),
        operation_type: WriteOperationType::Copy,
        lanes: Vec::new(),
        volume_ids: Vec::new(),
        summary: OperationSummaryText::default(),
        rollback_support: RollbackSupport::Supported,
        preview_id: None,
        reverses: None,
    };
    let settling = op.clone();
    let landed = std::sync::Arc::clone(&state);
    engine.manager().spawn_managed(
        descriptor,
        state,
        Box::new(move |ops: crate::Operations| {
            Box::pin(async move {
                let guard = ManagedTaskGuard::new(ops.manager(), settling.clone());
                let reporter = landed.transfer_tree.reporter();
                for source in ["/a.txt", "/b.txt"] {
                    reporter.leaf(reporter.source(Path::new(source))).complete();
                }
                guard.disarm();
                ops.manager().on_settled(&settling);
            })
        }),
    );
    wait_until_async(WAIT, "the copy to settle", || {
        engine.manager().lifecycle_status(&op).is_none()
    })
    .await;
    op
}

fn row_states(engine: &TestOperations, op: &OperationId) -> Result<Vec<(String, TreeRowState)>, TransferTreeViewError> {
    let view = engine
        .transfer_trees()
        .view(op, &TransferTreeViewRequest::following_the_work())?;
    Ok(view.rows.into_iter().map(|row| (row.label, row.state)).collect())
}

/// ❗ The rows a dialog pulls after the settle are the final ones.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_settled_copy_still_answers_its_final_rows() {
    let engine = TestOperations::new();
    let op = run_copy_to_settle(&engine, "tree-after-settle").await;

    assert_eq!(
        row_states(&engine, &op),
        Ok(vec![
            ("a.txt".to_owned(), TreeRowState::Done),
            ("b.txt".to_owned(), TreeRowState::Done),
        ])
    );
}

/// The dialog closing lets the tree go, so its memory lasts no longer than the
/// dialog showing it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn releasing_a_settled_tree_lets_it_go() {
    let engine = TestOperations::new();
    let op = run_copy_to_settle(&engine, "tree-released").await;
    assert!(row_states(&engine, &op).is_ok(), "kept until released");

    engine.transfer_trees().release(&op);

    assert_eq!(row_states(&engine, &op), Err(TransferTreeViewError::UnknownOperation));
}

/// A release that never comes (a window that went away mid-close) can't keep
/// a tree forever: it goes on its own after the safety TTL.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tree_nobody_releases_goes_after_its_ttl() {
    let engine = TestOperations::new();
    engine
        .manager()
        .finished_trees()
        .set_ttl_for_test(Duration::from_millis(500));
    let op = run_copy_to_settle(&engine, "tree-ttl").await;
    assert!(row_states(&engine, &op).is_ok(), "kept until the TTL");

    wait_until_async(WAIT, "the kept tree to expire", || {
        row_states(&engine, &op) == Err(TransferTreeViewError::UnknownOperation)
    })
    .await;
}
