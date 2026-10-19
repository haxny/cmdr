//! The rig both local-move suites drive the engines through: an operation state
//! with a real progress throttle, and one call-through per engine so a test can
//! name the move kind it means.

use super::cross_fs::move_with_staging;
use super::*;
use crate::event_sinks::CollectorEventSink;
use crate::testing::TestOperations;
use crate::types::{ConflictResolution, LockedSourcesAnswer};
use cmdr_fs::ignore_poison::IgnorePoison;

pub(crate) fn make_state(engine: &TestOperations, progress_interval_ms: u64) -> Arc<WriteOperationState> {
    Arc::new(engine.new_operation_state(std::time::Duration::from_millis(progress_interval_ms)))
}

/// A sink that answers a Locked-items question with `answer`, from inside its
/// emit, the way a surface may.
pub(crate) fn answering_sink(state: &Arc<WriteOperationState>, answer: LockedSourcesAnswer) -> Arc<CollectorEventSink> {
    let events = Arc::new(CollectorEventSink::new());
    let state = Arc::clone(state);
    events.answer_locked_with(move |_| {
        state.locked_sources.answer(answer);
    });
    events
}

/// Drives a same-FS local move (`move_with_rename`) of `sources` into `dst_dir`
/// under the given resolution. Within one tempdir, source and dest share a
/// filesystem, so `move_files_with_progress_inner` routes to the rename path. A
/// Locked-items question is answered Move anyway, which keeps the move whole.
pub(in crate::transfer) fn run_same_fs_move(
    engine: &TestOperations,
    sources: &[PathBuf],
    dst_dir: &Path,
    resolution: ConflictResolution,
    op_id: &str,
) -> Result<Arc<CollectorEventSink>, WriteOperationError> {
    run_same_fs_move_answering(
        engine,
        sources,
        dst_dir,
        resolution,
        op_id,
        LockedSourcesAnswer::MoveAnyway,
    )
}

/// [`run_same_fs_move`], answering a Locked-items question with `answer`.
pub(in crate::transfer) fn run_same_fs_move_answering(
    engine: &TestOperations,
    sources: &[PathBuf],
    dst_dir: &Path,
    resolution: ConflictResolution,
    op_id: &str,
    answer: LockedSourcesAnswer,
) -> Result<Arc<CollectorEventSink>, WriteOperationError> {
    let state = make_state(engine, 200);
    let events = answering_sink(&state, answer);
    let config = WriteOperationConfig {
        conflict_resolution: resolution,
        ..WriteOperationConfig::default()
    };
    move_files_with_progress_inner(&*events, op_id, &state, sources, dst_dir, &config)?;
    Ok(events)
}

/// Drives a cross-FS local move (`move_with_staging`) of `sources` into
/// `dst_dir` under the given resolution. A Locked-items question is answered
/// Move anyway, like [`run_same_fs_move`].
pub(crate) fn run_cross_fs_move(
    engine: &TestOperations,
    sources: &[PathBuf],
    dst_dir: &Path,
    resolution: ConflictResolution,
    op_id: &str,
) -> Result<Arc<CollectorEventSink>, WriteOperationError> {
    run_cross_fs_move_on(&make_state(engine, 200), sources, dst_dir, resolution, op_id)
}

/// [`run_cross_fs_move`] under a state the caller built (on an engine of its own).
pub(crate) fn run_cross_fs_move_on(
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    dst_dir: &Path,
    resolution: ConflictResolution,
    op_id: &str,
) -> Result<Arc<CollectorEventSink>, WriteOperationError> {
    run_cross_fs_move_answering(
        state,
        sources,
        dst_dir,
        resolution,
        op_id,
        LockedSourcesAnswer::MoveAnyway,
    )
}

/// [`run_cross_fs_move_on`], answering a Locked-items question with `answer`.
pub(crate) fn run_cross_fs_move_answering(
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    dst_dir: &Path,
    resolution: ConflictResolution,
    op_id: &str,
    answer: LockedSourcesAnswer,
) -> Result<Arc<CollectorEventSink>, WriteOperationError> {
    run_cross_fs_move_with(
        state,
        answering_sink(state, answer),
        sources,
        dst_dir,
        resolution,
        op_id,
    )
}

/// [`run_cross_fs_move_on`], reporting to `events` (which answers any question).
pub(crate) fn run_cross_fs_move_with(
    state: &Arc<WriteOperationState>,
    events: Arc<CollectorEventSink>,
    sources: &[PathBuf],
    dst_dir: &Path,
    resolution: ConflictResolution,
    op_id: &str,
) -> Result<Arc<CollectorEventSink>, WriteOperationError> {
    let config = WriteOperationConfig {
        conflict_resolution: resolution,
        ..WriteOperationConfig::default()
    };
    move_with_staging(&*events, op_id, state, sources, dst_dir, &config, 0)?;
    Ok(events)
}

/// The outcomes this run reported for `path`, in emit order. The LAST one is the
/// operation's verdict on that source (`types::SourceItemOutcome`).
pub(super) fn outcomes_for(events: &CollectorEventSink, path: &Path) -> Vec<SourceItemOutcome> {
    events
        .source_items_done
        .lock_ignore_poison()
        .iter()
        .filter(|e| e.source_path == path.display().to_string())
        .map(|e| e.outcome)
        .collect()
}

/// The `source_removed` flags this run reported for `path`, in emit order.
pub(super) fn removal_flags_for(events: &CollectorEventSink, path: &Path) -> Vec<bool> {
    events
        .source_items_done
        .lock_ignore_poison()
        .iter()
        .filter(|e| e.source_path == path.display().to_string())
        .map(|e| e.source_removed)
        .collect()
}
