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
    };
    let tool = TagTool::FromFileName { pattern: regex("(") };
    assert_eq!(apply_tool_in(&files, &ctx, &tool), files.to_vec());
}
