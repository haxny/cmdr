// The lint set this crate is held to lives in the workspace root's
// `[workspace.lints]`, opted into by `Cargo.toml`'s `lints.workspace = true`.
// These two can't go with them: `unused_crate_dependencies` is judged per
// compilation unit, and `missing_docs` is this crate's own contract.
#![warn(unused_crate_dependencies)]
#![deny(missing_docs)]

//! The restic backend: reads a restic backup repository, wherever it lives, as a
//! read-only source of snapshots.
//!
//! A storage backend as its own crate, built on `rustic_core`. The repository is
//! read THROUGH the volume it lives on (a local disk, an SMB share, SFTP, S3, …),
//! so anywhere Cmdr can browse, a repo opens. It never writes to the repo, never
//! names the application around it, and never produces a word a human reads.
//!
//! See `CLAUDE.md` for the must-knows and `DETAILS.md` for the design.

#[allow(dead_code, reason = "TEMP: the repo layer that uses it lands next")]
mod backend;
mod detect;

pub use detect::is_restic_repo;
