//! How a streaming listing's end reaches the app: the `listing-error`
//! classification and the bookkeeping around it (the MCP recent-errors ring,
//! the TCC-restricted set, the stale-mount check), plus the analytics category
//! an error reports.
//!
//! Split out of `streaming.rs`'s spawned task so a test drives every terminal
//! outcome without an `AppHandle` (`streaming_reaction_test.rs`).

use std::path::Path;

use crate::file_system::listing::streaming::ListingEventSink;
use crate::file_system::volume::VolumeError;
use crate::file_system::volume::friendly_error::{
    ListingError, archive_needs_password_listing_error, archive_unreadable_listing_error, enrich_with_provider,
    listing_error_from_volume_error,
};

/// The analytics category a `listing-error` reports: the classification's
/// category enum as its wire string, and nothing else (no path, message, or
/// provider). `None` for an unclassified error, which reports nothing.
pub(crate) fn error_analytics_category(error: Option<&ListingError>) -> Option<String> {
    let error = error?;
    serde_json::to_value(error.category)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
}

/// What a streaming listing's end does outside its own events: the app
/// bookkeeping [`finish_streaming_listing`] runs. Production answers with
/// [`AppListingEndEffects`]; a test records the calls.
pub(crate) trait ListingEndEffects {
    /// Records a failed listing into the MCP recent-errors ring.
    fn record_recent_error(&self, listing_id: &str, volume_id: &str, path: &Path, message: &str, error: &ListingError);
    /// Records a `PermissionDenied` path as possibly TCC-restricted.
    fn record_denial(&self, path: &Path);
    /// Drops `path` from the TCC-restricted set.
    fn clear_restricted(&self, path: &Path);
    /// Hands a listing's error to the volume's stale-mount check.
    fn note_root_failure(&self, volume_id: &str, error: &VolumeError);
}

/// The app's answers to [`ListingEndEffects`].
pub(crate) struct AppListingEndEffects;

impl ListingEndEffects for AppListingEndEffects {
    fn record_recent_error(&self, listing_id: &str, volume_id: &str, path: &Path, message: &str, error: &ListingError) {
        crate::mcp::listing_errors::record(listing_id, volume_id, &path.to_string_lossy(), message, Some(error));
    }

    fn record_denial(&self, path: &Path) {
        crate::restricted_paths::record_denial(path);
    }

    fn clear_restricted(&self, path: &Path) {
        crate::restricted_paths::clear_path(path);
    }

    fn note_root_failure(&self, volume_id: &str, error: &VolumeError) {
        crate::file_system::volume::note_root_failure(volume_id, error);
    }
}

/// Reacts to how a streaming listing's read ended: classifies a failure into a
/// `listing-error` and runs the app bookkeeping around it, or, on `Ok` (success,
/// a cancel, and the restricted-empty root alike), clears the path from the
/// TCC-restricted set.
pub(crate) fn finish_streaming_listing(
    events: &dyn ListingEventSink,
    effects: &dyn ListingEndEffects,
    listing_id: &str,
    volume_id: &str,
    path: &Path,
    result: Result<(), VolumeError>,
) {
    match result {
        Err(e) => {
            // Function returned an error (volume not found, permission denied, I/O, etc.)
            // A failed `.zip` browse — the archive is damaged, encrypted, an
            // unsupported format, or a file with an archive extension that isn't
            // really an archive — collapses to `NotSupported`/`IoError` at the
            // `ArchiveVolume` layer. Swap in the dedicated "damaged archive" copy
            // so the pane doesn't show a generic listing error. A valid archive
            // with a missing inner path stays `NotFound` (not an integrity fault),
            // and the archive case skips provider enrichment (no cloud suggestion).
            let listing_error = if let VolumeError::NeedsPassword { wrong_attempt } = e {
                // A HEADER-encrypted archive needs its password to even list; the
                // FE renders a password prompt, not an error pane. Skip provider
                // enrichment (no cloud suggestion on a password prompt).
                archive_needs_password_listing_error(wrong_attempt)
            } else if matches!(e, VolumeError::NotSupported | VolumeError::IoError { .. })
                && cmdr_archive::path_targets_archive_file(path)
            {
                archive_unreadable_listing_error()
            } else {
                let mut generic = listing_error_from_volume_error(&e, path);
                enrich_with_provider(&mut generic, path);
                generic
            };
            if matches!(&e, VolumeError::PermissionDenied { .. }) {
                effects.record_denial(path);
            }
            // A stale-mount errno here is the only evidence we ever get that
            // the volume's active mount died: probing for it would block on
            // the wedged mount itself. If the same filesystem is reachable
            // through another mount, this moves the volume there.
            effects.note_root_failure(volume_id, &e);
            let message = e.to_string();
            // Record into the MCP recent-errors ring buffer so `cmdr://state`
            // surfaces what just failed, without callers grepping the log file.
            effects.record_recent_error(listing_id, volume_id, path, &message, &listing_error);
            // One line per failed listing, carrying the TYPED reason, so a log
            // answers "why did this listing fail" without reading code.
            log::debug!(
                "listing {listing_id} on volume {volume_id} ended in error: {:?} ({:?}){}",
                listing_error.reason,
                listing_error.category,
                e.errno_field(),
            );
            events.emit_error(listing_id, message, Some(listing_error));
        }
        Ok(()) => {
            // Success: listing-complete already emitted. If this path was
            // previously recorded as TCC-restricted, drop it: the user
            // must have granted access (per-folder TCC popup or FDA toggle).
            effects.clear_restricted(path);
        }
    }
}
