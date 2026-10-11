//! Sources that left while an operation waited in the queue.
//!
//! A transfer that auto-queued behind a long one on the same device can wait many
//! minutes, and the person keeps working meanwhile: they may move or delete the
//! very files it was going to touch (ERR-962JD: a queued move of 22 mp3s whose
//! files the person had moved by hand stopped with a bare "no longer exists").
//! So an operation that WAITED checks its top-level sources when its turn comes,
//! and when some are gone it asks the person instead of failing on the first one:
//!
//! - **Some gone**: it emits `write-sources-gone` (`AwaitingAnswer`) and parks on
//!   [`SourcesGoneSlot`], holding its lanes the way a conflict prompt does.
//!   [`continue_without_gone_sources`] carries it on with the rest, each gone
//!   source reported `Skipped` with `source_removed: true`; a cancel ends it
//!   through the ordinary cancel path, which abandons the slot.
//! - **All gone**: nothing to ask. It reports each source `Skipped`, emits
//!   `write-sources-gone` (`NothingLeft`) so the person learns why, and completes
//!   with nothing done. ❌ Not a `write-error`: nothing went wrong, the work just
//!   isn't there any more.
//!
//! **One existence probe per top-level source, ❌ never a walk**, and through the
//! source `Volume` on a volume route, so SMB, MTP, and the rest answer it the same
//! way. Only a POSITIVE absence counts as gone: a source that can't be looked at
//! (permission, a dropped connection) is left for the engine, whose own typed
//! error says what really happened. "We couldn't look" is not "it's gone", and a
//! dead mount must never read as "all your files were moved".
//!
//! The check rides the pre-flight every starter already runs after admission
//! (`source_binding::retain_bound_sources*`), for an operation with no binding:
//! a bound one already drops a changed source on its own terms.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tokio::sync::oneshot;

use super::event_sinks::OperationEventSink;
use super::source_binding::{announce_empty_batch, local_source_is_gone};
use super::state::{WRITE_OPERATION_STATE, WriteOperationState, is_cancelled};
use super::types::{
    CancelRollback, SourceItemOutcome, SourcesGoneAnswerOutcome, SourcesGoneNext, WriteCancelledEvent,
    WriteOperationType, WriteSourceItemDoneEvent, WriteSourcesGoneEvent,
};
use crate::file_system::volume::{Volume, VolumeError};
use crate::ignore_poison::IgnorePoison;

/// How many gone sources the question names. The count beside it is the whole
/// truth; the names only help the person recognize what moved.
const GONE_SAMPLE_LEN: usize = 5;

/// Where a queued operation's sources-gone question stands: asked (with the
/// sender the parked operation listens on), or not.
// DEFAULT-OK: the empty slot means "nothing asked", which is the truth for every
// operation until its queue check finds a source gone.
#[derive(Default)]
pub(crate) struct SourcesGoneSlot {
    asking: Mutex<Option<Asking>>,
}

struct Asking {
    carry_on: oneshot::Sender<()>,
    question: WriteSourcesGoneEvent,
}

impl SourcesGoneSlot {
    /// Arms the slot with `question` and hands back what the operation waits on.
    /// Arm BEFORE emitting: an answer can only follow a question someone saw.
    fn arm(&self, question: WriteSourcesGoneEvent) -> oneshot::Receiver<()> {
        let (carry_on, answer) = oneshot::channel();
        *self.asking.lock_ignore_poison() = Some(Asking { carry_on, question });
        answer
    }

    /// The question this operation is parked on, for a reader that wasn't
    /// listening when it went out. `None` once answered or abandoned.
    pub(crate) fn pending(&self) -> Option<WriteSourcesGoneEvent> {
        self.asking
            .lock_ignore_poison()
            .as_ref()
            .map(|asking| asking.question.clone())
    }

    /// Drops the pending question, so the parked operation's receiver returns
    /// `Err` and the operation reads it as a cancel. What a cancel does.
    pub(crate) fn abandon(&self) {
        self.asking.lock_ignore_poison().take();
    }

    /// Delivers "carry on". `false` when nothing was asked, or the operation
    /// stopped listening.
    fn carry_on(&self) -> bool {
        let Some(asking) = self.asking.lock_ignore_poison().take() else {
            return false;
        };
        asking.carry_on.send(()).is_ok()
    }
}

/// Carries a queued operation on with the sources that are still there. The
/// person's "Move the rest" (or "Copy the rest", …); cancelling is the ordinary
/// `cancel_operation`.
pub fn continue_without_gone_sources(operation_id: &str) -> SourcesGoneAnswerOutcome {
    let Some(state) = WRITE_OPERATION_STATE.get(operation_id) else {
        log::info!(target: "queued_sources", "continue for op={operation_id}: no such operation");
        return SourcesGoneAnswerOutcome::UnknownOperation;
    };
    if state.sources_gone.carry_on() {
        log::info!(target: "queued_sources", "op={operation_id}: carrying on without the sources that left");
        SourcesGoneAnswerOutcome::Continued
    } else {
        log::info!(target: "queued_sources", "continue for op={operation_id}: it isn't asking");
        SourcesGoneAnswerOutcome::NotAsking
    }
}

/// The question `operation_id` is parked on, if any. For a surface that arrives
/// after the broadcast.
pub(crate) fn pending_sources_gone(operation_id: &str) -> Option<WriteSourcesGoneEvent> {
    WRITE_OPERATION_STATE.get(operation_id)?.sources_gone.pending()
}

/// The positions in `sources` this operation may still touch, after checking
/// what left while it queued, or `None` when it's over (its terminal event is
/// already out). Local filesystem; blocks the calling thread while the person
/// decides, so call it only from the operation's blocking task.
///
/// Every position comes back untouched for an operation that never waited.
pub(crate) fn kept_after_queue_local(
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: WriteOperationType,
    sources: &[PathBuf],
) -> Option<Vec<usize>> {
    let all = || (0..sources.len()).collect();
    let Some(state) = WRITE_OPERATION_STATE.get(operation_id) else {
        return Some(all());
    };
    if !state.waited_in_queue() {
        return Some(all());
    }
    let gone: Vec<bool> = sources.iter().map(|source| local_source_is_gone(source)).collect();
    match plan(events, operation_id, operation_type, sources, &gone, &state) {
        Plan::Proceed(kept) => Some(kept),
        Plan::Stopped => None,
        Plan::Ask(answer, kept) => {
            let carried_on = answer.blocking_recv().is_ok();
            settle_answer(events, operation_id, operation_type, sources, &gone, carried_on).then_some(kept)
        }
    }
}

/// [`kept_after_queue_local`] for a route that runs through a `Volume`: each
/// source is probed on `volume`, and the wait is async.
pub(crate) async fn kept_after_queue_on(
    volume: &dyn Volume,
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: WriteOperationType,
    sources: &[PathBuf],
) -> Option<Vec<usize>> {
    let all = || (0..sources.len()).collect();
    let Some(state) = WRITE_OPERATION_STATE.get(operation_id) else {
        return Some(all());
    };
    if !state.waited_in_queue() {
        return Some(all());
    }
    let mut gone = Vec::with_capacity(sources.len());
    for source in sources {
        gone.push(volume_source_is_gone(volume, source).await);
    }
    match plan(events, operation_id, operation_type, sources, &gone, &state) {
        Plan::Proceed(kept) => Some(kept),
        Plan::Stopped => None,
        Plan::Ask(answer, kept) => {
            let carried_on = answer.await.is_ok();
            settle_answer(events, operation_id, operation_type, sources, &gone, carried_on).then_some(kept)
        }
    }
}

/// What the probe results mean for the operation.
enum Plan {
    /// Nothing left; carry on with these positions (all of them).
    Proceed(Vec<usize>),
    /// Every source is gone: the operation completed with nothing to do.
    Stopped,
    /// Some are gone: wait for the answer, then keep these positions.
    Ask(oneshot::Receiver<()>, Vec<usize>),
}

fn plan(
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: WriteOperationType,
    sources: &[PathBuf],
    gone: &[bool],
    state: &WriteOperationState,
) -> Plan {
    let kept: Vec<usize> = (0..sources.len()).filter(|position| !gone[*position]).collect();
    let gone_count = sources.len() - kept.len();
    if gone_count == 0 {
        return Plan::Proceed(kept);
    }
    let question = |next| WriteSourcesGoneEvent {
        operation_id: operation_id.to_string(),
        operation_type,
        total_count: sources.len(),
        gone_count,
        gone_sample: sources
            .iter()
            .zip(gone)
            .filter(|(_, gone)| **gone)
            .take(GONE_SAMPLE_LEN)
            .map(|(source, _)| source.display().to_string())
            .collect(),
        next,
    };

    if kept.is_empty() {
        log::info!(
            target: "queued_sources",
            "op={operation_id}: all {gone_count} source(s) left while it was queued; nothing to do"
        );
        announce_gone(events, operation_id, sources, gone);
        events.emit_sources_gone(question(SourcesGoneNext::NothingLeft));
        announce_empty_batch(events, operation_id, operation_type, sources.len());
        return Plan::Stopped;
    }

    log::info!(
        target: "queued_sources",
        "op={operation_id}: {gone_count} of {} source(s) left while it was queued; asking",
        sources.len()
    );
    let asked = question(SourcesGoneNext::AwaitingAnswer);
    let answer = state.sources_gone.arm(asked.clone());
    // A cancel that landed before the arm abandoned an empty slot, so look
    // again now that the question is in place; one landing after it drops the
    // sender itself.
    if is_cancelled(&state.intent) {
        state.sources_gone.abandon();
    }
    events.emit_sources_gone(asked);
    Plan::Ask(answer, kept)
}

/// Reports the answer's consequences. `true` when the operation carries on.
fn settle_answer(
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: WriteOperationType,
    sources: &[PathBuf],
    gone: &[bool],
    carried_on: bool,
) -> bool {
    if carried_on {
        announce_gone(events, operation_id, sources, gone);
        return true;
    }
    log::info!(target: "queued_sources", "op={operation_id} was cancelled while asking about its sources");
    events.emit_cancelled(WriteCancelledEvent {
        operation_id: operation_id.to_string(),
        operation_type,
        files_processed: 0,
        rollback: CancelRollback::none(),
    });
    false
}

/// One `Skipped` per gone source, with `source_removed: true`: the operation
/// decided nothing about them, and they really are gone from where it looked.
fn announce_gone(events: &dyn OperationEventSink, operation_id: &str, sources: &[PathBuf], gone: &[bool]) {
    for (source, _) in sources.iter().zip(gone).filter(|(_, gone)| **gone) {
        events.emit_source_item_done(WriteSourceItemDoneEvent {
            operation_id: operation_id.to_string(),
            source_path: source.display().to_string(),
            source_removed: true,
            outcome: SourceItemOutcome::Skipped,
        });
    }
}

/// `local_source_is_gone` asked of a volume. ❌ Not `Volume::exists`, which
/// answers `false` for an unreachable source too.
async fn volume_source_is_gone(volume: &dyn Volume, source: &Path) -> bool {
    matches!(volume.get_metadata(source).await, Err(VolumeError::NotFound(_)))
}

#[cfg(test)]
#[path = "queued_sources_tests.rs"]
mod tests;
