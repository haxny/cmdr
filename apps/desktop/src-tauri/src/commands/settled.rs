//! The production `on_settled` for a mutation that answered `StillRunning`: the
//! engine hands its late end to a callback (`crates/cmdr-operations/src/mutation_reply.rs`),
//! and the IPC layer turns that into a broadcast event.

use serde::Serialize;
use tauri_specta::Event;

/// Broadcasts the settle event (`mutation-settled`, or a command's own, like
/// `clipboard-paste-settled`) to every window. `wire` turns what the callback
/// receives into the event that goes out: an engine payload's wire wrapper
/// (`operations_host::wire`), or `std::convert::identity` for an app-owned
/// event.
pub(crate) fn broadcast_settled<T, E>(
    app: tauri::AppHandle,
    wire: impl FnOnce(T) -> E + Send + 'static,
) -> impl FnOnce(T) + Send + 'static
where
    E: Event + Serialize + Clone,
{
    move |settled| {
        if let Err(e) = wire(settled).emit(&app) {
            log::warn!(target: "write_ops", "couldn't emit {}: {e}", E::NAME);
        }
    }
}
