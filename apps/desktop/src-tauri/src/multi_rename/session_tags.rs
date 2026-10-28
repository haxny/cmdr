//! The Tags window's side of a session (fork): holding one so eviction passes it by, and making
//! one whose media cache starts empty (no EXIF or tag read), from a pane's selection or as a copy
//! of an open sheet's.

use std::sync::Arc;

use super::{MediaSeed, MultiRenameError, MultiRenameOpened, SESSIONS, insert_new, open_seeded, with_session};
use crate::ignore_poison::IgnorePoison;

/// [`super::open`] for a Tags window: the same files, but its media cache starts empty, so no EXIF or
/// tag read is ever queued for it (the window reads tags itself, through `tag_editor`).
pub(crate) fn open_for_tags(
    listing_id: &str,
    include_hidden: bool,
    selected: Option<&[usize]>,
    expected_sequence: u64,
) -> Result<MultiRenameOpened, MultiRenameError> {
    open_seeded(
        listing_id,
        include_hidden,
        selected,
        expected_sequence,
        MediaSeed::Empty,
    )
}

/// Holds a session a Tags window owns: `evict_to` passes it by (idle or not) until [`release`].
/// `SessionClosed` when it's gone already.
pub(crate) fn hold(session_id: &str) -> Result<(), MultiRenameError> {
    with_session(session_id, |s| s.held = true)
}

/// Another held session over the same volume, folder, and files (in any order) as `session_id`:
/// the session of a Tags window already showing them. `None` when there's none or it's gone.
pub(crate) fn held_twin(session_id: &str) -> Option<String> {
    let sessions = SESSIONS.lock_ignore_poison();
    let this = sessions.get(session_id)?;
    let sorted = |names: &[String]| {
        let mut names = names.to_vec();
        names.sort_unstable();
        names
    };
    let files = sorted(&this.names);
    sessions
        .iter()
        .find(|(id, other)| {
            id.as_str() != session_id
                && other.held
                && other.volume_id == this.volume_id
                && other.dir == this.dir
                && sorted(&other.names) == files
        })
        .map(|(id, _)| id.clone())
}

/// Lets eviction take the session again. No-op when it's already gone.
pub(crate) fn release(session_id: &str) {
    if let Some(session) = SESSIONS.lock_ignore_poison().get_mut(session_id) {
        session.held = false;
    }
}

/// A new session over the same folder and the same files, in the session's current order: what
/// the Tags window takes from an open sheet, so closing the sheet never ends the window's
/// session. It reads no media (nothing previews a rename over it).
pub(crate) fn reopen(session_id: &str) -> Result<MultiRenameOpened, MultiRenameError> {
    let (listing_id, volume_id, dir, names) = with_session(session_id, |s| {
        (
            s.listing_id.clone(),
            s.volume_id.clone(),
            s.dir.clone(),
            Arc::clone(&s.names),
        )
    })?;
    Ok(insert_new(listing_id, volume_id, dir, names, &[]))
}

#[cfg(test)]
pub(crate) fn is_held(session_id: &str) -> bool {
    SESSIONS.lock_ignore_poison().get(session_id).is_some_and(|s| s.held)
}
