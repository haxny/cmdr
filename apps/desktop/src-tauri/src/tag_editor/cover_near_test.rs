//! Covers that look the same: re-encoded or resized copies of one picture (different bytes) are
//! one choice, the largest standing for them; visibly different pictures never merge. Every
//! picture is generated here.

use cmdr_tags::{ContentKind, Cover, FileTags, TagField, WriteOptions, write_tags};
use image::{DynamicImage, ImageFormat, Rgb, RgbImage, imageops::FilterType};

use super::cover_choices::{CoverChoice, NEAR_DUPLICATE_BITS, choices_of, covers_of, list, use_for_all};
use super::cover_image::{dhash_of, hamming};
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

fn hash(cover: &Cover) -> u64 {
    dhash_of(cover, &key_text(cover)).expect("decodes")
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

#[test]
fn re_encoded_and_resized_copies_hash_within_the_threshold() {
    let master = artwork(600);
    let q90 = jpeg(&master, 90);
    let q70 = jpeg(&master, 70);
    let small = jpeg(&resized(&master, 500), 85);
    let as_png = png(&resized(&master, 550));
    for copy in [&q70, &small, &as_png] {
        let distance = hamming(hash(&q90), hash(copy));
        assert!(distance <= NEAR_DUPLICATE_BITS, "Hamming distance {distance}");
    }
}

#[test]
fn different_pictures_hash_far_apart() {
    let pairs = [
        (jpeg(&artwork(600), 90), jpeg(&other_artwork(600), 90)),
        (jpeg(&artwork(600), 90), jpeg(&mirrored_artwork(600), 90)),
        (jpeg(&other_artwork(500), 80), jpeg(&mirrored_artwork(500), 80)),
    ];
    for (a, b) in &pairs {
        let distance = hamming(hash(a), hash(b));
        assert!(distance > NEAR_DUPLICATE_BITS * 2, "Hamming distance only {distance}");
    }
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
    assert_eq!(dhash_of(&broken, &key_text(&broken)), None);
    let listed = choices(&[Some(real), Some(broken)]);
    assert_eq!(listed.len(), 2);
    assert!(listed.iter().all(|choice| choice.near_duplicates == 0));
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
