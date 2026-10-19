//! A file→file Overwrite whose swap landing meets a dropped share, at each of
//! the five sites a safe-replace lands (both drivers, a deep merge child, a
//! cross-volume move, and a one-pass extract), through the real pipeline.
//!
//! From the moment the swap's `delete(orig)` goes through, the temp is the
//! destination's only copy of the file. So a drop at the delete or at the
//! rename after it waits for the share and SETTLES what happened before acting:
//! a whole temp lands without the file streaming again, a gone temp counts as
//! landed only when the name holds exactly the source's bytes, and a wait that
//! ends without the share keeps the new bytes, recorded as a landing for the
//! next sweep that reaches it, ❌ never cleaned as a partial.
//!
//! The landing alone, without a wait, is `swap_landing_tests.rs`; the same
//! sites without an outage are `safe_replace_grid_tests.rs`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::ignore_poison::IgnorePoison as _;
use cmdr_fs::testing::wait_until_async;
use cmdr_fs::volume::{InMemoryVolume, Volume};

use super::safe_replace_grid_tests::TarGzFixture;
use super::safety_oracle::{SafetySpec, assert_operation_was_safe};
use crate::event_sinks::CollectorEventSink;
use crate::in_flight_temps::Ledger;
use crate::outage_slot::OutagePolicy;
use crate::state::WriteOperationState;
use crate::testing::TestOperations;
use crate::transfer::outage_volume_test_support::{OutageVolume, RenameDrop};
use crate::transfer::staged_write::{LandingError, Resumed, StagedWrite, WriteStaging};
use crate::types::{ConflictResolution, VolumeCopyConfig, WriteOperationError};

/// The user's file the Overwrite replaces, and the new one: the SAME size, so
/// only a byte compare can tell them apart.
const OLD: &[u8] = b"the user's notes, before";
const NEW: &[u8] = b"the new notes, from here";
const KEEP: &[u8] = b"a dest-only file the source never shadows";
const FRESH: &[u8] = b"a source-only file, new to the destination";
const SIBLING_A: &[u8] = b"sibling a, new to the destination";
const SIBLING_Z: &[u8] = b"sibling z, new to the destination";

/// The volume id the destination's records name.
const NAS_ID: &str = "swap-outage-nas";
const WAIT: Duration = Duration::from_secs(10);
const CADENCE: &[Duration] = &[Duration::from_millis(10), Duration::from_millis(20)];

/// The five places a safe-replace lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Site {
    Serial,
    Concurrent,
    MergeChild,
    Move,
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

    fn orig(self) -> &'static str {
        match self {
            Site::Serial | Site::Concurrent | Site::Move => "/dest/notes.txt",
            Site::MergeChild | Site::Extract => "/dest/album/notes.txt",
        }
    }

    fn folder(self) -> &'static str {
        match self {
            Site::Serial | Site::Concurrent | Site::Move => "/dest",
            Site::MergeChild | Site::Extract => "/dest/album",
        }
    }

    fn source_notes(self) -> &'static str {
        match self {
            Site::Serial | Site::Concurrent | Site::Move => "/notes.txt",
            Site::MergeChild | Site::Extract => "/album/notes.txt",
        }
    }

    /// A one-pass extract can't stream a member again.
    fn streams_again(self) -> bool {
        !matches!(self, Site::Extract)
    }
}

/// Where the share drops inside the swap.
#[derive(Clone, Copy, Debug)]
enum Cut {
    /// `delete(orig)` never reaches the server.
    DeleteUnsent,
    /// The server deleted `orig`; the reply is lost.
    DeleteUnanswered,
    /// The delete went through; the rename after it never reaches the server.
    RenameUnsent,
    /// The delete went through; the server renamed the temp onto `orig`, and
    /// the reply is lost.
    RenameUnanswered,
    /// `delete(orig)` answers the error of a session someone else already
    /// replaced: the share reads live on a newer generation (it never reached
    /// the server).
    StaleDeleteError,
}

/// What happens while the copy waits for the share.
#[derive(Clone, Copy, Debug)]
enum Meanwhile {
    /// The share comes back.
    ComesBack,
    /// Someone removes our temp, then the share comes back.
    TempRemovedThenBack,
    /// The person cancels.
    Cancel,
    /// The share never comes back: the wait gives up.
    StaysGone,
}

/// A finished cell.
struct Ran {
    site: Site,
    result: Result<(), WriteOperationError>,
    engine: TestOperations,
    nas: Arc<OutageVolume>,
    dest_inner: Arc<InMemoryVolume>,
    source: Arc<dyn Volume>,
    source_inner: Option<Arc<InMemoryVolume>>,
    archive_root: Option<String>,
    ledger: Ledger,
    state: Arc<WriteOperationState>,
    _archive: Option<TarGzFixture>,
}

/// Runs one Overwrite at `site` with the share armed to drop at `cut`.
async fn run(site: Site, cut: Cut, meanwhile: Meanwhile) -> Ran {
    let op_id = format!("op-swap-outage-{site:?}-{cut:?}-{meanwhile:?}");
    let engine = TestOperations::new();

    let dest_inner = Arc::new(InMemoryVolume::new("Naspolya").with_space_info(10_000_000, 10_000_000));
    dest_inner.create_directory(Path::new("/dest")).await.unwrap();
    if site.folder() != "/dest" {
        dest_inner.create_directory(Path::new(site.folder())).await.unwrap();
    }
    dest_inner
        .create_file(&Path::new(site.folder()).join("keep.txt"), KEEP)
        .await
        .unwrap();
    dest_inner.create_file(Path::new(site.orig()), OLD).await.unwrap();
    let nas = OutageVolume::wrap(Arc::clone(&dest_inner), &op_id);
    let orig = Path::new(site.orig());
    match cut {
        Cut::DeleteUnsent => nas.drop_at_delete_of(orig, RenameDrop::BeforeTheServerSeesIt),
        Cut::DeleteUnanswered => nas.drop_at_delete_of(orig, RenameDrop::AfterTheServerPerformsIt),
        Cut::RenameUnsent => nas.drop_at_rename_onto(orig, RenameDrop::BeforeTheServerSeesIt),
        Cut::RenameUnanswered => nas.drop_at_rename_onto(orig, RenameDrop::AfterTheServerPerformsIt),
        Cut::StaleDeleteError => nas.stale_error_at_delete_of(orig, RenameDrop::BeforeTheServerSeesIt),
    }

    let (source, source_inner, sources, archive) = if let Site::Extract = site {
        let archive = TarGzFixture::new(&[("album/notes.txt", NEW), ("album/fresh.txt", FRESH)]);
        let source = archive.volume();
        let sources = vec![archive.inner("album")];
        (source, None, sources, Some(archive))
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
        (Arc::clone(&inner) as Arc<dyn Volume>, Some(inner), sources, None)
    };

    let budget = match meanwhile {
        Meanwhile::StaysGone => Duration::from_millis(200),
        _ => Duration::from_secs(30),
    };
    let ledger = Ledger::for_test();
    let state = Arc::new(
        engine
            .new_operation_state(Duration::from_millis(50))
            .with_journal_volumes("swap-outage-source".into(), NAS_ID.into())
            .with_in_flight_ledger(ledger.clone())
            .with_outage_policy(OutagePolicy {
                budget,
                cadence: CADENCE,
                poll_every: Duration::from_millis(5),
                attempt_timeout: Duration::from_secs(1),
            }),
    );
    let op = engine.register_state(&op_id, Arc::clone(&state));
    let during = act_once_it_waits(&engine, &state, &nas, &dest_inner, site, meanwhile, op.id());

    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };
    let events = Arc::new(CollectorEventSink::new());
    let dest = Arc::clone(&nas) as Arc<dyn Volume>;
    let result = tokio::time::timeout(Duration::from_secs(30), async {
        match site {
            Site::Move => {
                super::move_cross::move_volumes_with_progress(
                    events,
                    op.id(),
                    &state,
                    Arc::clone(&source),
                    &sources,
                    dest,
                    Path::new("/dest"),
                    &config,
                )
                .await
            }
            _ => {
                super::copy::copy_volumes_with_progress(
                    events,
                    op.id(),
                    &state,
                    Arc::clone(&source),
                    &sources,
                    dest,
                    Path::new("/dest"),
                    &config,
                )
                .await
            }
        }
    })
    .await
    .expect("the copy ends, one way or the other")
    .map(|_| ())
    .map_err(|failure| failure.error);
    during.abort();

    Ran {
        site,
        result,
        engine,
        nas,
        dest_inner,
        source,
        source_inner,
        archive_root: archive.as_ref().map(|a| a.path().display().to_string()),
        ledger,
        state,
        _archive: archive,
    }
}

/// Does what `meanwhile` says once the operation waits for the share.
fn act_once_it_waits(
    engine: &TestOperations,
    state: &Arc<WriteOperationState>,
    nas: &Arc<OutageVolume>,
    dest_inner: &Arc<InMemoryVolume>,
    site: Site,
    meanwhile: Meanwhile,
    op_id: &str,
) -> tokio::task::JoinHandle<()> {
    let state = Arc::clone(state);
    let nas = Arc::clone(nas);
    let dest_inner = Arc::clone(dest_inner);
    let engine = crate::Operations::clone(engine);
    let op_id = op_id.to_owned();
    tokio::spawn(async move {
        wait_until_async(WAIT, "the copy to wait for the share", || {
            state.outage.waiting_for().is_some()
        })
        .await;
        match meanwhile {
            Meanwhile::ComesBack => nas.become_reachable(),
            Meanwhile::TempRemovedThenBack => {
                for temp in temps_in(&dest_inner, site.folder()).await {
                    dest_inner.delete(&temp).await.expect("the temp goes");
                }
                nas.become_reachable();
            }
            Meanwhile::Cancel => engine.cancel_write_operation(&op_id, false),
            Meanwhile::StaysGone => {}
        }
    })
}

/// Every `.cmdr-tmp-*` in `folder`.
async fn temps_in(volume: &InMemoryVolume, folder: &str) -> Vec<PathBuf> {
    volume
        .list_directory(Path::new(folder), None)
        .await
        .unwrap()
        .into_iter()
        .filter(|entry| entry.name.contains(".cmdr-tmp-"))
        .map(|entry| PathBuf::from(entry.path))
        .collect()
}

async fn read_all(volume: &dyn Volume, path: &Path) -> Option<Vec<u8>> {
    let mut stream = volume.open_read_stream(path).await.ok()?;
    let mut buf = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        buf.extend_from_slice(&chunk.ok()?);
    }
    Some(buf)
}

impl Ran {
    fn label(&self) -> String {
        format!("{:?}", self.site)
    }

    async fn orig(&self) -> Option<Vec<u8>> {
        read_all(&*self.dest_inner, Path::new(self.site.orig())).await
    }

    async fn temps(&self) -> Vec<PathBuf> {
        temps_in(&self.dest_inner, self.site.folder()).await
    }

    async fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .dest_inner
            .list_directory(Path::new(self.site.folder()), None)
            .await
            .unwrap()
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        names.sort();
        names
    }

    /// The Overwrite landed: the new bytes at the original's name, nothing
    /// rescued, no temp left, and the merge invariant held.
    async fn assert_landed(&self) {
        let label = self.label();
        assert!(
            self.result.is_ok(),
            "{label}: the overwrite lands, got {:?}",
            self.result
        );
        assert_eq!(
            self.orig().await.as_deref(),
            Some(NEW),
            "{label}: the new bytes at the name"
        );
        assert!(
            !self.names().await.iter().any(|name| name.contains("(recovered)")),
            "{label}: nothing rescued: {:?}",
            self.names().await
        );
        assert_eq!(self.temps().await, Vec::<PathBuf>::new(), "{label}: no temp left");
        assert_eq!(
            self.ledger.recorded_landings(),
            Vec::<PathBuf>::new(),
            "{label}: no landing left recorded"
        );
        self.assert_safe(true).await;
    }

    /// How many times the replacing file streamed to the share.
    fn streams_of_the_file(&self) -> usize {
        self.nas.writes_for(Path::new(self.site.orig()))
    }

    /// The file wasn't streamed again.
    fn assert_streamed_once(&self) {
        assert_eq!(
            self.streams_of_the_file(),
            1,
            "{}: ❗ a whole temp lands without the file streaming again",
            self.label()
        );
    }

    /// The new bytes are on a temp at the destination, recorded as a landing:
    /// the next sweep that reaches the share finishes it. (A sibling's
    /// unfinished write in the concurrent window can leave a temp of its own
    /// that the dead share wouldn't take back: the cleanup's to report.)
    async fn assert_kept_as_a_landing(&self) -> PathBuf {
        let label = self.label();
        let landing_temp = format!("{}.cmdr-tmp-", self.site.orig());
        let temps: Vec<PathBuf> = self
            .temps()
            .await
            .into_iter()
            .filter(|temp| temp.to_string_lossy().starts_with(&landing_temp))
            .collect();
        let [temp] = temps.as_slice() else {
            panic!("{label}: exactly one temp holds the new bytes, got {temps:?}");
        };
        assert_eq!(
            read_all(&*self.dest_inner, temp).await.as_deref(),
            Some(NEW),
            "{label}: ❗ the only copy survives"
        );
        assert_eq!(
            self.ledger.recorded_landings(),
            vec![temp.clone()],
            "{label}: recorded as a landing"
        );
        assert!(
            !self.state.in_flight_temps.lock_ignore_poison().contains(temp),
            "{label}: and no removable temp of ours"
        );
        if let Site::Move = self.site {
            assert!(
                self.source_still_holds_the_file().await,
                "{label}: a failed move keeps its source"
            );
        }
        temp.clone()
    }

    async fn source_still_holds_the_file(&self) -> bool {
        let inner = self.source_inner.as_ref().expect("an in-memory source");
        read_all(&**inner, Path::new(self.site.source_notes())).await.as_deref() == Some(NEW)
    }

    async fn assert_safe(&self, delivered: bool) {
        let label = self.label();
        let source_root = self.archive_root.as_deref().unwrap_or("/");
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
            &(Arc::clone(&self.nas) as Arc<dyn Volume>),
            &SafetySpec {
                label: &label,
                source_root,
                dest_root: "/dest",
                source_files: &source_files,
                delivered: if delivered { &deliveries } else { &[] },
                untouched_dest: &untouched,
            },
        )
        .await;
    }
}

// ── The cells ───────────────────────────────────────────────────────────────

/// (a) The drop hits `delete(orig)` before the server sees it: once the share
/// is back, the original is still in the way, so the landing clears it again
/// and lands.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_drop_before_the_originals_delete_waits_and_lands_at_every_site() {
    for site in Site::ALL {
        let ran = run(site, Cut::DeleteUnsent, Meanwhile::ComesBack).await;
        assert!(ran.nas.reconnect_attempts() > 0, "{}: the share dropped", ran.label());
        ran.assert_landed().await;
        ran.assert_streamed_once();
    }
}

/// (b) The server deleted `orig` and the drop swallowed the reply: the temp is
/// the only copy, and once the share is back the rename alone finishes the
/// landing, ❌ without streaming the file again or parking it under
/// ` (recovered)`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_delete_whose_reply_was_lost_resumes_the_landing_without_streaming_again_at_every_site() {
    for site in Site::ALL {
        let ran = run(site, Cut::DeleteUnanswered, Meanwhile::ComesBack).await;
        assert!(ran.nas.reconnect_attempts() > 0, "{}: the share dropped", ran.label());
        ran.assert_landed().await;
        ran.assert_streamed_once();
    }
}

/// (c) The delete went through and the drop hits the rename after it: once the
/// share is back, the rename lands it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_drop_at_the_rename_after_the_delete_resumes_and_lands_at_every_site() {
    for site in Site::ALL {
        let ran = run(site, Cut::RenameUnsent, Meanwhile::ComesBack).await;
        assert!(ran.nas.reconnect_attempts() > 0, "{}: the share dropped", ran.label());
        ran.assert_landed().await;
        ran.assert_streamed_once();
    }
}

/// The server performed the rename after the delete and the reply was lost:
/// the temp is gone and the name holds exactly the source's bytes, so the file
/// counts as landed. ❗ A one-pass extract has no source to compare against, so
/// it never claims the file: it reports, and the file is there anyway.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rename_whose_reply_was_lost_counts_as_landed_only_on_the_sources_bytes() {
    for site in Site::ALL {
        let ran = run(site, Cut::RenameUnanswered, Meanwhile::ComesBack).await;
        let label = ran.label();
        assert!(ran.nas.reconnect_attempts() > 0, "{label}: the share dropped");
        if site.streams_again() {
            ran.assert_landed().await;
            ran.assert_streamed_once();
        } else {
            assert!(
                ran.result.is_err(),
                "{label}: ❗ never counted as landed without the proof"
            );
            assert_eq!(
                ran.orig().await.as_deref(),
                Some(NEW),
                "{label}: what the server landed stays"
            );
            assert_eq!(ran.temps().await, Vec::<PathBuf>::new(), "{label}");
            assert_eq!(ran.ledger.recorded_landings(), Vec::<PathBuf>::new(), "{label}");
        }
    }
}

/// (d) The outage outlasts the budget after the delete went through: the new
/// bytes stay on the destination, the failure names where they are, and the
/// landing record stays for the next sweep that reaches the share. ❌ Never
/// cleaned as a partial.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_outage_that_outlasts_the_wait_after_the_delete_keeps_the_new_bytes_at_every_site() {
    for site in Site::ALL {
        let ran = run(site, Cut::RenameUnsent, Meanwhile::StaysGone).await;
        let label = ran.label();
        assert_eq!(ran.orig().await, None, "{label}: the delete went through");
        let temp = ran.assert_kept_as_a_landing().await;
        // In the concurrent window a sibling can meet the dead share first,
        // and the first failure is the operation's.
        if site != Site::Concurrent {
            match &ran.result {
                Err(WriteOperationError::NewDataKeptAt { path, kept_at, .. }) => {
                    assert_eq!(path, site.orig(), "{label}");
                    assert_eq!(
                        Path::new(kept_at),
                        temp,
                        "{label}: the failure names where the new bytes are"
                    );
                }
                other => panic!("{label}: the failure names where the new bytes went, got {other:?}"),
            }
        } else {
            assert!(ran.result.is_err(), "{label}: the op fails");
        }
    }
}

/// (e) The temp vanished during the outage while `orig` still holds the OLD
/// bytes at the same size: nothing proves the file landed (the byte compare
/// catches what the size can't), so it runs again and lands; a one-pass
/// extract can't, so it fails with the original in place.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_temp_gone_with_the_old_bytes_at_the_name_is_not_landed_at_every_site() {
    for site in Site::ALL {
        let ran = run(site, Cut::DeleteUnsent, Meanwhile::TempRemovedThenBack).await;
        let label = ran.label();
        if site.streams_again() {
            ran.assert_landed().await;
            assert_eq!(ran.streams_of_the_file(), 2, "{label}: the file streamed again");
        } else {
            assert!(
                ran.result.is_err(),
                "{label}: ❗ never counted as landed on the old bytes"
            );
            assert_eq!(ran.orig().await.as_deref(), Some(OLD), "{label}: the original stays");
            assert_eq!(ran.temps().await, Vec::<PathBuf>::new(), "{label}");
        }
    }
}

/// (f) A stale transport error on `delete(orig)` while the share already reads
/// live on a newer generation: no discard and no refused-delete stat on the
/// fresh session (which would read the name as taken and throw the temp
/// away); the swap re-runs from the delete, at once, with nothing to dial.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stale_error_on_the_originals_delete_re_runs_the_swap_at_every_site() {
    for site in Site::ALL {
        let ran = run(site, Cut::StaleDeleteError, Meanwhile::ComesBack).await;
        assert_eq!(ran.nas.reconnect_attempts(), 0, "{}: nothing to dial", ran.label());
        assert!(ran.nas.generation() > 0, "{}: the session was replaced", ran.label());
        ran.assert_landed().await;
        ran.assert_streamed_once();
    }
}

/// The same classify-before-acting for a claimed name (a `Rename` pick's
/// `O_EXCL` placeholder, or a name a cross-type Overwrite cleared, holding
/// something when the bytes come): the delete that clears it answers a stale
/// transport error with the share already live on a newer session. The
/// refused-delete stat on that fresh session would read the name as still
/// taken, throw the temp away, and end the copy on the first rename's
/// `AlreadyExists`, which no wait accepts. Cut off instead, the landing keeps
/// its temp, and its resume clears the name again and lands.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stale_error_on_a_claimed_names_delete_keeps_the_temp_and_its_resume_lands() {
    let engine = TestOperations::new();
    let inner = Arc::new(InMemoryVolume::new("Naspolya"));
    inner.create_file(Path::new("/notes.txt"), b"").await.unwrap();
    let nas = OutageVolume::wrap(Arc::clone(&inner), "swap-outage-claimed-name");
    nas.stale_error_at_delete_of(Path::new("/notes.txt"), RenameDrop::BeforeTheServerSeesIt);
    let dest = Arc::clone(&nas) as Arc<dyn Volume>;
    let state = Arc::new(
        engine
            .new_operation_state(Duration::from_millis(50))
            .with_journal_volumes("swap-outage-source".into(), NAS_ID.into())
            .with_in_flight_ledger(Ledger::for_test()),
    );
    let claimed = state.scope().host().announce_on(&*dest, Path::new("/notes.txt"));
    let staged = StagedWrite::begin(&state, &*dest, claimed, WriteStaging::StageOntoClaimedName);
    let temp = staged.target().to_path_buf();
    inner.create_file(&temp, NEW).await.unwrap();

    let landed = staged.commit(&dest, NEW.len() as u64).await;

    assert!(nas.generation() > 0, "the session was replaced under the delete");
    assert_eq!(
        read_all(&*inner, &temp).await.as_deref(),
        Some(NEW),
        "❗ no discard on a guess made over the fresh session"
    );
    let Err(LandingError::CutOff(cut_off)) = landed else {
        panic!("a transport fault on the clearing delete cuts the landing off, got {landed:?}");
    };
    assert!(
        matches!(cut_off.resume(&dest, None).await, Resumed::Landed),
        "the resume lands it"
    );
    assert_eq!(read_all(&*inner, Path::new("/notes.txt")).await.as_deref(), Some(NEW));
    assert_eq!(temps_in(&inner, "/").await, Vec::<PathBuf>::new(), "no temp left");
}

/// (g) A Cancel during the wait after the delete went through: the cancel is
/// reported as one, the new bytes stay on their temp recorded as a landing,
/// and the share's next arrival lands them at the original's name.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_during_the_wait_after_the_delete_leaves_the_landing_for_the_arrival_sweep_at_every_site() {
    for site in Site::ALL {
        let ran = run(site, Cut::DeleteUnanswered, Meanwhile::Cancel).await;
        let label = ran.label();
        assert!(
            matches!(ran.result, Err(WriteOperationError::Cancelled { .. })),
            "{label}: a cancel is reported as one, got {:?}",
            ran.result
        );
        let temp = ran.assert_kept_as_a_landing().await;

        ran.nas.reconnected_elsewhere();
        ran.engine
            .volumes()
            .register(NAS_ID, Arc::clone(&ran.nas) as Arc<dyn Volume>);
        wait_until_async(WAIT, "the arrival sweep to settle the landing", || {
            ran.ledger.recorded_landings().is_empty()
        })
        .await;
        assert_eq!(
            ran.orig().await.as_deref(),
            Some(NEW),
            "{label}: the arrival sweep lands the new bytes at the original's name"
        );
        assert!(!ran.dest_inner.exists(&temp).await, "{label}: and the temp is gone");
    }
}
