//! Cover files: which ones are listed, the size limit, that a name never leaves the folder, and
//! that the type comes from the bytes.

use std::sync::mpsc;
use std::time::Duration;

use super::cover::{
    MAX_COVER_BYTES, candidates_in, cover_from_bytes, read_capped, read_cover, read_dropped, thumbnail_url,
};
use super::session::TagEditorError;
use crate::test_support::TestDir;

const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10, b'J', b'F', b'I', b'F'];
const PNG: &[u8] = b"\x89PNG\r\n\x1A\n\0\0\0\rIHDR";
const WEBP: &[u8] = b"RIFF\x24\0\0\0WEBPVP8 ";

#[test]
fn cover_candidates_lists_images_only() {
    let dir = TestDir::new("cover-candidates");
    for (name, bytes) in [
        ("b.PNG", vec![1u8; 3]),
        ("a.jpg", vec![1; 5]),
        ("c.webp", vec![1; 2]),
        ("d.jpeg", vec![1; 1]),
        ("notes.txt", vec![1; 9]),
        ("song.mp3", vec![1; 9]),
        ("gif.gif", vec![1; 9]),
    ] {
        std::fs::write(dir.join(name), bytes).expect("writes");
    }
    std::fs::create_dir(dir.join("folder.jpg")).expect("creates");
    let found = candidates_in(&dir).expect("lists");
    let listed: Vec<(&str, u64)> = found.iter().map(|c| (c.file_name.as_str(), c.size)).collect();
    assert_eq!(listed, vec![("a.jpg", 5), ("b.PNG", 3), ("c.webp", 2), ("d.jpeg", 1)]);
}

#[test]
fn oversized_cover_is_refused() {
    let dir = TestDir::new("cover-oversized");
    let big = vec![0u8; usize::try_from(MAX_COVER_BYTES).expect("fits") + 1];
    std::fs::write(dir.join("big.jpg"), big).expect("writes");
    std::fs::write(dir.join("ok.png"), PNG).expect("writes");
    assert_eq!(read_cover(&dir, "big.jpg"), Err(TagEditorError::CoverTooLarge));
    let cover = read_cover(&dir, "ok.png").expect("a small image reads");
    assert_eq!(cover.mime, "image/png");
    assert_eq!(&*cover.data, PNG);
}

#[test]
fn cover_name_cannot_leave_the_folder() {
    let root = TestDir::new("cover-escape");
    let folder = root.join("album");
    std::fs::create_dir(&folder).expect("creates");
    std::fs::write(root.join("secret.jpg"), JPEG).expect("writes");
    std::fs::write(folder.join("in.jpg"), JPEG).expect("writes");
    std::os::unix::fs::symlink(root.join("secret.jpg"), folder.join("link.jpg")).expect("links");
    for name in [
        "../secret.jpg",
        "..",
        ".",
        "",
        "sub/in.jpg",
        "/etc/passwd.jpg",
        "..\\secret.jpg",
        "a\0.jpg",
    ] {
        assert_eq!(read_cover(&folder, name), Err(TagEditorError::BadCoverName), "{name:?}");
    }
    assert_eq!(read_cover(&folder, "link.jpg"), Err(TagEditorError::BadCoverName));
    assert_eq!(read_cover(&folder, "missing.jpg"), Err(TagEditorError::CoverUnreadable));
    assert_eq!(read_cover(&folder, "in.txt"), Err(TagEditorError::CoverUnreadable));
    assert!(read_cover(&folder, "in.jpg").is_ok());
}

#[test]
fn a_webp_named_jpg_gets_the_webp_type() {
    let dir = TestDir::new("cover-mislabeled");
    std::fs::write(dir.join("cover.jpg"), WEBP).expect("writes");
    let cover = read_cover(&dir, "cover.jpg").expect("reads");
    assert_eq!(cover.mime, "image/webp");
}

#[test]
fn a_jpeg_named_png_gets_the_jpeg_type() {
    let dir = TestDir::new("cover-mislabeled-png");
    std::fs::write(dir.join("cover.png"), JPEG).expect("writes");
    assert_eq!(read_cover(&dir, "cover.png").expect("reads").mime, "image/jpeg");
}

#[test]
fn html_or_an_empty_file_named_jpg_is_not_an_image() {
    let dir = TestDir::new("cover-not-image");
    std::fs::write(dir.join("error.jpg"), b"<!DOCTYPE html><html>404</html>").expect("writes");
    std::fs::write(dir.join("empty.jpg"), b"").expect("writes");
    std::fs::write(dir.join("riff.webp"), b"RIFF\0\0\0\0WAVE").expect("writes");
    for name in ["error.jpg", "empty.jpg", "riff.webp"] {
        assert_eq!(read_cover(&dir, name), Err(TagEditorError::CoverNotAnImage), "{name}");
    }
}

/// A FIFO named like a picture would block a plain `open` until a writer comes, forever.
#[test]
fn a_fifo_named_jpg_is_refused_without_blocking() {
    let dir = TestDir::new("cover-fifo");
    let fifo = dir.join("pipe.jpg");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("mkfifo runs");
    assert!(status.success());
    let (send, answer) = mpsc::channel();
    let folder = dir.to_path_buf();
    std::thread::spawn(move || {
        let _ = send.send(read_cover(&folder, "pipe.jpg"));
    });
    let result = answer
        .recv_timeout(Duration::from_secs(5))
        .expect("read_cover answers instead of blocking on the FIFO");
    assert_eq!(result, Err(TagEditorError::CoverUnreadable));
}

/// A symlink whose target sits in the folder reads the target, typed by the target's bytes
/// whatever the link's name; one leading out is refused (`cover_name_cannot_leave_the_folder`).
#[test]
fn a_symlink_inside_the_folder_reads_its_target_by_its_bytes() {
    let dir = TestDir::new("cover-symlink-in");
    std::fs::write(dir.join("real.png"), PNG).expect("writes");
    std::os::unix::fs::symlink(dir.join("real.png"), dir.join("link.jpg")).expect("links");
    let cover = read_cover(&dir, "link.jpg").expect("reads");
    assert_eq!(cover.mime, "image/png");
    assert_eq!(&*cover.data, PNG);
}

/// The size limit holds on the read itself, for a file that grows after the stat.
#[test]
fn the_read_stops_past_the_limit() {
    let at_limit = vec![0u8; 16];
    assert_eq!(read_capped(at_limit.as_slice(), 16).expect("fits").len(), 16);
    let over = vec![0u8; 17];
    assert_eq!(read_capped(over.as_slice(), 16), Err(TagEditorError::CoverTooLarge));
    // An endless reader stops at limit + 1 bytes.
    assert_eq!(read_capped(std::io::repeat(1), 16), Err(TagEditorError::CoverTooLarge));
}

#[test]
fn the_thumbnail_is_a_data_url_typed_by_the_bytes() {
    assert_eq!(
        thumbnail_url(PNG).as_deref(),
        Some("data:image/png;base64,iVBORw0KGgoAAAANSUhEUg==")
    );
    assert!(thumbnail_url(WEBP).is_some_and(|url| url.starts_with("data:image/webp;base64,")));
    assert_eq!(thumbnail_url(b"<html>"), None);
}

/// A paste: the bytes are a picture by their first bytes, at most 10 MiB, or refused by kind.
#[test]
fn pasted_bytes_are_sniffed_and_capped() {
    let cover = cover_from_bytes(PNG.to_vec()).expect("a PNG");
    assert_eq!((cover.mime.as_str(), &*cover.data), ("image/png", PNG));
    assert_eq!(cover_from_bytes(WEBP.to_vec()).expect("a WebP").mime, "image/webp");
    for refused in [
        Vec::new(),
        b"<html>".to_vec(),
        b"GIF89a".to_vec(),
        b"RIFF\0\0\0\0WAVE".to_vec(),
    ] {
        assert_eq!(
            cover_from_bytes(refused),
            Err(TagEditorError::PastedNotAnImage),
            "a paste has no file name: its refusal is worded apart"
        );
    }
    let limit = usize::try_from(MAX_COVER_BYTES).expect("fits");
    let mut at_limit = JPEG.to_vec();
    at_limit.resize(limit, 0);
    assert!(cover_from_bytes(at_limit.clone()).is_ok(), "exactly 10 MiB is taken");
    at_limit.push(0);
    assert_eq!(cover_from_bytes(at_limit), Err(TagEditorError::CoverTooLarge));
}

/// A drop: a regular picture file anywhere reads; its size cap and type sniff are the same.
#[test]
fn a_dropped_picture_file_reads_by_its_bytes() {
    let dir = TestDir::new("cover-drop");
    std::fs::write(dir.join("front.jpg"), PNG).expect("writes");
    let cover = read_dropped(&dir.join("front.jpg")).expect("reads");
    assert_eq!((cover.mime.as_str(), &*cover.data), ("image/png", PNG));
    std::fs::write(dir.join("page.jpg"), b"<html>").expect("writes");
    assert_eq!(
        read_dropped(&dir.join("page.jpg")),
        Err(TagEditorError::CoverNotAnImage)
    );
    let big = vec![0xFFu8; usize::try_from(MAX_COVER_BYTES).expect("fits") + 1];
    std::fs::write(dir.join("big.jpg"), big).expect("writes");
    assert_eq!(read_dropped(&dir.join("big.jpg")), Err(TagEditorError::CoverTooLarge));
    assert_eq!(
        read_dropped(&dir.join("missing.jpg")),
        Err(TagEditorError::CoverUnreadable)
    );
    assert_eq!(
        read_dropped(std::path::Path::new("front.jpg")),
        Err(TagEditorError::CoverUnreadable),
        "a relative path"
    );
}

/// A dropped folder, FIFO or symlink is refused as not a picture file, and the FIFO without blocking.
#[test]
fn a_dropped_folder_fifo_or_symlink_is_refused() {
    let dir = TestDir::new("cover-drop-refused");
    std::fs::create_dir(dir.join("folder.jpg")).expect("creates");
    assert_eq!(
        read_dropped(&dir.join("folder.jpg")),
        Err(TagEditorError::CoverNotAFile)
    );

    std::fs::write(dir.join("real.png"), PNG).expect("writes");
    std::os::unix::fs::symlink(dir.join("real.png"), dir.join("link.png")).expect("links");
    assert_eq!(read_dropped(&dir.join("link.png")), Err(TagEditorError::CoverNotAFile));

    let fifo = dir.join("pipe.jpg");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("mkfifo runs");
    assert!(status.success());
    let (send, answer) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = send.send(read_dropped(&fifo));
    });
    let result = answer
        .recv_timeout(Duration::from_secs(5))
        .expect("read_dropped answers instead of blocking on the FIFO");
    assert_eq!(result, Err(TagEditorError::CoverNotAFile));
}

/// Noise, so neither PNG nor JPEG can shrink it much.
fn noise(width: u32, height: u32) -> image::RgbImage {
    let mut state = 0x1234_5678_u32;
    image::RgbImage::from_fn(width, height, |_, _| {
        let mut next = || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        };
        image::Rgb([next(), next(), next()])
    })
}

fn tiff_of(picture: image::RgbImage) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(picture)
        .write_to(&mut out, image::ImageFormat::Tiff)
        .expect("encodes a TIFF");
    out.into_inner()
}

/// A TIFF pasted (a macOS image copy can carry only TIFF) becomes a PNG of the same picture.
#[test]
fn a_pasted_tiff_becomes_a_png() {
    let cover = cover_from_bytes(tiff_of(noise(40, 30))).expect("converts");
    assert_eq!(cover.mime, "image/png");
    let decoded = image::load_from_memory(&cover.data).expect("a PNG");
    assert_eq!((decoded.width(), decoded.height()), (40, 30));
}

/// A big photo pasted as TIFF whose PNG is over 10 MiB becomes a JPEG under the cap instead of being refused.
#[test]
fn a_pasted_tiff_too_big_as_png_becomes_a_jpeg() {
    let cover = cover_from_bytes(tiff_of(noise(1900, 1900))).expect("converts");
    assert_eq!(cover.mime, "image/jpeg");
    assert!(u64::try_from(cover.data.len()).expect("fits") <= MAX_COVER_BYTES);
}

/// A broken TIFF is not a picture, and the conversion never panics out.
#[test]
fn a_broken_tiff_is_refused() {
    let mut broken = tiff_of(noise(8, 8));
    broken.truncate(16);
    assert_eq!(cover_from_bytes(broken), Err(TagEditorError::PastedNotAnImage));
}
