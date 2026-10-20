//! The hub's index lifecycle and passwords: one load per repo however many wait,
//! cancelled when the last waiter leaves, progress in bytes, the memory cap and
//! idle drop that never take an index in use, and the credential store asked
//! before a prompt and written only after a verified unlock.

use super::{KEYCHAIN_SERVICE, ResticRepos, Tuning};
use crate::path::VIRTUAL_ENTRY_NAME;
use crate::test_fixtures::{self, FixtureRepo};
use crate::volume::ResticVolume;
use cmdr_fs::entry::FileEntry;
use cmdr_fs::testing::{TestDir, wait_until_async};
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::credentials::{
    CredentialStore, CredentialsNotStored, InMemoryCredentials, StoredCredentials,
};
use cmdr_fs::volume::{
    InMemoryVolume, ListingPhase, ListingProgress, UnlockOutcome, Volume, VolumeError, VolumeReadStream,
};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

const PASSWORD: &str = "correct horse";
const VOLUME_ID: &str = "smb-naspi";
const ROOT: &str = "/Volumes/naspi/backups/m3-restic";
const OTHER_ROOT: &str = "/Volumes/naspi/backups/mini-restic";
/// Backups in the fixture: each one changes a file, so each writes an index file,
/// more than the prefetch reads at once.
const BACKUPS: u64 = 6;
/// Generous next to anything here, so a timeout means the behavior is missing.
const WITHIN: Duration = Duration::from_secs(10);

fn at(secs: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

fn view(root: &str) -> PathBuf {
    Path::new(root).join(VIRTUAL_ENTRY_NAME)
}

fn latest(root: &str) -> PathBuf {
    view(root).join("latest")
}

/// A parent over an in-memory repo whose index-file reads can be held at a gate
/// (a slow NAS) and slowed down, counting how many start and how many are in
/// flight.
struct ScriptedParent {
    inner: Arc<InMemoryVolume>,
    gate: watch::Receiver<bool>,
    index_read_delay: Duration,
    index_reads: AtomicUsize,
    in_flight: AtomicUsize,
}

impl ScriptedParent {
    fn index_reads(&self) -> usize {
        self.index_reads.load(Ordering::SeqCst)
    }

    fn in_flight(&self) -> usize {
        self.in_flight.load(Ordering::SeqCst)
    }

    fn is_index_file(path: &Path) -> bool {
        path.parent()
            .and_then(Path::file_name)
            .is_some_and(|dir| dir == "index")
    }
}

impl Volume for ScriptedParent {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn root(&self) -> &Path {
        self.inner.root()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.inner.list_directory(path, on_progress)
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.inner.get_metadata(path)
    }

    fn read_range<'a>(
        &'a self,
        path: &'a Path,
        offset: u64,
        len: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, VolumeError>> + Send + 'a>> {
        self.inner.read_range(path, offset, len)
    }

    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            if !Self::is_index_file(path) {
                return self.inner.open_read_stream(path).await;
            }
            self.index_reads.fetch_add(1, Ordering::SeqCst);
            self.in_flight.fetch_add(1, Ordering::SeqCst);
            let mut gate = self.gate.clone();
            let _ = gate.wait_for(|open| *open).await;
            // allowed-test-sleep: this fake parent IS a slow NAS; each wait is one
            // index file's round trip
            tokio::time::sleep(self.index_read_delay).await;
            let stream = self.inner.open_read_stream(path).await;
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            stream
        })
    }
}

/// One or two repos on a scripted parent, their hub, and an unlocked view over
/// each.
struct Rig {
    parent: Arc<ScriptedParent>,
    gate: watch::Sender<bool>,
    hub: Arc<ResticRepos>,
    volume: Arc<ResticVolume>,
    index_files: usize,
    index_bytes: u64,
    _source: TestDir,
}

struct RigOptions {
    tuning: Tuning,
    gate_open: bool,
    index_read_delay: Duration,
    credentials: Option<Arc<dyn CredentialStore>>,
    unlock: bool,
    second_repo: bool,
}

impl Default for RigOptions {
    fn default() -> Self {
        Self {
            tuning: Tuning::default(),
            gate_open: true,
            index_read_delay: Duration::ZERO,
            credentials: None,
            unlock: true,
            second_repo: false,
        }
    }
}

impl Rig {
    async fn new(options: RigOptions) -> Self {
        let source = TestDir::new("restic_lifecycle_source");
        let fixture = FixtureRepo::init(PASSWORD);
        for i in 0..BACKUPS {
            std::fs::write(source.join("a.txt"), format!("version {i}")).expect("source file");
            fixture.backup(&source, at(1_791_000_000 + i * 3_600));
        }
        let inner = Arc::new(InMemoryVolume::new("naspi"));
        fixture.write_into(inner.as_ref(), Path::new(ROOT)).await;
        if options.second_repo {
            fixture.write_into(inner.as_ref(), Path::new(OTHER_ROOT)).await;
        }
        let index: Vec<(PathBuf, Vec<u8>)> = fixture
            .files()
            .into_iter()
            .filter(|(path, _)| path.starts_with("index"))
            .collect();
        let (gate, gate_rx) = watch::channel(options.gate_open);
        let parent = Arc::new(ScriptedParent {
            inner,
            gate: gate_rx,
            index_read_delay: options.index_read_delay,
            index_reads: AtomicUsize::new(0),
            in_flight: AtomicUsize::new(0),
        });
        let mut host = VolumeHost::detached();
        if let Some(credentials) = options.credentials {
            host = host.with_credentials(credentials);
        }
        let hub = Arc::new(ResticRepos::with_tuning(host, options.tuning));
        let rig = Self {
            volume: Arc::new(view_of(&hub, &parent, ROOT)),
            parent,
            gate,
            hub,
            index_files: index.len(),
            index_bytes: index.iter().map(|(_, bytes)| bytes.len() as u64).sum(),
            _source: source,
        };
        if options.unlock {
            assert!(
                test_fixtures::unlock(&rig.volume, PASSWORD).await,
                "the right password unlocks"
            );
        }
        rig
    }

    fn open_gate(&self) {
        self.gate.send_replace(true);
    }
}

fn view_of(hub: &Arc<ResticRepos>, parent: &Arc<ScriptedParent>, root: &str) -> ResticVolume {
    let parent: Arc<dyn Volume> = Arc::clone(parent) as Arc<dyn Volume>;
    ResticVolume::new(Arc::clone(hub), VOLUME_ID, parent, PathBuf::from(root))
}

/// Lists `path` on a task of its own, the way a pane's read runs.
fn spawn_listing(
    volume: &Arc<ResticVolume>,
    path: PathBuf,
    cancel: Option<CancellationToken>,
) -> tokio::task::JoinHandle<Result<Vec<FileEntry>, VolumeError>> {
    let volume = Arc::clone(volume);
    tokio::spawn(async move { volume.list_directory_with_cancel(&path, None, cancel.as_ref()).await })
}

fn scope(root: &str) -> String {
    format!("{VOLUME_ID}:{root}")
}

// ── One load, shared, cancelled by the last to leave ───────────────────────

/// Two panes entering the same snapshot (or a stalled listing's second attempt
/// beside its first) wait on ONE load: every index file is read once.
#[tokio::test(flavor = "multi_thread")]
async fn concurrent_listings_share_one_index_load() {
    let rig = Rig::new(RigOptions {
        gate_open: false,
        ..RigOptions::default()
    })
    .await;
    let volume = Arc::clone(&rig.volume);

    let first = spawn_listing(&volume, latest(ROOT), None);
    let second = spawn_listing(&volume, latest(ROOT), None);
    let parent = Arc::clone(&rig.parent);
    wait_until_async(WITHIN, "the load is reading", || parent.in_flight() > 0).await;
    rig.open_gate();

    assert!(first.await.expect("task").is_ok());
    assert!(second.await.expect("task").is_ok());
    assert_eq!(rig.parent.index_reads(), rig.index_files, "each index file read once");
}

/// ❗ The listing engine never aborts a read: a cancelled pane DETACHES it, so
/// the token is the only signal. The listing answers at once, and with nobody
/// left waiting the load stops reading; the next listing starts a fresh one.
#[tokio::test(flavor = "multi_thread")]
async fn the_last_waiter_leaving_cancels_the_load_and_the_next_listing_starts_fresh() {
    let rig = Rig::new(RigOptions {
        gate_open: false,
        ..RigOptions::default()
    })
    .await;
    let parent = Arc::clone(&rig.parent);
    let volume = Arc::clone(&rig.volume);
    let cancel = CancellationToken::new();
    let listing = spawn_listing(&volume, latest(ROOT), Some(cancel.clone()));
    wait_until_async(WITHIN, "the load is reading", || parent.in_flight() > 0).await;

    let cancelled_at = std::time::Instant::now();
    cancel.cancel();
    let answer = listing.await.expect("task");
    assert!(matches!(answer, Err(VolumeError::Cancelled(_))), "got {answer:?}");
    assert!(
        cancelled_at.elapsed() < Duration::from_secs(1),
        "the pane hears at once"
    );

    // The reads held at the gate finish their round trip, and nothing new starts.
    let started = parent.index_reads();
    assert!(started < rig.index_files, "the gate held the load mid-way: {started}");
    rig.open_gate();
    wait_until_async(WITHIN, "the load's reads wind down", || parent.in_flight() == 0).await;
    assert_eq!(parent.index_reads(), started, "a cancelled load starts no more reads");
    assert!(!ResticRepos::is_indexed(&volume.slot_for_tests()));

    assert!(spawn_listing(&volume, latest(ROOT), None).await.expect("task").is_ok());
    assert_eq!(
        parent.index_reads(),
        started + rig.index_files,
        "the next listing loaded afresh"
    );
}

/// One pane cancelling while another still waits keeps the load alive for the
/// other: it isn't the last to leave.
#[tokio::test(flavor = "multi_thread")]
async fn a_waiter_leaving_keeps_the_load_alive_for_the_others() {
    let rig = Rig::new(RigOptions {
        gate_open: false,
        ..RigOptions::default()
    })
    .await;
    let parent = Arc::clone(&rig.parent);
    let volume = Arc::clone(&rig.volume);
    let cancel = CancellationToken::new();
    let leaving = spawn_listing(&volume, latest(ROOT), Some(cancel.clone()));
    let staying = spawn_listing(&volume, latest(ROOT), None);
    let slot = volume.slot_for_tests();
    wait_until_async(WITHIN, "both listings wait on the load", || {
        ResticRepos::load_waiters(&slot) == 2
    })
    .await;

    cancel.cancel();
    assert!(matches!(leaving.await.expect("task"), Err(VolumeError::Cancelled(_))));
    rig.open_gate();

    assert!(staying.await.expect("task").is_ok());
    assert_eq!(parent.index_reads(), rig.index_files, "one load, never restarted");
}

/// The pane hears the load in real bytes (the index files' sizes), never
/// rustic's file count, and a load that keeps reading keeps ticking, which is
/// what keeps the listing's stall watch quiet.
#[tokio::test(flavor = "multi_thread")]
async fn a_load_reports_its_bytes_as_it_reads() {
    let rig = Rig::new(RigOptions {
        index_read_delay: Duration::from_millis(150),
        ..RigOptions::default()
    })
    .await;
    let ticks: Mutex<Vec<ListingProgress>> = Mutex::new(Vec::new());
    let on_progress = |p: ListingProgress| ticks.lock().expect("no poison in a test").push(p);

    let entries = rig
        .volume
        .list_directory(&latest(ROOT), Some(&on_progress))
        .await
        .expect("lists");

    assert!(!entries.is_empty());
    let ticks = ticks.into_inner().expect("no poison in a test");
    let phases: Vec<ListingPhase> = ticks.iter().filter_map(|p| p.phase).collect();
    assert!(phases.len() >= 2, "a seconds-long load ticks more than once: {ticks:?}");
    let mut last_done = 0;
    for phase in &phases {
        let ListingPhase::LoadingIndex {
            done_bytes,
            total_bytes,
            total_files,
            ..
        } = *phase;
        assert_eq!(total_bytes, rig.index_bytes, "the total is the index files' sizes");
        assert_eq!(total_files as usize, rig.index_files);
        assert!(
            done_bytes >= last_done && done_bytes <= total_bytes,
            "bytes only move forward"
        );
        last_done = done_bytes;
    }
    assert!(last_done > 0, "bytes arrived");
    assert_eq!(
        ticks.last().map(|p| p.entries()),
        Some(entries.len()),
        "the listing's own count ends it"
    );
}

/// Past the prefetch limit, rustic reads the index files itself; the load still
/// counts their bytes and still lands.
#[tokio::test(flavor = "multi_thread")]
async fn an_index_too_big_to_prefetch_is_streamed_and_still_counted() {
    let rig = Rig::new(RigOptions {
        tuning: Tuning {
            prefetch_limit: 0,
            ..Tuning::default()
        },
        index_read_delay: Duration::from_millis(150),
        ..RigOptions::default()
    })
    .await;
    let last: Mutex<Option<ListingPhase>> = Mutex::new(None);
    let on_progress = |p: ListingProgress| {
        if let Some(phase) = p.phase {
            *last.lock().expect("no poison in a test") = Some(phase);
        }
    };

    rig.volume
        .list_directory(&latest(ROOT), Some(&on_progress))
        .await
        .expect("lists");

    assert_eq!(rig.parent.index_reads(), rig.index_files);
    let Some(ListingPhase::LoadingIndex { done_bytes, .. }) = *last.lock().expect("no poison in a test") else {
        panic!("the load reported its phase");
    };
    assert!(done_bytes > 0, "rustic's own reads are counted");
}

// ── Memory: the cap, the idle drop, and what they never take ───────────────

fn tight_tuning() -> Tuning {
    Tuning {
        max_indexed: 1,
        lease_grace: Duration::ZERO,
        ..Tuning::default()
    }
}

/// With a cap of one, indexing a second repo lets go of the first one's index,
/// and ONLY the index: its password stays, so coming back reloads silently.
#[tokio::test(flavor = "multi_thread")]
async fn past_the_cap_the_least_recently_used_index_goes_and_its_password_stays() {
    let rig = Rig::new(RigOptions {
        tuning: tight_tuning(),
        second_repo: true,
        ..RigOptions::default()
    })
    .await;
    let other = view_of(&rig.hub, &rig.parent, OTHER_ROOT);
    assert!(test_fixtures::unlock(&other, PASSWORD).await);

    rig.volume.list_directory(&latest(ROOT), None).await.expect("lists");
    assert!(ResticRepos::is_indexed(&rig.volume.slot_for_tests()));
    other.list_directory(&latest(OTHER_ROOT), None).await.expect("lists");

    assert!(!ResticRepos::is_indexed(&rig.volume.slot_for_tests()), "evicted");
    assert!(ResticRepos::is_indexed(&other.slot_for_tests()));
    let reads = rig.parent.index_reads();
    rig.volume
        .list_directory(&latest(ROOT), None)
        .await
        .expect("lists again without a password prompt");
    assert_eq!(rig.parent.index_reads(), reads + rig.index_files, "reloaded");
}

/// ❗ A copy out of a repo holds its index through the open stream; a third
/// repo browsed meanwhile must not evict it, or the copy pays a re-open plus a
/// full reload per file. The cap is overshot instead.
#[tokio::test(flavor = "multi_thread")]
async fn an_index_in_use_is_never_evicted() {
    let rig = Rig::new(RigOptions {
        tuning: tight_tuning(),
        second_repo: true,
        ..RigOptions::default()
    })
    .await;
    let other = view_of(&rig.hub, &rig.parent, OTHER_ROOT);
    assert!(test_fixtures::unlock(&other, PASSWORD).await);
    let file = find_file(&rig.volume, latest(ROOT), "a.txt").await;
    let stream = rig.volume.open_read_stream(&file).await.expect("opens");

    other.list_directory(&latest(OTHER_ROOT), None).await.expect("lists");

    assert!(
        ResticRepos::is_indexed(&rig.volume.slot_for_tests()),
        "the copy's index stays"
    );
    drop(stream);
}

/// An index nobody used for the idle period goes; the password stays, so the
/// next listing reloads without asking.
#[tokio::test(flavor = "multi_thread")]
async fn an_idle_index_is_dropped_and_its_password_kept() {
    let rig = Rig::new(RigOptions {
        tuning: Tuning {
            index_idle_drop: Duration::from_millis(200),
            ..tight_tuning()
        },
        ..RigOptions::default()
    })
    .await;
    rig.volume.list_directory(&latest(ROOT), None).await.expect("lists");
    let slot = rig.volume.slot_for_tests();

    wait_until_async(WITHIN, "the idle index is dropped", || !ResticRepos::is_indexed(&slot)).await;
    rig.volume
        .list_directory(&latest(ROOT), None)
        .await
        .expect("lists again without a password prompt");
}

/// A reload for a newer snapshot runs beside the index already loaded; a reader
/// holding the old one finishes on it.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_snapshot_reloads_the_index_while_an_old_reader_finishes() {
    let source = TestDir::new("restic_reload_source");
    std::fs::write(source.join("a.txt"), vec![7u8; 3 * 1024 * 1024]).expect("source file");
    let fixture = FixtureRepo::init(PASSWORD);
    fixture.backup(&source, at(1_791_000_000));
    let parent = Arc::new(InMemoryVolume::new("naspi"));
    fixture.write_into(parent.as_ref(), Path::new(ROOT)).await;
    let hub = Arc::new(ResticRepos::new(VolumeHost::detached()));
    let volume = ResticVolume::new(
        Arc::clone(&hub),
        VOLUME_ID,
        Arc::clone(&parent) as Arc<dyn Volume>,
        PathBuf::from(ROOT),
    );
    assert!(test_fixtures::unlock(&volume, PASSWORD).await);
    let old_snapshot = volume.list_directory(&view(ROOT), None).await.expect("lists")[0]
        .name
        .clone();
    let old_file = find_file(&volume, view(ROOT).join(&old_snapshot), "a.txt").await;
    let mut old_stream = volume.open_read_stream(&old_file).await.expect("opens");
    let first = old_stream.next_chunk().await.expect("a chunk").expect("reads");

    std::fs::write(source.join("a.txt"), b"newer").expect("change the source");
    fixture.backup(&source, at(1_792_000_000));
    fixture.write_into(parent.as_ref(), Path::new(ROOT)).await;
    let rows = volume
        .list_directory(&view(ROOT), None)
        .await
        .expect("refreshes the root");
    let new_snapshot = rows
        .iter()
        .map(|row| row.name.clone())
        .find(|name| *name != old_snapshot && name != "latest")
        .expect("the new backup is listed");
    let new_file = find_file(&volume, view(ROOT).join(&new_snapshot), "a.txt").await;
    let mut new_stream = volume.open_read_stream(&new_file).await.expect("opens");
    assert_eq!(
        new_stream.next_chunk().await.expect("a chunk").expect("reads"),
        b"newer"
    );

    let mut rest = first;
    while let Some(chunk) = old_stream.next_chunk().await {
        rest.extend_from_slice(&chunk.expect("the old reader finishes on the old index"));
    }
    assert_eq!(rest, vec![7u8; 3 * 1024 * 1024]);
}

/// Walks down from `dir` to the folder holding `name` (a snapshot of a Mac
/// starts at `/`), and answers that file's path.
async fn find_file(volume: &ResticVolume, mut dir: PathBuf, name: &str) -> PathBuf {
    loop {
        let entries = volume.list_directory(&dir, None).await.expect("lists");
        if entries.iter().any(|e| e.name == name) {
            return dir.join(name);
        }
        let [only] = entries.as_slice() else {
            panic!("{} should hold one folder on the way down: {entries:?}", dir.display());
        };
        dir = dir.join(&only.name);
    }
}

// ── Passwords and the credential store ─────────────────────────────────────

/// A store that counts how often it's asked.
#[derive(Default)]
struct CountingStore {
    inner: InMemoryCredentials,
    lookups: AtomicUsize,
}

impl CredentialStore for CountingStore {
    fn credentials(&self, service: &str, scope: Option<&str>) -> Option<StoredCredentials> {
        self.lookups.fetch_add(1, Ordering::SeqCst);
        self.inner.credentials(service, scope)
    }

    fn save_credentials(
        &self,
        service: &str,
        scope: Option<&str>,
        credentials: &StoredCredentials,
    ) -> Result<(), CredentialsNotStored> {
        self.inner.save_credentials(service, scope, credentials)
    }
}

async fn locked_rig(store: Arc<dyn CredentialStore>) -> Rig {
    Rig::new(RigOptions {
        credentials: Some(store),
        unlock: false,
        ..RigOptions::default()
    })
    .await
}

/// A remembered password opens the repo with no prompt at all.
#[tokio::test(flavor = "multi_thread")]
async fn a_stored_password_opens_the_repo_without_asking() {
    let store = InMemoryCredentials::new().with_entry(KEYCHAIN_SERVICE, Some(&scope(ROOT)), &scope(ROOT), PASSWORD);
    let rig = locked_rig(Arc::new(store)).await;

    let rows = rig.volume.list_directory(&view(ROOT), None).await.expect("lists");
    assert_eq!(rows.len(), BACKUPS as usize + 1, "every snapshot plus `latest`");
}

/// ❗ Every listing, stall retry, and pane would otherwise ask the store again
/// (a Keychain read each, maybe a prompt): a miss is remembered.
#[tokio::test(flavor = "multi_thread")]
async fn a_store_miss_is_asked_once() {
    let store = Arc::new(CountingStore::default());
    let rig = locked_rig(Arc::clone(&store) as Arc<dyn CredentialStore>).await;

    for _ in 0..3 {
        let listing = rig.volume.list_directory(&view(ROOT), None).await;
        assert!(
            matches!(listing, Err(VolumeError::NeedsPassword { wrong_attempt: false })),
            "got {listing:?}"
        );
    }
    assert_eq!(store.lookups.load(Ordering::SeqCst), 1);
}

/// A remembered password that no longer opens the repo asks as a wrong attempt.
#[tokio::test(flavor = "multi_thread")]
async fn a_stored_password_that_stopped_working_asks_as_a_wrong_attempt() {
    let store = InMemoryCredentials::new().with_entry(KEYCHAIN_SERVICE, Some(&scope(ROOT)), &scope(ROOT), "stale");
    let rig = locked_rig(Arc::new(store)).await;

    let listing = rig.volume.list_directory(&view(ROOT), None).await;
    assert!(
        matches!(listing, Err(VolumeError::NeedsPassword { wrong_attempt: true })),
        "got {listing:?}"
    );
}

/// Remember on files the password, scoped to this repo on this volume, after it
/// unlocked; and the miss memo clears, so a later look finds it.
#[tokio::test(flavor = "multi_thread")]
async fn unlocking_with_remember_files_the_password() {
    let store = Arc::new(InMemoryCredentials::new());
    let rig = locked_rig(Arc::clone(&store) as Arc<dyn CredentialStore>).await;
    let locked = rig.volume.as_password_locked().expect("a repo is password-locked");

    let outcome = locked.unlock(PASSWORD.to_string(), true).await.expect("asks the repo");

    assert_eq!(outcome, UnlockOutcome::Unlocked);
    let filed = store
        .credentials(KEYCHAIN_SERVICE, Some(&scope(ROOT)))
        .expect("filed under the repo's scope");
    assert_eq!(filed.secret, PASSWORD);
    assert!(rig.volume.list_directory(&view(ROOT), None).await.is_ok());
}

/// Remember off seeds nothing, a wrong password files nothing, and an unlock
/// dropped mid-scrypt (the dialog's Cancel) keeps and files nothing.
#[tokio::test(flavor = "multi_thread")]
async fn nothing_is_filed_without_remember_for_a_wrong_password_or_a_cancelled_unlock() {
    let store = Arc::new(InMemoryCredentials::new());
    let rig = locked_rig(Arc::clone(&store) as Arc<dyn CredentialStore>).await;
    let locked = rig.volume.as_password_locked().expect("a repo is password-locked");

    let cancelled = tokio::time::timeout(Duration::from_millis(1), locked.unlock(PASSWORD.to_string(), true)).await;
    assert!(cancelled.is_err(), "scrypt takes longer than a millisecond");
    // allowed-test-sleep: the dropped unlock's scrypt finishes on the blocking
    // pool; give it the time to (wrongly) commit anything
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(store.credentials(KEYCHAIN_SERVICE, Some(&scope(ROOT))).is_none());
    assert!(
        matches!(
            rig.volume.list_directory(&view(ROOT), None).await,
            Err(VolumeError::NeedsPassword { .. })
        ),
        "a cancelled unlock keeps no password"
    );

    let wrong = locked
        .unlock("incorrect".to_string(), true)
        .await
        .expect("asks the repo");
    assert_eq!(wrong, UnlockOutcome::WrongPassword);
    assert!(store.credentials(KEYCHAIN_SERVICE, Some(&scope(ROOT))).is_none());

    let right = locked.unlock(PASSWORD.to_string(), false).await.expect("asks the repo");
    assert_eq!(right, UnlockOutcome::Unlocked);
    assert!(
        store.credentials(KEYCHAIN_SERVICE, Some(&scope(ROOT))).is_none(),
        "remember off never seeds the store"
    );
}

/// Zero writes, still: a load, a listing, and a read through a scripted parent.
#[tokio::test(flavor = "multi_thread")]
async fn loading_the_index_never_writes_to_the_repo() {
    let rig = Rig::new(RigOptions::default()).await;
    let file = find_file(&rig.volume, latest(ROOT), "a.txt").await;
    let mut stream = rig.volume.open_read_stream(&file).await.expect("opens");
    while let Some(chunk) = stream.next_chunk().await {
        chunk.expect("reads");
    }
    assert_eq!(rig.hub.write_attempts(), 0);
}
