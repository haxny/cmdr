//! This app, acting as the write engine's host.
//!
//! The mirror of `file_system::write_operations::host`: that module declares
//! what the engine needs from the app around it, and this is where the app
//! answers, once, in `setup()`, by building the app's one engine
//! ([`operations`]).
//!
//! - runtime — the volume host's (`volume_host::host()`), which is the app's own
//!   tokio runtime, so an admitted operation never runs on a caller's
//! - the journal — the app's one `OperationLog`, handed in by `install`
//! - volumes — [`volumes::AppOperationVolumes`], the volume registry asked per
//!   call
//! - engine-wide events — [`events::TauriEngineEvents`]
//! - own writes — [`own_writes::AppOwnWrites`], the downloads watcher's ignore
//!   set
//! - the drive index — [`index::AppOperationIndex`], `index_host::index()` asked
//!   per call
//! - E2E pacing — [`pacing::AppPacing`], `test_mode`'s throttles
//! - name order — [`name_order::AppNameOrder`], the panes' collator
//! - one operation's or preview's events — [`TauriEventSink`] and
//!   [`TauriScanPreviewSink`] (`sinks.rs`), built at the IPC edge; a terminal
//!   event's app side effects ride along (`terminal_effects.rs`)
//! - the wire — `wire.rs` wraps each engine payload as the Tauri event it
//!   travels as

mod analytics;
mod events;
mod index;
mod name_order;
mod own_writes;
mod pacing;
mod sinks;
pub(crate) mod terminal_effects;
mod volumes;
// Which app side effects each terminal event fires, and in what order around its emit.
#[cfg(test)]
mod terminal_effects_tests;
pub(crate) mod wire;

use std::sync::{Arc, OnceLock};

use cmdr_operation_log::OperationLog;
use tauri::AppHandle;

use crate::file_system::write_operations::{self, OperationEventSink, Operations, OperationsBuilder};
pub(crate) use events::VolumesBusyChanged;
pub use sinks::{TauriEventSink, TauriScanPreviewSink};

/// The app's write engine, once [`install`] built it.
static OPERATIONS: OnceLock<Operations> = OnceLock::new();

/// Why [`operations`] has no engine to hand out: [`install`] hasn't run.
///
/// Unreachable in the app, which installs at the top of `setup()`; a caller
/// that meets it refuses rather than starting anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NotInstalled;

/// Builds the app's write engine, wired to this app, journaling into
/// `operation_log` (the handle `operation_log_host::install` returned, never a
/// second `operation_log()` read).
///
/// Call once, in `setup()`, unconditionally: after `volume_host::install` (the
/// engine's runtime is the volume host's) and `operation_log_host::install`, and
/// before the leftover sweep, `init_volume_manager`, or anything that can start
/// an operation. A second call keeps the first engine.
pub fn install(app: &AppHandle, operation_log: OperationLog) {
    let sink_app = app.clone();
    let engine = app_answers(Some(app.clone()), operation_log)
        .operation_sinks(Box::new(move || {
            Arc::new(TauriEventSink::new(sink_app.clone())) as Arc<dyn OperationEventSink>
        }))
        .build();
    if OPERATIONS.set(engine.clone()).is_err() {
        crate::log_error!(target: "op_manager", "the write engine was installed twice; keeping the first");
        return;
    }
    // The engine's own slot, for the free functions the app still calls.
    // shim: removed in M7a
    if write_operations::install(engine).is_err() {
        crate::log_error!(target: "op_manager", "the write engine's shim slot was already filled; keeping the first");
    }
}

/// The app's write engine.
///
/// `Err(NotInstalled)` before [`install`]: a caller refuses with a variant the
/// frontend already handles and raises an error report (see
/// [`refuse_not_installed`]); it never falls back to a detached engine, which
/// would write with nobody hearing. In a test binary nothing installs, so the
/// first read shares the engine the engine's own tests use.
pub(crate) fn operations() -> Result<&'static Operations, NotInstalled> {
    operations_in(&OPERATIONS)
}

/// [`operations`] over a given slot, so a test can hand an empty one in.
pub(crate) fn operations_in(slot: &'static OnceLock<Operations>) -> Result<&'static Operations, NotInstalled> {
    if let Some(engine) = slot.get() {
        return Ok(engine);
    }
    #[cfg(test)]
    if std::ptr::eq(slot, &OPERATIONS) {
        return Ok(OPERATIONS.get_or_init(|| write_operations::installed().clone()));
    }
    Err(NotInstalled)
}

/// What a start refused before `install` answers: the frontend's generic I/O
/// failure, which its error dialog renders without naming a file, plus an error
/// report, since the app can't reach this after `setup()`.
pub(crate) fn refuse_not_installed(_: NotInstalled) -> write_operations::WriteOperationError {
    crate::log_error!(target: "op_manager", "an operation was started before the write engine was installed; refused");
    write_operations::WriteOperationError::IoError {
        path: String::new(),
        message: "the write engine isn't ready yet".to_string(),
    }
}

/// The engine a test binary shares, since nothing there calls [`install`]: the
/// app's answers with no frontend behind them (the real indexing gauge,
/// analytics client, and error reporter, which `status_cache_tests`' gauge pins
/// read), the test binary's shared operation log (what `TestJournalGuard` fills), and
/// no sink factory.
#[cfg(test)]
pub(crate) fn test_engine() -> Operations {
    app_answers(None, crate::operation_log_host::operation_log().clone()).build()
}

/// Every seam the app answers the same with or without a frontend; `app` is
/// where the engine-wide events go out, if anywhere, and `operation_log` the
/// journal it records into.
fn app_answers(app: Option<AppHandle>, operation_log: OperationLog) -> OperationsBuilder {
    Operations::builder(
        Arc::new(volumes::AppOperationVolumes),
        Arc::new(events::TauriEngineEvents::new(app)),
    )
    .volume_host(crate::volume_host::host())
    .operation_log(operation_log)
    .own_writes(Arc::new(own_writes::AppOwnWrites))
    .index(Arc::new(index::AppOperationIndex))
    .pacing(Arc::new(pacing::AppPacing))
    .name_order(Arc::new(name_order::AppNameOrder))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use cmdr_fs::ignore_poison::IgnorePoison;
    use cmdr_operation_log::OperationLog;
    use cmdr_operation_log::capture::CapturingJournal;
    use cmdr_operation_log::types::{ExecutionStatus, Initiator, OpKind};
    use cmdr_operation_log::writer::OpenOperation;

    use crate::file_system::write_operations::Operations;

    fn header(op_id: &str) -> OpenOperation {
        OpenOperation {
            op_id: op_id.to_string(),
            kind: OpKind::Copy,
            initiator: Initiator::User,
            source_volume_id: None,
            dest_volume_id: None,
            item_count: 1,
            started_at: 0,
            rolls_back_op_id: None,
            execution_status: ExecutionStatus::Running,
        }
    }

    /// Whether `engine` journals into `log`: an operation opened on the engine is
    /// heard by a journal installed on `log`, so the two are one handle.
    fn journals_into(engine: &Operations, log: &OperationLog) -> bool {
        let journal = Arc::new(CapturingJournal::new());
        log.set_journal(Some(journal.clone()));
        drop(engine.operation_log().journal_open(header("same-log-probe")));
        let heard = journal.opens.lock_ignore_poison().len() == 1;
        log.set_journal(None);
        heard
    }

    /// The engine `install` builds journals into the very handle
    /// `operation_log_host::install` returned, never a second one.
    #[test]
    fn the_installed_engine_journals_into_the_log_it_was_handed() {
        let log = OperationLog::detached();
        let engine = super::app_answers(None, log.clone()).build();
        assert!(journals_into(&engine, &log));
    }

    /// The test binary's shared engine journals into `operation_log()`, which is
    /// what `TestJournalGuard` fills.
    #[test]
    fn the_shared_test_engine_journals_into_operation_log() {
        let journal = Arc::new(CapturingJournal::new());
        let _guard = crate::operation_log_host::TestJournalGuard::install(journal.clone());
        let engine = super::operations().expect("a test binary always has its engine");
        drop(engine.operation_log().journal_open(header("shared-log-probe")));
        assert_eq!(journal.opens.lock_ignore_poison().len(), 1);
    }
}
