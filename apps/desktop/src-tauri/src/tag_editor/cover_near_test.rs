//! Covers that look the same: re-encoded or resized copies of one picture (different bytes) are
//! one choice, the largest standing for them; visibly different pictures never merge, not even
//! solid colours, one template with another title or photo, or one template in another colour.
//! Every picture is generated here.

use cmdr_tags::{ContentKind, Cover, FileTags, TagField, WriteOptions, write_tags};
use image::{DynamicImage, ImageFormat, Rgb, RgbImage, imageops::FilterType};

use super::cover_choices::{CoverChoice, choices_of, covers_of, list, seeds_of, use_for_all};
use super::cover_image::{likeness_decodes, likeness_of};
use super::cover_likeness::Likeness;
use super::history::undo_edit;
use super::session::{Entry, Loaded, changes, close, open};
use super::session_test::{album, tags};
use super::view::{CoverSummary, cover_key};
use crate::multi_rename::session::close as close_rename;

/// A "book cover": a smooth diagonal gradient with a title band and a disc, at `side` px.
fn artwork(side: u32) -> DynamicImage {
    let s = side as f32;
    DynamicImage::ImageRgb8(RgbImage::from_fn(side, side, |x, y| {
        let (fx, fy) = (x as f32 / s, y as f32 / s);
        let in_band = (0.15..0.3).contains(&fy);
        let in_disc = (fx - 0.6).powi(2) + (fy - 0.65).powi(2) < 0.04;
        if in_band {
            Rgb([240, 230, 200])
        } else if in_disc {
            Rgb([180, 40, 30])
        } else {
            let v = (255.0 * (fx * 0.6 + fy * 0.4)) as u8;
            Rgb([v / 3, v / 2, v])
        }
    }))
}

/// Another picture altogether: a vertical stripe pattern with a dark corner.
fn other_artwork(side: u32) -> DynamicImage {
    DynamicImage::ImageRgb8(RgbImage::from_fn(side, side, |x, y| {
        if x < side / 3 && y > side / 2 {
            Rgb([20, 20, 20])
        } else if (x * 8 / side).is_multiple_of(2) {
            Rgb([30, 160, 60])
        } else {
            Rgb([230, 230, 90])
        }
    }))
}

/// The same layout as [`artwork`], mirrored: the same colors, a different picture.
fn mirrored_artwork(side: u32) -> DynamicImage {
    artwork(side).fliph()
}

fn solid(colour: [u8; 3]) -> DynamicImage {
    DynamicImage::ImageRgb8(RgbImage::from_pixel(500, 500, Rgb(colour)))
}

/// A glyph of `c`: a 3×5 block pattern taken from its code, so two titles differ where letters do.
fn glyph(c: char, col: u32, row: u32) -> bool {
    let code = u32::from(c).wrapping_mul(2_654_435_761) >> 7;
    c != ' ' && (code >> (row * 3 + col)) & 1 == 1
}

/// A series template: a tinted gradient, a light title band with `title` in dark blocks, and a
/// "photo" (`photo` picks its pattern) below it.
fn template(side: u32, title: &str, tint: [f32; 3], photo: u32) -> DynamicImage {
    let s = side as f32;
    let letters: Vec<char> = title.chars().collect();
    let glyph_width = 0.8 / letters.len() as f32;
    DynamicImage::ImageRgb8(RgbImage::from_fn(side, side, |x, y| {
        let (fx, fy) = (x as f32 / s, y as f32 / s);
        if (0.08..0.28).contains(&fy) {
            if (0.1..0.9).contains(&fx) {
                let at = (fx - 0.1) / glyph_width;
                let letter = letters[(at as usize).min(letters.len() - 1)];
                let col = (at.fract() * 4.0) as u32;
                let row = ((fy - 0.08) / 0.2 * 5.0) as u32;
                if col < 3 && glyph(letter, col, row.min(4)) {
                    return Rgb([30, 25, 20]);
                }
            }
            return Rgb([240, 230, 200]);
        }
        if (0.35..0.95).contains(&fy) && (0.1..0.9).contains(&fx) {
            let wave = match photo {
                0 => ((fx * 9.0).sin() * (fy * 7.0).cos() + 1.0) / 2.0,
                _ => (((fx - 0.5).powi(2) + (fy - 0.65).powi(2)).sqrt() * 30.0).sin().abs(),
            };
            let v = (40.0 + 180.0 * wave) as u8;
            return Rgb([v, v / 2 + 40, 255 - v]);
        }
        let v = 255.0 * (fx * 0.6 + fy * 0.4);
        Rgb([(v * tint[0]) as u8, (v * tint[1]) as u8, (v * tint[2]) as u8])
    }))
}

/// A badge about 5 % of the cover, in its lower-left corner.
const BADGE: (f32, f32, f32, f32) = (0.05, 0.33, 0.78, 0.96);
/// A small corner label, under 1 % of the cover (66×42 px of 600): it merged on a 32×32 grey grid.
const SMALL_BADGE: (f32, f32, f32, f32) = (0.05, 0.16, 0.89, 0.96);

/// [`artwork`] with a light badge at `left, right, top, bottom` (fractions of the side) holding
/// `label` in dark blocks, as ripped audiobooks mark each disc's copy of one artwork ("CD 1").
fn badged(side: u32, label: &str, (left, right, top, bottom): (f32, f32, f32, f32)) -> DynamicImage {
    let s = side as f32;
    let letters: Vec<char> = label.chars().collect();
    let glyph_width = (right - left) / letters.len() as f32;
    let mut picture = artwork(side).to_rgb8();
    for (x, y, pixel) in picture.enumerate_pixels_mut() {
        let (fx, fy) = (x as f32 / s, y as f32 / s);
        if (left..right).contains(&fx) && (top..bottom).contains(&fy) {
            let at = (fx - left) / glyph_width;
            let letter = letters[(at as usize).min(letters.len() - 1)];
            let col = (at.fract() * 4.0) as u32;
            let row = ((fy - top) / (bottom - top) * 5.0) as u32;
            *pixel = if col < 3 && glyph(letter, col, row.min(4)) {
                Rgb([30, 25, 20])
            } else {
                Rgb([245, 245, 240])
            };
        }
    }
    DynamicImage::ImageRgb8(picture)
}

const BLUE: [f32; 3] = [0.3, 0.5, 1.0];
const GREEN: [f32; 3] = [0.3, 1.0, 0.5];

fn jpeg(picture: &DynamicImage, quality: u8) -> Cover {
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality)
        .encode_image(&DynamicImage::ImageRgb8(picture.to_rgb8()))
        .expect("encodes");
    Cover {
        mime: "image/jpeg".to_string(),
        data: out.into(),
    }
}

fn png(picture: &DynamicImage) -> Cover {
    let mut out = std::io::Cursor::new(Vec::new());
    picture.write_to(&mut out, ImageFormat::Png).expect("encodes");
    Cover {
        mime: "image/png".to_string(),
        data: out.into_inner().into(),
    }
}

fn resized(picture: &DynamicImage, side: u32) -> DynamicImage {
    picture.resize_exact(side, side, FilterType::Lanczos3)
}

fn key_text(cover: &Cover) -> String {
    format!("{:016x}", cover_key(cover))
}

fn look(cover: &Cover) -> std::sync::Arc<Likeness> {
    likeness_of(cover, &key_text(cover)).expect("decodes")
}

fn apart(a: &Cover, b: &Cover) -> Option<u32> {
    look(a).distance(&look(b))
}

fn loaded(cover: Option<Cover>) -> Entry {
    let tags = FileTags {
        cover,
        ..FileTags::default()
    };
    Entry::Loaded(Box::new(Loaded {
        cover_key: tags.cover.as_ref().map(cover_key),
        disk: tags.clone(),
        edited: tags,
        duration_ms: None,
        fingerprint: None,
    }))
}

fn choices(covers: &[Option<Cover>]) -> Vec<CoverChoice> {
    let entries: Vec<Entry> = covers.iter().cloned().map(loaded).collect();
    let files: Vec<&Loaded> = entries.iter().filter_map(Entry::loaded).collect();
    choices_of(&covers_of(&files))
}

/// Copies of one picture: the encoder, its quality, the size, and the format change; the picture
/// doesn't.
fn copies_of(master: &DynamicImage) -> Vec<Cover> {
    vec![
        jpeg(master, 90),
        jpeg(master, 70),
        jpeg(&resized(master, 500), 85),
        png(&resized(master, 550)),
    ]
}

#[test]
fn re_encoded_and_resized_copies_look_the_same() {
    for master in [
        artwork(600),
        other_artwork(600),
        template(600, "Book One", BLUE, 0),
        badged(600, "CD 1", BADGE),
        badged(600, "CD 1", SMALL_BADGE),
    ] {
        let copies = copies_of(&master);
        for copy in &copies[1..] {
            assert!(
                apart(&copies[0], copy).is_some(),
                "a copy looks different: {:?}",
                look(&copies[0]).spread(&look(copy))
            );
        }
    }
}

#[test]
fn different_pictures_never_look_the_same() {
    let book = |title, tint, photo| jpeg(&template(600, title, tint, photo), 90);
    let pairs = [
        (
            "another picture",
            jpeg(&artwork(600), 90),
            jpeg(&other_artwork(600), 90),
        ),
        ("mirrored", jpeg(&artwork(600), 90), jpeg(&mirrored_artwork(600), 90)),
        (
            "two solid colours",
            jpeg(&solid([200, 30, 30]), 90),
            jpeg(&solid([30, 30, 200]), 90),
        ),
        (
            "two dark solids",
            jpeg(&solid([12, 12, 12]), 90),
            jpeg(&solid([40, 30, 30]), 90),
        ),
        ("another title", book("Book One", BLUE, 0), book("Book Two", BLUE, 0)),
        ("another photo", book("Book One", BLUE, 0), book("Book One", BLUE, 1)),
        ("another colour", book("Book One", BLUE, 0), book("Book One", GREEN, 0)),
        // One artwork per disc, told apart only by a small badge: merged, "use for all" would
        // give every disc the first one's badge.
        (
            "another disc badge",
            jpeg(&badged(600, "CD 1", BADGE), 90),
            jpeg(&badged(600, "CD 2", BADGE), 90),
        ),
        (
            "another small disc label",
            jpeg(&badged(600, "CD 1", SMALL_BADGE), 90),
            jpeg(&badged(600, "CD 2", SMALL_BADGE), 90),
        ),
        (
            "another small disc label, resized",
            jpeg(&badged(600, "CD 1", SMALL_BADGE), 90),
            jpeg(&resized(&badged(600, "CD 2", SMALL_BADGE), 500), 80),
        ),
        (
            "a badge or none",
            jpeg(&artwork(600), 90),
            jpeg(&badged(600, "CD 1", BADGE), 90),
        ),
    ];
    for (what, a, b) in &pairs {
        assert_eq!(apart(a, b), None, "{what}: {:?}", look(a).spread(&look(b)));
    }
}

/// A solid or smooth cover has no detail to tell copies by, so it never merges, even with itself
/// re-encoded.
#[test]
fn a_flat_cover_never_merges() {
    let red = solid([200, 30, 30]);
    assert!(!look(&jpeg(&red, 90)).informative());
    assert_eq!(apart(&jpeg(&red, 90), &jpeg(&red, 70)), None);
    assert!(look(&jpeg(&artwork(300), 90)).informative());
}

/// Each item joins the NEAREST seed, and only a seed: A like B and B like C with A unlike C keeps
/// C apart from A's group (no chaining), whatever the order.
#[test]
fn likeness_joins_the_nearest_seed_and_never_chains() {
    let within_five = |a: &i32, b: &i32| (a.abs_diff(*b) <= 5).then_some(a.abs_diff(*b));
    // 4 is like 0 and like 8, 8 is unlike 0: 8 starts its own group.
    assert_eq!(seeds_of(&[Some(0), Some(4), Some(8)], within_five), vec![0, 0, 2]);
    // 5 is within reach of both seeds 0 and 9; 9 is nearer.
    assert_eq!(seeds_of(&[Some(0), Some(9), Some(5)], within_five), vec![0, 1, 1]);
    // No likeness: alone.
    assert_eq!(seeds_of(&[Some(0), None, Some(1)], within_five), vec![0, 1, 0]);
}

/// The user's book: every file carries the same picture with different bytes. One choice, all
/// files, the largest (600 px) standing for them, and how many distinct covers it stands for.
#[test]
fn copies_of_one_picture_are_one_choice_with_the_largest_as_its_face() {
    let master = artwork(600);
    let big = jpeg(&master, 70);
    let covers = vec![
        Some(jpeg(&resized(&master, 500), 90)),
        Some(big.clone()),
        Some(jpeg(&master, 90)),
        Some(jpeg(&resized(&master, 500), 90)),
        Some(png(&resized(&master, 400))),
    ];
    let listed = choices(&covers);
    assert_eq!(listed.len(), 1, "{listed:?}");
    let only = &listed[0];
    assert_eq!(only.files, 5);
    assert_eq!(
        only.near_duplicates, 3,
        "four distinct covers: one face and three alike"
    );
    assert_eq!((only.width, only.height), (Some(600), Some(600)));
    // Of the two 600 px copies, the first carried stands.
    assert_eq!(only.key, key_text(&big));
}

#[test]
fn different_covers_stay_apart_and_exact_ones_still_group() {
    let a = jpeg(&artwork(500), 90);
    let b = jpeg(&other_artwork(500), 90);
    let c = jpeg(&mirrored_artwork(500), 90);
    let listed = choices(&[Some(a.clone()), Some(b.clone()), Some(a.clone()), Some(c.clone()), None]);
    let summary: Vec<(String, u32, u32)> = listed
        .iter()
        .map(|choice| (choice.key.clone(), choice.files, choice.near_duplicates))
        .collect();
    assert_eq!(
        summary,
        vec![
            (key_text(&a), 2, 0),
            (key_text(&b), 1, 0),
            (key_text(&c), 1, 0),
            ("none".to_string(), 1, 0),
        ]
    );
}

/// A cover that doesn't decode has no hash and never merges.
#[test]
fn a_cover_that_does_not_decode_stays_its_own_choice() {
    let real = jpeg(&artwork(300), 90);
    let broken = Cover {
        mime: "image/jpeg".to_string(),
        data: b"\xFF\xD8\xFF\xE0 not really a jpeg".to_vec().into(),
    };
    assert!(likeness_of(&broken, &key_text(&broken)).is_none());
    let listed = choices(&[Some(real), Some(broken)]);
    assert_eq!(listed.len(), 2);
    assert!(listed.iter().all(|choice| choice.near_duplicates == 0));
}

/// One cover by bytes (the common case) has nothing to be compared with: nothing is decoded.
#[test]
fn one_distinct_cover_is_not_decoded() {
    let only = jpeg(&template(320, "Only One", GREEN, 1), 88);
    let before = likeness_decodes();
    let listed = choices(&[Some(only.clone()), Some(only), None]);
    assert_eq!(listed.len(), 2);
    assert_eq!(likeness_decodes(), before);
}

/// "Use the largest one for all files" is `use_for_all` with the face's key: every file gets it,
/// and one ⌘Z gives every file its own copy back.
#[test]
fn using_the_largest_for_all_is_one_undo_step() {
    let (dir, _listing, id) = album("tag-editor-cover-near");
    let master = artwork(400);
    let copies = [
        jpeg(&master, 90),
        jpeg(&resized(&master, 300), 80),
        png(&resized(&master, 350)),
    ];
    for (name, cover) in ["a.mp3", "b.mp3", "c.mp3"].iter().zip(&copies) {
        let mut file = tags(&[(TagField::Title, name), (TagField::Album, "Book")]);
        file.cover = Some(cover.clone());
        write_tags(&dir.join(name), &file, &WriteOptions::default()).expect("writes the cover");
    }
    open(&id, false).expect("opens");
    let listed = list(&id).expect("lists");
    assert_eq!(listed.len(), 1);
    assert_eq!((listed[0].files, listed[0].near_duplicates), (3, 2));
    assert_eq!(listed[0].key, key_text(&copies[0]));

    let view = use_for_all(&id, &listed[0].key).expect("uses it");
    assert!(matches!(view.cover, CoverSummary::Same { .. }));
    assert_eq!(changes(&id, ContentKind::Audiobook).expect("changes").len(), 2);

    let undone = undo_edit(&id).expect("undoes");
    assert!(matches!(undone.cover, CoverSummary::Multiple));
    assert!(changes(&id, ContentKind::Audiobook).expect("changes").is_empty());
    close(&id);
    close_rename(&id);
}
