//! `normalize_for`: tags as a format will hold them.

use crate::format::AudioFormat;
use crate::model::{FileTags, TagField};
use crate::normalize::normalize_for;

fn tags_of(pairs: &[(TagField, &str)]) -> FileTags {
    FileTags {
        fields: pairs.iter().map(|(f, v)| (*f, vec![(*v).to_owned()])).collect(),
        ..FileTags::default()
    }
}

#[test]
fn m4a_keeps_integers() {
    let padded = tags_of(&[
        (TagField::Track, "01"),
        (TagField::TrackTotal, "012"),
        (TagField::Disc, "02"),
        (TagField::DiscTotal, " 3 "),
        (TagField::Album, "01 Kniha"),
    ]);
    assert_eq!(
        normalize_for(AudioFormat::Mp4, &padded),
        tags_of(&[
            (TagField::Track, "1"),
            (TagField::TrackTotal, "12"),
            (TagField::Disc, "2"),
            (TagField::DiscTotal, "3"),
            (TagField::Album, "01 Kniha"),
        ])
    );
}

#[test]
fn m4a_drops_a_number_it_cant_hold() {
    let tags = tags_of(&[
        (TagField::Track, "A1"),
        (TagField::Disc, "0"),
        (TagField::TrackTotal, "70000"),
    ]);
    assert_eq!(normalize_for(AudioFormat::Mp4, &tags), FileTags::default());
}

#[test]
fn mp3_keeps_padding() {
    let padded = tags_of(&[
        (TagField::Track, "01"),
        (TagField::TrackTotal, "12"),
        (TagField::Disc, "02"),
    ]);
    for format in [
        AudioFormat::Mp3,
        AudioFormat::Flac,
        AudioFormat::OggVorbis,
        AudioFormat::Opus,
    ] {
        assert_eq!(normalize_for(format, &padded), padded, "{format:?}");
    }
}
