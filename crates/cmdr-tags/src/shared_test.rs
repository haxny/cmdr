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
        data: bytes.to_vec(),
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
