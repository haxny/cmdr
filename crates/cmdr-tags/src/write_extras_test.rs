//! `write_tags` and extras: a write adds an extra the file lacks, so an undo can put back one
//! an edit removed, and keeps the file's own values for an extra both have.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use lofty::id3::v2::Id3v2Tag;
use lofty::mp4::{AtomIdent, Ilst};
use lofty::ogg::tag::VorbisComments;

use crate::model::{FileTags, TagError};
use crate::test_files::*;
use crate::write::write_tags;
use crate::write_test::{
    MUSIC, every_field, freeform_data, raw_flac, raw_id3, raw_ilst, read, strings, txxx_descriptions, utf8, write,
};

fn extras_of(pairs: &[(&str, &[&str])]) -> BTreeMap<String, Vec<String>> {
    pairs.iter().map(|(k, v)| ((*k).to_owned(), strings(v))).collect()
}

/// Writes `tags` with its extras dropped, then `tags` again: the extras come back.
fn extras_come_back(path: &Path, tags: &FileTags) {
    write(
        path,
        &FileTags {
            extras: BTreeMap::new(),
            ..tags.clone()
        },
        &MUSIC,
    );
    assert!(read(path).extras.is_empty(), "{path:?}: the extras are gone first");

    write(path, tags, &MUSIC);

    assert_eq!(read(path), *tags, "{path:?}");
}

#[test]
fn mp3_extra_missing_from_the_file_is_added_as_txxx() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_untagged_mp3(dir.path(), "a.mp3");
    let tags = FileTags {
        extras: extras_of(&[("ITUNESMEDIATYPE", &["Audiobook"]), ("MOOD", &["klidna", "vesela"])]),
        ..every_field()
    };

    extras_come_back(&path, &tags);

    let raw = raw_id3(&path);
    let mut descriptions = txxx_descriptions(&raw);
    descriptions.sort();
    assert_eq!(
        descriptions,
        strings(&[
            "Description",
            "ITUNESMEDIATYPE",
            "MOOD",
            "Performer",
            "Translator",
            "www"
        ])
    );
}

#[test]
fn m4a_extra_missing_from_the_file_is_added_as_an_itunes_freeform() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_m4a(dir.path(), "a.m4a", &Ilst::new());
    let tags = FileTags {
        extras: extras_of(&[("ITUNESMEDIATYPE", &["Audiobook"]), ("MOOD", &["klidna", "vesela"])]),
        ..every_field()
    };

    extras_come_back(&path, &tags);

    let raw = raw_ilst(&path);
    assert_eq!(freeform_data(&raw, "MOOD"), [&utf8("klidna"), &utf8("vesela")]);
    let means: Vec<String> = raw
        .into_iter()
        .filter_map(|a| match a.ident() {
            AtomIdent::Freeform { mean, name } if name == "ITUNESMEDIATYPE" => Some(mean.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(means, strings(&["com.apple.iTunes"]));
}

#[test]
fn vorbis_extra_missing_from_the_file_is_added_as_its_key() {
    let dir = tempfile::tempdir().unwrap();
    let flac = write_flac(dir.path(), "a.flac", VorbisComments::new(), 0);
    let ogg = write_ogg_vorbis(dir.path(), "a.ogg", VorbisComments::new(), 0);
    let opus = write_opus(dir.path(), "a.opus", VorbisComments::new(), 0);
    let tags = FileTags {
        extras: extras_of(&[("ITUNESMEDIATYPE", &["Audiobook"]), ("MOOD", &["klidna", "vesela"])]),
        ..every_field()
    };

    for path in [&flac, &ogg, &opus] {
        extras_come_back(path, &tags);
    }

    let raw = raw_flac(&flac);
    let comments = raw.vorbis_comments().expect("a comment block");
    assert_eq!(comments.get_all("MOOD").collect::<Vec<_>>(), ["klidna", "vesela"]);
}

#[test]
fn extra_in_the_file_and_in_tags_stays_as_the_file_has_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut ilst = Ilst::new();
    freeform(&mut ilst, "org.example", "MOOD", &["klidna"]);
    let m4a = write_m4a(dir.path(), "a.m4a", &ilst);
    let mut comments = VorbisComments::new();
    comments.push("MOOD".to_owned(), "klidna".to_owned());
    let flac = write_flac(dir.path(), "a.flac", comments, 0);
    let mut tag = Id3v2Tag::new();
    txxx(&mut tag, "MOOD", "klidna");
    let mp3 = write_mp3(dir.path(), "a.mp3", &tag);

    for path in [&m4a, &flac, &mp3] {
        let tags = FileTags {
            extras: extras_of(&[("MOOD", &["jina"])]),
            ..read(path)
        };
        write(path, &tags, &MUSIC);
        assert_eq!(read(path).extras, extras_of(&[("MOOD", &["klidna"])]), "{path:?}");
    }
    let means: Vec<String> = raw_ilst(&m4a)
        .into_iter()
        .filter_map(|a| match a.ident() {
            AtomIdent::Freeform { mean, .. } => Some(mean.to_string()),
            AtomIdent::Fourcc(_) => None,
        })
        .collect();
    assert_eq!(means, strings(&["org.example"]), "the foreign mean is kept");
}

#[test]
fn an_extra_the_write_cannot_add_fails_the_write_naming_it() {
    let dir = tempfile::tempdir().unwrap();
    let mp3 = write_untagged_mp3(dir.path(), "a.mp3");
    let m4a = write_m4a(dir.path(), "a.m4a", &Ilst::new());
    let flac = write_flac(dir.path(), "a.flac", VorbisComments::new(), 0);
    for (path, key) in [
        (&mp3, "performer"),
        (&m4a, "Performer"),
        (&flac, "PERFORMER"),
        (&flac, "TRACKTOTAL"),
    ] {
        let before = fs::read(path).unwrap();
        let tags = FileTags {
            extras: extras_of(&[(key, &["Cizi"])]),
            ..every_field()
        };
        assert_eq!(
            write_tags(path, &tags, &MUSIC),
            Err(TagError::UnwritableExtra(key.to_owned())),
            "{path:?}"
        );
        assert_eq!(fs::read(path).unwrap(), before, "{path:?}: untouched");
    }
}

#[test]
fn a_removed_foreign_mean_extra_with_a_managed_name_cannot_come_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut ilst = Ilst::new();
    freeform(&mut ilst, "org.example", "PERFORMER", &["Cizi"]);
    let path = write_m4a(dir.path(), "a.m4a", &ilst);
    let original = read(&path);
    assert_eq!(original.extras, extras_of(&[("PERFORMER", &["Cizi"])]));
    write(
        &path,
        &FileTags {
            extras: BTreeMap::new(),
            ..original.clone()
        },
        &MUSIC,
    );

    let result = write_tags(&path, &original, &MUSIC);

    assert_eq!(result, Err(TagError::UnwritableExtra("PERFORMER".to_owned())));
}

#[test]
fn vorbis_extra_keys_are_uppercased() {
    let dir = tempfile::tempdir().unwrap();
    let mut comments = VorbisComments::new();
    comments.push("KEEP".to_owned(), "soubor".to_owned());
    let path = write_flac(dir.path(), "a.flac", comments, 0);
    let tags = FileTags {
        extras: extras_of(&[("keep", &["jine"]), ("mood", &["klidna"])]),
        ..every_field()
    };

    write(&path, &tags, &MUSIC);

    assert_eq!(
        read(&path).extras,
        extras_of(&[("KEEP", &["soubor"]), ("MOOD", &["klidna"])])
    );
}

#[test]
fn id3_extra_in_another_letter_case_renames_the_frame() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    txxx(&mut tag, "Mood", "klidna");
    let path = write_mp3(dir.path(), "a.mp3", &tag);
    let tags = FileTags {
        extras: extras_of(&[("MOOD", &["klidna"])]),
        ..read(&path)
    };

    write(&path, &tags, &MUSIC);

    assert_eq!(
        txxx_descriptions(&raw_id3(&path)),
        strings(&["MOOD"]),
        "renamed, not doubled"
    );
    assert_eq!(read(&path).extras, extras_of(&[("MOOD", &["klidna"])]));
}
