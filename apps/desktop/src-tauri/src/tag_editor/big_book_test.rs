//! An audiobook-sized session: 22 MP3s, each with its own copy of a cover. Each cover is hashed
//! once per read, never per edit (every edit rebuilds the view). The ignored test measures how
//! long open, five edits and the change list take with 2 MiB covers; timing isn't a pass/fail
//! signal, so run it by name with `--run-ignored all` to print the numbers.

use std::time::{Duration, Instant};

use cmdr_listing::testing::TestListingGuard;
use cmdr_media_meta::test_fixtures::mp3_frames;
use cmdr_tags::shared::CoverEdit;
use cmdr_tags::{ContentKind, Cover, FileTags, TagField, WriteOptions, write_tags};

use super::session::{TagEdit, changes, close, edit, open};
use super::session_test::folder;
use super::view::{CoverSummary, cover_hashes};
use crate::multi_rename::session::{close as close_rename, open as open_rename};
use crate::test_support::TestDir;

const TRACKS: usize = 22;
const COVER_BYTES: usize = 2 * 1024 * 1024;
/// A few MB of audio per file, so a call that reads the whole file shows it.
const AUDIO_FRAME_REPEATS: usize = 2_000;

/// A cover of `len` bytes. Not a real image: with a JPEG's magic bytes in front, the view's
/// pixel-size probe would scan all of it for a header (~55 ms per view in a debug build), which a
/// real cover's header never costs and which would hide what hashing costs.
fn jpeg(len: usize) -> Cover {
    Cover {
        mime: "image/jpeg".to_string(),
        data: vec![0x5a_u8; len].into(),
    }
}

fn book_tags(index: usize) -> FileTags {
    let mut tags = FileTags::default();
    tags.fields.insert(TagField::Title, vec![format!("Kapitola {index}")]);
    tags.fields.insert(TagField::Album, vec!["Kniha".to_string()]);
    tags.fields.insert(TagField::Artist, vec!["Autor".to_string()]);
    tags.fields.insert(TagField::Track, vec![format!("{index:02}")]);
    tags
}

fn timed<T>(run: impl FnOnce() -> T) -> (T, Duration) {
    let started = Instant::now();
    let value = run();
    (value, started.elapsed())
}

/// A session of `TRACKS` covered MP3s with `cover_bytes` of cover each: the folder, its
/// listing, and the Multi-Rename session id.
fn covered_book(tag: &str, audio: &[u8], cover_bytes: usize) -> (TestDir, TestListingGuard, String) {
    let names: Vec<String> = (1..=TRACKS).map(|index| format!("{index:02}.mp3")).collect();
    let files: Vec<(&str, Vec<u8>)> = names.iter().map(|name| (name.as_str(), audio.to_vec())).collect();
    let (dir, listing) = folder(tag, "root", &files);
    for (index, name) in names.iter().enumerate() {
        let tags = FileTags {
            cover: Some(jpeg(cover_bytes)),
            ..book_tags(index + 1)
        };
        write_tags(&dir.join(name), &tags, &WriteOptions::default()).expect("the tags write");
    }
    let session = open_rename(listing.id(), true, None, 0).expect("the rename session opens");
    (dir, listing, session.session_id)
}

/// Each file's cover is hashed once when it's read; an edit hashes none, and a new cover given
/// to every file is hashed once, not once per file.
#[test]
fn covers_are_hashed_once_per_read_and_never_per_edit() {
    let (_dir, _listing, id) = covered_book("tag-cover-keys", &mp3_frames(), 4096);

    let before = cover_hashes();
    open(&id, true).expect("the tag session opens");
    assert_eq!(cover_hashes() - before, TRACKS);

    let before = cover_hashes();
    for value in ["Autor A", "Autor B", "Autor C"] {
        edit(
            &id,
            vec![TagEdit::SetAll {
                field: TagField::Artist,
                values: vec![value.to_string()],
            }],
        )
        .expect("the edit applies");
    }
    assert_eq!(cover_hashes(), before);

    let view = edit(
        &id,
        vec![TagEdit::Cover {
            edit: CoverEdit::Set(jpeg(1024)),
        }],
    )
    .expect("the cover edit applies");
    assert_eq!(cover_hashes() - before, 1);
    assert!(matches!(view.cover, CoverSummary::Same { bytes: 1024, .. }));
    close(&id);
    close_rename(&id);
}

#[test]
#[ignore = "a measurement, not a check: run by name to print the numbers"]
#[expect(clippy::print_stderr, reason = "the measurement's numbers are its output")]
fn an_audiobook_of_22_covered_mp3s_answers_quickly() {
    let (_dir, _listing, id) = covered_book("tag-timing", &mp3_frames().repeat(AUDIO_FRAME_REPEATS), COVER_BYTES);

    let (view, open_took) = timed(|| open(&id, true).expect("the tag session opens"));
    assert_eq!(view.tracks.len(), TRACKS);
    let mut edits_took = Vec::new();
    for (field, value) in [
        (TagField::Artist, "Autor A"),
        (TagField::Artist, "Autor B"),
        (TagField::Album, "Kniha 2"),
        (TagField::Performer, "Ctenar"),
        (TagField::Genre, "audiokniha"),
    ] {
        let (_, took) = timed(|| {
            edit(
                &id,
                vec![TagEdit::SetAll {
                    field,
                    values: vec![value.to_string()],
                }],
            )
            .expect("the edit applies")
        });
        edits_took.push(took);
    }
    let (listed, changes_took) = timed(|| changes(&id, ContentKind::Audiobook).expect("the changes list"));
    assert_eq!(listed.len(), TRACKS);
    eprintln!(
        "tag timing, {TRACKS} MP3s with a {COVER_BYTES}-byte cover: open {open_took:?}, edits {edits_took:?}, changes {changes_took:?}"
    );
    close(&id);
    close_rename(&id);
}
