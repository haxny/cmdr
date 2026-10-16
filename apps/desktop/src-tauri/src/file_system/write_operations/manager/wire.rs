//! The manager's request vocabulary: what each request answers (`PauseOutcome`,
//! `PauseAllOutcome`, `QueueMoveOutcome`). Serde and specta structs only; none of
//! it touches the manager's lock, which is why it lives apart from the admission
//! machinery in `../manager.rs`. What the manager TELLS the windows
//! (`OperationSnapshot`, `OperationsChanged`) is vocabulary the engine-wide
//! events seam carries too, so it lives in `types`.

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
