//! The registry of every live operation's [`WriteOperationState`], keyed by
//! operation id: the lookup every by-id control goes through, and the walks
//! that touch all of them at once (cancel and abort at quit, the newest tick
//! for a window that opens late).

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, LazyLock, RwLock};

use super::{OperationIntent, WriteOperationState, WriteProgressEvent, load_intent};
use crate::ignore_poison::RwLockIgnorePoison;

/// The live [`WriteOperationState`] of every registered operation, keyed by
/// operation id.
///
/// A struct with methods rather than a bare `static` map, because
/// [`cancel_all`](Self::cancel_all) is a WALK: it touches every entry at once.
/// A walk is only honestly testable against a registry the caller OWNS — driving
/// the process-global one from a test cancels whatever operations OTHER tests
/// have in flight at that moment. DETAILS § "Test isolation".
pub(in crate::file_system::write_operations) struct WriteOperationRegistry {
    entries: RwLock<HashMap<String, Arc<WriteOperationState>>>,
}

impl WriteOperationRegistry {
    pub(super) fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Registers `state` under `operation_id`, so cancel / pause / conflict
    /// resolution can reach it by id.
    pub(in crate::file_system::write_operations) fn insert(
        &self,
        operation_id: String,
        state: Arc<WriteOperationState>,
    ) {
        self.entries.write_ignore_poison().insert(operation_id, state);
    }

    /// The live state for `operation_id`, if it's still registered. Returns an
    /// `Arc` rather than a guard so callers don't hold the registry lock while
    /// they touch the operation.
    pub(in crate::file_system::write_operations) fn get(&self, operation_id: &str) -> Option<Arc<WriteOperationState>> {
        Some(Arc::clone(self.entries.read_ignore_poison().get(operation_id)?))
    }

    /// Whether `operation_id` still has a state entry.
    ///
    /// ❌ Test-only, and it must stay that way: this is presence in the state
    /// map, ❌ never "is it running". A paused operation keeps its entry and a
    /// queued one has none yet, so answering a lifecycle question with this got
    /// both backwards. `manager().lifecycle_status()` is that answer; the tests
    /// here use this to pin the difference.
    #[cfg(test)]
    pub(in crate::file_system::write_operations) fn contains(&self, operation_id: &str) -> bool {
        self.entries
            .read()
            .is_ok_and(|entries| entries.contains_key(operation_id))
    }

    /// Drops `operation_id`'s state, ending its liveness first.
    ///
    /// ❌ Every removal goes through here. Removing the entry alone would leave a
    /// wedged operation's staged temps hidden forever (a task the driver
    /// abandoned still holds an `Arc` to the state, so the map entry going away
    /// doesn't drop it): `WriteOperationState::end_liveness`.
    pub(in crate::file_system::write_operations) fn forget(&self, operation_id: &str) {
        let mut entries = self.entries.write_ignore_poison();
        if let Some(state) = entries.remove(operation_id) {
            state.end_liveness();
        }
    }

    /// The newest progress tick of every registered operation that has emitted
    /// one, its wait classified now: the stored tick was measured while the
    /// operation was still moving. The states are copied out first, so no
    /// operation's own locks are taken under the registry's.
    pub(in crate::file_system::write_operations) fn latest_progress(&self) -> Vec<WriteProgressEvent> {
        let states: Vec<(String, Arc<WriteOperationState>)> = self
            .entries
            .read_ignore_poison()
            .iter()
            .map(|(id, state)| (id.clone(), Arc::clone(state)))
            .collect();
        states
            .into_iter()
            .filter_map(|(id, state)| {
                let mut event = state.last_progress()?;
                event.activity = state.activity(&id, Some(&event));
                Some(event)
            })
            .collect()
    }

    /// Stops every registered operation, keeping partials.
    ///
    /// Transitions each to `Stopped` (never `RollingBack`: teardown must not
    /// silently delete files with no visual feedback), flips `backend_cancel` so
    /// in-flight backend I/O bails, drops any pending conflict sender to unblock
    /// its waiter, and wakes a paused op so it observes the cancel. Already-
    /// `Stopped` operations are left alone.
    pub(in crate::file_system::write_operations) fn cancel_all(&self) {
        let entries = self.entries.read_ignore_poison();
        for (id, state) in entries.iter() {
            let current = load_intent(&state.intent);
            if current != OperationIntent::Stopped {
                log::info!("cancel_all_write_operations: stopping op={id}");
                state.intent.store(OperationIntent::Stopped as u8, Ordering::Relaxed);
                state.backend_cancel.cancel();
                // Drop the conflict resolution sender to unblock any waiting receiver
                state.conflict_slot.abandon();
                state.sources_gone.abandon();
                // Wake a paused, parked op so teardown's cancel is observed.
                state.pause_gate.wake();
            }
        }
    }

    /// Stops every registered operation and stops WAITING for the ones that
    /// don't answer. See [`abort_all_write_operations`](super::abort_all_write_operations).
    pub(in crate::file_system::write_operations) fn abort_all(&self) {
        // Tier 1 first, and unconditionally: an abort is a cancel that ran out
        // of patience, so an operation must never observe tier 2 without the
        // cooperative signal that gives its backends the chance to wind down
        // cleanly in the moments before the process goes.
        self.cancel_all();
        let entries = self.entries.read_ignore_poison();
        for (id, state) in entries.iter() {
            if !state.backend_abort.is_cancelled() {
                log::info!("abort_all_write_operations: no longer waiting for op={id}");
                state.backend_abort.cancel();
            }
        }
    }
}

/// Global registry of in-progress write operation states.
pub(in crate::file_system::write_operations) static WRITE_OPERATION_STATE: LazyLock<WriteOperationRegistry> =
    LazyLock::new(WriteOperationRegistry::new);

/// The newest progress tick of every live operation that has emitted one. See
/// [`WriteOperationRegistry::latest_progress`].
pub fn latest_operation_progress() -> Vec<WriteProgressEvent> {
    WRITE_OPERATION_STATE.latest_progress()
}

/// Drops `operation_id`'s state, ending its liveness first. See
/// [`WriteOperationRegistry::forget`].
pub(in crate::file_system::write_operations) fn forget_operation(operation_id: &str) {
    WRITE_OPERATION_STATE.forget(operation_id);
}
