//! Where a volume's session stands: this backend's own states, and the rung the
//! session came up on.
//!
//! The reporting rules (transitions, never states; a retired volume stays
//! silent; a failing call loses only the session it ran on) belong to
//! `cmdr_fs::volume::ConnectionReporter`, which every connecting backend shares.

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::host::events::VolumeConnection;
use cmdr_fs::volume::{CellState, Retirement, Retires, SelfHandle};

use super::SftpVolumeInner;
use crate::auth::AuthRungUsed;

/// How this volume's session stands.
///
/// Stored in a `ConnectionCell` beside the session's generation, for lock-free
/// reads from any thread. Four values
/// rather than SMB's two, because the last two are states this backend RESTS in:
/// a rung that redials out of the secret store stops after one refusal, and a key
/// that no longer matches must not come back at all until a human has looked at
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ConnectionState {
    /// The session is up and serving.
    Connected = 0,
    /// The session is gone and something is expected to retry.
    Disconnected = 1,
    /// Only the user can move this forward: retrying costs an authentication
    /// attempt and buys nothing.
    NeedsCredentials = 2,
    /// The server's key isn't the one trusted for it. ❌ Never reported as
    /// `NeedsCredentials`: putting a password box in front of a possible
    /// man-in-the-middle is how a password gets typed into one.
    NeedsHostKeyApproval = 3,
}

impl CellState for ConnectionState {
    const LIVE: Self = Self::Connected;
    const LOST: Self = Self::Disconnected;

    fn to_byte(self) -> u8 {
        self as u8
    }

    fn from_byte(byte: u8) -> Self {
        match byte {
            0 => Self::Connected,
            2 => Self::NeedsCredentials,
            3 => Self::NeedsHostKeyApproval,
            _ => Self::Disconnected,
        }
    }
}

/// Widens the backend's own state into the enum every connecting backend reports
/// in.
///
/// ❌ Deliberately not the frontend's wire enum: `events::volume_mapping` owns
/// that translation for every backend, and producing it here would weld this
/// crate to the app.
impl From<ConnectionState> for VolumeConnection {
    fn from(state: ConnectionState) -> Self {
        match state {
            ConnectionState::Connected => Self::Connected,
            ConnectionState::Disconnected => Self::Disconnected,
            ConnectionState::NeedsCredentials => Self::NeedsCredentials,
            ConnectionState::NeedsHostKeyApproval => Self::NeedsHostKeyApproval,
        }
    }
}

/// The connection-scoped half carries the flag, which is what the reconnect loop
/// reaches back through.
impl Retires for SftpVolumeInner {
    fn retirement(&self) -> &Retirement {
        self.connection.retirement()
    }
}

impl SftpVolumeInner {
    /// This volume's own handle, for the background work that has to keep asking
    /// whether the registry still serves it.
    pub(super) fn self_handle(&self) -> SelfHandle<SftpVolumeInner> {
        SelfHandle::new(self.me.clone())
    }

    /// Which credential built the live session.
    pub(super) fn auth_rung(&self) -> AuthRungUsed {
        *self.rung.lock_ignore_poison()
    }

    /// Records the rung a fresh session came up on.
    pub(super) fn set_auth_rung(&self, rung: AuthRungUsed) {
        *self.rung.lock_ignore_poison() = rung;
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
