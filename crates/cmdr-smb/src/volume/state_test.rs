//! The share's binary connection-state machine, and how it widens.

use super::*;

#[test]
fn connection_state_round_trip() {
    for state in [ConnectionState::Direct, ConnectionState::Disconnected] {
        assert_eq!(ConnectionState::from_u8(state as u8), state);
    }
}

/// The backend widens its binary state machine into the BACKEND-FACING enum
/// (`cmdr_fs::volume::host::events::VolumeConnection`), never straight into the
/// frontend's wire enum: `events::volume_mapping` owns that second hop for every
/// backend, and a `From` into a `network` type here welds the SMB backend and
/// `network/` into one module cycle (`network/DETAILS.md` § "The one edge that must
/// not come back").
#[test]
fn connection_state_widens_into_the_backend_facing_enum() {
    use cmdr_fs::volume::host::events::VolumeConnection;

    assert_eq!(
        VolumeConnection::from(ConnectionState::Direct),
        VolumeConnection::Connected
    );
    assert_eq!(
        VolumeConnection::from(ConnectionState::Disconnected),
        VolumeConnection::Disconnected
    );
}

#[test]
fn connection_state_unknown_value_defaults_to_disconnected() {
    // The internal state machine is binary; `1` (the old `OsMount`
    // discriminant) and any other unknown byte must decode as
    // `Disconnected`, the safe / "stop using smb2" state.
    assert_eq!(ConnectionState::from_u8(1), ConnectionState::Disconnected);
    assert_eq!(ConnectionState::from_u8(255), ConnectionState::Disconnected);
}

/// `EPIPE` and the two local refusals the session must outlive, by number: the
/// same on macOS and Linux.
const EPIPE: i32 = 32;
const EACCES: i32 = 13;
const ENOSPC: i32 = 28;

/// What every funnel asks: a lost connection (the failing sender's `SendFailed`
/// as much as every other waiter's `Disconnected`), an expired session, and a
/// refused signature end the session; a local I/O failure never does, whatever
/// its errno.
#[test]
fn only_a_lost_expired_or_tainted_session_ends_it() {
    let send_failed = smb2::Error::SendFailed {
        command: smb2::types::Command::Write,
        source: std::io::Error::from_raw_os_error(EPIPE),
    };
    assert!(ends_the_session(&send_failed));
    assert!(ends_the_session(&smb2::Error::Disconnected));
    assert!(ends_the_session(&smb2::Error::SessionExpired));
    assert!(ends_the_session(&smb2::Error::SignatureInvalid {
        command: smb2::types::Command::Read,
        message_id: smb2::types::MessageId(7),
        reason: smb2::error::SignatureFailure::Mismatch,
    }));

    for errno in [EPIPE, EACCES, ENOSPC] {
        let local = smb2::Error::Io(std::io::Error::from_raw_os_error(errno));
        assert!(
            !ends_the_session(&local),
            "a local Io (errno {errno}) never ends the session"
        );
    }
    assert!(!ends_the_session(&smb2::Error::Timeout));
}

/// A loss marks only the session it happened on: once a reconnect installed the
/// next generation, a late loss from the previous one leaves the state alone.
#[test]
fn a_loss_from_a_replaced_session_leaves_the_state_alone() {
    let cell = SessionCell::new(ConnectionState::Direct, SessionGeneration::FIRST);
    assert!(
        cell.lose(SessionGeneration::FIRST),
        "the installed session's own loss flips it"
    );
    assert!(!cell.lose(SessionGeneration::FIRST), "and only once");

    let next = cell.advance();
    assert_ne!(next, SessionGeneration::FIRST);
    assert!(cell.restore(next));
    assert_eq!(cell.state(), ConnectionState::Direct);

    assert!(
        !cell.lose(SessionGeneration::FIRST),
        "a late loss from the replaced session"
    );
    assert_eq!(cell.state(), ConnectionState::Direct);
    assert!(cell.lose(next), "the current session's own loss still counts");
    assert_eq!(cell.state(), ConnectionState::Disconnected);
}

/// The generation and the state share one word: `advance` keeps the state, and a
/// restore for a generation that isn't installed does nothing.
#[test]
fn advancing_keeps_the_state_and_restore_needs_the_installed_generation() {
    let cell = SessionCell::new(ConnectionState::Disconnected, SessionGeneration::FIRST);
    let next = cell.advance();
    assert_eq!(cell.state(), ConnectionState::Disconnected);
    assert!(!cell.restore(SessionGeneration::FIRST), "not the installed session");
    assert_eq!(cell.state(), ConnectionState::Disconnected);
    assert!(cell.restore(next));
    assert!(!cell.restore(next), "already Direct");
    cell.shut_down();
    assert_eq!(cell.state(), ConnectionState::Disconnected);
}
