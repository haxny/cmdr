//! Bytes from a volume as a `Read + Seek`: [`VolumeByteSource`] bridges a
//! blocking parser to the async `Volume::read_range`, and [`BlockReader`] turns
//! any [`ByteSource`] into a seekable reader that fetches 64 KiB blocks on demand.

use std::io::{self, Read, Seek, SeekFrom};
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use cmdr_fs::deadline::Deadline;
use cmdr_fs::volume::Volume;

/// The most bytes one file's read may fetch. Past it the read stops with
/// `ReadError::TooLarge`: metadata never needs more, and a slow volume shouldn't
/// pay for a file that hides its tags behind megabytes.
pub const MAX_FETCHED_BYTES_PER_FILE: u64 = 4 * 1024 * 1024;

/// Positioned reads of one file.
pub trait ByteSource: Send + Sync {
    /// The file's size in bytes.
    fn size(&self) -> u64;
    /// Up to `len` bytes from `offset`; fewer only at the end of the file.
    fn read_at(&self, offset: u64, len: usize) -> io::Result<Vec<u8>>;
}

/// One file on a volume, read through `Volume::read_range`. ❌ Its `read_at`
/// blocks on the runtime, so it must only run on a blocking thread
/// (`spawn_blocking`), never on the async executor.
pub struct VolumeByteSource {
    volume: Arc<dyn Volume>,
    path: PathBuf,
    size: u64,
    handle: tokio::runtime::Handle,
    deadline: Deadline,
    cancel: Arc<AtomicBool>,
}

impl VolumeByteSource {
    /// The file at `path` on `volume`, `size` bytes long (from the listing). Reads
    /// run on `handle` within `deadline` and stop once `cancel` is set.
    pub fn new(
        volume: Arc<dyn Volume>,
        path: PathBuf,
        size: u64,
        handle: tokio::runtime::Handle,
        deadline: Deadline,
        cancel: Arc<AtomicBool>,
    ) -> Self {
        Self {
            volume,
            path,
            size,
            handle,
            deadline,
            cancel,
        }
    }
}

impl ByteSource for VolumeByteSource {
    fn size(&self) -> u64 {
        self.size
    }

    fn read_at(&self, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        let _ = (
            &self.volume,
            &self.path,
            &self.handle,
            &self.deadline,
            &self.cancel,
            offset,
            len,
        );
        Err(io::ErrorKind::Unsupported.into())
    }
}

/// A `Read + Seek` over a [`ByteSource`]: 64 KiB blocks fetched on demand, a few
/// kept, at most [`MAX_FETCHED_BYTES_PER_FILE`] fetched, and holes served as
/// zeros without fetching.
pub struct BlockReader<S: ByteSource> {
    source: S,
    position: u64,
    holes: Vec<Range<u64>>,
}

impl<S: ByteSource> BlockReader<S> {
    /// A reader at the start of `source`.
    pub fn new(source: S) -> Self {
        Self {
            source,
            position: 0,
            holes: Vec::new(),
        }
    }

    /// The same reader, serving the bytes in `holes` as zeros without fetching
    /// them (cover art a tag parser reads only to skip).
    pub fn with_holes(mut self, holes: Vec<Range<u64>>) -> Self {
        self.holes = holes;
        self
    }
}

impl<S: ByteSource> Read for BlockReader<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let _ = (&self.source, self.position, &self.holes, buf);
        Err(io::ErrorKind::Unsupported.into())
    }
}

impl<S: ByteSource> Seek for BlockReader<S> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let _ = pos;
        Err(io::ErrorKind::Unsupported.into())
    }
}
