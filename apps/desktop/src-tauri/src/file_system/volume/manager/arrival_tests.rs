//! The registry's arrival announcement (`VolumeManager::on_volume_arrival`):
//! what a deferred in-flight record waits on, so every way a volume gets in has
//! to announce it.

use std::sync::{Arc, Mutex};

use super::get_volume_manager;
use crate::file_system::volume::{InMemoryVolume, Volume};
use crate::ignore_poison::IgnorePoison;

/// The registry's arrival announcement is what makes a deferred record
/// terminate, so it has to actually fire for every way a volume gets in.
#[test]
fn every_registration_path_announces_the_volume() {
    let heard = Arc::new(Mutex::new(Vec::<String>::new()));
    let recorder = Arc::clone(&heard);
    get_volume_manager().on_volume_arrival(move |id| {
        if id.starts_with("in-flight-temps-arrival-probe") {
            recorder.lock_ignore_poison().push(id.to_string());
        }
    });

    let manager = get_volume_manager();
    let registered = manager.register_if_absent(
        "in-flight-temps-arrival-probe-absent",
        Arc::new(InMemoryVolume::new("probe")) as Arc<dyn Volume>,
    );
    assert!(registered, "the fixture id must be free");
    manager.register(
        "in-flight-temps-arrival-probe-register",
        Arc::new(InMemoryVolume::new("probe")) as Arc<dyn Volume>,
    );
    manager.unregister("in-flight-temps-arrival-probe-absent");
    manager.unregister("in-flight-temps-arrival-probe-register");

    let heard = heard.lock_ignore_poison().clone();
    assert!(
        heard.contains(&"in-flight-temps-arrival-probe-absent".to_string())
            && heard.contains(&"in-flight-temps-arrival-probe-register".to_string()),
        "both registration paths must announce; heard {heard:?}"
    );
}

/// A share that drops and comes back IN PLACE never registers again, yet the
/// records the in-flight ledger holds for it (a cancelled copy's temps the
/// share couldn't take while it was down) must be settled then too. So a
/// registered volume's `Connected` transition announces it the way a
/// registration does; any other transition, or an id nothing registered, says
/// nothing. Every fact still reaches the sink that tells the frontend.
#[test]
fn a_registered_volume_that_reconnects_in_place_announces_itself() {
    use cmdr_fs::volume::host::events::{RecordingVolumeEvents, VolumeConnection, VolumeEventSink};

    const REGISTERED: &str = "in-flight-temps-reconnect-probe-registered";
    const NOBODY: &str = "in-flight-temps-reconnect-probe-unregistered";
    let heard = Arc::new(Mutex::new(Vec::<String>::new()));
    let recorder = Arc::clone(&heard);
    get_volume_manager().on_volume_arrival(move |id| {
        if id.starts_with("in-flight-temps-reconnect-probe") {
            recorder.lock_ignore_poison().push(id.to_string());
        }
    });
    get_volume_manager().register(REGISTERED, Arc::new(InMemoryVolume::new("probe")) as Arc<dyn Volume>);
    heard.lock_ignore_poison().clear();
    let frontend = Arc::new(RecordingVolumeEvents::new());
    let sink = super::ArrivalOnReconnect::new(Arc::clone(&frontend) as Arc<dyn VolumeEventSink>);

    sink.connection_changed(REGISTERED, VolumeConnection::Disconnected);
    sink.connection_changed(REGISTERED, VolumeConnection::Connected);
    sink.connection_changed(NOBODY, VolumeConnection::Connected);
    get_volume_manager().unregister(REGISTERED);

    assert_eq!(
        heard.lock_ignore_poison().clone(),
        vec![REGISTERED.to_string()],
        "only the registered volume's reconnect is an arrival"
    );
    assert_eq!(
        frontend.transitions().len(),
        3,
        "every transition still reaches the frontend's sink"
    );
}
