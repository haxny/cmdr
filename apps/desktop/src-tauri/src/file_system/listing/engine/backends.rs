//! The engine answering `cmdr_fs`'s `ListingHost` itself, so a host wires its
//! storage backends to the engine in one line ([`Listings::listing_host`](super::Listings::listing_host)).

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Weak};

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::DirectoryChange;
use cmdr_fs::volume::host::listings::ListingHost;

use super::Inner;

/// A weak handle on the engine: a strong one would close a cycle (engine →
/// volumes → a volume → its host → engine) for a host that hands volumes over
/// as values. Once the engine is gone, every answer is the no-op one.
pub(super) struct EngineListingHost {
    inner: Weak<Inner>,
}

impl EngineListingHost {
    pub(super) fn new(inner: &Arc<Inner>) -> Self {
        Self {
            inner: Arc::downgrade(inner),
        }
    }
}

impl ListingHost for EngineListingHost {
    fn directory_changed(&self, volume_id: &str, parent_path: &Path, change: DirectoryChange) {
        if let Some(inner) = self.inner.upgrade() {
            inner.notify_directory_changed(volume_id, parent_path, change);
        }
    }

    fn authoritative_listing(&self, volume_id: &str, path: &Path) -> Option<Vec<FileEntry>> {
        self.inner.upgrade()?.try_get_authoritative_listing(volume_id, path)
    }

    fn volumes_with_open_listings(&self, volume_id_prefix: &str) -> Vec<String> {
        self.inner
            .upgrade()
            .map(|inner| inner.volume_ids_with_listings(volume_id_prefix))
            .unwrap_or_default()
    }

    fn refresh_archive_listings<'a>(
        &'a self,
        volume_id: &'a str,
        archive_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let inner = self.inner.upgrade();
        Box::pin(async move {
            if let Some(inner) = inner {
                inner.refresh_archive_listings(volume_id, archive_path).await;
            }
        })
    }
}
