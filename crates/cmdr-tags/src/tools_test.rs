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
        assert_eq!(one(tags, TagField::Track), Some(format!("{:02}", i + 1)));
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
fn narrator_handles_hyphens_initials_and_newlines() {
    let out = narrator_from_text(&[
        file(&[(TagField::Comment, "Čte: Jiří Dvořák-Ondříček.")]),
        file(&[(TagField::Comment, "Čte: J. Novák")]),
        file(&[(TagField::Comment, "Čte: Jan Novák\nPovídky o psech")]),
    ]);
    assert_eq!(
        get(&out[0], TagField::Performer),
        Some(vec!["Jiri Dvorak-Ondricek".into()])
    );
    assert!(!out[0].fields.contains_key(&TagField::Comment));
    assert_eq!(get(&out[1], TagField::Performer), Some(vec!["J. Novak".into()]));
    assert_eq!(get(&out[2], TagField::Performer), Some(vec!["Jan Novak".into()]));
    assert_eq!(one(&out[2], TagField::Comment).as_deref(), Some("Povídky o psech"));
}

#[test]
fn narrator_splits_several_names() {
    let out = narrator_from_text(&[
        file(&[(TagField::Comment, "čtou: Jan Novák a Eva Nováková.")]),
        file(&[(TagField::Comment, "Čte: Jan Novák, Eva Nováková and Petr Svoboda")]),
    ]);
    assert_eq!(
        get(&out[0], TagField::Performer),
        Some(vec!["Jan Novak".into(), "Eva Novakova".into()])
    );
    assert!(!out[0].fields.contains_key(&TagField::Comment));
    assert_eq!(get(&out[1], TagField::Performer).map(|v| v.len()), Some(3));
}

#[test]
fn narrator_regressions() {
    for text in ["interpretace: Jan Novák", "přečte: Jan Novák"] {
        let input = [file(&[(TagField::Comment, text)])];
        assert_eq!(narrator_from_text(&input), input.to_vec(), "{text}");
    }
    let mid = narrator_from_text(&[file(&[(TagField::Comment, "Povídky. Čte: Jan Novák. Konec")])]);
    assert_eq!(get(&mid[0], TagField::Performer), Some(vec!["Jan Novak".into()]));
    assert_eq!(one(&mid[0], TagField::Comment).as_deref(), Some("Povídky. Konec"));
    let comma = narrator_from_text(&[file(&[(TagField::Comment, "Čte: Jan Novák, povídky")])]);
    assert_eq!(one(&comma[0], TagField::Comment).as_deref(), Some("povídky"));
    let cap = narrator_from_text(&[file(&[(TagField::Comment, "Čte: Aa Bb Cc Dd Ee")])]);
    assert_eq!(get(&cap[0], TagField::Performer), Some(vec!["Aa Bb Cc Dd".into()]));
    assert_eq!(one(&cap[0], TagField::Comment).as_deref(), Some("Ee"));
    let ellipsis = narrator_from_text(&[file(&[(TagField::Comment, "Čte: Jan Novák... a dál")])]);
    assert_eq!(one(&ellipsis[0], TagField::Comment).as_deref(), Some("... a dál"));
}

#[test]
fn only_the_first_value_is_rewritten() {
    let mut multi = file(&[(TagField::Comment, "Čte: Jan Novák."), (TagField::Comment, "druhy")]);
    multi
        .fields
        .entry(TagField::Title)
        .or_default()
        .extend(["01 Prvni".to_string(), "Druhy".to_string()]);
    multi.fields.insert(TagField::Track, vec!["1".into()]);
    let out = narrator_from_text(&[multi.clone()]);
    assert_eq!(get(&out[0], TagField::Comment), Some(vec!["druhy".into()]));
    let out = strip_track_prefix(&[multi]);
    assert_eq!(
        get(&out[0], TagField::Title),
        Some(vec!["Prvni".into(), "Druhy".into()])
    );
}

#[test]
fn track_prefix_does_not_overflow() {
    let input = [file(&[(TagField::Title, "4294967295 Konec"), (TagField::Track, "1")])];
    assert_eq!(strip_track_prefix(&input), input.to_vec());
}

#[test]
fn multi_value_case_and_diacritics() {
    let input = [file(&[
        (TagField::Genre, "ŽÁNR JEDEN"),
        (TagField::Genre, "druhý žánr"),
    ])];
    let sentence = change_case(&input, &[TagField::Genre], CaseTool::Sentence);
    assert_eq!(
        get(&sentence[0], TagField::Genre),
        Some(vec!["Žánr jeden".into(), "Druhý žánr".into()])
    );
    let title = change_case(&input, &[TagField::Genre], CaseTool::Title);
    assert_eq!(
        get(&title[0], TagField::Genre),
        Some(vec!["Žánr Jeden".into(), "Druhý Žánr".into()])
    );
    let stripped = strip_diacritics(&input, &[TagField::Genre]);
    assert_eq!(
        get(&stripped[0], TagField::Genre),
        Some(vec!["ZANR JEDEN".into(), "druhy zanr".into()])
    );
}

#[test]
fn tools_return_changed_copies_and_leave_the_input_unchanged() {
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
    let keys = ["ITUNESMEDIATYPE".to_string()];
    let outputs = [
        change_case(&input, &[TagField::Artist], CaseTool::Lower),
        strip_diacritics(&input, &[TagField::Comment, TagField::Artist, TagField::Title]),
        strip_track_prefix(&input),
        renumber(&input, 5),
        album_artist_from_artist(&input),
        narrator_from_text(&input),
        remove_extras(&input, &keys),
    ];
    // strip_diacritics changes nothing here (no diacritics in the stripped fields); the rest do.
    for (i, out) in outputs.iter().enumerate() {
        if i == 1 {
            assert_eq!(out, &snapshot);
        } else {
            assert_ne!(out, &snapshot, "tool {i} should change something");
        }
    }
    // The title equals the album once case is ignored, so it is cleared.
    assert_ne!(clear_redundant_titles(&input), snapshot);
    assert_eq!(input, snapshot);
}

#[test]
fn later_narrators_need_two_words() {
    let out = narrator_from_text(&[
        file(&[(TagField::Comment, "Čte: Jan Novák, Povídky o lásce")]),
        file(&[(TagField::Comment, "Čte: Jan Novák a J. Dvořák")]),
    ]);
    assert_eq!(get(&out[0], TagField::Performer), Some(vec!["Jan Novak".into()]));
    assert_eq!(one(&out[0], TagField::Comment).as_deref(), Some("Povídky o lásce"));
    assert_eq!(
        get(&out[1], TagField::Performer),
        Some(vec!["Jan Novak".into(), "J. Dvorak".into()])
    );
}

#[test]
fn apply_tool_dispatches_each_variant() {
    let mut tags = file(&[
        (TagField::Title, "01 JEFFREY"),
        (TagField::Track, "1"),
        (TagField::Artist, "ARCHER Čte"),
        (TagField::Album, "01 jeffrey"),
        (TagField::Comment, "Čte: Jan Novák."),
    ]);
    tags.extras.insert("ITUNESMEDIATYPE".into(), vec!["2".into()]);
    let input = vec![tags, file(&[(TagField::Track, "9")])];
    let fields = vec![TagField::Artist];
    let keys = vec!["ITUNESMEDIATYPE".to_string()];
    let cases = [
        (
            TagTool::Case {
                fields: fields.clone(),
                case: CaseTool::Lower,
            },
            change_case(&input, &fields, CaseTool::Lower),
        ),
        (
            TagTool::StripDiacritics { fields: fields.clone() },
            strip_diacritics(&input, &fields),
        ),
        (TagTool::StripTrackPrefix, strip_track_prefix(&input)),
        (TagTool::Renumber { start: 5 }, renumber(&input, 5)),
        (TagTool::AlbumArtistFromArtist, album_artist_from_artist(&input)),
        (TagTool::NarratorFromText, narrator_from_text(&input)),
        (TagTool::ClearRedundantTitles, clear_redundant_titles(&input)),
        (
            TagTool::RemoveExtras { keys: keys.clone() },
            remove_extras(&input, &keys),
        ),
    ];
    for (tool, expected) in &cases {
        assert_eq!(&apply_tool(&input, tool), expected, "{tool:?}");
    }
}

#[test]
fn renumber_single_file_removes_track_and_total() {
    let out = renumber(
        &[file(&[
            (TagField::Track, "3"),
            (TagField::TrackTotal, "12"),
            (TagField::Disc, "2"),
            (TagField::DiscTotal, "3"),
        ])],
        1,
    );
    assert_eq!(get(&out[0], TagField::Track), None);
    assert_eq!(get(&out[0], TagField::TrackTotal), None);
    assert_eq!(one(&out[0], TagField::Disc).as_deref(), Some("2"));
    assert_eq!(one(&out[0], TagField::DiscTotal).as_deref(), Some("3"));
}

#[test]
fn renumber_pads_to_two() {
    let input = vec![FileTags::default(); 3];
    let out = renumber(&input, 1);
    let tracks: Vec<Option<String>> = out.iter().map(|t| one(t, TagField::Track)).collect();
    assert_eq!(tracks, [Some("01".into()), Some("02".into()), Some("03".into())]);
}

#[test]
fn renumber_pads_to_three_past_99() {
    let input = vec![FileTags::default(); 120];
    let out = renumber(&input, 1);
    assert_eq!(one(&out[0], TagField::Track).as_deref(), Some("001"));
    assert_eq!(one(&out[98], TagField::Track).as_deref(), Some("099"));
    assert_eq!(one(&out[119], TagField::Track).as_deref(), Some("120"));
}

#[test]
fn renumber_width_follows_the_last_number_not_the_count() {
    let out = renumber(&vec![FileTags::default(); 3], 98);
    let tracks: Vec<Option<String>> = out.iter().map(|t| one(t, TagField::Track)).collect();
    assert_eq!(tracks, [Some("098".into()), Some("099".into()), Some("100".into())]);
}

#[test]
fn pad_number_and_track_width() {
    assert_eq!(track_width(0), 2);
    assert_eq!(track_width(9), 2);
    assert_eq!(track_width(99), 2);
    assert_eq!(track_width(100), 3);
    assert_eq!(track_width(1000), 4);
    assert_eq!(pad_number(3, 2), "03");
    assert_eq!(pad_number(3, 3), "003");
    assert_eq!(pad_number(120, 2), "120");
}

#[test]
fn fields_with_diacritics_skips_comment_and_description() {
    let tags = file(&[
        (TagField::Album, "Labutí"),
        (TagField::Artist, "Archer"),
        (TagField::Comment, "Čte"),
        (TagField::Description, "Příběh"),
    ]);
    assert_eq!(fields_with_diacritics(&tags), vec![TagField::Album]);
}

#[test]
fn tag_tool_json_shape() {
    let json = serde_json::to_string(&TagTool::Renumber { start: 1 }).unwrap();
    assert_eq!(json, r#"{"kind":"renumber","start":1}"#);
    let back: TagTool = serde_json::from_str(r#"{"kind":"stripDiacritics","fields":["album"]}"#).unwrap();
    assert_eq!(
        back,
        TagTool::StripDiacritics {
            fields: vec![TagField::Album]
        }
    );
    let unit: TagTool = serde_json::from_str(r#"{"kind":"clearRedundantTitles"}"#).unwrap();
    assert_eq!(unit, TagTool::ClearRedundantTitles);
}

#[test]
fn tag_tool_case_json_shape() {
    let tool = TagTool::Case {
        fields: vec![TagField::Title, TagField::Album],
        case: CaseTool::Sentence,
    };
    let json = serde_json::to_string(&tool).unwrap();
    assert_eq!(json, r#"{"kind":"case","fields":["title","album"],"case":"sentence"}"#);
    assert_eq!(serde_json::from_str::<TagTool>(&json).unwrap(), tool);
}

#[test]
fn apply_tool_renumber_on_a_single_file_removes_its_track() {
    let input = [file(&[(TagField::Track, "7"), (TagField::Album, "Book")])];
    let out = apply_tool(&input, &TagTool::Renumber { start: 1 });
    assert_eq!(out.len(), 1);
    assert_eq!(get(&out[0], TagField::Track), None);
    assert_eq!(one(&out[0], TagField::Album), Some("Book".to_string()));
}

#[test]
fn renumber_one_file_of_a_bigger_book_keeps_a_track() {
    let input = [file(&[(TagField::Track, "7"), (TagField::TrackTotal, "9")])];
    let ctx = ToolContext {
        session_files: 5,
        ..ToolContext::default()
    };
    let out = apply_tool_in(&input, &ctx, &TagTool::Renumber { start: 1 });
    assert_eq!(one(&out[0], TagField::Track), Some("01".to_string()));
    let alone = ToolContext {
        session_files: 1,
        ..ToolContext::default()
    };
    let out = apply_tool_in(&input, &alone, &TagTool::Renumber { start: 1 });
    assert_eq!(get(&out[0], TagField::Track), None, "a one-file book has no track");
}
