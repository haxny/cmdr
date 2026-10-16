//! Tauri commands for file system operations.

mod archive;
mod drag;
#[cfg(any(feature = "playwright-e2e", debug_assertions))]
mod e2e_support;
mod git;
mod listing;
mod path_checks;
mod stat;
mod stored_spelling;
mod volume_copy;
mod write_ops;

pub use archive::*;
pub use drag::*;
#[cfg(any(feature = "playwright-e2e", debug_assertions))]
pub use e2e_support::*;
pub use git::*;
pub use listing::*;
pub use path_checks::*;
pub use stat::*;
pub use stored_spelling::*;
pub use volume_copy::*;
pub use write_ops::*;

/// Expands a leading `~` to the user's home directory: the IPC edge's spelling of
/// `cmdr_fs::tilde::expand_tilde`, gated per command on the path being local.
pub(crate) use cmdr_fs::tilde::expand_tilde;
