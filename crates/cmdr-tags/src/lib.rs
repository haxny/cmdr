#![warn(unused_crate_dependencies)]
#![deny(missing_docs)]

//! The tag editor's engine: read and write the text tags and front cover of
//! audio files (MP3, M4A/M4B, FLAC, Ogg Vorbis, Opus). No `tauri`, no app types,
//! no wording: the app words and drives it.
//!
//! Module map and must-knows: `CLAUDE.md`; field map and decisions: `DETAILS.md`.

mod fingerprint;
mod format;
mod model;
mod name_pattern;
mod normalize;
mod numbering;
mod read;
mod save;
pub mod shared;
pub mod text;
pub mod tools;
mod totals;
mod undo;
mod write;
mod write_extras;

#[cfg(test)]
mod format_test;
#[cfg(test)]
mod name_pattern_test;
#[cfg(test)]
mod normalize_test;
#[cfg(test)]
mod numbering_test;
#[cfg(test)]
mod read_test;
#[cfg(test)]
mod save_test;
#[cfg(test)]
mod serde_test;
#[cfg(test)]
mod shared_test;
#[cfg(test)]
mod test_files;
#[cfg(test)]
mod text_test;
#[cfg(test)]
mod tools_test;
#[cfg(test)]
mod undo_seal_test;
#[cfg(test)]
mod undo_test;
#[cfg(test)]
mod write_cover_test;
#[cfg(test)]
mod write_date_test;
#[cfg(test)]
mod write_extras_test;
#[cfg(test)]
mod write_hidden_test;
#[cfg(test)]
mod write_m4b_test;
#[cfg(test)]
mod write_test;
#[cfg(test)]
mod write_totals_test;

pub use fingerprint::Fingerprint;
pub use format::{AudioFormat, format_of};
pub use model::{ContentKind, Cover, FileTags, KindRemoval, TagError, TagField};
pub use name_pattern::{CompiledPattern, NamePattern, PatternError, compile_pattern};
pub use normalize::normalize_for;
pub use numbering::{DiscOptions, NumberOptions};
pub use read::{read_duration, read_tags, removed_by_kind};
pub use undo::{ApplyId, RestoreReport, UndoLimits, UndoStore};
pub use write::{WriteOptions, date_is_writable, write_tags};
