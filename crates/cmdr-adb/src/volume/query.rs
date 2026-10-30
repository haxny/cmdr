//! Reading what's on the device without changing it.
use std::path::Path;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{EntryKind, ListingProgress, VolumeError};
use tokio_util::sync::CancellationToken;

use super::AdbVolume;
use super::mapping::{stat_to_file_entry, with_link_target};
use super::paths::join_device_path;
use crate::errors::{EACCES, ENOENT, volume_error_from_errno};
use crate::sync::{SyncDirEntry, SyncEntryKind, SyncSession, SyncStat};

/// How many entries go by between two progress reports.
///
/// ❗ Never quieted: the pane's "Loading N entries…" line and the listing
/// watchdog both read it, and a quarter-million-entry `/sdcard/DCIM` with no
/// report looks exactly like a device that stopped answering.
const PROGRESS_EVERY: usize = 256;

impl AdbVolume {
    /// One `LIST` walk, as the device streams it.
    ///
    /// Cancellation is checked per entry, which is free here: the token is an
    /// atomic and the entries arrive on one socket.
    pub(super) async fn list_directory_impl(
        &self,
        path: &Path,
        on_progress: Option<&(dyn Fn(ListingProgress) + Sync)>,
        cancel: Option<&CancellationToken>,
    ) -> Result<Vec<FileEntry>, VolumeError> {
        let device = self.to_device_path(path)?;
        let mut session = self.open_sync(&device).await?;

        // ❗ `LIST` answers an unreadable or missing directory with an EMPTY
        // listing rather than an error. The stat here catches "missing" (and
        // "not a folder"); "unreadable" stats fine, so an empty listing asks the
        // shell afterwards (`refuse_if_unreadable`).
        let top = session
            .stat(&device)
            .await
            .map_err(|e| self.inner.map_adb_error(e, &device))?;
        if !top.exists() {
            session.quit().await;
            return Err(volume_error_from_errno(top.errno.unwrap_or(ENOENT), &device));
        }
        if top.kind() != SyncEntryKind::Directory {
            let followed = if top.kind() == SyncEntryKind::Symlink {
                self.follow(&mut session, &device).await
            } else {
                None
            };
            if followed.is_none_or(|t| t.kind() != SyncEntryKind::Directory) {
                session.quit().await;
                return Err(volume_error_from_errno(ENOTDIR, &device));
            }
        }

        let mut entries: Vec<FileEntry> = Vec::new();
        // Each link's index and DEVICE path: an entry carries the app path, and
        // `follow` asks the device, which only knows its own spelling.
        let mut symlinks: Vec<(usize, String)> = Vec::new();
        let mut tally = ListingProgress::default();
        let mut cancelled = false;
        let outcome = session
            .list(&device, &mut |entry: SyncDirEntry| {
                if cancel.is_some_and(CancellationToken::is_cancelled) {
                    cancelled = true;
                    return;
                }
                let child = join_device_path(&device, &entry.name);
                let Some(app_path) = self.to_app_path(&child) else {
                    return;
                };
                let built = stat_to_file_entry(&entry.name, &app_path.to_string_lossy(), &entry.stat);
                if built.is_symlink {
                    symlinks.push((entries.len(), child));
                }
                if built.is_directory {
                    tally.dirs += 1;
                } else {
                    tally.files += 1;
                    tally.bytes += built.size.unwrap_or(0);
                }
                entries.push(built);
                if let Some(on_progress) = on_progress
                    && entries.len().is_multiple_of(PROGRESS_EVERY)
                {
                    on_progress(tally);
                }
            })
            .await;
        if let Err(e) = outcome {
            session.quit().await;
            return Err(self.inner.map_adb_error(e, &device));
        }
        if cancelled {
            session.quit().await;
            return Err(VolumeError::Cancelled(device.into()));
        }
        if entries.is_empty()
            && let Err(e) = self.refuse_if_unreadable(&mut session, &device).await
        {
            session.quit().await;
            return Err(e);
        }

        // A link to a folder must navigate like one. One `stat` per symlink,
        // on the same socket, ❗ never per entry: a listing is mostly files.
        for (index, child) in symlinks {
            if let Some(target) = self.follow(&mut session, &child).await {
                let entry = std::mem::replace(
                    &mut entries[index],
                    FileEntry::new(String::new(), String::new(), false, false),
                );
                let entry = with_link_target(entry, &target);
                if entry.is_directory {
                    tally.dirs += 1;
                    tally.files = tally.files.saturating_sub(1);
                }
                entries[index] = entry;
            }
        }
        session.quit().await;

        if let Some(on_progress) = on_progress {
            on_progress(tally);
        }
        Ok(entries)
    }

    /// Tells an empty folder from one the device won't open, after `LIST`
    /// answered nothing: `adbd` sends the same bare `DONE` for both, and its
    /// `STAT` succeeds on a folder the shell user can't read (`/data` on a
    /// non-rooted phone). One `test -r && test -x` round trip, ONLY for an empty
    /// listing, read by its exit code ❌ never its stderr: 1 is "can't open",
    /// and the folder is stat'ed again so one deleted meanwhile says `NotFound`.
    /// Any other exit (2 for a `test` that can't parse, 127 for none) can't tell,
    /// so the listing stays empty.
    async fn refuse_if_unreadable(&self, session: &mut SyncSession, device: &str) -> Result<(), VolumeError> {
        let outcome = crate::shell::run_chain(
            &self.inner.endpoint,
            &self.inner.serial,
            &[&["test", "-r", device], &["test", "-x", device]],
        )
        .await
        .map_err(|e| self.inner.map_adb_error(e, device))?;
        if outcome.exit_code != 1 {
            return Ok(());
        }
        let again = session
            .stat(device)
            .await
            .map_err(|e| self.inner.map_adb_error(e, device))?;
        let errno = if again.exists() {
            EACCES
        } else {
            again.errno.unwrap_or(ENOENT)
        };
        Err(volume_error_from_errno(errno, device))
    }

    /// One `stat` round trip, as a `FileEntry`.
    pub(super) async fn get_metadata_impl(&self, path: &Path) -> Result<FileEntry, VolumeError> {
        let device = self.to_device_path(path)?;
        let mut session = self.open_sync(&device).await?;
        let stat = session
            .stat(&device)
            .await
            .map_err(|e| self.inner.map_adb_error(e, &device))?;
        if !stat.exists() {
            session.quit().await;
            return Err(volume_error_from_errno(stat.errno.unwrap_or(ENOENT), &device));
        }
        let name = Path::new(&device)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.name.clone());
        let app_path = self
            .to_app_path(&device)
            .ok_or_else(|| VolumeError::NotFound(path.to_string_lossy().into_owned()))?;
        let mut entry = stat_to_file_entry(&name, &app_path.to_string_lossy(), &stat);
        if entry.is_symlink
            && let Some(target) = self.follow(&mut session, &device).await
        {
            entry = with_link_target(entry, &target);
        }
        session.quit().await;
        Ok(entry)
    }

    /// What is at `path`, a link answered AS a link: one `STAT`, which the sync
    /// service answers as an `lstat`, and ❌ no `readlink` to follow it.
    pub(super) async fn entry_kind_impl(&self, path: &Path) -> Result<EntryKind, VolumeError> {
        let device = self.to_device_path(path)?;
        let mut session = self.open_sync(&device).await?;
        let stat = session.stat(&device).await;
        session.quit().await;
        let stat = stat.map_err(|e| self.inner.map_adb_error(e, &device))?;
        if !stat.exists() {
            return Err(volume_error_from_errno(stat.errno.unwrap_or(ENOENT), &device));
        }
        Ok(match stat.kind() {
            SyncEntryKind::Directory => EntryKind::Directory,
            SyncEntryKind::Symlink => EntryKind::Symlink,
            SyncEntryKind::File | SyncEntryKind::Other => EntryKind::File,
        })
    }

    /// Whether `path` is there, as a plain yes/no. A path off this volume
    /// doesn't exist ON THIS VOLUME, which is the question asked.
    pub(super) async fn exists_impl(&self, path: &Path) -> bool {
        self.get_metadata_impl(path).await.is_ok()
    }

    /// What a symlink at `device` points at, or `None` when the target is
    /// missing or the device declined to say.
    ///
    /// The sync service's `STAT` is an `lstat`, so the target is asked of the
    /// shell (`readlink -f`, in every Android `toybox`) and then stat'ed on the
    /// same sync socket. A `Symlink` answer again means `readlink` couldn't
    /// resolve a chain, which is left as a plain link rather than walked.
    pub(super) async fn follow(&self, session: &mut SyncSession, device: &str) -> Option<SyncStat> {
        let outcome = crate::shell::run(&self.inner.endpoint, &self.inner.serial, &["readlink", "-f", device])
            .await
            .ok()?;
        if !outcome.succeeded() {
            return None;
        }
        let target = outcome.stdout_text();
        let target = target.trim_end_matches(['\n', '\r']);
        if target.is_empty() {
            return None;
        }
        session.stat(target).await.ok().filter(SyncStat::exists)
    }
}

/// Linux `ENOTDIR`, which `crate::errors` has no name for: the device numbers
/// it, and every Android is Linux.
const ENOTDIR: i32 = 20;
