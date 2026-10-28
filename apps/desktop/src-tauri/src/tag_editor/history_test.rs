//! The pending-edit history: ⌘Z and ⇧⌘Z over edits made in memory.

use std::sync::Arc;

use cmdr_tags::shared::CoverEdit;
use cmdr_tags::tools::TagTool;
use cmdr_tags::{Cover, TagField};

use super::history::{EDIT_HISTORY_LIMIT, redo_edit, undo_edit};
use super::session::{TagEdit, close, edit, mark_written, open, tool};
use super::session_test::{album, path_of, set_title, tags};
use super::sort::{TrackSort, TrackSortKey, sort_tracks};
use super::view::TagEditorView;
use crate::multi_rename::session::close as close_rename;

/// The view as the user sees the tags: the history flags and the build stamp are not part of that.
fn shown(view: &TagEditorView) -> TagEditorView {
    TagEditorView {
        can_undo_edit: false,
        can_redo_edit: false,
        view_seq: 0,
        ..view.clone()
    }
}

fn set_album(value: &str) -> TagEdit {
    TagEdit::SetAll {
        field: TagField::Album,
        values: vec![value.to_string()],
    }
}

fn title_of(view: &TagEditorView, dir: &std::path::Path, name: &str) -> Option<String> {
    view.tracks
        .iter()
        .find(|row| row.path == path_of(dir, name))
        .and_then(|row| row.title.clone())
}

#[test]
fn undo_restores_the_view_before_the_edit_and_redo_reapplies_it() {
    let (dir, _listing, id) = album("tag-editor-history-basic");
    let opened = open(&id, false).expect("opens");
    assert!(!opened.can_undo_edit && !opened.can_redo_edit);
    let edited = edit(&id, vec![set_title(&dir, "a.mp3", "Changed")]).expect("edits");
    assert!(edited.can_undo_edit && !edited.can_redo_edit);
    assert!(edited.dirty);

    let undone = undo_edit(&id).expect("undoes");
    assert_eq!(
        shown(&undone),
        shown(&opened),
        "values, dirty flags and multiples are as before"
    );
    assert!(!undone.can_undo_edit && undone.can_redo_edit);

    let redone = redo_edit(&id).expect("redoes");
    assert_eq!(shown(&redone), shown(&edited));
    assert!(redone.can_undo_edit && !redone.can_redo_edit);
    close(&id);
    close_rename(&id);
}

#[test]
fn three_edits_undo_in_reverse_order() {
    let (dir, _listing, id) = album("tag-editor-history-order");
    let v0 = open(&id, false).expect("opens");
    let v1 = edit(&id, vec![set_title(&dir, "a.mp3", "One")]).expect("1");
    let v2 = edit(&id, vec![set_album("Two")]).expect("2");
    let v3 = edit(&id, vec![set_title(&dir, "b.mp3", "Three")]).expect("3");
    assert_eq!(shown(&undo_edit(&id).unwrap()), shown(&v2));
    assert_eq!(shown(&undo_edit(&id).unwrap()), shown(&v1));
    assert_eq!(shown(&undo_edit(&id).unwrap()), shown(&v0));
    assert_eq!(shown(&redo_edit(&id).unwrap()), shown(&v1));
    assert_eq!(shown(&redo_edit(&id).unwrap()), shown(&v2));
    assert_eq!(shown(&redo_edit(&id).unwrap()), shown(&v3));
    close(&id);
    close_rename(&id);
}

#[test]
fn a_new_edit_after_an_undo_clears_redo() {
    let (dir, _listing, id) = album("tag-editor-history-redo-cleared");
    open(&id, false).expect("opens");
    edit(&id, vec![set_title(&dir, "a.mp3", "One")]).expect("1");
    assert!(undo_edit(&id).unwrap().can_redo_edit);
    let view = edit(&id, vec![set_title(&dir, "a.mp3", "Other")]).expect("2");
    assert!(!view.can_redo_edit);
    assert_eq!(
        shown(&redo_edit(&id).unwrap()),
        shown(&view),
        "redo with nothing to redo changes nothing"
    );
    close(&id);
    close_rename(&id);
}

#[test]
fn a_tool_run_and_a_cover_change_are_each_one_step() {
    let (_dir, _listing, id) = album("tag-editor-history-tool");
    let v0 = open(&id, false).expect("opens");
    let cover = Cover {
        mime: "image/png".to_string(),
        data: Arc::from(vec![1u8, 2, 3]),
    };
    let covered = edit(
        &id,
        vec![TagEdit::Cover {
            edit: CoverEdit::Set(cover),
        }],
    )
    .expect("cover");
    assert!(covered.can_undo_edit);
    let numbered = tool(&id, TagTool::Renumber { start: 1 }, None).expect("renumbers");
    assert!(numbered.tracks.iter().all(|row| row.track.is_some()));
    assert_eq!(
        shown(&undo_edit(&id).unwrap()),
        shown(&covered),
        "the whole run is one step"
    );
    let back = undo_edit(&id).expect("undoes the cover");
    assert_eq!(shown(&back), shown(&v0));
    assert!(!back.can_undo_edit);
    close(&id);
    close_rename(&id);
}

#[test]
fn an_edit_that_changes_nothing_is_no_step_and_a_sort_is_not_an_edit() {
    let (dir, _listing, id) = album("tag-editor-history-noop");
    open(&id, false).expect("opens");
    let same = edit(&id, vec![set_title(&dir, "a.mp3", "Alpha")]).expect("same value");
    assert!(!same.can_undo_edit);
    let sorted = sort_tracks(
        &id,
        TrackSort {
            key: TrackSortKey::Title,
            descending: true,
        },
    )
    .expect("sorts");
    assert!(!sorted.can_undo_edit);
    close(&id);
    close_rename(&id);
}

#[test]
fn undo_follows_the_file_after_a_sort() {
    let (dir, _listing, id) = album("tag-editor-history-sort");
    open(&id, false).expect("opens");
    edit(&id, vec![set_title(&dir, "a.mp3", "Zed")]).expect("edits");
    sort_tracks(
        &id,
        TrackSort {
            key: TrackSortKey::Title,
            descending: false,
        },
    )
    .expect("sorts");
    let view = undo_edit(&id).expect("undoes");
    assert_eq!(title_of(&view, &dir, "a.mp3").as_deref(), Some("Alpha"));
    assert!(!view.dirty);
    close(&id);
    close_rename(&id);
}

#[test]
fn a_write_clears_both_stacks() {
    let (dir, _listing, id) = album("tag-editor-history-write");
    open(&id, false).expect("opens");
    edit(&id, vec![set_title(&dir, "a.mp3", "One")]).expect("1");
    edit(&id, vec![set_title(&dir, "b.mp3", "Two")]).expect("2");
    undo_edit(&id).expect("undoes");
    mark_written(&id, vec![(dir.join("a.mp3"), tags(&[]))]);
    let view = undo_edit(&id).expect("nothing to undo");
    assert!(!view.can_undo_edit && !view.can_redo_edit);
    close(&id);
    close_rename(&id);
}

#[test]
fn the_history_keeps_the_newest_200_steps() {
    let (dir, _listing, id) = album("tag-editor-history-cap");
    open(&id, false).expect("opens");
    for n in 0..EDIT_HISTORY_LIMIT + 5 {
        edit(&id, vec![set_title(&dir, "a.mp3", &format!("T{n}"))]).expect("edits");
    }
    let mut undone = 1;
    while undo_edit(&id).expect("undoes").can_undo_edit {
        undone += 1;
    }
    assert_eq!(undone, EDIT_HISTORY_LIMIT, "the oldest five are dropped");
    let view = undo_edit(&id).expect("nothing more");
    assert_eq!(
        title_of(&view, &dir, "a.mp3").as_deref(),
        Some("T4"),
        "the oldest kept step restores the state before it"
    );
    close(&id);
    close_rename(&id);
}
