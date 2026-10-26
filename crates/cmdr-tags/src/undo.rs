//! The undo store: before an apply writes tags, the caller has the store record each file's
//! old tags (the store reads them); undo writes them back, at the files' new paths after renames.
//!
//! One directory per apply under the root: `<id>/manifest.json` (the snapshot without cover
//! bytes) and `<id>/<k>.cover` (one file per distinct cover). A record is built in
//! `.<id>.staging` and renamed into place; an apply without a readable manifest doesn't
//! exist. Layout and rules: `DETAILS.md` § "Undo store layout".

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::model::{Cover, FileTags, TagError, TagField};
use crate::read::{RawTags, read_snapshot};
use crate::write::{WriteOptions, restore_tags};

const MANIFEST: &str = "manifest.json";
const COVER_SUFFIX: &str = ".cover";
const TEMP_SUFFIX: &str = ".tmp";
const STAGING_SUFFIX: &str = ".staging";
const MANIFEST_VERSION: u32 = 1;

/// Names one apply in the store, chosen by the caller (the journal's operation ID, say). It
/// becomes a directory name, so it must be one safe path component: not empty, no `/`, `\`,
/// or NUL, no `..`, and no leading `.`. Anything else is refused as
/// `TagError::Io(ErrorKind::InvalidInput, _)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReport {
    /// Files whose old tags are back.
    pub restored: Vec<PathBuf>,
    /// Files no longer at their (repathed) path.
    pub missing: Vec<PathBuf>,
    /// Files the write refused or failed on, unchanged, with why.
    pub failed: Vec<(PathBuf, TagError)>,
}

/// Snapshots of applies' old tags, in a directory the store owns.
///
/// One writer at a time: `record`, `repath`, `restore`, `forget`, and the pruning inside
/// `record` read and rewrite the store's directories without locking, so two calls at once
/// (or two stores on one root) can lose or half-prune a record. Callers serialize access;
/// the app keeps one store behind a `Mutex`.
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
    /// The totals and MP3 conductor, which `fields` doesn't hold.
    raw: RawTags,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoverRef {
    /// `<k>.cover` in the apply's directory, shared by every entry with the same bytes.
    file: String,
    mime: String,
}

/// An apply directory found in the store, for pruning.
struct Stored {
    name: String,
    age: Age,
    bytes: u64,
}

/// The pruning order, oldest first: applies whose manifest won't load (damaged, or written
/// by another version) go before every recorded one, by the directory's modification time;
/// recorded ones go by `seq`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Age {
    Unloadable(SystemTime),
    Recorded(u64),
}

impl UndoStore {
    /// Opens the store at `root`, creating it if needed. What a crash left behind goes: the
    /// staging directories of unfinished records, and the store's own temp files inside apply
    /// directories (nothing else). An apply directory whose manifest won't load can't be
    /// restored, but counts toward the limits and is pruned first.
    pub fn open(root: PathBuf, limits: UndoLimits) -> Result<Self, TagError> {
        fs::create_dir_all(&root)?;
        for item in fs::read_dir(&root)? {
            let item = item?;
            let staging = item.file_name().to_str().is_some_and(is_staging_name);
            if staging && item.file_type()?.is_dir() {
                fs::remove_dir_all(item.path())?;
            }
        }
        for dir in apply_dirs(&root)? {
            for item in fs::read_dir(&dir)? {
                let item = item?;
                let temp = item.file_name().to_str().is_some_and(is_store_temp);
                if temp && item.file_type()?.is_file() {
                    fs::remove_file(item.path())?;
                }
            }
        }
        Ok(Self { root, limits })
    }

    /// Records the current tags of an apply's files, replacing any record with the same `id`,
    /// then prunes the oldest applies past the limits. The store reads each file itself, once:
    /// its tags and what `FileTags` doesn't hold (track and disc totals, an MP3's `TPE3`), one
    /// consistent snapshot. Call it before the apply writes. A file that can't be read fails
    /// the record and nothing is stored. Returns once the record is on disk.
    pub fn record(&self, id: &ApplyId, paths: &[PathBuf]) -> Result<(), TagError> {
        let dir = self.apply_dir(id)?;
        let staging = self.root.join(format!(".{}{STAGING_SUFFIX}", id.0));
        let seq = self
            .stored()?
            .iter()
            .filter_map(|s| match s.age {
                Age::Recorded(seq) => Some(seq + 1),
                Age::Unloadable(_) => None,
            })
            .max()
            .unwrap_or(0);
        remove_dir_if_there(&staging)?;
        fs::create_dir(&staging)?;
        if let Err(err) = write_apply(&staging, seq, paths) {
            if let Err(cleanup) = fs::remove_dir_all(&staging) {
                log::warn!(target: "cmdr_tags::undo", "Couldn't remove an unfinished undo record: {cleanup}");
            }
            return Err(err);
        }
        // A crash between these two loses the old record of this `id`, which the caller is
        // replacing anyway; `open` removes the staging directory it leaves.
        remove_dir_if_there(&dir)?;
        fs::rename(&staging, &dir)?;
        sync_dir(&self.root);
        // The record is complete; a prune that doesn't work only leaves old records behind.
        if let Err(err) = self.prune(&id.0) {
            log::warn!(target: "cmdr_tags::undo", "Couldn't prune old undo records: {err}");
        }
        Ok(())
    }

    /// Points the record at the files' new paths after the apply renamed them. Each move is
    /// net: (the path as recorded, the file's final path), never an intermediate step of a
    /// chain. The moves apply all at once, so swaps work; a move whose old path isn't
    /// recorded is ignored.
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

    /// Writes each recorded file's old tags back as they were: as music (so a composer and an
    /// MP3's `TPE3` come back), with the totals recorded for that file. Of `opts` only
    /// `allow_frame_loss` applies. A file not at its path is `missing`; a write that fails is
    /// `failed` and the rest go on. The record stays until [`UndoStore::forget`].
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
        remove_dir_if_there(&self.apply_dir(id)?)?;
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

    /// Every apply directory, whether its manifest loads or not: one that doesn't (damaged, or
    /// from another version) can't be restored but still takes space and counts.
    fn stored(&self) -> Result<Vec<Stored>, TagError> {
        let mut stored = Vec::new();
        for dir in apply_dirs(&self.root)? {
            let age = match load_manifest(&dir) {
                Ok(manifest) => Age::Recorded(manifest.seq),
                Err(_) => Age::Unloadable(fs::metadata(&dir)?.modified()?),
            };
            let mut bytes = 0;
            for item in fs::read_dir(&dir)? {
                bytes += item?.metadata()?.len();
            }
            let name = dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
            stored.push(Stored { name, age, bytes });
        }
        Ok(stored)
    }

    /// Removes the oldest applies ([`Age`]), never `keep`, until both limits hold.
    fn prune(&self, keep: &str) -> Result<(), TagError> {
        let mut stored = self.stored()?;
        stored.sort_by(|a, b| a.age.cmp(&b.age).then_with(|| a.name.cmp(&b.name)));
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

/// `<k>.cover` with `k` ASCII letters and digits.
fn is_cover_name(name: &str) -> bool {
    name.strip_suffix(COVER_SUFFIX)
        .is_some_and(|stem| !stem.is_empty() && stem.bytes().all(|b| b.is_ascii_alphanumeric()))
}

/// `.<id>.staging`: a record [`UndoStore::record`] was building.
fn is_staging_name(name: &str) -> bool {
    name.strip_prefix('.')
        .and_then(|rest| rest.strip_suffix(STAGING_SUFFIX))
        .is_some_and(is_safe_name)
}

/// A temp file [`write_atomically`] makes: `manifest.json` or a cover name, then an optional
/// `.<random>`, then `.tmp`.
fn is_store_temp(name: &str) -> bool {
    let ours = |stem: &str| stem == MANIFEST || is_cover_name(stem);
    name.strip_suffix(TEMP_SUFFIX).is_some_and(|stem| {
        ours(stem)
            || stem
                .rsplit_once('.')
                .is_some_and(|(stem, random)| random.bytes().all(|b| b.is_ascii_alphanumeric()) && ours(stem))
    })
}

fn remove_dir_if_there(dir: &Path) -> Result<(), TagError> {
    match fs::remove_dir_all(dir) {
        Err(err) if err.kind() != ErrorKind::NotFound => Err(err.into()),
        _ => Ok(()),
    }
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

/// Reads each file, writes the covers, each distinct one once (a 100-chapter book shares
/// one), then the manifest that makes the apply exist.
fn write_apply(dir: &Path, seq: u64, paths: &[PathBuf]) -> Result<(), TagError> {
    let snapshots = paths
        .iter()
        .map(|path| read_snapshot(path).map(|(tags, raw)| (path, tags, raw)))
        .collect::<Result<Vec<_>, _>>()?;
    let mut files: HashMap<&[u8], String> = HashMap::new();
    let mut entries = Vec::with_capacity(snapshots.len());
    for (path, tags, raw) in &snapshots {
        let cover = match &tags.cover {
            Some(cover) => {
                let file = match files.get(cover.data.as_slice()) {
                    Some(file) => file.clone(),
                    None => {
                        let file = format!("{}{COVER_SUFFIX}", files.len());
                        write_atomically(dir, &file, &cover.data)?;
                        files.insert(&cover.data, file.clone());
                        file
                    }
                };
                Some(CoverRef {
                    file,
                    mime: cover.mime.clone(),
                })
            }
            None => None,
        };
        entries.push(Entry {
            path: (*path).clone(),
            fields: tags.fields.clone(),
            extras: tags.extras.clone(),
            cover,
            raw: raw.clone(),
        });
    }
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
        .find(|c| !is_cover_name(&c.file));
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
    match restore_tags(&entry.path, &tags, &entry.raw, opts.allow_frame_loss) {
        Ok(()) => Ok(true),
        // Gone between the check and the write.
        Err(TagError::Io(ErrorKind::NotFound, _)) => Ok(false),
        Err(err) => Err(err),
    }
}

/// Writes `bytes` to `dir/name` through `<name>.<random>.tmp` beside it: write, fsync,
/// rename. A crash leaves the old file or the new one, plus at worst a temp file that `open`
/// removes ([`is_store_temp`]).
fn write_atomically(dir: &Path, name: &str, bytes: &[u8]) -> Result<(), TagError> {
    let mut temp = tempfile::Builder::new()
        .prefix(&format!("{name}."))
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
