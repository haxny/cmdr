//! How a staged write's completed temp takes its final name ([`land`]), what
//! the in-flight ledgers hear as it does ([`Landing`]), and how a landing the
//! transport cut off is finished once the destination is back
//! ([`CutOffLanding`]). Split out of `staged_write.rs`, which owns where a write
//! goes; this owns how it arrives.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::super::in_flight_temps::{self, TrackedRecord};
use super::super::super::journal;
use super::super::super::state::WriteOperationState;
use super::super::interrupted_landing::holds_the_source;
use super::super::recovered_name::{
    AfterARefusedDelete, FinalizeFailure, LandingAftermath, discard_unplaced_temp, rescue_out_of_temp_space,
    what_a_refused_delete_left,
};
use super::{LandingName, home_of};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::staging::StagingTemp;
use cmdr_fs::volume::{Volume, VolumeError};
use cmdr_operation_log::types::NotRollbackableReason;

/// Why a landing didn't give the bytes their name.
#[derive(Debug)]
pub(crate) enum LandingError {
    /// It ended: whatever it left is settled in the ledgers, and the failure
    /// says where the new bytes are when the way was cleared.
    Failed(FinalizeFailure),
    /// A step met a transport fault (`VolumeError::transport_fault`), so the
    /// server may or may not have performed it. Nothing is settled yet: wait
    /// for the destination, then [`CutOffLanding::resume`], or
    /// [`CutOffLanding::give_up`] when the wait ends without it.
    CutOff(Box<CutOffLanding>),
}

/// One staged write's landing: its temp, where it's going, and where the temp
/// stands in the in-flight ledgers, told each time the landing changes what
/// the temp is.
///
/// ❗ Dropped mid-way (a task the concurrent driver let go, a cancel's drain
/// deadline), a temp recorded as a landing stays recorded and is handed to
/// the volume's next arrival, which finishes the landing: from the clearing
/// delete on it may be the destination's only copy. A temp still recorded as a
/// temp stays in the operation's in-flight set for the abandoned-write sweep,
/// which is safe: nothing was cleared, and the source holds the bytes.
pub(super) struct Landing {
    state: Arc<WriteOperationState>,
    /// The temp, kept out of the pane until the landing settles
    /// (`cmdr_fs::staging`).
    temp: StagingTemp,
    final_path: PathBuf,
    name: LandingName,
    /// The byte count the write confirmed: what a whole temp holds.
    size: u64,
    /// The volume whose path space the temp is recorded in (the staged
    /// write's).
    home_volume: Option<String>,
    recorded: Recorded,
    /// Set by every discard of the temp this landing asks for: a temp gone
    /// from the destination is proof of a landing only when nothing of ours
    /// deleted it.
    discard_asked: bool,
}

/// Where a landing's temp stands in the in-flight ledgers.
enum Recorded {
    /// A removable copy of ours: recorded as a temp and in the operation's
    /// in-flight set, so every sweep may take it.
    Temp,
    /// On its way to a name that may be cleared: recorded as a landing (on
    /// disk before the clearing delete, `in_flight_temps::hold_for_landing`;
    /// `None` on a ledger that keeps nothing) and in no in-memory set.
    Landing(Option<TrackedRecord>),
    /// Settled: nothing left to tell the ledgers.
    Settled,
}

impl Landing {
    pub(super) fn new(
        state: &Arc<WriteOperationState>,
        temp: StagingTemp,
        final_path: &Path,
        name: LandingName,
        size: u64,
        home_volume: Option<String>,
    ) -> Self {
        Self {
            state: Arc::clone(state),
            temp,
            final_path: final_path.to_path_buf(),
            name,
            size,
            home_volume,
            recorded: Recorded::Temp,
            discard_asked: false,
        }
    }

    fn temp(&self) -> &Path {
        self.temp.path()
    }

    fn home(&self) -> Option<in_flight_temps::RecordHome> {
        home_of(self.home_volume.as_deref())
    }

    /// The temp stopped being anything of ours: it landed, or it's gone.
    fn settled(&mut self) {
        match std::mem::replace(&mut self.recorded, Recorded::Settled) {
            Recorded::Temp => in_flight_temps::retire_temp(&self.state, self.home(), self.temp.path()),
            Recorded::Landing(Some(record)) => in_flight_temps::retire(&self.state, &record),
            Recorded::Landing(None) | Recorded::Settled => {}
        }
    }

    /// The way to the final name is about to be cleared: the temp becomes a
    /// landing, on disk before this resolves. `false` when the record can't be
    /// confirmed on disk, and the temp stays what it was.
    async fn hold(&mut self) -> bool {
        if !matches!(self.recorded, Recorded::Temp) {
            return true;
        }
        let held =
            in_flight_temps::hold_for_landing(&self.state, self.home(), self.temp.path(), &self.final_path, self.size)
                .await;
        match held {
            Ok(record) => {
                self.recorded = Recorded::Landing(record);
                true
            }
            Err(in_flight_temps::NotOnDisk) => false,
        }
    }

    /// The temp may be the destination's only copy and the landing couldn't
    /// finish: the record stays, for the sweep that next reaches the volume,
    /// and the operation's stop names the file it's on its way to.
    fn keep_landing(&mut self) {
        if let Recorded::Landing(Some(record)) = std::mem::replace(&mut self.recorded, Recorded::Settled) {
            in_flight_temps::keep_for_arrival(&self.state, record);
            self.state
                .kept_landings
                .lock_ignore_poison()
                .push(self.final_path.clone());
        }
    }

    /// Nothing was cleared after all and the temp couldn't be taken away, so
    /// it's a removable copy of ours again: recorded as a temp (before the
    /// landing record goes) for the sweeps to remove.
    fn landing_back_to_temp(&mut self) {
        if let Recorded::Landing(held) = std::mem::replace(&mut self.recorded, Recorded::Temp) {
            in_flight_temps::back_to_temp(&self.state, self.home(), self.temp.path(), held);
        }
    }

    /// Takes the temp away, a copy nobody needs because nothing was cleared for
    /// it. Answers whether it's gone; a temp that isn't stays as recorded.
    async fn discard(&mut self, dest_volume: &Arc<dyn Volume>) -> bool {
        self.discard_asked = true;
        let gone = discard_unplaced_temp(dest_volume.as_ref(), self.temp.path()).await;
        if gone {
            self.settled();
        }
        gone
    }

    /// A swap's delete of the user's original went through (or may have): the
    /// operation stops being rollbackable now, whatever the rename does next,
    /// because the file an undo would restore is gone. The ONE moment an
    /// overwrite rules out rollback on a failure path: a write that never got
    /// here left the original in place, and a landed swap's journal row says
    /// so as well.
    fn original_replaced(&self, in_the_way: &InTheWay) {
        if matches!(in_the_way, InTheWay::TheOriginal) {
            journal::note_not_rollbackable(self.state.scope(), NotRollbackableReason::Overwrote);
        }
    }
}

impl Drop for Landing {
    fn drop(&mut self) {
        self.keep_landing();
    }
}

/// What a landing clears out of the final name's way before its rename.
#[derive(Debug)]
pub(super) enum InTheWay {
    /// Something on a name the caller claimed, which the first rename met:
    /// its `AlreadyExists` is the error to report if the way can't be cleared.
    Claimed(VolumeError),
    /// The user's original, which a swap replaces. A refused delete reports
    /// its own refusal, which is about the user's file.
    TheOriginal,
}

/// Whether a transport fault cuts the landing off for the caller's wait, or
/// meets the refusal rules like any other error (the wait already gave up).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Patience {
    Waits,
    GaveUp,
}

/// Which step a transport fault cut a landing off at, which is what the
/// settle needs to know about what may have happened.
#[derive(Debug)]
enum CutOffAt {
    /// A rename-first landing's first rename: nothing was cleared, and the
    /// temp is still a removable copy of ours. The server may have renamed it.
    FirstRename,
    /// The delete that clears the way: it may have gone through. The temp is
    /// recorded as a landing.
    ClearingDelete(InTheWay),
    /// The rename onto the cleared name: the delete went through, so the temp
    /// may be the destination's only copy. The server may have renamed it.
    RenameOntoCleared,
    /// A swap's one replacing rename ([`replace_in_one_step`]): nothing was
    /// cleared, since the original is replaced in that step or not at all, so
    /// the temp is still a removable copy of ours. The server may have
    /// performed it.
    ReplacingRename,
}

/// Moves a completed temp onto its final name.
///
/// Renames FIRST, and only clears the final name if that rename said something
/// is in the way: it usually isn't (a fresh copy, or a conflict the resolver
/// already cleared), and a speculative delete would spend one extra round trip
/// per file on SMB and MTP for nothing. The name can still be taken — a
/// `Rename` resolution's `O_EXCL` placeholder, a cross-type Overwrite whose
/// dest delete failed, a racing writer — so the second attempt covers it.
///
/// ❗ **A swap is the other way round** ([`LandingName::ReplacesOriginal`], a
/// file→file Overwrite): the original is known to be in the way. Where the
/// destination's forced rename replaces a file in ONE step
/// (`Volume::supports_atomic_replace_rename`: a local disk's `rename(2)`, SMB's
/// `ReplaceIfExists`), the swap is that rename ([`replace_in_one_step`]): the
/// name never goes empty, so the temp is never the only copy and nothing needs
/// recording as a landing. Everywhere else the original is deleted first and
/// the temp renamed into the empty slot, through the same cleared-way rules
/// below ([`clear_the_way`]); ❌ never `rename(force)` there, because MTP's
/// doesn't delete what's there and can make a duplicate. The moment the
/// original is replaced (or its delete goes through), the operation stops
/// being rollbackable ([`Landing::original_replaced`]): the file an undo would
/// restore is gone.
///
/// ❗ **Only `AlreadyExists` earns the delete**, and this is the difference
/// between a transient blip and a destroyed file. A rename over a network
/// backend fails for plenty of reasons that say nothing about the destination:
/// the session blinked, the server refused, SFTP v3 collapsed an errno into its
/// one catch-all code. Clearing the way on every `Err` would delete a file the
/// user still has and then report the blip that "justified" it. It also covers a
/// backend that can delete but not rename, which would otherwise destroy the
/// destination and answer `NotSupported`.
///
/// ❗ **And only a name the CALLER claimed**, which is the other half of the
/// same question. `AlreadyExists` says something is in the way; only
/// [`LandingName`] says whose it is. Under
/// [`LandingName::ExpectedFree`] nothing resolved a conflict for this write, so
/// what's in the way is the user's file and a policy nobody applied to it: the
/// landing reports the clash and leaves both sides alone. A case-insensitive
/// destination is where that happens without any race — `Report.docx` renaming
/// onto a stored `report.docx` — and clearing the way there replaced files under
/// a Skip.
///
/// ❗ **A landing that fails with nothing cleared takes its temp away at once.**
/// The name still holds what it held, the source still holds the new bytes, and
/// a complete `.cmdr-tmp-*` of ours in the user's folder would otherwise wait
/// an hour for the stale-temp reap (`recovered_name::discard_unplaced_temp`).
/// That covers a rename refused for any reason but `AlreadyExists`, a clash on
/// a name the caller believed free, and a claimed name (or an original) whose
/// delete was refused with the name still taken. When the way was CLEARED and
/// the rename then failed anyway, the temp is the only complete copy at the
/// destination, so it is never deleted: it leaves temp space instead
/// ([`LandingAftermath::NewDataKeptAt`]).
///
/// ❗ **Classify before acting.** A step that fails on the transport
/// ([`VolumeError::transport_fault`]) may have been performed by the server,
/// so the landing is cut off right there ([`LandingError::CutOff`]): no
/// discard, no refused-delete stat, no rescue. Any of those would run on
/// whatever session answers next (a sibling's fresh one, when the error was the
/// old session's) and act on a guess: discard a temp that landed, read the name
/// as still taken and throw the only copy away, or park new bytes under
/// ` (recovered)` for a blip. The caller waits and settles
/// ([`CutOffLanding`]); the refusal rules run only for a refusal, or once the
/// wait gave up.
///
/// ❗ **Before it clears the way, the temp is re-recorded as a landing**
/// (`in_flight_temps::hold_for_landing`): on disk as an `ItemKind::Landing` at
/// the write's size before the delete goes out, and out of the operation's
/// in-flight set, so no in-operation sweep may delete it and a crash leaves a
/// record the sweep finishes the landing from. ❗ When that record can't be
/// confirmed on disk, the way is NOT cleared: the landing fails as a refusal
/// with nothing cleared, the original in place. The landing record retires
/// when the rename lands it, when the rescue gives it a real name, or when the
/// refused delete left the name taken (the temp is a spare copy then, and goes
/// or turns back into a temp). A landing that ends unsure, or is cut off and
/// never resumed, leaves it recorded for the sweep.
pub(super) async fn land(dest_volume: &Arc<dyn Volume>, mut landing: Landing) -> Result<(), LandingError> {
    if landing.name == LandingName::ReplacesOriginal {
        if dest_volume.supports_atomic_replace_rename() {
            return replace_in_one_step(dest_volume, landing).await;
        }
        return clear_the_way(dest_volume, landing, InTheWay::TheOriginal, Patience::Waits).await;
    }
    let Err(first) = dest_volume.rename(landing.temp(), &landing.final_path, false).await else {
        landing.settled();
        return Ok(());
    };
    if !matches!(first, VolumeError::AlreadyExists(_)) {
        if first.transport_fault().is_some() {
            return Err(cut_off(landing, CutOffAt::FirstRename, first));
        }
        landing.discard(dest_volume).await;
        return Err(LandingError::Failed(first.into()));
    }
    if matches!(landing.name, LandingName::ExpectedFree | LandingName::FreeInFreshFolder) {
        log::warn!(
            target: "copy",
            "staged write: {} is taken by something nobody resolved a conflict for; \
             leaving it alone and taking our temp {} away.",
            LogPrivate::file_path(&landing.final_path),
            LogPrivate::file_path(landing.temp()),
        );
        landing.discard(dest_volume).await;
        return Err(LandingError::Failed(first.into()));
    }
    clear_the_way(dest_volume, landing, InTheWay::Claimed(first), Patience::Waits).await
}

fn cut_off(landing: Landing, at: CutOffAt, error: VolumeError) -> LandingError {
    log::info!(
        target: "copy",
        "landing: {} was cut off on the transport ({error}); settling it once the destination is back",
        LogPrivate::file_path(&landing.final_path),
    );
    LandingError::CutOff(Box::new(CutOffLanding { landing, at, error }))
}

/// A swap on a destination whose forced rename replaces a file in one step:
/// the temp is renamed over the original. A refusal left the original whole
/// (the replace happened in full or not at all), so the temp is a spare copy
/// and goes; a transport fault cuts the landing off, since the server may have
/// performed it.
async fn replace_in_one_step(dest_volume: &Arc<dyn Volume>, mut landing: Landing) -> Result<(), LandingError> {
    match dest_volume.rename(landing.temp(), &landing.final_path, true).await {
        Ok(()) => {
            landing.original_replaced(&InTheWay::TheOriginal);
            landing.settled();
            Ok(())
        }
        Err(error) if error.transport_fault().is_some() => {
            landing.original_replaced(&InTheWay::TheOriginal);
            Err(cut_off(landing, CutOffAt::ReplacingRename, error))
        }
        Err(refused) => {
            landing.discard(dest_volume).await;
            Err(LandingError::Failed(refused.into()))
        }
    }
}

/// The landing's cleared-way tail, shared by a claimed name and a swap: record
/// the temp as a landing, clear the final name, and rename the temp onto it.
async fn clear_the_way(
    dest_volume: &Arc<dyn Volume>,
    mut landing: Landing,
    in_the_way: InTheWay,
    patience: Patience,
) -> Result<(), LandingError> {
    if !landing.hold().await {
        log::warn!(
            target: "copy",
            "landing: couldn't confirm the landing record for {} on disk, so {} stays where it is",
            LogPrivate::file_path(landing.temp()),
            LogPrivate::file_path(&landing.final_path),
        );
        landing.discard(dest_volume).await;
        return Err(LandingError::Failed(landing_not_recorded()));
    }
    match dest_volume.delete(&landing.final_path).await {
        Ok(()) | Err(VolumeError::NotFound(_)) => {}
        Err(error) if patience == Patience::Waits && error.transport_fault().is_some() => {
            landing.original_replaced(&in_the_way);
            return Err(cut_off(landing, CutOffAt::ClearingDelete(in_the_way), error));
        }
        Err(refused) => return after_a_refused_delete(dest_volume, landing, in_the_way, refused, patience).await,
    }
    landing.original_replaced(&in_the_way);
    rename_onto_cleared(dest_volume, landing, patience).await
}

/// The refusal for a landing whose record couldn't be confirmed on disk:
/// nothing was cleared, and the original (or whatever held the name) is where
/// it was. Typed ([`LandingAftermath::NotRecorded`]), so the dialog words it;
/// the `IoError` is a diagnostic for the logs, ❌ never shown.
fn landing_not_recorded() -> FinalizeFailure {
    FinalizeFailure {
        error: VolumeError::IoError {
            message: "the landing record couldn't be confirmed on disk".to_owned(),
            raw_os_error: None,
        },
        aftermath: Some(LandingAftermath::NotRecorded),
    }
}

/// The delete of what's in the way answered `refused`: one stat says whether
/// the name is still taken (the server can act and the answer still be lost).
async fn after_a_refused_delete(
    dest_volume: &Arc<dyn Volume>,
    mut landing: Landing,
    in_the_way: InTheWay,
    refused: VolumeError,
    patience: Patience,
) -> Result<(), LandingError> {
    match what_a_refused_delete_left(dest_volume.as_ref(), &landing.final_path).await {
        AfterARefusedDelete::Gone => {
            landing.original_replaced(&in_the_way);
            rename_onto_cleared(dest_volume, landing, patience).await
        }
        // Couldn't clear the way, so nothing was lost: the destination still
        // holds whatever was in the way, and the temp is a spare copy.
        AfterARefusedDelete::StillThere => {
            let report = refusal_to_report(in_the_way, refused);
            log::warn!(
                target: "copy",
                "landing: {} still holds what was in the way after a refused delete ({report}); \
                 taking the temp {} away",
                LogPrivate::file_path(&landing.final_path),
                LogPrivate::file_path(landing.temp()),
            );
            if !landing.discard(dest_volume).await {
                landing.landing_back_to_temp();
            }
            Err(LandingError::Failed(report.into()))
        }
        // Maybe cleared, maybe not: the temp may be the only copy, so the
        // landing record stays for the sweep to finish or rescue.
        AfterARefusedDelete::Unknown => {
            landing.original_replaced(&in_the_way);
            landing.keep_landing();
            Err(LandingError::Failed(refusal_to_report(in_the_way, refused).into()))
        }
    }
}

fn refusal_to_report(in_the_way: InTheWay, refused: VolumeError) -> VolumeError {
    match in_the_way {
        InTheWay::Claimed(first) => first,
        InTheWay::TheOriginal => refused,
    }
}

/// The way is clear: rename the temp onto the final name.
async fn rename_onto_cleared(
    dest_volume: &Arc<dyn Volume>,
    mut landing: Landing,
    patience: Patience,
) -> Result<(), LandingError> {
    match dest_volume.rename(landing.temp(), &landing.final_path, false).await {
        Ok(()) => {
            landing.settled();
            Ok(())
        }
        Err(error) if patience == Patience::Waits && error.transport_fault().is_some() => {
            Err(cut_off(landing, CutOffAt::RenameOntoCleared, error))
        }
        Err(error) => Err(LandingError::Failed(rescue(dest_volume, landing, error).await)),
    }
}

/// ❗ The way is cleared and the bytes couldn't take the name, so the temp is
/// now the only complete copy of a file with nothing at its destination — and
/// it wears a `.cmdr-tmp-*` name `cleanup.rs::reap_stale_transfer_temps`
/// matches once nothing records it. Get it out of temp space and report where
/// it went. A rescue that couldn't rename either leaves the bytes on the temp,
/// recorded.
async fn rescue(dest_volume: &Arc<dyn Volume>, mut landing: Landing, error: VolumeError) -> FinalizeFailure {
    let new_data_at = rescue_out_of_temp_space(dest_volume.as_ref(), landing.temp(), &landing.final_path).await;
    if new_data_at == landing.temp() {
        landing.keep_landing();
    } else {
        landing.settled();
    }
    FinalizeFailure {
        error,
        aftermath: Some(LandingAftermath::NewDataKeptAt(new_data_at)),
    }
}

/// A landing a transport fault cut off ([`LandingError::CutOff`]): the server
/// may or may not have performed the step it was on.
pub(crate) struct CutOffLanding {
    landing: Landing,
    at: CutOffAt,
    error: VolumeError,
}

impl std::fmt::Debug for CutOffLanding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CutOffLanding")
            .field("temp", &self.landing.temp())
            .field("final_path", &self.landing.final_path)
            .field("at", &self.at)
            .field("error", &self.error)
            .finish()
    }
}

/// The source a landed file's bytes came from, for the settle's byte compare.
#[derive(Clone, Copy)]
pub(crate) struct LandingSource<'a> {
    pub(crate) volume: &'a Arc<dyn Volume>,
    pub(crate) path: &'a Path,
}

/// What resuming a cut-off landing came to.
pub(crate) enum Resumed {
    /// The file is whole at its name.
    Landed,
    /// Nothing proves it landed, and nothing of it is left to land: run the
    /// file again from its first byte. Carries the error that cut it off, for
    /// a caller that can't run it again.
    RunAgain(VolumeError),
    /// It ended without landing; the failure is settled as any landing's.
    Failed(FinalizeFailure),
    /// Cut off again: wait once more.
    CutOff(Box<CutOffLanding>),
}

impl From<Result<(), LandingError>> for Resumed {
    fn from(landed: Result<(), LandingError>) -> Self {
        match landed {
            Ok(()) => Self::Landed,
            Err(LandingError::Failed(failure)) => Self::Failed(failure),
            Err(LandingError::CutOff(cut_off)) => Self::CutOff(cut_off),
        }
    }
}

impl CutOffLanding {
    /// What cut it off.
    pub(crate) fn error(&self) -> &VolumeError {
        &self.error
    }

    /// The destination is back: settles what the cut-off step did and lands
    /// what's still to land, without streaming the bytes again when the temp is
    /// still whole (`interrupted_landing.rs` has the proofs).
    ///
    /// - **The temp is there and whole**: land it from where the cut-off left
    ///   off. A first rename lands again from the top, and a one-step replace
    ///   runs again (the original is still whole). A clearing delete that
    ///   may have gone through asks the name: empty means it did, and the
    ///   rename alone finishes the landing; taken means it never reached the
    ///   server, and the way is cleared again. After a delete that went
    ///   through, ❗ the rename alone, ❌ never another delete: a file
    ///   someone put at the name during the outage makes it refuse, and the
    ///   new bytes are rescued and reported, never their file deleted.
    /// - **The temp is gone** and nothing of ours discarded it, so the server
    ///   performed our rename: landed when the name holds what we wrote. A
    ///   first rename's proof is the size (nothing cleared, so nothing else
    ///   could have moved our temp onto a name that was free); after the way
    ///   was cleared or a one-step replace (whose name held the original,
    ///   possibly at the same size), ❗ the name must hold exactly the SOURCE's
    ///   bytes, compared in full, and with no `source` to compare against (a
    ///   one-pass extract) it never counts as landed.
    /// - **Anything else** (a temp that isn't whole, a name that doesn't prove
    ///   it): run the file again.
    ///
    /// A step of this that meets the transport again cuts the landing off
    /// again; one that meets a refusal settles by the refusal rules.
    pub(crate) async fn resume(self, dest_volume: &Arc<dyn Volume>, source: Option<LandingSource<'_>>) -> Resumed {
        let Self { mut landing, at, error } = self;
        match dest_volume.get_metadata(landing.temp()).await {
            Ok(entry) if !entry.is_directory && !entry.is_symlink && entry.size == Some(landing.size) => {
                resume_with_a_whole_temp(dest_volume, landing, at).await
            }
            // Not the bytes we wrote, so nothing to land. A temp that never
            // left the in-flight set goes; one recorded as a landing stays
            // recorded (the sweep leaves a short one alone).
            Ok(_) => {
                if matches!(at, CutOffAt::FirstRename | CutOffAt::ReplacingRename) {
                    landing.discard(dest_volume).await;
                } else {
                    landing.keep_landing();
                }
                Resumed::RunAgain(error)
            }
            Err(VolumeError::NotFound(_)) => {
                let landed = !landing.discard_asked
                    && match (&at, source) {
                        (CutOffAt::FirstRename, _) => {
                            holds_our_size(dest_volume, &landing.final_path, landing.size).await
                        }
                        (_, Some(source)) => {
                            holds_the_source(
                                source.volume,
                                source.path,
                                landing.size,
                                dest_volume,
                                &landing.final_path,
                            )
                            .await
                        }
                        (_, None) => false,
                    };
                // The temp is gone either way: nothing left to protect.
                landing.settled();
                if landed {
                    log::info!(
                        target: "copy",
                        "landing: {} landed before the drop swallowed the reply; counting it done",
                        LogPrivate::file_path(&landing.final_path),
                    );
                    Resumed::Landed
                } else {
                    Resumed::RunAgain(error)
                }
            }
            Err(stat) if stat.transport_fault().is_some() => Resumed::CutOff(Box::new(Self {
                landing,
                at,
                error: stat,
            })),
            Err(_) => Self { landing, at, error }.give_up(dest_volume).await.into(),
        }
    }

    /// The wait ended without the destination: it gave up, the person
    /// cancelled, or there was nothing to wait for. The step's error now meets
    /// the refusal rules, as any refusal would: nothing cleared leaves the temp
    /// to the abandoned-write cleanup; a clearing delete takes one stat (taken:
    /// the temp is a spare copy and goes; can't tell: it stays recorded as a
    /// landing for the next sweep that reaches the share); a rename after the
    /// delete rescues the bytes out of temp space, or leaves them recorded when
    /// the share can't take that rename either. `Ok` when the share answered
    /// after all and the landing finished.
    pub(crate) async fn give_up(self, dest_volume: &Arc<dyn Volume>) -> Result<(), FinalizeFailure> {
        let Self { landing, at, error } = self;
        let ended = match at {
            CutOffAt::FirstRename | CutOffAt::ReplacingRename => return Err(error.into()),
            CutOffAt::ClearingDelete(in_the_way) => {
                after_a_refused_delete(dest_volume, landing, in_the_way, error, Patience::GaveUp).await
            }
            CutOffAt::RenameOntoCleared => return Err(rescue(dest_volume, landing, error).await),
        };
        ended.map_err(|ended| match ended {
            LandingError::Failed(failure) => failure,
            // Unreachable: a landing that gave up cuts nothing off.
            LandingError::CutOff(cut_off) => cut_off.error.into(),
        })
    }
}

impl From<Result<(), FinalizeFailure>> for Resumed {
    fn from(ended: Result<(), FinalizeFailure>) -> Self {
        match ended {
            Ok(()) => Self::Landed,
            Err(failure) => Self::Failed(failure),
        }
    }
}

/// [`CutOffLanding::resume`] with the temp still whole.
async fn resume_with_a_whole_temp(dest_volume: &Arc<dyn Volume>, landing: Landing, at: CutOffAt) -> Resumed {
    match at {
        CutOffAt::FirstRename => land(dest_volume, landing).await.into(),
        CutOffAt::ReplacingRename => replace_in_one_step(dest_volume, landing).await.into(),
        CutOffAt::RenameOntoCleared => rename_onto_cleared(dest_volume, landing, Patience::Waits).await.into(),
        CutOffAt::ClearingDelete(in_the_way) => match dest_volume.get_metadata(&landing.final_path).await {
            // The delete went through before the drop.
            Err(VolumeError::NotFound(_)) => rename_onto_cleared(dest_volume, landing, Patience::Waits).await.into(),
            // It never reached the server: what was in the way still is.
            Ok(_) => clear_the_way(dest_volume, landing, in_the_way, Patience::Waits)
                .await
                .into(),
            Err(stat) if stat.transport_fault().is_some() => Resumed::CutOff(Box::new(CutOffLanding {
                landing,
                at: CutOffAt::ClearingDelete(in_the_way),
                error: stat,
            })),
            Err(stat) => CutOffLanding {
                landing,
                at: CutOffAt::ClearingDelete(in_the_way),
                error: stat,
            }
            .give_up(dest_volume)
            .await
            .into(),
        },
    }
}

/// Whether `final_path` holds a file of exactly `size` bytes.
pub(super) async fn holds_our_size(dest_volume: &Arc<dyn Volume>, final_path: &Path, size: u64) -> bool {
    matches!(
        dest_volume.get_metadata(final_path).await,
        Ok(entry) if !entry.is_directory && !entry.is_symlink && entry.size == Some(size)
    )
}
