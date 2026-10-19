//! What a file→file Overwrite does today at each of the five places a
//! safe-replace lands, so a restructure of that landing can prove it changed
//! nothing outside the case it meant to change.
//!
//! The grid is site × outcome. Sites: the serial driver (`copy_serial.rs`), the
//! concurrent driver (`copy_concurrent_task.rs`), a deep merge child
//! (`merge.rs`), a cross-volume move (`move_cross.rs`), and a one-pass extract
//! out of a compressed tar (`sequential_extract.rs`). Outcomes:
//!
//! - **lands**: the original holds the new bytes, the journal records the
//!   ORIGINAL's path (never a temp) and calls the op not rollbackable, Locked
//!   goes on at the original's name, and no `.cmdr-tmp-*` is left.
//! - **the write fails**: the original is untouched and no temp is left.
//! - **the delete of the original is refused**: the original is untouched, the
//!   temp is taken away, and the refusal arrives typed.
//! - **the delete went through, then the rename was refused**: the new bytes
//!   are rescued to ` (recovered)`, and the failure names that path
//!   (`NewDataKeptAt`).
//!
//! Every cell also asserts through `safety_oracle.rs`, with a dest-only sibling
//! beside the replaced file: a failed replace never takes a byte from either
//! side, and never touches what the source doesn't shadow.
//!
//! One cell pins a KNOWN BUG as it behaves today:
//! `a_concurrent_task_dropped_mid_swap_has_its_temp_cleaned_as_a_partial`.
//!
//! Older pins of single cells stay where they are (`copy_crashsafe_tests.rs`,
//! `move_failure_tests.rs`, `finalize_recovery_tests.rs`, `copy_flags_tests.rs`,
//! `journal_capture_volume_tests.rs`); this grid is the one place all five sites
//! answer the same four questions.

use std::future::Future;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use cmdr_archive::{ArchiveFormat, ArchiveVolume, TarCodec};
use cmdr_fs::ignore_poison::IgnorePoison as _;
use cmdr_fs::metadata::{CarriedMetadata, UserFlags};
use cmdr_fs::testing::TestDir;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::{InMemoryVolume, Volume, VolumeError, VolumeReadStream};
use cmdr_operation_log::store::{open_read_connection, read_operation};
use cmdr_operation_log::types::{EntryType, ExecutionStatus, Initiator, OpKind, RollbackState};
use tokio::sync::Notify;

use super::faulty_volume::{FaultyOp, FaultyVolume, forward_volume_methods};
use super::safety_oracle::{SafetySpec, assert_operation_was_safe};
use crate::event_sinks::CollectorEventSink;
use crate::journal;
use crate::testing::{TestOperationLog, TestOperations};
use crate::types::{ConflictResolution, VolumeCopyConfig, WriteOperationError};

const OLD: &[u8] = b"the user's notes, before";
const NEW: &[u8] = b"the new notes from the source";
const KEEP: &[u8] = b"a dest-only file the source never shadows";
const FRESH: &[u8] = b"a source-only file, new to the destination";
const SIBLING_A: &[u8] = b"sibling a, new to the destination";
const SIBLING_Z: &[u8] = b"sibling z, new to the destination";

/// The five places a safe-replace lands.
#[derive(Clone, Copy, Debug)]
enum Site {
    /// One source file, so the serial driver runs it.
    Serial,
    /// Three source files, so the concurrent driver runs them.
    Concurrent,
    /// A folder merging into the user's folder: the clash is a deep child.
    MergeChild,
    /// One source file, moved.
    Move,
    /// A folder out of a `.tar.gz`, merging into the user's folder.
    Extract,
}

impl Site {
    const ALL: [Site; 5] = [
        Site::Serial,
        Site::Concurrent,
        Site::MergeChild,
        Site::Move,
        Site::Extract,
    ];

    /// The user's file the Overwrite replaces.
    fn orig(self) -> &'static str {
        match self {
            Site::Serial | Site::Concurrent | Site::Move => "/dest/notes.txt",
            Site::MergeChild | Site::Extract => "/dest/album/notes.txt",
        }
    }

    /// Where the rescue puts the new bytes when the swap's rename is refused.
    fn recovered(self) -> &'static str {
        match self {
            Site::Serial | Site::Concurrent | Site::Move => "/dest/notes (recovered).txt",
            Site::MergeChild | Site::Extract => "/dest/album/notes (recovered).txt",
        }
    }

    /// The source file whose bytes replace `orig`, on an in-memory source.
    fn source_notes(self) -> &'static str {
        match self {
            Site::Serial | Site::Concurrent | Site::Move => "/notes.txt",
            Site::MergeChild | Site::Extract => "/album/notes.txt",
        }
    }

    /// The archive and its flags don't travel: a tar member reports no flags.
    fn carries_flags(self) -> bool {
        !matches!(self, Site::Extract)
    }

    fn kind(self) -> OpKind {
        match self {
            Site::Move => OpKind::Move,
            _ => OpKind::Copy,
        }
    }
}

/// The fault a cell arms, if any.
#[derive(Clone, Copy, Debug)]
enum Arm {
    Nothing,
    /// The write of the replacing file fails: its source's first read on an
    /// in-memory source, the destination's first write for the extract (whose
    /// archive order puts `notes.txt` first).
    WriteFails,
    /// The destination refuses `delete(orig)`, and the original stays.
    DeleteOfOrigRefused,
    /// The destination refuses the rename onto `orig` (the delete before it
    /// went through); the rescue's rename elsewhere forwards.
    RenameOntoOrigRefused,
}

fn refusal(what: &str) -> VolumeError {
    VolumeError::PermissionDenied {
        path: what.to_string(),
        raw_os_error: None,
    }
}

/// A finished cell: the result, both sides, and the journal it wrote.
struct Ran {
    site: Site,
    result: Result<(), WriteOperationError>,
    source: Arc<dyn Volume>,
    /// The in-memory source under any wrapper; `None` for the archive.
    source_inner: Option<Arc<InMemoryVolume>>,
    dest: Arc<FaultyVolume<InMemoryVolume>>,
    dest_inner: Arc<InMemoryVolume>,
    source_faulty: Option<Arc<FaultyVolume<InMemoryVolume>>>,
    /// The archive's root on its volume, for the oracle's source walk.
    archive_root: Option<String>,
    log: TestOperationLog,
    op_id: String,
    /// Held for its `Drop`: it owns the `.tar.gz` the archive volume reads.
    _archive: Option<TarGzFixture>,
}

/// Runs one Overwrite at `site` with `arm`'s fault armed, journaled the way the
/// app journals a volume op.
async fn run(site: Site, arm: Arm) -> Ran {
    let op_id = format!("op-safe-replace-{site:?}-{arm:?}");
    let log = TestOperationLog::new("safe-replace-grid");
    let engine = TestOperations::journaling_into(&log);

    // The destination: the user's file, and a dest-only sibling.
    let dest_inner = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));
    dest_inner.create_directory(Path::new("/dest")).await.unwrap();
    if matches!(site, Site::MergeChild | Site::Extract) {
        dest_inner.create_directory(Path::new("/dest/album")).await.unwrap();
        dest_inner
            .create_file(Path::new("/dest/album/keep.txt"), KEEP)
            .await
            .unwrap();
    } else {
        dest_inner.create_file(Path::new("/dest/keep.txt"), KEEP).await.unwrap();
    }
    dest_inner.create_file(Path::new(site.orig()), OLD).await.unwrap();
    let mut dest = FaultyVolume::wrapping(Arc::clone(&dest_inner));
    dest = match (arm, site) {
        (Arm::WriteFails, Site::Extract) => {
            dest.failing_call(FaultyOp::WriteFromStream, 1, refusal("simulated write refusal"))
        }
        (Arm::DeleteOfOrigRefused, _) => {
            dest.failing_call_at(FaultyOp::Delete, site.orig(), 1, refusal("simulated delete refusal"))
        }
        (Arm::RenameOntoOrigRefused, _) => {
            dest.failing_call_at(FaultyOp::Rename, site.orig(), 1, refusal("simulated rename refusal"))
        }
        _ => dest,
    };
    let dest = dest.arc();

    // The source.
    let (source, source_inner, source_faulty, sources, archive) = if let Site::Extract = site {
        let archive = TarGzFixture::new(&[("album/notes.txt", NEW), ("album/fresh.txt", FRESH)]);
        let source = archive.volume();
        let sources = vec![archive.inner("album")];
        (source, None, None, sources, Some(archive))
    } else {
        let inner = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
        let sources = match site {
            Site::Serial | Site::Move => vec![PathBuf::from("/notes.txt")],
            Site::Concurrent => {
                inner.create_file(Path::new("/a.txt"), SIBLING_A).await.unwrap();
                inner.create_file(Path::new("/z.txt"), SIBLING_Z).await.unwrap();
                vec![
                    PathBuf::from("/a.txt"),
                    PathBuf::from("/notes.txt"),
                    PathBuf::from("/z.txt"),
                ]
            }
            Site::MergeChild => {
                inner.create_directory(Path::new("/album")).await.unwrap();
                inner.create_file(Path::new("/album/fresh.txt"), FRESH).await.unwrap();
                vec![PathBuf::from("/album")]
            }
            Site::Extract => unreachable!("handled above"),
        };
        inner.create_file(Path::new(site.source_notes()), NEW).await.unwrap();
        inner.set_carried_metadata(
            Path::new(site.source_notes()),
            CarriedMetadata {
                flags: UserFlags::LOCKED,
                ..CarriedMetadata::default()
            },
        );
        let mut faulty = FaultyVolume::wrapping(Arc::clone(&inner));
        if let Arm::WriteFails = arm {
            faulty = faulty.failing_call_at(
                FaultyOp::ReadStream,
                site.source_notes(),
                1,
                refusal("simulated read refusal"),
            );
        }
        let faulty = faulty.arc();
        (
            Arc::clone(&faulty) as Arc<dyn Volume>,
            Some(inner),
            Some(faulty),
            sources,
            None,
        )
    };

    let state = Arc::new(
        engine
            .new_operation_state(Duration::from_millis(50))
            .with_journal_volumes("src-vol".into(), "dst-vol".into()),
    );
    journal::open_volume_op(
        state.scope(),
        &op_id,
        site.kind(),
        Initiator::User,
        "src-vol",
        Some("dst-vol"),
        0,
    );
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        ..VolumeCopyConfig::default()
    };
    let events = Arc::new(CollectorEventSink::new());
    let dest_dyn = Arc::clone(&dest) as Arc<dyn Volume>;
    let result = match site {
        Site::Move => {
            super::move_cross::move_volumes_with_progress(
                events,
                &op_id,
                &state,
                Arc::clone(&source),
                &sources,
                dest_dyn,
                Path::new("/dest"),
                &config,
            )
            .await
        }
        _ => {
            super::copy::copy_volumes_with_progress(
                events,
                &op_id,
                &state,
                Arc::clone(&source),
                &sources,
                dest_dyn,
                Path::new("/dest"),
                &config,
            )
            .await
        }
    }
    .map_err(|failure| failure.error);
    if result.is_ok() {
        journal::finalize_op(state.scope(), site.kind(), ExecutionStatus::Done);
    }

    Ran {
        site,
        result,
        source,
        source_inner,
        dest,
        dest_inner,
        source_faulty,
        archive_root: archive.as_ref().map(|a| a.path.display().to_string()),
        log,
        op_id,
        _archive: archive,
    }
}

impl Ran {
    fn label(&self) -> String {
        format!("{:?}", self.site)
    }

    async fn dest_file(&self, path: &str) -> Option<Vec<u8>> {
        read_all(&*self.dest_inner, Path::new(path)).await
    }

    /// Every `.cmdr-tmp-*` anywhere under the destination.
    async fn temps_left(&self) -> Vec<String> {
        let mut found = Vec::new();
        let mut dirs = vec![PathBuf::from("/dest")];
        while let Some(dir) = dirs.pop() {
            for entry in self.dest_inner.list_directory(&dir, None).await.unwrap() {
                if entry.is_directory {
                    dirs.push(PathBuf::from(&entry.path));
                } else if entry.name.contains(".cmdr-tmp-") {
                    found.push(entry.path);
                }
            }
        }
        found
    }

    /// Whether the moved file is still at the source (only asked of `Move`).
    async fn source_still_holds_the_file(&self) -> bool {
        let inner = self.source_inner.as_ref().expect("an in-memory source");
        read_all(&**inner, Path::new(self.site.source_notes())).await.as_deref() == Some(NEW)
    }

    /// The three oracle clauses. `delivered` holds only where the op finished.
    async fn assert_safe(&self, delivered: bool) {
        let label = self.label();
        let (source_root, dest_root) = match (self.site, &self.archive_root) {
            (Site::Extract, Some(root)) => (root.as_str(), "/dest"),
            _ => ("/", "/dest"),
        };
        let mut source_files: Vec<(&str, &[u8])> = vec![("/notes.txt", NEW)];
        let mut deliveries: Vec<(&str, &[u8])> = Vec::new();
        let untouched: Vec<(&str, &[u8])>;
        match self.site {
            Site::Serial | Site::Move => {
                untouched = vec![("/keep.txt", KEEP)];
                deliveries.push(("/notes.txt", NEW));
            }
            Site::Concurrent => {
                source_files.extend([("/a.txt", SIBLING_A), ("/z.txt", SIBLING_Z)]);
                untouched = vec![("/keep.txt", KEEP)];
                deliveries.extend([("/notes.txt", NEW), ("/a.txt", SIBLING_A), ("/z.txt", SIBLING_Z)]);
            }
            Site::MergeChild | Site::Extract => {
                source_files.push(("/album/fresh.txt", FRESH));
                untouched = vec![("/album/keep.txt", KEEP)];
                deliveries.extend([("/album/notes.txt", NEW), ("/album/fresh.txt", FRESH)]);
            }
        }
        assert_operation_was_safe(
            &self.source,
            &(Arc::clone(&self.dest) as Arc<dyn Volume>),
            &SafetySpec {
                label: &label,
                source_root,
                dest_root,
                source_files: &source_files,
                delivered: if delivered { &deliveries } else { &[] },
                untouched_dest: &untouched,
            },
        )
        .await;
    }

    /// The journal's word on the op, and every file path it recorded.
    fn journal(&self) -> (RollbackState, Vec<String>) {
        let conn = open_read_connection(self.log.db()).expect("read conn");
        let row = read_operation(&conn, &self.op_id).expect("read").expect("op row");
        let detail = cmdr_operation_log::query::get_operation(&conn, &self.op_id, 1_000, 0)
            .expect("read detail")
            .expect("the op is journaled");
        let files = detail
            .items
            .iter()
            .filter(|i| i.entry_type == EntryType::File)
            .filter_map(|i| i.dest_path.clone())
            .collect();
        (row.rollback_state, files)
    }
}

async fn read_all(volume: &dyn Volume, path: &Path) -> Option<Vec<u8>> {
    let mut stream = volume.open_read_stream(path).await.ok()?;
    let mut buf = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        buf.extend_from_slice(&chunk.ok()?);
    }
    Some(buf)
}

/// A `.tar.gz` on disk with an `ArchiveVolume` over it (a compressed tar is
/// sequential, so it takes the one-pass extract). Members land in the order
/// given.
struct TarGzFixture {
    /// Held for its `Drop`: it owns the directory `path` lives in.
    _dir: TestDir,
    path: PathBuf,
}

impl TarGzFixture {
    fn new(files: &[(&str, &[u8])]) -> Self {
        let mut builder = ::tar::Builder::new(Vec::new());
        for (name, data) in files {
            let mut header = ::tar::Header::new_ustar();
            header.set_mtime(1_700_000_000);
            header.set_mode(0o644);
            header.set_size(data.len() as u64);
            header.set_entry_type(::tar::EntryType::Regular);
            header.set_cksum();
            builder.append_data(&mut header, name, *data).expect("append file");
        }
        let plain = builder.into_inner().expect("finish tar");
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(&plain).expect("gz write");
        let dir = TestDir::new("safe-replace-grid");
        let path = dir.join("fixture.tar.gz");
        std::fs::write(&path, gz.finish().expect("gz finish")).expect("write fixture");
        Self { _dir: dir, path }
    }

    fn volume(&self) -> Arc<dyn Volume> {
        let parent: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("parent").with_local_fs_access());
        Arc::new(ArchiveVolume::new(
            parent,
            self.path.clone(),
            ArchiveFormat::Tar(TarCodec::Gzip),
            VolumeHost::detached(),
        ))
    }

    fn inner(&self, rel: &str) -> PathBuf {
        self.path.join(rel)
    }
}

// ── The grid ────────────────────────────────────────────────────────────────

/// The Overwrite lands: new bytes at the original's name, journaled at that
/// name and not rollbackable, Locked on the original's name, no temp.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_site_lands_the_new_bytes_at_the_originals_name() {
    for site in Site::ALL {
        let ran = run(site, Arm::Nothing).await;
        let label = ran.label();
        assert!(ran.result.is_ok(), "{label}: the overwrite lands, got {:?}", ran.result);
        assert_eq!(ran.dest_file(site.orig()).await.as_deref(), Some(NEW), "{label}");
        assert_eq!(ran.temps_left().await, Vec::<String>::new(), "{label}: no temp left");
        if site.carries_flags() {
            assert_eq!(
                ran.dest_inner
                    .carried_metadata(Path::new(site.orig()))
                    .map(|carried| carried.flags),
                Some(UserFlags::LOCKED),
                "{label}: Locked goes on at the original's name"
            );
        }
        let (rollback, files) = ran.journal();
        assert_eq!(
            rollback,
            RollbackState::NotRollbackable,
            "{label}: an op that overwrote can't roll back"
        );
        assert!(
            files.iter().any(|f| f == site.orig()),
            "{label}: the journal records the original's path, got {files:?}"
        );
        assert!(
            !files.iter().any(|f| f.contains(".cmdr-tmp-")),
            "{label}: the journal never names a temp, got {files:?}"
        );
        if let Site::Move = site {
            assert!(
                !ran.source_still_holds_the_file().await,
                "{label}: a move takes the source"
            );
        }
        ran.assert_safe(true).await;
    }
}

/// The write fails: the original is untouched and no temp is left.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_write_leaves_the_original_and_no_temp_at_every_site() {
    for site in Site::ALL {
        let ran = run(site, Arm::WriteFails).await;
        let label = ran.label();
        let fired = match &ran.source_faulty {
            Some(source) => source.fault_fired(FaultyOp::ReadStream),
            None => ran.dest.fault_fired(FaultyOp::WriteFromStream),
        };
        assert!(fired, "{label}: the write fault fired");
        assert!(ran.result.is_err(), "{label}: a failed write fails the op");
        assert_eq!(ran.dest_file(site.orig()).await.as_deref(), Some(OLD), "{label}");
        assert_eq!(ran.temps_left().await, Vec::<String>::new(), "{label}: no temp left");
        if let Site::Move = site {
            assert!(ran.source_still_holds_the_file().await, "{label}: the source stays");
        }
        ran.assert_safe(false).await;
    }
}

/// The destination refuses `delete(orig)` and keeps it: the original is
/// untouched, the temp is taken away at once, and the refusal arrives typed,
/// naming the original.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_delete_of_the_original_keeps_it_and_takes_the_temp_away_at_every_site() {
    for site in Site::ALL {
        let ran = run(site, Arm::DeleteOfOrigRefused).await;
        let label = ran.label();
        assert!(
            ran.dest.fault_fired(FaultyOp::Delete),
            "{label}: the delete fault fired"
        );
        match &ran.result {
            Err(WriteOperationError::PermissionDenied { path, .. }) => {
                assert_eq!(path, site.orig(), "{label}: the refusal names the original");
            }
            other => panic!("{label}: the refusal arrives typed, got {other:?}"),
        }
        assert_eq!(ran.dest_file(site.orig()).await.as_deref(), Some(OLD), "{label}");
        assert_eq!(ran.temps_left().await, Vec::<String>::new(), "{label}: no temp left");
        if let Site::Move = site {
            assert!(ran.source_still_holds_the_file().await, "{label}: the source stays");
        }
        ran.assert_safe(false).await;
    }
}

/// The delete of the original went through and the rename onto its name was
/// refused: the new bytes leave temp space for ` (recovered)`, and the failure
/// names where they went.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_rename_after_the_delete_rescues_the_new_bytes_at_every_site() {
    for site in Site::ALL {
        let ran = run(site, Arm::RenameOntoOrigRefused).await;
        let label = ran.label();
        assert!(
            ran.dest.fault_fired(FaultyOp::Rename),
            "{label}: the rename fault fired"
        );
        match &ran.result {
            Err(WriteOperationError::NewDataKeptAt { path, kept_at, .. }) => {
                assert_eq!(path, site.orig(), "{label}");
                assert_eq!(kept_at, site.recovered(), "{label}: the failure names the rescue");
            }
            other => panic!("{label}: the failure names where the new bytes went, got {other:?}"),
        }
        assert_eq!(ran.dest_file(site.orig()).await, None, "{label}: the original is gone");
        assert_eq!(
            ran.dest_file(site.recovered()).await.as_deref(),
            Some(NEW),
            "{label}: the rescue holds the new bytes"
        );
        assert_eq!(ran.temps_left().await, Vec::<String>::new(), "{label}: no temp left");
        if let Site::Move = site {
            assert!(ran.source_still_holds_the_file().await, "{label}: the source stays");
        }
        ran.assert_safe(false).await;
    }
}

// ── The known bug ───────────────────────────────────────────────────────────

/// A destination that holds the rename onto `held_onto` forever, after saying
/// it got there: the swap's `delete(orig)` went through, and the task sits
/// between it and the rename.
struct HoldsTheSwapRename {
    inner: Arc<InMemoryVolume>,
    held_onto: PathBuf,
    reached: Arc<Notify>,
    /// The temp the held rename would have landed, and what it held then.
    held_temp: StdMutex<Option<(PathBuf, Option<Vec<u8>>)>>,
}

impl Volume for HoldsTheSwapRename {
    forward_volume_methods!(inner =>
        name, root, lane_key, list_directory, get_metadata, exists, is_directory, create_file,
        create_directory, create_directory_all, delete, get_space_info, supports_streaming,
        supports_export, operations_are_local, max_concurrent_ops, scan_for_copy, scan_for_copy_batch,
        scan_for_conflicts, open_read_stream, supports_unknown_length_writes, write_from_stream,
        write_is_single_shot, apply_metadata, metadata_caps, folder_metadata,
    );
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        if to != self.held_onto {
            return self.inner.rename(from, to, force);
        }
        Box::pin(async move {
            let held = read_all(&*self.inner, from).await;
            *self.held_temp.lock_ignore_poison() = Some((from.to_path_buf(), held));
            self.reached.notify_one();
            std::future::pending().await
        })
    }
}

/// A source whose read of `fails` waits until the swap is held, then refuses:
/// the sibling's failure that makes the concurrent driver drop its window.
struct FailsOnceTheSwapIsHeld {
    inner: Arc<InMemoryVolume>,
    fails: PathBuf,
    reached: Arc<Notify>,
}

impl Volume for FailsOnceTheSwapIsHeld {
    forward_volume_methods!(inner =>
        name, root, lane_key, list_directory, get_metadata, exists, is_directory, get_space_info,
        supports_streaming, supports_export, operations_are_local, max_concurrent_ops, scan_for_copy,
        scan_for_copy_batch, metadata_caps,
    );
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        if path != self.fails {
            return self.inner.open_read_stream(path);
        }
        Box::pin(async move {
            self.reached.notified().await;
            Err(refusal("simulated read refusal"))
        })
    }
}

/// ❗ KNOWN BUG, pinned as it behaves today. A top-level concurrent
/// safe-replace task sits between `delete(orig)` and its rename when a sibling
/// fails; the driver drops its window, and the post-loop's
/// `clean_partial_writes` deletes the task's temp, which it still lists as a
/// partial (`in_flight_partials`). The destination ends with neither the
/// original nor the new file, and the failure names the sibling. The source
/// still holds the bytes, so the user can copy again; flip this cell when the
/// temp stops being cleaned as a partial mid-landing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_concurrent_task_dropped_mid_swap_has_its_temp_cleaned_as_a_partial() {
    let engine = TestOperations::new();
    let reached = Arc::new(Notify::new());
    let source_inner = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    for (name, bytes) in [("/a.txt", SIBLING_A), ("/notes.txt", NEW), ("/z.txt", SIBLING_Z)] {
        source_inner.create_file(Path::new(name), bytes).await.unwrap();
    }
    let source = Arc::new(FailsOnceTheSwapIsHeld {
        inner: Arc::clone(&source_inner),
        fails: PathBuf::from("/z.txt"),
        reached: Arc::clone(&reached),
    });
    let dest_inner = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));
    dest_inner.create_directory(Path::new("/dest")).await.unwrap();
    dest_inner.create_file(Path::new("/dest/notes.txt"), OLD).await.unwrap();
    let dest = Arc::new(HoldsTheSwapRename {
        inner: Arc::clone(&dest_inner),
        held_onto: PathBuf::from("/dest/notes.txt"),
        reached,
        held_temp: StdMutex::new(None),
    });

    let result = tokio::time::timeout(
        Duration::from_secs(30),
        super::copy::copy_volumes_with_progress(
            Arc::new(CollectorEventSink::new()),
            "op-safe-replace-dropped-mid-swap",
            &Arc::new(engine.new_operation_state(Duration::from_millis(50))),
            Arc::clone(&source) as Arc<dyn Volume>,
            &[
                PathBuf::from("/a.txt"),
                PathBuf::from("/notes.txt"),
                PathBuf::from("/z.txt"),
            ],
            Arc::clone(&dest) as Arc<dyn Volume>,
            Path::new("/dest"),
            &VolumeCopyConfig {
                conflict_resolution: ConflictResolution::Overwrite,
                ..VolumeCopyConfig::default()
            },
        ),
    )
    .await
    .expect("the driver drops the held task instead of waiting on it");

    let (temp, held) = dest
        .held_temp
        .lock_ignore_poison()
        .take()
        .expect("the swap reached its held rename");
    assert_eq!(
        held.as_deref(),
        Some(NEW),
        "the temp held the whole new file at the hold"
    );
    match result {
        Err(failure) => match failure.error {
            WriteOperationError::PermissionDenied { path, .. } => {
                assert_eq!(path, "/z.txt", "the failure names the sibling");
            }
            other => panic!("the sibling's refusal ends the op, got {other:?}"),
        },
        Ok(()) => panic!("a sibling's failure fails the op"),
    }
    assert_eq!(
        read_all(&*dest_inner, Path::new("/dest/notes.txt")).await,
        None,
        "the swap's delete went through"
    );
    assert_eq!(
        read_all(&*dest_inner, &temp).await,
        None,
        "❗ BUG: the post-loop cleaned the committed temp as a partial"
    );
    let names: Vec<String> = dest_inner
        .list_directory(Path::new("/dest"), None)
        .await
        .unwrap()
        .into_iter()
        .map(|entry| entry.name)
        .filter(|name| name.starts_with("notes"))
        .collect();
    assert_eq!(
        names,
        Vec::<String>::new(),
        "❗ BUG: neither the original nor the new bytes are left at the destination"
    );
    assert_eq!(
        read_all(&*source_inner, Path::new("/notes.txt")).await.as_deref(),
        Some(NEW),
        "the source still holds the new bytes"
    );
}
