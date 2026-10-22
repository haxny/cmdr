// The lint set this crate is held to lives in the workspace root's
// `[workspace.lints]`, opted into by `Cargo.toml`'s `lints.workspace = true`.
// `unused_crate_dependencies` can't go with them: it's judged per compilation
// unit, so it belongs on the lib alone. `missing_docs` is this crate's own
// contract: its API is what the app's Multi-Rename codes against.
#![warn(unused_crate_dependencies)]
#![deny(missing_docs)]

//! What a photo, video, or music file says about itself: EXIF, video capture
//! dates, and media tags, read through any volume.
//!
//! [`read_media`] parses one file over any `Read + Seek` and answers every field
//! at once ([`MediaFacts`]). [`BlockReader`] over a [`VolumeByteSource`] is that
//! reader for a file on a [`Volume`](cmdr_fs::volume::Volume): 64 KiB blocks on
//! demand, a byte budget per file, and holes for the cover art nobody needs. No
//! `tauri`, no app types, no wording: the app's Multi-Rename caches and words it.
//!
//! Module map and must-knows: `CLAUDE.md`; formats and costs: `DETAILS.md`.

mod ebml;
mod holes;
mod isobmff;
mod read;
mod source;
mod types;

#[cfg(test)]
mod types_test;

pub use read::read_media;
pub use source::{BlockReader, ByteSource, MAX_FETCHED_BYTES_PER_FILE, VolumeByteSource};
pub use types::{FieldKind, MediaFacts, MediaField, MediaSource, MediaValue, ReadError, is_media_name};
