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
