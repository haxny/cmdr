//! A volume whose session a test can take away and give back: the double for
//! `volume_outage.rs`, the way `liveness_test_support.rs` is for the watchdog.
//!
//! It behaves like the SMB share in `#398`. While its session is down, every
//! call fails at once with `DeviceDisconnected` and
//! [`Volume::connection_state`] reads `Disconnected` (what `cmdr-smb`'s
//! `check_connection` does). [`Volume::attempt_reconnect`] succeeds only once
//! the test has made the server reachable again, and a test can also arm the
//! session to drop at the start of the Nth write, which is the mid-copy outage,
//! or around the Nth rename: before the server sees it, or after the server
//! performed it but before its reply arrived (the landing a drop swallows).
//!
//! Wraps an `InMemoryVolume` (rather than owning one) so a test can read the
//! destination back through the same handle after the copy.

use crate::file_system::volume::{
    ConnectionState, DirectoryCreation, InMemoryVolume, LaneKey, ListingProgress, ScanConflict, SourceItemInfo,
    SpaceInfo, StreamLength, StreamWriteProgress, Volume, VolumeError, VolumeReadStream, WriteMode,
};
use crate::ignore_poison::IgnorePoison;
use cmdr_fs::entry::FileEntry;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

type Fut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub(crate) struct OutageVolume {
    inner: Arc<InMemoryVolume>,
    /// Its own lane, so tests running in parallel never share an outage record.
    lane: String,
    /// Whether the session is up: what `connection_state` reports and what
    /// every call checks first.
    up: AtomicBool,
    /// Whether a reconnect would succeed right now: the server is back.
    reachable: AtomicBool,
    /// A reconnect is refused for its credentials, as after a password change.
    needs_sign_in: AtomicBool,
    reconnect_attempts: AtomicUsize,
    writes: AtomicUsize,
    /// Drop the session at the start of this (0-based) write.
    drop_at_write: Mutex<Option<usize>>,
    renames: AtomicUsize,
    /// Drop the session around this (0-based) rename, and on which side of it.
    drop_at_rename: Mutex<Option<(usize, RenameDrop)>>,
}

/// Which side of a rename the session drops on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RenameDrop {
    /// The request never reaches the server: nothing moves.
    BeforeTheServerSeesIt,
    /// The server performs the rename, and the reply is lost with the session.
    AfterTheServerPerformsIt,
}

impl OutageVolume {
    pub(crate) fn wrap(inner: Arc<InMemoryVolume>, lane: &str) -> Arc<Self> {
        Arc::new(Self {
            inner,
            lane: lane.to_owned(),
            up: AtomicBool::new(true),
            reachable: AtomicBool::new(true),
            needs_sign_in: AtomicBool::new(false),
            reconnect_attempts: AtomicUsize::new(0),
            writes: AtomicUsize::new(0),
            drop_at_write: Mutex::new(None),
            renames: AtomicUsize::new(0),
            drop_at_rename: Mutex::new(None),
        })
    }

    /// The session drops and the server becomes unreachable.
    pub(crate) fn go_away(&self) {
        self.reachable.store(false, Ordering::SeqCst);
        self.up.store(false, Ordering::SeqCst);
    }

    /// The server is reachable again; the session stays down until someone
    /// reconnects it.
    pub(crate) fn become_reachable(&self) {
        self.reachable.store(true, Ordering::SeqCst);
    }

    /// Somebody else (a pane's own backoff) reconnected the session.
    pub(crate) fn reconnected_elsewhere(&self) {
        self.reachable.store(true, Ordering::SeqCst);
        self.up.store(true, Ordering::SeqCst);
    }

    pub(crate) fn require_sign_in(&self) {
        self.needs_sign_in.store(true, Ordering::SeqCst);
    }

    /// The session drops (and the server goes away) the moment write number
    /// `n` (0-based) starts.
    pub(crate) fn drop_at_write(&self, n: usize) {
        *self.drop_at_write.lock_ignore_poison() = Some(n);
    }

    /// The session drops (and the server goes away) around rename number `n`
    /// (0-based), on the given side of it.
    pub(crate) fn drop_at_rename(&self, n: usize, side: RenameDrop) {
        *self.drop_at_rename.lock_ignore_poison() = Some((n, side));
    }

    pub(crate) fn reconnect_attempts(&self) -> usize {
        self.reconnect_attempts.load(Ordering::SeqCst)
    }

    fn gate(&self) -> Result<(), VolumeError> {
        if self.up.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(VolumeError::DeviceDisconnected("the session is down".to_owned()))
        }
    }
}

impl Volume for OutageVolume {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn root(&self) -> &Path {
        self.inner.root()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn lane_key(&self) -> LaneKey {
        LaneKey::new(self.lane.clone())
    }
    fn supports_streaming(&self) -> bool {
        true
    }
    fn max_concurrent_ops(&self) -> usize {
        self.inner.max_concurrent_ops()
    }
    fn connection_state(&self) -> Option<ConnectionState> {
        Some(
            if self.needs_sign_in.load(Ordering::SeqCst) && !self.up.load(Ordering::SeqCst) {
                ConnectionState::NeedsSignIn
            } else if self.up.load(Ordering::SeqCst) {
                ConnectionState::Direct
            } else {
                ConnectionState::Disconnected
            },
        )
    }
    fn attempt_reconnect<'a>(&'a self) -> Fut<'a, Result<(), VolumeError>> {
        Box::pin(async move {
            self.reconnect_attempts.fetch_add(1, Ordering::SeqCst);
            if self.needs_sign_in.load(Ordering::SeqCst) {
                return Err(VolumeError::PermissionDenied {
                    path: "/".to_owned(),
                    raw_os_error: None,
                });
            }
            if self.reachable.load(Ordering::SeqCst) {
                self.up.store(true, Ordering::SeqCst);
                Ok(())
            } else {
                Err(VolumeError::ConnectionTimeout("the server didn't answer".to_owned()))
            }
        })
    }
    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Fut<'a, Result<Vec<FileEntry>, VolumeError>> {
        Box::pin(async move {
            self.gate()?;
            self.inner.list_directory(path, on_progress).await
        })
    }
    fn get_metadata<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<FileEntry, VolumeError>> {
        Box::pin(async move {
            self.gate()?;
            self.inner.get_metadata(path).await
        })
    }
    fn exists<'a>(&'a self, path: &'a Path) -> Fut<'a, bool> {
        Box::pin(async move { self.gate().is_ok() && self.inner.exists(path).await })
    }
    fn is_directory<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<bool, VolumeError>> {
        Box::pin(async move {
            self.gate()?;
            self.inner.is_directory(path).await
        })
    }
    fn create_directory<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<(), VolumeError>> {
        Box::pin(async move {
            self.gate()?;
            self.inner.create_directory(path).await
        })
    }
    fn create_directory_all<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<DirectoryCreation, VolumeError>> {
        Box::pin(async move {
            self.gate()?;
            self.inner.create_directory_all(path).await
        })
    }
    fn create_file<'a>(&'a self, path: &'a Path, content: &'a [u8]) -> Fut<'a, Result<(), VolumeError>> {
        Box::pin(async move {
            self.gate()?;
            self.inner.create_file(path, content).await
        })
    }
    fn delete<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<(), VolumeError>> {
        Box::pin(async move {
            self.gate()?;
            self.inner.delete(path).await
        })
    }
    fn rename<'a>(&'a self, from: &'a Path, to: &'a Path, force: bool) -> Fut<'a, Result<(), VolumeError>> {
        Box::pin(async move {
            let n = self.renames.fetch_add(1, Ordering::SeqCst);
            let armed = *self.drop_at_rename.lock_ignore_poison();
            match armed {
                Some((at, RenameDrop::BeforeTheServerSeesIt)) if at == n => self.go_away(),
                Some((at, RenameDrop::AfterTheServerPerformsIt)) if at == n => {
                    self.gate()?;
                    self.inner.rename(from, to, force).await?;
                    self.go_away();
                }
                _ => {}
            }
            self.gate()?;
            self.inner.rename(from, to, force).await
        })
    }
    fn get_space_info<'a>(&'a self) -> Fut<'a, Result<SpaceInfo, VolumeError>> {
        Box::pin(async move {
            self.gate()?;
            self.inner.get_space_info().await
        })
    }
    fn open_read_stream<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<Box<dyn VolumeReadStream>, VolumeError>> {
        Box::pin(async move {
            self.gate()?;
            self.inner.open_read_stream(path).await
        })
    }
    fn create_directory_errors_on_existing_dir(&self) -> bool {
        self.inner.create_directory_errors_on_existing_dir()
    }
    fn scan_for_conflicts<'a>(
        &'a self,
        source_items: &'a [SourceItemInfo],
        dest_path: &'a Path,
    ) -> Fut<'a, Result<Vec<ScanConflict>, VolumeError>> {
        Box::pin(async move {
            self.gate()?;
            self.inner.scan_for_conflicts(source_items, dest_path).await
        })
    }
    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        mode: WriteMode,
        length: StreamLength,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(StreamWriteProgress) -> std::ops::ControlFlow<()> + Sync),
    ) -> Fut<'a, Result<u64, VolumeError>> {
        Box::pin(async move {
            let n = self.writes.fetch_add(1, Ordering::SeqCst);
            if *self.drop_at_write.lock_ignore_poison() == Some(n) {
                self.go_away();
            }
            self.gate()?;
            self.inner
                .write_from_stream(dest, mode, length, stream, on_progress)
                .await
        })
    }
}
