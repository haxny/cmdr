//! Per-backend `Volume` implementations.
//!
//! Each submodule wraps a different storage system behind the `Volume` trait,
//! which lives in `cmdr_fs::volume` and is re-exported by [`super`]. Only one
//! backend still lives IN the app: `local_posix`. The others are crates of their
//! own (`cmdr-archive`, `cmdr-smb`, `cmdr-sftp`, `cmdr-webdav`, `cmdr-adb`,
//! `cmdr-mtp`) and every call site imports them by crate name.
//!
//! See [`super::CLAUDE.md`](../CLAUDE.md) for the trait shape and capability
//! matrix, and `backends/CLAUDE.md` for the per-backend decisions and gotchas
//! that drive each implementation here.

mod local_posix;

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use cmdr_mtp::MtpVolume;
pub use local_posix::LocalPosixVolume;
// The local backend's OS probes the rest of the app still reads: volume space
// (the index's tier-2 scan progress, the copy pre-flight, the space poller, the
// `get_volume_space` command), the NSURL resource readers and `statfs` strings
// `volumes/` enriches with, and the live-space memo a settled write retires.
#[cfg(target_os = "macos")]
pub use local_posix::live_space::{expect_space_change, live_volume_space};
#[cfg(target_os = "macos")]
pub(crate) use local_posix::nsurl::{get_bool_resource, get_string_resource};
pub(crate) use local_posix::rename_local_exclusive;
pub(crate) use local_posix::rename_volume_error;
pub(crate) use local_posix::space::get_space_info_for_path;
#[cfg(target_os = "macos")]
pub use local_posix::space::get_volume_space;
#[cfg(target_os = "macos")]
pub(crate) use local_posix::statfs::{get_mount_info, statfs_string};
// The mount probes the engine and `volumes*/` ask, beside the backend that
// serves those mounts: the filesystem behind a path (and whether it's a network
// one), the macOS mount table, and Linux's `/proc/mounts`.
pub use local_posix::filesystem_kind;
#[cfg(target_os = "linux")]
pub(crate) use local_posix::linux_mounts;
#[cfg(target_os = "macos")]
pub(crate) use local_posix::mount_table;
// The local reader: the `std::fs` listing and stat that `LocalPosixVolume`, the
// watcher's unregistered-volume fallback, `tags.rs`, and `create.rs` read through.
// `mutation.rs` beside it is what a local mutation means for the listing cache.
pub(crate) use local_posix::mutation::patch_listing_after_local_mutation;
#[cfg(test)]
pub(crate) use local_posix::reading::list_directory;
pub(crate) use local_posix::reading::{get_single_entry, list_directory_core};

#[cfg(test)]
mod local_posix_test;
// The cells that drive the app's real host (listing cache, collator, FSEvents
// watcher) rather than a detached or recording one.
#[cfg(test)]
mod local_posix_host_test;
// The shared `volume::conformance` assertions LocalPosix runs, split out the way
// every other backend keeps its own (SMB's, SFTP's, and MTP's
// `volume::conformance_test`).
#[cfg(test)]
mod local_posix_conformance_test;
