//! What the app-side NFS suites reach the fixture stack through.
//!
//! The dial itself is `cmdr_nfs::testing`, shared with the backend's own
//! suites. This adds the one thing every app cell repeats: a live volume on the
//! knfsd fixture's `/insecure` export plus a scratch folder nothing else in the
//! run will pick, since every cell shares one export and `nextest` runs them in
//! parallel.

use std::path::PathBuf;
use std::sync::Arc;

use cmdr_fs::volume::Volume;
use cmdr_nfs::testing::{connect_insecure, scratch_dir};

/// A live volume on `/insecure` and a scratch folder of its own on it, already
/// created. `what` says which cell to look at when one leaves a mess.
pub(super) async fn fixture(what: &str) -> (Arc<dyn Volume>, PathBuf) {
    let volume = connect_insecure().await;
    let dir = scratch_dir(&volume, what).await;
    (Arc::new(volume), dir)
}
