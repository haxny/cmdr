//! This app, acting as the operation log's host.
//!
//! [`install`] builds the app's one [`OperationLog`] first thing in `setup()`,
//! detached, so nothing can reach the log before it exists; [`start`] opens it
//! later, once the data dir and settings are known, on the SAME handle. The
//! answers the log needs from the app live here: the data dir, the recording
//! switch and the retention limits from settings, the app's runtime, and the
//! volume registry the rollback planner looks volumes up in ([`volumes`]).

mod journal;
#[cfg(test)]
mod test_journal;
mod volumes;

use std::sync::{Arc, OnceLock};

use tauri::AppHandle;

use cmdr_operation_log::OpenError;
use cmdr_operation_log::OperationLog;
use cmdr_operation_log::retention::{RetentionLimits, RetentionLimitsSource};
use cmdr_operation_log::store::operation_log_db_path;

pub(crate) use journal::{
    journal_finalize, journal_note_coverage, journal_note_not_rollbackable, journal_open, journal_record_items,
};
#[cfg(test)]
pub(crate) use test_journal::TestJournalGuard;

/// The app's operation log, once [`install`] built it.
static LOG: OnceLock<OperationLog> = OnceLock::new();

/// Builds the app's operation log, detached, and returns it.
///
/// Call early in `setup()`, before anything can journal or roll back. The
/// handle journals nothing until [`start`] opens it.
pub fn install() -> OperationLog {
    let mut built = false;
    let log = LOG.get_or_init(|| {
        built = true;
        OperationLog::detached()
    });
    if !built {
        crate::log_error!(target: "operation_log", "the operation log was installed twice; keeping the first");
    }
    // Something read the log before this ran and got a detached one: what it
    // journaled, it journaled nowhere.
    if early_reads::happened() {
        crate::log_error!(target: "operation_log", "the operation log was read before install(); early journal calls were dropped");
    }
    log.clone()
}

/// The app's operation log.
///
/// In a test or bench binary nothing installs, so the first read builds the
/// shared handle and keeps it, quietly. In the app a read before [`install`] is
/// a bug that loses journal rows, so it's loud, and it answers from a SEPARATE
/// detached handle: the slot stays free for [`install`], which then reports the
/// early read.
pub(crate) fn operation_log() -> &'static OperationLog {
    if let Some(log) = LOG.get() {
        return log;
    }
    #[cfg(any(test, feature = "transfer-bench"))]
    {
        LOG.get_or_init(OperationLog::detached)
    }
    #[cfg(not(any(test, feature = "transfer-bench")))]
    {
        early_reads::note()
    }
}

/// Open `operation-log.db` in the app's data dir on `log` and start retention.
/// Failure is non-fatal: the app runs without the journal rather than refusing
/// to start.
pub fn start(app: &AppHandle, log: &OperationLog) {
    let data_dir = match crate::config::resolved_app_data_dir(app) {
        Ok(dir) => dir,
        Err(e) => {
            log::warn!(target: "operation_log", "operation log not started: {e}");
            return;
        }
    };
    log.set_recording(crate::settings::load_operation_log_enabled(app));
    match log.open(&operation_log_db_path(&data_dir)) {
        // Prune on startup and on a slow timer, on the app's runtime. Runs before
        // the app is under load; the size loop is a no-op while under budget.
        Ok(()) => log.spawn_retention(tauri::async_runtime::handle().inner(), retention_limits(app)),
        Err(OpenError::Store(e)) => log::warn!(target: "operation_log", "operation log store not opened: {e}"),
        Err(OpenError::Writer(e)) => log::warn!(target: "operation_log", "operation log writer not spawned: {e}"),
        Err(OpenError::AlreadyOpen) => {
            crate::log_error!(target: "operation_log", "the operation log was started twice; keeping the first");
        }
    }
}

/// The retention limits from `settings.json`, read fresh on every tick so a
/// change applies without a restart.
fn retention_limits(app: &AppHandle) -> RetentionLimitsSource {
    let app = app.clone();
    Arc::new(move || {
        let limits = crate::settings::load_operation_log_retention_limits(&app);
        RetentionLimits {
            max_age_secs: limits.max_age_secs,
            max_size_bytes: limits.max_size_bytes,
        }
    })
}

#[cfg(not(any(test, feature = "transfer-bench")))]
mod early_reads {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicBool, Ordering};

    use cmdr_operation_log::OperationLog;

    static HAPPENED: AtomicBool = AtomicBool::new(false);
    static DETACHED: OnceLock<OperationLog> = OnceLock::new();

    pub(super) fn note() -> &'static OperationLog {
        if !HAPPENED.swap(true, Ordering::Relaxed) {
            crate::log_error!(target: "operation_log", "the operation log was read before operation_log_host::install(); answering detached");
        }
        DETACHED.get_or_init(OperationLog::detached)
    }

    pub(super) fn happened() -> bool {
        HAPPENED.load(Ordering::Relaxed)
    }
}

#[cfg(any(test, feature = "transfer-bench"))]
mod early_reads {
    /// The test and bench binaries build their handle on first read, so there's
    /// never an early read to report.
    pub(super) fn happened() -> bool {
        false
    }
}
