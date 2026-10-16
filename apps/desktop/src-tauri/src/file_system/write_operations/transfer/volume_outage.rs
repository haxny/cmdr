//! Waiting out a volume that went away mid-transfer, instead of failing.
//!
//! A router restart or a NAS hiccup drops the share for longer than a per-file
//! retry can bridge (`retry.rs`: three attempts inside 1.25 s), and once a
//! backend marks its session `Disconnected` every call fails at once. Before
//! this, that ended the whole copy, and every job queued behind it for the same
//! share started, failed within a second, and piled up as a failed row (`#398`).
//!
//! Now a transfer that meets a transport failure on a volume whose session reads
//! [`ConnectionState::Disconnected`] waits for it instead: it reconnects the
//! volume itself, on a growing cadence, for up to [`OutagePolicy::budget`](super::super::outage_slot::OutagePolicy::budget) (fifteen
//! minutes), and then runs the work again. If the volume isn't back by then, the
//! transfer fails exactly as it did before.
//!
//! **Why this layer, and why no new `Volume` method.** The capability already
//! exists on the trait and every session-owning backend implements it: SMB,
//! SFTP, WebDAV, S3, and ADB each answer [`Volume::connection_state`] and each
//! rebuild their session in place, single-flight, on
//! [`Volume::attempt_reconnect`]. What's missing is PATIENCE that respects an
//! operation (a budget, its Cancel, its Pause, its progress UI), and only the
//! transfer layer has an operation. An `await_reconnect` on the trait would put
//! five copies of one backoff loop in five crates, none of which can see a
//! Cancel. So a backend owns ONE attempt and the transfer owns the waiting. A
//! volume with no session to lose (a local disk, an OS-mounted share, MTP)
//! answers `None` and is never waited for; its failures stay what they were.
//!
//! **The guarantees, each load-bearing:**
//!
//! - **Bounded.** Every wait ends: the volume comes back, the budget runs out,
//!   the person cancels, or the backend says it can't come back on its own (a
//!   changed password: [`ConnectionState::NeedsSignIn`] or an auth refusal).
//! - **Cancelable at once.** Every await races `backend_cancel`; a Cancel never
//!   waits out a reconnect attempt or a sleep, and drops the attempt in flight.
//! - **No attempt holds the wait.** Each reconnect gets
//!   [`OutagePolicy::attempt_timeout`](super::super::outage_slot::OutagePolicy::attempt_timeout)
//!   (15 s); one that hasn't returned by then is dropped and counts as still
//!   down. A single hung handshake once held a copy for the whole budget after
//!   the NAS was back (`#406`).
//! - **Pausable.** A Pause parks the wait on the pause gate and stops dialing.
//!   A Resume is a person's action, so the wait gets a fresh budget from there.
//! - **Shared per volume.** An outage record per lane ([`OUTAGES`]) gives every
//!   waiter on that share the same deadline, so a concurrent window's eight
//!   tasks give up together, and a job queued behind one that gave up doesn't
//!   start a second fifteen-minute wait for the same outage. A job CREATED after
//!   the give-up (a Retry, a new copy) is a fresh question and gets a fresh wait.
//! - **Retries nothing it shouldn't.** Only a failure [`retry::is_retryable`]
//!   calls transport ever triggers a wait, so a refusal or a cancel is reported
//!   as it always was.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use tokio::time::Instant;

use super::super::event_sinks::OperationEventSink;
use super::super::state::{WriteOperationState, is_cancelled};
use super::super::types::{WriteOperationPhase, WriteOperationType, WriteProgressEvent};
use super::retry;
use super::transfer_probe::{TaskPhase, current_task_phase, set_task_phase};
use crate::file_system::volume::{ConnectionState, LaneKey, Volume, VolumeError};
use crate::ignore_poison::IgnorePoison;

/// How a wait for a volume ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutageEnd {
    /// Every volume that was out is back: run the work again.
    Back,
    /// The budget ran out with the volume still gone. The caller fails the way
    /// it would have without waiting.
    GaveUp,
    /// The person cancelled while the operation waited.
    Cancelled,
    /// Nothing to wait for: no listed volume is out in a way it can come back
    /// from on its own (no session at all, a session that needs a person to
    /// sign in, or a backend with no reconnect). The caller fails as before.
    NotWaitable,
}

/// How many separate outages one unit of work (a file, a directory level, a
/// pre-check) may wait out before a further one fails it as before.
///
/// A link that drops, comes back, and drops again is still worth riding out a
/// couple of times. One that does it over and over is broken in a way waiting
/// won't fix, and without a cap a flapping link could hold a file forever.
pub(crate) const MAX_OUTAGES_PER_ITEM: u32 = 3;

/// Who to tell that the operation started waiting, for the waits that happen
/// before the transfer probe is registered (the start gate), where nothing else
/// would speak for the operation. Waits inside the transfer don't need one: the
/// probe's heartbeat carries the wait to the UI within a second.
pub(crate) struct OutageAnnounce<'a> {
    pub sink: &'a dyn OperationEventSink,
    pub operation_id: &'a str,
    pub operation_type: WriteOperationType,
}

/// Waits for every volume in `volumes` that's out, one after the other.
///
/// [`OutageEnd::NotWaitable`] when none of them is out in a way it can come back
/// from: the caller's failure is about something else, and stays its failure.
pub(crate) async fn wait_while_out(
    state: &Arc<WriteOperationState>,
    volumes: &[&Arc<dyn Volume>],
    announce: Option<&OutageAnnounce<'_>>,
) -> OutageEnd {
    let mut waited = false;
    for volume in volumes {
        if volume.connection_state() != Some(ConnectionState::Disconnected) {
            continue;
        }
        match wait_for_volume(state, volume, announce).await {
            OutageEnd::Back => waited = true,
            // A volume that's out but can't be waited for is the same answer as
            // one that isn't out: look at the next.
            OutageEnd::NotWaitable => {}
            ended => return ended,
        }
    }
    if waited {
        OutageEnd::Back
    } else {
        OutageEnd::NotWaitable
    }
}

/// Runs `call`, and when it fails on a transport error while one of `volumes`
/// is out, waits for the volume and runs it again, up to
/// [`MAX_OUTAGES_PER_ITEM`] times.
///
/// ❗ Only for a call that's safe to run twice: a read (a listing, a `stat`), or
/// a create whose second run answers `AlreadyExists` and is read as "there".
/// A write that may have half-landed has its own loop (`strategy.rs`), which
/// clears the attempt's partial first.
pub(crate) async fn through_outages<T, E, F, Fut>(
    state: &Arc<WriteOperationState>,
    volumes: &[&Arc<dyn Volume>],
    mut call: F,
) -> Result<T, E>
where
    E: OutageFailure,
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut outages = 0;
    loop {
        let error = match call().await {
            Ok(value) => return Ok(value),
            Err(error) => error,
        };
        match wait_after_failure(state, error.volume_error(), volumes, &mut outages).await {
            OutageEnd::Back => {}
            OutageEnd::Cancelled => return Err(error.into_cancelled()),
            OutageEnd::GaveUp | OutageEnd::NotWaitable => return Err(error),
        }
    }
}

/// What [`through_outages`] needs of a call's error: the `VolumeError` to judge
/// the outage by, and the same failure recast as the cancel that ended a wait.
/// A bare `VolumeError`, or one that also carries where it happened
/// (`volume::transfer_error::PathedVolumeError`), which keeps that through the
/// wait.
pub(crate) trait OutageFailure {
    fn volume_error(&self) -> &VolumeError;
    fn into_cancelled(self) -> Self;
}

impl OutageFailure for VolumeError {
    fn volume_error(&self) -> &VolumeError {
        self
    }

    fn into_cancelled(self) -> Self {
        cancelled()
    }
}

/// After `error`, waits for whichever of `volumes` went away, when that's what
/// `error` was: a transport failure ([`retry::is_retryable`]), with this item's
/// outage budget ([`MAX_OUTAGES_PER_ITEM`], counted in `outages`) not spent and
/// no cancel in. [`OutageEnd::NotWaitable`] for every other failure, which the
/// caller reports as it always did.
pub(crate) async fn wait_after_failure(
    state: &Arc<WriteOperationState>,
    error: &VolumeError,
    volumes: &[&Arc<dyn Volume>],
    outages: &mut u32,
) -> OutageEnd {
    if *outages >= MAX_OUTAGES_PER_ITEM || !retry::is_retryable(error) || is_cancelled(&state.intent) {
        return OutageEnd::NotWaitable;
    }
    let ended = wait_while_out(state, volumes, None).await;
    if ended == OutageEnd::Back {
        *outages += 1;
    }
    ended
}

/// The error a cancel during a wait reports: the cancel is why the work stopped,
/// so the post-loop must see it as one and emit `write-cancelled`, never log the
/// person's own click as a failed transfer.
pub(crate) fn cancelled() -> VolumeError {
    VolumeError::Cancelled("Operation cancelled by user".to_string())
}

/// Waits for ONE volume that reads `Disconnected`. See the module docs.
async fn wait_for_volume(
    state: &Arc<WriteOperationState>,
    volume: &Arc<dyn Volume>,
    announce: Option<&OutageAnnounce<'_>>,
) -> OutageEnd {
    let policy = state.outage_policy;
    let lane = volume.lane_key();
    let name = volume.name().to_owned();
    let (mut deadline, _membership) = match OUTAGES.join(&lane, state.born, policy.budget) {
        Joined::Waiting { deadline, membership } => (deadline, membership),
        Joined::AlreadyGaveUp => {
            // This outage was waited out already, by work queued before it gave
            // up. One look before agreeing: the volume may be back by now, and
            // with no pane on it, nothing else would have noticed.
            log::info!(
                target: "copy",
                "volume outage: {name} already timed out for work queued before; trying it once"
            );
            return match attempt_once(state, volume, None).await {
                Attempt::Reconnected => {
                    OUTAGES.back(&lane);
                    OutageEnd::Back
                }
                Attempt::Cancelled => OutageEnd::Cancelled,
                Attempt::Failed | Attempt::Hopeless => OutageEnd::GaveUp,
            };
        }
    };

    let _slot = state.outage.begin(&name);
    let _phase = TaskPhaseGuard::enter(TaskPhase::WaitingForVolume);
    log::warn!(
        target: "copy",
        "volume outage: {name} went away; waiting up to {:?} for it to come back",
        deadline.saturating_duration_since(Instant::now()),
    );
    if let Some(announce) = announce {
        announce_wait(state, announce);
    }

    let started = Instant::now();
    let mut failed_attempts = 0;
    let mut next_attempt = Instant::now();
    loop {
        if is_cancelled(&state.intent) {
            return OutageEnd::Cancelled;
        }
        if state.pause_gate.is_paused() {
            // A paused operation dials nothing and its budget stops with it:
            // whoever paused it is deciding, and a Resume is a fresh question.
            if state.stop_or_park_async().await {
                return OutageEnd::Cancelled;
            }
            deadline = deadline.max(Instant::now() + policy.budget);
            next_attempt = Instant::now();
            continue;
        }
        match volume.connection_state() {
            Some(live) if live.is_live() => return came_back(&lane, &name, started),
            Some(ConnectionState::Disconnected) => {}
            // A sign-in, a host key, or no session at all: not something time
            // fixes. The pane offers the sign-in; the transfer fails as before.
            _ => return OutageEnd::NotWaitable,
        }
        let now = Instant::now();
        if now >= deadline {
            OUTAGES.gave_up(&lane);
            log::warn!(
                target: "copy",
                "volume outage: {name} didn't come back within {:?}; giving up",
                now.saturating_duration_since(started),
            );
            return OutageEnd::GaveUp;
        }
        if now >= next_attempt {
            match attempt_once(state, volume, Some(deadline)).await {
                Attempt::Reconnected => return came_back(&lane, &name, started),
                Attempt::Cancelled => return OutageEnd::Cancelled,
                Attempt::Hopeless => return OutageEnd::NotWaitable,
                Attempt::Failed => {
                    failed_attempts += 1;
                    next_attempt = Instant::now() + policy.delay_after(failed_attempts);
                }
            }
            continue;
        }
        // Sleep to the next attempt, the deadline, or the next look (a pause, or
        // a reconnect someone else made), whichever comes first. A cancel ends
        // it at once.
        let wake = next_attempt.min(deadline).min(now + policy.poll_every);
        tokio::select! {
            biased;
            () = state.backend_cancel.cancelled() => return OutageEnd::Cancelled,
            () = tokio::time::sleep_until(wake) => {}
        }
    }
}

/// Holds the current copy task (if any) in a phase, and puts back the one it
/// was in when it drops: a walker that waited goes back to `walking`, not to a
/// wait that's over. A no-op outside a copy task.
struct TaskPhaseGuard(Option<TaskPhase>);

impl TaskPhaseGuard {
    fn enter(phase: TaskPhase) -> Self {
        let previous = current_task_phase();
        set_task_phase(phase);
        Self(previous)
    }
}

impl Drop for TaskPhaseGuard {
    fn drop(&mut self) {
        if let Some(previous) = self.0 {
            set_task_phase(previous);
        }
    }
}

fn came_back(lane: &LaneKey, name: &str, started: Instant) -> OutageEnd {
    OUTAGES.back(lane);
    log::info!(
        target: "copy",
        "volume outage: {name} is back after {:?}; carrying on",
        started.elapsed(),
    );
    OutageEnd::Back
}

enum Attempt {
    Reconnected,
    Failed,
    /// The backend can't come back on its own: no reconnect at all, or a
    /// credential only a person can fix.
    Hopeless,
    Cancelled,
}

/// One reconnect, raced against a cancel and (when given) the deadline, and
/// bounded by the policy's attempt timeout: an attempt that never returns (an
/// SMB handshake a server accepted and never answered, `#406`) is dropped and
/// counts as still down, so the next tick dials afresh instead of the whole
/// wait sitting on it until the budget runs out.
async fn attempt_once(
    state: &Arc<WriteOperationState>,
    volume: &Arc<dyn Volume>,
    deadline: Option<Instant>,
) -> Attempt {
    let until_deadline = async {
        match deadline {
            Some(at) => tokio::time::sleep_until(at).await,
            None => std::future::pending().await,
        }
    };
    let attempt_timeout = state.outage_policy.attempt_timeout;
    let result = tokio::select! {
        biased;
        () = state.backend_cancel.cancelled() => return Attempt::Cancelled,
        // The loop reads the deadline next and gives up; an attempt still
        // dialing then is dropped, which the backends' single-flight tolerates.
        () = until_deadline => return Attempt::Failed,
        result = tokio::time::timeout(attempt_timeout, volume.attempt_reconnect()) => match result {
            Ok(result) => result,
            Err(_elapsed) => {
                log::warn!(
                    target: "copy",
                    "volume outage: a reconnect attempt to {} didn't return within {attempt_timeout:?}; \
                     dropping it and trying again on the next tick",
                    volume.name(),
                );
                return Attempt::Failed;
            }
        },
    };
    match result {
        Ok(()) => Attempt::Reconnected,
        // No reconnect story at all, or the server refused the saved
        // credentials (the backend already asked the pane to offer Sign in, and
        // dialing again risks locking the account).
        Err(VolumeError::NotSupported | VolumeError::PermissionDenied { .. } | VolumeError::NeedsPassword { .. }) => {
            Attempt::Hopeless
        }
        Err(error) => {
            log::debug!(target: "copy", "volume outage: reconnect attempt failed: {error}");
            Attempt::Failed
        }
    }
}

/// Tells the windows the operation started waiting: the newest tick it emitted,
/// re-sent so `enrich_progress` classifies it (`Reconnecting`, with the volume's
/// name), or an opening scan tick when it hasn't emitted one yet.
fn announce_wait(state: &Arc<WriteOperationState>, announce: &OutageAnnounce<'_>) {
    let event = state.last_progress().unwrap_or_else(|| {
        WriteProgressEvent::new(
            announce.operation_id.to_owned(),
            announce.operation_type,
            WriteOperationPhase::Scanning,
            None,
            0,
            0,
            0,
            0,
        )
    });
    state.emit_progress_via_sink(announce.sink, event);
}

// ============================================================================
// The lane's outage record
// ============================================================================

/// One outage per lane (`Volume::lane_key`, the physical resource: one share,
/// one server, one device), shared by every operation waiting on it.
static OUTAGES: LazyLock<Outages> = LazyLock::new(Outages::default);

// DEFAULT-OK: an empty map is "no lane is in an outage", which is true until a
// waiter starts one.
#[derive(Default)]
struct Outages {
    records: Mutex<HashMap<LaneKey, Outage>>,
}

struct Outage {
    /// Tells a stale membership from this record's, so a waiter leaving an
    /// outage that ended (and was replaced) can't remove the new one.
    id: u64,
    started: Instant,
    gave_up_at: Option<std::time::Instant>,
    waiters: usize,
}

enum Joined {
    Waiting { deadline: Instant, membership: Membership },
    AlreadyGaveUp,
}

static NEXT_OUTAGE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl Outages {
    /// Join the lane's current outage, or start one.
    ///
    /// - None running: start one now, with the full budget.
    /// - One running: share its deadline.
    /// - One that gave up, for an operation created BEFORE it gave up and within
    ///   a budget of it: [`Joined::AlreadyGaveUp`]. That's a job queued behind
    ///   the one that waited, and the outage it would wait out has already been
    ///   waited out once.
    /// - One that gave up, for an operation created after it (a Retry, a new
    ///   copy) or long after: a fresh outage, because that's a fresh question.
    fn join(&self, lane: &LaneKey, born: std::time::Instant, budget: Duration) -> Joined {
        let mut records = self.records.lock_ignore_poison();
        let now = Instant::now();
        if let Some(outage) = records.get_mut(lane) {
            match outage.gave_up_at {
                None => {
                    outage.waiters += 1;
                    return Joined::Waiting {
                        deadline: outage.started + budget,
                        membership: Membership {
                            lane: lane.clone(),
                            id: outage.id,
                        },
                    };
                }
                Some(gave_up_at) if born <= gave_up_at && gave_up_at.elapsed() < budget => {
                    return Joined::AlreadyGaveUp;
                }
                Some(_) => {}
            }
        }
        let id = NEXT_OUTAGE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        records.insert(
            lane.clone(),
            Outage {
                id,
                started: now,
                gave_up_at: None,
                waiters: 1,
            },
        );
        Joined::Waiting {
            deadline: now + budget,
            membership: Membership { lane: lane.clone(), id },
        }
    }

    /// The volume is back: the outage is over for everyone.
    fn back(&self, lane: &LaneKey) {
        self.records.lock_ignore_poison().remove(lane);
    }

    /// The outage outlasted its budget. Kept, so the jobs queued behind the one
    /// that waited don't each wait again (see [`join`](Self::join)).
    fn gave_up(&self, lane: &LaneKey) {
        if let Some(outage) = self.records.lock_ignore_poison().get_mut(lane) {
            outage.gave_up_at.get_or_insert_with(std::time::Instant::now);
        }
    }

    fn leave(&self, membership: &Membership) {
        let mut records = self.records.lock_ignore_poison();
        let Some(outage) = records.get_mut(&membership.lane) else {
            return;
        };
        if outage.id != membership.id {
            return;
        }
        outage.waiters = outage.waiters.saturating_sub(1);
        // Everyone who waited stopped (cancelled, or the work ended some other
        // way) without a verdict: there's no outage left to share. A give-up
        // stays, for the queued jobs that come after it.
        if outage.waiters == 0 && outage.gave_up_at.is_none() {
            records.remove(&membership.lane);
        }
    }
}

/// A waiter's place in its lane's outage; leaving drops it, whichever way the
/// wait ended.
struct Membership {
    lane: LaneKey,
    id: u64,
}

impl Drop for Membership {
    fn drop(&mut self) {
        OUTAGES.leave(self);
    }
}

#[cfg(test)]
#[path = "volume_outage_tests.rs"]
mod tests;
