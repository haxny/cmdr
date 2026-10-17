//! Fakes for the seams a test reads back, so a test can build an engine of its
//! own and assert on what the engine reported. `testing::TestOperations` is an
//! engine over one of each.
//!
//! - [`RecordingEngineEvents`]: every engine-wide fact, in order.
//! - [`FakeVolumes`]: a registry the test fills, answering the engine's volume
//!   questions from it alone.
//! - [`RecordingOwnWrites`]: every path the engine announced before writing it.
//! - [`SettablePacing`]: a rollback pace the test sets on its engine alone.
//! - [`pane_order_host`]: a volume host for a backend a test builds, with no
//!   panes but the panes' name order.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use cmdr_collation::NameCollator;
use cmdr_fs::entry::FileEntry;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::listings::ListingHost;
use cmdr_fs::volume::{DirectoryChange, ResolvedVolume, Volume};
use cmdr_listing::{DirectorySortMode, SortColumn, SortOrder, sort_entries};
use cmdr_operation_log::rollback::VolumeLookup;

use super::super::types::{MoveLeftoversKeptEvent, OperationsChanged, WriteProgressEvent};
use super::{
    EngineEvents, InstantMutationFact, OperationErrorReport, OperationVolumes, OwnWrites, Pacing, UnregisteredVolume,
    VolumeAnswer,
};

/// Records every engine-wide fact it hears.
// DEFAULT-OK: a recorder that has heard nothing yet is exactly empty lists.
#[derive(Default)]
pub(crate) struct RecordingEngineEvents {
    pub operations_changed: Mutex<Vec<OperationsChanged>>,
    pub volumes_busy_changed: Mutex<Vec<Vec<String>>>,
    pub transfers_started: Mutex<Vec<Vec<String>>>,
    pub transfers_finished: Mutex<Vec<Vec<String>>>,
    pub staging_leftovers_kept: Mutex<Vec<MoveLeftoversKeptEvent>>,
    pub instant_mutations: Mutex<Vec<InstantMutationFact>>,
    pub error_reports: Mutex<Vec<OperationErrorReport>>,
    pub scan_wait_progress: Mutex<Vec<WriteProgressEvent>>,
}

impl EngineEvents for RecordingEngineEvents {
    fn operations_changed(&self, snapshot: OperationsChanged) {
        self.operations_changed.lock_ignore_poison().push(snapshot);
    }
    fn volumes_busy_changed(&self, volume_ids: Vec<String>) {
        self.volumes_busy_changed.lock_ignore_poison().push(volume_ids);
    }
    fn transfer_started(&self, volume_ids: &[String]) {
        self.transfers_started.lock_ignore_poison().push(volume_ids.to_vec());
    }
    fn transfer_finished(&self, volume_ids: &[String]) {
        self.transfers_finished.lock_ignore_poison().push(volume_ids.to_vec());
    }
    fn staging_leftovers_kept(&self, notice: MoveLeftoversKeptEvent) {
        self.staging_leftovers_kept.lock_ignore_poison().push(notice);
    }
    fn instant_mutation_settled(&self, fact: InstantMutationFact) {
        self.instant_mutations.lock_ignore_poison().push(fact);
    }
    fn error_report(&self, report: OperationErrorReport) {
        self.error_reports.lock_ignore_poison().push(report);
    }
    fn scan_wait_progress(&self, event: WriteProgressEvent) {
        self.scan_wait_progress.lock_ignore_poison().push(event);
    }
}

/// A volume registry the test fills: nothing routes, nothing is an archive, and
/// an id it doesn't hold is gone.
// DEFAULT-OK: a registry the test hasn't filled holds no volume, which is what it says.
#[derive(Default)]
pub(crate) struct FakeVolumes {
    volumes: Mutex<HashMap<String, Arc<dyn Volume>>>,
}

impl FakeVolumes {
    /// Registers `volume` under `volume_id`.
    pub(crate) fn register(&self, volume_id: &str, volume: Arc<dyn Volume>) {
        self.volumes.lock_ignore_poison().insert(volume_id.to_string(), volume);
    }
}

impl VolumeLookup for FakeVolumes {
    fn get(&self, volume_id: &str) -> Option<Arc<dyn Volume>> {
        self.volumes.lock_ignore_poison().get(volume_id).cloned()
    }
}

impl OperationVolumes for FakeVolumes {
    fn resolve<'a>(&'a self, volume_id: &'a str, path: &'a Path) -> VolumeAnswer<'a, ResolvedVolume> {
        let volume = self.get(volume_id);
        Box::pin(async move { ResolvedVolume::passthrough(volume, path) })
    }
    fn routes_over_its_parent(&self, _path: &Path) -> bool {
        false
    }
    fn path_is_inside_archive<'a>(&'a self, _volume_id: &'a str, _path: &'a Path) -> VolumeAnswer<'a, bool> {
        Box::pin(async { false })
    }
    fn path_crosses_archive_boundary<'a>(&'a self, _volume_id: &'a str, _path: &'a Path) -> VolumeAnswer<'a, bool> {
        Box::pin(async { false })
    }
    fn find_by_root(&self, root: &Path) -> Option<(String, Arc<dyn Volume>)> {
        self.volumes
            .lock_ignore_poison()
            .iter()
            .find(|(_, volume)| volume.root() == root)
            .map(|(id, volume)| (id.clone(), Arc::clone(volume)))
    }
    fn forget_archive_index(&self, _archive_path: &Path) {}
    fn on_volume_arrival(&self, _listener: Box<dyn Fn(&str) + Send + Sync>) {}
    fn unregistered<'a>(&'a self, _volume_id: &'a str) -> VolumeAnswer<'a, UnregisteredVolume> {
        Box::pin(async { UnregisteredVolume::Gone })
    }
}

/// Records every path the engine announced, in order, with whether anything
/// (file, folder, or symlink) sat there at that moment: an announce that came
/// after its syscall shows as a creation that already existed.
// DEFAULT-OK: a recorder that has heard no announce yet is exactly an empty list.
#[derive(Default)]
pub(crate) struct RecordingOwnWrites {
    pub announced: Mutex<Vec<(PathBuf, bool)>>,
}

impl RecordingOwnWrites {
    /// Every announce under `root`, its path relative to `root`, in call order.
    pub(crate) fn announced_under(&self, root: &Path) -> Vec<(PathBuf, bool)> {
        self.announced
            .lock_ignore_poison()
            .iter()
            .filter_map(|(path, existed)| Some((path.strip_prefix(root).ok()?.to_path_buf(), *existed)))
            .collect()
    }
}

impl OwnWrites for RecordingOwnWrites {
    fn announce(&self, path: &Path) {
        let existed = path.symlink_metadata().is_ok();
        self.announced.lock_ignore_poison().push((path.to_path_buf(), existed));
    }
}

/// Paces each rollback item by what the test set, and nothing else: one
/// engine's pace, where the app's (`test_mode`'s throttle) is the process's.
// DEFAULT-OK: a pace nobody set is no pause, which is every real build's answer.
#[derive(Default)]
pub(crate) struct SettablePacing {
    rollback_millis: AtomicU64,
}

impl SettablePacing {
    /// Paces each rollback item by `pace` from now on; `None` stops pacing.
    pub(crate) fn set_rollback_pace(&self, pace: Option<Duration>) {
        let millis = pace.map_or(0, |pace| u64::try_from(pace.as_millis()).unwrap_or(u64::MAX));
        self.rollback_millis.store(millis, Ordering::SeqCst);
    }
}

impl Pacing for SettablePacing {
    fn copy_throttle(&self) -> Option<Duration> {
        None
    }

    fn copy_throttle_for(&self, _source: &Path) -> Option<Duration> {
        None
    }

    fn scan_preview_delay(&self) -> Option<Duration> {
        None
    }

    fn rollback_pace(&self) -> Option<Duration> {
        match self.rollback_millis.load(Ordering::SeqCst) {
            0 => None,
            millis => Some(Duration::from_millis(millis)),
        }
    }
}

/// A volume host for a backend a test builds (a `LocalPosixVolume`, an archive
/// volume): no panes, so every change goes nowhere and every oracle question
/// misses, but a local listing comes back in the panes' name order, as under
/// the app's host. The engine's walkers take a listing's order as is, so a
/// scenario keeps running in the order it was written against.
pub(crate) fn pane_order_host() -> VolumeHost {
    VolumeHost::builder()
        .listings(Arc::new(PaneOrderListings::default()))
        .build()
}

/// [`pane_order_host`]'s listings: the app's `sort_listing` over English
/// collation (what `EnglishNameOrder` gives a host with no panes), and nothing
/// else.
// DEFAULT-OK: a collator nobody asked for yet is built on first use.
#[derive(Default)]
struct PaneOrderListings {
    collator: OnceLock<Arc<NameCollator>>,
}

impl ListingHost for PaneOrderListings {
    fn directory_changed(&self, _volume_id: &str, _parent_path: &Path, _change: DirectoryChange) {}

    fn authoritative_listing(&self, _volume_id: &str, _path: &Path) -> Option<Vec<FileEntry>> {
        None
    }

    fn volumes_with_open_listings(&self, _volume_id_prefix: &str) -> Vec<String> {
        Vec::new()
    }

    fn refresh_archive_listings<'a>(
        &'a self,
        _volume_id: &'a str,
        _archive_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async {})
    }

    fn sort_listing(&self, entries: &mut [FileEntry]) {
        let collator = self.collator.get_or_init(|| Arc::new(NameCollator::new("en")));
        sort_entries(
            collator,
            entries,
            SortColumn::Name,
            SortOrder::Ascending,
            DirectorySortMode::LikeFiles,
        );
    }
}
