//! What a streamed write stamps on what it wrote beyond the bytes (the dates
//! and Hidden, in one `FILE_BASIC_INFORMATION`), and what it reports of that.

use cmdr_fs::metadata::{CarriedMetadata, ClassOutcome, MetadataReport, UserFlags};
use cmdr_fs::volume::StreamWritten;
use log::warn;

use super::SmbVolume;
use super::mapping::attributes_of;

/// What a copy stamps on what it wrote, in one `FILE_BASIC_INFORMATION`: the
/// source's `LastWriteTime` and `CreationTime`, and `HIDDEN` when the source is
/// Hidden, so neither costs a frame of its own. The server keeps its own access
/// time. `None` when the source has none of them, which leaves the server's.
///
/// Only Hidden rides here ([`SmbVolume::METADATA_CAPS`]'s `staged_flags`):
/// this lands on the staged temp, and Read-only waits for the landing. Sending
/// attributes REPLACES the settable set, so a Hidden copy doesn't wear the
/// `ARCHIVE` a fresh file gets, which nothing on a Mac reads.
pub(super) fn source_times(
    modified: Option<std::time::SystemTime>,
    created: Option<std::time::SystemTime>,
    flags: UserFlags,
) -> Option<smb2::FileTimes> {
    let staged = flags.intersection(SmbVolume::METADATA_CAPS.staged_flags);
    if modified.is_none() && created.is_none() && staged.is_empty() {
        return None;
    }
    let mut times = smb2::FileTimes::new();
    if let Some(modified) = modified {
        times = times.set_modified(modified);
    }
    if let Some(created) = created {
        times = times.set_created(created);
    }
    if !staged.is_empty() {
        times = times.set_attributes(attributes_of(staged));
    }
    Some(times)
}

/// Stamps the source's date on a streaming writer's OWN handle, before
/// `finish()`: the server rewrites the date when a writing handle closes, so a
/// path stamp while this one is open would lose to that close. Best effort: a
/// date that won't set leaves the server's, and the copy goes on. Returns
/// whether the server took them.
pub(super) async fn stamp_writer(
    writer: &mut smb2::client::stream::FileWriter,
    times: smb2::FileTimes,
    smb_path: &str,
) -> bool {
    match writer.set_times(times).await {
        Ok(()) => true,
        Err(e) => {
            warn!("SmbVolume::write_from_stream: path={smb_path:?} keeps the server's date, not the source's: {e}");
            false
        }
    }
}

/// What a write of `bytes` reports: the creation date and the staged flags
/// rode `times`, so the server took both or refused both.
pub(super) fn stamped(
    bytes: u64,
    carried: &CarriedMetadata,
    modified: Option<std::time::SystemTime>,
    refused: bool,
) -> StreamWritten {
    let stamped = CarriedMetadata {
        created: carried.created,
        flags: carried.flags.intersection(SmbVolume::METADATA_CAPS.staged_flags),
        ..CarriedMetadata::default()
    };
    let outcome = if refused {
        ClassOutcome::Refused
    } else {
        ClassOutcome::Kept
    };
    let report = MetadataReport::uniform(&stamped, modified, outcome);
    StreamWritten::with_report(bytes, &report, &stamped, modified)
}
