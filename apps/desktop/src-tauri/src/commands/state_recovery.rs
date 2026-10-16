//! The recovery notice's queue. A pass-through to `crate::state_recovery`.

use crate::state_recovery::StateRecovery;

/// Every state file Cmdr set aside since the last call, oldest first. The main window calls it at
/// start and on each `state-recovery-queued` nudge; the queue drains, so each shows once.
#[tauri::command]
#[specta::specta]
pub fn take_state_recoveries() -> Vec<StateRecovery> {
    crate::state_recovery::take_queued()
}
