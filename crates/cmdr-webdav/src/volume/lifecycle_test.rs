//! The registry's two hand-overs reach the shared lifecycle: a superseded
//! volume reports nothing, and an unmounted one reports no later disconnect.
//! The reporting rules themselves are `cmdr_fs`'s to test.

use std::sync::Arc;

use cmdr_fs::volume::Volume;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::events::{RecordingVolumeEvents, VolumeEventSink};
use cmdr_fs::volume::http_session::HttpConnectionState;

use super::WebdavVolume;
use super::test_support::make_test_volume_with;

fn watched() -> (Arc<RecordingVolumeEvents>, WebdavVolume) {
    let events = Arc::new(RecordingVolumeEvents::new());
    let host = VolumeHost::builder()
        .events(Arc::clone(&events) as Arc<dyn VolumeEventSink>)
        .build();
    (events, make_test_volume_with("/", host))
}

#[test]
fn a_retired_volume_reports_nothing_under_an_id_it_no_longer_owns() {
    let (events, volume) = watched();
    volume.on_superseded();
    let connection = volume.inner.session.connection();
    assert!(connection.emit_if_changed(HttpConnectionState::Disconnected));
    assert_eq!(connection.state(), HttpConnectionState::Disconnected);
    assert!(events.transitions().is_empty());
}

#[test]
fn a_volume_that_left_silently_reports_no_later_disconnect() {
    let (events, volume) = watched();
    volume.on_unmount();
    assert!(
        !volume
            .inner
            .session
            .connection()
            .emit_if_changed(HttpConnectionState::Disconnected)
    );
    assert!(events.transitions().is_empty());
}
