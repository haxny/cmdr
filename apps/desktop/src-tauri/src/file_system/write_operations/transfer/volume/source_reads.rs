//! Telling a source that failed its read from a destination that failed its
//! write.
//!
//! A destination's `write_from_stream` pulls the source's chunks itself, so a
//! read that fails mid-file comes back as the WRITE's error, with nothing in it
//! saying which volume broke. [`watch`] wraps the source stream before it's
//! handed over and remembers whether one of its reads failed, so the engine
//! labels the failure with the side that actually refused
//! (`transfer_error.rs::PathedVolumeError`).

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::transfer_error::{PathRole, PathedVolumeError};
use crate::file_system::volume::{ScanStop, StreamLength, VolumeError, VolumeReadStream};
use cmdr_fs::metadata::{CarriedMetadata, MetadataClasses};

/// Whether a watched source stream failed a read. Shared with the stream, which
/// the destination owns once the write starts.
pub(super) struct SourceReads(Arc<AtomicBool>);

impl SourceReads {
    /// Labels a failed `write_from_stream` with the side that broke: the source,
    /// at `source_path`, when one of its reads failed, else the destination, at
    /// `dest_path`.
    pub(super) fn label(&self, error: VolumeError, source_path: &Path, dest_path: &Path) -> PathedVolumeError {
        if self.0.load(Ordering::Acquire) {
            PathedVolumeError::new(PathRole::Source, source_path, error)
        } else {
            PathedVolumeError::new(PathRole::Destination, dest_path, error)
        }
    }
}

/// Wraps `stream` so a failed read is remembered. Everything else forwards
/// untouched (a wrapper stream forwards its inner stream's answers).
pub(super) fn watch(stream: Box<dyn VolumeReadStream>) -> (Box<dyn VolumeReadStream>, SourceReads) {
    let failed = Arc::new(AtomicBool::new(false));
    let watched = Box::new(WatchedStream {
        inner: stream,
        failed: Arc::clone(&failed),
    });
    (watched, SourceReads(failed))
}

struct WatchedStream {
    inner: Box<dyn VolumeReadStream>,
    failed: Arc<AtomicBool>,
}

impl VolumeReadStream for WatchedStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            let chunk = self.inner.next_chunk().await;
            if matches!(chunk, Some(Err(_))) {
                self.failed.store(true, Ordering::Release);
            }
            chunk
        })
    }

    fn total_size(&self) -> StreamLength {
        self.inner.total_size()
    }

    fn bytes_read(&self) -> u64 {
        self.inner.bytes_read()
    }

    fn modified_at(&self) -> Option<std::time::SystemTime> {
        self.inner.modified_at()
    }

    fn metadata(&self, wanted: MetadataClasses) -> CarriedMetadata {
        self.inner.metadata(wanted)
    }

    fn metadata_outline(&self, wanted: MetadataClasses) -> CarriedMetadata {
        self.inner.metadata_outline(wanted)
    }

    fn stop_signal(&self) -> ScanStop {
        self.inner.stop_signal()
    }

    fn cancel_and_release(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        self.inner.cancel_and_release()
    }
}
