//! Opening a repo with its password, and reading its snapshot list.
//!
//! Every `rustic_core` call runs on `spawn_blocking` (the storage adapter
//! `block_on`s the parent volume, which would panic on an async worker), so
//! each public method here is async and hops to a blocking thread once.

use crate::backend::{VolumeBackend, volume_error_in};
use crate::snapshots::{RawSnapshot, SnapshotList, snapshot_list};
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::{Volume, VolumeError};
use rustic_core::jiff::tz::TimeZone;
use rustic_core::{
    Credentials, FileType, OpenStatus, ReadBackend, Repository, RepositoryBackends, RepositoryOptions, RusticError,
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::runtime::Handle;
use zeroize::Zeroizing;

/// Why a repo couldn't be opened or read. Typed, so the host words each one.
#[derive(Debug, Clone)]
pub enum ResticError {
    /// The password doesn't unlock any of the repo's keys.
    WrongPassword,
    /// The folder has no restic `config`, so it isn't a repo (any more).
    NotARepository,
    /// The volume the repo lives on couldn't answer (a dropped share, no access).
    Volume(VolumeError),
    /// rustic couldn't read the repo for another reason (a damaged or
    /// unsupported repo, a crashed worker).
    Unreadable {
        /// rustic's own message, for the log. ❌ Never shown to a user or
        /// matched on.
        detail: String,
    },
}

/// An unlocked restic repo, read through the volume it lives on.
///
/// Holds rustic's opened (not yet indexed) repository: enough for the snapshot
/// list, which never needs the index.
pub struct ResticRepo {
    pub(crate) repo: Arc<Repository<OpenStatus>>,
    pub(crate) backend: Arc<VolumeBackend>,
    runtime: Handle,
}

impl ResticRepo {
    /// Opens the repo in the folder `root` on `parent`, and unlocks it with
    /// `password`.
    ///
    /// The key derivation (scrypt) takes about a second by design, on a blocking
    /// thread. `cache_dir` is where rustic may keep its local cache of snapshot
    /// and index files; `None` runs without one. Opening never writes to the repo
    /// and never takes a restic lock.
    pub async fn open(
        parent: Arc<dyn Volume>,
        root: PathBuf,
        password: &str,
        cache_dir: Option<PathBuf>,
        host: &VolumeHost,
    ) -> Result<Self, ResticError> {
        let runtime = host.runtime();
        let backend = Arc::new(VolumeBackend::new(parent, root, runtime.clone()));
        let password = Zeroizing::new(password.to_string());
        let task_backend = Arc::clone(&backend);
        let repo = runtime
            .spawn_blocking(move || {
                let mut options = RepositoryOptions::default();
                options.no_cache = cache_dir.is_none();
                options.cache_dir = cache_dir;
                let backends = RepositoryBackends::new(Arc::clone(&task_backend) as _, None);
                Repository::new(&options, &backends)
                    .and_then(|repo| repo.open(&Credentials::password(password.as_str())))
                    .map_err(|err| classify(&err, &task_backend))
            })
            .await
            .map_err(worker_crashed)??;
        Ok(Self {
            repo: Arc::new(repo),
            backend,
            runtime,
        })
    }

    /// Every snapshot in the repo, named and ordered (local time from the
    /// system's zone). Reads only the snapshot files, never the index.
    pub async fn snapshots(&self) -> Result<SnapshotList, ResticError> {
        let repo = Arc::clone(&self.repo);
        let backend = Arc::clone(&self.backend);
        self.runtime
            .spawn_blocking(move || {
                let snapshots = repo.get_all_snapshots().map_err(|err| classify(&err, &backend))?;
                let raw = snapshots
                    .into_iter()
                    .map(|s| RawSnapshot {
                        id: *s.id,
                        time: s.time.timestamp(),
                        hostname: s.hostname,
                        paths: s.paths.iter().cloned().collect(),
                        tags: s.tags.iter().cloned().collect(),
                    })
                    .collect();
                Ok(snapshot_list(raw, &TimeZone::system()))
            })
            .await
            .map_err(worker_crashed)?
    }
}

/// Sorts a rustic failure into a [`ResticError`] by type, never by text.
///
/// rustic marks a wrong password with its own error code, read through
/// `is_incorrect_password()`. A parent failure rides in the error's source chain
/// as the typed [`VolumeError`] the adapter put there. Anything else is a missing
/// repo when the config is gone (asked of the parent again, which is cheap and
/// typed), and unreadable otherwise. Runs on the blocking thread.
fn classify(err: &RusticError, backend: &VolumeBackend) -> ResticError {
    if err.is_incorrect_password() {
        return ResticError::WrongPassword;
    }
    if let Some(volume_error) = volume_error_in(err) {
        return ResticError::Volume(volume_error.clone());
    }
    if backend
        .list_with_size(FileType::Config)
        .is_ok_and(|config| config.is_empty())
    {
        return ResticError::NotARepository;
    }
    ResticError::Unreadable {
        detail: err.display_log(),
    }
}

fn worker_crashed(err: tokio::task::JoinError) -> ResticError {
    ResticError::Unreadable {
        detail: format!("the blocking worker stopped: {err}"),
    }
}

impl std::fmt::Debug for ResticRepo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResticRepo")
            .field("name", &self.repo.name)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[path = "repo_tests.rs"]
mod tests;
