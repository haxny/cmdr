//! The Tags window's hold on its Multi-Rename session: what holding answers, and that the
//! window going (however it goes) releases and ends both sessions.

use super::session::{is_open as tag_session_open, open as open_tags};
use super::session_test::album;
use super::window::{TagsSessionInfo, end, focused_tags_window, hold, session_of_label};
use crate::multi_rename::session::{close as close_rename, is_held};

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
            folder_name: folder
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
