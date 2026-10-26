#![warn(unused_crate_dependencies)]
#![deny(missing_docs)]

//! The tag editor's engine: read and write the text tags and front cover of
//! audio files (MP3, M4A/M4B, FLAC, Ogg Vorbis, Opus). No `tauri`, no app types,
//! no wording: the app words and drives it.
//!
//! Module map and must-knows: `CLAUDE.md`; field map and decisions: `DETAILS.md`.

mod format;
mod model;
mod read;
mod save;
pub mod shared;
pub mod text;
pub mod tools;
mod write;

// Dependencies the later tasks (read, write, tools, undo) use; declared now so the
// crate's manifest is final. Each `as _` goes when its first user lands.
use serde_json as _;

#[cfg(test)]
mod format_test;
#[cfg(test)]
mod read_test;
#[cfg(test)]
mod save_test;
#[cfg(test)]
mod shared_test;
#[cfg(test)]
mod test_files;
#[cfg(test)]
mod text_test;
#[cfg(test)]
mod tools_test;
#[cfg(test)]
mod write_extras_test;
#[cfg(test)]
mod write_test;

pub use format::{AudioFormat, format_of};
pub use model::{ContentKind, Cover, FileTags, TagError, TagField};
pub use read::read_tags;
pub use write::{WriteOptions, write_tags};
