//! The hub's memo of which folders are repos: what a confirmation costs, what
//! it remembers, and for how long.

use super::{ResticRepos, Tuning};
use crate::path::VIRTUAL_ENTRY_NAME;
use crate::test_fixtures::FixtureRepo;
use cmdr_fs::entry::FileEntry;
use cmdr_fs::testing::WedgedVolume;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::{InMemoryVolume, ListingProgress, Volume, VolumeError};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const REPO: &str = "/Volumes/naspi/backups/m3-restic";
const PLAIN: &str = "/Volumes/naspi/photos";

/// A parent that counts its listings, over an in-memory volume holding a repo
/// at [`REPO`] and a plain folder at [`PLAIN`].
struct CountingParent {
    inner: InMemoryVolume,
    listings: AtomicUsize,
}

impl CountingParent {
    async fn new() -> Self {
        let inner = InMemoryVolume::new("naspi");
        FixtureRepo::init("pw").write_into(&inner, Path::new(REPO)).await;
        inner.create_directory(Path::new(PLAIN)).await.expect("plain folder");
        Self {
            inner,
            listings: AtomicUsize::new(0),
        }
    }

    fn listings(&self) -> usize {
        self.listings.load(Ordering::Relaxed)
    }
}

impl Volume for CountingParent {
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
        self.listings.fetch_add(1, Ordering::Relaxed);
        self.inner.list_directory(path, on_progress)
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.inner.get_metadata(path)
    }
}

fn hub() -> ResticRepos {
    ResticRepos::new(VolumeHost::detached())
}

#[tokio::test]
async fn a_repo_is_confirmed_by_one_listing_and_remembered() {
    let parent = CountingParent::new().await;
    let hub = hub();

    assert_eq!(hub.serves_confirmed("nas", Path::new(REPO)), None, "never asked");
    assert!(hub.confirm_repo("nas", &parent, Path::new(REPO)).await);
    assert!(hub.confirm_repo("nas", &parent, Path::new(REPO)).await);

    assert_eq!(parent.listings(), 1);
    assert_eq!(hub.serves_confirmed("nas", Path::new(REPO)), Some(true));
}

/// ❗ Without a remembered "no", every path below a real folder named
/// [`VIRTUAL_ENTRY_NAME`] outside a repo would pay a listing of its parent.
#[tokio::test]
async fn a_folder_that_isnt_a_repo_costs_one_listing_across_many_calls() {
    let parent = CountingParent::new().await;
    let hub = hub();

    for _ in 0..5 {
        assert!(!hub.confirm_repo("nas", &parent, Path::new(PLAIN)).await);
    }
    assert_eq!(parent.listings(), 1);
    assert_eq!(hub.serves_confirmed("nas", Path::new(PLAIN)), Some(false));
}

#[tokio::test]
async fn a_no_goes_stale_and_is_asked_again() {
    let parent = CountingParent::new().await;
    let hub = ResticRepos::with_tuning(
        VolumeHost::detached(),
        Tuning {
            not_a_repo_for: Duration::ZERO,
            ..Tuning::default()
        },
    );

    assert!(!hub.confirm_repo("nas", &parent, Path::new(PLAIN)).await);
    assert_eq!(
        hub.serves_confirmed("nas", Path::new(PLAIN)),
        None,
        "a stale no is no answer"
    );
    assert!(!hub.confirm_repo("nas", &parent, Path::new(PLAIN)).await);
    assert_eq!(parent.listings(), 2);
}

#[tokio::test]
async fn a_missing_folder_is_not_a_repo() {
    let parent = CountingParent::new().await;
    let hub = hub();
    assert!(!hub.confirm_repo("nas", &parent, Path::new("/Volumes/naspi/gone")).await);
    assert_eq!(
        hub.serves_confirmed("nas", Path::new("/Volumes/naspi/gone")),
        Some(false)
    );
}

/// The overlay already saw the repo's entries, so its row confirms the repo
/// for free: entering it costs no listing.
#[tokio::test]
async fn the_overlay_row_confirms_the_repo_without_a_listing() {
    let parent = CountingParent::new().await;
    let hub = hub();

    let row = hub.overlay_row("nas", Path::new(REPO));

    assert_eq!(row.name, VIRTUAL_ENTRY_NAME);
    assert_eq!(PathBuf::from(&row.path), Path::new(REPO).join(VIRTUAL_ENTRY_NAME));
    assert!(row.is_directory && !row.is_symlink);
    // The frontend draws its own glyph for this reserved id, never an OS folder icon.
    assert_eq!(row.icon_id, "restic:snapshots");
    assert!(hub.confirm_repo("nas", &parent, Path::new(REPO)).await);
    assert_eq!(parent.listings(), 0);
}

#[tokio::test]
async fn the_memo_is_per_volume() {
    let hub = hub();
    hub.overlay_row("nas", Path::new(REPO));
    assert_eq!(hub.serves_confirmed("other-volume", Path::new(REPO)), None);
}

/// A wedged share must not hang every path below it: the confirmation gives up
/// at its deadline, answers "not confirmed", and remembers nothing.
#[tokio::test]
async fn a_wedged_parent_times_out_unconfirmed_and_unremembered() {
    let parent = WedgedVolume::new("naspi");
    let hub = ResticRepos::with_tuning(
        VolumeHost::detached(),
        Tuning {
            confirm_deadline: Duration::from_millis(50),
            ..Tuning::default()
        },
    );

    assert!(!hub.confirm_repo("nas", &parent, Path::new(REPO)).await);
    assert_eq!(hub.serves_confirmed("nas", Path::new(REPO)), None);
}
