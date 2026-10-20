// The lint set this crate is held to lives in the workspace root's
// `[workspace.lints]`, opted into by `Cargo.toml`'s `lints.workspace = true`.
// These two can't go with them: `unused_crate_dependencies` is judged per
// compilation unit, and `missing_docs` is this crate's own contract.
#![warn(unused_crate_dependencies)]
#![deny(missing_docs)]

//! The restic backend: reads a restic backup repository, wherever it lives, as a
//! read-only volume of snapshots.
//!
//! A storage backend as its own crate, built on `rustic_core`. The repository is
//! read THROUGH the volume it lives on (a local disk, an SMB share, SFTP, S3, …),
//! so anywhere Cmdr can browse, a repo opens. It never writes to the repo, never
//! names the application around it, and never produces a word a human reads.
//!
//! - [`is_restic_repo`]: is this folder a repo? From its listing alone.
//! - [`restic_route`] and [`VIRTUAL_ENTRY_NAME`]: which repo a path browses
//!   into, lexically.
//! - [`ResticRepos`]: the hub every view shares (passwords, opened repos,
//!   indexes, and which folders are repos), and the overlay row.
//! - [`ResticVolume`]: the read-only view of one repo's snapshots, `latest`
//!   included.
//!
//! See `CLAUDE.md` for the must-knows and `DETAILS.md` for the design.

//noinspection RsUnusedImport
// We dev-depend on ourselves so the `testing` feature is on for dev targets and
// off for the lib (see `Cargo.toml`). That makes `cmdr_restic` an extern crate of
// its own test target, which `unused_crate_dependencies` reports.
#[cfg(test)]
use cmdr_restic as _;
// The two parents `examples/open_cost.rs` measures through are dev-dependencies,
// so every test target links them, and this one doesn't use them.
#[cfg(test)]
use {cmdr_local as _, cmdr_smb as _};

mod backend;
mod detect;
mod hub;
mod path;
mod read_stream;
mod repo;
mod snapshots;
#[cfg(any(test, feature = "testing"))]
pub mod test_fixtures;
mod tree;
mod volume;

pub use detect::is_restic_repo;
pub use hub::ResticRepos;
pub use path::{VIRTUAL_ENTRY_NAME, restic_route};
pub use volume::ResticVolume;
