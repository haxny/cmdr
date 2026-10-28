//! Writing a tag session to disk and undoing it, against real audio files, a real
//! Multi-Rename session, and a real undo store.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use cmdr_local::LocalPosixVolume;
use cmdr_media_meta::test_fixtures::mp3_frames;
use cmdr_tags::{ContentKind, TagError, TagField, UndoLimits, UndoStore};

use super::apply::{ApplyRequest, TagApplyReport, apply_with, apply_with_before_write, narrow_record};
use super::session::{TagEdit, TagEditorError, changes, close, edit, open, unchanged_on_disk};
use super::session_test::{album, folder, on_disk, path_of, set_title, tag_file};
use super::undo::undo_with;
use crate::file_system::volume::manager::get_volume_manager;
use crate::multi_rename::plan::MultiRenameSpec;
use crate::multi_rename::session::{
    close as close_rename, media_progress, open as open_rename, preview_session, start_media,
};
use crate::multi_rename::transform::CaseChange;
use crate::test_support::{TestDir, wait_until_async};

/// A fresh undo store in its own scratch dir.
pub(super) fn store(tag: &str) -> (TestDir, UndoStore) {
    let dir = TestDir::new(tag);
    let store = UndoStore::open(dir.join("tag-undo"), UndoLimits::default()).expect("the store opens");
    (dir, store)
}

/// The apply records left in the store.
fn records(store_dir: &TestDir) -> usize {
    std::fs::read_dir(store_dir.join("tag-undo"))
        .expect("the store dir lists")
        .filter(|item| {
            item.as_ref()
                .is_ok_and(|item| !item.file_name().to_string_lossy().starts_with('.'))
        })
        .count()
}

pub(super) fn request() -> ApplyRequest {
    ApplyRequest {
        kind: ContentKind::Audiobook,
        allow_frame_loss: false,
        skip: Vec::new(),
    }
}

pub(super) fn set_album(album: &str) -> TagEdit {
    TagEdit::SetAll {
        field: TagField::Album,
        values: vec![album.to_string()],
    }
}

fn title_of(dir: &Path, name: &str) -> Option<Vec<String>> {
    on_disk(dir, name).fields.get(&TagField::Title).cloned()
}

pub(super) fn album_of(dir: &Path, name: &str) -> Option<Vec<String>> {
    on_disk(dir, name).fields.get(&TagField::Album).cloned()
}

pub(super) fn one(value: &str) -> Option<Vec<String>> {
    Some(vec![value.to_string()])
}

pub(super) fn names(paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .map(|path| {
            Path::new(path)
                .file_name()
                .expect("a file path")
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

fn set_mode(path: &Path, mode: u32) {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("the mode changes");
}

pub(super) fn done(id: &str) {
    close(id);
    close_rename(id);
}

#[test]
fn apply_writes_changed_files_only() {
    let (dir, _listing, id) = album("tag-apply-changed-only");
    let (store_dir, store) = store("tag-apply-changed-only-store");
    open(&id, false).expect("opens");
    edit(&id, vec![set_title(&dir, "a.mp3", "Alpha edited")]).expect("edits");
    let b_before = std::fs::read(dir.join("b.mp3")).expect("b reads");

    let report = apply_with(&store, &id, request()).expect("applies");

    assert_eq!(names(&report.written), vec!["a.mp3"]);
    assert!(report.would_drop.is_empty() && report.failed.is_empty() && report.changed_on_disk.is_empty());
    let apply_id = report.apply_id.expect("an apply id");
    assert!(apply_id.starts_with("tags-") && apply_id.ends_with(&id), "{apply_id}");
    assert_eq!(title_of(&dir, "a.mp3"), one("Alpha edited"));
    assert_eq!(
        std::fs::read(dir.join("b.mp3")).expect("b reads"),
        b_before,
        "b is untouched"
    );
    assert_eq!(records(&store_dir), 1);
    assert!(
        !open(&id, false).expect("still open").dirty,
        "the written copy is the disk copy now"
    );
    done(&id);
}

/// Review focus 3: one file can't be written; the others are, and undo restores exactly those.
#[test]
fn one_failure_keeps_the_others_and_undo_restores_written() {
    let (dir, _listing, id) = album("tag-apply-one-fails");
    let (_store_dir, store) = store("tag-apply-one-fails-store");
    // Before the session reads it: a chmod moves the ctime, which a write would take for a change.
    set_mode(&dir.join("b.mp3"), 0o444);
    open(&id, false).expect("opens");
    edit(&id, vec![set_album("Kniha")]).expect("edits");

    let report = apply_with(&store, &id, request()).expect("applies");

    assert_eq!(names(&report.written), vec!["a.mp3", "c.mp3"]);
    assert_eq!(report.failed.len(), 1);
    assert_eq!(names(&[report.failed[0].path.clone()]), vec!["b.mp3"]);
    assert!(
        matches!(
            report.failed[0].error,
            TagError::Io(std::io::ErrorKind::PermissionDenied, _)
        ),
        "{:?}",
        report.failed[0].error
    );
    assert_eq!(album_of(&dir, "a.mp3"), one("Kniha"));
    assert_eq!(album_of(&dir, "b.mp3"), one("Book"));
    let view = open(&id, false).expect("still open");
    assert!(view.dirty, "b's edit is still waiting");

    let restored = undo_with(
        &store,
        report.apply_id.as_deref().expect("an apply id"),
        Some(id.as_str()),
    )
    .expect("undoes");
    let restored_names: Vec<String> = restored
        .restored
        .iter()
        .map(|path| path.file_name().expect("a file").to_string_lossy().into_owned())
        .collect();
    assert_eq!(restored_names, vec!["a.mp3", "c.mp3"]);
    assert!(
        restored.failed.is_empty() && restored.missing.is_empty(),
        "{restored:?}"
    );
    assert_eq!(album_of(&dir, "a.mp3"), one("Book"));
    assert_eq!(album_of(&dir, "c.mp3"), one("Other"));
    set_mode(&dir.join("b.mp3"), 0o644);
    done(&id);
}

/// An MP3 whose ID3v2.4 tag holds a title and a `TSOP` frame, which ID3v2.3 has no place for.
fn mp3_with_v24_only_frame() -> Vec<u8> {
    mp3_with_v24_frames(&[(b"TIT2", "Alpha"), (b"TSOP", "Sort name")])
}

/// An MP3 with an ID3v2.4 tag of these text frames.
pub(super) fn mp3_with_v24_frames(frames: &[(&[u8; 4], &str)]) -> Vec<u8> {
    fn syncsafe(n: usize) -> [u8; 4] {
        let n = u32::try_from(n).expect("a small tag");
        [
            (n >> 21) as u8 & 0x7F,
            (n >> 14) as u8 & 0x7F,
            (n >> 7) as u8 & 0x7F,
            n as u8 & 0x7F,
        ]
    }
    fn frame(id: &[u8; 4], text: &str) -> Vec<u8> {
        let body: Vec<u8> = std::iter::once(3).chain(text.bytes()).collect();
        [id.as_slice(), &syncsafe(body.len()), &[0, 0], &body].concat()
    }
    let frames: Vec<u8> = frames.iter().flat_map(|(id, text)| frame(id, text)).collect();
    [
        b"ID3".as_slice(),
        &[4, 0, 0],
        &syncsafe(frames.len()),
        &frames,
        &mp3_frames(),
    ]
    .concat()
}

#[test]
fn would_drop_is_reported_not_written_then_allow_frame_loss_writes_it() {
    let (dir, listing) = folder(
        "tag-apply-would-drop",
        "root",
        &[("a.mp3", mp3_with_v24_only_frame()), ("b.mp3", mp3_frames())],
    );
    let (store_dir, store) = store("tag-apply-would-drop-store");
    let id = open_rename(listing.id(), true, None, 0).expect("opens").session_id;
    open(&id, false).expect("opens");
    edit(&id, vec![set_album("Kniha")]).expect("edits");
    let a_before = std::fs::read(dir.join("a.mp3")).expect("a reads");

    let report = apply_with(&store, &id, request()).expect("applies");

    assert_eq!(names(&report.written), vec!["b.mp3"]);
    assert_eq!(report.would_drop.len(), 1);
    assert_eq!(names(&[report.would_drop[0].path.clone()]), vec!["a.mp3"]);
    assert_eq!(report.would_drop[0].items, vec!["TSOP".to_string()]);
    assert_eq!(
        std::fs::read(dir.join("a.mp3")).expect("a reads"),
        a_before,
        "a is untouched"
    );
    assert_eq!(records(&store_dir), 1);

    let report = apply_with(
        &store,
        &id,
        ApplyRequest {
            allow_frame_loss: true,
            ..request()
        },
    )
    .expect("applies again");
    assert_eq!(names(&report.written), vec!["a.mp3"], "b was written the first time");
    assert!(report.would_drop.is_empty());
    assert_eq!(album_of(&dir, "a.mp3"), one("Kniha"));
    done(&id);
}

#[test]
fn skip_list_is_respected() {
    let (dir, _listing, id) = album("tag-apply-skip");
    let (_store_dir, store) = store("tag-apply-skip-store");
    open(&id, false).expect("opens");
    edit(&id, vec![set_album("Kniha")]).expect("edits");

    let report = apply_with(
        &store,
        &id,
        ApplyRequest {
            skip: vec![path_of(&dir, "b.mp3")],
            ..request()
        },
    )
    .expect("applies");

    assert_eq!(names(&report.written), vec!["a.mp3", "c.mp3"]);
    assert_eq!(album_of(&dir, "b.mp3"), one("Book"));
    let view = open(&id, false).expect("still open");
    assert_eq!(
        view.tracks.iter().filter(|row| row.changed).count(),
        1,
        "only the skipped file still waits"
    );
    done(&id);
}

#[test]
fn undo_restores_tags_after_apply() {
    let (dir, _listing, id) = album("tag-apply-undo");
    let (store_dir, store) = store("tag-apply-undo-store");
    open(&id, false).expect("opens");
    edit(&id, vec![set_title(&dir, "a.mp3", "Alpha edited"), set_album("Kniha")]).expect("edits");
    let before: Vec<_> = ["a.mp3", "b.mp3", "c.mp3"].map(|name| on_disk(&dir, name)).into();
    let report = apply_with(&store, &id, request()).expect("applies");
    assert_eq!(report.written.len(), 3);

    let restored = undo_with(
        &store,
        report.apply_id.as_deref().expect("an apply id"),
        Some(id.as_str()),
    )
    .expect("undoes");

    assert_eq!(restored.restored.len(), 3);
    let after: Vec<_> = ["a.mp3", "b.mp3", "c.mp3"].map(|name| on_disk(&dir, name)).into();
    assert_eq!(after, before);
    let view = open(&id, false).expect("still open");
    assert!(!view.dirty, "the session holds the restored tags");
    assert_eq!(view.tracks[0].title.as_deref(), Some("Alpha"));
    assert_eq!(
        unchanged_on_disk(&id, &dir.join("a.mp3")),
        Ok(true),
        "re-read after the undo"
    );
    assert_eq!(records(&store_dir), 0, "a clean undo forgets its record");
    done(&id);
}

#[test]
fn undo_after_session_closed_still_works() {
    let (dir, _listing, id) = album("tag-apply-undo-closed");
    let (_store_dir, store) = store("tag-apply-undo-closed-store");
    open(&id, false).expect("opens");
    edit(&id, vec![set_title(&dir, "a.mp3", "Alpha edited")]).expect("edits");
    let report = apply_with(&store, &id, request()).expect("applies");
    done(&id);

    let restored = undo_with(
        &store,
        report.apply_id.as_deref().expect("an apply id"),
        Some(id.as_str()),
    )
    .expect("undoes");

    assert_eq!(restored.restored.len(), 1);
    assert_eq!(title_of(&dir, "a.mp3"), one("Alpha"));
}

/// An undo from a later sheet: the session that wrote is gone, another one over the same files is
/// open, and it holds the restored tags afterwards, not the written ones.
#[test]
fn undo_reloads_every_open_session_that_holds_a_restored_file() {
    let (dir, listing, first) = album("tag-apply-undo-other-sheet");
    let (_store_dir, store) = store("tag-apply-undo-other-sheet-store");
    open(&first, false).expect("opens");
    edit(&first, vec![set_title(&dir, "a.mp3", "Alpha edited")]).expect("edits");
    let report = apply_with(&store, &first, request()).expect("applies");
    done(&first);
    let later = open_rename(listing.id(), true, None, 0)
        .expect("a later rename session opens")
        .session_id;
    let before_undo = open(&later, false).expect("the later sheet opens its tags");
    assert_eq!(before_undo.tracks[0].title.as_deref(), Some("Alpha edited"));

    undo_with(
        &store,
        report.apply_id.as_deref().expect("an apply id"),
        Some(later.as_str()),
    )
    .expect("undoes");

    let view = open(&later, false).expect("still open");
    assert_eq!(
        view.tracks[0].title.as_deref(),
        Some("Alpha"),
        "the later sheet reads the restored tags"
    );
    assert!(!view.dirty);
    assert_eq!(
        unchanged_on_disk(&later, &dir.join("a.mp3")),
        Ok(true),
        "re-read after the undo"
    );
    done(&later);
}

/// Another sheet's tag session keeps its unwritten edits through an undo it didn't ask for: only
/// what it never edited follows the restored disk, and its disk copy and fingerprint are fresh.
#[test]
fn undo_keeps_another_sessions_unwritten_edits_and_refreshes_its_disk_copy() {
    let (dir, listing, writer) = album("tag-apply-undo-keeps-edits");
    let (_store_dir, store) = store("tag-apply-undo-keeps-edits-store");
    open(&writer, false).expect("opens");
    edit(&writer, vec![set_album("Kniha")]).expect("edits");
    let report = apply_with(&store, &writer, request()).expect("applies");
    let other = open_rename(listing.id(), true, None, 0)
        .expect("another rename session opens")
        .session_id;
    open(&other, false).expect("the other session opens its tags");
    edit(&other, vec![set_title(&dir, "a.mp3", "Alpha by the other")]).expect("edits");

    undo_with(
        &store,
        report.apply_id.as_deref().expect("an apply id"),
        Some(writer.as_str()),
    )
    .expect("undoes");

    let view = open(&other, false).expect("still open");
    assert_eq!(
        view.tracks[0].title.as_deref(),
        Some("Alpha by the other"),
        "the unwritten edit stays"
    );
    assert!(view.dirty);
    let changes = changes(&other, ContentKind::Audiobook).expect("lists its changes");
    assert_eq!(changes.len(), 1, "only the edited file differs from the disk");
    assert_eq!(
        changes[0].fields,
        vec![TagField::Title],
        "the album follows the restored disk"
    );
    assert_eq!(
        unchanged_on_disk(&other, &dir.join("a.mp3")),
        Ok(true),
        "its fingerprint is fresh"
    );
    assert_eq!(album_of(&dir, "a.mp3"), one("Book"));
    done(&other);
    done(&writer);
}

#[test]
fn undo_of_an_unknown_apply_is_typed() {
    let (_store_dir, store) = store("tag-apply-undo-unknown");
    assert!(matches!(
        undo_with(&store, "tags-1-multi-rename-0", None),
        Err(TagEditorError::Undo {
            error: TagError::Io(std::io::ErrorKind::NotFound, _)
        })
    ));
}

#[test]
fn nothing_changed_writes_nothing_and_records_nothing() {
    let (dir, _listing, id) = album("tag-apply-nothing");
    let (store_dir, store) = store("tag-apply-nothing-store");
    open(&id, false).expect("opens");
    let before = std::fs::read(dir.join("a.mp3")).expect("a reads");

    let report = apply_with(&store, &id, request()).expect("applies");

    assert_eq!(report, TagApplyReport::default());
    assert_eq!(std::fs::read(dir.join("a.mp3")).expect("a reads"), before);
    assert_eq!(records(&store_dir), 0);
    done(&id);
}

#[test]
fn nothing_written_forgets_the_record() {
    let (dir, _listing, id) = album("tag-apply-all-fail");
    let (store_dir, store) = store("tag-apply-all-fail-store");
    set_mode(&dir.join("a.mp3"), 0o444);
    open(&id, false).expect("opens");
    edit(&id, vec![set_title(&dir, "a.mp3", "Alpha edited")]).expect("edits");

    let report = apply_with(&store, &id, request()).expect("applies");

    assert_eq!(report.apply_id, None);
    assert_eq!(report.failed.len(), 1);
    assert_eq!(records(&store_dir), 0);
    set_mode(&dir.join("a.mp3"), 0o644);
    done(&id);
}

#[test]
fn a_file_changed_on_disk_since_open_is_reported_not_overwritten() {
    let (dir, _listing, id) = album("tag-apply-changed-on-disk");
    let (_store_dir, store) = store("tag-apply-changed-on-disk-store");
    open(&id, false).expect("opens");
    edit(&id, vec![set_album("Kniha")]).expect("edits");
    tag_file(&dir, "b.mp3", &[(TagField::Title, "Rewritten elsewhere")]);

    let report = apply_with(&store, &id, request()).expect("applies");

    assert_eq!(names(&report.written), vec!["a.mp3", "c.mp3"]);
    assert_eq!(names(&report.changed_on_disk), vec!["b.mp3"]);
    assert_eq!(title_of(&dir, "b.mp3"), one("Rewritten elsewhere"));
    assert_eq!(album_of(&dir, "b.mp3"), None);
    done(&id);
}

/// The app's own write refreshes the file's fingerprint: a second apply isn't refused.
#[test]
fn a_second_apply_after_a_write_goes_through() {
    let (dir, _listing, id) = album("tag-apply-twice");
    let (_store_dir, store) = store("tag-apply-twice-store");
    open(&id, false).expect("opens");
    edit(&id, vec![set_title(&dir, "a.mp3", "Once")]).expect("edits");
    apply_with(&store, &id, request()).expect("applies");
    assert_eq!(unchanged_on_disk(&id, &dir.join("a.mp3")), Ok(true));
    edit(&id, vec![set_title(&dir, "a.mp3", "Twice")]).expect("edits again");

    let report = apply_with(&store, &id, request()).expect("applies again");

    assert_eq!(names(&report.written), vec!["a.mp3"]);
    assert!(report.changed_on_disk.is_empty());
    assert_eq!(title_of(&dir, "a.mp3"), one("Twice"));
    done(&id);
}

/// A `chmod` moves only the ctime, and still counts as a change.
#[test]
fn a_permission_change_counts_as_changed_on_disk() {
    let (dir, _listing, id) = album("tag-apply-ctime");
    open(&id, false).expect("opens");
    // APFS keeps the ctime to the nanosecond, so the chmod right after the read still moves it.
    set_mode(&dir.join("a.mp3"), 0o600);
    assert_eq!(unchanged_on_disk(&id, &dir.join("a.mp3")), Ok(false));
    done(&id);
}

#[test]
fn a_read_only_session_refuses_to_apply() {
    let (_dir, listing) = folder("tag-apply-remote", "smb-share", &[("a.mp3", mp3_frames())]);
    let (_store_dir, store) = store("tag-apply-remote-store");
    let id = open_rename(listing.id(), true, None, 0).expect("opens").session_id;
    open(&id, false).expect("opens read-only");
    assert_eq!(apply_with(&store, &id, request()), Err(TagEditorError::ReadOnly));
    done(&id);
}

fn ensure_root_volume() {
    get_volume_manager().register_if_absent(
        "root",
        Arc::new(LocalPosixVolume::local_folder(
            "Test root",
            "/",
            crate::volume_host::host(),
        )),
    );
}

fn album_mask() -> MultiRenameSpec {
    MultiRenameSpec {
        name_mask: "[=tag.album] [N]".to_string(),
        extension_mask: "[E]".to_string(),
        search: String::new(),
        replace: String::new(),
        case_sensitive: false,
        first_only: false,
        include_extension: false,
        regex: false,
        substitute: false,
        case: CaseChange::Unchanged,
        remove_diacritics: false,
        greek_to_latin: false,
        normalize_unicode: false,
    }
}

fn new_name(id: &str, old_name: &str) -> String {
    let preview = preview_session(id, &album_mask()).expect("a preview");
    preview
        .rows
        .iter()
        .find(|row| row.old_name == old_name)
        .expect("the preview lists the file")
        .new_name
        .clone()
}

async fn media_read(id: &str) {
    wait_until_async(Duration::from_secs(5), "the media read to finish", || {
        media_progress(id).is_ok_and(|progress| progress.done)
    })
    .await;
}

/// Review focus 5: after a write, and after its undo, Multi-Rename's `[=tag.*]` preview shows the
/// new values, and the read reports its end: the event the sheet re-previews on.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn media_cache_dropped_for_written_rows() {
    ensure_root_volume();
    let (dir, _listing, id) = album("tag-apply-media");
    let (_store_dir, store) = store("tag-apply-media-store");
    let ends = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&ends);
    start_media(
        &id,
        Arc::new(move |progress| {
            if progress.done {
                counter.fetch_add(1, Ordering::SeqCst);
            }
        }),
    )
    .expect("the read starts");
    media_read(&id).await;
    assert_eq!(new_name(&id, "a.mp3"), "Book a.mp3");

    open(&id, false).expect("opens");
    edit(
        &id,
        vec![TagEdit::SetOne {
            path: path_of(&dir, "a.mp3"),
            field: TagField::Album,
            values: vec!["Kniha".to_string()],
        }],
    )
    .expect("edits");
    let ends_before_write = ends.load(Ordering::SeqCst);
    let report = apply_with(&store, &id, request()).expect("applies");
    media_read(&id).await;

    assert_eq!(new_name(&id, "a.mp3"), "Kniha a.mp3");
    assert_eq!(new_name(&id, "b.mp3"), "Book b.mp3");
    wait_until_async(Duration::from_secs(5), "the re-read to report its end", || {
        ends.load(Ordering::SeqCst) > ends_before_write
    })
    .await;

    let ends_before_undo = ends.load(Ordering::SeqCst);
    undo_with(
        &store,
        report.apply_id.as_deref().expect("an apply id"),
        Some(id.as_str()),
    )
    .expect("undoes");
    media_read(&id).await;

    assert_eq!(
        new_name(&id, "a.mp3"),
        "Book a.mp3",
        "the undo's old album is back in the preview"
    );
    wait_until_async(Duration::from_secs(5), "the undo's re-read to report its end", || {
        ends.load(Ordering::SeqCst) > ends_before_undo
    })
    .await;
    done(&id);
}

/// Undoing a "Write anyway" apply: the dropped ID3v2.4-only frame can't come back into an
/// ID3v2.3 file, but every other old tag does.
#[test]
fn undo_after_write_anyway_restores_the_other_tags() {
    let (dir, listing) = folder("tag-apply-undo-anyway", "root", &[("a.mp3", mp3_with_v24_only_frame())]);
    let (store_dir, store) = store("tag-apply-undo-anyway-store");
    let id = open_rename(listing.id(), true, None, 0).expect("opens").session_id;
    open(&id, false).expect("opens");
    edit(&id, vec![set_title(&dir, "a.mp3", "Alpha edited"), set_album("Kniha")]).expect("edits");
    let report = apply_with(
        &store,
        &id,
        ApplyRequest {
            allow_frame_loss: true,
            ..request()
        },
    )
    .expect("applies");
    assert_eq!(names(&report.written), vec!["a.mp3"]);
    assert!(
        !open(&id, false).expect("still open").dirty,
        "the session holds what the write left on disk, the dropped frame gone"
    );

    let restored = undo_with(
        &store,
        report.apply_id.as_deref().expect("an apply id"),
        Some(id.as_str()),
    )
    .expect("undoes");

    assert!(restored.failed.is_empty(), "{restored:?}");
    assert_eq!(restored.restored.len(), 1);
    assert_eq!(title_of(&dir, "a.mp3"), one("Alpha"));
    assert_eq!(album_of(&dir, "a.mp3"), None);
    assert_eq!(records(&store_dir), 0);
    done(&id);
}

/// A store that can't narrow the record after the writes: no undo is offered, since the
/// record would also rewrite files the apply never wrote.
#[test]
fn a_record_that_cannot_be_narrowed_is_dropped_and_undo_is_unavailable() {
    let (dir, _listing, _id) = album("tag-apply-narrow-fails");
    let (store_dir, store) = store("tag-apply-narrow-fails-store");
    let never_recorded = cmdr_tags::ApplyId("tags-1-never-recorded".to_string());
    let a = dir.join("a.mp3");
    let fingerprint = cmdr_tags::Fingerprint::of(&a).expect("a stats");
    assert!(!narrow_record(&store, &never_recorded, &[(a, fingerprint)]));
    assert_eq!(records(&store_dir), 0);
}

/// A file that changes between the record and its own write is not written, is reported,
/// and is not in the undo record.
#[test]
fn a_file_changed_right_before_its_write_is_skipped_and_left_out_of_undo() {
    let (dir, _listing, id) = album("tag-apply-toctou");
    let (_store_dir, store) = store("tag-apply-toctou-store");
    open(&id, false).expect("opens");
    edit(&id, vec![set_album("Kniha")]).expect("edits");
    let b = dir.join("b.mp3");
    let meddle = |path: &Path| {
        if path == b {
            tag_file(&dir, "b.mp3", &[(TagField::Title, "Rewritten elsewhere")]);
        }
    };

    let report = apply_with_before_write(&store, &id, request(), &meddle).expect("applies");

    assert_eq!(names(&report.written), vec!["a.mp3", "c.mp3"]);
    assert_eq!(names(&report.changed_on_disk), vec!["b.mp3"]);
    let restored = undo_with(
        &store,
        report.apply_id.as_deref().expect("an apply id"),
        Some(id.as_str()),
    )
    .expect("undoes");
    let restored_names: Vec<String> = restored
        .restored
        .iter()
        .map(|path| path.file_name().expect("a file").to_string_lossy().into_owned())
        .collect();
    assert_eq!(restored_names, vec!["a.mp3", "c.mp3"]);
    assert_eq!(title_of(&dir, "b.mp3"), one("Rewritten elsewhere"));
    done(&id);
}

/// Ruling 20: after the write, a rename swapped two files' names (renumbering after a reorder
/// does this). Undo must not write a's old tags into the file now called a.mp3 (which is b):
/// both are reported as changed since the write and keep what they hold.
#[test]
fn undo_after_a_rename_swapped_two_written_files_touches_neither() {
    let (dir, _listing, id) = album("tag-apply-undo-swap");
    let (store_dir, store) = store("tag-apply-undo-swap-store");
    open(&id, false).expect("opens");
    edit(
        &id,
        vec![
            set_title(&dir, "a.mp3", "Alpha written"),
            set_title(&dir, "b.mp3", "Beta written"),
        ],
    )
    .expect("edits");
    let report = apply_with(&store, &id, request()).expect("applies");
    assert_eq!(names(&report.written), vec!["a.mp3", "b.mp3"]);
    let swap = dir.join("swap.tmp");
    std::fs::rename(dir.join("a.mp3"), &swap).expect("a moves aside");
    std::fs::rename(dir.join("b.mp3"), dir.join("a.mp3")).expect("b takes a's name");
    std::fs::rename(&swap, dir.join("b.mp3")).expect("a takes b's name");

    let restored = undo_with(
        &store,
        report.apply_id.as_deref().expect("an apply id"),
        Some(id.as_str()),
    )
    .expect("undoes");

    assert!(restored.restored.is_empty(), "{restored:?}");
    let changed: Vec<String> = restored
        .changed
        .iter()
        .map(|path| path.file_name().expect("a file").to_string_lossy().into_owned())
        .collect();
    assert_eq!(changed, vec!["a.mp3", "b.mp3"]);
    assert_eq!(
        title_of(&dir, "a.mp3"),
        one("Beta written"),
        "a.mp3 holds b's file, untouched"
    );
    assert_eq!(
        title_of(&dir, "b.mp3"),
        one("Alpha written"),
        "b.mp3 holds a's file, untouched"
    );
    assert_eq!(records(&store_dir), 0, "nothing left to retry, so the record goes");
    done(&id);
}

#[test]
fn apply_keeps_a_track_total_it_did_not_edit() {
    let (dir, _listing, id) = album("tag-apply-keeps-total");
    let (_store_dir, store) = store("tag-apply-keeps-total-store");
    tag_file(
        &dir,
        "a.mp3",
        &[
            (TagField::Title, "Alpha"),
            (TagField::Album, "Book"),
            (TagField::Track, "3"),
            (TagField::TrackTotal, "12"),
        ],
    );
    open(&id, false).expect("opens");
    edit(&id, vec![set_album("Renamed book")]).expect("edits");

    let report = apply_with(&store, &id, request()).expect("applies");

    assert!(names(&report.written).contains(&"a.mp3".to_string()));
    let a = on_disk(&dir, "a.mp3");
    assert_eq!(album_of(&dir, "a.mp3"), one("Renamed book"));
    assert_eq!(a.fields.get(&TagField::Track).cloned(), one("3"));
    assert_eq!(
        a.fields.get(&TagField::TrackTotal).cloned(),
        one("12"),
        "the total survives the write"
    );
    done(&id);
}
