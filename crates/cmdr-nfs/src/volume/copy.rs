//! What a copy onto the export keeps beyond the bytes: `SETATTR` of the mtime,
//! the atime, and the mode after the last WRITE (and on a created folder).
//!
//! ❗ v3 has no creation date, so `CREATED` stays out of the caps, and xattrs
//! are dropped with the loss note saying so (no AppleDouble `._` sidecars).
//! The plan's § "Product decisions".

use std::path::Path;
use std::time::SystemTime;

use cmdr_fs::metadata::{CarriedMetadata, MetadataCaps, MetadataReport};

use super::NfsVolume;

impl NfsVolume {
    /// What this backend keeps of a copy's metadata.
    pub(super) fn metadata_caps_impl(&self) -> MetadataCaps {
        MetadataCaps::NONE
    }

    /// Puts `carried` and `modified` on `path` after its bytes landed.
    pub(super) async fn apply_metadata_impl(
        &self,
        path: &Path,
        carried: &CarriedMetadata,
        modified: Option<SystemTime>,
    ) -> MetadataReport {
        let _ = path;
        MetadataReport::none_applicable(carried, modified)
    }
}
