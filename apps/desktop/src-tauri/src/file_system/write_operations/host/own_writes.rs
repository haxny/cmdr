//! "Cmdr is about to write here": every path the engine creates, replaces,
//! renames, or removes is announced first, so the host can tell the engine's own
//! writes from somebody else's. Cmdr's answer feeds the downloads watcher's
//! ignore set, through `operations_host::own_writes`, so a copy into
//! `~/Downloads` never pops a "new download" toast.

use std::path::Path;

/// Where the engine announces its own writes.
///
/// ❗ **Announce BEFORE the syscall** (a rename: both ends), once per path per
/// file landing, ❌ never per chunk: the watcher's event can arrive before the
/// syscall returns, and an announce after it loses that race.
pub(crate) trait OwnWrites: Send + Sync {
    /// Cmdr is about to create, replace, rename, or remove `path`. Returns at
    /// once: it sits beside a syscall on every file landing.
    fn announce(&self, path: &Path);
}

/// The answer for a host with nobody listening: a bench, a tool, an engine
/// before the app wired its own.
pub(super) struct NoOwnWrites;

impl OwnWrites for NoOwnWrites {
    fn announce(&self, _path: &Path) {}
}
