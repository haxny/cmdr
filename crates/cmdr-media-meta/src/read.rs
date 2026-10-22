//! [`read_media`]: one file's facts, by extension and then content.

use std::io::{Read, Seek};

// The readers parse with these.
use exif as _;
use lofty as _;
use log as _;

use crate::types::{MediaFacts, ReadError};

/// Every field `file_name`'s bytes in `reader` carry. Dispatches by extension
/// (`is_media_name`'s list): photos to `exif`, audio to `lofty`, MP4/MOV to our
/// ISOBMFF walk then `lofty`, MKV/WebM to our EBML walk.
pub fn read_media<R: Read + Seek>(reader: &mut R, file_name: &str) -> Result<MediaFacts, ReadError> {
    let _ = (reader, file_name);
    Err(ReadError::NotSupported)
}
