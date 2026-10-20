//! One file of a snapshot as a [`VolumeReadStream`]: bounded chunks, each one
//! `read_file_at` on a blocking thread.

use crate::hub::RepoSlot;
use crate::repo::{IndexedRepo, worker_crashed};
use cmdr_fs::metadata::{CarriedMetadata, MetadataClasses};
use cmdr_fs::volume::{StreamLength, VolumeError, VolumeReadStream};
use rustic_core::vfs::OpenFile;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::runtime::Handle;

/// How much one chunk reads. The house streaming size: peak memory per transfer
/// stays one chunk, and a 1 MiB read spans a handful of blobs.
pub(crate) const CHUNK_SIZE: usize = 1024 * 1024;

/// A snapshot file's bytes, read in [`CHUNK_SIZE`] windows.
pub(crate) struct ResticReadStream {
    pub(crate) repo: Arc<IndexedRepo>,
    pub(crate) file: Arc<OpenFile>,
    pub(crate) runtime: Handle,
    /// The path asked for, for the error a short read answers.
    pub(crate) path: PathBuf,
    pub(crate) size: u64,
    pub(crate) modified: Option<SystemTime>,
    /// The node's permission bits (see `tree::permissions_of`).
    pub(crate) mode: u32,
    /// The repo, when the file was opened under `latest/`: every chunk counts as
    /// a read there, so `latest` stays put for the whole copy.
    pub(crate) latest_of: Option<Arc<RepoSlot>>,
    pub(crate) read: u64,
}

impl VolumeReadStream for ResticReadStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            if self.read >= self.size {
                return None;
            }
            if let Some(slot) = &self.latest_of {
                slot.touch_latest();
            }
            let want = usize::try_from(self.size - self.read).map_or(CHUNK_SIZE, |left| left.min(CHUNK_SIZE));
            let offset = usize::try_from(self.read).unwrap_or(usize::MAX);
            let chunk = read_at(&self.runtime, &self.repo, &self.file, offset, want)
                .await
                .map_err(|err| err.into_volume_error(&self.path));
            Some(match chunk {
                // A file that ends before its recorded size is a damaged snapshot;
                // feeding a short file to a copy would land it silently.
                Ok(bytes) if bytes.is_empty() => Err(VolumeError::SourceChanged(self.path.display().to_string())),
                Ok(bytes) => {
                    self.read += bytes.len() as u64;
                    Ok(bytes)
                }
                Err(err) => Err(err),
            })
        })
    }

    fn total_size(&self) -> StreamLength {
        StreamLength::Known(self.size)
    }

    fn bytes_read(&self) -> u64 {
        self.read
    }

    fn modified_at(&self) -> Option<SystemTime> {
        self.modified
    }

    /// The node's permission bits and nothing else: rustic's nodes carry no
    /// creation date, and restoring flags or xattrs isn't this view's job.
    fn metadata(&self, wanted: MetadataClasses) -> CarriedMetadata {
        CarriedMetadata {
            mode: Some(self.mode),
            ..CarriedMetadata::default()
        }
        .restricted_to(wanted)
    }
}

/// Reads `len` bytes of `file` at `offset` on a blocking thread. Fewer only at
/// the end of the file.
pub(crate) async fn read_at(
    runtime: &Handle,
    repo: &Arc<IndexedRepo>,
    file: &Arc<OpenFile>,
    offset: usize,
    len: usize,
) -> Result<Vec<u8>, crate::repo::ResticError> {
    let (repo, file) = (Arc::clone(repo), Arc::clone(file));
    runtime
        .spawn_blocking(move || {
            repo.repo
                .read_file_at(&file, offset, len)
                .map(|bytes| bytes.to_vec())
                .map_err(|err| repo.classify(&err))
        })
        .await
        .map_err(worker_crashed)?
}
