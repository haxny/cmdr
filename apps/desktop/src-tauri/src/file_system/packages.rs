//! Package detection for the folders the package list can't answer: a dotted
//! folder (`Logo.sketchbundle`) is a package when macOS says so
//! (`NSURLIsPackageKey`: a registered package type, or the `kHasBundle` bit).
//!
//! Deferred to the VISIBLE rows, the way custom folder icons and Finder tags
//! are: the frontend asks after each visible-range fetch (`enrich_packages`),
//! ❌ never the bulk listing, where an NSURL lookup per dotted folder would cost a
//! 100k-entry directory dearly. The list (`cmdr_fs::file_extension`) stays the
//! free fast path. Only a volume backed by the local filesystem asks; SMB, MTP,
//! S3, and every other backend, and Linux, keep the list's answer.

use std::path::Path;
#[cfg(target_os = "macos")]
use std::sync::LazyLock;
#[cfg(target_os = "macos")]
use std::time::Duration;

use cmdr_fs::file_extension::needs_package_lookup;
use cmdr_listing::ListingId;

use crate::listing_host::listings;

/// How long one visible range's lookups may take before the pane keeps the
/// list's answer. Each lookup is a local metadata read; a hung mount is what
/// this bounds.
#[cfg(target_os = "macos")]
const LOOKUP_BUDGET: Duration = Duration::from_secs(2);

/// The lookups' own pool: NSURL resource reads can reach File Provider over XPC
/// (an iCloud-synced Desktop), which needs an 8 MB stack and may never return.
#[cfg(target_os = "macos")]
static LOOKUP_POOL: LazyLock<crate::file_system::framework_pool::Pool> = LazyLock::new(|| {
    crate::file_system::framework_pool::Pool::new(crate::file_system::framework_pool::PoolConfig {
        name: "package_lookup",
        target_workers: 1,
        max_workers: 4,
        wedged_after: Duration::from_secs(10),
    })
});

/// The paths among `paths` only macOS can answer: a dotted name the package
/// list doesn't know. A row with no dot, or a listed suffix, is never asked.
pub(crate) fn lookup_candidates(paths: Vec<String>) -> Vec<String> {
    paths
        .into_iter()
        .filter(|path| {
            Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(needs_package_lookup)
        })
        .collect()
}

/// Asks macOS about the visible dotted folders of `listing_id` and patches the
/// answers into the listing, which tells the pane. Returns whether the lookups
/// ran out of time (the pane then keeps the list's answer for this range).
pub(crate) async fn enrich(listing_id: ListingId, paths: Vec<String>) -> bool {
    let candidates = lookup_candidates(paths);
    if candidates.is_empty() || crate::fda_gate::is_fda_pending_runtime() || !is_on_local_disk(&listing_id) {
        return false;
    }
    match look_up(candidates).await {
        Some(updates) => {
            listings().apply_row_facts(&listing_id, cmdr_listing::RowFacts::Packages(updates));
            false
        }
        None => true,
    }
}

/// Whether the listing's volume is the local filesystem (`Volume::local_path`),
/// the only place an NSURL lookup reads the folder the row names.
fn is_on_local_disk(listing_id: &ListingId) -> bool {
    let Some((volume_id, _)) = listings().location(listing_id) else {
        return false;
    };
    crate::file_system::volume::manager::get_volume_manager()
        .get(&volume_id)
        .is_some_and(|volume| volume.local_path().is_some())
}

/// Each candidate with macOS's answer, or `None` when the lookups outlived
/// [`LOOKUP_BUDGET`].
#[cfg(target_os = "macos")]
async fn look_up(candidates: Vec<String>) -> Option<Vec<(String, bool)>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    LOOKUP_POOL.submit(Box::new(move || {
        let answers = candidates
            .into_iter()
            .map(|path| {
                let is_package = is_package_to_macos(&path);
                (path, is_package)
            })
            .collect();
        let _ = tx.send(answers);
    }));
    tokio::time::timeout(LOOKUP_BUDGET, rx).await.ok()?.ok()
}

/// Off macOS there's no package concept beyond the list.
#[cfg(not(target_os = "macos"))]
async fn look_up(_candidates: Vec<String>) -> Option<Vec<(String, bool)>> {
    Some(Vec::new())
}

/// Whether macOS presents the folder at `path` as one item
/// (`NSURLIsPackageKey`). `false` for a missing path, a file, or a failed read:
/// the row then stays a folder.
#[cfg(target_os = "macos")]
pub(crate) fn is_package_to_macos(path: &str) -> bool {
    use objc2::rc::autoreleasepool;
    use objc2_foundation::{NSString, NSURL};
    autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        cmdr_local::nsurl::get_bool_resource(&url, "NSURLIsPackageKey").unwrap_or(false)
    })
}

#[cfg(test)]
#[path = "packages_test.rs"]
mod tests;
