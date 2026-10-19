//! The one-frame write's condition and limit, and the single-shot write promise
//! they back. End-to-end writes are `write_stream_integration_test.rs`; the
//! frames they cost are `wire_shape_integration_test.rs`.

use super::*;
use crate::volume::test_support::*;

/// The compound fast path's condition, which is also the transfer layer's
/// staging exemption. A write is one all-or-nothing frame when it has bytes and
/// they fit one SMB2 WRITE; the boundary is inclusive, and an empty file is NOT
/// single-shot (it has no WRITE to compound with, so it takes the streaming
/// writer's create+finish).
#[test]
fn only_a_write_that_fits_one_compound_frame_is_single_shot() {
    assert!(!fits_one_compound_write(65_536, 0), "an empty file has no WRITE");
    assert!(fits_one_compound_write(65_536, 1));
    assert!(fits_one_compound_write(65_536, 65_536), "the limit itself fits");
    assert!(
        !fits_one_compound_write(65_536, 65_537),
        "one byte over needs two WRITEs"
    );
    assert!(
        fits_one_compound_write(8 * 1024 * 1024, 65_537),
        "a server that negotiated a bigger WRITE takes bigger files in one shot"
    );
}

/// The limit a write goes out as one frame up to must cover every size a
/// single-shot promise could have covered, even after smb2's `quick_write_limit`
/// moved under it (the upload rate changed or expired, or the credit window
/// shrank). A promised write is the only one aimed at the user's real name, so
/// that name keeps `max_write`; a staging temp follows the limit as it is now,
/// since streaming onto a temp is safe.
#[test]
fn a_real_name_keeps_every_size_a_promise_could_have_covered() {
    let max_write = 8 * 1024 * 1024;
    let shrunk = 29 * 65_536;

    assert_eq!(one_frame_write_limit(false, max_write, shrunk), max_write);
    assert_eq!(one_frame_write_limit(true, max_write, shrunk), shrunk);
    assert_eq!(one_frame_write_limit(true, max_write, max_write), max_write);
    assert_eq!(
        one_frame_write_limit(true, max_write, 0),
        0,
        "a tiny window streams every temp"
    );
}

/// No live session means no promise: the transfer layer stages the write, as it
/// does for any backend without the guarantee. ❌ Never answer from the size
/// alone.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_disconnected_share_promises_nothing_about_single_shot_writes() {
    let vol = make_test_volume();
    assert!(!vol.write_is_single_shot(StreamLength::Known(10)).await);
    assert!(vol.supports_unknown_length_writes());
    assert!(!vol.write_is_single_shot(StreamLength::Unknown).await);
}
