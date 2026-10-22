//! The byte ranges a tag parser would read and throw away, mapped before it runs
//! so [`BlockReader`](crate::BlockReader) serves them as zeros without fetching
//! them: ID3v2 `APIC` frame bodies and MP4 `covr` pictures.
//!
//! `lofty` with `read_cover_art(false)` still reads both: an `APIC` frame is
//! skipped by copying it into a sink, and an `ilst` is read whole before its
//! atoms are parsed (verified in lofty 0.25.4, `id3/v2/frame/read.rs` and
//! `mp4/ilst/read.rs`, 2026-10-22). FLAC `PICTURE` blocks need no hole: lofty
//! seeks past them (`flac/block.rs`).

use std::io::{self, Read, Seek};
use std::ops::Range;

use crate::bytes::{be_u32, file_len, read_exact_at};
use crate::isobmff::{children, find_moov, meta_children_start};

/// The most ID3v2 frames the pre-pass walks: a real tag has dozens.
const MAX_FRAMES: usize = 4096;

/// The cover art in `reader`, as byte ranges nobody needs to fetch. Anything the
/// pre-pass can't map with certainty stays unmapped (lofty then reads it, within
/// the byte budget): ID3v2.2, a tag-wide unsynchronisation, or an extended
/// header, whose frame offsets lofty and this walk could disagree on.
pub(crate) fn cover_art<R: Read + Seek>(reader: &mut R, file_name: &str) -> io::Result<Vec<Range<u64>>> {
    let mut magic = [0u8; 3];
    if read_exact_at(reader, 0, &mut magic)? && &magic == b"ID3" {
        return id3v2_pictures(reader);
    }
    if is_isobmff_name(file_name) {
        return mp4_pictures(reader);
    }
    Ok(Vec::new())
}

/// Whether `file_name` is an MP4-family file `lofty` reads an `ilst` from.
fn is_isobmff_name(file_name: &str) -> bool {
    let extension = file_name.rsplit_once('.').map_or("", |(_, ext)| ext);
    ["mp4", "m4a", "m4b", "m4v", "mov", "3gp"]
        .iter()
        .any(|known| known.eq_ignore_ascii_case(extension))
}

/// `APIC` frame bodies in an ID3v2.3 or 2.4 tag at the start of the file. Frame
/// sizes are plain in 2.3 and syncsafe in 2.4, read exactly as lofty reads them.
fn id3v2_pictures<R: Read + Seek>(reader: &mut R) -> io::Result<Vec<Range<u64>>> {
    let mut header = [0u8; 10];
    if !read_exact_at(reader, 0, &mut header)? {
        return Ok(Vec::new());
    }
    let version = header[3];
    let flags = header[5];
    let unsynchronised = flags & 0x80 != 0;
    let extended_header = flags & 0x40 != 0;
    if !matches!(version, 3 | 4) || unsynchronised || extended_header {
        return Ok(Vec::new());
    }
    let Some(tag_size) = syncsafe(&header[6..10]) else {
        return Ok(Vec::new());
    };
    let tag_end = 10 + u64::from(tag_size);
    let mut holes = Vec::new();
    let mut pos = 10;
    for _ in 0..MAX_FRAMES {
        let mut frame = [0u8; 10];
        if pos + 10 > tag_end || !read_exact_at(reader, pos, &mut frame)? || frame[0] == 0 {
            break;
        }
        let raw_size = be_u32(&frame, 4).unwrap_or(0);
        let size = if version == 4 {
            syncsafe(&frame[4..8])
        } else {
            Some(raw_size)
        };
        let Some(size) = size else {
            break;
        };
        let body = pos + 10;
        let end = body + u64::from(size);
        if &frame[..4] == b"APIC" && size > 0 {
            holes.push(body..end.min(tag_end));
        }
        pos = end;
    }
    Ok(holes)
}

/// A 28-bit syncsafe integer (7 bits per byte), or `None` when a byte has its
/// top bit set.
fn syncsafe(bytes: &[u8]) -> Option<u32> {
    bytes
        .iter()
        .try_fold(0u32, |acc, b| (b & 0x80 == 0).then(|| (acc << 7) | u32::from(*b)))
}

/// The pictures in `moov/udta/meta/ilst/covr`: each `data` atom's payload past
/// its type and locale.
fn mp4_pictures<R: Read + Seek>(reader: &mut R) -> io::Result<Vec<Range<u64>>> {
    let mut holes = Vec::new();
    let end = file_len(reader)?;
    let Some(moov) = find_moov(reader)? else {
        return Ok(holes);
    };
    for udta in children(reader, moov.body, moov.end.min(end))?
        .into_iter()
        .filter(|a| &a.kind == b"udta")
    {
        for meta in children(reader, udta.body, udta.end)?
            .into_iter()
            .filter(|a| &a.kind == b"meta")
        {
            let start = meta_children_start(reader, meta)?;
            for ilst in children(reader, start, meta.end)?
                .into_iter()
                .filter(|a| &a.kind == b"ilst")
            {
                for covr in children(reader, ilst.body, ilst.end)?
                    .into_iter()
                    .filter(|a| &a.kind == b"covr")
                {
                    for data in children(reader, covr.body, covr.end)?
                        .into_iter()
                        .filter(|a| &a.kind == b"data")
                    {
                        let payload = data.body + 8;
                        if payload < data.end {
                            holes.push(payload..data.end);
                        }
                    }
                }
            }
        }
    }
    Ok(holes)
}
