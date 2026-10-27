//! Tests for reading tags from file names.

use super::model::{FileTags, TagField};
use super::name_pattern::*;
use super::tools::{TagTool, ToolContext, apply_tool_in};

fn format(p: &str) -> NamePattern {
    NamePattern::Format { pattern: p.to_string() }
}

fn regex(p: &str) -> NamePattern {
    NamePattern::Regex { pattern: p.to_string() }
}

fn read(p: &NamePattern, stem: &str) -> Option<Vec<(TagField, String)>> {
    compile_pattern(p).expect("compiles").read(stem)
}

fn pair(field: TagField, value: &str) -> (TagField, String) {
    (field, value.to_string())
}

#[test]
fn format_track_title() {
    assert_eq!(
        read(&format("%track% - %title%"), "03 - Posledni soud"),
        Some(vec![
            pair(TagField::Track, "03"),
            pair(TagField::Title, "Posledni soud")
        ])
    );
}

#[test]
fn format_dummy_skips() {
    assert_eq!(
        read(&format("%dummy%_%title%"), "CD1_Predmluva"),
        Some(vec![pair(TagField::Title, "Predmluva")])
    );
}

#[test]
fn format_last_placeholder_is_greedy() {
    assert_eq!(
        read(&format("%artist% - %title%"), "A - B - C"),
        Some(vec![pair(TagField::Artist, "A"), pair(TagField::Title, "B - C")])
    );
}

#[test]
fn format_literal_metachars() {
    assert_eq!(
        read(&format("(%year%) %album%"), "(2008) Labuti pisen"),
        Some(vec![
            pair(TagField::Date, "2008"),
            pair(TagField::Album, "Labuti pisen")
        ])
    );
}

#[test]
fn format_no_match_is_none() {
    assert_eq!(read(&format("%track% - %title%"), "Predmluva"), None);
}

#[test]
fn format_fields() {
    let all = "%title%|%artist%|%albumartist%|%album%|%track%|%disc%|%year%|%genre%|%performer%|%composer%|%publisher%|%comment%|%dummy%";
    let got = read(&format(all), "1|2|3|4|5|6|7|8|9|10|11|12|13").expect("matches");
    let fields: Vec<TagField> = got.iter().map(|(f, _)| *f).collect();
    assert_eq!(
        fields,
        vec![
            TagField::Title,
            TagField::Artist,
            TagField::AlbumArtist,
            TagField::Album,
            TagField::Track,
            TagField::Disc,
            TagField::Date,
            TagField::Genre,
            TagField::Performer,
            TagField::Composer,
            TagField::Publisher,
            TagField::Comment,
        ]
    );
    assert_eq!(got[6], pair(TagField::Date, "7"));
}

#[test]
fn format_placeholders_ignore_case() {
    assert_eq!(
        read(&format("%Track% - %TITLE%"), "01 - A"),
        Some(vec![pair(TagField::Track, "01"), pair(TagField::Title, "A")])
    );
}

#[test]
fn format_errors() {
    assert_eq!(
        compile_pattern(&format("%track% - %foo%")).unwrap_err(),
        PatternError::UnknownField { name: "foo".into() }
    );
    assert_eq!(
        compile_pattern(&format("no placeholders")).unwrap_err(),
        PatternError::NoFields
    );
}

#[test]
fn regex_named_groups() {
    assert_eq!(
        read(&regex(r"(?<track>\d+)_(?<title>.+)"), "07_Kapitola"),
        Some(vec![pair(TagField::Track, "07"), pair(TagField::Title, "Kapitola")])
    );
}

#[test]
fn regex_wire_names_and_aliases() {
    let p = regex(r"(?<albumArtist>[^-]+)-(?<year>\d+)-(?<date>\d+)-(?<dummy>x)");
    assert_eq!(
        read(&p, "A-2008-2009-x"),
        Some(vec![
            pair(TagField::AlbumArtist, "A"),
            pair(TagField::Date, "2008"),
            pair(TagField::Date, "2009"),
        ])
    );
}

#[test]
fn regex_errors() {
    assert_eq!(
        compile_pattern(&regex(r"(?<foo>.+)")).unwrap_err(),
        PatternError::UnknownField { name: "foo".into() }
    );
    assert_eq!(
        compile_pattern(&regex(r"(unclosed")).unwrap_err(),
        PatternError::BadRegex
    );
    assert_eq!(compile_pattern(&regex(r"\d+")).unwrap_err(), PatternError::NoFields);
}

#[test]
fn values_are_trimmed_and_empty_dropped() {
    assert_eq!(
        read(&regex(r"(?<track>\d*)\|(?<title>.*)"), "  |  Kapitola  "),
        Some(vec![pair(TagField::Title, "Kapitola")])
    );
}

#[test]
fn matching_runs_on_nfc() {
    // "é" decomposed (e + U+0301) still matches a precomposed literal.
    let got = read(&format("%track% é %title%"), "01 e\u{301} A");
    assert_eq!(got, Some(vec![pair(TagField::Track, "01"), pair(TagField::Title, "A")]));
}

fn tagged(title: &str) -> FileTags {
    let mut tags = FileTags::default();
    tags.fields.insert(TagField::Title, vec![title.to_string()]);
    tags
}

#[test]
fn from_file_name_leaves_unmatched_alone() {
    let files = [tagged("old"), tagged("keep"), tagged("short ctx")];
    let ctx = ToolContext {
        folders: vec![],
        stems: vec!["03 - New".to_string(), "nomatch".to_string()],
        session_files: 0,
    };
    let tool = TagTool::FromFileName {
        pattern: format("%track% - %title%"),
    };
    let out = apply_tool_in(&files, &ctx, &tool);
    assert_eq!(out[0].fields[&TagField::Title], vec!["New".to_string()]);
    assert_eq!(out[0].fields[&TagField::Track], vec!["03".to_string()]);
    assert_eq!(out[1], files[1]);
    assert_eq!(out[2], files[2]);
}

#[test]
fn from_file_name_bad_pattern_changes_nothing() {
    let files = [tagged("old")];
    let ctx = ToolContext {
        folders: vec![],
        stems: vec!["a".to_string()],
        session_files: 0,
    };
    let tool = TagTool::FromFileName { pattern: regex("(") };
    assert_eq!(apply_tool_in(&files, &ctx, &tool), files.to_vec());
}

#[test]
fn dummy_only_patterns_have_no_fields() {
    assert_eq!(compile_pattern(&format("%dummy%")).unwrap_err(), PatternError::NoFields);
    assert_eq!(
        compile_pattern(&format("%dummy%-%DUMMY%")).unwrap_err(),
        PatternError::NoFields
    );
    assert_eq!(
        compile_pattern(&regex(r"(?<dummy>.+)")).unwrap_err(),
        PatternError::NoFields
    );
    assert_eq!(
        compile_pattern(&regex(r"(.+)_(.+)")).unwrap_err(),
        PatternError::NoFields
    );
}

#[test]
fn pattern_literals_are_nfc_normalized() {
    // A pattern pasted from a macOS (NFD) file name: e + U+0301.
    let nfd_format = format("Kapitola e\u{301} - %title%");
    assert_eq!(
        read(&nfd_format, "Kapitola \u{e9} - A"),
        Some(vec![pair(TagField::Title, "A")])
    );
    let nfd_regex = regex("Kapitola e\u{301} - (?<title>.+)");
    assert_eq!(
        read(&nfd_regex, "Kapitola \u{e9} - A"),
        Some(vec![pair(TagField::Title, "A")])
    );
}

#[test]
fn double_percent_is_a_literal_percent() {
    assert_eq!(
        read(&format("50%% off %title%"), "50% off Sale"),
        Some(vec![pair(TagField::Title, "Sale")])
    );
    assert_eq!(read(&format("50%% off %title%"), "50 off Sale"), None);
}

#[test]
fn format_placeholders_cover_the_wire_only_fields() {
    let p = format("%translator%|%disctotal%|%description%|%www%");
    assert_eq!(
        read(&p, "a|2|c|d"),
        Some(vec![
            pair(TagField::Translator, "a"),
            pair(TagField::DiscTotal, "2"),
            pair(TagField::Description, "c"),
            pair(TagField::Www, "d"),
        ])
    );
}

#[test]
fn regex_group_names_cover_every_field() {
    let p = regex(r"(?<translator>a)\|(?<discTotal>2)\|(?<description>c)\|(?<www>d)\|(?<trackTotal>9)");
    assert_eq!(
        read(&p, "a|2|c|d|9"),
        Some(vec![
            pair(TagField::Translator, "a"),
            pair(TagField::DiscTotal, "2"),
            pair(TagField::Description, "c"),
            pair(TagField::Www, "d"),
            pair(TagField::TrackTotal, "9"),
        ])
    );
}

#[test]
fn from_file_name_duplicate_fields_follow_the_multi_rule() {
    let files = [FileTags::default()];
    let ctx = ToolContext {
        folders: vec![],
        stems: vec!["2008-2009".to_string(), "A - B".to_string()],
        session_files: 0,
    };
    let files = [files[0].clone(), files[0].clone()];
    // Single-value field (date, via the year alias): the first match wins.
    let dates = TagTool::FromFileName {
        pattern: regex(r"(?<date>\d+)-(?<year>\d+)"),
    };
    let out = apply_tool_in(&files, &ctx, &dates);
    assert_eq!(out[0].fields[&TagField::Date], vec!["2008".to_string()]);
    // Multi-value field (artist twice): the values append.
    let artists = TagTool::FromFileName {
        pattern: format("%artist% - %artist%"),
    };
    let out = apply_tool_in(&files, &ctx, &artists);
    assert_eq!(out[1].fields[&TagField::Artist], vec!["A".to_string(), "B".to_string()]);
}

#[test]
fn from_file_name_skips_track_on_a_one_file_book() {
    let files = [tagged("old")];
    let ctx = ToolContext {
        folders: vec![],
        stems: vec!["03 - New".to_string()],
        session_files: 1,
    };
    let tool_track = TagTool::FromFileName {
        pattern: format("%track% - %title%"),
    };
    let out = apply_tool_in(&files, &ctx, &tool_track);
    assert_eq!(out[0].fields[&TagField::Title], vec!["New".to_string()]);
    assert!(
        !out[0].fields.contains_key(&TagField::Track),
        "a one-file book has no track"
    );
    // A bigger book keeps it.
    let book = ToolContext {
        session_files: 4,
        ..ctx.clone()
    };
    let out = apply_tool_in(&files, &book, &tool_track);
    assert_eq!(out[0].fields[&TagField::Track], vec!["03".to_string()]);
    // TrackTotal is skipped too.
    let ctx2 = ToolContext {
        stems: vec!["03 of 09 - New".to_string()],
        ..ctx
    };
    let tool = TagTool::FromFileName {
        pattern: format("%track% of %tracktotal% - %title%"),
    };
    let out = apply_tool_in(&files, &ctx2, &tool);
    assert!(!out[0].fields.contains_key(&TagField::TrackTotal));
    assert!(!out[0].fields.contains_key(&TagField::Track));
}
