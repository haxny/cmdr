//! How an archive edit's steps on a remote volume ride out a dropped share:
//! the wait a copy takes (`transfer/volume_outage.rs`), and the error a step
//! ends with when the wait ends without the share. Shared by the remote edit
//! (`remote.rs`) and fresh compression (`fresh_compress.rs`).

use std::future::Future;
use std::path::Path;
use std::sync::Arc;

use super::super::state::WriteOperationState;
use super::super::transfer::recovered_name::FinalizeFailure;
use super::super::transfer::staged_write::PublishFailure;
use super::super::transfer::volume::{PathRole, map_failure_after_wait, map_finalize_failure};
use super::super::transfer::volume_outage::{
    LostConnection, OutageEnd, StartedOn, lost_connection, wait_after_failure,
};
use super::super::types::WriteOperationError;
use super::edit_error::EditError;
use cmdr_fs::volume::{Volume, VolumeError};

/// Why one step stopped short.
pub(super) enum StepError {
    /// A volume's own error, which an outage wait judges. Boxed: only a
    /// failure pays for it.
    Fault(Box<Fault>),
    /// A failure on this side (a local file, a cancel, a mismatch), which ends
    /// the edit as it is.
    Ended(EditError),
}

/// A volume's error, and what to report when the wait doesn't ride it out and
/// no lost server connection explains it.
pub(super) struct Fault {
    pub(super) error: VolumeError,
    pub(super) otherwise: EditError,
}

impl StepError {
    /// A fault an outage wait judges, `otherwise` reported when it can't.
    pub(super) fn fault(error: VolumeError, otherwise: EditError) -> Self {
        Self::Fault(Box::new(Fault { error, otherwise }))
    }

    /// `error` from `role`'s volume at `path`, reported as itself when the wait
    /// doesn't ride it out.
    pub(super) fn at(path: &Path, role: PathRole, error: VolumeError) -> Self {
        let otherwise = EditError::Op(map_failure_after_wait(path, role, error.clone(), None));
        Self::fault(error, otherwise)
    }
}

/// What comes after a step's failure.
pub(super) enum Next {
    /// The share is back: run the step again.
    Again,
    Stop(EditError),
}

/// Waits out the outage behind `failed`, when that's what it was
/// (`volume_outage::wait_after_failure`, judged against the generations the
/// step started on, `outages` the item's cap). A wait that ends without the
/// share says "Lost the connection to {server}", `path` as context; any other
/// failure is reported as it always was, and a cancel as the cancel it is.
pub(super) async fn wait_out(
    state: &Arc<WriteOperationState>,
    failed: StepError,
    volumes: &[&Arc<dyn Volume>],
    started_on: &StartedOn,
    outages: &mut u32,
    path: &Path,
) -> Next {
    let (error, otherwise) = match failed {
        StepError::Ended(ended) => return Next::Stop(ended),
        StepError::Fault(fault) => (fault.error, fault.otherwise),
    };
    let ended = wait_after_failure(state, &error, volumes, started_on, outages).await;
    if ended == OutageEnd::Back {
        return Next::Again;
    }
    if ended == OutageEnd::Cancelled || state.intent().is_cancelled() {
        return Next::Stop(EditError::Cancelled);
    }
    Next::Stop(match lost_connection(ended, &error, volumes) {
        Some(lost) => EditError::Op(map_failure_after_wait(path, PathRole::Destination, error, Some(lost))),
        None => otherwise,
    })
}

/// Runs `step`, which must be safe to run twice (a read, or a write that
/// starts over from its first byte), and when it fails on the transport while
/// one of `volumes` is out, waits for it and runs it again ([`wait_out`]).
pub(super) async fn rerun_through_outages<T, F, Fut>(
    state: &Arc<WriteOperationState>,
    volumes: &[&Arc<dyn Volume>],
    path: &Path,
    mut step: F,
) -> Result<T, EditError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, StepError>>,
{
    let mut outages = 0;
    loop {
        let started_on = StartedOn::now(volumes);
        let failed = match step().await {
            Ok(value) => return Ok(value),
            Err(failed) => failed,
        };
        match wait_out(state, failed, volumes, &started_on, &mut outages, path).await {
            Next::Again => {}
            Next::Stop(ended) => return Err(ended),
        }
    }
}

/// A landing that ended without the bytes at `path`: where the new bytes went
/// when the way was cleared (that outranks everything), else the lost
/// connection, else the destination's own refusal.
pub(super) fn landing_failure(path: &Path, failure: FinalizeFailure, lost: Option<LostConnection>) -> EditError {
    EditError::Op(if failure.aftermath.is_some() {
        map_finalize_failure(path, failure)
    } else {
        map_failure_after_wait(path, PathRole::Destination, failure.error, lost)
    })
}

/// A whole-publish that ended without the bytes at `path`: the original kept
/// beside its name when it couldn't go back (that outranks everything), with
/// the lost connection or the refusal as its cause.
pub(super) fn publish_failure(path: &Path, failure: PublishFailure, lost: Option<LostConnection>) -> EditError {
    let cause = map_failure_after_wait(path, PathRole::Destination, failure.error, lost);
    EditError::Op(match failure.recovered {
        Some(recovered) => WriteOperationError::OriginalsKeptAside {
            cause: Box::new(cause),
            recovered: vec![recovered],
        },
        None => cause,
    })
}
