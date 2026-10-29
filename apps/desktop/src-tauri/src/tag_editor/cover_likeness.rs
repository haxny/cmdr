//! Whether two covers look the same: the same picture re-encoded or resized (different bytes), not
//! two pictures that share a layout. Pure: it works on a decoded picture (`cover_image::likeness_of`
//! decodes and caches); `cover_choices` merges the covers it calls alike.
//!
//! Four signals, all of which must agree, because each alone is fooled by something a book shelf
//! has: a horizontal and a vertical dHash (the layout of light and dark, blind to colour and to
//! anything small), a 32×32 grey grid compared cell by cell (catches a different title or photo in
//! the same template), and a 4×4 colour grid (catches the same template in another colour). A
//! picture whose both hashes carry almost no bits (a solid or smooth cover) never merges: there is
//! nothing to tell two such covers apart by.
//!
//! Evidence for the limits (`image` 0.25.10, `cover_near_test.rs` pictures plus one real MP3
//! audiobook, 2026-10-29). Copies (JPEG quality 90 against 70, 600 px against 500 px, PNG against
//! JPEG): hashes 0–1 bits apart, grey cells at most 12 apart, colour at most 2. The real book's 24
//! distinct re-encoded copies of its cover: 0 bits, alike. Different pictures: another picture
//! 20/25 bits; the same template with another title only 1/0 bits (dHash alone would merge it)
//! but grey 174; another photo 18/18 bits, grey 84; another colour 4/7 bits, grey 81, colour 61;
//! two solid colours 0/0 bits (flat, never merged), colour 171. The limits sit between, with a
//! margin both ways: 5 bits per hash, 32 per grey cell, 16 per colour channel.

use image::{DynamicImage, GrayImage};

/// Each dHash may differ in at most this many of its 64 bits.
pub const NEAR_DUPLICATE_BITS: u32 = 5;
/// A cell of the 32×32 grey grid may differ by at most this much (of 255).
pub const GREY_TOLERANCE: u8 = 32;
/// A channel of a cell of the 4×4 colour grid may differ by at most this much (of 255).
pub const COLOUR_TOLERANCE: u8 = 16;
/// A hash with at most this many bits set, or unset, says almost nothing about the picture.
const FLAT_BITS: u32 = 2;

const GREY_SIDE: u32 = 32;
const COLOUR_SIDE: u32 = 4;
/// The one box-sampled thumbnail every signal is read from.
const BASE_SIDE: u32 = 64;

/// What a cover looks like, small: enough to tell a copy from another picture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Likeness {
    /// Bit `y * 8 + x`: pixel `x` of row `y` is brighter than its right neighbor (9×8 grey).
    across: u64,
    /// Bit `y * 8 + x`: pixel `y` of column `x` is brighter than the one below it (8×9 grey).
    down: u64,
    grey: Vec<u8>,
    colour: Vec<u8>,
}

/// `picture`'s likeness. Box sampling (`thumbnail_exact`) averages every source pixel, so a
/// re-encode's noise and a resize wash out and only the picture is left.
pub fn likeness(picture: &DynamicImage) -> Likeness {
    let base = DynamicImage::ImageRgb8(picture.thumbnail_exact(BASE_SIDE, BASE_SIDE).to_rgb8());
    let across = base.thumbnail_exact(9, 8).to_luma8();
    let down = base.thumbnail_exact(8, 9).to_luma8();
    Likeness {
        across: hash(&across, |x, y| ((x, y), (x + 1, y))),
        down: hash(&down, |x, y| ((x, y), (x, y + 1))),
        grey: base.thumbnail_exact(GREY_SIDE, GREY_SIDE).to_luma8().into_raw(),
        colour: base.thumbnail_exact(COLOUR_SIDE, COLOUR_SIDE).to_rgb8().into_raw(),
    }
}

type Pair = ((u32, u32), (u32, u32));

/// 64 bits over an 8×8 walk: bit `y * 8 + x` set when the first pixel of `pair(x, y)` is brighter.
fn hash(grey: &GrayImage, pair: impl Fn(u32, u32) -> Pair) -> u64 {
    let mut bits = 0u64;
    for y in 0..8 {
        for x in 0..8 {
            let ((ax, ay), (bx, by)) = pair(x, y);
            if grey.get_pixel(ax, ay)[0] > grey.get_pixel(bx, by)[0] {
                bits |= 1 << (y * 8 + x);
            }
        }
    }
    bits
}

/// How many bits two hashes differ in.
fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

fn flat(bits: u64) -> bool {
    let set = bits.count_ones();
    set <= FLAT_BITS || set >= 64 - FLAT_BITS
}

fn within(a: &[u8], b: &[u8], tolerance: u8) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.abs_diff(*b) <= tolerance)
}

impl Likeness {
    /// Whether the picture has enough detail to be told apart from another (not solid or smooth).
    pub fn informative(&self) -> bool {
        !(flat(self.across) && flat(self.down))
    }

    /// How far apart two covers that look the same are (the bits their hashes differ in, for
    /// picking the nearest of several), or `None` when they don't look the same.
    pub fn distance(&self, other: &Likeness) -> Option<u32> {
        let across = hamming(self.across, other.across);
        let down = hamming(self.down, other.down);
        let alike = self.informative()
            && other.informative()
            && across <= NEAR_DUPLICATE_BITS
            && down <= NEAR_DUPLICATE_BITS
            && within(&self.grey, &other.grey, GREY_TOLERANCE)
            && within(&self.colour, &other.colour, COLOUR_TOLERANCE);
        alike.then_some(across + down)
    }

    /// The largest grey-cell and colour-channel differences, for tuning the tolerances.
    #[cfg(test)]
    pub fn spread(&self, other: &Likeness) -> (u8, u8) {
        let most = |a: &[u8], b: &[u8]| a.iter().zip(b).map(|(a, b)| a.abs_diff(*b)).max().unwrap_or(0);
        (most(&self.grey, &other.grey), most(&self.colour, &other.colour))
    }
}
