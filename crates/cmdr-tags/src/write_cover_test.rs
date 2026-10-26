//! Which picture is the cover (Ruling 24): a front cover, else an untyped ("Other") picture,
//! never a typed one such as a back cover or a booklet page. Removing the cover never touches
//! those; setting one on a file that has only those adds a front cover beside them.

use std::path::Path;

use cmdr_media_meta::test_fixtures::cover;
use lofty::id3::v2::{Frame, Id3v2Tag};
use lofty::ogg::OggPictureStorage;
use lofty::ogg::tag::VorbisComments;
use lofty::picture::{Picture, PictureType};

use crate::model::FileTags;
use crate::test_files::*;
use crate::undo::{ApplyId, UndoLimits, UndoStore};
use crate::write_test::{MUSIC, jpeg, raw_flac, raw_id3, read, write};

fn mp3_pictures(path: &Path) -> Vec<Picture> {
    raw_id3(path)
        .into_iter()
        .filter_map(|f| match f {
            Frame::Picture(p) => Some(p.picture.clone().into_owned()),
            _ => None,
        })
        .collect()
}

fn flac_pictures(path: &Path) -> Vec<Picture> {
    let file = raw_flac(path);
    let in_comments = file
        .vorbis_comments()
        .map(|c| c.pictures().to_vec())
        .unwrap_or_default();
    file.pictures()
        .iter()
        .chain(&in_comments)
        .map(|(p, _)| p.clone())
        .collect()
}

fn of_type(pictures: &[Picture], kind: PictureType) -> Vec<Picture> {
    pictures.iter().filter(|p| p.pic_type() == kind).cloned().collect()
}

fn back_cover(len: usize) -> Picture {
    let mut back = cover(len);
    back.set_pic_type(PictureType::CoverBack);
    back
}

/// Removes the cover of `path`, checks the back cover is untouched and no cover is shown,
/// then undoes and checks both pictures are back, the back cover unchanged.
fn remove_then_undo(path: &Path, pictures: fn(&Path) -> Vec<Picture>) {
    let root = tempfile::tempdir().unwrap();
    let before = read(path);
    let back = of_type(&pictures(path), PictureType::CoverBack);
    assert_eq!(back.len(), 1);
    assert_eq!(before.cover.as_ref().map(|c| c.data.len()), Some(40));
    let store = UndoStore::open(root.path().to_path_buf(), UndoLimits::default()).unwrap();
    let id = ApplyId("op-1".to_owned());
    store.record(&id, &[path.to_path_buf()]).unwrap();

    write(
        path,
        &FileTags {
            cover: None,
            ..before.clone()
        },
        &MUSIC,
    );

    assert_eq!(pictures(path), back, "only the back cover is left, unchanged");
    assert_eq!(read(path).cover, None, "a back cover is no cover");

    let report = store.restore(&id, &MUSIC).unwrap();

    assert_eq!(report.restored, vec![path.to_path_buf()]);
    assert_eq!(read(path), before);
    let after = pictures(path);
    assert_eq!(after.len(), 2, "{after:?}");
    assert_eq!(of_type(&after, PictureType::CoverBack), back);
    assert_eq!(of_type(&after, PictureType::CoverFront).len(), 1);
}

#[test]
fn mp3_removing_the_cover_leaves_the_back_cover_and_undo_brings_the_front_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    id3_cover(&mut tag, 40);
    id3_picture(&mut tag, 50, PictureType::CoverBack);
    let path = write_mp3(dir.path(), "a.mp3", &tag);

    remove_then_undo(&path, mp3_pictures);
}

#[test]
fn flac_removing_the_cover_leaves_the_back_cover_and_undo_brings_the_front_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut comments = VorbisComments::new();
    comments.insert_picture(back_cover(50), None).unwrap();
    let path = write_flac(dir.path(), "a.flac", comments, 40);

    remove_then_undo(&path, flac_pictures);
}

#[test]
fn mp3_with_only_typed_pictures_has_no_cover_and_setting_one_adds_a_front_cover() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    id3_picture(&mut tag, 50, PictureType::CoverBack);
    id3_picture(&mut tag, 60, PictureType::Leaflet);
    let path = write_mp3(dir.path(), "a.mp3", &tag);
    let typed = mp3_pictures(&path);
    assert_eq!(read(&path).cover, None);

    write(
        &path,
        &FileTags {
            cover: Some(jpeg(70, 0x21)),
            ..read(&path)
        },
        &MUSIC,
    );

    let after = mp3_pictures(&path);
    assert_eq!(after.len(), 3, "{after:?}");
    assert!(
        typed.iter().all(|p| after.contains(p)),
        "the typed pictures are untouched"
    );
    let front = of_type(&after, PictureType::CoverFront);
    assert_eq!(front.len(), 1);
    assert_eq!(front[0].data(), vec![0x21; 70].as_slice());
    assert_eq!(read(&path).cover, Some(jpeg(70, 0x21)));
}

#[test]
fn flac_with_only_a_back_cover_has_no_cover_and_setting_one_adds_a_front_cover() {
    let dir = tempfile::tempdir().unwrap();
    let mut comments = VorbisComments::new();
    comments.insert_picture(back_cover(50), None).unwrap();
    let path = write_flac(dir.path(), "a.flac", comments, 0);
    let back = flac_pictures(&path);
    assert_eq!(read(&path).cover, None);

    write(
        &path,
        &FileTags {
            cover: Some(jpeg(70, 0x22)),
            ..read(&path)
        },
        &MUSIC,
    );

    let after = flac_pictures(&path);
    assert_eq!(of_type(&after, PictureType::CoverBack), back);
    assert_eq!(of_type(&after, PictureType::CoverFront).len(), 1);
    assert_eq!(read(&path).cover, Some(jpeg(70, 0x22)));
}
