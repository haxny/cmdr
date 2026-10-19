//! What a write makes of its date stamp's answer.

use super::super::test_support::*;
use crate::volume::ConnectionState;

/// The stamp is the last step of a write before it reports, so a stamp that met
/// the connection dying must fail the write: reporting `Ok` there sends the
/// transfer on to a landing or the next file on a dead session. The error goes
/// through the funnel, so the share reads `Disconnected` too.
#[test]
fn a_stamp_that_meets_a_lost_connection_fails_the_write() {
    const EPIPE: i32 = 32;
    let lost = || smb2::Error::SendFailed {
        command: smb2::types::Command::SetInfo,
        source: std::io::Error::from_raw_os_error(EPIPE),
    };
    for err in [lost(), smb2::Error::Disconnected] {
        let vol = make_test_volume_direct();
        let outcome = vol.stamp_taken("a.txt", SessionGeneration::FIRST, Err(err));
        assert!(
            matches!(outcome, Err(VolumeError::DeviceDisconnected(_))),
            "got: {outcome:?}"
        );
        assert_eq!(vol.session_state(), ConnectionState::Disconnected);
    }
}

/// A server that refuses the dates leaves the server's own: the bytes landed,
/// and the write goes on and reports the refusal.
#[test]
fn a_refused_stamp_keeps_the_servers_date_and_the_write_goes_on() {
    let vol = make_test_volume_direct();
    let refused = smb2::Error::Protocol {
        status: smb2::types::status::NtStatus::ACCESS_DENIED,
        command: smb2::types::Command::SetInfo,
    };
    let outcome = vol.stamp_taken("a.txt", SessionGeneration::FIRST, Err(refused));
    assert!(matches!(outcome, Ok(false)), "got: {outcome:?}");
    assert_eq!(vol.session_state(), ConnectionState::Direct);
    assert!(matches!(
        vol.stamp_taken("a.txt", SessionGeneration::FIRST, Ok(())),
        Ok(true)
    ));
}
