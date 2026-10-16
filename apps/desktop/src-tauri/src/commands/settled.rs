//! The production `on_settled` for a mutation that answered `StillRunning`: the
//! engine hands its late end to a callback (`write_operations/mutation_reply.rs`),
//! and the IPC layer turns that into a broadcast event.

use serde::Serialize;
use tauri_specta::Event;

/// Broadcasts the settle event (`mutation-settled`, or a command's own, like
/// `clipboard-paste-settled`) to every window.
pub(crate) fn broadcast_settled<E>(app: tauri::AppHandle) -> impl FnOnce(E) + Send + 'static
where
    E: Event + Serialize + Clone + Send + 'static,
{
    move |settled| {
        if let Err(e) = settled.emit(&app) {
            log::warn!(target: "write_ops", "couldn't emit {}: {e}", E::NAME);
        }
    }
}
