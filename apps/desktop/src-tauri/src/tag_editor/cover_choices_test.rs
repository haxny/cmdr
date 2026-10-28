//! Every cover of the book: grouped by key with how many files carry it, "none" counted, one
//! used for every file as an in-memory change (nothing written), and a new one set from bytes.

use cmdr_tags::{ContentKind, Cover, FileTags, TagField, WriteOptions, write_tags};

use super::cover_choices::{CoverChoice, NO_COVER_KEY, choices_of, image_of, list, set_for_all, use_for_all};
use super::session::{Entry, Loaded, TagEditorError, changes, close, open};
use super::session_test::{album, on_disk, tags};
use super::view::{CoverSummary, cover_key};
use crate::multi_rename::session::close as close_rename;

/// A PNG's signature and IHDR start: a picture by its bytes, with a different tail per `fill`.
fn png(fill: u8, len: usize) -> Cover {
    let mut data = b"\x89PNG\r\n\x1A\n\0\0\0\rIHDR".to_vec();
    data.resize(len.max(data.len()), fill);
    Cover {
        mime: "image/png".to_string(),
        data: data.into(),
    }
}

fn loaded(cover: Option<Cover>) -> Entry {
    let tags = FileTags {
        cover,
        ..FileTags::default()
    };
    Entry::Loaded(Box::new(Loaded {
        cover_key: tags.cover.as_ref().map(cover_key),
        disk: tags.clone(),
        edited: tags,
        duration_ms: None,
        fingerprint: None,
    }))
}

fn key_of(cover: &Cover) -> String {
    format!("{:016x}", cover_key(cover))
}

#[test]
fn choices_group_by_cover_key_with_counts_most_common_first_and_none_last() {
    let front = png(1, 40);
    let back = png(2, 40);
    let entries = [
        loaded(None),
        loaded(Some(back.clone())),
        loaded(Some(front.clone())),
        loaded(Some(front.clone())),
        loaded(None),
        // Same bytes, a separate buffer (another file's own copy): still one choice.
        loaded(Some(Cover {
            mime: front.mime.clone(),
            data: front.data.to_vec().into(),
        })),
    ];
    let files: Vec<&Loaded> = entries.iter().filter_map(Entry::loaded).collect();
    let choices = choices_of(&files);
    let summary: Vec<(String, u32, u64)> = choices.iter().map(|c| (c.key.clone(), c.files, c.bytes)).collect();
    assert_eq!(
        summary,
        vec![
            (key_of(&front), 3, 40),
            (key_of(&back), 1, 40),
            (NO_COVER_KEY.to_string(), 2, 0),
        ]
    );
    assert_eq!(choices[0].mime, "image/png");
    assert_eq!(choices[2].mime, "");
}

#[test]
fn none_is_counted_and_no_files_give_no_choices() {
    let entries = [loaded(None), loaded(None)];
    let files: Vec<&Loaded> = entries.iter().filter_map(Entry::loaded).collect();
    assert_eq!(
        choices_of(&files),
        vec![CoverChoice {
            key: NO_COVER_KEY.to_string(),
            mime: String::new(),
            bytes: 0,
            width: None,
            height: None,
            files: 2,
        }]
    );
    assert!(choices_of(&[]).is_empty());
}

/// The user's case: the files carry different covers. Each shows, one is used for all, and that
/// is a change of the others in memory only, listed for the write; the files stay as they were.
#[test]
fn use_for_all_sets_that_cover_on_every_file_and_lists_it_as_a_change() {
    let (dir, _listing, id) = album("tag-editor-cover-choices");
    let front = png(7, 64);
    let mut with_cover = tags(&[(TagField::Title, "Alpha"), (TagField::Album, "Book")]);
    with_cover.cover = Some(front.clone());
    write_tags(&dir.join("a.mp3"), &with_cover, &WriteOptions::default()).expect("writes the cover");
    open(&id, false).expect("opens");

    let choices = list(&id).expect("lists");
    let counts: Vec<(&str, u32)> = choices.iter().map(|c| (c.key.as_str(), c.files)).collect();
    assert_eq!(counts, vec![(key_of(&front).as_str(), 1), (NO_COVER_KEY, 2)]);
    assert!(
        image_of(&id, Some(&key_of(&front)))
            .expect("an image")
            .is_some_and(|url| url.starts_with("data:image/png;base64,"))
    );
    assert_eq!(image_of(&id, Some(NO_COVER_KEY)), Ok(None));
    assert_eq!(image_of(&id, None), Ok(None), "no cover every file shares");

    let view = use_for_all(&id, &key_of(&front)).expect("uses it");
    assert!(matches!(view.cover, CoverSummary::Same { bytes: 64, .. }));
    let listed = changes(&id, ContentKind::Audiobook).expect("changes");
    let covered: Vec<(&str, bool)> = listed.iter().map(|c| (c.file_name.as_str(), c.cover)).collect();
    assert_eq!(covered, vec![("b.mp3", true), ("c.mp3", true)]);
    // Nothing is written before Write: the disk keeps its covers.
    assert_eq!(on_disk(&dir, "a.mp3").cover, Some(front.clone()));
    assert_eq!(on_disk(&dir, "b.mp3").cover, None);
    let after = list(&id).expect("lists");
    assert_eq!(after.len(), 1);
    assert_eq!((after[0].key.as_str(), after[0].files), (key_of(&front).as_str(), 3));

    close(&id);
    close_rename(&id);
}

#[test]
fn an_unknown_cover_key_refuses_and_changes_nothing() {
    let (_dir, _listing, id) = album("tag-editor-cover-unknown");
    open(&id, false).expect("opens");
    assert_eq!(use_for_all(&id, "0123456789abcdef"), Err(TagEditorError::UnknownCover));
    assert_eq!(
        image_of(&id, Some("0123456789abcdef")),
        Err(TagEditorError::UnknownCover)
    );
    assert!(changes(&id, ContentKind::Audiobook).expect("changes").is_empty());
    close(&id);
    close_rename(&id);
}

#[test]
fn a_new_cover_goes_to_every_file_in_memory_and_a_closed_session_is_typed() {
    let (dir, _listing, id) = album("tag-editor-cover-set");
    assert_eq!(list(&id), Err(TagEditorError::NotOpen));
    open(&id, false).expect("opens");
    let pasted = png(3, 30);
    let view = set_for_all(&id, pasted.clone()).expect("sets");
    assert!(matches!(view.cover, CoverSummary::Same { bytes: 30, .. }));
    assert_eq!(changes(&id, ContentKind::Audiobook).expect("changes").len(), 3);
    assert_eq!(on_disk(&dir, "a.mp3").cover, None, "nothing written before Write");
    close(&id);
    close_rename(&id);
}
