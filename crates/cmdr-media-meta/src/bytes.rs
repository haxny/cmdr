//! Positioned reads for the container walkers: a short file is "nothing there",
//! never an error, while a refusal (`Cancelled`, `TooLarge`, …) still travels up.

use std::io::{self, ErrorKind, Read, Seek, SeekFrom};

/// The most bytes a walker reads into memory for one value (a tag string, an
/// EXIF block). A size field claiming more is skipped, never allocated.
pub(crate) const MAX_VALUE_BYTES: u64 = 1024 * 1024;

/// The reader's length, from a seek to its end.
pub(crate) fn file_len<R: Seek>(reader: &mut R) -> io::Result<u64> {
    reader.seek(SeekFrom::End(0))
}

/// Fills `buf` from `offset`. `Ok(false)` when the file ends first.
pub(crate) fn read_exact_at<R: Read + Seek>(reader: &mut R, offset: u64, buf: &mut [u8]) -> io::Result<bool> {
    reader.seek(SeekFrom::Start(offset))?;
    match reader.read_exact(buf) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == ErrorKind::UnexpectedEof => Ok(false),
        Err(error) => Err(error),
    }
}

/// `len` bytes from `offset`, or `None` when the file ends first or `len` is past
/// [`MAX_VALUE_BYTES`]. Allocates only what the file actually holds.
pub(crate) fn read_vec_at<R: Read + Seek>(reader: &mut R, offset: u64, len: u64) -> io::Result<Option<Vec<u8>>> {
    if len > MAX_VALUE_BYTES {
        return Ok(None);
    }
    reader.seek(SeekFrom::Start(offset))?;
    let mut buf = Vec::new();
    reader.by_ref().take(len).read_to_end(&mut buf)?;
    Ok((buf.len() as u64 == len).then_some(buf))
}

/// Up to `buf.len()` bytes from `offset`; how many there were.
pub(crate) fn read_up_to_at<R: Read + Seek>(reader: &mut R, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
    reader.seek(SeekFrom::Start(offset))?;
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(filled)
}

/// A big-endian `u16` from `bytes` at `at`.
pub(crate) fn be_u16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

/// A big-endian `u32` from `bytes` at `at`.
pub(crate) fn be_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// A big-endian `u64` from `bytes` at `at`.
pub(crate) fn be_u64(bytes: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
}

/// A little-endian `u32` from `bytes` at `at`.
pub(crate) fn le_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}
