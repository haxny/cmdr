//! Making a written file's bytes durable before a copy reports it done.

use std::fs::File;

/// `sync_data`, which on macOS is `F_FULLFSYNC`, falling back to `fsync(2)`
/// where the filesystem doesn't offer the full barrier: an SMB mount of a Samba
/// share answers `ENOTSUP` (verified against the Docker guest share, macOS 27.0,
/// 2026-10-11), which used to fail every chunked copy onto it. SQLite falls back
/// the same way.
///
/// ❗ On a network mount this is also what makes a copy's dates stick: writes
/// still buffered when the dates go on reach the server at the handle's real
/// close, and the server stamps that close as the mtime. So a copy flushes
/// first and sets the dates after (`metadata::apply_staged`).
pub fn sync_file_data(file: &File) -> std::io::Result<()> {
    match file.sync_data() {
        #[cfg(target_os = "macos")]
        Err(e) if e.raw_os_error() == Some(libc::ENOTSUP) => {
            use std::os::unix::io::AsRawFd as _;
            // SAFETY: `file` is open for the whole call, so its fd is valid;
            // `fsync` touches nothing but that fd.
            if unsafe { libc::fsync(file.as_raw_fd()) } == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        }
        other => other,
    }
}
