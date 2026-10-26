//! An audiobook M4B carries a chapter track: a second `trak` (handler `text`) whose samples
//! live in the same `mdat` as the audio. Growing the tag moves `mdat`, so lofty must move every
//! track's chunk offsets, not only the audio's.

use std::fs;

use crate::model::FileTags;
use crate::test_files::*;
use crate::write_test::{MUSIC, every_field, jpeg, read, write};

/// Checks every `stco` of `bytes` still points at its own chunks.
fn offsets_point_at_their_chunks(bytes: &[u8], chunks: &[u8]) {
    let tables = all_stco_offsets(bytes);
    assert_eq!(tables.len(), chunks.len(), "one stco per track");
    for (track, (offsets, count)) in tables.iter().zip(chunks).enumerate() {
        assert_eq!(offsets.len(), usize::from(*count), "track {track}");
        for (chunk, offset) in offsets.iter().enumerate() {
            assert_eq!(
                bytes[*offset..*offset + CHUNK_LEN],
                [chunk_byte(track, chunk); CHUNK_LEN],
                "track {track}, chunk {chunk}"
            );
        }
    }
}

#[test]
fn m4b_chapter_track_offsets_follow_a_growing_tag() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_bytes(
        dir.path(),
        "kniha.m4b",
        m4a_with_tracks(b"M4B ", &[(b"soun", 3), (b"text", 2)]),
    );
    let before = fs::read(&path).unwrap();
    offsets_point_at_their_chunks(&before, &[3, 2]);

    let tags = FileTags {
        cover: Some(jpeg(50_000, 0x34)),
        ..every_field()
    };
    write(&path, &tags, &MUSIC);

    let after = fs::read(&path).unwrap();
    assert!(after.len() > before.len() + 50_000);
    offsets_point_at_their_chunks(&after, &[3, 2]);
    assert_eq!(read(&path), tags);
}
