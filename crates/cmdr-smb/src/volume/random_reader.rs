//! `Volume::open_random_reader` for SMB: one smb2 `FileReader` held open for many
//! positioned reads, so each read is one READ round trip instead of the
//! CREATE + READ + CLOSE that `read_range` pays per call.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::{VolumeError, VolumeRandomReader};
use log::debug;
use smb2::client::stream::FileReader;

use super::mapping::map_smb_error;
use super::session::SessionLease;
use super::state::SessionGeneration;
use super::{SmbVolume, SmbVolumeInner};

/// A held-open SMB file. Carries the share and the session it was opened on, so
/// an error reaches the share's error funnel like the streaming-download
/// producer's does: it outlives the call that opened it and holds no
/// `&SmbVolume`.
pub(super) struct SmbRandomReader {
    reader: FileReader,
    share: Arc<SmbVolumeInner>,
    generation: SessionGeneration,
    /// The app-addressable path, the payload of a path-carrying `VolumeError`.
    display_path: String,
}

impl SmbRandomReader {
    fn map_error(&self, op: &str, e: smb2::Error) -> VolumeError {
        map_reader_error(&self.share, self.generation, op, &self.display_path, e)
    }
}

/// Reports `e` from session `generation` through the share's funnel and maps it.
fn map_reader_error(
    share: &SmbVolumeInner,
    generation: SessionGeneration,
    op: &str,
    display_path: &str,
    e: smb2::Error,
) -> VolumeError {
    share.note_smb_error(generation, &e);
    debug!(
        "SmbRandomReader::{op}(volume={:?}, path={:?}): {e}",
        share.volume_id,
        LogPrivate::file_path(display_path)
    );
    map_smb_error(e, display_path)
}

impl VolumeRandomReader for SmbRandomReader {
    fn size(&self) -> u64 {
        self.reader.size()
    }

    fn read_at(
        &self,
        offset: u64,
        len: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, VolumeError>> + Send + '_>> {
        Box::pin(async move {
            self.reader
                .read_at(offset, len as u64)
                .await
                .map_err(|e| self.map_error("read_at", e))
        })
    }

    fn close(self: Box<Self>) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send>> {
        Box::pin(async move {
            let Self {
                reader,
                share,
                generation,
                display_path,
            } = *self;
            reader
                .close()
                .await
                .map_err(|e| map_reader_error(&share, generation, "close", &display_path, e))
        })
    }
}

impl SmbVolume {
    /// The body of `Volume::open_random_reader`: one CREATE, then the handle is the
    /// caller's until it closes it.
    pub(super) async fn open_random_reader_impl(
        &self,
        path: &Path,
    ) -> Result<Box<dyn VolumeRandomReader>, VolumeError> {
        let smb_path = self.to_smb_path(path)?;
        debug!(
            "SmbVolume::open_random_reader: share={:?}, path={:?}",
            LogPrivate::volume(&self.inner.share_name),
            LogPrivate::file_path(&smb_path)
        );
        let SessionLease { tree, conn, generation } = self.clone_session().await?;
        let reader = self.handle_smb_result(
            "open_random_reader",
            &smb_path,
            generation,
            tree.open_file_reader(conn, &smb_path).await,
        )?;
        if reader.info().is_directory {
            // Close it rather than leak the handle; the caller only needs the refusal.
            let _ = reader.close().await;
            return Err(VolumeError::IsADirectory(self.to_display_path(&smb_path)));
        }
        Ok(Box::new(SmbRandomReader {
            reader,
            share: Arc::clone(&self.inner),
            generation,
            display_path: self.to_display_path(&smb_path),
        }))
    }
}
