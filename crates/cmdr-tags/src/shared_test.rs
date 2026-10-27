//! Tests for the shared view, edits, and the change list.

use super::model::{Cover, FileTags, TagField};
use super::shared::*;

fn file(pairs: &[(TagField, &[&str])]) -> FileTags {
    let mut tags = FileTags::default();
    for (field, values) in pairs {
        tags.fields
            .insert(*field, values.iter().map(|v| (*v).to_string()).collect());
    }
    tags
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).to_string()).collect()
}

fn cover(bytes: &[u8]) -> Cover {
    Cover {
        mime: "image/jpeg".to_string(),
        data: bytes.into(),
    }
}

#[test]
fn same_album_is_same() {
    let files = [file(&[(TagField::Album, &["X"])]), file(&[(TagField::Album, &["X"])])];
    let view = shared_view(&files);
    assert_eq!(view[&TagField::Album], SharedValue::Same(strings(&["X"])));
}

#[test]
fn different_titles_are_multiple() {
    let files = [file(&[(TagField::Title, &["A"])]), file(&[(TagField::Title, &["B"])])];
    assert_eq!(shared_view(&files)[&TagField::Title], SharedValue::Multiple);
}

#[test]
fn present_in_some_files_is_multiple() {
    let files = [file(&[(TagField::Genre, &["Drama"])]), file(&[])];
    assert_eq!(shared_view(&files)[&TagField::Genre], SharedValue::Multiple);
}

#[test]
fn absent_everywhere_is_empty_and_every_field_is_a_key() {
    let files = [file(&[]), file(&[])];
    let view = shared_view(&files);
    assert_eq!(view.len(), TagField::ALL.len());
    assert!(view.values().all(|v| *v == SharedValue::Empty));
}

#[test]
fn no_files_is_all_empty() {
    let view = shared_view(&[]);
    assert_eq!(view.len(), TagField::ALL.len());
    assert!(view.values().all(|v| *v == SharedValue::Empty));
}

#[test]
fn value_order_matters() {
    let files = [
        file(&[(TagField::Genre, &["A", "B"])]),
        file(&[(TagField::Genre, &["B", "A"])]),
    ];
    assert_eq!(shared_view(&files)[&TagField::Genre], SharedValue::Multiple);
}

#[test]
fn set_all_applies_to_every_file() {
    let files = [file(&[]), file(&[(TagField::Album, &["Old"])])];
    let edits = [Edit::SetAll {
        field: TagField::Album,
        values: strings(&["New"]),
    }];
    let out = apply_edits(&files, &edits);
    assert!(out.iter().all(|f| f.fields[&TagField::Album] == strings(&["New"])));
    assert_eq!(files[1].fields[&TagField::Album], strings(&["Old"]), "input untouched");
}

#[test]
fn set_all_with_empty_values_removes_the_field() {
    let files = [file(&[(TagField::Album, &["A"])]), file(&[(TagField::Album, &["B"])])];
    let out = apply_edits(
        &files,
        &[Edit::SetAll {
            field: TagField::Album,
            values: vec![],
        }],
    );
    assert!(out.iter().all(|f| !f.fields.contains_key(&TagField::Album)));
}

#[test]
fn values_are_trimmed_and_blank_ones_dropped() {
    let files = [file(&[])];
    let edits = [Edit::SetAll {
        field: TagField::Genre,
        values: strings(&["  Drama ", "", "   "]),
    }];
    assert_eq!(
        apply_edits(&files, &edits)[0].fields[&TagField::Genre],
        strings(&["Drama"])
    );
    let blank = [Edit::SetAll {
        field: TagField::Genre,
        values: strings(&["", " "]),
    }];
    assert!(!apply_edits(&files, &blank)[0].fields.contains_key(&TagField::Genre));
}

#[test]
fn set_one_touches_one_row() {
    let files = [file(&[(TagField::Title, &["A"])]), file(&[(TagField::Title, &["B"])])];
    let edits = [Edit::SetOne {
        row: 1,
        field: TagField::Title,
        values: strings(&["C"]),
    }];
    let out = apply_edits(&files, &edits);
    assert_eq!(out[0], files[0]);
    assert_eq!(out[1].fields[&TagField::Title], strings(&["C"]));
}

#[test]
fn set_one_out_of_range_is_ignored() {
    let files = [file(&[(TagField::Title, &["A"])])];
    let edits = [Edit::SetOne {
        row: 5,
        field: TagField::Title,
        values: strings(&["C"]),
    }];
    assert_eq!(apply_edits(&files, &edits), files.to_vec());
}

#[test]
fn edits_apply_in_order() {
    let files = [file(&[]), file(&[])];
    let edits = [
        Edit::SetAll {
            field: TagField::Album,
            values: strings(&["X"]),
        },
        Edit::SetOne {
            row: 0,
            field: TagField::Album,
            values: strings(&["Y"]),
        },
    ];
    let out = apply_edits(&files, &edits);
    assert_eq!(out[0].fields[&TagField::Album], strings(&["Y"]));
    assert_eq!(out[1].fields[&TagField::Album], strings(&["X"]));
}

#[test]
fn cover_edits() {
    let files = [
        FileTags {
            cover: Some(cover(b"a")),
            ..FileTags::default()
        },
        file(&[]),
    ];
    let kept = apply_edits(&files, &[Edit::Cover { edit: CoverEdit::Keep }]);
    assert_eq!(kept, files.to_vec());
    let set = apply_edits(
        &files,
        &[Edit::Cover {
            edit: CoverEdit::Set(cover(b"b")),
        }],
    );
    assert!(set.iter().all(|f| f.cover == Some(cover(b"b"))));
    let removed = apply_edits(
        &files,
        &[Edit::Cover {
            edit: CoverEdit::Remove,
        }],
    );
    assert!(removed.iter().all(|f| f.cover.is_none()));
}

#[test]
fn changes_list_only_differing_rows_and_fields() {
    let before = [
        file(&[(TagField::Title, &["A"]), (TagField::Album, &["X"])]),
        file(&[(TagField::Title, &["B"])]),
    ];
    let after = apply_edits(
        &before,
        &[
            Edit::SetOne {
                row: 1,
                field: TagField::Title,
                values: strings(&["C"]),
            },
            Edit::SetOne {
                row: 1,
                field: TagField::Genre,
                values: strings(&["G"]),
            },
        ],
    );
    let list = changes(&before, &after);
    assert_eq!(
        list,
        vec![FileChange {
            row: 1,
            fields: vec![TagField::Title, TagField::Genre],
            cover: false,
            extras: false
        }]
    );
}

#[test]
fn changes_flag_cover_and_extras() {
    let before = [FileTags {
        cover: Some(cover(b"a")),
        ..FileTags::default()
    }];
    let mut after = before.to_vec();
    after[0].cover = Some(cover(b"b"));
    after[0].extras.insert("X".to_string(), strings(&["1"]));
    let list = changes(&before, &after);
    assert_eq!(
        list,
        vec![FileChange {
            row: 0,
            fields: vec![],
            cover: true,
            extras: true
        }]
    );
}

#[test]
fn changes_of_identical_files_is_empty() {
    let files = [file(&[(TagField::Title, &["A"])])];
    assert!(changes(&files, &files).is_empty());
}

#[test]
fn changes_with_different_lengths_marks_extra_rows_changed() {
    let before = [file(&[(TagField::Title, &["A"])])];
    let after = [
        before[0].clone(),
        file(&[(TagField::Title, &["B"]), (TagField::Album, &["X"])]),
    ];
    let list = changes(&before, &after);
    assert_eq!(
        list,
        vec![FileChange {
            row: 1,
            fields: vec![TagField::Title, TagField::Album],
            cover: false,
            extras: false
        }]
    );
}

#[test]
fn totals_are_shared_fields() {
    let files = [
        file(&[(TagField::Track, &["01"]), (TagField::TrackTotal, &["12"])]),
        file(&[
            (TagField::Track, &["02"]),
            (TagField::TrackTotal, &["12"]),
            (TagField::DiscTotal, &["2"]),
        ]),
    ];
    let view = shared_view(&files);
    assert_eq!(view[&TagField::TrackTotal], SharedValue::Same(strings(&["12"])));
    assert_eq!(view[&TagField::DiscTotal], SharedValue::Multiple);
    let edited = apply_edits(
        &files,
        &[Edit::SetAll {
            field: TagField::TrackTotal,
            values: strings(&["14"]),
        }],
    );
    assert!(
        edited
            .iter()
            .all(|f| f.fields[&TagField::TrackTotal] == strings(&["14"]))
    );
}

#[test]
fn totals_follow_their_numbers_in_display_order() {
    let at = |field| TagField::ALL.iter().position(|f| *f == field).unwrap();
    assert_eq!(at(TagField::TrackTotal), at(TagField::Track) + 1);
    assert_eq!(at(TagField::DiscTotal), at(TagField::Disc) + 1);
    assert!(
        TagField::TrackTotal < TagField::Disc,
        "the derive order is the display order"
    );
    for total in [TagField::TrackTotal, TagField::DiscTotal] {
        assert!(!total.is_multi() && !total.keeps_diacritics());
    }
    assert_eq!(serde_json::to_string(&TagField::TrackTotal).unwrap(), r#""trackTotal""#);
    assert_eq!(serde_json::to_string(&TagField::DiscTotal).unwrap(), r#""discTotal""#);
}

#[test]
fn distinct_values_counts_and_orders() {
    let files: Vec<FileTags> = ["X", "X", "Y", "X", "Y", "Z"]
        .iter()
        .map(|v| file(&[(TagField::Album, &[v])]))
        .chain([file(&[(TagField::Title, &["no album"])])])
        .collect();
    let all = distinct_values(&files, TagField::Album, 10);
    assert_eq!(
        all,
        vec![
            ValueCount {
                values: strings(&["X"]),
                files: 3
            },
            ValueCount {
                values: strings(&["Y"]),
                files: 2
            },
            ValueCount {
                values: strings(&["Z"]),
                files: 1
            },
        ]
    );
    assert_eq!(distinct_values(&files, TagField::Album, 2).len(), 2);
}

#[test]
fn distinct_values_ties_keep_first_appearance_and_multi_values_group() {
    let files = [
        file(&[(TagField::Genre, &["b", "a"])]),
        file(&[(TagField::Genre, &["c"])]),
        file(&[(TagField::Genre, &["b", "a"])]),
        file(&[(TagField::Genre, &["d"])]),
    ];
    let got = distinct_values(&files, TagField::Genre, 10);
    assert_eq!(
        got[0],
        ValueCount {
            values: strings(&["b", "a"]),
            files: 2
        }
    );
    assert_eq!(got[1].values, strings(&["c"]));
    assert_eq!(got[2].values, strings(&["d"]));
}

#[test]
fn distinct_values_many_distinct_values_in_order() {
    let files: Vec<FileTags> = (0..2000)
        .map(|i| file(&[(TagField::Title, &[&format!("t{}", i % 1000)])]))
        .collect();
    let got = distinct_values(&files, TagField::Title, 5);
    assert_eq!(got.len(), 5);
    assert_eq!(
        got[0],
        ValueCount {
            values: strings(&["t0"]),
            files: 2
        }
    );
    assert_eq!(got[4].values, strings(&["t4"]));
}
