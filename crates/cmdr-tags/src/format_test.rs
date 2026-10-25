use super::*;

#[test]
fn detects_format_by_extension_case_insensitively() {
    assert_eq!(format_of("a.MP3"), Some(AudioFormat::Mp3));
    assert_eq!(format_of("b.m4b"), Some(AudioFormat::Mp4));
    assert_eq!(format_of("b.m4a"), Some(AudioFormat::Mp4));
    assert_eq!(format_of("b.mp4"), Some(AudioFormat::Mp4));
    assert_eq!(format_of("c.opus"), Some(AudioFormat::Opus));
    assert_eq!(format_of("c.flac"), Some(AudioFormat::Flac));
    assert_eq!(format_of("c.ogg"), Some(AudioFormat::OggVorbis));
    assert_eq!(format_of("c.OGA"), Some(AudioFormat::OggVorbis));
}

#[test]
fn unknown_or_missing_extension_is_none() {
    assert_eq!(format_of("d.wav"), None);
    assert_eq!(format_of("noext"), None);
    assert_eq!(format_of(".mp3"), None);
}

#[test]
fn diacritics_and_multi_flags() {
    assert!(TagField::Comment.keeps_diacritics());
    assert!(TagField::Description.keeps_diacritics());
    assert!(TagField::Www.keeps_diacritics());
    assert!(!TagField::Title.keeps_diacritics());
    assert!(TagField::Genre.is_multi());
    assert!(!TagField::Title.is_multi());
    assert_eq!(TagField::ALL.len(), 15);
}
