//! The tab's pure view: summaries over entries, unreadable rows left out of them.

use std::path::Path;

use cmdr_tags::shared::SharedValue;
use cmdr_tags::{Cover, FileTags, TagField};

use super::session::{Entry, Loaded};
use super::view::{Access, CoverSummary, ReadErrorCode, build};

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
