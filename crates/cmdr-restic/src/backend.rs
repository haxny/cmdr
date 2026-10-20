//! The storage adapter: rustic's blocking `ReadBackend` over a Cmdr `Volume`.
//!
//! rustic reads a repo through a tiny, blocking storage trait. This implements it
//! on top of the volume the repo lives on, so a repo on an SMB share, SFTP, S3,
//! or a phone opens the same way a local one does. Every method `block_on`s the
//! parent's async call on the captured runtime handle, which is only sound off
//! the runtime's own workers: rustic is only ever driven from `spawn_blocking`
//! (`repo.rs`). Same bridge as `cmdr-archive`'s `VolumeByteSource`.

use bytes::Bytes;
use cmdr_fs::volume::{Volume, VolumeError};
use rustic_core::repofile::KeyFile;
use rustic_core::{BytesList, ErrorKind, FileType, Id, ReadBackend, RusticError, RusticResult, WriteBackend};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::runtime::Handle;

/// Reads a restic repo's files through the volume it lives on, and refuses every
/// write.
pub(crate) struct VolumeBackend {
    parent: Arc<dyn Volume>,
    /// The repo's folder on `parent`: the one holding `config`.
    root: PathBuf,
    handle: Handle,
    /// How many writes rustic tried. Every one is refused; the count is what lets
    /// a test pin "browsing never even tries to write".
    write_attempts: AtomicUsize,
}

impl VolumeBackend {
    pub(crate) fn new(parent: Arc<dyn Volume>, root: PathBuf, handle: Handle) -> Self {
        Self {
            parent,
            root,
            handle,
            write_attempts: AtomicUsize::new(0),
        }
    }

    /// How many write calls rustic made (all refused).
    #[cfg(test)]
    pub(crate) fn write_attempts(&self) -> usize {
        self.write_attempts.load(Ordering::Relaxed)
    }

    /// Where a file of type `tpe` lives, per the restic repo layout. Packs fan out
    /// into 256 subfolders by their id's first two hex digits.
    fn path_of(&self, tpe: FileType, id: &Id) -> PathBuf {
        match tpe {
            FileType::Config => self.root.join("config"),
            FileType::Pack => {
                let hex = id.to_hex();
                self.root.join(tpe.dirname()).join(&hex[..2]).join(&*hex)
            }
            FileType::Index | FileType::Key | FileType::Snapshot => self.root.join(tpe.dirname()).join(&*id.to_hex()),
        }
    }

    /// The files in `dir` whose names are restic ids, with their sizes. A missing
    /// folder lists as empty (`index/` can be empty, and some copies drop it).
    fn list_ids_in(&self, dir: &Path) -> RusticResult<Vec<(Id, u32)>> {
        let entries = match self.handle.block_on(self.parent.list_directory(dir, None)) {
            Ok(entries) => entries,
            Err(VolumeError::NotFound(_)) => return Ok(Vec::new()),
            Err(err) => return Err(backend_error(err)),
        };
        Ok(entries
            .into_iter()
            .filter(|e| !e.is_directory)
            .filter_map(|e| {
                let id = e.name.parse::<Id>().ok()?;
                Some((id, size_u32(e.size.unwrap_or(0))))
            })
            .collect())
    }

    /// Streams a whole file. Used for the small files (config, keys, snapshots,
    /// index) and as the fallback for a parent without ranged reads.
    fn read_stream(&self, path: &Path, stop_after: Option<usize>) -> Result<Vec<u8>, VolumeError> {
        self.handle.block_on(async {
            let mut stream = self.parent.open_read_stream(path).await?;
            let mut out = Vec::new();
            while let Some(chunk) = stream.next_chunk().await {
                out.extend_from_slice(&chunk?);
                if stop_after.is_some_and(|limit| out.len() >= limit) {
                    break;
                }
            }
            Ok(out)
        })
    }
}

impl VolumeBackend {
    /// The key files rustic may try a password on: at most [`MAX_KEYS`] of them
    /// (the first by id), each a real key file whose scrypt cost is tolerable.
    ///
    /// rustic tries every key it lists, and passes each file's own `N`, `r`, and
    /// `p` (plaintext JSON) straight to scrypt with no ceiling. Anyone can leave
    /// a folder that looks like a repo on a shared drive, so without this a
    /// typed password could ask scrypt for terabytes (an allocation abort) or
    /// thousands of one-second derivations. rustic finds keys only through this
    /// listing, so a key skipped here is never derived.
    fn tolerable_keys(&self, mut keys: Vec<(Id, u32)>) -> RusticResult<Vec<(Id, u32)>> {
        keys.sort_unstable_by_key(|(id, _)| *id);
        keys.truncate(MAX_KEYS);
        let mut tolerable = Vec::with_capacity(keys.len());
        for (id, size) in keys {
            if size > MAX_KEY_FILE_BYTES {
                continue;
            }
            let bytes = self
                .read_stream(&self.path_of(FileType::Key, &id), None)
                .map_err(backend_error)?;
            let Ok(key) = serde_json::from_slice::<KeyFile>(&bytes) else {
                continue;
            };
            if scrypt_cost_is_tolerable(key.n, key.r, key.p) {
                tolerable.push((id, size));
            }
        }
        Ok(tolerable)
    }
}

/// The most key files offered to rustic. A real repo has a handful (one per
/// person or machine); a password typed into a crafted one with thousands would
/// otherwise cost a second of scrypt per key.
pub(crate) const MAX_KEYS: usize = 32;

/// The largest file read as a key. A real one is about 450 bytes of JSON.
const MAX_KEY_FILE_BYTES: u32 = 64 * 1024;

/// The ceiling on a key's scrypt cost, `N·r·p`: 8 times rustic's own default
/// (`N = 2^17`, `r = 8`, `p = 1`) and 32 times restic's (`N = 2^15`). It also
/// caps scrypt's memory, `128·N·r` bytes, at 1 GiB.
const MAX_SCRYPT_COST: u64 = 1 << 23;

/// Whether deriving a key with these scrypt parameters stays inside
/// [`MAX_SCRYPT_COST`].
pub(crate) fn scrypt_cost_is_tolerable(n: u32, r: u32, p: u32) -> bool {
    u64::from(n)
        .checked_mul(u64::from(r))
        .and_then(|nr| nr.checked_mul(u64::from(p)))
        .is_some_and(|cost| cost <= MAX_SCRYPT_COST)
}

/// A size as rustic's listing wants it. Pack files stay far below 4 GiB (rustic
/// caps them), so the clamp is a guard, never a real path.
fn size_u32(size: u64) -> u32 {
    u32::try_from(size).unwrap_or(u32::MAX)
}

/// Wraps a parent failure so it survives rustic's error plumbing as the error's
/// typed source; [`volume_error_in`] finds it again.
fn backend_error(err: VolumeError) -> Box<RusticError> {
    RusticError::with_source(
        ErrorKind::Backend,
        "Reading the repository through its volume didn't work.",
        err,
    )
}

/// The parent's [`VolumeError`] somewhere in `err`'s source chain, if a parent
/// failure is what caused it. A typed downcast, never a look at message text.
pub(crate) fn volume_error_in<'a>(err: &'a (dyn std::error::Error + 'static)) -> Option<&'a VolumeError> {
    let mut current = Some(err);
    while let Some(err) = current {
        if let Some(volume_error) = err.downcast_ref::<VolumeError>() {
            return Some(volume_error);
        }
        current = err.source();
    }
    None
}

/// The one refusal every write gets: browsing must never change a user's backup.
fn refuse_write(be: &VolumeBackend) -> Box<RusticError> {
    be.write_attempts.fetch_add(1, Ordering::Relaxed);
    RusticError::new(ErrorKind::Unsupported, "Cmdr opens restic repositories read-only.")
}

impl ReadBackend for VolumeBackend {
    fn location(&self) -> String {
        self.root.display().to_string()
    }

    fn list_with_size(&self, tpe: FileType) -> RusticResult<Vec<(Id, u32)>> {
        match tpe {
            // rustic lists the config as one file with the default id.
            FileType::Config => match self
                .handle
                .block_on(self.parent.get_metadata(&self.path_of(tpe, &Id::default())))
            {
                Ok(entry) if !entry.is_directory => Ok(vec![(Id::default(), size_u32(entry.size.unwrap_or(0)))]),
                Ok(_) | Err(VolumeError::NotFound(_)) => Ok(Vec::new()),
                Err(err) => Err(backend_error(err)),
            },
            // `data/` holds up to 256 subfolders; list the ones that exist rather
            // than asking for all 256 names.
            FileType::Pack => {
                let data = self.root.join(tpe.dirname());
                let subdirs = match self.handle.block_on(self.parent.list_directory(&data, None)) {
                    Ok(entries) => entries,
                    Err(VolumeError::NotFound(_)) => return Ok(Vec::new()),
                    Err(err) => return Err(backend_error(err)),
                };
                let mut packs = Vec::new();
                for subdir in subdirs.into_iter().filter(|e| e.is_directory) {
                    packs.extend(self.list_ids_in(&data.join(&subdir.name))?);
                }
                Ok(packs)
            }
            FileType::Key => self.tolerable_keys(self.list_ids_in(&self.root.join(tpe.dirname()))?),
            FileType::Index | FileType::Snapshot => self.list_ids_in(&self.root.join(tpe.dirname())),
        }
    }

    fn read_full(&self, tpe: FileType, id: &Id) -> RusticResult<Bytes> {
        self.read_stream(&self.path_of(tpe, id), None)
            .map(Bytes::from)
            .map_err(backend_error)
    }

    fn read_partial(&self, tpe: FileType, id: &Id, _cacheable: bool, offset: u32, length: u32) -> RusticResult<Bytes> {
        let path = self.path_of(tpe, id);
        let (offset, length) = (u64::from(offset), length as usize);
        let window = match self.handle.block_on(self.parent.read_range(&path, offset, length)) {
            Ok(bytes) => bytes,
            // No positioned reads on this parent: stream from the start and cut
            // the window out. Correct, not cheap; every Cmdr backend that hosts
            // repos in practice has `read_range`.
            Err(VolumeError::NotSupported) => {
                let end = offset as usize + length;
                let whole = self.read_stream(&path, Some(end)).map_err(backend_error)?;
                whole
                    .get(offset as usize..end.min(whole.len()))
                    .unwrap_or_default()
                    .to_vec()
            }
            Err(err) => return Err(backend_error(err)),
        };
        if window.len() != length {
            return Err(RusticError::new(
                ErrorKind::InputOutput,
                "A repository file ended before the part rustic asked for.",
            ));
        }
        Ok(Bytes::from(window))
    }

    fn warmup_path(&self, _tpe: FileType, _id: &Id) -> String {
        // Cold-storage warm-up never applies: `needs_warm_up` stays false.
        String::new()
    }
}

impl WriteBackend for VolumeBackend {
    fn create(&self) -> RusticResult<()> {
        Err(refuse_write(self))
    }

    fn write_bytes(&self, _tpe: FileType, _id: &Id, _cacheable: bool, _content: BytesList) -> RusticResult<()> {
        Err(refuse_write(self))
    }

    fn remove(&self, _tpe: FileType, _id: &Id, _cacheable: bool) -> RusticResult<()> {
        Err(refuse_write(self))
    }
}

#[cfg(test)]
#[path = "backend_tests.rs"]
mod tests;
