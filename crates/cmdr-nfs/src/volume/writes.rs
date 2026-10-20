//! The byte path INTO the export: `write_from_stream` and `create_file`.
//!
//! ❗ The engine owns staging (`write_is_single_shot` keeps `false`), so a
//! `CreateOrReplace` write is one WRITE stream onto the temp it hands us, ❌
//! never staged a second time here. The plan (`docs/specs/nfs-support-plan.md`
//! § "Write safety") has the UNSTABLE + COMMIT + verifier rules this fills.

use std::ops::ControlFlow;
use std::path::Path;

use cmdr_fs::volume::{StreamLength, StreamWriteProgress, StreamWritten, VolumeError, VolumeReadStream, WriteMode};

use super::NfsVolume;

impl NfsVolume {
    /// Streams `stream` (`length` bytes) to `dest`, reporting each acknowledged
    /// chunk through `on_progress`, whose `Break` cancels.
    pub(super) async fn write_from_stream_impl(
        &self,
        dest: &Path,
        mode: WriteMode,
        length: StreamLength,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &(dyn Fn(StreamWriteProgress) -> ControlFlow<()> + Sync),
    ) -> Result<StreamWritten, VolumeError> {
        let _ = (dest, mode, length, stream, on_progress);
        Err(VolumeError::NotSupported)
    }

    /// Creates `path` with `content`, refusing a taken name (`CREATE GUARDED`):
    /// ❌ never truncates.
    pub(super) async fn create_file_impl(&self, path: &Path, content: &[u8]) -> Result<(), VolumeError> {
        let _ = (path, content);
        Err(VolumeError::NotSupported)
    }
}
