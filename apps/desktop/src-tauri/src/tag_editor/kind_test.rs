//! The change list names what the write kind removes beyond the fields (an MP3's conductor under
//! Audiobook), and the write removes exactly that.

use cmdr_media_meta::test_fixtures::mp3_frames;
use cmdr_tags::{ContentKind, KindRemoval, TagField, removed_by_kind};

use super::apply::{ApplyRequest, WouldRemoveRow, apply_with, held_back};
use super::apply_test::{mp3_with_v24_frames, request, store};
use super::session::{TagEdit, changes, close, edit, open};
use super::session_test::{folder, path_of};
use crate::multi_rename::session::{close as close_rename, open as open_rename};

#[test]
fn an_audiobook_write_lists_the_conductor_it_removes_and_a_music_write_lists_nothing() {
    let (dir, listing) = folder(
        "tag-kind-conductor",
        "root",
        &[
            (
                "a.mp3",
                mp3_with_v24_frames(&[(b"TIT2", "Alpha"), (b"TPE3", "Dirigent")]),
            ),
            ("b.mp3", mp3_frames()),
        ],
    );
    let (_store_dir, store) = store("tag-kind-conductor-store");
    let id = open_rename(listing.id(), true, None, 0).expect("opens").session_id;
    open(&id, false).expect("opens");
    edit(
        &id,
        vec![TagEdit::SetAll {
            field: TagField::Album,
            values: vec!["Kniha".to_string()],
        }],
    )
    .expect("edits");

    let as_book = changes(&id, ContentKind::Audiobook).expect("lists");
    let removes: Vec<(&str, &[KindRemoval])> = as_book
        .iter()
        .map(|change| (change.file_name.as_str(), change.removes.as_slice()))
        .collect();
    assert_eq!(
        removes,
        vec![("a.mp3", &[KindRemoval::Conductor][..]), ("b.mp3", &[][..])]
    );
    let as_music = changes(&id, ContentKind::Music).expect("lists");
    assert!(as_music.iter().all(|change| change.removes.is_empty()));

    let report = apply_with(
        &store,
        &id,
        ApplyRequest {
            kind: ContentKind::Audiobook,
            allow_removals: vec![path_of(&dir, "a.mp3")],
            ..request()
        },
    )
    .expect("applies");
    assert_eq!(report.written.len(), 2);
    assert_eq!(
        removed_by_kind(&dir.join("a.mp3"), ContentKind::Audiobook).expect("reads"),
        vec![],
        "the write removed the conductor the list named"
    );
    close(&id);
    close_rename(&id);
}

/// A folder with an MP3 that has a conductor and one that hasn't, both edited.
fn conductor_book(tag: &str) -> (crate::test_support::TestDir, String) {
    let (dir, listing) = folder(
        tag,
        "root",
        &[
            (
                "a.mp3",
                mp3_with_v24_frames(&[(b"TIT2", "Alpha"), (b"TPE3", "Dirigent")]),
            ),
            ("b.mp3", mp3_frames()),
        ],
    );
    let id = open_rename(listing.id(), true, None, 0).expect("opens").session_id;
    open(&id, false).expect("opens");
    edit(
        &id,
        vec![TagEdit::SetAll {
            field: TagField::Album,
            values: vec!["Kniha".to_string()],
        }],
    )
    .expect("edits");
    (dir, id)
}

#[test]
fn an_audiobook_write_never_removes_a_conductor_it_was_not_allowed_to() {
    let (dir, id) = conductor_book("tag-kind-guard");
    let (_store_dir, store) = store("tag-kind-guard-store");
    let before = std::fs::read(dir.join("a.mp3")).expect("reads");

    let report = apply_with(&store, &id, request()).expect("applies");

    assert_eq!(
        report.would_remove,
        vec![WouldRemoveRow {
            path: path_of(&dir, "a.mp3"),
            removes: vec![KindRemoval::Conductor],
            unknown: false,
        }]
    );
    assert_eq!(report.written, vec![path_of(&dir, "b.mp3")]);
    assert_eq!(
        std::fs::read(dir.join("a.mp3")).expect("reads"),
        before,
        "a.mp3 is untouched"
    );
    assert_eq!(
        removed_by_kind(&dir.join("a.mp3"), ContentKind::Audiobook).expect("reads"),
        vec![KindRemoval::Conductor]
    );

    // Allowed (the list showed it): written, the conductor gone.
    let report = apply_with(
        &store,
        &id,
        ApplyRequest {
            allow_removals: vec![path_of(&dir, "a.mp3")],
            ..request()
        },
    )
    .expect("applies again");
    assert_eq!(report.written, vec![path_of(&dir, "a.mp3")]);
    assert!(report.would_remove.is_empty());
    assert_eq!(
        removed_by_kind(&dir.join("a.mp3"), ContentKind::Audiobook).expect("reads"),
        vec![]
    );
    close(&id);
    close_rename(&id);
}

#[test]
fn a_music_write_removes_nothing_so_holds_nothing_back() {
    let (dir, id) = conductor_book("tag-kind-music");
    let (_store_dir, store) = store("tag-kind-music-store");
    let report = apply_with(
        &store,
        &id,
        ApplyRequest {
            kind: ContentKind::Music,
            ..request()
        },
    )
    .expect("applies");
    assert!(report.would_remove.is_empty());
    assert_eq!(report.written.len(), 2);
    assert_eq!(
        removed_by_kind(&dir.join("a.mp3"), ContentKind::Audiobook).expect("reads"),
        vec![KindRemoval::Conductor],
        "a music write keeps the conductor"
    );
    close(&id);
    close_rename(&id);
}

#[test]
fn a_file_whose_removals_cannot_be_read_is_held_back_and_listed_as_unknown() {
    let dir = crate::test_support::TestDir::new("tag-kind-unreadable");
    let path = dir.join("broken.mp3");
    std::fs::write(&path, b"not audio at all").expect("writes");

    assert_eq!(
        held_back(&path, ContentKind::Audiobook, &|_| false),
        Some(WouldRemoveRow {
            path: path.to_string_lossy().into_owned(),
            removes: vec![],
            unknown: true,
        })
    );
    assert_eq!(
        held_back(&path, ContentKind::Audiobook, &|_| true),
        None,
        "allowed: the user saw it"
    );
    assert_eq!(
        held_back(&path, ContentKind::Music, &|_| false),
        None,
        "music removes nothing"
    );
}

#[test]
fn the_change_list_says_when_it_could_not_tell_what_the_kind_removes() {
    let (dir, id) = conductor_book("tag-kind-list-unknown");
    std::fs::write(dir.join("a.mp3"), b"gone bad").expect("writes");
    let listed = changes(&id, ContentKind::Audiobook).expect("lists");
    let a = listed
        .iter()
        .find(|change| change.file_name == "a.mp3")
        .expect("a is listed");
    assert!(a.removes_unknown, "an unreadable file is never \"removes nothing\"");
    assert!(a.removes.is_empty());
    close(&id);
    close_rename(&id);
}
