use std::sync::Arc;

use super::CellState;
use super::connection_reporter::ConnectionReporter;
use super::host::VolumeHost;
use super::host::events::{RecordingVolumeEvents, VolumeConnection, VolumeEventSink};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Up,
    Down,
    NeedsUser,
}

impl CellState for State {
    const LIVE: Self = Self::Up;
    const LOST: Self = Self::Down;

    fn to_byte(self) -> u8 {
        self as u8
    }

    fn from_byte(byte: u8) -> Self {
        match byte {
            0 => Self::Up,
            2 => Self::NeedsUser,
            _ => Self::Down,
        }
    }
}

impl From<State> for VolumeConnection {
    fn from(state: State) -> Self {
        match state {
            State::Up => Self::Connected,
            State::Down => Self::Disconnected,
            State::NeedsUser => Self::NeedsCredentials,
        }
    }
}

fn watched() -> (Arc<RecordingVolumeEvents>, ConnectionReporter<State>) {
    let events = Arc::new(RecordingVolumeEvents::new());
    let host = VolumeHost::builder()
        .events(Arc::clone(&events) as Arc<dyn VolumeEventSink>)
        .build();
    (events, ConnectionReporter::new("vol-1", &host, State::Up))
}

fn seen(events: &RecordingVolumeEvents) -> Vec<VolumeConnection> {
    events.transitions().into_iter().map(|(_, state)| state).collect()
}

#[test]
fn a_state_that_did_not_move_reports_nothing() {
    let (events, reporter) = watched();
    assert!(reporter.emit_if_changed(State::Down));
    assert!(!reporter.emit_if_changed(State::Down));
    assert!(reporter.emit_if_changed(State::Up));
    assert_eq!(
        seen(&events),
        vec![VolumeConnection::Disconnected, VolumeConnection::Connected]
    );
}

#[test]
fn a_transition_names_the_volume_it_is_about() {
    let (events, reporter) = watched();
    reporter.emit_if_changed(State::NeedsUser);
    assert_eq!(
        events.transitions(),
        vec![("vol-1".to_string(), VolumeConnection::NeedsCredentials)]
    );
}

#[test]
fn a_retired_reporter_tracks_its_state_but_reports_nothing() {
    let (events, reporter) = watched();
    reporter.retirement().retire();
    assert!(reporter.is_retired());
    assert!(reporter.emit_if_changed(State::Down));
    assert_eq!(reporter.state(), State::Down);
    assert!(reporter.install());
    assert!(events.transitions().is_empty());
}

#[test]
fn leaving_silently_closes_the_edge_for_a_later_loss() {
    let (events, reporter) = watched();
    reporter.mark_gone_silently();
    assert!(
        !reporter.lose(reporter.generation()),
        "the state already moved, so the failing call has nothing to report"
    );
    assert!(events.transitions().is_empty());
}

#[test]
fn a_loss_reports_once_and_only_for_the_installed_session() {
    let (events, reporter) = watched();
    let first = reporter.generation();
    assert!(reporter.lose(first));
    assert!(
        !reporter.lose(first),
        "the second call meeting the same dead session moves nothing"
    );
    assert!(reporter.install());
    assert!(
        !reporter.lose(first),
        "a late error from the replaced session leaves its successor live"
    );
    assert_eq!(reporter.state(), State::Up);
    assert_eq!(
        seen(&events),
        vec![VolumeConnection::Disconnected, VolumeConnection::Connected]
    );
}

#[test]
fn an_install_over_a_live_session_moves_the_generation_quietly() {
    let (events, reporter) = watched();
    assert!(!reporter.install(), "the state didn't move");
    assert_eq!(reporter.generation(), 1);
    assert!(events.transitions().is_empty());
}

#[test]
fn a_revive_reports_only_a_connection_that_was_down() {
    let (events, reporter) = watched();
    assert!(!reporter.revive());
    assert_eq!(reporter.generation(), 0, "a probe finding it live starts nothing new");
    reporter.emit_if_changed(State::Down);
    assert!(reporter.revive());
    assert_eq!(reporter.generation(), 1);
    assert_eq!(
        seen(&events),
        vec![VolumeConnection::Disconnected, VolumeConnection::Connected]
    );
}
