//! Planning an edit: which original entries a [`Changeset`] keeps, and under
//! what name. Pure bookkeeping over the source archive's central directory; the
//! temp+rename write that follows lives in [`super::mutator`].

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::Path;

use zip::ZipArchive;
use zip::read::ZipFile;

use super::mutator::{Changeset, MutationError, sidecar_entry_name};
use crate::apple_double;

/// One original entry kept in the rewrite, with its post-edit name.
pub(super) struct RetainedEntry {
    /// The entry's index in the source `ZipArchive`.
    pub(super) index: usize,
    /// `None` keeps the stored name bytes as they are.
    pub(super) new_name: Option<String>,
    pub(super) compressed_size: u64,
}

/// What an edit keeps of the original archive.
pub(super) struct RetainedPlan {
    /// How many bytes precede the zip data (a self-extractor stub), copied to
    /// the start of the new archive verbatim. 0 for a plain zip.
    pub(super) prefix_len: u64,
    pub(super) entries: Vec<RetainedEntry>,
    /// The prefix and the kept entries' compressed bytes, for the progress total.
    pub(super) bytes: u64,
    /// Original items (sidecars aside) the edit drops or renames, for the
    /// user-facing affected-item tally (`entries_changed`), NOT the
    /// retained-rewrite total.
    pub(super) deleted_count: usize,
    pub(super) renamed_count: usize,
}

/// Plans the retained set: for each original entry, its post-edit name (dropped
/// by a delete, prefix-rewritten by a rename, or unchanged). One raw lookup per
/// entry reads the name + compressed size together.
///
/// Deletes and renames name entries as the reader shows them, which for a name
/// stored without the UTF-8 flag (Shift-JIS, GBK, CP866) isn't what the `zip`
/// crate decodes. See [`planned_name`].
pub(super) fn plan_retained(
    src: &mut ZipArchive<File>,
    archive_path: &Path,
    changeset: &Changeset,
) -> Result<RetainedPlan, MutationError> {
    // Every delete and rename takes the item's AppleDouble sidecar along, and
    // every add drops a stale one at its name (§ `with_sidecar_deletes`).
    let deletes = with_sidecar_deletes(changeset);
    let renames = with_sidecar_renames(&changeset.renames);
    let shown = shown_names(archive_path);
    refuse_a_folded_duplicate(src, &shown)?;
    let shown_names: HashMap<u64, String> = shown.into_iter().collect();

    let prefix_len = leading_bytes_len(src);
    let mut plan = RetainedPlan {
        prefix_len,
        entries: Vec::new(),
        bytes: prefix_len,
        deleted_count: 0,
        renamed_count: 0,
    };
    for index in 0..src.len() {
        let file = src.by_index_raw(index).map_err(MutationError::Zip)?;
        let original_name = planned_name(&file, &shown_names)?;
        let compressed_size = file.compressed_size();
        let encrypted = file.encrypted();
        drop(file);
        // A sidecar is metadata, never an item the user counts.
        let is_item = !apple_double::is_in_macosx_folder(&original_name);
        match plan_new_name(&original_name, &deletes, &renames) {
            Some(new_name) => {
                // Refuse before creating any temp: a retained encrypted entry can't
                // be copied verbatim (the flag is lost), so touching this archive at
                // all would corrupt it. Bailing here keeps the original untouched.
                if encrypted {
                    return Err(MutationError::EncryptedEntryRetained { name: original_name });
                }
                let renamed = new_name != original_name;
                if renamed && is_item {
                    plan.renamed_count += 1;
                }
                plan.entries.push(RetainedEntry {
                    index,
                    new_name: renamed.then_some(new_name),
                    compressed_size,
                });
                plan.bytes += compressed_size;
            }
            None if is_item => plan.deleted_count += 1,
            None => {}
        }
    }
    Ok(plan)
}

/// Every entry's local header offset and name as Cmdr's reader shows it (see
/// [`crate::read::shown_names_with_header_offsets`]). Empty when the reader
/// can't parse the archive, which [`planned_name`] covers.
pub(super) fn shown_names(archive_path: &Path) -> Vec<(u64, String)> {
    let parsed = crate::read::LocalFileSource::open(archive_path)
        .map_err(crate::read::ArchiveError::from)
        .and_then(|source| crate::read::shown_names_with_header_offsets(&source));
    parsed.unwrap_or_else(|err| {
        log::warn!(target: "archive_mutator", "Couldn't read {}'s entry names the way the reader shows them, so the edit matches the zip crate's: {err}", archive_path.display());
        Vec::new()
    })
}

/// Refuses an archive in which two entries share a name. The `zip` crate keeps
/// one entry per raw name (the later one's data, in the earlier one's slot),
/// and the rewrite copies through it, so the other copy would silently vanish.
/// The reader lists every central-directory record, so it listing MORE entries
/// than the `zip` crate holds is exactly that fold.
fn refuse_a_folded_duplicate(src: &ZipArchive<File>, shown: &[(u64, String)]) -> Result<(), MutationError> {
    if shown.len() <= src.len() {
        return Ok(());
    }
    let mut seen = HashSet::new();
    let name = shown
        .iter()
        .map(|(_, name)| name)
        .find(|name| !seen.insert(*name))
        .cloned()
        .unwrap_or_default();
    Err(MutationError::DuplicateEntryName { name })
}

/// How many bytes sit before the zip data: everything ahead of the first local
/// header, or ahead of the central directory in an archive with no entries.
/// A self-extractor's stub lives there, and so does whatever else a tool
/// prepended; the rewrite keeps all of it.
fn leading_bytes_len(src: &ZipArchive<File>) -> u64 {
    (0..src.len())
        .filter_map(|index| src.by_index_data(index).ok().map(|entry| entry.header_start()))
        .min()
        .unwrap_or_else(|| src.central_directory_start())
}

/// The name an edit matches `file` by: what the reader shows for it, the name
/// the user picked. The `zip` crate's own reading is the fallback, for an entry
/// the reader didn't list. Planning by name never touches the name bytes: an
/// entry the edit keeps under its name is copied with its stored bytes.
fn planned_name<R: Read>(file: &ZipFile<'_, R>, shown_names: &HashMap<u64, String>) -> Result<String, MutationError> {
    match shown_names.get(&file.header_start()) {
        Some(shown) => Ok(shown.clone()),
        None => Ok(file.name().map_err(MutationError::Zip)?.into_owned()),
    }
}

/// The deletes `changeset` asks for, each with its item's sidecars: the item's
/// own (`__MACOSX/<parent>/._<name>`) and, for a folder, every one inside it
/// (`__MACOSX/<item>/…`). Plus the sidecar name of every add and sourced mkdir:
/// what's there is stale (the new item writes its own, or carries nothing), and
/// leaving it would put two entries at one name, which the writer refuses.
fn with_sidecar_deletes(changeset: &Changeset) -> Vec<String> {
    let mut deletes = Vec::with_capacity(changeset.deletes.len() * 3 + changeset.adds.len());
    for delete in &changeset.deletes {
        let item = delete.trim_matches('/');
        deletes.push(item.to_string());
        if !item.is_empty() && !apple_double::is_in_macosx_folder(item) {
            deletes.push(format!("{}/{item}", apple_double::MACOSX_FOLDER));
            deletes.push(sidecar_entry_name(item));
        }
    }
    let written = changeset
        .adds
        .iter()
        .map(|add| add.inner_path.as_str())
        .chain(changeset.mkdir_sources.keys().map(String::as_str));
    deletes.extend(written.map(sidecar_entry_name));
    deletes
}

/// The renames `changeset` asks for, each followed by its item's sidecars
/// moving with it, the same two shapes [`with_sidecar_deletes`] drops.
fn with_sidecar_renames(renames: &[(String, String)]) -> Vec<(String, String)> {
    let mut out = Vec::with_capacity(renames.len() * 3);
    for (from, to) in renames {
        out.push((from.clone(), to.clone()));
        let (from, to) = (from.trim_matches('/'), to.trim_matches('/'));
        if from.is_empty() || apple_double::is_in_macosx_folder(from) {
            continue;
        }
        let folder = apple_double::MACOSX_FOLDER;
        out.push((format!("{folder}/{from}"), format!("{folder}/{to}")));
        out.push((sidecar_entry_name(from), sidecar_entry_name(to)));
    }
    out
}

/// Computes an original entry's post-edit name, or `None` if a delete drops it.
///
/// A trailing slash (an explicit directory entry) is preserved. Deletes are
/// checked before renames (a deleted entry is gone regardless of any rename).
/// Both deletes and renames match a whole path component: `foo` deletes/renames
/// `foo` and everything under `foo/`, but never a sibling `foobar`.
fn plan_new_name(original_name: &str, deletes: &[String], renames: &[(String, String)]) -> Option<String> {
    let is_dir = original_name.ends_with('/');
    let logical = original_name.trim_end_matches('/');

    for delete in deletes {
        if path_matches_or_under(logical, delete.trim_matches('/')) {
            return None;
        }
    }

    let mut new_logical = logical.to_string();
    for (from, to) in renames {
        let from = from.trim_matches('/');
        let to = to.trim_matches('/');
        if logical == from {
            new_logical = to.to_string();
            break;
        }
        if let Some(rest) = logical.strip_prefix(&format!("{from}/")) {
            new_logical = format!("{to}/{rest}");
            break;
        }
    }

    Some(if is_dir { format!("{new_logical}/") } else { new_logical })
}

/// Whether `path` is `target` itself or a descendant of it (component-wise).
fn path_matches_or_under(path: &str, target: &str) -> bool {
    path == target || path.strip_prefix(target).is_some_and(|rest| rest.starts_with('/'))
}
