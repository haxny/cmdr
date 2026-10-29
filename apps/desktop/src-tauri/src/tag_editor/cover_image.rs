//! Decoding cover pictures, for the three places a cover's pixels matter: a pasted TIFF (a macOS image copy can carry
//! only TIFF) becomes a PNG, or a JPEG when the PNG is over the cap; and a tile's thumbnail, a small PNG cached by the
//! cover's key so the section never ships every full-size cover to draw 88 px tiles; and a cover's likeness
//! (`cover_likeness`), cached the same way, to tell copies of one picture.
//!
//! Every decode runs under `contain_panics` with `image::Limits`, so a hostile picture costs its thumbnail or its
//! paste, not the app or its memory. Pure CPU: no file is read here.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{Arc, LazyLock, Mutex};

use cmdr_tags::Cover;
use image::{DynamicImage, ImageFormat, ImageReader, Limits};

use super::cover::MAX_COVER_BYTES;
use super::cover_likeness::{Likeness, likeness};
use super::session::TagEditorError;
use crate::crash_reporter::contain_panics;
use crate::ignore_poison::IgnorePoison;

/// A thumbnail's longest side: twice the section's 88 px tile, for Retina.
const THUMBNAIL_SIDE: u32 = 176;
/// JPEG quality for a pasted picture whose PNG is over the cap.
const JPEG_QUALITY: u8 = 90;
/// How many thumbnails stay cached; past it the cache starts over (one book has a handful of covers).
const THUMBNAIL_CACHE_LIMIT: usize = 64;

/// The decode limits: no side over 20,000 px, no more than 256 MiB of pixels.
fn limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(20_000);
    limits.max_image_height = Some(20_000);
    limits.max_alloc = Some(256 * 1024 * 1024);
    limits
}

/// `data` decoded within [`limits`], as `format` or the format its bytes say; `None` when it doesn't decode, is
/// over the limits, or the decoder panics.
fn decode(data: &[u8], format: Option<ImageFormat>) -> Option<DynamicImage> {
    contain_panics(|| {
        let mut reader = ImageReader::new(Cursor::new(data));
        match format {
            Some(format) => reader.set_format(format),
            None => reader = reader.with_guessed_format().ok()?,
        }
        reader.limits(limits());
        reader.decode().ok()
    })
    .flatten()
}

fn encode(picture: &DynamicImage, format: ImageFormat) -> Option<Vec<u8>> {
    let mut out = Cursor::new(Vec::new());
    contain_panics(|| picture.write_to(&mut out, format).ok()).flatten()?;
    Some(out.into_inner())
}

fn encode_jpeg(picture: &DynamicImage) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let rgb = DynamicImage::ImageRgb8(picture.to_rgb8());
    contain_panics(|| {
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
            .encode_image(&rgb)
            .ok()
    })
    .flatten()?;
    Some(out)
}

fn fits(data: &[u8]) -> bool {
    u64::try_from(data.len()).is_ok_and(|len| len <= MAX_COVER_BYTES)
}

/// Whether `data` starts like a TIFF (little- or big-endian).
pub(super) fn is_tiff(data: &[u8]) -> bool {
    data.starts_with(b"II*\0") || data.starts_with(b"MM\0*")
}

/// A pasted TIFF as a cover: a PNG of it, or (when that is over 10 MiB, as a big photo is) a JPEG at quality 90,
/// else `CoverTooLarge`. Bytes that don't decode are `PastedNotAnImage`.
pub(super) fn cover_from_tiff(data: &[u8]) -> Result<Cover, TagEditorError> {
    let picture = decode(data, Some(ImageFormat::Tiff)).ok_or(TagEditorError::PastedNotAnImage)?;
    if let Some(png) = encode(&picture, ImageFormat::Png).filter(|png| fits(png)) {
        return Ok(Cover {
            mime: "image/png".to_owned(),
            data: png.into(),
        });
    }
    let jpeg = encode_jpeg(&picture)
        .filter(|jpeg| fits(jpeg))
        .ok_or(TagEditorError::CoverTooLarge)?;
    Ok(Cover {
        mime: "image/jpeg".to_owned(),
        data: jpeg.into(),
    })
}

/// Thumbnails by cover key (a hash of the bytes, the same in every session).
static THUMBNAILS: LazyLock<Mutex<HashMap<String, Option<String>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// A small PNG `data:` URL of `cover` (longest side [`THUMBNAIL_SIDE`], aspect kept), cached by `key`; `None` when
/// the cover doesn't decode. The cache key is `key` and the byte length. Decoding happens outside
/// the cache's lock.
pub(super) fn thumbnail_of(cover: &Cover, key: &str) -> Option<String> {
    // The byte length beside the hash, as the grouping uses it: a hash collision needs equal sizes too.
    let key = format!("{key}:{}", cover.data.len());
    if let Some(cached) = THUMBNAILS.lock_ignore_poison().get(&key) {
        return cached.clone();
    }
    let made = make_thumbnail(cover);
    let mut cache = THUMBNAILS.lock_ignore_poison();
    if cache.len() >= THUMBNAIL_CACHE_LIMIT {
        cache.clear();
    }
    cache.insert(key, made.clone());
    made
}

fn make_thumbnail(cover: &Cover) -> Option<String> {
    #[cfg(test)]
    ENCODES.with(|count| count.set(count.get() + 1));
    let picture = decode(&cover.data, None)?;
    let small = contain_panics(|| picture.thumbnail(THUMBNAIL_SIDE, THUMBNAIL_SIDE))?;
    let png = encode(&small, ImageFormat::Png)?;
    use base64::Engine;
    Some(format!(
        "data:image/png;base64,{}",
        base64::prelude::BASE64_STANDARD.encode(png)
    ))
}

// How many thumbnails this thread made: per thread, so tests side by side don't count each other's.
#[cfg(test)]
thread_local! {
    static ENCODES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(super) fn thumbnail_encodes() -> usize {
    ENCODES.with(std::cell::Cell::get)
}

/// How many likenesses stay cached; past it the cache starts over. A book whose every file carries its own re-encoded
/// copy of the cover has one per file, so this is well above a big book's file count (each is about 4 KiB, 8 MiB at most).
const LIKENESS_CACHE_LIMIT: usize = 2048;

/// Likenesses by cover key and length, as [`THUMBNAILS`]; `None` for a cover that doesn't decode.
static LIKENESSES: LazyLock<Mutex<HashMap<String, Option<Arc<Likeness>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// What `cover` looks like, small (`cover_likeness`), cached by `key` and the byte length; `None` when it doesn't
/// decode. Decoding happens outside the cache's lock, and never under the session's (the caller copied the cover out).
pub(super) fn likeness_of(cover: &Cover, key: &str) -> Option<Arc<Likeness>> {
    let key = format!("{key}:{}", cover.data.len());
    if let Some(cached) = LIKENESSES.lock_ignore_poison().get(&key) {
        return cached.clone();
    }
    #[cfg(test)]
    LIKENESS_DECODES.with(|count| count.set(count.get() + 1));
    let made = decode(&cover.data, None)
        .and_then(|picture| contain_panics(|| likeness(&picture)))
        .map(Arc::new);
    let mut cache = LIKENESSES.lock_ignore_poison();
    if cache.len() >= LIKENESS_CACHE_LIMIT {
        cache.clear();
    }
    cache.insert(key, made.clone());
    made
}

// How many likenesses this thread decoded: per thread, as `ENCODES`.
#[cfg(test)]
thread_local! {
    static LIKENESS_DECODES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(super) fn likeness_decodes() -> usize {
    LIKENESS_DECODES.with(std::cell::Cell::get)
}
