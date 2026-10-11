//! What keeps something alive while a pane shows a directory.

use std::path::Path;

use cmdr_fs::volume::Volume;

use crate::file_system::listing::listing_id::ListingId;

/// A listing that just entered the cache, as an observer hears it. The facts an
/// observer needs ride along, so none of them reads the cache back.
pub struct OpenedListing<'a> {
    /// The listing's id; [`ListingObserver::listing_closed`] names it again.
    pub id: &'a ListingId,
    /// The volume the pane asked for, which keys the listing.
    pub volume_id: &'a str,
    /// The volume that served the read (for a routed path, the routed one).
    pub volume: &'a dyn Volume,
    /// The directory as the read listed it.
    pub path: &'a Path,
    /// The directory in the volume's listing spelling, the one the cache keys on.
    pub stored_path: &'a Path,
}

/// A subsystem that cares which directories a pane has open.
///
/// Called in registration order, outside every engine lock: on open after the
/// cache insert, on close after the cache removal. Both run on the open and
/// close paths, so an observer does cheap, non-blocking work and detaches the
/// rest.
pub trait ListingObserver: Send + Sync {
    /// The observer's stable name (`"git-portal"`), which registration dedupes on.
    fn id(&self) -> &'static str;

    /// A pane opened a listing.
    fn listing_opened(&self, opened: &OpenedListing<'_>);

    /// The listing left the cache. The path isn't passed back: an observer that
    /// armed something remembers what, so a close releases what its open took.
    fn listing_closed(&self, id: &ListingId);
}
