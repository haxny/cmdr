//! Every cover of the book: grouped by key with how many files carry it, "none" counted, one
//! used for every file as an in-memory change (nothing written), and a new one set from bytes.

use std::sync::atomic::{AtomicU64, Ordering};

use cmdr_tags::{ContentKind, Cover, FileTags, TagField, WriteOptions, write_tags};

use super::cover_choices::{
    CoverChoice, NO_COVER_KEY, choices_of, covers_of, image_of, list, set_for_all, set_from_path_within, use_for_all,
};
use super::cover_image::{thumbnail_encodes, thumbnail_of};
use super::session::{Entry, Loaded, TagEditorError, changes, close, open};
use super::session_test::{album, on_disk, tags};
use super::view::{CoverSummary, cover_key};
use crate::multi_rename::session::close as close_rename;

/// A PNG's signature and IHDR start: a picture by its bytes, with a different tail per `fill`.
fn png(fill: u8, len: usize) -> Cover {
    let mut data = b"\x89PNG\r\n\x1A\n\0\0\0\rIHDR".to_vec();
    data.resize(len.max(data.len()), fill);
    Cover {
        mime: "image/png".to_string(),
        data: data.into(),
    }
}

fn loaded(cover: Option<Cover>) -> Entry {
    let tags = FileTags {
        cover,
        ..FileTags::default()
    };
    Entry::Loaded(Box::new(Loaded {
        cover_key: tags.cover.as_ref().map(cover_key),
        disk: tags.clone(),
        edited: tags,
        duration_ms: None,
        fingerprint: None,
    }))
}

fn key_of(cover: &Cover) -> String {
    format!("{:016x}", cover_key(cover))
}

#[test]
fn choices_group_by_cover_key_with_counts_most_common_first_and_none_last() {
    let front = png(1, 40);
    let back = png(2, 40);
    let entries = [
        loaded(None),
        loaded(Some(back.clone())),
        loaded(Some(front.clone())),
        loaded(Some(front.clone())),
        loaded(None),
        // Same bytes, a separate buffer (another file's own copy): still one choice.
        loaded(Some(Cover {
            mime: front.mime.clone(),
            data: front.data.to_vec().into(),
        })),
    ];
    let files: Vec<&Loaded> = entries.iter().filter_map(Entry::loaded).collect();
    let choices = choices_of(&covers_of(&files));
    let summary: Vec<(String, u32, u64)> = choices.iter().map(|c| (c.key.clone(), c.files, c.bytes)).collect();
    assert_eq!(
        summary,
        vec![
            (key_of(&front), 3, 40),
            (key_of(&back), 1, 40),
            (NO_COVER_KEY.to_string(), 2, 0),
        ]
    );
    assert_eq!(choices[0].mime, "image/png");
    assert_eq!(choices[2].mime, "");
}

#[test]
fn none_is_counted_and_no_files_give_no_choices() {
    let entries = [loaded(None), loaded(None)];
    let files: Vec<&Loaded> = entries.iter().filter_map(Entry::loaded).collect();
    assert_eq!(
        choices_of(&covers_of(&files)),
        vec![CoverChoice {
            key: NO_COVER_KEY.to_string(),
            mime: String::new(),
            bytes: 0,
            width: None,
            height: None,
            files: 2,
        }]
    );
    assert!(choices_of(&[]).is_empty());
}

/// The user's case: the files carry different covers. Each shows, one is used for all, and that
/// is a change of the others in memory only, listed for the write; the files stay as they were.
#[test]
fn use_for_all_sets_that_cover_on_every_file_and_lists_it_as_a_change() {
    let (dir, _listing, id) = album("tag-editor-cover-choices");
    let front = png(7, 64);
    let mut with_cover = tags(&[(TagField::Title, "Alpha"), (TagField::Album, "Book")]);
    with_cover.cover = Some(front.clone());
    write_tags(&dir.join("a.mp3"), &with_cover, &WriteOptions::default()).expect("writes the cover");
    open(&id, false).expect("opens");

    let choices = list(&id).expect("lists");
    let counts: Vec<(&str, u32)> = choices.iter().map(|c| (c.key.as_str(), c.files)).collect();
    assert_eq!(counts, vec![(key_of(&front).as_str(), 1), (NO_COVER_KEY, 2)]);
    assert!(
        image_of(&id, Some(&key_of(&front)))
            .expect("an image")
            .is_some_and(|url| url.starts_with("data:image/png;base64,"))
    );
    assert_eq!(image_of(&id, Some(NO_COVER_KEY)), Ok(None));
    assert_eq!(image_of(&id, None), Ok(None), "no cover every file shares");

    let view = use_for_all(&id, &key_of(&front)).expect("uses it");
    assert!(matches!(view.cover, CoverSummary::Same { bytes: 64, .. }));
    let listed = changes(&id, ContentKind::Audiobook).expect("changes");
    let covered: Vec<(&str, bool)> = listed.iter().map(|c| (c.file_name.as_str(), c.cover)).collect();
    assert_eq!(covered, vec![("b.mp3", true), ("c.mp3", true)]);
    // Nothing is written before Write: the disk keeps its covers.
    assert_eq!(on_disk(&dir, "a.mp3").cover, Some(front.clone()));
    assert_eq!(on_disk(&dir, "b.mp3").cover, None);
    let after = list(&id).expect("lists");
    assert_eq!(after.len(), 1);
    assert_eq!((after[0].key.as_str(), after[0].files), (key_of(&front).as_str(), 3));

    close(&id);
    close_rename(&id);
}

#[test]
fn an_unknown_cover_key_refuses_and_changes_nothing() {
    let (_dir, _listing, id) = album("tag-editor-cover-unknown");
    open(&id, false).expect("opens");
    assert_eq!(use_for_all(&id, "0123456789abcdef"), Err(TagEditorError::UnknownCover));
    assert_eq!(
        image_of(&id, Some("0123456789abcdef")),
        Err(TagEditorError::UnknownCover)
    );
    assert!(changes(&id, ContentKind::Audiobook).expect("changes").is_empty());
    close(&id);
    close_rename(&id);
}

#[test]
fn a_new_cover_goes_to_every_file_in_memory_and_a_closed_session_is_typed() {
    let (dir, _listing, id) = album("tag-editor-cover-set");
    assert_eq!(list(&id), Err(TagEditorError::NotOpen));
    open(&id, false).expect("opens");
    let pasted = png(3, 30);
    let view = set_for_all(&id, pasted.clone()).expect("sets");
    assert!(matches!(view.cover, CoverSummary::Same { bytes: 30, .. }));
    assert_eq!(changes(&id, ContentKind::Audiobook).expect("changes").len(), 3);
    assert_eq!(on_disk(&dir, "a.mp3").cover, None, "nothing written before Write");
    close(&id);
    close_rename(&id);
}

/// A read off a slow NAS that keeps delivering bytes is never cut off, however long it takes in
/// all: here 40 chunks at 60 ms apart, 2.4 s against a 1 s stall limit (the old fixed 2 s tier
/// refused it).
#[tokio::test(flavor = "multi_thread")]
async fn a_dropped_file_read_that_keeps_moving_is_taken_however_long() {
    let (_dir, _listing, id) = album("tag-editor-cover-drop-slow");
    open(&id, false).expect("opens");
    let steady = |_: &std::path::Path, progress: &AtomicU64| {
        for _ in 0..40 {
            // allowed-test-sleep: the pace of a slow NAS is the subject; each chunk lands well inside the stall limit.
            std::thread::sleep(std::time::Duration::from_millis(60));
            progress.fetch_add(64 * 1024, Ordering::Relaxed);
        }
        Ok(png(6, 40))
    };
    let view = set_from_path_within(
        id.clone(),
        "/Volumes/nas/front.png".into(),
        std::time::Duration::from_secs(1),
        reads(),
        steady,
    )
    .await
    .expect("a moving read is taken");
    assert!(matches!(view.cover, CoverSummary::Same { bytes: 40, .. }));
    close(&id);
    close_rename(&id);
}

/// A read whose bytes stopped coming (a hung mount) is refused once it stood still for the stall
/// limit, and the read finishing later changes nothing: the edit is made only from the awaited
/// answer, which already gave up.
#[tokio::test(flavor = "multi_thread")]
async fn a_stalled_dropped_file_read_changes_nothing_ever() {
    let (_dir, _listing, id) = album("tag-editor-cover-drop-stalled");
    open(&id, false).expect("opens");
    let (gate_send, gate) = std::sync::mpsc::channel::<()>();
    let (done_send, done) = std::sync::mpsc::channel();
    let stalled = move |_: &std::path::Path, progress: &AtomicU64| {
        progress.fetch_add(64 * 1024, Ordering::Relaxed);
        let _ = gate.recv();
        let _ = done_send.send(());
        Ok(png(5, 40))
    };
    let result = set_from_path_within(
        id.clone(),
        "/Volumes/hung/front.png".into(),
        std::time::Duration::from_millis(300),
        reads(),
        stalled,
    )
    .await;
    assert_eq!(result, Err(TagEditorError::CoverTimedOut));
    gate_send.send(()).expect("the read still waits");
    done.recv_timeout(std::time::Duration::from_secs(5))
        .expect("the late read ends");
    assert!(
        changes(&id, ContentKind::Audiobook).expect("changes").is_empty(),
        "the late read set no cover"
    );
    close(&id);
    close_rename(&id);
}

/// Reads stuck on a dead mount stay parked on blocking threads; past two at once a new drop is
/// refused at once (as stalled) instead of parking another, and doesn't read at all.
#[tokio::test(flavor = "multi_thread")]
async fn a_third_drop_while_two_reads_are_stuck_is_refused_without_reading() {
    let (_dir, _listing, id) = album("tag-editor-cover-drop-budget");
    open(&id, false).expect("opens");
    let pool = reads();
    let (started_send, started) = std::sync::mpsc::channel::<()>();
    let mut gates = Vec::new();
    let mut stuck = Vec::new();
    for _ in 0..2 {
        let (gate_send, gate) = std::sync::mpsc::channel::<()>();
        gates.push(gate_send);
        let started_send = started_send.clone();
        let reader = move |_: &std::path::Path, _: &AtomicU64| {
            let _ = started_send.send(());
            let _ = gate.recv();
            Ok(png(7, 40))
        };
        stuck.push(tokio::spawn(set_from_path_within(
            id.clone(),
            "/Volumes/dead/a.png".into(),
            std::time::Duration::from_secs(60),
            pool.clone(),
            reader,
        )));
    }
    for _ in 0..2 {
        started
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("each read starts");
    }
    let never =
        |_: &std::path::Path, _: &AtomicU64| -> Result<Cover, TagEditorError> { panic!("a third read must not start") };
    let third = set_from_path_within(
        id.clone(),
        "/Volumes/dead/b.png".into(),
        std::time::Duration::from_secs(60),
        pool,
        never,
    )
    .await;
    assert_eq!(third, Err(TagEditorError::CoverTimedOut));
    for gate in gates {
        gate.send(()).expect("the read waits");
    }
    for read in stuck {
        assert!(read.await.expect("joins").is_ok());
    }
    close(&id);
    close_rename(&id);
}

/// A fresh permit pool for one test, so tests side by side in one process don't share the app's.
fn reads() -> std::sync::Arc<tokio::sync::Semaphore> {
    std::sync::Arc::new(tokio::sync::Semaphore::new(super::cover_choices::DROP_READS_AT_ONCE))
}

fn real_png(width: u32, height: u32) -> Cover {
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(width, height, image::Rgb([200, 10, 10])))
        .write_to(&mut out, image::ImageFormat::Png)
        .expect("encodes");
    Cover {
        mime: "image/png".to_string(),
        data: out.into_inner().into(),
    }
}

/// A tile gets a small PNG of the cover, not the full image, made once per cover.
#[test]
fn a_thumbnail_is_small_and_made_once_per_cover() {
    let big = real_png(1200, 600);
    let before = thumbnail_encodes();
    let url = thumbnail_of(&big, &key_of(&big)).expect("a thumbnail");
    let base64 = url.strip_prefix("data:image/png;base64,").expect("a PNG data URL");
    use base64::Engine;
    let bytes = base64::prelude::BASE64_STANDARD.decode(base64).expect("base64");
    let small = image::load_from_memory(&bytes).expect("decodes");
    assert!(
        small.width() <= 176 && small.height() <= 176,
        "{}×{}",
        small.width(),
        small.height()
    );
    assert_eq!((small.width(), small.height()), (176, 88), "the aspect is kept");
    assert_eq!(thumbnail_of(&big, &key_of(&big)), Some(url));
    assert_eq!(thumbnail_encodes() - before, 1, "the second ask is the cache's");
    // Bytes that aren't a picture have no thumbnail.
    assert_eq!(thumbnail_of(&png(1, 40), &key_of(&png(1, 40))), None);
}
