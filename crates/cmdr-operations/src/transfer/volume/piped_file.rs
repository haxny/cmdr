//! What one file's write reports once it landed at its final name: its bytes
//! and the landed modified time the write promised for that name. A leaf both
//! `stream_pipe.rs` and `server_side_copy.rs` speak, so neither imports the
//! other for it; `sequential_extract.rs` asks [`landed_at_final_name`] too.

use std::path::Path;

use cmdr_fs::volume::{LandedModified, StreamWritten};

/// A file `stream_pipe_file` (or a server-side copy) landed at its final name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PipedFile {
    /// The bytes it holds.
    pub(super) bytes: u64,
    /// Its modified time as `get_metadata` answers it, when the write that
    /// landed it reported one for the final name ([`landed_at_final_name`]).
    /// A landing settled after the fact (a lost reply, a cut-off rename) is
    /// `Unknown`: the operation log reads those back.
    pub(super) landed: LandedModified,
}

impl PipedFile {
    /// `bytes` landed, with no landed time reported.
    pub(super) const fn unreported(bytes: u64) -> Self {
        Self {
            bytes,
            landed: LandedModified::Unknown,
        }
    }
}

/// The landed time `written` reported, for the file at `final_name`: it holds
/// only when the write went to that name. A staged write's time is its temp's,
/// and the landing rename after it isn't promised to keep it.
pub(super) fn landed_at_final_name(written: &StreamWritten, wrote_to: &Path, final_name: &Path) -> LandedModified {
    if wrote_to == final_name {
        written.landed_modified()
    } else {
        LandedModified::Unknown
    }
}
