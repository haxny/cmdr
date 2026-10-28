//! Audiobookshelf side files (`reader.txt`, `desc.txt`) written with the tags, against real audio
//! files, a real Multi-Rename session, and a real undo store.

use std::path::Path;

use cmdr_media_meta::test_fixtures::mp3_frames;
use cmdr_tags::{FileTags, TagField};

use super::apply::{ApplyRequest, apply_with};
use super::apply_test::{done, request, store};
use super::session::{TagEdit, TagEditorError, edit, open};
use super::session_test::{album, folder, path_of, tags};
use super::side_files::{
    SIDE_FILE_LIMIT, SideFile, SideFileChange, SideFileSeen, SideFileWithheld, side_file_changes, wanted,
};
use super::undo::undo_with;
use crate::multi_rename::session::open as open_rename;

fn set_all(field: TagField, values: &[&str]) -> TagEdit {
    TagEdit::SetAll {
        field,
        values: values.iter().map(|value| (*value).to_string()).collect(),
    }
}

fn with_side_files() -> ApplyRequest {
    ApplyRequest {
        side_files: true,
        ..request()
    }
}

fn text_of(dir: &Path, name: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(name)).ok()
}

#[test]
fn setting_off_writes_no_side_files() {
    let (dir, _listing, id) = album("side-off");
    let (_store_dir, store) = store("side-off-store");
    open(&id, false).expect("opens");
    edit(&id, vec![set_all(TagField::Performer, &["Vladimir Kudla"])]).expect("edits");

    let report = apply_with(&store, &id, request()).expect("applies");

    assert_eq!(report.written.len(), 3);
    assert!(report.side_files_written.is_empty());
    assert_eq!(text_of(&dir, "reader.txt"), None);
    done(&id);
}

#[test]
fn setting_on_writes_the_narrator_with_the_tracks_and_undo_removes_it() {
    let (dir, _listing, id) = album("side-reader");
    let (_store_dir, store) = store("side-reader-store");
    open(&id, false).expect("opens");
    edit(&id, vec![set_all(TagField::Performer, &["Vladimir Kudla"])]).expect("edits");
    assert_eq!(
        side_file_changes(&id).expect("lists"),
        vec![SideFileChange::Create {
            file: SideFile::Reader,
            path: path_of(&dir, "reader.txt"),
            text: "Vladimir Kudla".to_string(),
        }]
    );

    let report = apply_with(&store, &id, with_side_files()).expect("applies");

    assert_eq!(report.written.len(), 3);
    assert_eq!(report.side_files_written, vec![path_of(&dir, "reader.txt")]);
    assert_eq!(text_of(&dir, "reader.txt").as_deref(), Some("Vladimir Kudla\n"));
    assert_eq!(text_of(&dir, "desc.txt"), None, "no description, no desc.txt");
    assert!(
        side_file_changes(&id).expect("lists").is_empty(),
        "it says the same now"
    );

    let undone = undo_with(&store, &report.apply_id.expect("undoable"), Some(&id)).expect("undoes");
    assert_eq!(undone.restored.len(), 4, "three tracks and reader.txt");
    assert_eq!(text_of(&dir, "reader.txt"), None, "the created file is gone");
    done(&id);
}

#[test]
fn two_narrators_are_joined_with_a_comma() {
    let (dir, _listing, id) = album("side-two");
    let (_store_dir, store) = store("side-two-store");
    open(&id, false).expect("opens");
    edit(
        &id,
        vec![set_all(TagField::Performer, &["Jana Bauerová", "Ivan Trojan"])],
    )
    .expect("edits");

    apply_with(&store, &id, with_side_files()).expect("applies");

    assert_eq!(
        text_of(&dir, "reader.txt").as_deref(),
        Some("Jana Bauerová, Ivan Trojan\n")
    );
    done(&id);
}

#[test]
fn tracks_that_disagree_give_no_reader_and_the_list_says_why() {
    let (dir, _listing, id) = album("side-disagree");
    let (_store_dir, store) = store("side-disagree-store");
    open(&id, false).expect("opens");
    edit(
        &id,
        vec![
            set_all(TagField::Performer, &["A"]),
            TagEdit::SetOne {
                path: path_of(&dir, "b.mp3"),
                field: TagField::Performer,
                values: vec!["B".to_string()],
            },
        ],
    )
    .expect("edits");

    assert_eq!(
        side_file_changes(&id).expect("lists"),
        vec![SideFileChange::Withheld {
            file: SideFile::Reader,
            reason: SideFileWithheld::TracksDisagree,
        }]
    );
    let report = apply_with(&store, &id, with_side_files()).expect("applies");
    assert_eq!(report.written.len(), 3);
    assert!(report.side_files_written.is_empty());
    assert_eq!(text_of(&dir, "reader.txt"), None);
    done(&id);
}

#[test]
fn an_existing_file_with_the_same_first_line_is_untouched() {
    let (dir, _listing, id) = album("side-same");
    let (_store_dir, store) = store("side-same-store");
    std::fs::write(dir.join("reader.txt"), "Vladimir Kudla\nread in 2019\n").expect("writes");
    let before = std::fs::metadata(dir.join("reader.txt"))
        .expect("stats")
        .modified()
        .expect("mtime");
    open(&id, false).expect("opens");
    edit(&id, vec![set_all(TagField::Performer, &["Vladimir Kudla"])]).expect("edits");

    assert!(side_file_changes(&id).expect("lists").is_empty());
    let report = apply_with(&store, &id, with_side_files()).expect("applies");

    assert!(report.side_files_written.is_empty() && report.side_files_held.is_empty());
    assert_eq!(
        text_of(&dir, "reader.txt").as_deref(),
        Some("Vladimir Kudla\nread in 2019\n")
    );
    let after = std::fs::metadata(dir.join("reader.txt"))
        .expect("stats")
        .modified()
        .expect("mtime");
    assert_eq!(before, after);
    done(&id);
}

#[test]
fn a_different_file_is_replaced_only_as_listed_and_undo_puts_it_back() {
    let (dir, _listing, id) = album("side-replace");
    let (_store_dir, store) = store("side-replace-store");
    std::fs::write(dir.join("reader.txt"), "Someone Else\n").expect("writes");
    open(&id, false).expect("opens");
    edit(&id, vec![set_all(TagField::Performer, &["Vladimir Kudla"])]).expect("edits");
    let reader = path_of(&dir, "reader.txt");
    assert_eq!(
        side_file_changes(&id).expect("lists"),
        vec![SideFileChange::Replace {
            file: SideFile::Reader,
            path: reader.clone(),
            old: "Someone Else\n".to_string(),
            text: "Vladimir Kudla".to_string(),
        }]
    );

    // Not shown (⌘⏎ straight away): held back, the tracks written.
    let report = apply_with(&store, &id, with_side_files()).expect("applies");
    assert_eq!(report.written.len(), 3);
    assert_eq!(report.side_files_held, vec![reader.clone()]);
    assert_eq!(text_of(&dir, "reader.txt").as_deref(), Some("Someone Else\n"));

    // Shown with other old text (the file changed since the list): still held.
    let stale = ApplyRequest {
        allow_side_files: vec![SideFileSeen {
            path: reader.clone(),
            old: "Another\n".to_string(),
        }],
        ..with_side_files()
    };
    assert_eq!(
        apply_with(&store, &id, stale).expect("applies").side_files_held,
        vec![reader.clone()]
    );

    // Shown as it is: replaced, alone (the tracks were written already), and undoable.
    let seen = ApplyRequest {
        allow_side_files: vec![SideFileSeen {
            path: reader.clone(),
            old: "Someone Else\n".to_string(),
        }],
        ..with_side_files()
    };
    let report = apply_with(&store, &id, seen).expect("applies");
    assert!(report.written.is_empty());
    assert_eq!(report.side_files_written, vec![reader.clone()]);
    assert_eq!(text_of(&dir, "reader.txt").as_deref(), Some("Vladimir Kudla\n"));

    let undone = undo_with(&store, &report.apply_id.expect("undoable"), Some(&id)).expect("undoes");
    assert_eq!(undone.restored, vec![dir.join("reader.txt")]);
    assert_eq!(text_of(&dir, "reader.txt").as_deref(), Some("Someone Else\n"));
    done(&id);
}

#[test]
fn undo_leaves_a_side_file_changed_since_the_write_alone() {
    let (dir, _listing, id) = album("side-changed");
    let (_store_dir, store) = store("side-changed-store");
    open(&id, false).expect("opens");
    edit(&id, vec![set_all(TagField::Performer, &["Vladimir Kudla"])]).expect("edits");
    let report = apply_with(&store, &id, with_side_files()).expect("applies");
    std::fs::write(dir.join("reader.txt"), "Hand edited\n").expect("writes");

    let undone = undo_with(&store, &report.apply_id.expect("undoable"), Some(&id)).expect("undoes");

    assert_eq!(undone.changed, vec![dir.join("reader.txt")]);
    assert_eq!(text_of(&dir, "reader.txt").as_deref(), Some("Hand edited\n"));
    done(&id);
}

#[test]
fn desc_txt_holds_the_shared_description() {
    let (dir, _listing, id) = album("side-desc");
    let (_store_dir, store) = store("side-desc-store");
    std::fs::write(dir.join("desc.txt"), "Old blurb\n").expect("writes");
    open(&id, false).expect("opens");
    edit(&id, vec![set_all(TagField::Description, &["A book about a dog."])]).expect("edits");
    let desc = path_of(&dir, "desc.txt");
    let listed = side_file_changes(&id).expect("lists");
    assert_eq!(
        listed,
        vec![SideFileChange::Replace {
            file: SideFile::Desc,
            path: desc.clone(),
            old: "Old blurb\n".to_string(),
            text: "A book about a dog.".to_string(),
        }]
    );

    let report = apply_with(
        &store,
        &id,
        ApplyRequest {
            allow_side_files: vec![SideFileSeen {
                path: desc.clone(),
                old: "Old blurb\n".to_string(),
            }],
            ..with_side_files()
        },
    )
    .expect("applies");

    assert_eq!(report.side_files_written, vec![desc]);
    assert_eq!(text_of(&dir, "desc.txt").as_deref(), Some("A book about a dog.\n"));
    undo_with(&store, &report.apply_id.expect("undoable"), Some(&id)).expect("undoes");
    assert_eq!(text_of(&dir, "desc.txt").as_deref(), Some("Old blurb\n"));
    done(&id);
}

#[test]
fn side_files_alone_are_written_and_undone() {
    let (dir, _listing, id) = album("side-alone");
    let (_store_dir, store) = store("side-alone-store");
    open(&id, false).expect("opens");
    edit(&id, vec![set_all(TagField::Performer, &["Vladimir Kudla"])]).expect("edits");
    apply_with(&store, &id, request()).expect("the tags are written first, without side files");

    let report = apply_with(&store, &id, with_side_files()).expect("applies");

    assert!(report.written.is_empty());
    assert_eq!(report.side_files_written, vec![path_of(&dir, "reader.txt")]);
    let undone = undo_with(&store, &report.apply_id.expect("undoable"), Some(&id)).expect("undoes");
    assert_eq!(undone.restored, vec![dir.join("reader.txt")]);
    assert_eq!(text_of(&dir, "reader.txt"), None);
    done(&id);
}

#[test]
fn a_symlink_named_reader_txt_is_never_followed() {
    let (dir, _listing, id) = album("side-link");
    let (_store_dir, store) = store("side-link-store");
    std::fs::write(dir.join("elsewhere.txt"), "keep me\n").expect("writes");
    std::os::unix::fs::symlink(dir.join("elsewhere.txt"), dir.join("reader.txt")).expect("links");
    open(&id, false).expect("opens");
    edit(&id, vec![set_all(TagField::Performer, &["Vladimir Kudla"])]).expect("edits");

    assert_eq!(
        side_file_changes(&id).expect("lists"),
        vec![SideFileChange::Withheld {
            file: SideFile::Reader,
            reason: SideFileWithheld::NotAFile,
        }]
    );
    let report = apply_with(&store, &id, with_side_files()).expect("applies");

    assert!(report.side_files_written.is_empty());
    assert!(
        std::fs::symlink_metadata(dir.join("reader.txt"))
            .expect("stats")
            .file_type()
            .is_symlink()
    );
    assert_eq!(text_of(&dir, "elsewhere.txt").as_deref(), Some("keep me\n"));
    done(&id);
}

#[test]
fn a_session_off_the_local_disk_lists_and_writes_no_side_files() {
    let (dir, listing) = folder("side-remote", "smb-share", &[("a.mp3", mp3_frames())]);
    let (_store_dir, store) = store("side-remote-store");
    let id = open_rename(listing.id(), true, None, 0).expect("opens").session_id;
    open(&id, false).expect("opens read-only");

    assert!(side_file_changes(&id).expect("lists").is_empty());
    assert_eq!(
        apply_with(&store, &id, with_side_files()),
        Err(TagEditorError::ReadOnly)
    );
    assert_eq!(text_of(&dir, "reader.txt"), None);
    done(&id);
}

fn performers(values: &[&str]) -> FileTags {
    let mut tags = tags(&[]);
    tags.fields.insert(
        TagField::Performer,
        values.iter().map(|value| (*value).to_string()).collect(),
    );
    tags
}

#[test]
fn narrators_agree_in_any_order_and_a_track_without_one_disagrees() {
    assert_eq!(
        wanted(&[performers(&["A", "B"]), performers(&["B", "A", "A"])]),
        vec![(SideFile::Reader, Ok("A, B".to_string()))]
    );
    assert_eq!(
        wanted(&[performers(&["A"]), tags(&[])]),
        vec![(SideFile::Reader, Err(SideFileWithheld::TracksDisagree))]
    );
    assert!(wanted(&[tags(&[]), tags(&[])]).is_empty());
    let long = "x".repeat(usize::try_from(SIDE_FILE_LIMIT).expect("fits"));
    assert_eq!(
        wanted(&[tags(&[(TagField::Description, &long)])]),
        vec![(SideFile::Desc, Err(SideFileWithheld::TooLong))]
    );
}
