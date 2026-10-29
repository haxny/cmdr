//! The foobar2000 title-formatting engine against `foobar_vectors.json` (the user's own patterns
//! among them), plus the no-panic and nesting-limit guarantees.

use std::collections::HashMap;

use cmdr_media_meta::MediaFacts;

use super::foobar::{self, FoobarError, MAX_DEPTH, Unclosed};
use super::mask::{NumberWidth, RowFacts, RowMedia};

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Vectors {
    facts: HashMap<String, Row>,
    render: Vec<RenderCase>,
    errors: Vec<ErrorCase>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    file_name: String,
    parent: String,
    media: Option<Media>,
}

/// The `MediaFacts` fields a vector may set.
#[derive(serde::Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
struct Media {
    title: Option<String>,
    artist: Option<String>,
    album_artist: Option<String>,
    album: Option<String>,
    genre: Option<String>,
    comment: Option<String>,
    track: Option<i64>,
    disc: Option<i64>,
    year: Option<i64>,
    performer: Vec<String>,
    composer: Vec<String>,
    publisher: Option<String>,
    date: Option<String>,
    total_tracks: Option<i64>,
    total_discs: Option<i64>,
    description: Option<String>,
    translator: Vec<String>,
    www: Vec<String>,
}

impl From<Media> for MediaFacts {
    fn from(m: Media) -> Self {
        MediaFacts {
            title: m.title,
            artist: m.artist.into_iter().collect(),
            album_artist: m.album_artist.into_iter().collect(),
            album: m.album,
            genre: m.genre.into_iter().collect(),
            comment: m.comment,
            track: m.track,
            disc: m.disc,
            year: m.year,
            performer: m.performer,
            composer: m.composer,
            publisher: m.publisher,
            date: m.date,
            total_tracks: m.total_tracks,
            total_discs: m.total_discs,
            description: m.description,
            translator: m.translator,
            www: m.www,
            ..MediaFacts::default()
        }
    }
}

#[derive(serde::Deserialize)]
struct RenderCase {
    pattern: String,
    facts: String,
    expected: String,
}

#[derive(serde::Deserialize)]
struct ErrorCase {
    pattern: String,
    error: serde_json::Value,
}

fn vectors() -> Vectors {
    serde_json::from_str(include_str!("foobar_vectors.json")).expect("the vectors are JSON")
}

fn row<'a>(file_name: &'a str, parent: &'a str, media: RowMedia<'a>) -> RowFacts<'a> {
    RowFacts {
        file_name,
        is_directory: false,
        parent,
        grandparent: "",
        modified: None,
        position: 0,
        media,
        number_width: NumberWidth::default(),
    }
}

#[test]
fn every_render_vector_renders_exactly() {
    let Vectors { facts, render, .. } = vectors();
    let media: HashMap<&str, Option<MediaFacts>> = facts
        .iter()
        .map(|(name, row)| (name.as_str(), row.media.clone().map(MediaFacts::from)))
        .collect();
    let mut failures = Vec::new();
    for case in &render {
        let source = facts
            .get(&case.facts)
            .unwrap_or_else(|| panic!("no facts {}", case.facts));
        let tags = media[case.facts.as_str()].as_ref();
        let row = row(
            &source.file_name,
            &source.parent,
            tags.map_or(RowMedia::NotMedia, RowMedia::Read),
        );
        match foobar::parse(&case.pattern) {
            Ok(pattern) => {
                let got = pattern.render(&row);
                if got != case.expected {
                    failures.push(format!(
                        "{:?} on {}: got {got:?}, want {:?}",
                        case.pattern, case.facts, case.expected
                    ));
                }
            }
            Err(err) => failures.push(format!("{:?}: unexpected {err:?}", case.pattern)),
        }
    }
    assert!(
        failures.is_empty(),
        "{} failing:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn every_error_vector_names_the_problem_and_where() {
    let mut failures = Vec::new();
    for case in vectors().errors {
        match foobar::parse(&case.pattern) {
            Ok(_) => failures.push(format!("{:?} parsed", case.pattern)),
            Err(err) => {
                let got = serde_json::to_value(&err).expect("serializes");
                if got != case.error {
                    failures.push(format!("{:?}: got {got}, want {}", case.pattern, case.error));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failing:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn an_error_round_trips_over_ipc() {
    let err = FoobarError::Unclosed {
        what: Unclosed::Bracket,
        at: 4,
    };
    let json = serde_json::to_string(&err).expect("serializes");
    assert_eq!(json, r#"{"type":"unclosed","what":"bracket","at":4}"#);
    assert_eq!(serde_json::from_str::<FoobarError>(&json).expect("parses"), err);
}

#[test]
fn a_row_still_being_read_has_only_its_name() {
    let pattern = foobar::parse("[%artist% - ]%title%").expect("valid");
    assert_eq!(
        pattern.render(&row("01 Song.mp3", "Album", RowMedia::Reading)),
        "01 Song"
    );
    assert_eq!(
        pattern.render(&row("01 Song.mp3", "Album", RowMedia::Unreadable)),
        "01 Song"
    );
}

#[test]
fn uses_media_is_false_only_for_name_fields() {
    let uses = |p: &str| foobar::parse(p).expect("valid").uses_media();
    assert!(!uses("%filename% %directoryname% 'x' $upper(a)"));
    assert!(uses("%filename% %title%"));
    assert!(uses("$if(a,b,[%track%])"));
    assert!(uses("$upper(%album artist%)"));
}

#[test]
fn nesting_up_to_the_limit_parses_and_one_more_is_too_deep() {
    let brackets = |n: usize| format!("{}%artist%{}", "[".repeat(n), "]".repeat(n));
    let at_limit = foobar::parse(&brackets(MAX_DEPTH)).expect("at the limit");
    let facts = MediaFacts {
        artist: vec!["A".into()],
        ..MediaFacts::default()
    };
    assert_eq!(at_limit.render(&row("x.mp3", "p", RowMedia::Read(&facts))), "A");
    assert_eq!(
        foobar::parse(&brackets(MAX_DEPTH + 1)),
        Err(FoobarError::TooDeep { at: MAX_DEPTH })
    );

    let calls = |n: usize| format!("{}x{}", "$upper(".repeat(n), ")".repeat(n));
    assert!(foobar::parse(&calls(MAX_DEPTH)).is_ok());
    // `$upper(` is seven bytes: the 33rd call starts at 7 * 32.
    assert_eq!(
        foobar::parse(&calls(MAX_DEPTH + 1)),
        Err(FoobarError::TooDeep { at: 7 * MAX_DEPTH })
    );
}

#[test]
fn a_depth_bomb_is_refused_without_overflowing_the_stack() {
    for bomb in ["[".repeat(200_000), "$if(".repeat(200_000), "$if(a,[".repeat(100_000)] {
        assert!(matches!(foobar::parse(&bomb), Err(FoobarError::TooDeep { .. })));
    }
}

#[test]
fn a_blow_up_pattern_stays_bounded() {
    // Each $replace doubles the text; 32 levels would be 2^32 bytes without the cap.
    let mut pattern = String::from("aaaaaaaaaaaaaaaa");
    for _ in 0..(MAX_DEPTH - 1) {
        pattern = format!("$replace({pattern},a,aa)");
    }
    let out = foobar::parse(&pattern)
        .expect("valid")
        .render(&row("x", "p", RowMedia::NotMedia));
    assert!(out.len() <= foobar::MAX_TEXT_BYTES, "{} bytes", out.len());
    let wide = foobar::parse("$pad(x,999999999999)$num(1,-99999999999999999999)").expect("valid");
    assert!(wide.render(&row("x", "p", RowMedia::NotMedia)).len() <= 300);
}

/// xorshift64*: a fixed seed, so a failure reproduces.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

#[test]
fn ten_thousand_random_patterns_never_panic() {
    const PIECES: &[&str] = &[
        "%",
        "[",
        "]",
        "'",
        "$",
        "(",
        ")",
        ",",
        "%artist%",
        "%album artist%",
        "%track%",
        "%title%",
        "%date%",
        "%year%",
        "$num(",
        "$if(",
        "$if2(",
        "$if3(",
        "$left(",
        "$right(",
        "$replace(",
        "$lower(",
        "$upper(",
        "$caps(",
        "$trim(",
        "$pad(",
        "$year(",
        "-",
        "9",
        "99999999999999999999",
        "a",
        " ",
        "Č",
        "🎵",
        "''",
        "\u{301}",
    ];
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let facts = MediaFacts {
        artist: vec!["Ärtist".into()],
        track: Some(-3),
        date: Some("20".into()),
        ..MediaFacts::default()
    };
    for _ in 0..10_000 {
        let len = (rng.next() % 40) as usize;
        let pattern: String = (0..len)
            .map(|_| PIECES[(rng.next() % PIECES.len() as u64) as usize])
            .collect();
        if let Ok(parsed) = foobar::parse(&pattern) {
            let _ = parsed.render(&row("ž.mp3", "p", RowMedia::Read(&facts)));
            let _ = parsed.uses_media();
        }
        // Raw random bytes too, as UTF-8 (lossy).
        let bytes: Vec<u8> = (0..len).map(|_| (rng.next() & 0xFF) as u8).collect();
        let _ = foobar::parse(&String::from_utf8_lossy(&bytes));
    }
}
