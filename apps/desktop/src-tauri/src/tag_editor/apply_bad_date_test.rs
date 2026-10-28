//! A date on disk the writer refuses (`TagError::BadDate`): named per file, and written once
//! the user corrects it. Split from `apply_test.rs`.

use cmdr_media_meta::test_fixtures::mp3_frames;
use cmdr_tags::{TagError, TagField};

use super::apply::apply_with;
use super::apply_test::{album_of, done, names, one, request, set_album, store};
use super::session::{TagEdit, edit, open};
use super::session_test::{folder, on_disk};
use crate::multi_rename::session::open as open_rename;

/// An MP3 with an ID3v2.3 tag of these Latin-1 text frames.
fn mp3_with_v23_frames(frames: &[(&[u8; 4], &str)]) -> Vec<u8> {
    fn frame(id: &[u8; 4], text: &str) -> Vec<u8> {
        let body: Vec<u8> = std::iter::once(0).chain(text.bytes()).collect();
        let size = u32::try_from(body.len()).expect("a small frame").to_be_bytes();
        [id.as_slice(), &size, &[0, 0], &body].concat()
    }
    let frames: Vec<u8> = frames.iter().flat_map(|(id, text)| frame(id, text)).collect();
    let size = u32::try_from(frames.len()).expect("a small tag");
    let syncsafe = [
        (size >> 21) & 0x7F,
        (size >> 14) & 0x7F,
        (size >> 7) & 0x7F,
        size & 0x7F,
    ]
    .map(|b| b as u8);
    [b"ID3".as_slice(), &[3, 0, 0], &syncsafe, &frames, &mp3_frames()].concat()
}

/// A tagger that writes `TDAT` as month-day (`0920`) leaves a date the file can't hold
/// (`2010-20-09`): the write refuses it as a bad date, naming it, and once the user corrects the
/// Date the same file writes. Seen on a real audiobook, 2026-10-27.
#[test]
fn a_bad_date_on_disk_is_refused_by_name_and_writes_once_corrected() {
    let bad = mp3_with_v23_frames(&[(b"TIT2", "Kapitola"), (b"TYER", "2010"), (b"TDAT", "0920")]);
    let (dir, listing) = folder("tag-apply-bad-date", "root", &[("a.mp3", bad)]);
    let id = open_rename(listing.id(), true, None, 0)
        .expect("the rename session opens")
        .session_id;
    let (_store_dir, store) = store("tag-apply-bad-date-store");
    open(&id, false).expect("opens");
    assert_eq!(
        on_disk(&dir, "a.mp3").fields.get(&TagField::Date).cloned(),
        one("2010-20-09")
    );

    edit(&id, vec![set_album("Kniha")]).expect("edits");
    let refused = apply_with(&store, &id, request()).expect("applies");
    assert!(refused.written.is_empty());
    assert_eq!(
        refused.failed.iter().map(|row| row.error.clone()).collect::<Vec<_>>(),
        vec![TagError::BadDate("2010-20-09".to_string())]
    );

    edit(
        &id,
        vec![TagEdit::SetAll {
            field: TagField::Date,
            values: vec!["2010-09-20".to_string()],
        }],
    )
    .expect("edits");
    let written = apply_with(&store, &id, request()).expect("applies");
    assert_eq!(names(&written.written), vec!["a.mp3"]);
    assert_eq!(
        on_disk(&dir, "a.mp3").fields.get(&TagField::Date).cloned(),
        one("2010-09-20")
    );
    assert_eq!(album_of(&dir, "a.mp3"), one("Kniha"));
    done(&id);
}
