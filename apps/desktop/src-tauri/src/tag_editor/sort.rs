//! The order of the Tags window's tracks: which key and direction, and the comparison itself.
//!
//! What this guarantees:
//! - Sorting is a pure ordering of the session's file NAMES from what is already in memory; it never
//!   reads or writes a file and never touches an edit (edits are keyed by path).
//! - A track with no value for the key (no track number, no title, no length) and an unreadable file
//!   sort after the others in either direction (a file-name sort needs no tags, so it keeps them in place). Equal keys fall back to the file name, always
//!   ascending, so the result is deterministic.
//! - Names compare with the app's name collation, on the whole name the session holds (which may be a
//!   relative path such as `Disc 1/01.mp3`).

use std::cmp::Ordering;

use cmdr_collation::NameCollator;
use cmdr_tags::{TagField, format_of};
use serde::{Deserialize, Serialize};

use super::error::TagEditorError;
use super::session::{
    Entry, LOG_TARGET, Layout, Loaded, SESSIONS, TagSession, close, layout_of, prune, view_of, with_session,
};
use super::view::TagEditorView;
use crate::ignore_poison::IgnorePoison;
use crate::multi_rename::error::MultiRenameError;
use crate::multi_rename::session::{SessionFiles, files_of, set_order};

/// What the tracks are ordered by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum TrackSortKey {
    /// Disc number, then track number (`3` or `3/12`); a missing disc counts as disc 1.
    DiscTrack,
    Title,
    Length,
    FileName,
}

/// The tracks' order: a key and a direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TrackSort {
    pub key: TrackSortKey,
    pub descending: bool,
}

impl TrackSort {
    /// The order a tag session opens in: disc and track when any readable track has a track number,
    /// else by file name.
    pub(super) fn default_for(session: &TagSession) -> Self {
        let numbered = session
            .by_path
            .values()
            .filter_map(Entry::loaded)
            .any(|loaded| track_of(loaded).is_some());
        Self {
            key: if numbered {
                TrackSortKey::DiscTrack
            } else {
                TrackSortKey::FileName
            },
            descending: false,
        }
    }
}

impl TagSession {
    /// Forgets `sort` when the tracks no longer stand in the order it made.
    pub(super) fn drop_stale_sort(&mut self, layout: &Layout) {
        if self.sort.is_some() && self.sorted_tracks != layout.tracks {
            self.sort = None;
        }
    }
}

/// One session file to place: its name as the session holds it, and its tags when it was read.
pub(super) struct SortItem<'a> {
    pub name: &'a str,
    pub loaded: Option<&'a Loaded>,
}

/// The `N` of a `N` or `N/M` value.
fn number_of(value: &str) -> Option<u32> {
    value.split('/').next()?.trim().parse().ok()
}

fn first_number(loaded: &Loaded, field: TagField) -> Option<u32> {
    loaded
        .edited
        .fields
        .get(&field)?
        .first()
        .and_then(|value| number_of(value))
}

fn track_of(loaded: &Loaded) -> Option<u32> {
    first_number(loaded, TagField::Track)
}

/// The `(disc, track)` of a track that has a track number.
fn disc_track_of(loaded: &Loaded) -> Option<(u32, u32)> {
    let track = track_of(loaded)?;
    Some((first_number(loaded, TagField::Disc).unwrap_or(1), track))
}

fn title_of(loaded: &Loaded) -> Option<&str> {
    loaded
        .edited
        .fields
        .get(&TagField::Title)?
        .first()
        .map(String::as_str)
        .filter(|title| !title.is_empty())
}

/// `a` against `b` where a missing value is last in either direction.
fn with_missing_last<T>(
    a: Option<T>,
    b: Option<T>,
    descending: bool,
    compare: impl FnOnce(T, T) -> Ordering,
) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => {
            let ordering = compare(a, b);
            if descending { ordering.reverse() } else { ordering }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn compare_items(a: &SortItem, b: &SortItem, sort: TrackSort, collator: &NameCollator) -> Ordering {
    let by_name = || collator.compare(a.name, b.name);
    if sort.key == TrackSortKey::FileName {
        // The name needs no tags, so an unreadable file keeps its place among the others.
        let ordering = by_name();
        return if sort.descending { ordering.reverse() } else { ordering };
    }
    let (Some(first), Some(second)) = (a.loaded, b.loaded) else {
        // An unreadable file goes after the readable ones, by name among themselves.
        return a.loaded.is_none().cmp(&b.loaded.is_none()).then_with(by_name);
    };
    let by_key = match sort.key {
        TrackSortKey::FileName => Ordering::Equal, // Decided above, by name alone.
        TrackSortKey::DiscTrack => {
            with_missing_last(disc_track_of(first), disc_track_of(second), sort.descending, |a, b| {
                a.cmp(&b)
            })
        }
        TrackSortKey::Title => with_missing_last(title_of(first), title_of(second), sort.descending, |a, b| {
            collator.compare(a, b)
        }),
        TrackSortKey::Length => {
            with_missing_last(first.duration_ms, second.duration_ms, sort.descending, |a, b| a.cmp(&b))
        }
    };
    by_key.then_with(by_name)
}

/// The items' names in `sort` order.
pub(super) fn sorted_names(items: &[SortItem], sort: TrackSort, collator: &NameCollator) -> Vec<String> {
    let mut ordered: Vec<&SortItem> = items.iter().collect();
    ordered.sort_by(|a, b| compare_items(a, b, sort, collator));
    ordered.into_iter().map(|item| item.name.to_string()).collect()
}

/// Puts the tracks in `sort` order and answers the new view. Only the Multi-Rename session's order
/// changes: no file is read or written and no edit is touched. Files that aren't tracks keep their
/// places among the session's names.
pub(crate) fn sort_tracks(session_id: &str, sort: TrackSort) -> Result<TagEditorView, TagEditorError> {
    reorder(session_id, sort)
}

pub(super) fn reorder(session_id: &str, sort: TrackSort) -> Result<TagEditorView, TagEditorError> {
    let files = files_of(session_id).map_err(|_| {
        close(session_id);
        TagEditorError::NoSession
    })?;
    let layout = layout_of(&files);
    let new_names = {
        let mut sessions = SESSIONS.lock_ignore_poison();
        let session = sessions.get_mut(session_id).ok_or(TagEditorError::NotOpen)?;
        prune(session, &layout);
        ordered_names(&files, &layout, session, sort)
    };
    set_order(session_id, new_names).map_err(|error| match error {
        MultiRenameError::SessionClosed => {
            close(session_id);
            TagEditorError::NoSession
        }
        _ => TagEditorError::TaskFailed,
    })?;
    log::debug!(target: LOG_TARGET, "sorted {} tracks", layout.tracks.len());
    with_session(session_id, |layout, session| {
        // Only now that the order is set: a failed sort leaves the session to be sorted again.
        session.sort = Some(sort);
        session.sorted_tracks = layout.tracks.clone();
        session.ordered = true;
        Ok(view_of(layout, session))
    })
}

/// The session's names with its audio files in `sort` order, in the places the audio files held.
fn ordered_names(files: &SessionFiles, layout: &Layout, session: &TagSession, sort: TrackSort) -> Vec<String> {
    if layout.tracks.is_empty() {
        return files.names.to_vec();
    }
    let items: Vec<SortItem> = files
        .names
        .iter()
        .filter(|name| format_of(name).is_some())
        .map(|name| SortItem {
            name,
            loaded: session.by_path.get(&files.dir.join(name)).and_then(Entry::loaded),
        })
        .collect();
    let mut sorted = sorted_names(&items, sort, &crate::listing_host::listings().collator()).into_iter();
    files
        .names
        .iter()
        .map(|name| {
            if format_of(name).is_some() {
                sorted.next().unwrap_or_else(|| name.clone())
            } else {
                name.clone()
            }
        })
        .collect()
}
