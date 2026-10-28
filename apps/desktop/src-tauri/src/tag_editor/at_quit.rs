//! What quitting would cut short in the Tags windows, for the quit gate (`quit/`): tag sessions
//! holding edits not yet written, and a tag write or undo running. A quit asks about the first
//! and waits for the second, so a book is never left half written.

use std::sync::atomic::{AtomicUsize, Ordering};

use serde::{Deserialize, Serialize};

/// The Tags windows' share of a quit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TagsAtQuit {
    /// Tag sessions (one per Tags window) holding edits not yet written.
    pub unwritten: u32,
    /// A tag write or undo is running.
    pub writing: bool,
}

impl TagsAtQuit {
    /// Whether a quit has to ask (or wait) for the Tags windows.
    pub(crate) fn holds_quit(&self) -> bool {
        self.unwritten > 0 || self.writing
    }
}

/// Tag writes and undos running now.
static WRITING: AtomicUsize = AtomicUsize::new(0);

/// A tag write or undo running, counted until dropped.
#[must_use = "the write counts only while this lives"]
pub(crate) struct Writing(());

impl Drop for Writing {
    fn drop(&mut self) {
        WRITING.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Counts a tag write or undo from now until the answer is dropped. Take it inside the blocking
/// work, so it spans the files being written even when the IPC call that started it is gone.
pub(crate) fn writing() -> Writing {
    WRITING.fetch_add(1, Ordering::SeqCst);
    Writing(())
}

/// The Tags windows' share of a quit, now. Cheap: one lock over the tag sessions, no file work.
pub(crate) fn now() -> TagsAtQuit {
    TagsAtQuit {
        unwritten: u32::try_from(super::session::unwritten_sessions()).unwrap_or(u32::MAX),
        writing: WRITING.load(Ordering::SeqCst) > 0,
    }
}
