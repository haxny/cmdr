//! Where a volume's connection stands, in this backend's own states.
//!
//! The reporting rules (transitions, never states; a retired volume stays
//! silent; a failing call loses only the client it ran on) belong to
//! `cmdr_fs::volume::ConnectionReporter`, which every connecting backend shares.

use cmdr_fs::volume::host::events::VolumeConnection;
use cmdr_fs::volume::{CellState, Retirement, Retires, SelfHandle};

use super::WebdavVolumeInner;

/// How this volume's connection stands. HTTP has no session, so "connected"
/// means the last request that reached the wire came back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ConnectionState {
    /// Requests are being answered.
    Connected = 0,
    /// The last request found the server gone; something is expected to retry.
    Disconnected = 1,
    /// The server refused the stored secret. Only the user moves this forward:
    /// retrying costs an authentication attempt and buys nothing.
    NeedsCredentials = 2,
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
            _ => Self::Disconnected,
        }
    }
}

impl From<ConnectionState> for VolumeConnection {
    fn from(state: ConnectionState) -> Self {
        match state {
            ConnectionState::Connected => Self::Connected,
            ConnectionState::Disconnected => Self::Disconnected,
            ConnectionState::NeedsCredentials => Self::NeedsCredentials,
        }
    }
}

impl Retires for WebdavVolumeInner {
    fn retirement(&self) -> &Retirement {
        self.connection.retirement()
    }
}

impl WebdavVolumeInner {
    /// This volume's own handle, for the background work that has to keep
    /// asking whether the registry still serves it.
    pub(super) fn self_handle(&self) -> SelfHandle<WebdavVolumeInner> {
        SelfHandle::new(self.me.clone())
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
