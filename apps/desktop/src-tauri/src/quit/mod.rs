//! The quit gate: the backend owns the decision to exit.
//!
//! Quitting with work in flight asks the user, counts down on a timer this
//! module owns, then stops everything and exits inside a hard budget. Both ways
//! out of the app funnel through [`request_quit`] — `RunEvent::ExitRequested`
//! (⌘Q, the menu, `AppHandle::exit`) and the main window's `CloseRequested`.
//!
//! **The countdown is Rust's, and the dialog only displays it.** A frontend
//! `setInterval` would never fire in a wedged webview, and a wedged UI is a
//! likely reason someone is quitting in the first place. The frontend renders,
//! counts down for show, and answers with `quit_confirm` / `quit_cancel`; the
//! gate quits on whichever lands first, its own deadline or the answer.
//!
//! Architecture, the budget's arithmetic, and the teardown's ordering:
//! `DETAILS.md`.

pub(crate) mod commands;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use crate::ignore_poison::IgnorePoison;
use crate::tag_editor::at_quit::TagsAtQuit;
use cmdr_operations::{LifecycleStatus, OperationSnapshot, Operations, WriteOperationType};

/// How long the dialog counts down before the gate quits on its own.
///
/// macOS gives an app a limited window to answer a logout or restart before the
/// system complains or cancels the restart, and Tauri surfaces no signal that
/// separates that case from a plain ⌘Q (`RunEvent::ExitRequested` carries only
/// an exit code, and a user-driven quit carries `None` either way). So the one
/// countdown has to fit the strictest case: 15 s plus [`DRAIN`] plus the rest of
/// the teardown leaves margin where 20 s did not.
pub(crate) const COUNTDOWN: Duration = Duration::from_secs(15);

/// How long the teardown waits for the cooperative cancel to be obeyed before
/// firing the hard abort. The whole decision-to-process-gone budget is 2 s.
pub(crate) const DRAIN: Duration = Duration::from_millis(1_500);

/// How often the drain re-checks whether the operations answered. Small enough
/// that a prompt cancel isn't waited out, large enough to cost nothing.
const DRAIN_POLL: Duration = Duration::from_millis(20);

/// How long the teardown waits for a tag write or undo still running (fork, the tag editor's
/// Tags windows). Each file is written whole (copy, check, replace), but a quit between files
/// would leave a book half written, so the teardown waits for the write to end; a write that
/// stands still this long is left, and the process exits as it would at a logout.
pub(crate) const TAG_WRITE_WAIT: Duration = Duration::from_secs(10);

/// Emitted when a quit is held: what's still running, and how long the user has.
///
/// Kebab-cases to `quit-requested`. The `countdownMs` is what the dialog counts
/// down for display; it is NOT the authority, [`QuitGate`]'s thread is.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct QuitRequested {
    /// The non-instant operations holding the quit, in registration order.
    pub operations: Vec<OperationSnapshot>,
    /// Milliseconds from this event to the automatic quit.
    pub countdown_ms: u32,
    /// The Tags windows' share: unwritten tag edits, and a tag write the quit waits for.
    pub tags: TagsAtQuit,
    /// Whether the gate quits on its own when `countdown_ms` runs out. `false` when only
    /// unwritten tag edits hold the quit ([`counts_down`]): the prompt waits for an answer.
    pub counts_down: bool,
}

/// Emitted when a held quit is called off, so every window takes its prompt down.
///
/// The dialog closes itself when the person clicks "Keep working". The gate can
/// also be released from somewhere that isn't holding a prompt (the MCP `quit`
/// surface), and a dialog left counting toward a quit that will never come is a
/// lie. Kebab-cases to `quit-called-off`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, tauri_specta::Event)]
pub struct QuitCalledOff;

/// What the caller of [`QuitGate::request_quit`] should do about the quit it was
/// handed.
#[derive(Debug, Clone)]
pub(crate) enum QuitOutcome {
    /// Nothing to lose, or the decision is already made: let the exit through.
    Proceed,
    /// The gate is asking the user. Prevent the exit (and the window close);
    /// the gate ends the process itself once it has an answer or its deadline.
    Held {
        /// What's holding it, the same list the dialog is showing. A caller that
        /// isn't a window (the `quit` tool) has no other way to learn what it
        /// would be interrupting.
        operations: Vec<OperationSnapshot>,
        /// Milliseconds left before the gate quits on its own. On a second
        /// request this is what's LEFT of the countdown already running, not a
        /// fresh one.
        countdown_ms: u32,
    },
}

impl QuitOutcome {
    /// Whether the exit has to be prevented. The two app-level entry points act
    /// on nothing else.
    pub(crate) fn is_held(&self) -> bool {
        matches!(self, Self::Held { .. })
    }
}

/// What [`QuitGate::confirm`] / [`QuitGate::cancel`] did with the answer.
///
/// Either answer can arrive late or never, and the gate's own deadline may have
/// claimed the decision first. A caller that can't see the dialog (an agent over
/// MCP) has nothing else to tell it whether its answer landed, so the outcome is
/// returned rather than dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum QuitAnswer {
    /// The gate was holding a quit, and this answer decided it.
    Answered,
    /// Nothing was pending: no quit was held, or the decision was already made
    /// (the countdown ran out, or another surface answered first).
    NoQuitPending,
}

/// The two durations, injectable so a test doesn't sit through the real ones.
pub(crate) struct QuitTimings {
    pub countdown: Duration,
    pub drain: Duration,
    pub tag_write_wait: Duration,
}

impl Default for QuitTimings {
    fn default() -> Self {
        Self {
            countdown: COUNTDOWN,
            drain: DRAIN,
            tag_write_wait: TAG_WRITE_WAIT,
        }
    }
}

/// Everything the gate needs from the outside world. [`TauriQuitHost`] is the
/// real one; tests substitute a recorder, which is how the deadline can be
/// exercised without ending the test process.
pub(crate) trait QuitHost: Send + Sync + 'static {
    /// Every operation the manager currently knows about. The gate applies
    /// [`blocks_quit`] itself, so the policy stays in one testable place.
    fn operations(&self) -> Vec<OperationSnapshot>;
    /// What a quit would cut short in the Tags windows (fork). None by default: a host without
    /// them has nothing to lose there.
    fn tags(&self) -> TagsAtQuit {
        TagsAtQuit::default()
    }
    /// Tell the windows a quit is pending.
    fn announce(&self, event: QuitRequested);
    /// Bring the prompt's window (main) forward, so it never sits behind another window, a
    /// Tags window included (fork). Nothing by default: a recorder has no windows.
    fn raise_prompt(&self) {}
    /// Tell the windows the pending quit is off, so no prompt is left counting.
    fn announce_called_off(&self);
    /// Tier 1: cooperative cancel of every live operation, keep-partials.
    fn cancel_all(&self);
    /// Tier 2: stop waiting on whatever didn't answer.
    fn abort_all(&self);
    /// Fence the in-flight temp ledger so the next launch sweeps the leftovers.
    fn flush_temp_ledger(&self);
    /// End the process.
    fn exit(&self);
}

/// Whether this operation is worth interrupting a quit for.
///
/// Both matches are exhaustive on purpose: a new operation type or lifecycle
/// status has to come here and say which side it's on, rather than defaulting
/// into "silently killable at quit".
pub(crate) fn blocks_quit(snapshot: &OperationSnapshot) -> bool {
    let still_going = match snapshot.status {
        // A conflict prompt leaves the operation Running, so "waiting on an
        // answer" needs no arm of its own.
        LifecycleStatus::Queued | LifecycleStatus::Running | LifecycleStatus::Paused => true,
        LifecycleStatus::Done | LifecycleStatus::Cancelled | LifecycleStatus::Failed => false,
    };
    let moves_bytes = match snapshot.operation_type {
        WriteOperationType::Copy
        | WriteOperationType::Move
        | WriteOperationType::Delete
        | WriteOperationType::Trash
        | WriteOperationType::ArchiveEdit
        | WriteOperationType::Compress => true,
        // Instant metadata ops (`manager::run_instant`) finish faster than a
        // human could read a dialog about them.
        WriteOperationType::Rename | WriteOperationType::CreateFolder | WriteOperationType::CreateFile => false,
    };
    still_going && moves_bytes
}

/// Whether a quit has to ask: an operation [`blocks_quit`], or a Tags window holds unwritten
/// edits or is writing (fork).
pub(crate) fn holds_quit(blocking: &[OperationSnapshot], tags: TagsAtQuit) -> bool {
    !blocking.is_empty() || tags.holds_quit()
}

/// Whether a held quit counts down to quitting on its own (fork). Not when only unwritten tag
/// edits hold it: nothing would stop safely, the user's own work would just be gone, so the
/// prompt waits for an answer as a document app with unsaved changes does (a logout then
/// reports Cmdr interrupted it). File operations or a running tag write keep the countdown.
pub(crate) fn counts_down(blocking: &[OperationSnapshot], tags: TagsAtQuit) -> bool {
    !blocking.is_empty() || tags.writing
}

/// The decision, on its way from an IPC command to the deadline thread.
enum Decision {
    Quit,
    Cancel,
}

/// Where the gate is. Exactly one deadline thread exists per [`Phase::Waiting`].
enum Phase {
    /// No quit pending.
    Idle,
    /// The dialog is up and the deadline thread is counting.
    Waiting {
        /// How an answer reaches the deadline thread; dropping it stands the
        /// thread down.
        decisions: mpsc::Sender<Decision>,
        /// When the gate quits on its own, so a second request is told what's
        /// left of the clock rather than a fresh countdown. `None`: it waits for
        /// an answer ([`counts_down`]).
        deadline: Option<Instant>,
        /// Kept because [`QuitGate::cancel`] has to reach the windows and
        /// `request_quit` is the only place a host is handed in.
        host: Arc<dyn QuitHost>,
    },
    /// The decision is made and the teardown is running (or done). Every
    /// further quit request sails through: the teardown ends in
    /// `AppHandle::exit(0)`, which comes straight back as `ExitRequested`, and
    /// a second prompt there would trap the app forever.
    Quitting,
}

pub(crate) struct QuitGate {
    phase: Mutex<Phase>,
    timings: QuitTimings,
    /// Deadline threads started, ever. Nothing in production reads it; it names
    /// each thread so a log or a sample tells two apart.
    countdowns: AtomicU64,
}

impl QuitGate {
    pub(crate) fn with_timings(timings: QuitTimings) -> Self {
        Self {
            phase: Mutex::new(Phase::Idle),
            timings,
            countdowns: AtomicU64::new(0),
        }
    }

    /// Answers a quit request: [`QuitOutcome::Proceed`] to let it through,
    /// [`QuitOutcome::Held`] once the gate has taken the decision over.
    ///
    /// Cheap and synchronous — it runs on the event loop thread.
    pub(crate) fn request_quit(self: &Arc<Self>, host: Arc<impl QuitHost>) -> QuitOutcome {
        let host: Arc<dyn QuitHost> = host;
        let mut phase = self.phase.lock_ignore_poison();
        match &*phase {
            Phase::Quitting => return QuitOutcome::Proceed,
            // Asking again must not restart the clock the user is watching, so
            // this answers with what's left of the one already running.
            Phase::Waiting { deadline, .. } => {
                let deadline = *deadline;
                drop(phase);
                return QuitOutcome::Held {
                    operations: host.operations().into_iter().filter(blocks_quit).collect(),
                    countdown_ms: deadline.map_or(0, remaining_ms),
                };
            }
            Phase::Idle => {}
        }

        let operations: Vec<OperationSnapshot> = host.operations().into_iter().filter(blocks_quit).collect();
        let tags = host.tags();
        if !holds_quit(&operations, tags) {
            *phase = Phase::Quitting;
            return QuitOutcome::Proceed;
        }

        let counts = counts_down(&operations, tags);
        let countdown = self.timings.countdown;
        let drain = self.timings.drain;
        let tag_write_wait = self.timings.tag_write_wait;
        let countdown_ms = if counts {
            u32::try_from(countdown.as_millis()).unwrap_or(u32::MAX)
        } else {
            0
        };

        let (decisions, answers) = mpsc::channel();
        *phase = Phase::Waiting {
            decisions,
            deadline: counts.then(|| Instant::now() + countdown),
            host: Arc::clone(&host),
        };
        drop(phase);

        log::info!(
            target: "quit",
            "holding the quit: {} operation(s) still running, {} Tags window(s) unwritten, tag write running: {}, {}s on the clock (0: waits for an answer)",
            operations.len(),
            tags.unwritten,
            tags.writing,
            countdown_ms / 1_000
        );
        host.announce(QuitRequested {
            operations: operations.clone(),
            countdown_ms,
            tags,
            counts_down: counts,
        });
        host.raise_prompt();

        // A dedicated OS thread, not a tokio task: the deadline must not be
        // schedulable behind whatever the runtime is already busy with, and the
        // point of the whole design is that it fires when other things are stuck.
        let name = format!("cmdr-quit-deadline-{}", self.countdowns.fetch_add(1, Ordering::SeqCst));
        let gate = Arc::clone(self);
        let spawned = std::thread::Builder::new().name(name).spawn(move || {
            let answer = if counts {
                answers.recv_timeout(countdown)
            } else {
                answers.recv().map_err(|_| RecvTimeoutError::Disconnected)
            };
            match answer {
                Ok(Decision::Cancel) | Err(RecvTimeoutError::Disconnected) => {
                    log::info!(target: "quit", "the quit was called off; the countdown is gone");
                    return;
                }
                Ok(Decision::Quit) => log::info!(target: "quit", "quit confirmed; stopping everything"),
                Err(RecvTimeoutError::Timeout) => {
                    // The frontend never answered — a wedged webview, or a user
                    // who walked away. Claim the decision, unless a cancel beat
                    // us to the lock by a hair.
                    if !gate.claim_deadline() {
                        return;
                    }
                    log::info!(target: "quit", "the countdown ran out; stopping everything");
                }
            }
            tear_down_and_exit(&*host, drain, tag_write_wait);
        });
        if let Err(e) = spawned {
            // No timer means no way back out of a held quit, so don't hold it.
            crate::log_error!(target: "quit", "couldn't start the quit countdown ({e}); quitting straight away");
            *self.phase.lock_ignore_poison() = Phase::Quitting;
            return QuitOutcome::Proceed;
        }
        QuitOutcome::Held {
            operations,
            countdown_ms,
        }
    }

    /// Quit now, skipping the rest of the countdown. Idempotent, and
    /// [`QuitAnswer::NoQuitPending`] once the deadline already claimed the
    /// decision.
    pub(crate) fn confirm(&self) -> QuitAnswer {
        let mut phase = self.phase.lock_ignore_poison();
        match std::mem::replace(&mut *phase, Phase::Quitting) {
            Phase::Waiting { decisions, .. } => {
                let _ = decisions.send(Decision::Quit);
                QuitAnswer::Answered
            }
            other => {
                *phase = other;
                log::debug!(target: "quit", "quit_confirm with no quit pending; ignoring");
                QuitAnswer::NoQuitPending
            }
        }
    }

    /// Keep working. The countdown is **gone**, not deferred: a snooze would
    /// still kill the transfer seconds later, which is worse than not having
    /// asked.
    pub(crate) fn cancel(&self) -> QuitAnswer {
        let mut phase = self.phase.lock_ignore_poison();
        match std::mem::replace(&mut *phase, Phase::Idle) {
            Phase::Waiting { decisions, host, .. } => {
                let _ = decisions.send(Decision::Cancel);
                // Off the lock before touching the outside world, and only on
                // the arm that actually took a prompt down.
                drop(phase);
                host.announce_called_off();
                QuitAnswer::Answered
            }
            other => {
                *phase = other;
                log::debug!(target: "quit", "quit_cancel with no quit pending; ignoring");
                QuitAnswer::NoQuitPending
            }
        }
    }

    /// Whether a held quit is counting down to quitting on its own.
    #[cfg(test)]
    pub(crate) fn is_counting_down(&self) -> bool {
        matches!(
            &*self.phase.lock_ignore_poison(),
            Phase::Waiting { deadline: Some(_), .. }
        )
    }

    /// Moves `Waiting` → `Quitting` for the deadline thread. `false` means an
    /// answer landed first and the thread should stand down.
    fn claim_deadline(&self) -> bool {
        let mut phase = self.phase.lock_ignore_poison();
        match std::mem::replace(&mut *phase, Phase::Quitting) {
            Phase::Waiting { .. } => true,
            other => {
                *phase = other;
                false
            }
        }
    }
}

/// How much of the countdown is left, clamped at zero.
fn remaining_ms(deadline: Instant) -> u32 {
    let left = deadline.saturating_duration_since(Instant::now());
    u32::try_from(left.as_millis()).unwrap_or(u32::MAX)
}

/// Stop everything, leave the disk safe, and end the process — inside 2 s from
/// here, whatever the operations are doing.
///
/// The order is the contract:
///
/// 1. **Cooperative cancel, no rollback.** Every fully-copied file is kept; only
///    the file in flight loses its partial. This is `OperationIntent::Stopped`.
/// 2. **Wait, but only up to `drain`.** Operations that answer let us skip the
///    rest of it.
/// 3. **Hard abort** whatever didn't answer: stop *waiting*, and let the staging
///    layer own the leftovers rather than a backend that stopped talking.
/// 4. **Fence the in-flight temp ledger**, so the next launch's sweep finds
///    every partial an abandoned worker is still writing.
/// 5. **Wait for a tag write** still running, up to `tag_write_wait` (fork): it
///    has no cancel, and cutting it between files leaves a book half written.
/// 6. **Exit.**
fn tear_down_and_exit(host: &dyn QuitHost, drain: Duration, tag_write_wait: Duration) {
    let started = Instant::now();
    host.cancel_all();

    let deadline = started + drain;
    while Instant::now() < deadline {
        if !host.operations().iter().any(blocks_quit) {
            break;
        }
        std::thread::sleep(DRAIN_POLL);
    }

    host.abort_all();
    host.flush_temp_ledger();
    let tags_deadline = Instant::now() + tag_write_wait;
    while host.tags().writing && Instant::now() < tags_deadline {
        std::thread::sleep(DRAIN_POLL);
    }
    log::info!(
        target: "quit",
        "everything stopped in {:?}; exiting",
        started.elapsed()
    );
    host.exit();
}

/// The process-wide gate. One per app, so ⌘Q and a window close share one
/// countdown instead of racing two.
static GATE: LazyLock<Arc<QuitGate>> = LazyLock::new(|| Arc::new(QuitGate::with_timings(QuitTimings::default())));

pub(crate) fn gate() -> &'static Arc<QuitGate> {
    &GATE
}

/// The real outside world: the operation manager, the main window, and the app.
///
/// Generic over the runtime because the MCP executor is: its handlers take an
/// `AppHandle<R>`, and the `quit` tool has to reach the same gate ⌘Q does.
pub(crate) struct TauriQuitHost<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
}

impl<R: tauri::Runtime> TauriQuitHost<R> {
    pub(crate) fn new(app: tauri::AppHandle<R>) -> Arc<Self> {
        Arc::new(Self { app })
    }
}

impl<R: tauri::Runtime> QuitHost for TauriQuitHost<R> {
    fn operations(&self) -> Vec<OperationSnapshot> {
        crate::operations_host::operations()
            .map(Operations::list_operations)
            .unwrap_or_default()
    }

    fn tags(&self) -> TagsAtQuit {
        crate::tag_editor::at_quit::now()
    }

    fn announce(&self, event: QuitRequested) {
        use tauri_specta::Event as _;
        if let Err(e) = event.emit(&self.app) {
            // The dialog is the only thing lost; the deadline still fires, so the
            // app still quits. That's the whole point of owning the timer here.
            log::warn!(target: "quit", "couldn't tell the windows about the pending quit: {e}");
        }
    }

    // The same three calls as the reveal's and the Dock menu's `raise_main_window` (both
    // private there, and the reveal's macOS-only).
    fn raise_prompt(&self) {
        use tauri::Manager as _;
        let Some(window) = self.app.get_webview_window("main") else {
            return;
        };
        let _ = window.unminimize();
        let _ = window.show();
        if let Err(e) = window.set_focus() {
            log::warn!(target: "quit", "couldn't bring the main window forward for the quit prompt: {e}");
        }
    }

    fn announce_called_off(&self) {
        use tauri_specta::Event as _;
        if let Err(e) = QuitCalledOff.emit(&self.app) {
            log::warn!(target: "quit", "couldn't tell the windows the quit is off: {e}");
        }
    }

    // Before `install` nothing runs and nothing is recorded, so each of these
    // three has nothing to do.
    fn cancel_all(&self) {
        if let Ok(engine) = crate::operations_host::operations() {
            engine.cancel_all_write_operations();
        }
    }

    fn abort_all(&self) {
        if let Ok(engine) = crate::operations_host::operations() {
            engine.abort_all_write_operations();
        }
    }

    fn flush_temp_ledger(&self) {
        if let Ok(engine) = crate::operations_host::operations() {
            engine.flush_in_flight_temps();
        }
    }

    fn exit(&self) {
        // Comes back around as `RunEvent::ExitRequested`, where the gate is now
        // `Quitting` and waves it through to `RunEvent::Exit`.
        self.app.exit(0);
    }
}

/// Answers a quit request from an app-level entry point or the `quit` tool. See
/// [`QuitGate::request_quit`].
pub(crate) fn request_quit<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> QuitOutcome {
    gate().request_quit(TauriQuitHost::new(app.clone()))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "tags_tests.rs"]
mod tags_tests;
