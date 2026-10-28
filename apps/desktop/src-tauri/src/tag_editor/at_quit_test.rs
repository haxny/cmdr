//! What a quit would cut short in the Tags windows: edits not yet written, and a write running.

use cmdr_tags::TagField;

use super::at_quit::{TagsAtQuit, now, writing};
use super::session::{TagEdit, close, edit, open};
use super::session_test::album;
use crate::multi_rename::session::close as close_rename;

#[test]
fn nothing_open_holds_no_quit() {
    assert!(!TagsAtQuit::default().holds_quit());
    assert!(
        TagsAtQuit {
            unwritten: 1,
            writing: false
        }
        .holds_quit()
    );
    assert!(
        TagsAtQuit {
            unwritten: 0,
            writing: true
        }
        .holds_quit()
    );
}

#[test]
fn a_write_counts_while_it_runs_and_not_after() {
    assert!(!now().writing);
    let first = writing();
    let second = writing();
    assert!(now().writing);
    drop(first);
    assert!(now().writing, "one write is still running");
    drop(second);
    assert!(!now().writing);
}

#[test]
fn a_session_with_an_unwritten_edit_counts_until_it_closes() {
    let (_dir, _listing, id) = album("tag-at-quit-unwritten");
    open(&id, false).expect("opens");
    assert_eq!(now().unwritten, 0, "a session only read holds nothing");
    edit(
        &id,
        vec![TagEdit::SetAll {
            field: TagField::Album,
            values: vec!["Kniha".to_string()],
        }],
    )
    .expect("edits");
    assert_eq!(now().unwritten, 1);
    close(&id);
    assert_eq!(now().unwritten, 0);
    close_rename(&id);
}
