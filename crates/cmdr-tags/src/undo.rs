//! The undo store: before an apply writes tags, the caller records each file's old tags
//! here; undo writes them back with [`write_tags`], at the files' new paths after renames.
//!
//! One directory per apply under the root: `<id>/manifest.json` (the snapshot without cover
//! bytes) and `<id>/<n>.cover` (entry `n`'s cover). Every file is written to a `*.tmp` beside
//! it, fsynced, and renamed into place, and the manifest goes last: an apply without a
//! readable manifest doesn't exist. Layout and rules: `DETAILS.md` § "Undo store layout".

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::model::{Cover, FileTags, TagError, TagField};
use crate::write::{WriteOptions, write_tags};

const MANIFEST: &str = "manifest.json";
const TEMP_SUFFIX: &str = ".tmp";
const MANIFEST_VERSION: u32 = 1;

/// Names one apply in the store, chosen by the caller (the journal's operation ID, say). It
/// becomes a directory name, so it must be one safe path component: not empty, no `/`, `\`,
/// or NUL, no `..`, and no leading `.`. Anything else is refused as
/// `TagError::Io(ErrorKind::InvalidInput, _)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ApplyId(pub String);

/// How much the store keeps. After each record the oldest applies go until both hold; the
/// apply just recorded is always kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UndoLimits {
    /// The most applies kept.
    pub max_applies: usize,
    /// The most bytes (manifests plus covers) kept.
    pub max_bytes: u64,
}

impl Default for UndoLimits {
    /// 50 applies and 512 MiB.
    fn default() -> Self {
        Self {
            max_applies: 50,
            max_bytes: 512 * 1024 * 1024,
        }
    }
}

/// What a restore did, per file, in the order the files were recorded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RestoreReport {
    /// Files whose old tags are back.
    pub restored: Vec<PathBuf>,
    /// Files no longer at their (repathed) path.
    pub missing: Vec<PathBuf>,
    /// Files the write refused or failed on, unchanged, with why.
    pub failed: Vec<(PathBuf, TagError)>,
}

/// Snapshots of applies' old tags, in a directory the store owns.
#[derive(Debug)]
pub struct UndoStore {
    root: PathBuf,
    limits: UndoLimits,
}

/// `<id>/manifest.json`. Covers are files beside it, named in `cover`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    version: u32,
    /// Recording order across the store: pruning drops the lowest first.
    seq: u64,
    entries: Vec<Entry>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    path: PathBuf,
    fields: BTreeMap<TagField, Vec<String>>,
    extras: BTreeMap<String, Vec<String>>,
    cover: Option<CoverRef>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoverRef {
    /// `<n>.cover` in the apply's directory.
    file: String,
    mime: String,
}

/// An apply found in the store, for pruning.
struct Stored {
    name: String,
    seq: u64,
    bytes: u64,
}

impl UndoStore {
    /// Opens the store at `root`, creating it if needed. Temp files a crash left inside apply
    /// directories are removed; an apply directory without a readable manifest is ignored.
    pub fn open(root: PathBuf, limits: UndoLimits) -> Result<Self, TagError> {
        fs::create_dir_all(&root)?;
        for dir in apply_dirs(&root)? {
            for item in fs::read_dir(&dir)? {
                let item = item?;
                if item.file_type()?.is_file() && item.file_name().to_string_lossy().ends_with(TEMP_SUFFIX) {
                    fs::remove_file(item.path())?;
                }
            }
        }
        Ok(Self { root, limits })
    }

    /// Records the old tags of an apply's files, replacing any record with the same `id`,
    /// then prunes the oldest applies past the limits. Returns once the record is on disk.
    pub fn record(&self, id: &ApplyId, before: &[(PathBuf, FileTags)]) -> Result<(), TagError> {
        let dir = self.apply_dir(id)?;
        let seq = self.stored()?.iter().map(|s| s.seq + 1).max().unwrap_or(0);
        if fs::symlink_metadata(&dir).is_ok() {
            fs::remove_dir_all(&dir)?;
        }
        fs::create_dir(&dir)?;
        if let Err(err) = write_apply(&dir, seq, before) {
            if let Err(cleanup) = fs::remove_dir_all(&dir) {
                log::warn!(target: "cmdr_tags::undo", "Couldn't remove an unfinished undo record: {cleanup}");
            }
            return Err(err);
        }
        sync_dir(&self.root);
        // The record is complete; a prune that doesn't work only leaves old records behind.
        if let Err(err) = self.prune(&id.0) {
            log::warn!(target: "cmdr_tags::undo", "Couldn't prune old undo records: {err}");
        }
        Ok(())
    }

    /// Points the record at the files' new paths after the apply renamed them. Each move is
    /// (old path, new path), all at once, so swaps work; a move whose old path isn't recorded
    /// is ignored.
    pub fn repath(&self, id: &ApplyId, moves: &[(PathBuf, PathBuf)]) -> Result<(), TagError> {
        let dir = self.apply_dir(id)?;
        let manifest = load_manifest(&dir)?;
        let entries = manifest
            .entries
            .into_iter()
            .map(|entry| match moves.iter().find(|(from, _)| *from == entry.path) {
                Some((_, to)) => Entry {
                    path: to.clone(),
                    ..entry
                },
                None => entry,
            })
            .collect();
        write_manifest(&dir, &Manifest { entries, ..manifest })
    }

    /// Writes each recorded file's old tags back with `opts` (a removed composer comes back
    /// only with `ContentKind::Music`). A file not at its path is `missing`; a write that
    /// fails is `failed` and the rest go on. The record stays until [`UndoStore::forget`].
    pub fn restore(&self, id: &ApplyId, opts: &WriteOptions) -> Result<RestoreReport, TagError> {
        let dir = self.apply_dir(id)?;
        let manifest = load_manifest(&dir)?;
        let mut report = RestoreReport::default();
        for entry in &manifest.entries {
            let path = entry.path.clone();
            match restore_entry(&dir, entry, opts) {
                Ok(true) => report.restored.push(path),
                Ok(false) => report.missing.push(path),
                Err(err) => report.failed.push((path, err)),
            }
        }
        Ok(report)
    }

    /// Removes the record. A record that isn't there is fine.
    pub fn forget(&self, id: &ApplyId) -> Result<(), TagError> {
        let dir = self.apply_dir(id)?;
        match fs::remove_dir_all(&dir) {
            Err(err) if err.kind() != ErrorKind::NotFound => return Err(err.into()),
            _ => {}
        }
        sync_dir(&self.root);
        Ok(())
    }

    fn apply_dir(&self, id: &ApplyId) -> Result<PathBuf, TagError> {
        if is_safe_name(&id.0) {
            Ok(self.root.join(&id.0))
        } else {
            Err(TagError::Io(
                ErrorKind::InvalidInput,
                format!("{:?} isn't a safe undo record name", id.0),
            ))
        }
    }

    /// Every apply with a readable manifest.
    fn stored(&self) -> Result<Vec<Stored>, TagError> {
        let mut stored = Vec::new();
        for dir in apply_dirs(&self.root)? {
            let Ok(manifest) = load_manifest(&dir) else { continue };
            let mut bytes = 0;
            for item in fs::read_dir(&dir)? {
                bytes += item?.metadata()?.len();
            }
            let name = dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
            stored.push(Stored {
                name,
                seq: manifest.seq,
                bytes,
            });
        }
        Ok(stored)
    }

    /// Removes the oldest applies, never `keep`, until both limits hold.
    fn prune(&self, keep: &str) -> Result<(), TagError> {
        let mut stored = self.stored()?;
        stored.sort_by(|a, b| a.seq.cmp(&b.seq).then_with(|| a.name.cmp(&b.name)));
        let mut count = stored.len();
        let mut bytes: u64 = stored.iter().map(|s| s.bytes).sum();
        for old in stored.iter().filter(|s| s.name != keep) {
            if count <= self.limits.max_applies && bytes <= self.limits.max_bytes {
                break;
            }
            fs::remove_dir_all(self.root.join(&old.name))?;
            count -= 1;
            bytes = bytes.saturating_sub(old.bytes);
        }
        sync_dir(&self.root);
        Ok(())
    }
}

/// Not empty, no separator or NUL, no `..`, and no leading `.` (temp and hidden names).
fn is_safe_name(name: &str) -> bool {
    !name.is_empty() && !name.starts_with('.') && !name.contains(['/', '\\', '\0']) && !name.contains("..")
}

/// The directories under `root` whose names an [`ApplyId`] could have.
fn apply_dirs(root: &Path) -> Result<Vec<PathBuf>, TagError> {
    let mut dirs = Vec::new();
    for item in fs::read_dir(root)? {
        let item = item?;
        let named = item.file_name().to_str().is_some_and(is_safe_name);
        if named && item.file_type()?.is_dir() {
            dirs.push(item.path());
        }
    }
    Ok(dirs)
}

/// Writes the covers, then the manifest that makes the apply exist.
fn write_apply(dir: &Path, seq: u64, before: &[(PathBuf, FileTags)]) -> Result<(), TagError> {
    let entries = before
        .iter()
        .enumerate()
        .map(|(n, (path, tags))| {
            let cover = match &tags.cover {
                Some(cover) => {
                    let file = format!("{n}.cover");
                    write_atomically(dir, &file, &cover.data)?;
                    Some(CoverRef {
                        file,
                        mime: cover.mime.clone(),
                    })
                }
                None => None,
            };
            Ok(Entry {
                path: path.clone(),
                fields: tags.fields.clone(),
                extras: tags.extras.clone(),
                cover,
            })
        })
        .collect::<Result<Vec<_>, TagError>>()?;
    let manifest = Manifest {
        version: MANIFEST_VERSION,
        seq,
        entries,
    };
    write_manifest(dir, &manifest)
}

fn write_manifest(dir: &Path, manifest: &Manifest) -> Result<(), TagError> {
    let bytes = serde_json::to_vec(manifest).map_err(|e| TagError::Write(format!("the undo record: {e}")))?;
    write_atomically(dir, MANIFEST, &bytes)
}

/// The apply's manifest. No manifest is `TagError::Io(NotFound, _)`: the apply doesn't
/// exist (or its record never finished). One that doesn't parse is `TagError::Parse`.
fn load_manifest(dir: &Path) -> Result<Manifest, TagError> {
    let bytes = fs::read(dir.join(MANIFEST)).map_err(|e| match e.kind() {
        ErrorKind::NotFound => TagError::Io(ErrorKind::NotFound, "no undo record for this apply".to_owned()),
        _ => e.into(),
    })?;
    let manifest: Manifest =
        serde_json::from_slice(&bytes).map_err(|e| TagError::Parse(format!("the undo record: {e}")))?;
    if manifest.version != MANIFEST_VERSION {
        return Err(TagError::Parse(format!(
            "the undo record has version {}",
            manifest.version
        )));
    }
    let bad_cover = manifest
        .entries
        .iter()
        .filter_map(|e| e.cover.as_ref())
        .find(|c| !is_safe_name(&c.file) || c.file == MANIFEST);
    if let Some(cover) = bad_cover {
        return Err(TagError::Parse(format!("the undo record names cover {:?}", cover.file)));
    }
    Ok(manifest)
}

/// Writes one entry's old tags back: `Ok(false)` when the file isn't there.
fn restore_entry(dir: &Path, entry: &Entry, opts: &WriteOptions) -> Result<bool, TagError> {
    if !entry.path.try_exists()? {
        return Ok(false);
    }
    let cover = match &entry.cover {
        Some(cover) => Some(Cover {
            mime: cover.mime.clone(),
            data: fs::read(dir.join(&cover.file))?,
        }),
        None => None,
    };
    let tags = FileTags {
        fields: entry.fields.clone(),
        cover,
        extras: entry.extras.clone(),
    };
    match write_tags(&entry.path, &tags, opts) {
        Ok(()) => Ok(true),
        // Gone between the check and the write.
        Err(TagError::Io(ErrorKind::NotFound, _)) => Ok(false),
        Err(err) => Err(err),
    }
}

/// Writes `bytes` to `dir/name` through a `*.tmp` beside it: write, fsync, rename. A crash
/// leaves the old file or the new one, plus at worst a temp file that `open` removes.
fn write_atomically(dir: &Path, name: &str, bytes: &[u8]) -> Result<(), TagError> {
    let mut temp = tempfile::Builder::new()
        .prefix(name)
        .suffix(TEMP_SUFFIX)
        .tempfile_in(dir)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(dir.join(name)).map_err(|e| TagError::from(e.error))?;
    sync_dir(dir);
    Ok(())
}

/// Makes renames in `dir` durable. The files are in place either way, so a failure is only
/// logged, as in the safe overwrite.
fn sync_dir(dir: &Path) {
    if let Err(e) = File::open(dir).and_then(|d| d.sync_all()) {
        log::warn!(target: "cmdr_tags::undo", "Couldn't sync an undo folder: {e}");
    }
}
