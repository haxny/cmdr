//! Where a volume's connection stands, and the rule that keeps the frontend
//! from flickering.
//!
//! **A backend reports transitions, never states.** A server that is down
//! fails every request aimed at it, and reporting each failure would flood a
//! frontend that acted on the first one. So every report goes through
//! [`WebdavVolumeInner::emit_if_changed`], which swaps an atomic and stays
//! quiet when the value didn't move. ❗ A retired volume stays silent: its id
//! belongs to a newer instance now.

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
        &self.retirement
    }
}

impl WebdavVolumeInner {
    /// This volume's own handle, for the background work that has to keep
    /// asking whether the registry still serves it.
    pub(super) fn self_handle(&self) -> SelfHandle<WebdavVolumeInner> {
        SelfHandle::new(self.me.clone())
    }

    /// Whether this volume has stopped owning its id.
    pub(super) fn is_retired(&self) -> bool {
        self.retirement.is_retired()
    }

    /// Where the connection stands right now.
    pub(super) fn connection_state(&self) -> ConnectionState {
        self.state.state()
    }

    /// The installed client's generation: what `Volume::connection_generation`
    /// answers, and what a call reads BEFORE it runs to name the client it ran
    /// on (`volume_impl.rs` § noting).
    pub(super) fn session_generation(&self) -> u64 {
        self.state.generation()
    }

    /// Records that the connection is gone ❗ without reporting it, for the
    /// paths where the volume is LEAVING: the frontend learns through
    /// `volumes-changed`, and a `disconnected` alongside it would race that
    /// into a banner for a volume no longer in the sidebar.
    pub(super) fn mark_gone_silently(&self) {
        self.state.set(ConnectionState::Disconnected);
    }

    /// Moves to `next` and reports it, ❗ only if that is a change. Returns
    /// whether anything moved, so a transition several tasks noticed at once
    /// is acted on once. For a reconnect attempt's outcome; a failing call goes
    /// through [`lose_session`](Self::lose_session).
    pub(super) fn emit_if_changed(&self, next: ConnectionState) -> bool {
        let moved = self.state.set(next);
        if moved {
            self.announce(next);
        }
        moved
    }

    /// The client generation `ran_on` found the server gone: `Connected` →
    /// `Disconnected` and reported, ❗ only while `ran_on` is still the
    /// installed client. Whether this call flipped it, so exactly one of the
    /// requests meeting the same dead server starts the recovery.
    pub(super) fn lose_session(&self, ran_on: u64) -> bool {
        let moved = self.state.lose(ran_on);
        if moved {
            self.announce(ConnectionState::Disconnected);
        }
        moved
    }

    /// A rebuilt client is in place: the next generation and `Connected`, in
    /// one step, reported if the state moved. Called under the same guard that
    /// installs the client.
    pub(super) fn session_installed(&self) {
        if self.state.install() {
            self.announce(ConnectionState::Connected);
        }
    }

    /// Tells the host about a transition, ❗ unless retired: the id belongs to
    /// a newer instance now.
    fn announce(&self, state: ConnectionState) {
        if !self.is_retired() {
            self.host.events().connection_changed(&self.volume_id, state.into());
        }
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
