//! Tests for the auto-numbering wizard.

use super::model::{FileTags, TagField};
use super::numbering::{DiscOptions, NumberOptions};
use super::tools::{TagTool, ToolContext, apply_tool_in};

fn files(albums: &[&str]) -> Vec<FileTags> {
    albums
        .iter()
        .map(|album| {
            let mut tags = FileTags::default();
            if !album.is_empty() {
                tags.fields.insert(TagField::Album, vec![(*album).to_string()]);
            }
            tags
        })
        .collect()
}

fn ctx(folders: &[&str]) -> ToolContext {
    ToolContext {
        folders: folders.iter().map(|f| (*f).to_string()).collect(),
        stems: Vec::new(),
        session_files: 0,
        book_folders: Vec::new(),
    }
}

fn disc() -> DiscOptions {
    DiscOptions {
        start: 1,
        total: false,
        pad: false,
        next_on_folder: true,
        next_on_field: None,
        reset_track: true,
        reset_disc_on_field: None,
    }
}

fn options(disc: Option<DiscOptions>) -> NumberOptions {
    NumberOptions {
        track_start: 1,
        track_total: false,
        track_pad: true,
        disc,
    }
}

fn run(input: &[FileTags], ctx: &ToolContext, opts: NumberOptions) -> Vec<FileTags> {
    apply_tool_in(input, ctx, &TagTool::AutoNumber(opts))
}

fn column(out: &[FileTags], field: TagField) -> Vec<String> {
    out.iter()
        .map(|f| {
            f.fields
                .get(&field)
                .and_then(|v| v.first())
                .cloned()
                .unwrap_or_default()
        })
        .collect()
}

#[test]
fn numbers_in_order_with_total() {
    let input = files(&["", "", "", ""]);
    let out = run(
        &input,
        &ctx(&[]),
        NumberOptions {
            track_total: true,
            ..options(None)
        },
    );
    assert_eq!(column(&out, TagField::Track), ["01", "02", "03", "04"]);
    assert_eq!(column(&out, TagField::TrackTotal), ["4", "4", "4", "4"]);
}

#[test]
fn pad_off_writes_plain() {
    let out = run(
        &files(&["", "", "", ""]),
        &ctx(&[]),
        NumberOptions {
            track_pad: false,
            ..options(None)
        },
    );
    assert_eq!(column(&out, TagField::Track), ["1", "2", "3", "4"]);
}

#[test]
fn start_zero_and_wide_numbers() {
    let out = run(
        &files(&["", "", ""]),
        &ctx(&[]),
        NumberOptions {
            track_start: 0,
            ..options(None)
        },
    );
    assert_eq!(column(&out, TagField::Track), ["00", "01", "02"]);
    let many = files(&vec![""; 100]);
    let out = run(&many, &ctx(&[]), options(None));
    assert_eq!(column(&out, TagField::Track)[0], "001");
    assert_eq!(column(&out, TagField::Track)[99], "100");
}

#[test]
fn next_disc_on_folder_change() {
    let input = files(&["", "", "", "", ""]);
    let opts = NumberOptions {
        track_total: true,
        ..options(Some(DiscOptions { total: true, ..disc() }))
    };
    let out = run(&input, &ctx(&["a", "a", "b", "b", "b"]), opts);
    assert_eq!(column(&out, TagField::Track), ["01", "02", "01", "02", "03"]);
    assert_eq!(column(&out, TagField::Disc), ["1", "1", "2", "2", "2"]);
    assert_eq!(column(&out, TagField::DiscTotal), ["2", "2", "2", "2", "2"]);
    assert_eq!(column(&out, TagField::TrackTotal), ["2", "2", "3", "3", "3"]);
}

#[test]
fn next_disc_on_album_change() {
    let input = files(&["X", "X", "Y", "Y", "Y"]);
    let opts = options(Some(DiscOptions {
        next_on_folder: false,
        next_on_field: Some(TagField::Album),
        ..disc()
    }));
    let out = run(&input, &ctx(&[]), opts);
    assert_eq!(column(&out, TagField::Track), ["01", "02", "01", "02", "03"]);
    assert_eq!(column(&out, TagField::Disc), ["1", "1", "2", "2", "2"]);
}

#[test]
fn no_reset_keeps_counting() {
    let opts = NumberOptions {
        track_total: true,
        ..options(Some(DiscOptions {
            reset_track: false,
            ..disc()
        }))
    };
    let out = run(&files(&["", "", ""]), &ctx(&["a", "a", "b"]), opts);
    assert_eq!(column(&out, TagField::Track), ["01", "02", "03"]);
    assert_eq!(column(&out, TagField::Disc), ["1", "1", "2"]);
    assert_eq!(column(&out, TagField::TrackTotal), ["3", "3", "3"]);
}

#[test]
fn reset_disc_on_field() {
    // Folders change every file; the album changes at index 3, where discs restart at 1.
    let input = files(&["X", "X", "X", "Y", "Y"]);
    let opts = options(Some(DiscOptions {
        total: true,
        reset_disc_on_field: Some(TagField::Album),
        ..disc()
    }));
    let out = run(&input, &ctx(&["a", "b", "c", "d", "e"]), opts);
    assert_eq!(column(&out, TagField::Disc), ["1", "2", "3", "1", "2"]);
    assert_eq!(column(&out, TagField::DiscTotal), ["3", "3", "3", "2", "2"]);
    assert_eq!(column(&out, TagField::Track), ["01", "01", "01", "01", "01"]);
}

#[test]
fn disc_pad() {
    let opts = options(Some(DiscOptions { pad: true, ..disc() }));
    let out = run(&files(&["", ""]), &ctx(&["a", "b"]), opts);
    assert_eq!(column(&out, TagField::Disc), ["01", "02"]);
}

#[test]
fn disc_start_zero_is_allowed() {
    let opts = options(Some(DiscOptions { start: 0, ..disc() }));
    let out = run(&files(&["", ""]), &ctx(&["a", "b"]), opts);
    assert_eq!(column(&out, TagField::Disc), ["0", "1"]);
}

#[test]
fn single_file_gets_no_track() {
    let mut input = files(&[""]);
    input[0].fields.insert(TagField::Track, vec!["7".into()]);
    input[0].fields.insert(TagField::TrackTotal, vec!["9".into()]);
    let opts = NumberOptions {
        track_total: true,
        ..options(Some(DiscOptions { total: true, ..disc() }))
    };
    let out = run(&input, &ctx(&["a"]), opts);
    assert!(!out[0].fields.contains_key(&TagField::Track));
    assert!(!out[0].fields.contains_key(&TagField::TrackTotal));
    assert_eq!(column(&out, TagField::Disc), ["1"]);
    assert_eq!(column(&out, TagField::DiscTotal), ["1"]);
}

#[test]
fn disc_none_leaves_disc_alone() {
    let mut input = files(&["", ""]);
    input[0].fields.insert(TagField::Disc, vec!["5".into()]);
    input[0].fields.insert(TagField::DiscTotal, vec!["6".into()]);
    let out = run(&input, &ctx(&["a", "b"]), options(None));
    assert_eq!(column(&out, TagField::Disc), ["5", ""]);
    assert_eq!(column(&out, TagField::DiscTotal), ["6", ""]);
}

#[test]
fn json_shape_is_flat_and_camel_case() {
    let json = serde_json::to_value(TagTool::AutoNumber(options(None))).unwrap();
    assert_eq!(json["kind"], "autoNumber");
    assert_eq!(json["trackStart"], 1);
    let back: TagTool = serde_json::from_value(json).unwrap();
    assert_eq!(back, TagTool::AutoNumber(options(None)));
}

#[test]
fn totals_off_removes_existing_totals() {
    let mut input = files(&["", ""]);
    for tags in &mut input {
        tags.fields.insert(TagField::TrackTotal, vec!["9".into()]);
        tags.fields.insert(TagField::DiscTotal, vec!["7".into()]);
    }
    let out = run(&input, &ctx(&["a", "b"]), options(Some(disc())));
    assert_eq!(column(&out, TagField::TrackTotal), ["", ""]);
    assert_eq!(column(&out, TagField::DiscTotal), ["", ""]);
}

#[test]
fn track_total_off_without_disc_keeps_disc_total() {
    let mut input = files(&["", ""]);
    for tags in &mut input {
        tags.fields.insert(TagField::TrackTotal, vec!["9".into()]);
        tags.fields.insert(TagField::DiscTotal, vec!["7".into()]);
    }
    let out = run(&input, &ctx(&[]), options(None));
    assert_eq!(column(&out, TagField::TrackTotal), ["", ""]);
    assert_eq!(column(&out, TagField::DiscTotal), ["7", "7"]);
}

#[test]
fn field_reset_alone_starts_a_new_group_and_restarts_tracks() {
    // No folder or next-field boundary; only the reset field changes at index 2.
    let opts = options(Some(DiscOptions {
        next_on_folder: false,
        reset_disc_on_field: Some(TagField::Album),
        ..disc()
    }));
    let out = run(&files(&["X", "X", "Y", "Y"]), &ctx(&[]), opts);
    assert_eq!(column(&out, TagField::Disc), ["1", "1", "1", "1"]);
    assert_eq!(column(&out, TagField::Track), ["01", "02", "01", "02"]);
}

#[test]
fn field_reset_with_tracks_continuing() {
    let opts = options(Some(DiscOptions {
        next_on_folder: false,
        reset_track: false,
        reset_disc_on_field: Some(TagField::Album),
        ..disc()
    }));
    let out = run(&files(&["X", "X", "Y"]), &ctx(&[]), opts);
    assert_eq!(column(&out, TagField::Track), ["01", "02", "03"]);
}

#[test]
fn folder_and_field_boundaries_together_make_one_new_disc() {
    let opts = options(Some(DiscOptions {
        next_on_field: Some(TagField::Album),
        ..disc()
    }));
    // Folder and album both change at index 2: one new disc, not two.
    let out = run(&files(&["X", "X", "Y", "Y"]), &ctx(&["a", "a", "b", "b"]), opts.clone());
    assert_eq!(column(&out, TagField::Disc), ["1", "1", "2", "2"]);
    // Only the album changes at index 1, only the folder at index 3.
    let out = run(&files(&["X", "Y", "Y", "Y"]), &ctx(&["a", "a", "a", "b"]), opts);
    assert_eq!(column(&out, TagField::Disc), ["1", "2", "2", "3"]);
}

#[test]
fn a_missing_album_next_to_a_present_one_is_a_boundary() {
    let opts = options(Some(DiscOptions {
        next_on_folder: false,
        next_on_field: Some(TagField::Album),
        ..disc()
    }));
    let out = run(&files(&["X", "", "", "X"]), &ctx(&[]), opts);
    assert_eq!(column(&out, TagField::Disc), ["1", "2", "2", "3"]);
}

#[test]
fn empty_input_gives_empty_output() {
    let out = run(&[], &ctx(&[]), options(Some(disc())));
    assert!(out.is_empty());
}

#[test]
fn one_file_of_a_bigger_book_is_numbered() {
    let opts = NumberOptions {
        track_total: true,
        ..options(None)
    };
    let book = ToolContext {
        session_files: 5,
        ..ctx(&["a"])
    };
    let out = run(&files(&[""]), &book, opts);
    assert_eq!(column(&out, TagField::Track), ["01"]);
    // A partial scope counts the files it numbers, as Mp3tag does.
    assert_eq!(column(&out, TagField::TrackTotal), ["1"]);
}
