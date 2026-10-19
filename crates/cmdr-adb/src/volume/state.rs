//! Where a volume's connection stands, and the rule that keeps the frontend from
//! flickering.
//!
//! **A backend reports transitions, never states.** A device that is gone fails
//! every operation aimed at it, and reporting each failure as news would flood a
//! frontend that acted on the first one. So every report goes through
//! [`AdbVolumeInner::emit_if_changed`], which swaps an atomic and stays quiet
//! when the value didn't move.
//!
//! ❗ **A retired volume stays silent.** Once a newer instance owns the id, this
//! one's news would describe a volume that no longer exists under that name.
//!
//! ❗ **A failing call marks the device gone only for the generation it started
//! on.** There is no session here (one socket per operation), so the generation
//! counts the times a reconnect's hello brought the device BACK
//! (`cmdr_fs::volume::ConnectionCell::revive`): a late error from an operation that started
//! before the device came back can't mark it gone again
//! ([`AdbVolume::note_lost_session`]).

use std::sync::atomic::Ordering;

use cmdr_fs::volume::VolumeError;
use cmdr_fs::volume::host::events::VolumeConnection;
use cmdr_fs::volume::{CellState, Retirement, Retires};

use super::{AdbVolume, AdbVolumeInner};

/// How this volume's device stands, kept in a `ConnectionCell` beside the
/// generation.
///
/// Two values: there is no credential to ask for on this backend (the device
/// authorizes the HOST, once, on its own screen), so nothing rests between
/// "up" and "gone".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ConnectionState {
    /// The device answers sync requests.
    Connected = 0,
    /// The device is gone or not answering, and something is expected to retry.
    Disconnected = 1,
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
            _ => Self::Disconnected,
        }
    }
}

/// Widens the backend's own state into the enum every connecting backend reports
/// in. ❌ Deliberately not the frontend's wire enum: `events::volume_mapping`
/// owns that translation.
impl From<ConnectionState> for VolumeConnection {
    fn from(state: ConnectionState) -> Self {
        match state {
            ConnectionState::Connected => Self::Connected,
            ConnectionState::Disconnected => Self::Disconnected,
        }
    }
}

impl Retires for AdbVolumeInner {
    fn retirement(&self) -> &Retirement {
        &self.retirement
    }
}

impl AdbVolumeInner {
    /// Where the connection stands right now.
    pub(super) fn connection_state(&self) -> ConnectionState {
        self.state.state()
    }

    /// How many times a reconnect has brought the device back: what
    /// `Volume::connection_generation` answers, and what a call reads BEFORE it
    /// runs to name the generation it ran on (`volume_impl.rs` § noting).
    pub(super) fn generation(&self) -> u64 {
        self.state.generation()
    }

    /// Records that the device is gone ❗ without reporting it, for the paths
    /// where the volume is LEAVING: the frontend learns through
    /// `volumes-changed`, and a `disconnected` alongside it would race that into
    /// a banner for a volume no longer in the sidebar.
    pub(super) fn mark_gone_silently(&self) {
        self.state.set(ConnectionState::Disconnected);
    }

    /// Moves to `next` and reports it, ❗ only if that is a change. For a
    /// verdict about the device NOW (a failed hello, the tracker); a failing
    /// operation goes through [`AdbVolume::note_lost_session`].
    pub(super) fn emit_if_changed(&self, next: ConnectionState) -> bool {
        let moved = self.state.set(next);
        if moved {
            self.announce(next);
        }
        moved
    }

    /// Tells the host about a transition, ❗ unless retired.
    fn announce(&self, state: ConnectionState) {
        if !self.retirement.is_retired() {
            self.host.events().connection_changed(&self.volume_id, state.into());
        }
    }

    /// Re-runs the hello, single-flight, and reports the outcome as a
    /// transition.
    pub(super) async fn do_attempt_reconnect(&self) -> Result<(), VolumeError> {
        let _guard = self.reconnect_lock.lock().await;
        if self.unmounted.load(Ordering::Relaxed) || self.retirement.is_retired() {
            return Err(VolumeError::DeviceDisconnected(self.volume_id.clone()));
        }
        match self.hello().await {
            Ok(()) => {
                // The next generation only if the device was down: a hello that
                // finds it already up brought nothing back.
                if self.state.revive() {
                    self.announce(ConnectionState::Connected);
                }
                Ok(())
            }
            Err(e) => {
                self.emit_if_changed(ConnectionState::Disconnected);
                Err(self.map_adb_error(e, "/"))
            }
        }
    }
}

impl AdbVolume {
    /// Reads a failed operation's answer for "the device is gone" and reports
    /// the transition once.
    ///
    /// Operations ARE the detector on this backend: there is no watcher, so a
    /// device that went away is invisible until something asks it for
    /// something. ❗ Only for the generation the operation started on
    /// (`ran_on`, read before it ran): one that started before a reconnect
    /// brought the device back is old news, and marking the device gone on it
    /// would hand every operation since an outage it never had.
    pub(super) fn note_lost_session(&self, error: &VolumeError, ran_on: u64) {
        if matches!(
            error,
            VolumeError::DeviceDisconnected(_) | VolumeError::ConnectionTimeout(_)
        ) && self.inner.state.lose(ran_on)
        {
            self.inner.announce(ConnectionState::Disconnected);
        }
    }

    /// The device tracker saw this serial leave the list.
    ///
    /// Published so the app can retire the volume the moment `track-devices`
    /// says so, rather than waiting for the next operation to fail on it.
    pub fn note_device_gone(&self) {
        self.inner.emit_if_changed(ConnectionState::Disconnected);
    }
}
