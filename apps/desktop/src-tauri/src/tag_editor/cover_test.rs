//! Cover files: which ones are listed, the size limit, and that a name never leaves the folder.

use super::cover::{MAX_COVER_BYTES, candidates_in, read_cover};
use super::session::TagEditorError;
use crate::test_support::TestDir;

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
    std::fs::write(dir.join("ok.png"), [7u8; 4]).expect("writes");
    assert_eq!(read_cover(&dir, "big.jpg"), Err(TagEditorError::CoverTooLarge));
    let cover = read_cover(&dir, "ok.png").expect("a small image reads");
    assert_eq!(cover.mime, "image/png");
    assert_eq!(&*cover.data, &[7u8; 4]);
}

#[test]
fn cover_name_cannot_leave_the_folder() {
    let root = TestDir::new("cover-escape");
    let folder = root.join("album");
    std::fs::create_dir(&folder).expect("creates");
    std::fs::write(root.join("secret.jpg"), [1u8; 4]).expect("writes");
    std::fs::write(folder.join("in.jpg"), [2u8; 4]).expect("writes");
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
