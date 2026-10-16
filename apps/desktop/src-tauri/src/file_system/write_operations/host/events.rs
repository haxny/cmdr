//! Engine-wide facts: what happened to the engine as a whole, rather than to
//! one operation (that's `OperationEventSink`).
//!
//! The engine says "the queue's membership changed, here it is" or "a rename
//! landed in a zip and worked"; the host decides that the first becomes an
//! `operations-changed` event and the second a PostHog capture under
//! `rename_used`. Cmdr's answer is `TauriEngineEvents`, in the app's
//! `operations_host::events`.

use cmdr_operation_log::types::Initiator;

use super::super::types::{MoveLeftoversKeptEvent, OperationsChanged};
use super::error_report::OperationErrorReport;

/// Everything the engine reports that belongs to no single operation.
///
/// Every method is required, so an answer that forgets one doesn't compile.
/// ❗ Each is called with no engine lock held and returns at once: an answer
/// that blocks stalls whichever operation reached it.
pub(crate) trait EngineEvents: Send + Sync {
    /// The registry's membership or lifecycle status changed. `snapshot` is the
    /// whole thin row set at that moment, retained failures included.
    fn operations_changed(&self, snapshot: OperationsChanged);

    /// The set of volumes an in-flight operation touches changed. Deduped:
    /// called only when membership moved, never per progress tick. Sorted, and
    /// never holding the default `root` volume.
    fn volumes_busy_changed(&self, volume_ids: Vec<String>);

    /// An operation touching `volume_ids` started running. Exactly one
    /// [`Self::transfer_finished`] follows with the same ids, however the
    /// operation ends, panics included.
    fn transfer_started(&self, volume_ids: &[String]);

    /// The operation that started on `volume_ids` is over.
    fn transfer_finished(&self, volume_ids: &[String]);

    /// The leftover sweep kept a non-empty staging folder on a drive.
    fn staging_leftovers_kept(&self, notice: MoveLeftoversKeptEvent);

    /// An instant metadata op (rename, new folder, new file) finished, however
    /// it ended. Exactly one per op, from the wrapper around its whole driver.
    fn instant_mutation_settled(&self, fact: InstantMutationFact);

    /// Something went wrong badly enough to raise an error report.
    fn error_report(&self, report: OperationErrorReport);
}

/// The answer for a host that wants none of it: a bench, a tool, an engine
/// before the app wired its own.
#[cfg_attr(
    test,
    expect(dead_code, reason = "a test binary falls back to the app's own answers")
)]
pub(super) struct NoEngineEvents;

impl EngineEvents for NoEngineEvents {
    fn operations_changed(&self, _snapshot: OperationsChanged) {}
    fn volumes_busy_changed(&self, _volume_ids: Vec<String>) {}
    fn transfer_started(&self, _volume_ids: &[String]) {}
    fn transfer_finished(&self, _volume_ids: &[String]) {}
    fn staging_leftovers_kept(&self, _notice: MoveLeftoversKeptEvent) {}
    fn instant_mutation_settled(&self, _fact: InstantMutationFact) {}
    fn error_report(&self, _report: OperationErrorReport) {}
}

/// How one instant metadata op ended, as categories only: no name, no path, no
/// reason it failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InstantMutationFact {
    pub op: InstantOp,
    /// Who asked: the person, the agent, or an AI client.
    pub initiator: Initiator,
    pub target: InstantTarget,
    pub outcome: InstantOutcome,
}

/// The three instant metadata ops. A closed set, so a fourth one is a compile
/// error in every answer rather than a silently uncounted op.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InstantOp {
    Rename,
    CreateFolder,
    CreateFile,
}

/// Where an instant metadata op landed. An `Archive` target is a `.zip` rewrite
/// routed to the managed archive-edit driver, so it costs orders of magnitude more
/// than the same gesture on a real filesystem and its `done` means "the edit
/// STARTED" (its completion rides `archive_edit_completed`). Folding the two into
/// one number would make the rename/create counts unreadable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InstantTarget {
    Volume,
    Archive,
}

/// The outcome vocabulary of an instant metadata op: it either landed or it didn't.
/// There's no cancel and no conflict prompt on this path (`manager::run_instant`
/// inserts no `WriteOperationState`), so two outcomes cover it completely.
///
/// The failure arm is the point of having an outcome at all: a rename that keeps
/// bouncing off a taken name or a read-only volume is a usability answer, and a
/// success-only count can't tell that from a feature nobody reaches for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InstantOutcome {
    Done,
    Failed,
}

impl InstantMutationFact {
    /// The fact for an op that ended with `result`.
    pub(crate) fn of<T, E>(op: InstantOp, initiator: Initiator, target: InstantTarget, result: &Result<T, E>) -> Self {
        let outcome = match result {
            Ok(_) => InstantOutcome::Done,
            Err(_) => InstantOutcome::Failed,
        };
        Self {
            op,
            initiator,
            target,
            outcome,
        }
    }
}
