//! A connecting backend's news about its connection: each transition once, and
//! nothing at all once the volume is retired.
//!
//! **A backend reports transitions, never states.** A server that is down fails
//! every operation aimed at it, and reporting each failure as news would flood a
//! frontend that acted on the first one. So every report is a swap on the
//! [`ConnectionCell`] that stays quiet when the value didn't move.
//!
//! ❗ **A retired volume stays silent.** Once a newer instance owns the id, this
//! one's news would describe a volume that no longer exists under that name, and
//! the frontend would show a healthy server as dropped. The reporter owns the
//! [`Retirement`] so the flag and the rule that reads it can't drift apart.
//!
//! SFTP, WebDAV, S3, and ADB share it, each over its own state enum.

use std::sync::Arc;

use super::host::VolumeHost;
use super::host::events::{VolumeConnection, VolumeEventSink};
use super::{CellState, ConnectionCell, Retirement};

/// One volume's connection state and generation, the retirement flag that
/// silences it, and where its transitions are reported.
///
/// ❗ A failing call goes through [`lose`](Self::lose) with the generation it
/// STARTED on (`ConnectionCell` has why), ❌ never through
/// [`emit_if_changed`](Self::emit_if_changed), which is for verdicts about the
/// connection NOW.
pub struct ConnectionReporter<S> {
    cell: ConnectionCell<S>,
    retirement: Retirement,
    volume_id: String,
    events: Arc<dyn VolumeEventSink>,
}

impl<S: CellState + Into<VolumeConnection>> ConnectionReporter<S> {
    /// A reporter for `volume_id`, in `state`, on the session the volume was
    /// built with, telling `host`'s event sink.
    pub fn new(volume_id: &str, host: &VolumeHost, state: S) -> Self {
        Self {
            cell: ConnectionCell::new(state),
            retirement: Retirement::new(),
            volume_id: volume_id.to_string(),
            events: host.event_sink(),
        }
    }

    /// The id this reporter's news is filed under.
    pub fn volume_id(&self) -> &str {
        &self.volume_id
    }

    /// Where the connection stands right now.
    pub fn state(&self) -> S {
        self.cell.state()
    }

    /// The installed session's generation: what `Volume::connection_generation`
    /// answers, and what a call reads BEFORE it runs to name the session it ran
    /// on.
    pub fn generation(&self) -> u64 {
        self.cell.generation()
    }

    /// The flag the registry writes when it stops serving this volume, which
    /// `Volume::retirement` publishes.
    pub fn retirement(&self) -> &Retirement {
        &self.retirement
    }

    /// Whether this volume has stopped owning its id: superseded by a newer
    /// instance, or removed from the registry outright.
    pub fn is_retired(&self) -> bool {
        self.retirement.is_retired()
    }

    /// Records that the connection is gone ❗ without reporting it.
    ///
    /// For the paths where the volume is LEAVING: the frontend learns through
    /// `volumes-changed`, and a `disconnected` alongside it would race that into
    /// a banner for a volume no longer in the sidebar. It also closes the edge,
    /// so an in-flight operation failing a moment later finds the state already
    /// moved and doesn't report one of its own.
    pub fn mark_gone_silently(&self) {
        self.cell.set(S::LOST);
    }

    /// Moves to `next` and reports it, ❗ only if that is a change. Whether
    /// anything moved, so a transition several tasks noticed at once is acted on
    /// once. For a verdict about the connection now (a reconnect attempt's
    /// outcome, a device the OS says is gone).
    pub fn emit_if_changed(&self, next: S) -> bool {
        let moved = self.cell.set(next);
        if moved {
            self.announce(next);
        }
        moved
    }

    /// The session generation `ran_on` is gone: live → lost and reported, ❗
    /// only while `ran_on` is still the installed session. Whether this call
    /// flipped it, so exactly one of the calls meeting the same dead session
    /// starts the recovery.
    pub fn lose(&self, ran_on: u64) -> bool {
        let moved = self.cell.lose(ran_on);
        if moved {
            self.announce(S::LOST);
        }
        moved
    }

    /// A rebuilt session is in place: the next generation and live, in one
    /// step, reported if the state moved. Call it under the same guard that
    /// installs the session.
    pub fn install(&self) -> bool {
        let moved = self.cell.install();
        if moved {
            self.announce(S::LIVE);
        }
        moved
    }

    /// The connection came back without a session object to install: the next
    /// generation and live, reported, ❗ only if it wasn't live already
    /// ([`ConnectionCell::revive`]).
    pub fn revive(&self) -> bool {
        let moved = self.cell.revive();
        if moved {
            self.announce(S::LIVE);
        }
        moved
    }

    /// Tells the host about a transition, ❗ unless retired: a retired volume
    /// still tracks its own state for whoever holds it, but the id belongs to
    /// somebody else now.
    fn announce(&self, state: S) {
        if !self.retirement.is_retired() {
            self.events.connection_changed(&self.volume_id, state.into());
        }
    }
}
