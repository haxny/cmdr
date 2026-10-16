//! The manager's wire vocabulary: what it tells the windows (`OperationSnapshot`,
//! `OperationsChanged`) and what each request answers (`PauseOutcome`,
//! `PauseAllOutcome`, `QueueMoveOutcome`). Serde and specta structs only; none of
//! it touches the manager's lock, which is why it lives apart from the admission
//! machinery in `../manager.rs`.

use cmdr_operation_log::types::OpKind;

use super::super::types::{ItemKinds, LifecycleStatus, WriteOperationError, WriteOperationType};

/// What a pause or resume request actually did, so every caller can say so
/// instead of assuming it worked. Pause and resume share it: the three outcomes
/// are the same in both directions.
///
/// The distinction is load-bearing at the MCP boundary, where an agent acts on
/// the answer: `Applied` means "the queue has stopped", `NotApplicable` means
/// nothing changed and nothing is remembered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum PauseOutcome {
    /// The record flipped: `Running`→`Paused` (the operation parks at its next
    /// boundary — between files while writing, between entries while scanning)
    /// or `Paused`→`Running`.
    Applied,
    /// The operation is already in the state asked for (pausing a `Paused` one,
    /// resuming a `Running` one). Nothing changed because nothing had to, so a
    /// caller that retries its own request gets an honest yes rather than a
    /// refusal.
    AlreadyInState,
    /// Nothing happened and nothing is remembered: the operation is queued,
    /// over, or unknown.
    NotApplicable,
}

/// What a whole `pause_all` / `resume_all` sweep did: one count per
/// [`PauseOutcome`] it collected. Shared by both directions, for the reason
/// `PauseOutcome` is.
///
/// A sweep touches several operations, so "it worked" isn't a thing it can
/// truthfully say. A caller that assumes one tells its user (or its agent) the
/// device is free when a scan is still running or the set was empty all along,
/// which is the whole reason the per-operation outcome exists.
// DEFAULT-OK: all-zero is exactly what an empty sweep collected, and it's the value
// `FromIterator` folds onto. It makes no claim about a disk: `total() == 0` reads as "the
// sweep found nothing to ask", which is the truth in that case, and the reply builder
// says so out loud rather than reporting a success.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PauseAllOutcome {
    /// Operations that flipped right now.
    pub applied: usize,
    /// Operations already sitting where the caller wants them.
    pub already_in_state: usize,
    /// Operations the sweep couldn't touch. From a sweep this is the settle
    /// race alone: the snapshot named them, and they finished before the call
    /// landed.
    pub not_applicable: usize,
}

impl PauseAllOutcome {
    /// How many operations the sweep asked about.
    pub fn total(self) -> usize {
        self.applied + self.already_in_state + self.not_applicable
    }

    /// Whether the caller's intent now holds for at least one operation.
    pub fn took_effect_anywhere(self) -> bool {
        self.applied > 0
    }
}

/// Folding the per-operation outcomes into the sweep's answer. Keeping the
/// aggregation here (rather than inline in the loop) is what makes it testable
/// without touching the process-global manager.
impl FromIterator<PauseOutcome> for PauseAllOutcome {
    fn from_iter<I: IntoIterator<Item = PauseOutcome>>(outcomes: I) -> Self {
        let mut totals = Self::default();
        for outcome in outcomes {
            match outcome {
                PauseOutcome::Applied => totals.applied += 1,
                PauseOutcome::AlreadyInState => totals.already_in_state += 1,
                PauseOutcome::NotApplicable => totals.not_applicable += 1,
            }
        }
        totals
    }
}

/// Which way a waiting operation moves in line: one place nearer the front
/// (admitted sooner) or one place nearer the back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum QueueMoveDirection {
    Earlier,
    Later,
}

/// What a request to move a waiting operation did. Like [`PauseOutcome`], it
/// rides out to the caller so nobody announces a move that didn't happen.
///
/// `position` and `waiting` count WAITING operations only (1-based), since
/// they're the only ones in line: a running or paused one holds its lanes and
/// has no place to trade. They describe the line after the request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum QueueMoveOutcome {
    /// It traded places with its waiting neighbor and now sits at `position`.
    Moved { position: u32, waiting: u32 },
    /// It was already first (moving earlier) or last (moving later). Nothing
    /// changed and nothing was broadcast.
    AtEdge { position: u32, waiting: u32 },
    /// Asked to go to the place it already holds (a drop where it started).
    /// Nothing changed and nothing was broadcast.
    Unchanged { position: u32, waiting: u32 },
    /// Not waiting: running, paused, settled, or never registered. Only a
    /// waiting operation has a place in line to change.
    NotQueued,
}

/// Best-effort human-readable source/destination summary for the queue window.
// DEFAULT-OK: both sides `None` means "couldn't summarize", and the queue window renders
// that absence rather than an empty string.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OperationSummaryText {
    pub source: Option<String>,
    pub destination: Option<String>,
    /// What the operation acts on, spelled out for the queue row. `None` where
    /// the starter has no top-level items to name (a rename, a new folder, an
    /// edit inside an archive): the row falls back to the two names above.
    pub items: Option<OperationItems>,
}

/// The items an operation acts on and where they go, for the queue row's
/// "Copying 2 folders and 1 file Dropbox → cmdr-s3-test/target/folder".
///
/// Static for the op's whole life and bounded (at most
/// [`ITEM_NAMES_SHOWN`](super::super::summary::ITEM_NAMES_SHOWN) names), which
/// is why it rides the thin snapshot: it changes only when membership does.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OperationItems {
    /// How many top-level items, files and folders together.
    pub count: u32,
    /// The same items split by kind, when the starter said; `None` reads as
    /// "3 items". Dropped when it doesn't add up to `count`.
    pub kinds: Option<ItemKinds>,
    /// The first top-level names, in order, for the row's tooltip. The rest is
    /// `count` minus these.
    pub names: Vec<String>,
    /// The folder they come from, as a person reads it (`~/Dropbox`,
    /// `cmdr-s3-test/photos`). `None` when the sources share no parent.
    pub from: Option<String>,
    /// The folder they go to, in the same form. `None` for a delete or trash.
    pub to: Option<String>,
}

/// One thin registry snapshot row (membership + lifecycle status, NOT 200 ms
/// progress). The queue window subscribes to `operations-changed` for the row
/// set and to the per-file `write-progress` stream for live bars.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OperationSnapshot {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    pub status: LifecycleStatus,
    pub source: Option<String>,
    pub destination: Option<String>,
    /// See [`OperationDescriptor::supports_rollback`](super::OperationDescriptor::supports_rollback).
    pub supports_rollback: bool,
    /// See [`OperationDescriptor::reverses`](super::OperationDescriptor::reverses). Static for the op's whole life,
    /// which is why it rides the thin snapshot rather than the 200 ms progress
    /// tick.
    pub reverses: Option<OpKind>,
    /// Why the operation stopped, on a retained `Failed` row only; `None` on
    /// every live row. The typed variant, never rendered prose: the frontend's
    /// `transfer-error-messages.ts` owns the wording. DETAILS § "Retained
    /// failures".
    pub error: Option<WriteOperationError>,
    /// The queue row may offer Retry: a retained failure that kept a way to
    /// start again (`routing/retry.rs`), with no Retry for it already starting.
    /// `false` on every live row. Whether the ERROR is worth retrying is the
    /// frontend's call (`offersRetry`), the same one the error dialog makes.
    pub can_retry: bool,
    /// See [`OperationSummaryText::items`].
    pub items: Option<OperationItems>,
}

/// Typed `operations-changed` Tauri event carrying the thin registry snapshot
/// (membership + lifecycle status, NOT 200 ms progress). The struct name
/// kebab-cases to `operations-changed`. The queue window subscribes to it for
/// the row set and to the per-file `write-progress` stream for live bars.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct OperationsChanged {
    pub operations: Vec<OperationSnapshot>,
}
