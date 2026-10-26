//! Tests for the clean-up tools.

use super::model::{FileTags, TagField};
use super::tools::*;

fn file(pairs: &[(TagField, &str)]) -> FileTags {
    let mut tags = FileTags::default();
    for (field, value) in pairs {
        tags.fields.entry(*field).or_default().push((*value).to_string());
    }
    tags
}

fn get(tags: &FileTags, field: TagField) -> Option<Vec<String>> {
    tags.fields.get(&field).cloned()
}

fn one(tags: &FileTags, field: TagField) -> Option<String> {
    get(tags, field).and_then(|v| v.into_iter().next())
}

#[test]
fn change_case_title_on_archer() {
    let out = change_case(
        &[file(&[(TagField::Artist, "JEFFREY ARCHER")])],
        &[TagField::Artist],
        CaseTool::Title,
    );
    assert_eq!(one(&out[0], TagField::Artist).as_deref(), Some("Jeffrey Archer"));
}

#[test]
fn change_case_sentence_lower_upper_and_multi_values() {
    let input = [file(&[
        (TagField::Title, "Laik Na Kolobezce"),
        (TagField::Genre, "ROMAN"),
        (TagField::Genre, "SCI-FI"),
    ])];
    let sentence = change_case(&input, &[TagField::Title], CaseTool::Sentence);
    assert_eq!(one(&sentence[0], TagField::Title).as_deref(), Some("Laik na kolobezce"));
    let lower = change_case(&input, &[TagField::Genre], CaseTool::Lower);
    assert_eq!(
        get(&lower[0], TagField::Genre),
        Some(vec!["roman".into(), "sci-fi".into()])
    );
    let upper = change_case(&input, &[TagField::Title], CaseTool::Upper);
    assert_eq!(one(&upper[0], TagField::Title).as_deref(), Some("LAIK NA KOLOBEZCE"));
}

#[test]
fn change_case_sentence_handles_roman_numerals_and_czech_capitals() {
    let input = [
        file(&[(TagField::Title, "KARLA IV.")]),
        file(&[(TagField::Title, "ŽLUŤOUČKÝ")]),
    ];
    let out = change_case(&input, &[TagField::Title], CaseTool::Sentence);
    assert_eq!(one(&out[0], TagField::Title).as_deref(), Some("Karla iv."));
    assert_eq!(one(&out[1], TagField::Title).as_deref(), Some("Žluťoučký"));
}

#[test]
fn strip_diacritics_skips_comment_and_strips_album() {
    let input = [file(&[(TagField::Album, "Povídky"), (TagField::Comment, "Povídky")])];
    let out = strip_diacritics(&input, &[TagField::Album, TagField::Comment]);
    assert_eq!(one(&out[0], TagField::Album).as_deref(), Some("Povidky"));
    assert_eq!(one(&out[0], TagField::Comment).as_deref(), Some("Povídky"));
}

#[test]
fn strip_track_prefix_cases() {
    let input = [
        file(&[(TagField::Title, "01 Posledni soud"), (TagField::Track, "1")]),
        file(&[(TagField::Title, "00_Predmluva"), (TagField::Track, "1")]),
        file(&[(TagField::Title, "12 opic"), (TagField::Track, "3")]),
        file(&[(TagField::Title, "01 Bez cisla")]),
        file(&[(TagField::Title, "01 Spatne"), (TagField::Track, "abc")]),
        file(&[(TagField::Title, "02_Dve_slova"), (TagField::Track, "2/10")]),
    ];
    let out = strip_track_prefix(&input);
    assert_eq!(one(&out[0], TagField::Title).as_deref(), Some("Posledni soud"));
    assert_eq!(one(&out[1], TagField::Title).as_deref(), Some("Predmluva"));
    assert_eq!(out[2], input[2]);
    assert_eq!(out[3], input[3]);
    assert_eq!(out[4], input[4]);
    assert_eq!(one(&out[5], TagField::Title).as_deref(), Some("Dve slova"));
}

#[test]
fn renumber_numbers_from_one_and_leaves_disc() {
    let input: Vec<FileTags> = (0..40)
        .map(|i| file(&[(TagField::Track, &i.to_string()), (TagField::Disc, "2")]))
        .collect();
    let out = renumber(&input, 1);
    for (i, tags) in out.iter().enumerate() {
        assert_eq!(one(tags, TagField::Track), Some((i + 1).to_string()));
        assert_eq!(one(tags, TagField::Disc).as_deref(), Some("2"));
    }
    assert_eq!(one(&out[39], TagField::Track).as_deref(), Some("40"));
}

#[test]
fn album_artist_copies_artist_only_when_absent() {
    let input = [
        file(&[(TagField::Artist, "Archer")]),
        file(&[(TagField::Artist, "Archer"), (TagField::AlbumArtist, "Jiny")]),
        file(&[(TagField::Title, "Bez autora")]),
    ];
    let out = album_artist_from_artist(&input);
    assert_eq!(one(&out[0], TagField::AlbumArtist).as_deref(), Some("Archer"));
    assert_eq!(one(&out[1], TagField::AlbumArtist).as_deref(), Some("Jiny"));
    assert_eq!(out[2], input[2]);
}

#[test]
fn narrator_moves_from_comment_to_performer() {
    let input = [file(&[(TagField::Comment, "Čte: Tomáš Jirman. Povídky…")])];
    let out = narrator_from_text(&input);
    assert_eq!(get(&out[0], TagField::Performer), Some(vec!["Tomas Jirman".into()]));
    assert_eq!(one(&out[0], TagField::Comment).as_deref(), Some("Povídky…"));
}

#[test]
fn narrator_accepts_czech_capitals_and_falls_back_to_title() {
    let input = [file(&[(TagField::Title, "Kapitola 1 vypráví: Řehoř Šťastný")])];
    let out = narrator_from_text(&input);
    assert_eq!(get(&out[0], TagField::Performer), Some(vec!["Rehor Stastny".into()]));
    assert_eq!(one(&out[0], TagField::Title).as_deref(), Some("Kapitola 1"));
}

#[test]
fn narrator_removes_an_emptied_source_field() {
    let out = narrator_from_text(&[file(&[(TagField::Comment, "Čte: Tomáš Jirman.")])]);
    assert_eq!(get(&out[0], TagField::Performer), Some(vec!["Tomas Jirman".into()]));
    assert!(!out[0].fields.contains_key(&TagField::Comment));
}

#[test]
fn narrator_ignores_no_colon_lowercase_names_and_existing_performer() {
    let plain = [file(&[(TagField::Title, "Čtenářský deník")])];
    assert_eq!(narrator_from_text(&plain), plain.to_vec());
    let lower = [file(&[(TagField::Comment, "čte: tomáš jirman")])];
    assert_eq!(narrator_from_text(&lower), lower.to_vec());
    let set = [file(&[
        (TagField::Comment, "Čte: Tomáš Jirman. Povídky"),
        (TagField::Performer, "Jiny Čtenář"),
    ])];
    assert_eq!(narrator_from_text(&set), set.to_vec());
}

#[test]
fn clear_redundant_titles_ignores_case_and_diacritics() {
    let input = [
        file(&[(TagField::Title, "POVÍDKY"), (TagField::Album, "Povidky")]),
        file(&[(TagField::Title, "Kapitola"), (TagField::Album, "Povidky")]),
    ];
    let out = clear_redundant_titles(&input);
    assert!(!out[0].fields.contains_key(&TagField::Title));
    assert_eq!(out[1], input[1]);
}

#[test]
fn remove_extras_drops_only_the_named_keys() {
    let mut tags = file(&[(TagField::Title, "x")]);
    tags.extras.insert("ITUNESMEDIATYPE".into(), vec!["2".into()]);
    tags.extras.insert("OTHER".into(), vec!["y".into()]);
    let out = remove_extras(&[tags], &["ITUNESMEDIATYPE".to_string()]);
    assert!(!out[0].extras.contains_key("ITUNESMEDIATYPE"));
    assert!(out[0].extras.contains_key("OTHER"));
}

#[test]
fn tools_return_new_values_and_leave_the_input_unchanged() {
    let mut tags = file(&[
        (TagField::Title, "01 JEFFREY"),
        (TagField::Track, "1"),
        (TagField::Artist, "ARCHER"),
        (TagField::Album, "01 jeffrey"),
        (TagField::Comment, "Čte: Jan Novák."),
    ]);
    tags.extras.insert("ITUNESMEDIATYPE".into(), vec!["2".into()]);
    let input = vec![tags];
    let snapshot = input.clone();
    let _ = change_case(&input, &[TagField::Artist], CaseTool::Lower);
    let _ = strip_diacritics(&input, &[TagField::Artist]);
    let _ = strip_track_prefix(&input);
    let _ = renumber(&input, 5);
    let _ = album_artist_from_artist(&input);
    let _ = narrator_from_text(&input);
    let _ = clear_redundant_titles(&input);
    let _ = remove_extras(&input, &["ITUNESMEDIATYPE".to_string()]);
    assert_eq!(input, snapshot);
}
