//! Own writes are announced before they exist, on every write path: each test
//! drives one path end to end and pins the exact sequence of
//! `downloads::note_pending_write_for_cmdr` calls under its own `TestDir`, with
//! whether each path existed at that moment (`downloads/own_write_recorder.rs`).
//!
//! A creation announces a path that doesn't exist yet; a removal or a rename's
//! source announces one that still does. The whole sequence is pinned, so an
//! announce moved after its syscall goes red on order or count, not only on the
//! existence bit, and a path that stopped announcing fails on a missing entry.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::archive_edit::{ArchiveEditRequest, archive_edit_start};
use super::state::WriteOperationState;
use super::transfer::move_op::test_support::run_cross_fs_move;
use super::types::{ConflictResolution, VolumeCopyConfig, WriteOperationConfig};
use super::{
    BulkRenameRow, CollectorEventSink, OperationEventSink, OperationSummaryText, SourceFingerprint, copy_files_start,
    copy_volumes_with_progress, create_directory_managed, create_file_managed, delete_files_start, move_files_start,
    move_within_same_volume_with_progress, rename_managed, start_bulk_rename,
};
use crate::downloads::own_write_recorder::{OwnWriteRecording, record_under};
use crate::file_system::volume::{InMemoryVolume, Volume};
use crate::ignore_poison::IgnorePoison;
use crate::test_support::{TestDir, wait_until_async};
use cmdr_archive::mutator::{AddEntry, AddSource, Changeset};
use cmdr_operation_log::types::Initiator;

const WAIT: Duration = Duration::from_secs(5);

/// The recorded sequence as `(path relative to the root, existed)`, with Cmdr's
/// random scratch suffixes (`a.txt.cmdr-tmp-<uuid>`, `.cmdr-staging-<op>`)
/// folded to their kind, so the pin reads as the shape it is.
fn shape(recording: &OwnWriteRecording) -> Vec<(String, bool)> {
    recording
        .announced_relative()
        .into_iter()
        .map(|a| (fold_scratch_names(&a.path), a.existed))
        .collect()
}

fn fold_scratch_names(path: &Path) -> String {
    path.components()
        .map(|c| {
            let name = c.as_os_str().to_string_lossy();
            let folded = name.find(".cmdr-").and_then(|at| {
                let (kind, _) = name[at + ".cmdr-".len()..].split_once('-')?;
                Some(format!("{}.cmdr-{kind}-*", &name[..at]))
            });
            folded.unwrap_or_else(|| name.into_owned())
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Registers a real local-FS `root` volume, so the instant ops with no volume id
/// take the timed `Volume` path production takes. Idempotent.
fn ensure_root_volume() {
    crate::file_system::volume::manager::get_volume_manager().register_if_absent(
        "root",
        Arc::new(cmdr_local::LocalPosixVolume::local_folder(
            "Test root",
            "/",
            crate::volume_host::host(),
        )),
    );
}

fn announced(entries: &[(&str, bool)]) -> Vec<(String, bool)> {
    entries.iter().map(|(p, e)| ((*p).to_string(), *e)).collect()
}

fn fresh_state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(200)))
}

/// A root with `src/` and `dst/`, recording every announce under it.
fn rig(label: &str) -> (TestDir, OwnWriteRecording) {
    let dir = TestDir::new(label);
    fs::create_dir(dir.join("src")).expect("src");
    fs::create_dir(dir.join("dst")).expect("dst");
    let recording = record_under(&dir);
    (dir, recording)
}

async fn wait_settled(events: &CollectorEventSink) {
    wait_until_async(WAIT, "the operation to settle", || {
        !events.settled.lock_ignore_poison().is_empty()
    })
    .await;
    assert!(
        events.errors.lock_ignore_poison().is_empty(),
        "the operation must succeed: {:?}",
        events.errors.lock_ignore_poison()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_local_copy_announces_its_destination_and_temp_before_landing() {
    let (dir, recording) = rig("announce_copy");
    fs::write(dir.join("src/a.txt"), b"a").expect("source");

    let events = Arc::new(CollectorEventSink::new());
    copy_files_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        vec![dir.join("src/a.txt")],
        dir.join("dst"),
        WriteOperationConfig::default(),
        vec![],
        None,
        Initiator::User,
        None,
        None,
    )
    .await
    .expect("start copy");
    wait_settled(&events).await;

    assert!(dir.join("dst/a.txt").exists());
    // The copy loop announces the destination, then `stage_and_land_file`
    // announces its temp and the destination again before the landing rename.
    assert_eq!(
        shape(&recording),
        announced(&[
            ("dst/a.txt", false),
            ("dst/a.txt.cmdr-tmp-*", false),
            ("dst/a.txt", false),
        ])
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_same_fs_move_announces_both_ends_before_the_rename() {
    let (dir, recording) = rig("announce_same_fs_move");
    fs::write(dir.join("src/a.txt"), b"a").expect("source");

    let events = Arc::new(CollectorEventSink::new());
    move_files_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        vec![dir.join("src/a.txt")],
        dir.join("dst"),
        WriteOperationConfig::default(),
        vec![],
        None,
        Initiator::User,
        None,
        None,
    )
    .await
    .expect("start move");
    wait_settled(&events).await;

    assert!(dir.join("dst/a.txt").exists());
    assert_eq!(
        shape(&recording),
        announced(&[("src/a.txt", true), ("dst/a.txt", false)])
    );
}

#[test]
fn a_cross_fs_move_announces_its_final_path_before_the_staged_rename() {
    let (dir, recording) = rig("announce_cross_fs_move");
    fs::write(dir.join("src/a.txt"), b"a").expect("source");

    run_cross_fs_move(
        &[dir.join("src/a.txt")],
        &dir.join("dst"),
        ConflictResolution::Stop,
        "announce-cross-fs",
    )
    .expect("cross-fs move");

    assert!(dir.join("dst/a.txt").exists());
    // The copy phase lands in the staging folder like any copy, then the final
    // name is announced before the staged rename. The source delete announces
    // nothing.
    assert_eq!(
        shape(&recording),
        announced(&[
            ("dst/.cmdr-staging-*/a.txt", false),
            ("dst/.cmdr-staging-*/a.txt.cmdr-tmp-*", false),
            ("dst/.cmdr-staging-*/a.txt", false),
            ("dst/a.txt", false),
        ])
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_delete_announces_each_file_then_its_folder_while_they_still_exist() {
    let (dir, recording) = rig("announce_delete");
    fs::create_dir(dir.join("src/folder")).expect("folder");
    fs::write(dir.join("src/folder/a.txt"), b"a").expect("file");

    let events = Arc::new(CollectorEventSink::new());
    delete_files_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        vec![dir.join("src/folder")],
        WriteOperationConfig::default(),
        None,
        Initiator::User,
        None,
    )
    .await
    .expect("start delete");
    wait_settled(&events).await;

    assert!(!dir.join("src/folder").exists());
    assert_eq!(
        shape(&recording),
        announced(&[("src/folder/a.txt", true), ("src/folder", true)])
    );
}

#[cfg(target_os = "macos")]
#[test]
fn a_trash_announces_each_item_while_it_still_exists() {
    let (dir, recording) = rig("announce_trash");
    fs::write(dir.join("src/a.txt"), b"a").expect("file");

    let events = CollectorEventSink::new();
    super::delete::trash::trash_files_with_progress(
        &events,
        "announce-trash",
        &fresh_state(),
        &[dir.join("src/a.txt")],
        None,
    )
    .expect("trash");

    assert!(!dir.join("src/a.txt").exists());
    assert_eq!(shape(&recording), announced(&[("src/a.txt", true)]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rename_announces_both_ends_before_the_syscall() {
    let (dir, recording) = rig("announce_rename");
    fs::write(dir.join("src/a.txt"), b"a").expect("file");

    rename_managed(
        dir.join("src/a.txt"),
        dir.join("src/b.txt"),
        false,
        "root".to_string(),
        Initiator::User,
    )
    .await
    .expect("rename");

    assert!(dir.join("src/b.txt").exists());
    assert_eq!(
        shape(&recording),
        announced(&[("src/a.txt", true), ("src/b.txt", false)])
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_folder_and_a_new_file_are_announced_before_they_exist() {
    ensure_root_volume();
    let (dir, recording) = rig("announce_create");
    let parent = dir.join("dst").to_string_lossy().into_owned();

    create_directory_managed(None, parent.clone(), "new folder".to_string(), Initiator::User)
        .await
        .expect("mkdir");
    create_file_managed(None, parent, "new.txt".to_string(), Initiator::User)
        .await
        .expect("mkfile");

    assert!(dir.join("dst/new folder").is_dir());
    assert!(dir.join("dst/new.txt").is_file());
    assert_eq!(
        shape(&recording),
        announced(&[("dst/new folder", false), ("dst/new.txt", false)])
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bulk_rename_announces_both_ends_of_every_hop() {
    let (dir, recording) = rig("announce_bulk_rename");
    let row = |id: &str, from: &str, to: &str| {
        let source = dir.join("src").join(from);
        fs::write(&source, id.as_bytes()).expect("source");
        BulkRenameRow {
            row_id: id.to_string(),
            expected_fingerprint: SourceFingerprint::capture_local(&source).expect("fingerprint"),
            source,
            destination: dir.join("src").join(to),
        }
    };
    let rows = vec![row("1", "a.txt", "x.txt"), row("2", "b.txt", "y.txt")];

    let events = Arc::new(CollectorEventSink::new());
    start_bulk_rename(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        "root".to_string(),
        rows,
        Initiator::User,
    )
    .expect("start bulk rename");
    wait_settled(&events).await;

    assert!(dir.join("src/x.txt").exists() && dir.join("src/y.txt").exists());
    assert_eq!(
        shape(&recording),
        announced(&[
            ("src/a.txt", true),
            ("src/x.txt", false),
            ("src/b.txt", true),
            ("src/y.txt", false),
        ])
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_archive_edit_announces_its_temp_then_the_archive_it_replaces() {
    let (dir, recording) = rig("announce_archive_edit");
    let archive = dir.join("dst/a.zip");
    {
        use std::io::Write as _;
        let mut writer = zip::ZipWriter::new(fs::File::create(&archive).expect("zip"));
        writer
            .start_file("keep.txt", zip::write::SimpleFileOptions::default())
            .expect("entry");
        writer.write_all(b"keep").expect("bytes");
        writer.finish().expect("finish");
    }

    let events = Arc::new(CollectorEventSink::new());
    let request = ArchiveEditRequest {
        archive_path: archive.clone(),
        // A unique parent keeps the op off every shared lane.
        parent_volume_id: format!("announce-archive-{}", uuid::Uuid::new_v4()),
        changeset: Changeset {
            adds: vec![AddEntry {
                inner_path: "added.txt".to_string(),
                source: AddSource::Bytes(b"new".to_vec()),
            }],
            ..Default::default()
        },
        summary: OperationSummaryText::default(),
        skipped_count: 0,
        preview_id: None,
    };
    archive_edit_start(Arc::clone(&events) as Arc<dyn OperationEventSink>, request, 0)
        .await
        .expect("start archive edit");
    wait_settled(&events).await;

    // The mutator builds a sibling temp, then announces the archive it is about
    // to replace before the swap rename.
    assert_eq!(
        shape(&recording),
        announced(&[("dst/a.zip.cmdr-tmp-*", false), ("dst/a.zip", true)])
    );
}

/// The volume engine landing on a local disk (`staged_write::note_pending_for_local_dest`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_volume_copy_onto_a_local_disk_announces_its_destination_and_temp() {
    let (dir, recording) = rig("announce_volume_copy");
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    source.create_file(Path::new("/a.txt"), b"a").await.expect("source");
    let dest: Arc<dyn Volume> = Arc::new(cmdr_local::LocalPosixVolume::local_folder(
        "Dest",
        dir.join("dst"),
        crate::volume_host::host(),
    ));

    copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "announce-volume-copy",
        &fresh_state(),
        source,
        &[PathBuf::from("/a.txt")],
        dest,
        Path::new("/"),
        &VolumeCopyConfig::default(),
    )
    .await
    .expect("volume copy");

    assert!(dir.join("dst/a.txt").exists());
    // The final name, then the temp the bytes stream into; the landing rename
    // carries no third announce.
    assert_eq!(
        shape(&recording),
        announced(&[("dst/a.txt", false), ("dst/a.txt.cmdr-tmp-*", false)])
    );
}

/// The volume engine's same-volume move on a local disk
/// (`move_same::note_pending_for_local_volume`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_same_volume_move_on_a_local_disk_announces_both_ends() {
    let (dir, recording) = rig("announce_volume_move");
    fs::write(dir.join("src/a.txt"), b"a").expect("source");
    let volume: Arc<dyn Volume> = Arc::new(cmdr_local::LocalPosixVolume::local_folder(
        "Local",
        dir.to_path_buf(),
        crate::volume_host::host(),
    ));

    move_within_same_volume_with_progress(
        Arc::new(CollectorEventSink::new()),
        "announce-volume-move",
        &fresh_state(),
        volume,
        &[PathBuf::from("/src/a.txt")],
        Path::new("/dst"),
        &VolumeCopyConfig::default(),
    )
    .await
    .expect("same-volume move");

    assert!(dir.join("dst/a.txt").exists());
    assert_eq!(
        shape(&recording),
        announced(&[("src/a.txt", true), ("dst/a.txt", false)])
    );
}
