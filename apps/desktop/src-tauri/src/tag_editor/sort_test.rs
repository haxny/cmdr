//! The Tags window's track order: the comparison, the sort of a real session, and the order a
//! session opens in.

use std::collections::BTreeMap;

use cmdr_media_meta::test_fixtures::mp3_frames;
use cmdr_tags::{FileTags, TagField};

use super::session::{Loaded, close, edit, open, unchanged_on_disk};
use super::session_test::{album, folder, path_of, set_title, tag_file};
use super::sort::{SortItem, TrackSort, TrackSortKey, sort_tracks, sorted_names};
use super::view::TagEditorView;
use crate::multi_rename::session::{RowMove, close as close_rename, files_of, move_row, open as open_rename};

const ASC: bool = false;

fn sort(key: TrackSortKey, descending: bool) -> TrackSort {
    TrackSort { key, descending }
}

fn names(view: &TagEditorView) -> Vec<&str> {
    view.tracks.iter().map(|row| row.file_name.as_str()).collect()
}

/// Mp3s named `name` tagged with `pairs`, in a fresh session.
fn session_of(tag: &str, files: &[(&str, &[(TagField, &str)])]) -> (crate::test_support::TestDir, String, impl Drop) {
    let entries: Vec<(&str, Vec<u8>)> = files.iter().map(|(name, _)| (*name, mp3_frames())).collect();
    let (dir, listing) = folder(tag, "root", &entries);
    for (name, pairs) in files {
        tag_file(&dir, name, pairs);
    }
    let session = open_rename(listing.id(), true, None, 0).expect("the rename session opens");
    (dir, session.session_id, listing)
}

fn loaded(pairs: &[(TagField, &str)], duration_ms: Option<u64>) -> Loaded {
    let mut tags = FileTags::default();
    let mut fields: BTreeMap<TagField, Vec<String>> = BTreeMap::new();
    for (field, value) in pairs {
        fields.entry(*field).or_default().push((*value).to_string());
    }
    tags.fields = fields;
    Loaded {
        disk: tags.clone(),
        edited: tags,
        duration_ms,
        cover_key: None,
        fingerprint: None,
    }
}

fn collator() -> std::sync::Arc<cmdr_collation::NameCollator> {
    crate::listing_host::listings().collator()
}

#[test]
fn sort_by_disc_track_orders_numerically() {
    let (_dir, id, _listing) = session_of(
        "tag-sort-disc-track",
        &[
            ("a.mp3", &[(TagField::Track, "10")]),
            ("b.mp3", &[(TagField::Track, "2")]),
            ("c.mp3", &[(TagField::Track, "1/12")]),
            ("d.mp3", &[(TagField::Track, "1"), (TagField::Disc, "2")]),
            ("e.mp3", &[(TagField::Title, "no number")]),
            ("f.mp3", &[(TagField::Title, "no number either"), (TagField::Disc, "1")]),
        ],
    );
    open(&id, false).expect("opens");

    let view = sort_tracks(&id, sort(TrackSortKey::DiscTrack, ASC)).expect("sorts");

    assert_eq!(names(&view), vec!["c.mp3", "b.mp3", "a.mp3", "d.mp3", "e.mp3", "f.mp3"]);
    assert_eq!(view.sort, Some(sort(TrackSortKey::DiscTrack, ASC)));
    close(&id);
    close_rename(&id);
}

#[test]
fn sort_by_title_uses_collation() {
    let titles = ["b", "Á", "a10", "a9"];
    let files: Vec<(String, [(TagField, &str); 1])> = titles
        .iter()
        .enumerate()
        .map(|(index, title)| {
            (
                format!("{}.mp3", (b'a' + index as u8) as char),
                [(TagField::Title, *title)],
            )
        })
        .collect();
    let refs: Vec<(&str, &[(TagField, &str)])> = files.iter().map(|(n, p)| (n.as_str(), &p[..])).collect();
    let (_dir, id, _listing) = session_of("tag-sort-title", &refs);
    open(&id, false).expect("opens");

    let view = sort_tracks(&id, sort(TrackSortKey::Title, ASC)).expect("sorts");

    let mut expected = titles.to_vec();
    let collator = collator();
    expected.sort_by(|a, b| collator.compare(a, b));
    let got: Vec<&str> = view
        .tracks
        .iter()
        .map(|row| row.title.as_deref().unwrap_or(""))
        .collect();
    assert_eq!(got, expected);
    close(&id);
    close_rename(&id);
}

#[test]
fn sort_descending_reverses() {
    let (_dir, id, _listing) = session_of(
        "tag-sort-descending",
        &[
            ("a.mp3", &[(TagField::Track, "1")]),
            ("b.mp3", &[(TagField::Track, "2")]),
            ("c.mp3", &[(TagField::Track, "3")]),
            ("d.mp3", &[(TagField::Title, "no number")]),
        ],
    );
    open(&id, false).expect("opens");

    let view = sort_tracks(&id, sort(TrackSortKey::DiscTrack, true)).expect("sorts");
    assert_eq!(names(&view), vec!["c.mp3", "b.mp3", "a.mp3", "d.mp3"]);

    let view = sort_tracks(&id, sort(TrackSortKey::FileName, true)).expect("sorts");
    assert_eq!(names(&view), vec!["d.mp3", "c.mp3", "b.mp3", "a.mp3"]);
    close(&id);
    close_rename(&id);
}

#[test]
fn unreadable_tracks_sort_last_by_name() {
    let (dir, listing) = folder(
        "tag-sort-unreadable",
        "root",
        &[
            ("a.flac", vec![0x42; 64]),
            ("b.mp3", mp3_frames()),
            ("c.flac", vec![0x42; 64]),
            ("d.mp3", mp3_frames()),
        ],
    );
    tag_file(&dir, "b.mp3", &[(TagField::Track, "2")]);
    tag_file(&dir, "d.mp3", &[(TagField::Track, "1")]);
    let id = open_rename(listing.id(), true, None, 0).expect("opens").session_id;
    open(&id, false).expect("opens");

    let view = sort_tracks(&id, sort(TrackSortKey::DiscTrack, ASC)).expect("sorts");
    assert_eq!(names(&view), vec!["d.mp3", "b.mp3", "a.flac", "c.flac"]);
    let view = sort_tracks(&id, sort(TrackSortKey::DiscTrack, true)).expect("sorts");
    assert_eq!(names(&view), vec!["b.mp3", "d.mp3", "a.flac", "c.flac"]);
    close(&id);
    close_rename(&id);
}

#[test]
fn sort_by_length_puts_unknown_lengths_last_and_ties_by_name() {
    let collator = collator();
    let short = loaded(&[], Some(1_000));
    let long = loaded(&[], Some(9_000));
    let same = loaded(&[], Some(1_000));
    let unknown = loaded(&[], None);
    let items = [
        SortItem {
            name: "a",
            loaded: Some(&unknown),
        },
        SortItem {
            name: "b",
            loaded: Some(&long),
        },
        SortItem {
            name: "d",
            loaded: Some(&same),
        },
        SortItem {
            name: "c",
            loaded: Some(&short),
        },
    ];

    assert_eq!(
        sorted_names(&items, sort(TrackSortKey::Length, ASC), &collator),
        vec!["c", "d", "b", "a"]
    );
    assert_eq!(
        sorted_names(&items, sort(TrackSortKey::Length, true), &collator),
        vec!["b", "c", "d", "a"]
    );
}

#[test]
fn file_name_sort_compares_the_whole_relative_name() {
    let collator = collator();
    let items = [
        SortItem {
            name: "Disc 2/01.mp3",
            loaded: None,
        },
        SortItem {
            name: "Disc 1/02.mp3",
            loaded: None,
        },
        SortItem {
            name: "Disc 1/01.mp3",
            loaded: None,
        },
    ];
    let readable = loaded(&[], None);
    let items: Vec<SortItem> = items
        .into_iter()
        .map(|item| SortItem {
            loaded: Some(&readable),
            ..item
        })
        .collect();

    assert_eq!(
        sorted_names(&items, sort(TrackSortKey::FileName, ASC), &collator),
        vec!["Disc 1/01.mp3", "Disc 1/02.mp3", "Disc 2/01.mp3"]
    );
}

#[test]
fn sorting_touches_neither_the_edits_nor_the_files_nor_other_names() {
    let (dir, _listing, id) = album("tag-sort-edits");
    open(&id, false).expect("opens");
    edit(&id, vec![set_title(&dir, "a.mp3", "Zulu edited")]).expect("edits a");

    let view = sort_tracks(&id, sort(TrackSortKey::Title, ASC)).expect("sorts");

    assert_eq!(
        view.tracks
            .iter()
            .map(|row| row.title.as_deref().unwrap())
            .collect::<Vec<_>>(),
        vec!["Beta", "Gamma", "Zulu edited"]
    );
    assert!(view.dirty);
    assert!(unchanged_on_disk(&id, &dir.join("a.mp3")).expect("asked"));
    // The cover and the text file keep their slots among the session's names.
    let all = files_of(&id).expect("the rename session is open");
    assert_eq!(&all.names[..], ["b.mp3", "c.mp3", "a.mp3", "cover.jpg", "notes.txt"]);
    // An edit still lands on its file by path.
    let view = edit(&id, vec![set_title(&dir, "a.mp3", "Again")]).expect("edits a");
    assert_eq!(view.tracks[2].title.as_deref(), Some("Again"));
    assert_eq!(view.tracks[2].path, path_of(&dir, "a.mp3"));
    close(&id);
    close_rename(&id);
}

#[test]
fn first_open_sorts_by_disc_track_when_any_track_has_a_number() {
    let (_dir, id, _listing) = session_of(
        "tag-sort-first-open",
        &[
            ("a.mp3", &[(TagField::Title, "no number")]),
            ("b.mp3", &[(TagField::Track, "2")]),
            ("c.mp3", &[(TagField::Track, "1")]),
        ],
    );

    let view = open(&id, false).expect("opens");
    assert_eq!(names(&view), vec!["c.mp3", "b.mp3", "a.mp3"]);
    assert_eq!(view.sort, Some(sort(TrackSortKey::DiscTrack, ASC)));
    // The rename session's own order moved with it.
    assert_eq!(&files_of(&id).expect("open").names[..], ["c.mp3", "b.mp3", "a.mp3"]);

    // The user's sort survives a reopen and a refresh.
    sort_tracks(&id, sort(TrackSortKey::FileName, true)).expect("sorts");
    let view = open(&id, false).expect("reopens");
    assert_eq!(names(&view), vec!["c.mp3", "b.mp3", "a.mp3"]);
    assert_eq!(view.sort, Some(sort(TrackSortKey::FileName, true)));
    let view = open(&id, true).expect("refreshes");
    assert_eq!(view.sort, Some(sort(TrackSortKey::FileName, true)));
    close(&id);
    close_rename(&id);
}

#[test]
fn first_open_keeps_file_name_order_otherwise() {
    let (_dir, _listing, id) = album("tag-sort-first-open-names");

    let view = open(&id, false).expect("opens");

    assert_eq!(names(&view), vec!["a.mp3", "b.mp3", "c.mp3"]);
    assert_eq!(view.sort, Some(sort(TrackSortKey::FileName, ASC)));
    close(&id);
    close_rename(&id);
}

#[test]
fn a_file_name_sort_keeps_unreadable_files_in_name_order() {
    let collator = collator();
    let readable = loaded(&[], None);
    let items = [
        SortItem {
            name: "c.mp3",
            loaded: Some(&readable),
        },
        SortItem {
            name: "b.flac",
            loaded: None,
        },
        SortItem {
            name: "a.mp3",
            loaded: Some(&readable),
        },
    ];

    assert_eq!(
        sorted_names(&items, sort(TrackSortKey::FileName, ASC), &collator),
        vec!["a.mp3", "b.flac", "c.mp3"]
    );
}

#[test]
fn a_row_moved_by_another_road_clears_the_stored_sort() {
    let (_dir, id, _listing) = session_of(
        "tag-sort-stale",
        &[
            ("a.mp3", &[(TagField::Track, "1")]),
            ("b.mp3", &[(TagField::Track, "2")]),
            ("c.mp3", &[(TagField::Track, "3")]),
        ],
    );
    open(&id, false).expect("opens");
    let view = sort_tracks(&id, sort(TrackSortKey::DiscTrack, ASC)).expect("sorts");
    assert_eq!(view.sort, Some(sort(TrackSortKey::DiscTrack, ASC)));

    // Multi-Rename's own reorder: the header must no longer claim an order.
    move_row(&id, 0, RowMove::Down).expect("moves");
    let view = open(&id, false).expect("reopens");
    assert_eq!(names(&view), vec!["b.mp3", "a.mp3", "c.mp3"]);
    assert_eq!(view.sort, None);

    // The moved order is the user's: reopening never sorts it back.
    let view = open(&id, false).expect("reopens again");
    assert_eq!(names(&view), vec!["b.mp3", "a.mp3", "c.mp3"]);
    assert_eq!(view.sort, None);

    // A fresh sort sets it again.
    let view = sort_tracks(&id, sort(TrackSortKey::FileName, ASC)).expect("sorts");
    assert_eq!(view.sort, Some(sort(TrackSortKey::FileName, ASC)));
    close(&id);
    close_rename(&id);
}
