//! Shims over the installed engine for `streaming`'s callers (`shims/mod.rs`).

use std::path::Path;
use std::sync::Arc;

use super::installed;
use crate::file_system::listing::host::ListingEvents;
use crate::file_system::listing::listing_id::ListingId;
use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder};
use crate::file_system::listing::{ListingRequest, StreamingListingState};
use cmdr_fs::volume::VolumeError;

/// A fresh read's state on the installed engine's stall watch.
// shim: removed in M4b
impl StreamingListingState {
    pub fn new() -> Self {
        Self::with_stall_watch(installed().stall_watch())
    }
}

/// The installed engine's in-flight reads, for tests that drive the cancel map.
// shim: removed in M4b
pub(crate) fn streaming_state()
-> &'static std::sync::RwLock<std::collections::HashMap<String, Arc<StreamingListingState>>> {
    &installed().streaming
}

/// `Inner::start_streaming_listing` on the installed engine.
// shim: removed in M4b
pub(crate) fn start_streaming_listing(events: Arc<dyn ListingEvents>, request: ListingRequest) {
    installed().start_streaming_listing(events, request);
}

/// `Inner::read_directory_with_progress` on the installed engine.
// shim: removed in M4b
#[allow(
    clippy::too_many_arguments,
    reason = "Streaming operation requires many state parameters"
)]
pub(crate) async fn read_directory_with_progress(
    events: &Arc<dyn ListingEvents>,
    listing_id: &str,
    state: &Arc<StreamingListingState>,
    volume_id: &str,
    path: &Path,
    include_hidden: bool,
    sort_by: SortColumn,
    sort_order: SortOrder,
    dir_sort_mode: DirectorySortMode,
) -> Result<(), VolumeError> {
    let request = ListingRequest {
        id: ListingId::new(listing_id),
        volume_id: volume_id.to_string(),
        path: path.to_path_buf(),
        include_hidden,
        sort_by,
        sort_order,
        dir_sort_mode,
    };
    installed().read_directory_with_progress(events, state, &request).await
}

/// `Inner::cancel_listing` on the installed engine.
// shim: removed in M4b
pub fn cancel_listing(listing_id: &str) {
    installed().cancel_listing(listing_id);
}
