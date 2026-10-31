//! The LZ4 frame format, as `RCV2` and `SND2` carry it.
//!
//! With `kSyncFlagLZ4` set, the concatenated `DATA` payloads of one transfer are
//! ONE LZ4 frame, cut into packets at arbitrary byte boundaries: a packet can end
//! mid-block or mid-header. adbd writes the frame through `LZ4F_compressBegin` with
//! default preferences, so linked blocks of at most 64 KiB and no checksums
//! (`compression_utils.h`, AOSP `packages/modules/adb` main, 2026-10-30). The
//! `lz4` 1.10.0 CLI writes the same shape with `-B4 -BD --no-frame-crc`, which is
//! where `testing/liblz4_frame.bin` comes from.
//!
//! The reader is ours for two reasons: the socket is async while
//! `lz4_flex::frame::FrameDecoder` is a blocking `Read` that can't resume a partial
//! block, and that decoder reads a frame cut off at a block boundary as a clean end,
//! which would land a short file as a good one.
//!
//! ❗ The device chooses every byte here. The block size word is capped by the
//! frame's own declared maximum, itself at most the format's 4 MiB, before any of
//! the block is buffered; a block can't decode past that buffer; and decoded bytes
//! leave in pieces of at most `MAX_DATA_CHUNK`, one per call, so a compression bomb
//! (255:1 at best) reaches the consumer at the rate the consumer asks for.

use std::hash::Hasher;
use std::io::Write;

use lz4_flex::block::{decompress_into, decompress_into_with_dict};
use lz4_flex::frame::{BlockMode, BlockSize, FrameEncoder, FrameInfo};
use twox_hash::XxHash32;

use crate::errors::AdbError;
use crate::sync::MAX_DATA_CHUNK;

/// The frame format's magic number, as it appears on the wire.
const MAGIC: [u8; 4] = 0x184D_2204u32.to_le_bytes();

/// Magic + `FLG` + `BD` + the header checksum: the shortest possible header.
const MIN_HEADER: usize = 7;

/// How far back a linked block may reach into the blocks before it.
const WINDOW: usize = 64 * 1024;

/// `FLG` bits. The version occupies the top two bits and must read `01`.
const FLG_VERSION_MASK: u8 = 0b1100_0000;
const FLG_VERSION_01: u8 = 0b0100_0000;
const FLG_INDEPENDENT_BLOCKS: u8 = 0b0010_0000;
const FLG_BLOCK_CHECKSUM: u8 = 0b0001_0000;
const FLG_CONTENT_SIZE: u8 = 0b0000_1000;
const FLG_CONTENT_CHECKSUM: u8 = 0b0000_0100;
const FLG_RESERVED: u8 = 0b0000_0010;
const FLG_DICT_ID: u8 = 0b0000_0001;

/// The high bit of a block's size word: the block is stored, not compressed.
const BLOCK_STORED: u32 = 0x8000_0000;

fn bad_frame(what: &str) -> AdbError {
    AdbError::Protocol(format!("LZ4 frame: {what}"))
}

/// The little-endian `u32` at the start of `bytes`. Callers have checked the length.
fn le_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn xxh32(bytes: &[u8]) -> u32 {
    let mut hasher = XxHash32::with_seed(0);
    hasher.write(bytes);
    hasher.finish() as u32
}

/// Where in the frame the reader stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Header,
    Blocks,
    ContentChecksum,
    Finished,
}

/// What a frame's header said.
#[derive(Debug, Clone, Copy, Default)]
struct Layout {
    max_block: usize,
    linked: bool,
    block_checksum: bool,
    content_checksum: bool,
    content_size: Option<u64>,
}

/// Decodes one frame, fed in `DATA` packets.
///
/// Call [`Lz4Reader::feed`] with a packet, then [`Lz4Reader::next_chunk`] until it
/// answers `None`, and [`Lz4Reader::finish`] when the transfer's `DONE` arrives.
pub(crate) struct Lz4Reader {
    /// Bytes fed and not yet decoded. Holds at most one block plus one packet: a
    /// feed only follows a `next_chunk` that wanted more, and the size word bounds
    /// what that block can ask for.
    input: Vec<u8>,
    consumed: usize,
    stage: Stage,
    layout: Layout,
    /// The current block's decoded bytes: `layout.max_block` long once the header
    /// has been read, filled up to `decoded_len`, handed out up to `handed_out`.
    decoded: Vec<u8>,
    decoded_len: usize,
    handed_out: usize,
    /// The last [`WINDOW`] decoded bytes, for a linked block to reach back into.
    window: Vec<u8>,
    content_hash: XxHash32,
    content_len: u64,
}

impl Lz4Reader {
    pub(crate) fn new() -> Self {
        Self {
            input: Vec::new(),
            consumed: 0,
            stage: Stage::Header,
            layout: Layout::default(),
            decoded: Vec::new(),
            decoded_len: 0,
            handed_out: 0,
            window: Vec::new(),
            content_hash: XxHash32::with_seed(0),
            content_len: 0,
        }
    }

    /// Takes one `DATA` payload. Call only after [`Lz4Reader::next_chunk`] has
    /// answered `None`.
    pub(crate) fn feed(&mut self, packet: &[u8]) {
        self.input.drain(..self.consumed);
        self.consumed = 0;
        self.input.extend_from_slice(packet);
    }

    /// The next decoded bytes, at most `MAX_DATA_CHUNK` of them. `None` means the
    /// reader wants another packet, or the frame is complete.
    pub(crate) fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, AdbError> {
        loop {
            if self.handed_out < self.decoded_len {
                let end = self.decoded_len.min(self.handed_out + MAX_DATA_CHUNK);
                let chunk = self.decoded[self.handed_out..end].to_vec();
                self.handed_out = end;
                return Ok(Some(chunk));
            }
            let advanced = match self.stage {
                Stage::Header => self.read_header()?,
                Stage::Blocks => self.read_block()?,
                Stage::ContentChecksum => self.read_content_checksum()?,
                Stage::Finished => {
                    // adbd's own decoder refuses this too: "LZ4 stream hit end
                    // before reading all data". Nothing writes a second frame.
                    if self.consumed < self.input.len() {
                        return Err(bad_frame("bytes after the end of the frame"));
                    }
                    false
                }
            };
            if !advanced {
                return Ok(None);
            }
        }
    }

    /// Whether the frame ended properly. A `DONE` that arrives before the frame's
    /// end mark is a truncated file, never a short one: the size check downstream
    /// can't catch this on a stream of unknown length.
    pub(crate) fn finish(&self) -> Result<(), AdbError> {
        if self.stage == Stage::Finished && self.consumed == self.input.len() {
            Ok(())
        } else {
            Err(bad_frame("the transfer ended before the frame did"))
        }
    }

    fn read_header(&mut self) -> Result<bool, AdbError> {
        let available = &self.input[self.consumed..];
        if available.len() < MIN_HEADER {
            return Ok(false);
        }
        if available[..4] != MAGIC {
            return Err(bad_frame("not an LZ4 frame"));
        }
        let flg = available[4];
        let bd = available[5];
        if flg & FLG_VERSION_MASK != FLG_VERSION_01 || flg & FLG_RESERVED != 0 {
            return Err(bad_frame("unknown frame version"));
        }
        if flg & FLG_DICT_ID != 0 {
            return Err(bad_frame("the frame asks for a dictionary"));
        }
        let max_block = match bd {
            0x40 => 64 * 1024,
            0x50 => 256 * 1024,
            0x60 => 1024 * 1024,
            0x70 => 4 * 1024 * 1024,
            _ => return Err(bad_frame("unknown block size")),
        };
        let descriptor_end = if flg & FLG_CONTENT_SIZE != 0 { 14 } else { 6 };
        let header_len = descriptor_end + 1;
        if available.len() < header_len {
            return Ok(false);
        }
        if (xxh32(&available[4..descriptor_end]) >> 8) as u8 != available[descriptor_end] {
            return Err(bad_frame("header checksum mismatch"));
        }
        let content_size = (flg & FLG_CONTENT_SIZE != 0).then(|| {
            let mut size = [0u8; 8];
            size.copy_from_slice(&available[6..14]);
            u64::from_le_bytes(size)
        });
        self.layout = Layout {
            max_block,
            linked: flg & FLG_INDEPENDENT_BLOCKS == 0,
            block_checksum: flg & FLG_BLOCK_CHECKSUM != 0,
            content_checksum: flg & FLG_CONTENT_CHECKSUM != 0,
            content_size,
        };
        self.consumed += header_len;
        self.decoded = vec![0u8; max_block];
        self.stage = Stage::Blocks;
        Ok(true)
    }

    fn read_block(&mut self) -> Result<bool, AdbError> {
        let available = &self.input[self.consumed..];
        if available.len() < 4 {
            return Ok(false);
        }
        let word = le_u32(available);
        if word == 0 {
            self.consumed += 4;
            if self.layout.content_size.is_some_and(|size| size != self.content_len) {
                return Err(bad_frame("content size mismatch"));
            }
            self.stage = if self.layout.content_checksum {
                Stage::ContentChecksum
            } else {
                Stage::Finished
            };
            return Ok(true);
        }
        let stored = word & BLOCK_STORED != 0;
        let size = (word & !BLOCK_STORED) as usize;
        // ❗ Refused off the size word alone, before waiting for (or buffering) a
        // byte of the block.
        if size > self.layout.max_block {
            return Err(bad_frame("block larger than the frame's maximum"));
        }
        let checksum_len = if self.layout.block_checksum { 4 } else { 0 };
        if available.len() < 4 + size + checksum_len {
            return Ok(false);
        }
        let block = &available[4..4 + size];
        if self.layout.block_checksum && xxh32(block) != le_u32(&available[4 + size..]) {
            return Err(bad_frame("block checksum mismatch"));
        }
        // `decoded` is `max_block` long, so a block that would decode past it fails
        // inside the codec, with nothing allocated for the excess.
        let decoded = if stored {
            self.decoded[..size].copy_from_slice(block);
            size
        } else if self.layout.linked {
            decompress_into_with_dict(block, &mut self.decoded, &self.window).map_err(|_| bad_frame("corrupt block"))?
        } else {
            decompress_into(block, &mut self.decoded).map_err(|_| bad_frame("corrupt block"))?
        };
        self.consumed += 4 + size + checksum_len;
        self.decoded_len = decoded;
        self.handed_out = 0;
        self.content_len += decoded as u64;
        let bytes = &self.decoded[..decoded];
        if self.layout.content_checksum {
            self.content_hash.write(bytes);
        }
        if self.layout.linked {
            if bytes.len() >= WINDOW {
                self.window.clear();
                self.window.extend_from_slice(&bytes[bytes.len() - WINDOW..]);
            } else {
                let keep = WINDOW - bytes.len();
                if self.window.len() > keep {
                    self.window.drain(..self.window.len() - keep);
                }
                self.window.extend_from_slice(bytes);
            }
        }
        Ok(true)
    }

    fn read_content_checksum(&mut self) -> Result<bool, AdbError> {
        let available = &self.input[self.consumed..];
        if available.len() < 4 {
            return Ok(false);
        }
        if self.content_hash.finish() as u32 != le_u32(available) {
            return Err(bad_frame("content checksum mismatch"));
        }
        self.consumed += 4;
        self.stage = Stage::Finished;
        Ok(true)
    }
}

/// Encodes one frame the way adbd's own client does: linked blocks of 64 KiB, no
/// checksums.
pub(crate) struct Lz4Writer {
    encoder: FrameEncoder<Vec<u8>>,
}

impl Lz4Writer {
    pub(crate) fn new() -> Self {
        let info = FrameInfo::new()
            .block_size(BlockSize::Max64KB)
            .block_mode(BlockMode::Linked);
        Self {
            encoder: FrameEncoder::with_frame_info(info, Vec::new()),
        }
    }

    /// Compresses `input` and answers the output that fills whole `DATA` packets;
    /// the rest waits for more input or [`Lz4Writer::finish`]. adbd's encoder holds
    /// back the same way: compressible data would otherwise cost one packet per
    /// small piece.
    pub(crate) fn write(&mut self, input: &[u8]) -> Result<Vec<u8>, AdbError> {
        self.encoder.write_all(input)?;
        let pending = self.encoder.get_mut();
        let full = pending.len() - pending.len() % MAX_DATA_CHUNK;
        if full == 0 {
            return Ok(Vec::new());
        }
        let rest = pending.split_off(full);
        Ok(std::mem::replace(pending, rest))
    }

    /// Ends the frame and answers everything still unsent, never empty: an empty
    /// file is a header and an end mark.
    ///
    /// ❗ That frame must go out even for zero bytes. adbd's decoder only reports
    /// `Done` once it has seen the end mark, so a `DONE` without one spins its
    /// decode loop forever (`LZ4Decoder::Decode` answers `MoreOutput` every time).
    pub(crate) fn finish(self) -> Result<Vec<u8>, AdbError> {
        self.encoder.finish().map_err(|e| bad_frame(&e.to_string()))
    }
}

#[cfg(test)]
#[path = "lz4_test.rs"]
mod lz4_test;
