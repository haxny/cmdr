//! The non-macOS watch's `notify` half (`watcher/notify_stream.rs`): how a debounced
//! inotify batch becomes `ListingChange`s.

use std::path::{Path, PathBuf};
use std::time::Instant;

use notify_debouncer_full::DebouncedEvent;
use notify_debouncer_full::notify::Event;
use notify_debouncer_full::notify::event::{
    AccessKind, CreateKind, DataChange, EventKind, Flag, ModifyKind, RemoveKind, RenameMode,
};

use super::watcher::{ListingChange, translate};

fn event(kind: EventKind, paths: &[&str]) -> DebouncedEvent {
    DebouncedEvent::new(
        Event {
            kind,
            paths: paths.iter().map(PathBuf::from).collect(),
            attrs: Default::default(),
        },
        Instant::now(),
    )
}

fn structural(path: &str) -> ListingChange {
    ListingChange::Structural(Path::new(path).to_path_buf())
}

#[test]
fn creates_removes_and_renames_are_structural() {
    let changes = translate(vec![
        event(EventKind::Create(CreateKind::File), &["/d/a"]),
        event(EventKind::Remove(RemoveKind::File), &["/d/b"]),
        event(EventKind::Modify(ModifyKind::Name(RenameMode::Both)), &["/d/c", "/d/e"]),
    ]);

    assert_eq!(
        changes,
        vec![
            structural("/d/a"),
            structural("/d/b"),
            structural("/d/c"),
            structural("/d/e")
        ]
    );
}

#[test]
fn a_data_change_is_content_and_an_access_is_nothing() {
    let changes = translate(vec![
        event(EventKind::Modify(ModifyKind::Data(DataChange::Content)), &["/d/a"]),
        event(EventKind::Access(AccessKind::Read), &["/d/b"]),
    ]);

    assert_eq!(changes, vec![ListingChange::Content(PathBuf::from("/d/a"))]);
}

#[test]
fn any_other_and_rescan_flagged_events_are_rescans() {
    let mut overflowed = event(EventKind::Modify(ModifyKind::Any), &[]);
    overflowed.event.attrs.set_flag(Flag::Rescan);

    let changes = translate(vec![
        event(EventKind::Any, &["/d/a"]),
        event(EventKind::Other, &["/d/b"]),
        overflowed,
    ]);

    assert_eq!(changes, vec![ListingChange::Rescan; 3]);
}
