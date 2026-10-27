//! The tab's pure view: summaries over entries, unreadable rows left out of them.

use std::path::{Path, PathBuf};

use cmdr_tags::shared::{SharedValue, ValueCount};
use cmdr_tags::{Cover, FileTags, TagField};

use super::session::{Entry, Loaded};
use super::view::{Access, CoverSummary, ReadErrorCode, build, cover_hashes, cover_key};

fn loaded(pairs: &[(TagField, &str)], cover: Option<&[u8]>, extras: &[&str]) -> Entry {
    let mut tags = FileTags::default();
    for (field, value) in pairs {
        tags.fields.entry(*field).or_default().push((*value).to_string());
    }
    tags.cover = cover.map(|data| Cover {
        mime: "image/jpeg".to_string(),
        data: data.into(),
    });
    for key in extras {
        tags.extras.insert((*key).to_string(), vec!["x".to_string()]);
    }
    Entry::Loaded(Box::new(Loaded {
        cover_key: tags.cover.as_ref().map(cover_key),
        disk: tags.clone(),
        edited: tags,
        duration_ms: Some(1500),
        fingerprint: None,
    }))
}

const OPEN: Access = Access {
    editable: true,
    reason: None,
};

#[test]
fn a_cover_is_same_multiple_or_empty() {
    let a = Path::new("/m/a.mp3");
    let b = Path::new("/m/b.mp3");
    let same = build(
        OPEN,
        &[
            (a, &loaded(&[], Some(&[1, 2]), &[])),
            (b, &loaded(&[], Some(&[1, 2]), &[])),
        ],
    );
    assert!(matches!(same.cover, CoverSummary::Same { bytes: 2, .. }));
    let differ = build(
        OPEN,
        &[
            (a, &loaded(&[], Some(&[1, 2]), &[])),
            (b, &loaded(&[], Some(&[9]), &[])),
        ],
    );
    assert_eq!(differ.cover, CoverSummary::Multiple);
    let partial = build(
        OPEN,
        &[(a, &loaded(&[], Some(&[1]), &[])), (b, &loaded(&[], None, &[]))],
    );
    assert_eq!(partial.cover, CoverSummary::Multiple);
    assert_eq!(build(OPEN, &[]).cover, CoverSummary::Empty);
}

#[test]
fn extras_are_a_sorted_union_and_rows_carry_track_title_duration() {
    let a = Path::new("/m/a.mp3");
    let b = Path::new("/m/b.mp3");
    let view = build(
        OPEN,
        &[
            (
                a,
                &loaded(
                    &[(TagField::Title, "T"), (TagField::Track, "3")],
                    None,
                    &["ZED", "ALPHA"],
                ),
            ),
            (b, &loaded(&[], None, &["MID"])),
        ],
    );
    assert_eq!(view.extras, vec!["ALPHA", "MID", "ZED"]);
    assert_eq!(view.tracks[0].title.as_deref(), Some("T"));
    assert_eq!(view.tracks[0].track.as_deref(), Some("3"));
    assert_eq!(view.tracks[0].duration_ms, Some(1500));
    assert_eq!(view.tracks[0].file_name, "a.mp3");
}

#[test]
fn an_unreadable_row_is_listed_but_counts_in_no_summary() {
    let a = Path::new("/m/a.mp3");
    let bad = Path::new("/m/bad.mp3");
    let view = build(
        OPEN,
        &[
            (a, &loaded(&[(TagField::Album, "Book")], None, &[])),
            (bad, &Entry::Unreadable(ReadErrorCode::Parse)),
        ],
    );
    assert_eq!(view.tracks.len(), 2);
    assert_eq!(view.tracks[1].read_error, Some(ReadErrorCode::Parse));
    let album = view
        .shared
        .iter()
        .find(|row| row.field == TagField::Album)
        .map(|row| &row.value);
    assert_eq!(album, Some(&SharedValue::Same(vec!["Book".to_string()])));
}

fn row(view: &super::view::TagEditorView, field: TagField) -> &super::view::SharedRow {
    view.shared
        .iter()
        .find(|row| row.field == field)
        .expect("a row per field")
}

#[test]
fn multiple_row_carries_choices() {
    let files = [("/m/a.mp3", "Book"), ("/m/b.mp3", "Other"), ("/m/c.mp3", "Book")]
        .map(|(path, album)| (Path::new(path), loaded(&[(TagField::Album, album)], None, &[])));
    let tracks: Vec<(&Path, &Entry)> = files.iter().map(|(path, entry)| (*path, entry)).collect();
    let view = build(OPEN, &tracks);
    let album = row(&view, TagField::Album);
    assert_eq!(album.value, SharedValue::Multiple);
    assert_eq!(
        album.choices,
        vec![
            ValueCount {
                values: vec!["Book".to_string()],
                files: 2,
            },
            ValueCount {
                values: vec!["Other".to_string()],
                files: 1,
            },
        ]
    );
}

#[test]
fn multiple_row_choices_are_capped() {
    let entries: Vec<(String, Entry)> = (0..60)
        .map(|n| {
            (
                format!("/m/{n}.mp3"),
                loaded(&[(TagField::Album, &n.to_string())], None, &[]),
            )
        })
        .collect();
    let tracks: Vec<(&Path, &Entry)> = entries
        .iter()
        .map(|(path, entry)| (Path::new(path.as_str()), entry))
        .collect();
    let album = row(&build(OPEN, &tracks), TagField::Album).clone();
    assert_eq!(album.choices.len(), 50);
    // The 10 values the list was cut by are counted, so the UI can say so.
    assert_eq!(album.choices_more, 10);
}

#[test]
fn a_list_that_fits_has_nothing_more() {
    let a = Path::new("/m/a.mp3");
    let b = Path::new("/m/b.mp3");
    let view = build(
        OPEN,
        &[
            (a, &loaded(&[(TagField::Album, "One")], None, &[])),
            (b, &loaded(&[(TagField::Album, "Two")], None, &[])),
        ],
    );
    assert_eq!(row(&view, TagField::Album).choices_more, 0);
}

#[test]
fn same_row_has_no_choices() {
    let a = Path::new("/m/a.mp3");
    let b = Path::new("/m/b.mp3");
    let tags = [(TagField::Album, "Book")];
    let view = build(OPEN, &[(a, &loaded(&tags, None, &[])), (b, &loaded(&tags, None, &[]))]);
    assert_eq!(
        row(&view, TagField::Album).value,
        SharedValue::Same(vec!["Book".to_string()])
    );
    assert!(view.shared.iter().all(|row| row.choices.is_empty()));
}

/// An audiobook's 22 tracks with a 2 MiB cover each: building the view (which every edit does)
/// hashes no cover, the keys come from the read.
#[test]
fn a_view_build_hashes_no_cover() {
    let cover = vec![7_u8; 2 * 1024 * 1024];
    let entries: Vec<(PathBuf, Entry)> = (1..=22)
        .map(|index| {
            (
                PathBuf::from(format!("/m/{index:02}.mp3")),
                loaded(&[], Some(&cover), &[]),
            )
        })
        .collect();
    let tracks: Vec<(&Path, &Entry)> = entries.iter().map(|(path, entry)| (path.as_path(), entry)).collect();

    let before = cover_hashes();
    let view = build(OPEN, &tracks);

    assert_eq!(cover_hashes(), before);
    assert!(matches!(view.cover, CoverSummary::Same { bytes, .. } if bytes == cover.len()));
}

#[test]
fn files_with_a_date_the_writer_refuses_are_counted_on_open() {
    let a = Path::new("/m/a.mp3");
    let b = Path::new("/m/b.mp3");
    let m4a = Path::new("/m/c.m4a");
    let ok = loaded(&[(TagField::Date, "2010")], None, &[]);
    let bad = loaded(&[(TagField::Date, "2010-20-09")], None, &[]);
    let view = build(OPEN, &[(a, &bad), (b, &ok), (m4a, &bad)]);
    // The M4A keeps its date as text, so only the MP3 is unwritable.
    assert_eq!(view.bad_dates, 1);
    assert_eq!(build(OPEN, &[(a, &ok), (b, &loaded(&[], None, &[]))]).bad_dates, 0);
}

#[test]
fn the_date_flag_clears_once_the_date_is_edited() {
    let a = Path::new("/m/a.mp3");
    let Entry::Loaded(mut file) = loaded(&[(TagField::Date, "2010-20-09")], None, &[]) else {
        unreachable!()
    };
    assert_eq!(build(OPEN, &[(a, &Entry::Loaded(file.clone()))]).bad_dates, 1);
    file.edited.fields.insert(TagField::Date, vec!["2010".to_string()]);
    assert_eq!(build(OPEN, &[(a, &Entry::Loaded(file))]).bad_dates, 0);
}
