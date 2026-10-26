//! The explicit leaves of a transfer share the ordinary subtree remap.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::types::TransferLeaves;

/// `<destination>/<own name>` → `<destination>/<leaf>` for each source that
/// lands under a chosen name, seeded before the walk so every per-file path
/// below it lands renamed.
pub(super) fn initial_remap(
    sources: &[PathBuf],
    destination: &Path,
    leaves: &TransferLeaves,
) -> HashMap<PathBuf, PathBuf> {
    sources
        .iter()
        .filter_map(|source| {
            let leaf = leaves.chosen_for(source)?;
            let own = source.file_name()?;
            Some((destination.join(own), destination.join(leaf)))
        })
        .collect()
}
