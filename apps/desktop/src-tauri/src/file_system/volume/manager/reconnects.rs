//! A share that reconnects IN PLACE is an arrival too.
//!
//! The registry announces a volume when it registers one
//! ([`VolumeManager::on_volume_arrival`]), and the in-flight ledger
//! (`cmdr_operations::in_flight_temps`) settles what it holds for that volume
//! then. A connecting backend that drops and rebuilds its session never
//! registers again, so without this a temp a cancelled copy left on a NAS that
//! blipped would wait for the next launch. [`ArrivalOnReconnect`] sits in front
//! of the backends' event sink and turns a registered volume's `Connected`
//! transition into the same announcement.
//!
//! The backends report transitions, never states, and a retired instance stays
//! silent (`cmdr_fs::volume::ConnectionReporter`), so a flapping link announces
//! once per reconnect and a superseded volume never announces its successor's
//! id.

use std::sync::{Arc, LazyLock};

use cmdr_fs::volume::host::events::{SecurityRefusal, VolumeConnection, VolumeEventSink};
use tokio::sync::broadcast;

use super::get_volume_manager;

/// One connection transition a backend reported, as the backend-side sink saw it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionNews {
    pub volume_id: String,
    pub connection: VolumeConnection,
}

/// Every transition, for in-process work waiting on one volume's next one (Ask
/// Cmdr's `connect_volume` waiting out a reconnect). A lagging or absent listener
/// costs nothing: a send with nobody subscribed is dropped.
static CONNECTION_NEWS: LazyLock<broadcast::Sender<ConnectionNews>> = LazyLock::new(|| broadcast::channel(64).0);

/// Listen for every connection transition from now on. ❗ Subscribe BEFORE asking
/// for the change you'll wait on, or it can land unheard.
pub fn connection_news() -> broadcast::Receiver<ConnectionNews> {
    CONNECTION_NEWS.subscribe()
}

/// Forwards every connection fact to `inner`, and announces a registered
/// volume's reconnect to the registry's arrival listeners first.
pub struct ArrivalOnReconnect {
    inner: Arc<dyn VolumeEventSink>,
}

impl ArrivalOnReconnect {
    /// Wraps `inner`, the sink that tells the frontend.
    pub fn new(inner: Arc<dyn VolumeEventSink>) -> Self {
        Self { inner }
    }
}

impl VolumeEventSink for ArrivalOnReconnect {
    fn connection_changed(&self, volume_id: &str, connection: VolumeConnection) {
        if connection == VolumeConnection::Connected {
            get_volume_manager().announce_reconnected(volume_id);
        }
        // Nobody listening is the usual case, not a problem.
        let _ = CONNECTION_NEWS.send(ConnectionNews {
            volume_id: volume_id.to_string(),
            connection,
        });
        self.inner.connection_changed(volume_id, connection);
    }

    fn security_refusal(&self, volume_id: &str, refusal: SecurityRefusal) {
        self.inner.security_refusal(volume_id, refusal);
    }
}
