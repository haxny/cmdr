//! Where a volume's connection stands, and the rule that keeps the frontend from
//! flickering.
//!
//! **A backend reports transitions, never states.** A server that is down fails
//! every operation aimed at it, and reporting each failure as news would flood a
//! frontend that acted on the first one. So every report goes through
//! [`SftpVolumeInner::emit_if_changed`], which swaps an atomic and stays quiet
//! when the value didn't move.
//!
//! ❗ **A retired volume stays silent.** Once a newer instance owns the id, this
//! one's news would describe a volume that no longer exists under that name, and
//! the frontend would show a healthy server as dropped.
//!
//! ❗ **A failing call loses only the session it ran on.** The state shares one
//! atomic with the session's generation (`cmdr_fs::volume::ConnectionCell`), so a
//! late error from a session a reconnect already replaced can't mark the fresh
//! one lost, drop it, and hand every operation on it an outage it never had
//! ([`SftpVolumeInner::lose_session`]).

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
        &self.retirement
    }
}

impl SftpVolumeInner {
    /// This volume's own handle, for the background work that has to keep asking
    /// whether the registry still serves it.
    pub(super) fn self_handle(&self) -> SelfHandle<SftpVolumeInner> {
        SelfHandle::new(self.me.clone())
    }

    /// Whether this volume has stopped owning its id: superseded by a newer
    /// instance, or removed from the registry outright.
    pub(super) fn is_retired(&self) -> bool {
        self.retirement.is_retired()
    }

    /// Which credential built the live session.
    pub(super) fn auth_rung(&self) -> AuthRungUsed {
        *self.rung.lock_ignore_poison()
    }

    /// Records the rung a fresh session came up on.
    pub(super) fn set_auth_rung(&self, rung: AuthRungUsed) {
        *self.rung.lock_ignore_poison() = rung;
    }

    /// Where the connection stands right now.
    pub(super) fn connection_state(&self) -> ConnectionState {
        self.state.state()
    }

    /// The installed session's generation: what `Volume::connection_generation`
    /// answers, and what a call reads BEFORE it runs to name the session it ran
    /// on (`volume_impl.rs` § noting).
    pub(super) fn session_generation(&self) -> u64 {
        self.state.generation()
    }

    /// Records that the session is gone ❗ without reporting it.
    ///
    /// For the two paths where the volume is LEAVING: the frontend learns through
    /// `volumes-changed`, and a `disconnected` alongside it would race that into a
    /// banner for a volume that is no longer in the sidebar. It also closes the
    /// edge, so an in-flight operation failing a moment later finds the state
    /// already moved and doesn't report one of its own.
    pub(super) fn mark_gone_silently(&self) {
        self.state.set(ConnectionState::Disconnected);
    }

    /// Moves to `next` and reports it, ❗ only if that is a change.
    ///
    /// Returns whether anything moved, which is what lets the caller act once on
    /// a transition several threads noticed at the same moment. For a reconnect
    /// attempt's outcome; a failing call goes through
    /// [`lose_session`](Self::lose_session).
    pub(super) fn emit_if_changed(&self, next: ConnectionState) -> bool {
        let moved = self.state.set(next);
        if moved {
            self.announce(next);
        }
        moved
    }

    /// The session generation `ran_on` is gone: `Connected` → `Disconnected`
    /// and reported, ❗ only while `ran_on` is still the installed session.
    /// Whether this call flipped it, so exactly one of the operations meeting
    /// the same dead session starts the recovery.
    pub(super) fn lose_session(&self, ran_on: u64) -> bool {
        let moved = self.state.lose(ran_on);
        if moved {
            self.announce(ConnectionState::Disconnected);
        }
        moved
    }

    /// A rebuilt session is in place: the next generation and `Connected`, in
    /// one step, reported if the state moved. Called under the same guard that
    /// installs the session.
    pub(super) fn session_installed(&self) {
        if self.state.install() {
            self.announce(ConnectionState::Connected);
        }
    }

    /// Tells the host about a transition, ❗ unless retired: a retired volume
    /// still tracks its own state for whoever holds it, but the id belongs to
    /// somebody else now.
    fn announce(&self, state: ConnectionState) {
        if !self.is_retired() {
            self.host.events().connection_changed(&self.volume_id, state.into());
        }
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
