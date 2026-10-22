//! Media fields for a session: what each file's EXIF and tags say, read once in
//! the background and cached for the session's life, and what the sheet asks of
//! that cache (progress, the picker's overview, a token's samples).
//!
//! `session.rs` owns one [`SessionMedia`] per session and hands it to every
//! preview as the [`MediaLookup`]. ❌ Nothing here takes `SESSIONS`, and no file
//! is read under any lock: the loader reads on blocking threads and takes the
//! cache's own lock only to store an answer.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use chrono::{Local, NaiveDateTime, TimeZone};
use serde::{Deserialize, Serialize};
use tauri_specta::Event;
use unicode_normalization::UnicodeNormalization;

use cmdr_fs::deadline::Deadline;
use cmdr_fs::entry::FileEntry;
use cmdr_fs::log_private::LogPrivate;
use cmdr_media_meta::{MediaFacts, MediaField, ReadError, VolumeByteSource, is_media_name, read_media_from};

use super::mask::{FieldGap, FieldText, MediaToken, RowFacts, RowMedia};
use super::plan::{MediaLookup, folder_names};
use crate::ignore_poison::IgnorePoison;

const LOG_TARGET: &str = "multi_rename::media";

/// How many files the loader reads at once. A NAS answers several requests in
/// parallel; MTP serializes inside its own backend, so no per-volume tuning.
pub(crate) const MAX_PARALLEL_READS: usize = 8;

/// The least time between two progress events; the last one (`done`) always goes.
const PROGRESS_EVERY: Duration = Duration::from_millis(250);

/// One file's read budget on a local disk; a volume with a live session gets
/// more (`io_budget_for_volume`).
const READ_BUDGET: Duration = Duration::from_secs(10);

/// How far a session's media read got. `total` counts media files only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MediaProgress {
    pub read: usize,
    pub total: usize,
    pub done: bool,
}

/// The `multi-rename-media-progress` event: a session's read moved on. At most
/// every 250 ms, plus a final one with `done`. ❗ The struct name kebab-cases to
/// the wire event name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct MultiRenameMediaProgress {
    pub session_id: String,
    pub read: usize,
    pub total: usize,
    pub done: bool,
}

/// What the field picker shows: the read's progress, and each field's example.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MediaOverview {
    pub progress: MediaProgress,
    /// Every field, in `MediaField::ALL` order.
    pub fields: Vec<MediaFieldOverview>,
}

/// One field in the picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MediaFieldOverview {
    pub field: MediaField,
    /// The field in its default format, from the first file in rename order that
    /// has it; `None` when no file read so far does.
    pub example: Option<String>,
    /// How many files read so far have it.
    pub files_with_it: usize,
}

/// What one `[=source.field…]` placeholder renders alone on the token editor's
/// sample rows, and how it fares across the whole session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FieldSamples {
    pub samples: Vec<FieldSample>,
    pub usage: FieldUsage,
}

/// The placeholder on one row: its text, or why it has none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FieldSample {
    /// The file's place in the session (what `PreviewRow::row` names).
    pub row: usize,
    pub text: Option<String>,
    pub gap: Option<FieldGap>,
}

/// How the session's files fare with one placeholder, for "38 files use the
/// modified date".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FieldUsage {
    /// Files with the field's own value.
    pub with_value: usize,
    /// Files without it that take the `|modified` fallback.
    pub fallback: usize,
    /// Files without it and with no fallback: problem rows.
    pub missing: usize,
    /// Files still being read.
    pub reading: usize,
}

/// One file the loader reads.
pub(crate) struct FileJob {
    /// Where it is on the session's volume.
    pub path: PathBuf,
    /// Its name, which picks the parser.
    pub name: String,
    /// Its size from the listing (`0` when the listing has none).
    pub size: u64,
    /// Set when the session goes: a read stops at its next fetch.
    pub cancel: Arc<AtomicBool>,
    /// The listing's facts the read was for, so a file that changed meanwhile is
    /// read again rather than cached with the old answer.
    stamp: Stamp,
}

/// Reads one file's facts. Runs on a blocking thread.
pub(crate) type FileReader = Arc<dyn Fn(&FileJob) -> Result<MediaFacts, ReadError> + Send + Sync>;

/// Hands a progress event to the sheet.
pub(crate) type ProgressEmit = Arc<dyn Fn(MultiRenameMediaProgress) + Send + Sync>;

/// What a session's loader reads with and reports to.
pub(crate) struct Loader {
    pub reader: FileReader,
    pub emit: ProgressEmit,
    /// How many files at once.
    pub parallel: usize,
}

impl Loader {
    /// Reads files on the volume `volume_id` (resolved per file, so a reconnect's
    /// new volume is the one read) and reports through `emit`.
    pub(crate) fn for_volume(volume_id: String, emit: ProgressEmit) -> Self {
        Self {
            reader: volume_reader(volume_id),
            emit,
            parallel: MAX_PARALLEL_READS,
        }
    }
}

/// The real [`FileReader`]: `read_media_from` over the file's bytes through
/// `Volume::read_range` (64 KiB blocks, cover art never fetched), within one
/// file's budget. ❗ Builds on the runtime that
/// calls it, so call it on the async executor.
fn volume_reader(volume_id: String) -> FileReader {
    let handle = tokio::runtime::Handle::current();
    Arc::new(move |job: &FileJob| {
        let volume = crate::file_system::volume::manager::get_volume_manager()
            .get(&volume_id)
            .ok_or_else(|| ReadError::Io("the session's volume isn't registered".to_string()))?;
        let budget = crate::file_system::volume::io_budget_for_volume(&volume_id, READ_BUDGET);
        let source = VolumeByteSource::new(
            volume,
            job.path.clone(),
            job.size,
            handle.clone(),
            Deadline::new(budget),
            Arc::clone(&job.cancel),
        );
        read_media_from(source, &job.name)
    })
}

/// What the listing said about a file when it was read: a different size or
/// date means it changed, and its facts are stale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    size: Option<u64>,
    modified: Option<u64>,
}

impl Stamp {
    fn of(entry: &FileEntry) -> Self {
        Self {
            size: entry.size,
            modified: entry.modified_at,
        }
    }
}

/// Where one file's read stands.
#[derive(Debug, Clone)]
pub(crate) enum MediaRead {
    Reading,
    Read(Arc<MediaFacts>),
    /// Why is in the log; the row only says it couldn't be read.
    Unreadable,
    /// A folder, or a file whose type isn't one we read.
    NotMedia,
}

impl MediaRead {
    fn row_media(&self) -> RowMedia<'_> {
        match self {
            Self::Reading => RowMedia::Reading,
            Self::Read(facts) => RowMedia::Read(facts),
            Self::Unreadable => RowMedia::Unreadable,
            Self::NotMedia => RowMedia::NotMedia,
        }
    }

    fn is_media(&self) -> bool {
        !matches!(self, Self::NotMedia)
    }

    fn is_reading(&self) -> bool {
        matches!(self, Self::Reading)
    }
}

/// One file in the cache.
struct Cached {
    is_directory: bool,
    stamp: Stamp,
    state: MediaRead,
}

/// The cache and the loader's bookkeeping, behind one lock.
struct Inner {
    files: HashMap<String, Cached>,
    /// Files waiting to be read, the next one first.
    queue: VecDeque<String>,
    /// Files a reader holds now.
    in_flight: HashSet<String>,
    /// Media files, and how many of them are no longer `Reading`.
    total: usize,
    read: usize,
    /// Set by `start`; a loader can only run once it's there.
    loader: Option<(Arc<Loader>, tokio::runtime::Handle)>,
    running: bool,
}

impl Inner {
    /// Sets `name`'s state, keeping the counts in step.
    fn set_state(&mut self, name: &str, state: MediaRead) {
        let Some(cached) = self.files.get_mut(name) else {
            return;
        };
        let was = std::mem::replace(&mut cached.state, state);
        let now = &cached.state;
        self.total = self.total + usize::from(now.is_media()) - usize::from(was.is_media());
        let done = |state: &MediaRead| state.is_media() && !state.is_reading();
        self.read = self.read + usize::from(done(now)) - usize::from(done(&was));
    }

    fn progress(&self) -> MediaProgress {
        MediaProgress {
            read: self.read,
            total: self.total,
            done: self.read == self.total,
        }
    }

    /// The next file to read, marked in flight; `None` when there's none or the
    /// session is going.
    fn next_job(&mut self, dir: &Path, cancel: &Arc<AtomicBool>) -> Option<FileJob> {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        while let Some(name) = self.queue.pop_front() {
            let Some(cached) = self.files.get(&name) else {
                continue;
            };
            if !cached.state.is_reading() || self.in_flight.contains(&name) {
                continue;
            }
            let stamp = cached.stamp;
            self.in_flight.insert(name.clone());
            return Some(FileJob {
                path: dir.join(&name),
                size: stamp.size.unwrap_or(0),
                name,
                cancel: Arc::clone(cancel),
                stamp,
            });
        }
        None
    }
}

/// One session's media: every file's read, the queue, and the loader's state.
pub(crate) struct SessionMedia {
    session_id: String,
    dir: PathBuf,
    cancel: Arc<AtomicBool>,
    inner: Mutex<Inner>,
}

impl SessionMedia {
    /// The cache for the files `names` (in rename order) of the folder at `dir`,
    /// whose listing entries are `entries`: a media file is `Reading` and queued in
    /// rename order, a folder or any other file `NotMedia`. Nothing reads until
    /// [`start`](Self::start).
    pub(crate) fn seed(session_id: &str, dir: PathBuf, names: &[String], entries: &[FileEntry]) -> Self {
        let by_name: HashMap<&str, &FileEntry> = entries.iter().map(|e| (e.name.as_str(), e)).collect();
        let mut files = HashMap::with_capacity(names.len());
        let mut queue = VecDeque::new();
        for name in names {
            let Some(entry) = by_name.get(name.as_str()) else {
                // Gone already: the preview shows it `Missing` and never asks.
                continue;
            };
            let state = if !entry.is_directory && is_media_name(name) {
                queue.push_back(name.clone());
                MediaRead::Reading
            } else {
                MediaRead::NotMedia
            };
            files.insert(
                name.clone(),
                Cached {
                    is_directory: entry.is_directory,
                    stamp: Stamp::of(entry),
                    state,
                },
            );
        }
        let total = queue.len();
        Self {
            session_id: session_id.to_string(),
            dir,
            cancel: Arc::default(),
            inner: Mutex::new(Inner {
                files,
                queue,
                in_flight: HashSet::new(),
                total,
                read: 0,
                loader: None,
                running: false,
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock_ignore_poison()
    }

    /// How far the read got.
    pub(crate) fn progress(&self) -> MediaProgress {
        self.lock().progress()
    }

    /// Starts reading with `loader`, on the runtime that calls this. A second
    /// call keeps the first loader.
    pub(crate) fn start(self: &Arc<Self>, loader: Loader) {
        let mut inner = self.lock();
        if inner.loader.is_none() {
            inner.loader = Some((Arc::new(loader), tokio::runtime::Handle::current()));
        }
        self.kick(&mut inner);
    }

    /// Runs the loader if it's started, idle, and has work.
    fn kick(self: &Arc<Self>, inner: &mut Inner) {
        if inner.running || inner.queue.is_empty() || self.is_cancelled() {
            return;
        }
        let Some((loader, handle)) = inner.loader.clone() else {
            return;
        };
        inner.running = true;
        handle.spawn(Arc::clone(self).run(loader));
    }

    /// Stops the loader: no new read starts, and a read in flight stops at its
    /// next fetch. For a session that's going.
    pub(crate) fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Reads the files `wanted` next (the rows the sheet shows), after `first`
    /// (the first file in rename order) while it's still waiting.
    pub(crate) fn prioritize<'a>(self: &Arc<Self>, first: Option<&'a str>, wanted: impl IntoIterator<Item = &'a str>) {
        let mut inner = self.lock();
        let mut front: Vec<String> = Vec::new();
        let mut seen = HashSet::new();
        for name in first.into_iter().chain(wanted) {
            let waiting =
                inner.files.get(name).is_some_and(|c| c.state.is_reading()) && !inner.in_flight.contains(name);
            if waiting && seen.insert(name) {
                front.push(name.to_string());
            }
        }
        inner.queue.retain(|name| !seen.contains(name.as_str()));
        for name in front.into_iter().rev() {
            inner.queue.push_front(name);
        }
        self.kick(&mut inner);
    }

    /// The cache as one preview's [`MediaLookup`], locked while the preview runs.
    /// [`Lookup::finish`] re-queues the files it found changed.
    pub(crate) fn lookup(self: &Arc<Self>) -> Lookup<'_> {
        Lookup {
            media: self,
            inner: self.lock(),
            changed: RefCell::new(Vec::new()),
        }
    }

    /// The loader: reads queued files, `parallel` at a time, until none is left
    /// or the session goes, then says it's done.
    async fn run(self: Arc<Self>, loader: Arc<Loader>) {
        let mut reads = tokio::task::JoinSet::new();
        let mut last_emit: Option<Instant> = None;
        loop {
            while reads.len() < loader.parallel.max(1) {
                let Some(job) = self.lock().next_job(&self.dir, &self.cancel) else {
                    break;
                };
                let reader = Arc::clone(&loader.reader);
                reads.spawn_blocking(move || {
                    // A parser that panics on a hostile file makes it unreadable, not the read stuck.
                    let result = catch_unwind(AssertUnwindSafe(|| reader(&job)))
                        .unwrap_or_else(|_| Err(ReadError::Io("the reader panicked".to_string())));
                    (job, result)
                });
            }
            let Some(joined) = reads.join_next().await else {
                if self.stop_if_idle() {
                    break;
                }
                continue;
            };
            match joined {
                Ok((job, result)) => self.store(job, result),
                // `catch_unwind` above leaves only a runtime shutdown here.
                Err(error) => log::debug!(target: LOG_TARGET, "a media read didn't come back: {error}"),
            }
            if !self.is_cancelled() && last_emit.is_none_or(|at| at.elapsed() >= PROGRESS_EVERY) {
                last_emit = Some(Instant::now());
                self.emit(&loader, false);
            }
        }
        if !self.is_cancelled() {
            self.emit(&loader, true);
        }
    }

    /// Ends the loader's run when nothing's queued (or the session went); `false`
    /// when work arrived since the queue last ran dry.
    fn stop_if_idle(&self) -> bool {
        let mut inner = self.lock();
        let has_work = !self.is_cancelled()
            && inner
                .queue
                .iter()
                .any(|name| inner.files.get(name).is_some_and(|c| c.state.is_reading()));
        if !has_work {
            inner.running = false;
            inner.queue.clear();
        }
        !has_work
    }

    fn emit(&self, loader: &Loader, done: bool) {
        let progress = self.progress();
        (loader.emit)(MultiRenameMediaProgress {
            session_id: self.session_id.clone(),
            read: progress.read,
            total: progress.total,
            done: done || progress.done,
        });
    }

    /// Stores a read's answer, unless the file changed meanwhile (then it's read
    /// again) or the session is going.
    fn store(&self, job: FileJob, result: Result<MediaFacts, ReadError>) {
        let mut inner = self.lock();
        inner.in_flight.remove(&job.name);
        let Some(cached) = inner.files.get(&job.name) else {
            return;
        };
        if !cached.state.is_reading() {
            return;
        }
        if cached.stamp != job.stamp {
            inner.queue.push_front(job.name);
            return;
        }
        let state = match result {
            Ok(facts) => MediaRead::Read(Arc::new(facts)),
            Err(ReadError::Cancelled) => return,
            Err(error) => {
                log::debug!(
                    target: LOG_TARGET,
                    "Couldn't read the media fields of {}: {error:?}",
                    LogPrivate::file_name(&job.name)
                );
                MediaRead::Unreadable
            }
        };
        inner.set_state(&job.name, state);
    }

    /// The picker's overview over the files `names` (in rename order).
    pub(crate) fn overview(&self, names: &[String]) -> MediaOverview {
        let inner = self.lock();
        let facts: Vec<&MediaFacts> = names
            .iter()
            .filter_map(|name| match &inner.files.get(name)?.state {
                MediaRead::Read(facts) => Some(facts.as_ref()),
                _ => None,
            })
            .collect();
        let fields = MediaField::ALL
            .into_iter()
            .map(|field| {
                let token = MediaToken::of(field);
                MediaFieldOverview {
                    field,
                    example: facts.iter().find_map(|facts| token.value_text(facts)),
                    files_with_it: facts.iter().filter(|facts| facts.get(field).is_some()).count(),
                }
            })
            .collect();
        MediaOverview {
            progress: inner.progress(),
            fields,
        }
    }

    /// `token` rendered alone on the files at `rows` of `names` (in rename order;
    /// a row past the end is left out), and its usage across all of them.
    pub(crate) fn field_samples(&self, token: &MediaToken, names: &[String], rows: &[usize]) -> FieldSamples {
        let inner = self.lock();
        let (parent, grandparent) = folder_names(&self.dir);
        let resolve = |position: usize, name: &str| -> Result<FieldText, FieldGap> {
            let cached = inner.files.get(name);
            // Composed, as the preview renders it (`plan.rs`).
            let file_name: String = name.nfc().collect();
            token.resolve(&RowFacts {
                file_name: &file_name,
                is_directory: cached.is_some_and(|c| c.is_directory),
                parent: &parent,
                grandparent: &grandparent,
                modified: cached.and_then(|c| c.stamp.modified).and_then(local_time),
                position,
                media: cached.map_or(RowMedia::NotMedia, |c| c.state.row_media()),
            })
        };
        let samples = rows
            .iter()
            .filter_map(|&row| Some((row, names.get(row)?)))
            .map(|(row, name)| match resolve(row, name) {
                Ok(text) => FieldSample {
                    row,
                    text: Some(text.into_text()),
                    gap: None,
                },
                Err(gap) => FieldSample {
                    row,
                    text: None,
                    gap: Some(gap),
                },
            })
            .collect();
        let mut usage = FieldUsage::default();
        for (position, name) in names.iter().enumerate() {
            match resolve(position, name) {
                Ok(FieldText::Value(_)) => usage.with_value += 1,
                Ok(FieldText::Fallback(_)) => usage.fallback += 1,
                Err(FieldGap::Reading) => usage.reading += 1,
                Err(FieldGap::NoValue { .. } | FieldGap::Unreadable { .. }) => usage.missing += 1,
            }
        }
        FieldSamples { samples, usage }
    }
}

/// A Unix-seconds date as local wall-clock time, as the preview reads `modified`.
fn local_time(unix_seconds: u64) -> Option<NaiveDateTime> {
    let seconds = i64::try_from(unix_seconds).ok()?;
    Local.timestamp_opt(seconds, 0).single().map(|t| t.naive_local())
}

/// The cache, locked for one preview. A file whose listing entry no longer
/// matches what was read answers `Reading`, and [`finish`](Self::finish) queues
/// it again.
pub(crate) struct Lookup<'a> {
    media: &'a Arc<SessionMedia>,
    inner: MutexGuard<'a, Inner>,
    changed: RefCell<Vec<(String, bool, Stamp)>>,
}

impl MediaLookup for Lookup<'_> {
    fn media(&self, entry: &FileEntry) -> RowMedia<'_> {
        let stamp = Stamp::of(entry);
        match self.inner.files.get(&entry.name) {
            Some(cached) if !cached.state.is_media() => RowMedia::NotMedia,
            Some(cached) if cached.stamp == stamp => cached.state.row_media(),
            // Changed since it was read, or new to the cache: read it (again).
            _ if !entry.is_directory && is_media_name(&entry.name) => {
                self.changed
                    .borrow_mut()
                    .push((entry.name.clone(), entry.is_directory, stamp));
                RowMedia::Reading
            }
            _ => RowMedia::NotMedia,
        }
    }
}

impl Lookup<'_> {
    /// Ends the preview: the files it found changed are `Reading` again, first in
    /// the queue, and the loader runs if it was idle.
    pub(crate) fn finish(self) {
        let Self {
            media,
            mut inner,
            changed,
        } = self;
        for (name, is_directory, stamp) in changed.into_inner() {
            inner.files.entry(name.clone()).or_insert(Cached {
                is_directory,
                stamp,
                state: MediaRead::NotMedia,
            });
            if let Some(cached) = inner.files.get_mut(&name) {
                cached.stamp = stamp;
            }
            inner.set_state(&name, MediaRead::Reading);
            inner.queue.push_front(name);
        }
        media.kick(&mut inner);
    }
}
