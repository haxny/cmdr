//! Where a transfer's sources may land by name: the ONE check of its
//! `TransferLeaves`, run by every start before the operation is admitted
//! (`transfer/DETAILS.md` § "Named destinations"), and the refusal for a route
//! that can't name each source.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use cmdr_fs::name_fold::fold_name;

use super::types::{NamedLeavesRefusal, TransferLeaves, WriteOperationError};
use super::validation::validate_path_length;

/// How the start may tell whether two paths are one item: by name alone, or
/// also by `dev+ino` when every path is on this machine's disk and can be
/// statted (`is_same_file`, which settles a symlinked or case-differing route).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LeafIdentity {
    /// Folded names only: a remote volume's paths can't be statted here.
    Names,
    /// Folded names, and `dev+ino` wherever both paths exist.
    NamesAndInodes,
}

/// The ONE check of a transfer's leaves: the routed start runs it before the
/// operation is admitted (`routing.rs`, `local_start.rs::move_files_start`), and
/// every engine below trusts it. `sources` and `destination` are the paths the
/// engine will walk, in the same frame as the leaves' keys.
///
/// - **A single leaf** names exactly one source and never escapes its parent.
/// - **Per-source names** ([`validate_per_source_leaves`]): every key is a
///   source, every name a plain leaf, every landing name distinct ignoring case
///   and Unicode form, nothing lands on another source, and no source already
///   sits in the destination.
pub(crate) fn validate_transfer_leaves(
    leaves: &TransferLeaves,
    sources: &[PathBuf],
    destination: &Path,
    identity: LeafIdentity,
) -> Result<(), WriteOperationError> {
    if let Some(names) = leaves.per_source_names() {
        return validate_per_source_leaves(names, leaves, sources, destination, identity);
    }
    let Some(name) = leaves.single_leaf() else {
        return Ok(());
    };
    let named_path = destination.join(name);
    if sources.len() != 1
        || sources[0].file_name().is_none()
        || name.is_empty()
        || name.contains(['/', '\0'])
        || name == "."
        || name == ".."
    {
        return Err(WriteOperationError::InvalidName {
            path: named_path.display().to_string(),
            message: "Choose one item and a filename without path separators".into(),
        });
    }
    validate_path_length(&named_path)
}

/// The per-source half of [`validate_transfer_leaves`], in the order a person
/// would fix them: the names themselves, then how they meet each other and the
/// sources.
///
/// Folding is always [`fold_name`] (NFC, then lowercase), whatever the
/// destination's own case rule: a refusal on a case-sensitive disk costs a
/// retry, a collision missed on a case-insensitive one costs a file. Two
/// sources sharing an ORIGINAL name still refuse byte-exact in the started
/// operation (`validate_source_names_are_distinct`), because the native
/// engines key their subtree remap on `<destination>/<own name>`.
fn validate_per_source_leaves(
    names: &HashMap<PathBuf, String>,
    leaves: &TransferLeaves,
    sources: &[PathBuf],
    destination: &Path,
    identity: LeafIdentity,
) -> Result<(), WriteOperationError> {
    let refused = |path: &Path, reason| WriteOperationError::NamedLeavesRefused {
        path: path.display().to_string(),
        reason,
    };
    // A name for nothing is a caller that lost track of its sources: refuse,
    // never ignore, or the item it meant lands under its old name unannounced.
    let known: HashSet<&PathBuf> = sources.iter().collect();
    let mut keys: Vec<&PathBuf> = names.keys().collect();
    keys.sort();
    if let Some(stray) = keys.into_iter().find(|key| !known.contains(key)) {
        return Err(refused(stray, NamedLeavesRefusal::NotASource));
    }

    for source in sources {
        let Some(leaf) = names.get(source) else { continue };
        let landing = destination.join(leaf);
        if let Err(refusal) = cmdr_fs::validation::validate_plain_leaf(leaf) {
            return Err(WriteOperationError::InvalidName {
                path: landing.display().to_string(),
                message: format!("Not a plain file name: {refusal:?}").into(),
            });
        }
        validate_path_length(&landing)?;
    }

    // Where each source lands, its own name included: a renamed item can
    // collide with one that kept its name as easily as with another rename.
    let mut landings: Vec<(&PathBuf, PathBuf)> = Vec::with_capacity(sources.len());
    let mut taken: HashSet<String> = HashSet::with_capacity(sources.len());
    for source in sources {
        let Some(leaf) = leaves.leaf_of(source) else { continue };
        let landing = destination.join(leaf);
        if !taken.insert(folded(Path::new(leaf))) {
            return Err(refused(&landing, NamedLeavesRefusal::SameName));
        }
        landings.push((source, landing));
    }

    // A landing on another source replaces it before it's read (a copy) or
    // turns "a to b, b to c" into a lost file when the order runs wrong (a
    // move). Compared folded, and by `dev+ino` where both exist, which catches
    // the same item reached through another spelling or a symlinked folder.
    let by_name: HashMap<String, &PathBuf> = sources.iter().map(|s| (folded(s), s)).collect();
    let by_inode: HashMap<(u64, u64), &PathBuf> = match identity {
        LeafIdentity::NamesAndInodes => sources.iter().filter_map(|s| Some((file_id(s)?, s))).collect(),
        LeafIdentity::Names => HashMap::new(),
    };
    for (source, landing) in &landings {
        let onto = by_name
            .get(&folded(landing))
            .or_else(|| file_id(landing).and_then(|id| by_inode.get(&id)));
        // Landing on ITSELF is the next rule's, not a chain.
        if onto.is_some_and(|other| other != source) {
            return Err(refused(landing, NamedLeavesRefusal::OntoASource));
        }
    }

    // Names go to OTHER folders. One already in the destination is a rename,
    // and the same-volume move's identity guard would drop a case-only one.
    let destination_key = folded(destination);
    let destination_id = (identity == LeafIdentity::NamesAndInodes)
        .then(|| file_id(destination))
        .flatten();
    for source in sources {
        let Some(parent) = source.parent() else { continue };
        let already_there =
            folded(parent) == destination_key || destination_id.is_some_and(|id| file_id(parent) == Some(id));
        if already_there {
            return Err(refused(source, NamedLeavesRefusal::AlreadyInDestination));
        }
    }
    Ok(())
}

/// `path` under [`fold_name`], whole, for the per-source comparisons. Its
/// components first, so a trailing `/` or a `.` can't make one folder two.
fn folded(path: &Path) -> String {
    let normal: PathBuf = path.components().collect();
    fold_name(&normal.to_string_lossy()).into_owned()
}

/// `path`'s `(dev, ino)`, following symlinks the way [`is_same_file`] does;
/// `None` when it doesn't exist (or off unix).
#[cfg(unix)]
fn file_id(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(path).ok().map(|meta| (meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
fn file_id(_path: &Path) -> Option<(u64, u64)> {
    None
}

/// Refuses per-source names on a route that can't honour them: an archive
/// changeset (copy or move into a zip, move out of one), whose plan never takes
/// more than one leaf. Every other engine names each source. A single leaf passes.
pub(crate) fn refuse_per_source_leaves_here(
    leaves: &TransferLeaves,
    destination: &Path,
) -> Result<(), WriteOperationError> {
    if leaves.is_per_source() {
        return Err(WriteOperationError::NamedLeavesRefused {
            path: destination.display().to_string(),
            reason: NamedLeavesRefusal::RouteCannotName,
        });
    }
    Ok(())
}
