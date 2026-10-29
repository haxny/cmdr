//! The Disc column's data and the two Disc tools through a real session: the row carries the Disc
//! tag, a tool respects the scope and is one undo step.

use cmdr_media_meta::test_fixtures::mp3_frames;
use cmdr_tags::TagField;
use cmdr_tags::tools::TagTool;

use super::history::undo_edit;
use super::session::{close, open, tool};
use super::session_test::{folder, path_of, tag_file};
use super::view::TagEditorView;
use crate::multi_rename::session::{close as close_rename, open as open_rename};

fn discs(view: &TagEditorView) -> Vec<(&str, Option<&str>)> {
    view.tracks
        .iter()
        .map(|row| (row.file_name.as_str(), row.disc.as_deref()))
        .collect()
}

#[test]
fn the_row_shows_the_disc_and_number_only_is_one_scoped_undo_step() {
    let mp3s: Vec<(&str, Vec<u8>)> = ["a.mp3", "b.mp3", "c.mp3"]
        .into_iter()
        .map(|n| (n, mp3_frames()))
        .collect();
    let (dir, listing) = folder("tag-editor-disc", "root", &mp3s);
    tag_file(&dir, "a.mp3", &[(TagField::Disc, "CD1")]);
    tag_file(&dir, "b.mp3", &[(TagField::Disc, "CD2")]);
    tag_file(&dir, "c.mp3", &[(TagField::Disc, "CD2")]);
    let id = open_rename(listing.id(), true, None, 0).expect("opens").session_id;
    let opened = open(&id, false).expect("opens");
    assert_eq!(
        discs(&opened),
        vec![("a.mp3", Some("CD1")), ("b.mp3", Some("CD2")), ("c.mp3", Some("CD2"))]
    );

    let scope = vec![path_of(&dir, "b.mp3")];
    let view = tool(&id, TagTool::DiscNumberOnly, Some(scope)).expect("runs");
    assert_eq!(
        discs(&view),
        vec![("a.mp3", Some("CD1")), ("b.mp3", Some("2")), ("c.mp3", Some("CD2"))]
    );

    let all = tool(&id, TagTool::DiscNumberOnly, None).expect("runs");
    assert_eq!(
        discs(&all),
        vec![("a.mp3", Some("1")), ("b.mp3", Some("2")), ("c.mp3", Some("2"))]
    );
    // One run, one undo step: back to the scoped result, not to the file's own tags.
    let undone = undo_edit(&id).expect("undoes");
    assert_eq!(
        discs(&undone),
        vec![("a.mp3", Some("CD1")), ("b.mp3", Some("2")), ("c.mp3", Some("CD2"))]
    );
    close(&id);
    close_rename(&id);
}

#[test]
fn disc_from_folder_leaves_a_flat_book_alone_and_is_no_edit() {
    let mp3s: Vec<(&str, Vec<u8>)> = ["a.mp3", "b.mp3"].into_iter().map(|n| (n, mp3_frames())).collect();
    let (dir, listing) = folder("tag-editor-disc-flat", "root", &mp3s);
    tag_file(&dir, "a.mp3", &[(TagField::Disc, "CD1")]);
    let id = open_rename(listing.id(), true, None, 0).expect("opens").session_id;
    open(&id, false).expect("opens");
    let view = tool(&id, TagTool::DiscFromFolder, None).expect("runs");
    assert_eq!(discs(&view), vec![("a.mp3", Some("CD1")), ("b.mp3", None)]);
    assert!(view.tracks.iter().all(|row| !row.changed));
    close(&id);
    close_rename(&id);
}
