#![warn(unused_crate_dependencies)]
#![deny(missing_docs)]

//! The tag editor's engine: read and write the text tags and front cover of
//! audio files (MP3, M4A/M4B, FLAC, Ogg Vorbis, Opus). No `tauri`, no app types,
//! no wording: the app words and drives it.
//!
//! Module map and must-knows: `CLAUDE.md`; field map and decisions: `DETAILS.md`.

mod format;
mod model;
pub mod text;

// Dependencies the later tasks (read, write, tools, undo) use; declared now so the
// crate's manifest is final. Each `as _` goes when its first user lands.
use lofty as _;
use regex as _;
use serde_json as _;
// Test-only: real tagged files and temp dirs for the read/write tests.
#[cfg(test)]
use cmdr_media_meta as _;
#[cfg(test)]
use tempfile as _;

#[cfg(test)]
mod format_test;
#[cfg(test)]
mod text_test;

pub use format::{AudioFormat, format_of};
pub use model::{ContentKind, Cover, FileTags, TagError, TagField};
