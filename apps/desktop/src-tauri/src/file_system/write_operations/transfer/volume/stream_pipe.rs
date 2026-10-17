//! The per-file streaming write: one file's bytes from `open_read_stream` into
//! the destination's `write_from_stream`, staged on a `.cmdr-tmp-*` sibling,
//! retried per file, parked and yielded between chunks, and raced against both
//! cancel tiers. `strategy.rs` dispatches here for every FILE source; `merge.rs`
//! for every file child of a walked tree.

use std::ops::ControlFlow;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use super::super::super::metadata_losses;
use super::super::super::state::WriteOperationState;
use super::super::checkpoint_stream::CheckpointStream;
use super::super::interrupted_landing::{
    InterruptedLanding, create_went_unanswered, settle_interrupted_landing, settle_unanswered_create,
};
use super::super::recovered_name::FinalizeFailure;
use super::super::retry;
use super::super::staged_write::{StagedWrite, WriteStaging, note_pending_for_local_dest, resolve_staging};
use super::super::transfer_probe::{
    TaskPhase, arm_current_task_stall_abort, note_task_retry, set_task_bytes, set_task_phase,
};
use super::super::volume_outage::{self, OutageEnd, wait_after_failure};
use super::preflight::SourceFileFacts;
use super::server_side_copy::try_server_side_copy;
use super::transfer_error::{AtPath, PathedVolumeError, hard_abort_error};
use cmdr_fs::volume::{StreamWritten, Volume, VolumeError, VolumeReadStream};

/// Debounce window for the foreground auto-yield: after foreground work drains,
/// the checkpoint stays parked until the device has been quiet for this long
/// before starting the next window, so a BURST of listings (e.g. arrow-keying
/// down a folder tree) is served as ONE suspension instead of re-checking every
/// window. ~400 ms balances nav responsiveness (the copy is suspended the whole
/// window) against park thrash; a starting value, to be tuned on real hardware.
const FOREGROUND_YIELD_DEBOUNCE: Duration = Duration::from_millis(400);

/// Minimum-progress floor for the foreground auto-yield: after a resume, the
/// transfer must move at least this many bytes before it will honor the next
/// foreground yield. Without it, continuous foreground nav would park the copy
/// before every window and starve it to zero throughput.
///
/// At 4 MiB this is SMALLER than one bounded read window (`MTP_READ_WINDOW`,
/// 8 MiB), so in practice the floor resolves to "at least one full window between
/// yields" — the copy always reads one more 8 MiB window before it can yield
/// again. That's the intended guarantee; the 4 MiB value just never bites
/// distinctly from the window today (real-device verified to feel right).
///
/// ⚠️ Don't naively raise this to a "big" number to make it look meaningful: the
/// gate SKIPS the yield until this many bytes have moved since the last resume,
/// and the count starts at zero for EVERY file, so a floor ≥ a typical file size
/// means the copy never yields at all for files smaller than the floor — i.e. it
/// would disable navigate-during-transfer for normal files. If you want it to
/// read as a real multi-window guard, raise it to a small multiple of
/// `MTP_READ_WINDOW` (e.g. 2-4× = "N windows between yields"); that changes
/// behavior, so re-verify on a real device.
///
/// The destination arm exempts a SINGLE-SHOT write from the floor for exactly
/// that reason: such a write holds nothing open on the server while it drains, so
/// the floor has nothing to protect, and without the exemption a folder of photos
/// uploaded to a NAS stood aside for the user zero times. A streaming write under
/// the floor still never yields; that gap is deferred, see `transfer/DETAILS.md`
/// § "Foreground auto-yield".
pub(super) const MIN_PROGRESS_FLOOR_BYTES: u64 = 4 * 1024 * 1024;

/// Hard cap on a SINGLE destination-side foreground park (uploads to SMB). Unlike
/// the SOURCE arm's `wait_until_foreground_idle` (unbounded, since a read holds
/// nothing scarce between windows), the destination arm holds an OPEN SMB write
/// handle across the pause, so it must resume and write at least this often even
/// under continuous browsing, keeping the handle warm so the server can't reap it
/// as idle. 1 s balances browsing responsiveness (the upload stands aside up to a
/// second at a time) against handle safety (a WRITE lands at least once a second).
/// The share's OWN session stays warm regardless (the user's navigation rides it),
/// so this cap protects only the write handle. ❌ Don't raise it toward any
/// server idle-timeout; keep it a small, safe fraction. Data-safety bound; see
/// `checkpoint_stream.rs::CheckpointStream::bounded_yield_to_dest_foreground`.
const DEST_FOREGROUND_YIELD_HARD_CAP: Duration = Duration::from_secs(1);

/// The (debounce, min-progress-floor, dest-yield-hard-cap) tuple a freshly-built
/// `CheckpointStream` uses. Production always returns the named constants. Tests
/// override all three (debounce ≈ 0, a tiny floor, a short cap) via
/// `AutoYieldTuningGuard` so both the source and destination auto-yield arms are
/// deterministic without real device latency or megabytes of synthetic data. The
/// stream construction lives behind `copy_single_path`, so a thread-local override
/// is how a test reaches it without widening the public copy API.
fn auto_yield_tuning() -> (Duration, u64, Duration) {
    #[cfg(test)]
    {
        if let Some(t) = super::strategy::test_support::auto_yield_tuning_override() {
            return t;
        }
    }
    (
        FOREGROUND_YIELD_DEBOUNCE,
        MIN_PROGRESS_FLOOR_BYTES,
        DEST_FOREGROUND_YIELD_HARD_CAP,
    )
}

/// Streams one file from source to destination via `open_read_stream` /
/// `write_from_stream`. Per-chunk progress and cancellation are enforced by
/// the destination's `write_from_stream` implementation, which calls
/// `on_progress` between chunks and returns `VolumeError::Cancelled` on
/// `ControlFlow::Break(())`.
///
/// The source stream is wrapped in a [`CheckpointStream`] so a between-chunk
/// cooperative checkpoint (park-while-paused, then `yield_now`) runs once per
/// chunk: that's what makes a paused op stop advancing MID-FILE (the sync
/// `on_progress` callback can't `.await` to park), and what keeps a long
/// single-file transfer from starving foreground tasks.
///
/// The bytes never touch `dest_path` until the last one has landed: unless the
/// caller already staged the write, they go to a `.cmdr-tmp-*` sibling that is
/// renamed into place at the end (`staged_write.rs`).
#[allow(
    clippy::too_many_arguments,
    reason = "One file's whole streaming context: both volumes, both paths, the size hint, shared state, the progress callback, and who staged the write."
)]
pub(super) async fn stream_pipe_file(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    source_facts: SourceFileFacts,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    state: &Arc<WriteOperationState>,
    on_file_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    staging: WriteStaging,
) -> Result<u64, PathedVolumeError> {
    log::debug!("stream_pipe_file: {} -> {}", source_path.display(), dest_path.display());
    // Whether the CALLER lands this write (a safe-replace temp it swaps over the
    // original): then the flags that wait for the landing wait for that swap.
    let caller_lands = matches!(staging, WriteStaging::AlreadyStaged);

    // Register BOTH halves of the eventual rename with the downloads watcher's
    // ignore set when the destination is local-FS-backed (the only case where
    // the watcher could otherwise fire). Covers MTP→Local and SMB→Local imports
    // that land in ~/Downloads.
    note_pending_for_local_dest(state.scope().host(), dest_volume, dest_path);

    // A destination that can't rename can't stage. No production backend is in
    // that position (Local, SMB, and MTP all rename), but a minimal `Volume`
    // impl must stay usable as a copy destination, so a `NotSupported` landing
    // re-runs the file unstaged — the pre-staging behavior — instead of failing.
    // The server can copy for itself: the bytes never leave it. A pure
    // optimization, so anything short of a clean success falls through to the
    // streaming loop below rather than failing the file.
    if let Some(bytes) = try_server_side_copy(
        source_volume,
        source_path,
        dest_volume,
        dest_path,
        state,
        on_file_progress,
        staging,
    )
    .await
    .map_err(|f| PathedVolumeError::from_landing(f, dest_path))?
    {
        // Landed, so a ` (N)` placeholder at the name is filled, not ours to take back.
        state.claimed_names.release_placeholder(dest_path);
        return Ok(bytes);
    }

    let mut staging = staging;
    // Set once a destination that can't land a staged write sends the file back
    // to be written at its final name. From then on a failed attempt's partial
    // sits AT that name and is ours, so the terminal failure removes it here:
    // the driver can't, because it only knows the staging it asked for
    // (`failed_write_leaves_ours_at`).
    let mut writing_at_the_final_name = false;
    // Which attempt at THIS file we're on, 1-based. A transport blip
    // (`retry::is_retryable`) runs the file again from its first byte on a fresh
    // source stream and a fresh staging temp, up to `retry::MAX_ATTEMPTS`; see
    // `retry.rs` for the policy and why it lives here rather than any layer above.
    // Restarting the whole file is what keeps the retry honest: nothing partial
    // survives an attempt, so no byte is written twice and no ledger records a
    // file twice.
    let mut attempt: u32 = 1;
    // How many outages THIS file has waited out: a volume that went away
    // entirely (`volume_outage.rs`), as opposed to a blip the attempts above
    // ride. Each one that ends with the volume back gives the file a fresh set
    // of attempts; the cap is what keeps a flapping link from holding it forever.
    let mut outages: u32 = 0;
    // Set when an attempt's create at the FINAL name (a single-shot write) failed
    // on the transport: the server may have completed it, so the next attempt
    // settles that before writing again (`interrupted_landing.rs`).
    let mut unanswered_create = None;
    loop {
        if let Some((size, lost)) = unanswered_create.take()
            && settle_unanswered_create(source_volume, source_path, size, dest_volume, dest_path).await
                == InterruptedLanding::Landed
        {
            log::info!(target: "copy", "stream_pipe_file: {} was written whole before the reply was lost", dest_path.display());
            state.claimed_names.release_placeholder(dest_path);
            state.metadata_losses.note_file(lost);
            return Ok(size);
        }
        // Opening the source is a device round-trip on MTP / SMB and can hang on
        // its own; it needs to be distinguishable from streaming in a dump. It
        // happens BEFORE the staging decision because that decision needs the
        // stream's `total_size()` — the same number the destination gets, so the
        // exemption is asked about exactly the write that will be performed.
        // Nothing is staged yet, so a failure here has nothing to clean up.
        //
        // Raced against TIER 2 for the same reason the write below is: on the
        // SERIAL path nothing above this await can end it, and a device round trip
        // that hangs before the first byte is half of the wedge shape. With
        // nothing staged, the abort has nothing to do but return.
        set_task_phase(TaskPhase::OpeningSource);
        let opened = tokio::select! {
            biased;
            () = state.backend_abort.cancelled() => return Err(hard_abort_error(source_path)).at_source(source_path),
            opened = source_volume.open_read_stream_with_hint(source_path, source_facts.size) => opened,
        };
        let stream = match opened {
            Ok(stream) => stream,
            // A source share that's gone (a download from a NAS that dropped):
            // wait for it and open the file again. Nothing is staged yet, so
            // there's nothing to clear first.
            Err(e) => match wait_after_failure(state, &e, &[source_volume, dest_volume], &mut outages).await {
                OutageEnd::Back => {
                    attempt = 1;
                    continue;
                }
                OutageEnd::Cancelled => return Err(volume_outage::cancelled()).at_source(source_path),
                OutageEnd::GaveUp | OutageEnd::NotWaitable => return Err(e).at_source(source_path),
            },
        };
        let Some(size) = stream.total_size().known() else {
            return Err(VolumeError::NotSupported).at_source(source_path);
        };
        let (stream, source_reads) = super::source_reads::watch(stream);
        let length = cmdr_fs::volume::StreamLength::Known(size);
        // An extract carries the archive's quarantine with the rest of what
        // the source reports, so the destination puts it on before the landing
        // like any other xattr (`extract_quarantine.rs`).
        let stream = super::extract_quarantine::quarantine_stream(state, source_volume, stream);
        // What the destination can't keep of what this file had worth keeping,
        // asked before the write consumes the stream and counted once it
        // lands: nothing at all for a pair that keeps everything.
        let lost = metadata_losses::file_losses(&**source_volume, &**dest_volume, &*stream);
        // What can't go on until the file has its final name (Locked), asked
        // before the write consumes the stream; empty for a plain file.
        let after_landing = super::landed_flags::after_landing(&**source_volume, &**dest_volume, &*stream);
        // ONE probe, two consumers: the staging decision below and the
        // destination-side foreground yield's floor exemption (handed to the
        // `CheckpointStream`). See `resolve_staging`.
        let write_is_single_shot = dest_volume.write_is_single_shot(length).await;
        // A whole-publishing destination (an object store) needs no staging
        // either, though it makes no single-shot promise: the yield floor and
        // the stall watchdog below keep reading the single-shot answer alone.
        let resolved_staging = resolve_staging(staging, write_is_single_shot || dest_volume.publishes_writes_whole());
        let staged = StagedWrite::begin(state, dest_path, resolved_staging);
        note_pending_for_local_dest(state.scope().host(), dest_volume, staged.target());
        // Wrap so a paused op parks (and a long copy yields to foreground)
        // between bounded windows. `size` is read off the raw stream first — the
        // wrapper forwards `total_size()` unchanged, so the destination still sees
        // the real size. The wrapper carries BOTH volumes: the source drives the
        // read-side auto-yield (downloads), the destination drives the bounded
        // write-side yield (uploads to SMB). Each is a no-op unless its side opts
        // in (`supports_foreground_yield()` / `supports_foreground_yield_as_destination()`).
        let (foreground_debounce, min_progress_floor, dest_yield_hard_cap) = auto_yield_tuning();
        let stream: Box<dyn VolumeReadStream> = Box::new(CheckpointStream::new(
            stream,
            Arc::clone(state),
            Arc::clone(source_volume),
            Arc::clone(dest_volume),
            foreground_debounce,
            min_progress_floor,
            dest_yield_hard_cap,
            write_is_single_shot,
        ));
        set_task_phase(TaskPhase::Streaming);
        set_task_bytes(0, size);
        // The watchdog ACTING: a task that sits inside a backend call with
        // zero byte movement for `STALL_ABORT_AFTER` has its wait ended here, and
        // the transport error that produces feeds straight back into the retry
        // above. It is the layer of last resort — every backend that can bound its
        // own waits already does, sooner — for the wedge shape that has no
        // deadline anywhere and left a user force-quitting the app.
        //
        // ❌ Never armed for a SINGLE-SHOT write. Those land in one indivisible
        // frame at the file's FINAL name, and only the backend can tell "the
        // server created the file and then refused the bytes" from "the file was
        // already there and we never touched it" (`staged_write.rs` § abandon).
        // Abandoning one from out here would add a client-initiated instance of
        // the transport hazard that exemption already documents as unfixable.
        let stall_abort = if matches!(resolved_staging, WriteStaging::SingleShot(_)) {
            None
        } else {
            arm_current_task_stall_abort()
        };
        // TIER 2 (`state.backend_abort`) rides the same `select!`, and it is a
        // different animal from tier 1: the user's Cancel travels to the backend
        // through `on_file_progress` so the backend drops its own handle and
        // deletes its own partial, and that stays the default for every cancel.
        // This arm is the quit deadline saying it will not wait for a backend
        // that is not answering — armed for EVERY write, single-shot included
        // (dropping one indivisible frame is what the process dying would do
        // anyway, which `volume/DETAILS.md` § "The single-shot exemption" already
        // accounts for). ❌ Never fire it for anything a user clicked.
        //
        // Cost on the happy path: two already-live atomics polled per wakeup of
        // the write future. No allocation, no timer, no syscall, and no change to
        // any backend.
        let write_progress =
            |progress: cmdr_fs::volume::StreamWriteProgress| on_file_progress(progress.bytes_written, size);
        let write_fut =
            dest_volume.write_from_stream(staged.target(), staged.write_mode(), length, stream, &write_progress);
        let outcome = tokio::select! {
            biased;
            () = state.backend_abort.cancelled() => WriteAttemptOutcome::HardAborted,
            () = cancelled_or_never(stall_abort.as_ref()) => WriteAttemptOutcome::Finished(Err(
                VolumeError::ConnectionTimeout(format!(
                    "the write of {} stopped moving and the transfer stopped waiting for it",
                    dest_path.display()
                )),
            )),
            result = write_fut => WriteAttemptOutcome::Finished(result),
        };
        let write_result = match outcome {
            WriteAttemptOutcome::Finished(result) => result,
            WriteAttemptOutcome::HardAborted => {
                log::warn!(
                    target: "copy",
                    "stream_pipe_file: stopped waiting for the write of {}; the app is shutting down. \
                     Its partial stays registered for the startup sweep.",
                    dest_path.display(),
                );
                // ❌ No `staged.abandon` here. The delete would go back through
                // the connection that just failed to answer, which is a second
                // hold on the very deadline this tier exists to keep. A staged
                // write's temp stays in `in_flight_temps` — in memory AND in the
                // persisted log — so `in_flight_temps::init_and_sweep` removes it
                // at the next launch, and nothing sits at a real name meanwhile.
                // A SINGLE-SHOT write has no temp and needs none: the destination
                // promised one indivisible frame, so dropping it leaves either the
                // whole file or nothing, which is the same outcome the process
                // dying produces.
                return Err(hard_abort_error(dest_path)).at_destination(dest_path);
            }
        };
        if create_went_unanswered(&staged, &write_result) {
            unanswered_create = Some((size, lost));
        }
        // A volume that went away entirely (the share dropped off the network
        // and its session reads `Disconnected`): wait for it to come back
        // rather than burn the attempts below in a second, then run the file
        // again from its first byte with a fresh set of attempts. The failed
        // attempt's partial is cleared AFTER the wait, when the delete can reach
        // the share. A wait that gives up, or a failure that isn't an outage,
        // falls through to the arms below exactly as before; a cancel during
        // the wait reaches the cancel arm (`is_cancelled` is set by then).
        let write_result = match write_result {
            Err(e) => match wait_after_failure(state, &e, &[source_volume, dest_volume], &mut outages).await {
                OutageEnd::Back => {
                    staged.abandon_attempt(dest_volume).await;
                    log::info!(
                        target: "copy",
                        "stream_pipe_file: running {} again now that its volume is back",
                        dest_path.display(),
                    );
                    attempt = 1;
                    continue;
                }
                OutageEnd::Cancelled | OutageEnd::GaveUp | OutageEnd::NotWaitable => Err(e),
            },
            ok => ok,
        };
        let written = match write_result {
            Ok(written) => {
                if attempt > 1 {
                    log::info!(
                        target: "copy",
                        "stream_pipe_file: {} landed on attempt {attempt} of {}",
                        dest_path.display(),
                        retry::MAX_ATTEMPTS,
                    );
                }
                written
            }
            Err(e) if retry::should_retry(&e, attempt, state) => {
                // The staged bytes are a partial of an attempt we're abandoning;
                // clear them BEFORE the next attempt starts, so a retried file
                // never leaves a trail of `.cmdr-tmp-*` siblings and the next
                // write lands on a clean path (see `abandon_attempt` for why that
                // reaches one case further than the terminal `abandon`).
                staged.abandon_attempt(dest_volume).await;
                log::warn!(
                    target: "copy",
                    "stream_pipe_file: attempt {attempt} of {} for {} failed ({e}); running the file again in {:?}",
                    retry::MAX_ATTEMPTS,
                    dest_path.display(),
                    retry::backoff_after(attempt),
                );
                note_task_retry();
                set_task_phase(TaskPhase::WaitingToRetry);
                if !retry::wait_before_retry(state, attempt).await {
                    // Cancelled during the backoff. Report it as the cancel it is
                    // rather than the transport error that triggered the retry, so
                    // the post-loop reclassifies it and emits `write-cancelled`.
                    return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()))
                        .at_destination(dest_path);
                }
                attempt += 1;
                continue;
            }
            // A cancel landed while an attempt was failing on something we WOULD
            // have run again. The cancel is the reason this file stops here, so
            // report it as one: the post-loop keys `write-cancelled` off a
            // `Cancelled`-shaped error, and a transport error in its place would
            // log the user's own click as a failed transfer.
            Err(e) if retry::is_retryable(&e) && super::super::super::state::is_cancelled(&state.intent) => {
                staged.abandon(dest_volume).await;
                remove_unstaged_partial(writing_at_the_final_name, dest_volume, dest_path).await;
                return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()))
                    .at_destination(dest_path);
            }
            Err(e) => {
                // The staged bytes are a partial (a mid-stream failure, or the
                // cancel the backend turned into `Cancelled`); drop them.
                staged.abandon(dest_volume).await;
                remove_unstaged_partial(writing_at_the_final_name, dest_volume, dest_path).await;
                if attempt > 1 {
                    log::warn!(
                        target: "copy",
                        "stream_pipe_file: giving up on {} after {attempt} attempt(s): {e}",
                        dest_path.display(),
                    );
                }
                return Err(source_reads.label(e, source_path, dest_path));
            }
        };

        // Past the last byte: give the file its final name. What the source
        // carries beyond its bytes is already on it: the destination asked the
        // stream and put it on the staged write (`Volume::write_from_stream`),
        // all but the flags that wait for the final name. What it refused on
        // the way joins what its caps couldn't hold, for the loss note.
        let bytes = written.bytes;
        let lost = lost.union(written.refused());
        let landing_from = staged.target().to_path_buf();
        match staged.commit(dest_volume).await {
            Ok(()) => {
                // Landed, so a ` (N)` placeholder at the name is filled, not ours
                // to take back (`naming.rs::take_back_unfilled_reservations`).
                state.claimed_names.release_placeholder(dest_path);
                state.metadata_losses.note_file(lost);
                if caller_lands {
                    super::landed_flags::hold(state, dest_path, after_landing);
                } else {
                    super::landed_flags::put_on(dest_volume, dest_path, &after_landing, state).await;
                }
                return Ok(bytes);
            }
            Err(FinalizeFailure {
                error: VolumeError::NotSupported,
                ..
            }) if matches!(
                staging,
                WriteStaging::Stage | WriteStaging::StageInFreshFolder | WriteStaging::StageOntoClaimedName
            ) =>
            {
                log::warn!(
                    target: "copy",
                    "stream_pipe_file: destination can't land a staged write for {}; falling back to writing at the final name",
                    dest_path.display()
                );
                staging = WriteStaging::AlreadyStaged;
                writing_at_the_final_name = true;
                continue;
            }
            // The landing failed with NOTHING cleared because the destination
            // went away under it: the rename may never have reached the server,
            // or the server performed it and the drop swallowed the reply. Wait
            // for the share, then settle which (`interrupted_landing.rs`): our
            // file whole at its name counts as landed; anything else takes the
            // temp away and runs the file again. ❌ Never with `new_data_at`
            // set: then the temp was the only copy, and `land` has already
            // moved it somewhere safe.
            Err(failure) if failure.new_data_at.is_none() => {
                match wait_after_failure(state, &failure.error, &[source_volume, dest_volume], &mut outages).await {
                    OutageEnd::Back => {
                        set_task_phase(TaskPhase::Finalizing);
                        let settled =
                            settle_interrupted_landing(state, dest_volume, &landing_from, dest_path, bytes).await;
                        if settled == InterruptedLanding::Landed {
                            log::info!(
                                target: "copy",
                                "stream_pipe_file: {} landed before the drop swallowed the reply; counting it done",
                                dest_path.display(),
                            );
                            state.claimed_names.release_placeholder(dest_path);
                            state.metadata_losses.note_file(lost);
                            return Ok(bytes);
                        }
                        log::info!(
                            target: "copy",
                            "stream_pipe_file: landing {} again now that its volume is back",
                            dest_path.display(),
                        );
                        attempt = 1;
                        continue;
                    }
                    OutageEnd::Cancelled => return Err(volume_outage::cancelled()).at_destination(dest_path),
                    OutageEnd::GaveUp | OutageEnd::NotWaitable => {
                        return Err(PathedVolumeError::from_landing(failure, dest_path));
                    }
                }
            }
            // The write SUCCEEDED and the landing didn't: the temp holds the only
            // complete copy of the new bytes, and `commit` already dropped it from
            // the in-flight set so nothing sweeps it. When the landing had already
            // cleared the way, `land` also got those bytes out of temp space and
            // `new_data_at` says where they are. Surface the failure.
            Err(e) => return Err(PathedVolumeError::from_landing(e, dest_path)),
        }
    }
}

/// Removes the partial a failed write left AT its final name, which happens only
/// after a destination that can't land a staged write sent the file back to be
/// written there unstaged. Best-effort: the write's own error is the one to
/// report.
async fn remove_unstaged_partial(writing_at_the_final_name: bool, dest_volume: &Arc<dyn Volume>, dest_path: &Path) {
    if !writing_at_the_final_name {
        return;
    }
    if let Err(e) = dest_volume.delete(dest_path).await {
        log::debug!(
            target: "copy",
            "stream_pipe_file: couldn't remove the unstaged partial {}: {e}",
            dest_path.display()
        );
    }
}

/// TIER 2: the operation's hard-abort signal, or a future that never resolves.
///
/// One `select!` arm covers both the armed and the unarmed case without a token
/// allocation, so an inert tier costs a poll of an already-live atomic.
async fn cancelled_or_never(token: Option<&tokio_util::sync::CancellationToken>) {
    match token {
        Some(token) => token.cancelled().await,
        None => std::future::pending().await,
    }
}

/// How one attempt at a file's write ended.
enum WriteAttemptOutcome {
    /// The destination's `write_from_stream` returned, one way or the other.
    Finished(Result<StreamWritten, VolumeError>),
    /// TIER 2 ended the wait: the write future was dropped mid-flight and the
    /// backend ran none of its own cleanup. ❌ Nothing may go back through that
    /// connection now; the staged partial is left to the sweep.
    HardAborted,
}
