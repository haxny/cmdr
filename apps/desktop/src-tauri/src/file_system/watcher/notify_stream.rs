//! The non-macOS half of a pane's watch: a debounced `notify` watcher (inotify on
//! Linux), translated into [`ListingChange`]s. macOS uses `fsevents.rs` instead.

use std::path::Path;
use std::time::Duration;

use notify_debouncer_full::{
    DebounceEventResult, DebouncedEvent, Debouncer, NoCache, new_debouncer_opt,
    notify::{
        self, RecommendedWatcher, RecursiveMode,
        event::{EventKind, ModifyKind},
    },
};

use super::ListingChange;

/// A live `notify` watch. Dropping it stops watching.
pub(super) type ListingStream = Debouncer<RecommendedWatcher, NoCache>;

/// Arms a non-recursive `notify` watch on `path` whose debounced batches reach
/// `deliver` on notify's own thread. A watcher error re-reads the listing.
pub(super) fn arm(
    path: &Path,
    debounce: Duration,
    deliver: impl Fn(Vec<ListingChange>) + Send + 'static,
) -> Result<ListingStream, String> {
    let mut debouncer = new_debouncer_opt::<_, RecommendedWatcher, NoCache>(
        debounce,
        None, // No tick rate limit
        move |result: DebounceEventResult| match result {
            Ok(events) => deliver(translate(events)),
            // Watcher errors often mean the watched directory was deleted. The re-read
            // emits `directory-deleted` when it finds it gone.
            Err(_errors) => deliver(vec![ListingChange::Rescan]),
        },
        // `NoCache`: the file-id map pairs a rename's two halves by `stat`ing every
        // entry at arm time, and the handler re-stats each path anyway.
        NoCache,
        notify::Config::default(),
    )
    .map_err(|e| format!("Failed to create watcher: {}", e))?;
    debouncer
        .watch(path, RecursiveMode::NonRecursive)
        .map_err(|e| format!("Failed to watch path: {}", e))?;
    Ok(debouncer)
}

/// One debounced batch as listing changes. A rescan-flagged, `Any`, or `Other` event
/// (an inotify queue overflow, say) is a rescan; access events say nothing changed.
pub(crate) fn translate(events: Vec<DebouncedEvent>) -> Vec<ListingChange> {
    let mut changes = Vec::with_capacity(events.len());
    for event in events {
        if event.need_rescan() || matches!(event.kind, EventKind::Any | EventKind::Other) {
            changes.push(ListingChange::Rescan);
            continue;
        }
        let structural = matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_))
        );
        if matches!(event.kind, EventKind::Access(_)) {
            continue;
        }
        for path in &event.paths {
            changes.push(if structural {
                ListingChange::Structural(path.clone())
            } else {
                ListingChange::Content(path.clone())
            });
        }
    }
    changes
}
