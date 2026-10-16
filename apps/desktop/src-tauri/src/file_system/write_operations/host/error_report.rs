//! What went wrong badly enough that a person should hear about it in an error
//! report, as typed facts the app re-raises.
//!
//! The engine never calls `log_error!`: that macro is the app's (it lives at the
//! app crate's root and carries the report dispatch), so the engine names what
//! happened and `EngineEvents::error_report` hands it over. Cmdr's answer
//! re-raises each one through `log_error!` with the target and words it always
//! had.

use std::path::PathBuf;
use std::time::Duration;

use super::super::types::WriteOperationError;

/// One error-level fact, grouped by what it means for the person.
#[derive(Debug)]
pub(crate) enum OperationErrorReport {
    // An operation failed, and the person already sees it. The report is the
    // signal opt-in users send on "this didn't work".
    /// A local copy failed and kept the `kept_files` it had landed.
    LocalCopyFailed {
        operation_id: String,
        error: WriteOperationError,
        kept_files: usize,
    },
    /// A copy between volumes (Local, SMB, MTP, …) failed.
    VolumeCopyFailed {
        operation_id: String,
        error: WriteOperationError,
    },

    // A file of the person's is still safe, but not where they left it.
    /// A displaced original couldn't take a name back, so it stays on disk under
    /// its aside name, with its record, for the sweep to answer for.
    AsideStranded {
        attempt: AsideAttempt,
        aside: PathBuf,
        /// The OS's words for the refusal.
        error: String,
    },

    // A transfer is stuck on a backend that stopped answering.
    /// The probe gave up on a task that moved no bytes for `still_for_secs`:
    /// the write is abandoned, and the file runs again if it has attempts left.
    StalledTaskAbandoned {
        operation_id: String,
        still_for_secs: u64,
        /// The task as the probe renders it.
        task: String,
    },
    /// A cancelled volume copy left `tasks` in-flight tasks behind that didn't
    /// wind down within `deadline`.
    WindDownAbandoned {
        operation_id: String,
        tasks: usize,
        deadline: Duration,
        /// The probe's dump of the wedged tasks, when the op had a probe.
        probe_dump: Option<String>,
    },

    // A bug, contained: the operation carries on, or its lanes are freed.
    /// The transfer tree stopped for this operation; the transfer carries on.
    TreeAbandoned { cause: TreeFailure },
    /// The admission pass found a queued op with no deferred start, and freed
    /// its lanes.
    AdmittedWithoutStart { operation_id: String },
}

/// Which attempt to give a displaced original a name back failed.
#[derive(Debug)]
pub(crate) enum AsideAttempt {
    /// Putting it back at `original` after a staged write didn't land
    /// (`stage_and_land_file`).
    LandingRestore { original: PathBuf },
    /// Putting it back at `original` (`DisplacedEntry::restore`).
    DisplacedRestore { original: PathBuf },
    /// Putting it back at `original` after a folder overwrite failed
    /// (`safe_overwrite_dir`).
    DirOverwriteRestore { original: PathBuf },
    /// Giving it a ` (recovered)` name beside the folder that took its own
    /// (`DisplacedEntry::keep_as_recovered_sibling`).
    RecoveredName,
}

/// Why a transfer tree stopped.
#[derive(Debug)]
pub(crate) enum TreeFailure {
    /// A progress report into a live tree panicked.
    ReportPanicked,
    /// Building the tree panicked during `step`.
    BuildPanicked { step: &'static str },
}
