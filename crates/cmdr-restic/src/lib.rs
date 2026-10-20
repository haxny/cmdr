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
//! - [`is_restic_repo`]: is this folder a repo? From its listing alone.
//! - [`ResticRepo`]: open one with its password, then list its snapshots as a
//!   [`SnapshotList`] of named [`Snapshot`]s, `latest` included.
//!
//! See `CLAUDE.md` for the must-knows and `DETAILS.md` for the design.

//noinspection RsUnusedImport
// We dev-depend on ourselves so the `testing` feature is on for dev targets and
// off for the lib (see `Cargo.toml`). That makes `cmdr_restic` an extern crate of
// its own test target, which `unused_crate_dependencies` reports.
#[cfg(test)]
use cmdr_restic as _;

mod backend;
mod detect;
mod repo;
mod snapshots;
#[cfg(any(test, feature = "testing"))]
pub mod test_fixtures;

pub use detect::is_restic_repo;
pub use repo::{ResticError, ResticRepo};
pub use snapshots::{LATEST_FOLDER_NAME, Snapshot, SnapshotList};
