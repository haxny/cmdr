//! `Volume::open_random_reader` for SMB: one smb2 `FileReader` held open for many
//! positioned reads, so each read is one READ round trip instead of the
//! CREATE + READ + CLOSE that `read_range` pays per call.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::AtomicU8;

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::{Retirement, VolumeError, VolumeRandomReader};
use log::debug;
use smb2::client::stream::FileReader;

use super::SmbVolume;
use super::mapping::map_smb_error;
use super::session::update_state_on_smb_error;

/// A held-open SMB file. Carries what an error needs to update the share's
/// connection state, like the streaming-download producer does, because it
/// outlives the call that opened it and holds no `&SmbVolume`.
pub(super) struct SmbRandomReader {
    reader: FileReader,
    host: VolumeHost,
    state: Arc<AtomicU8>,
    retirement: Arc<Retirement>,
    volume_id: String,
    /// The app-addressable path, the payload of a path-carrying `VolumeError`.
    display_path: String,
}

impl SmbRandomReader {
    fn map_error(&self, op: &str, e: smb2::Error) -> VolumeError {
        update_state_on_smb_error(&self.host, &self.state, &self.retirement, &self.volume_id, &e);
        debug!(
            "SmbRandomReader::{op}(volume={:?}, path={:?}): {e}",
            self.volume_id, self.display_path
        );
        map_smb_error(e, &self.display_path)
    }
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
                host,
                state,
                retirement,
                volume_id,
                display_path,
            } = *self;
            reader.close().await.map_err(|e| {
                update_state_on_smb_error(&host, &state, &retirement, &volume_id, &e);
                map_smb_error(e, &display_path)
            })
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
            self.inner.share_name, smb_path
        );
        let (tree, conn) = self.clone_session().await?;
        let reader = self.handle_smb_result(
            "open_random_reader",
            &smb_path,
            tree.open_file_reader(conn, &smb_path).await,
        )?;
        if reader.info().is_directory {
            // Close it rather than leak the handle; the caller only needs the refusal.
            let _ = reader.close().await;
            return Err(VolumeError::IsADirectory(self.to_display_path(&smb_path)));
        }
        Ok(Box::new(SmbRandomReader {
            reader,
            host: self.inner.host().clone(),
            state: Arc::clone(&self.inner.state),
            retirement: Arc::clone(&self.inner.retirement),
            volume_id: self.inner.volume_id.clone(),
            display_path: self.to_display_path(&smb_path),
        }))
    }
}
