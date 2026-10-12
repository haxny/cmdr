//! What keeps something alive while a pane shows a directory.
//!
//! A [`ListingObserver`] hears one call when a listing enters the listing cache
//! and one when it leaves. Cmdr's observers, in order: the FSEvents watcher, the
//! virtual `.git` portal (which keeps a repository's `.git/*` watcher armed for
//! as long as a pane stands in its virtual trees), and `listing_index_sizes`
//! (which routes the index's folder-size updates to the listings they touch).
//!
//! **Arming belongs to the BACKEND, ❌ never to a frontend subscription.** The
//! breadcrumb chip's `subscribe_git_state` used to be the only thing starting a
//! per-repo watcher, so a pane on `branches/` with nothing else open went stale,
//! and turning both git features off stopped every watcher in the app. An
//! observer here holds for a pane the MCP server drove, for a window that never
//! rendered a chip, and for whatever asks next.
//!
//! The host hands its observers over at install, in order
//! ([`ListingSeams::observer`](super::ListingSeams::observer)).

use std::path::Path;

use cmdr_fs::volume::Volume;

use crate::file_system::listing::listing_id::ListingId;

/// A listing that just entered the cache, as an observer hears it. The facts an
/// observer needs ride along, so none of them reads the cache back.
pub struct OpenedListing<'a> {
    /// The listing's id; [`ListingObserver::listing_closed`] names it again.
    pub id: &'a ListingId,
    /// The volume the pane asked for, which keys the listing.
    #[expect(
        dead_code,
        reason = "issue #301: `IndexSizeListings` reads these instead of the cache once each test owns its engine (M3a)"
    )]
    pub volume_id: &'a str,
    /// The volume that served the read (for a routed path, the routed one).
    pub volume: &'a dyn Volume,
    /// The directory as the read listed it.
    pub path: &'a Path,
    /// The directory in the volume's listing spelling, the one the cache keys on.
    #[expect(
        dead_code,
        reason = "issue #301: `IndexSizeListings` reads these instead of the cache once each test owns its engine (M3a)"
    )]
    pub stored_path: &'a Path,
}

/// A subsystem that cares which directories a pane has open.
///
/// Called in registration order, outside every engine lock: on open after the
/// cache insert, on close after the cache removal. ❗ Both run on the open and
/// close paths, so an observer does cheap, non-blocking work and detaches the
/// rest: `git/arming.rs` decides from the path alone and hands the subscribe to
/// the blocking pool.
pub trait ListingObserver: Send + Sync {
    /// The observer's stable name (`"git-portal"`), which registration dedupes on.
    fn id(&self) -> &'static str;

    /// A pane opened a listing.
    fn listing_opened(&self, opened: &OpenedListing<'_>);

    /// The listing left the cache. The path isn't passed back: an observer that
    /// armed something remembers what, so a close releases what its open took.
    fn listing_closed(&self, id: &ListingId);
}

/// Tells every observer, in order, that a listing opened.
pub(crate) fn listing_opened(opened: &OpenedListing<'_>) {
    for observer in super::observers() {
        observer.listing_opened(opened);
    }
}

/// Tells every observer, in order, that `id` left the cache.
///
/// ❗ Call it AFTER the cache removal, never before: an observer's own detached
/// arm reconciles against cache membership, so releasing while the entry is
/// still there lets a racing arm re-take what the close just gave back.
pub(crate) fn listing_closed(id: &ListingId) {
    for observer in super::observers() {
        observer.listing_closed(id);
    }
}
