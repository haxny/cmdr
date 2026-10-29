//! The Tags window's hold on its Multi-Rename session: what holding answers, and that the
//! window going (however it goes) releases and ends both sessions.

use super::at_quit::writing;
use super::session::{TagEdit, edit, is_open as tag_session_open, open as open_tags};
use super::session_test::album;
use super::window::{
    TagsMenuSelection, TagsSessionInfo, end, focused_tags_window, hold, may_close_unasked, may_hand_back,
    menu_key_command_of, menu_selection_of, native_edit_id_of, orphaned_tags_windows, session_of_label,
};
use crate::menu::{DESELECT_ALL_ID, EDIT_COPY_ID, EDIT_CUT_ID, EDIT_PASTE_ID, SELECT_ALL_ID};
use crate::multi_rename::session::{close as close_rename, is_held, open as open_rename};
use crate::window_events::TagsClipboardAction;

#[test]
fn a_window_label_names_its_session_and_no_other_window_does() {
    assert_eq!(session_of_label("tags-multi-rename-7"), Some("multi-rename-7"));
    assert_eq!(session_of_label("tags-"), None);
    assert_eq!(session_of_label("viewer-123"), None);
    assert_eq!(session_of_label("settings"), None);
}

#[test]
fn select_all_goes_to_the_focused_tags_window_only() {
    let windows = [
        ("main", false),
        ("tags-multi-rename-1", false),
        ("tags-multi-rename-2", true),
    ];
    assert_eq!(focused_tags_window(windows), Some("tags-multi-rename-2"));
    assert_eq!(
        focused_tags_window([("main", true), ("tags-multi-rename-1", false)]),
        None
    );
    assert_eq!(focused_tags_window([("settings", true)]), None);
}

/// The menu branch (`menu_handlers.rs`) sends a Tags window exactly what was picked: Select all
/// selects, Deselect all clears, and no item the window doesn't know is taken from its own road.
#[test]
fn select_all_and_deselect_all_each_reach_a_tags_window_as_themselves() {
    assert_eq!(menu_selection_of(SELECT_ALL_ID), Some(TagsMenuSelection::SelectAll));
    assert_eq!(menu_selection_of(DESELECT_ALL_ID), Some(TagsMenuSelection::DeselectAll));
    assert_eq!(menu_selection_of("close_tab"), None);
}

/// Edit > Cut, Copy and Paste reach a focused Tags window as themselves, so ⌘X/⌘C/⌘V act on the
/// field under the cursor; the window hands each back as the same native action for a text box.
#[test]
fn cut_copy_and_paste_reach_a_focused_tags_window_as_themselves() {
    let expected = [
        (EDIT_CUT_ID, TagsClipboardAction::Cut),
        (EDIT_COPY_ID, TagsClipboardAction::Copy),
        (EDIT_PASTE_ID, TagsClipboardAction::Paste),
    ];
    let focused = focused_tags_window([("main", false), ("tags-multi-rename-4", true)]);
    for (id, action) in expected {
        assert_eq!(
            (menu_selection_of(id), focused),
            (Some(TagsMenuSelection::Clipboard(action)), Some("tags-multi-rename-4")),
            "{id}"
        );
        assert_eq!(native_edit_id_of(action), id);
    }
}

/// Main in front: no Tags window is focused, so `route_menu_event` has no window to send the
/// three to and main's own Cut / Copy / Paste run as before.
#[test]
fn no_tags_window_takes_cut_copy_or_paste_while_main_is_focused() {
    assert_eq!(
        focused_tags_window([("main", true), ("tags-multi-rename-4", false)]),
        None
    );
}

/// The native action handed back goes to whatever window is key when it runs: only a Tags window
/// that still has focus may send it, so a ⌘` away mid-round-trip never pastes into main.
#[test]
fn only_a_focused_tags_window_may_hand_an_edit_action_back() {
    assert!(may_hand_back("tags-multi-rename-4", true));
    assert!(!may_hand_back("tags-multi-rename-4", false));
    assert!(!may_hand_back("main", true));
    assert!(!may_hand_back("viewer-1", true));
}

#[test]
fn deselect_all_goes_to_the_focused_tags_window_only() {
    let focused = focused_tags_window([("main", false), ("tags-multi-rename-3", true)]);
    assert_eq!(
        (menu_selection_of(DESELECT_ALL_ID), focused),
        (Some(TagsMenuSelection::DeselectAll), Some("tags-multi-rename-3"))
    );
    // Main in front: the branch takes nothing, so main's own Deselect all runs.
    assert_eq!(
        focused_tags_window([("main", true), ("tags-multi-rename-3", false)]),
        None
    );
}

/// Each File / Select F-key item names the Tags command holding the same default key, so the
/// press runs what the bar says the key does (`TagsFunctionKeyBar`, `sources/tags-window.ts`).
#[test]
fn the_file_and_select_f_keys_name_the_tag_command_with_the_same_key() {
    use crate::menu::{
        COMPARE_DIRECTORIES_ID, EDIT_ID, FILE_COPY_ID, FILE_DELETE_ID, FILE_DELETE_PERMANENTLY_ID, FILE_MOVE_ID,
        FILE_NEW_FILE_ID, FILE_NEW_FOLDER_ID, FILE_VIEW_ID, RENAME_ID, SEARCH_FILES_ID,
    };
    let expected = [
        (RENAME_ID, "tags.edit"),                          // F2
        (COMPARE_DIRECTORIES_ID, "tags.editNumber"),       // ⇧F2
        (FILE_VIEW_ID, "tags.play"),                       // F3
        (EDIT_ID, "tags.replace"),                         // F4
        (FILE_NEW_FILE_ID, "tags.case"),                   // ⇧F4
        (FILE_COPY_ID, "tags.format"),                     // F5
        (FILE_MOVE_ID, "tags.fromName"),                   // F6
        (FILE_NEW_FOLDER_ID, "tags.number"),               // F7
        (FILE_DELETE_ID, "tags.clearField"),               // F8
        (FILE_DELETE_PERMANENTLY_ID, "tags.removeExtras"), // ⇧F8
        (SEARCH_FILES_ID, "tags.filter"),                  // ⌘F
    ];
    for (id, command) in expected {
        assert_eq!(menu_key_command_of(id), Some(command), "{id}");
    }
}

/// Items whose keys the Tags window has no command for stay on their own road (greyed there).
#[test]
fn other_menu_items_are_not_tag_keys() {
    use crate::menu::{
        CLOSE_TAB_ID, FILE_COMPRESS_ID, FILE_DUPLICATE_ID, OPEN_TERMINAL_HERE_ID, SELECT_ALL_ID, SWITCH_PANE_ID,
    };
    // Switch pane's Tab stays the page's: in a dialog or a field it moves focus, never the panes.
    for id in [
        SWITCH_PANE_ID,
        FILE_COMPRESS_ID,
        FILE_DUPLICATE_ID,
        OPEN_TERMINAL_HERE_ID,
        SELECT_ALL_ID,
        CLOSE_TAB_ID,
        "unknown",
    ] {
        assert_eq!(menu_key_command_of(id), None, "{id}");
    }
}

#[test]
fn hold_counts_the_tracks_and_names_the_folder() {
    let (dir, _listing, session) = album("tags-window-hold");

    let info = hold(&session).expect("the session is open");

    let folder = dir
        .file_name()
        .and_then(|n| n.to_str())
        .expect("a named folder")
        .to_string();
    // Three mp3s; the jpg and the txt are no tracks.
    assert_eq!(
        info,
        TagsSessionInfo {
            tracks: 3,
            folder_name: folder,
            already_open: None,
        }
    );
    assert!(is_held(&session));
    close_rename(&session);
}

#[test]
fn ending_the_window_releases_and_closes_both_sessions() {
    let (_dir, _listing, session) = album("tags-window-end");
    hold(&session).expect("the session is open");
    open_tags(&session, false).expect("the tag session opens");
    assert!(tag_session_open(&session));

    end(&format!("tags-{session}"));

    assert!(!is_held(&session));
    assert!(!tag_session_open(&session));
    assert!(matches!(
        hold(&session),
        Err(crate::multi_rename::error::MultiRenameError::SessionClosed)
    ));
    // Ending twice (Escape, then the window's Destroyed) is a no-op.
    end(&format!("tags-{session}"));
}

/// Edit tags twice on the same files: the second open finds the window already showing them, so
/// the main window raises it instead of opening another over the same files.
#[test]
fn a_second_hold_over_the_same_files_names_the_window_already_open() {
    let (_dir, listing, first) = album("tags-window-twin");
    assert_eq!(hold(&first).expect("holds").already_open, None);
    let second = open_rename(listing.id(), true, None, 0)
        .expect("a second session opens")
        .session_id;
    assert_eq!(hold(&second).expect("holds").already_open, Some(first.clone()));

    // Other files of the same folder are another window's.
    let third = open_rename(listing.id(), true, Some(&[0]), 0)
        .expect("a third session opens")
        .session_id;
    assert_eq!(hold(&third).expect("holds").already_open, None);
    for id in [&first, &second, &third] {
        end(&format!("tags-{id}"));
    }
}

/// A Tags window whose session is no longer held has been closed (its `end` released it) but is
/// somehow still around, hidden: opening the next one takes it down, so it can't sit there for the
/// life of the process. A held session's window, and every other window, is left alone.
#[test]
fn only_a_tags_window_whose_session_is_no_longer_held_is_an_orphan() {
    let held = |id: &str| id == "multi-rename-1";
    let labels = [
        "main",
        "tags-multi-rename-1",
        "tags-multi-rename-2",
        "viewer-3",
        "settings",
        "tags-",
    ];
    assert_eq!(orphaned_tags_windows(labels, held), vec!["tags-multi-rename-2"]);
    assert!(orphaned_tags_windows(["main", "tags-multi-rename-1"], held).is_empty());
}

/// Wired to the real hold: a released (ended) session's window is an orphan, a held one isn't.
#[test]
fn a_window_whose_session_ended_is_an_orphan_and_a_held_one_is_not() {
    let (_kept_dir, _kept_listing, kept) = album("tags-window-orphan-kept");
    let (_ended_dir, _ended_listing, ended) = album("tags-window-orphan-ended");
    hold(&kept).expect("hold");
    hold(&ended).expect("hold");
    end(&format!("tags-{ended}"));
    let labels = [format!("tags-{kept}"), format!("tags-{ended}")];
    let orphans = orphaned_tags_windows(labels.iter().map(String::as_str), is_held);
    assert_eq!(orphans, vec![labels[1].as_str()]);
    end(&labels[0]);
}

/// The window's error boundary may close it without the discard question only while nothing
/// would be lost: no unwritten edit in its session (the edits live here, not in the page).
#[test]
fn a_tags_window_with_unwritten_edits_may_not_close_unasked() {
    let (_dir, _listing, id) = album("tags-window-may-close-dirty");
    hold(&id).expect("holds");
    open_tags(&id, false).expect("the tag session opens");
    assert!(may_close_unasked(&id), "a session only read loses nothing");
    edit(
        &id,
        vec![TagEdit::SetAll {
            field: cmdr_tags::TagField::Album,
            values: vec!["Kniha".to_string()],
        }],
    )
    .expect("edits");
    assert!(!may_close_unasked(&id));
    assert!(is_held(&id), "asking changes nothing");
    end(&format!("tags-{id}"));
}

/// Nor while a tag write or undo runs: closing then would cut a book's write short.
#[test]
fn a_tags_window_may_not_close_unasked_while_a_write_runs() {
    let (_dir, _listing, id) = album("tags-window-may-close-writing");
    hold(&id).expect("holds");
    open_tags(&id, false).expect("the tag session opens");
    let running = writing();
    assert!(!may_close_unasked(&id));
    drop(running);
    assert!(may_close_unasked(&id));
    end(&format!("tags-{id}"));
}
