//! The change list names what the write kind removes beyond the fields (an MP3's conductor under
//! Audiobook), and the write removes exactly that.

use cmdr_media_meta::test_fixtures::mp3_frames;
use cmdr_tags::{ContentKind, KindRemoval, TagField, removed_by_kind};

use super::apply::{ApplyRequest, apply_with};
use super::apply_test::{mp3_with_v24_frames, request, store};
use super::session::{TagEdit, changes, close, edit, open};
use super::session_test::folder;
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
