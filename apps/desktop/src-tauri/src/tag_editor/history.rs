//! The pending-edit history of a tag session: what ⌘Z and ⇧⌘Z step through before anything is written.
//!
//! One step is one edit call (an `edit_tags` batch, one tool run, one text-tool run, one cover
//! change). A step keeps only the files that call changed, each as its edited tags and cover key
//! BEFORE the call, so undoing it puts back exactly what the view showed. Sorting is no edit and
//! is not here. A write (or anything that re-reads files behind the edits) clears both stacks.

use std::collections::VecDeque;
use std::path::PathBuf;

use cmdr_tags::FileTags;

use super::session::{Entry, TagEditorError, TagSession, require_editable, with_session};
use super::view::TagEditorView;

/// How many steps a session keeps; the oldest is dropped past it.
pub(crate) const EDIT_HISTORY_LIMIT: usize = 200;

/// A file's edited tags and the key of its cover.
type Held = (PathBuf, FileTags, Option<u64>);

/// What an edit call changed: the changed files as they were before it.
pub(super) type Snapshot = Vec<Held>;

#[derive(Default)]
pub(super) struct EditHistory {
    undo: VecDeque<Snapshot>,
    redo: Vec<Snapshot>,
}

impl EditHistory {
    pub(super) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub(super) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

/// Every readable file's edited state, to compare with after an edit.
pub(super) fn capture(session: &TagSession) -> Snapshot {
    session
        .by_path
        .iter()
        .filter_map(|(path, entry)| {
            let loaded = entry.loaded()?;
            Some((path.clone(), loaded.edited.clone(), loaded.cover_key))
        })
        .collect()
}

/// Records the step an edit made: the files in `before` whose edited tags now differ. An edit that
/// changed nothing leaves no step. A step clears the redo stack.
pub(super) fn record(session: &mut TagSession, before: Snapshot) {
    let changed: Snapshot = before
        .into_iter()
        .filter(|(path, tags, _)| {
            session
                .by_path
                .get(path)
                .and_then(Entry::loaded)
                .is_some_and(|loaded| &loaded.edited != tags)
        })
        .collect();
    if changed.is_empty() {
        return;
    }
    let history = &mut session.history;
    history.redo.clear();
    history.undo.push_back(changed);
    while history.undo.len() > EDIT_HISTORY_LIMIT {
        history.undo.pop_front();
    }
}

/// Puts `snapshot` back and returns what it replaced (the opposite step).
fn swap_in(session: &mut TagSession, snapshot: Snapshot) -> Snapshot {
    snapshot
        .into_iter()
        .filter_map(|(path, tags, key)| {
            let Some(Entry::Loaded(loaded)) = session.by_path.get_mut(&path) else {
                return None;
            };
            let current = (path, std::mem::replace(&mut loaded.edited, tags), loaded.cover_key);
            loaded.cover_key = key;
            Some(current)
        })
        .collect()
}

/// Forgets every step: the files changed under them.
pub(super) fn clear(session: &mut TagSession) {
    session.history = EditHistory::default();
}

/// Reverses the last edit step. Nothing to undo changes nothing.
pub fn undo_edit(session_id: &str) -> Result<TagEditorView, TagEditorError> {
    step(session_id, |session| {
        let snapshot = session.history.undo.pop_back()?;
        let opposite = swap_in(session, snapshot);
        if !opposite.is_empty() {
            session.history.redo.push(opposite);
        }
        Some(())
    })
}

/// Re-applies the last undone edit step. Nothing to redo changes nothing.
pub fn redo_edit(session_id: &str) -> Result<TagEditorView, TagEditorError> {
    step(session_id, |session| {
        let snapshot = session.history.redo.pop()?;
        let opposite = swap_in(session, snapshot);
        if !opposite.is_empty() {
            session.history.undo.push_back(opposite);
        }
        Some(())
    })
}

fn step(session_id: &str, f: impl FnOnce(&mut TagSession) -> Option<()>) -> Result<TagEditorView, TagEditorError> {
    with_session(session_id, |layout, session| {
        require_editable(layout)?;
        f(session);
        Ok(super::session::view_of(layout, session))
    })
}
