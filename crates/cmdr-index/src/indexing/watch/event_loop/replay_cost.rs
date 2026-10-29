//! What a cold-start replay costs, and when it stops being worth it.
//!
//! Two pieces, both pure so their boundaries are pinned in the unit tier:
//!
//! - [`ReplayWindow`]: the dedup buffer. Replay events collapse by path inside a
//!   window bounded by DISTINCT paths, not by raw events, so a build that rewrites
//!   the same `target/` files millions of times costs one write per file per
//!   window.
//! - [`ReplayBudget`]: the cost cap. It counts the paths the replay actually
//!   processes (each one a stat, a path resolve, and an upsert) against the size
//!   of the index a full reconcile would walk instead.
//!
//! Why the cost is counted here and not as an FSEvents-id gap at launch: the id
//! counter is machine-wide and moves with every write, so a ten-minute `cargo`
//! build alone advances it by millions while touching a few hundred thousand
//! distinct paths. `watch/DETAILS.md` § "Replay cost, not event count".

use std::collections::HashMap;

use super::super::watcher;
use super::merge_fs_events;

/// Distinct paths the replay's dedup window holds before it drains.
///
/// Processing reads the disk as it is NOW (every event re-stats its path), so a
/// path processed once per window lands at its final state; a wider window only
/// saves repeat work. At ~300 B per entry (path key, path, flags) this is ~30 MB,
/// small next to the ingestion queue's 1.5 GB hard cap.
pub(super) const REPLAY_DEDUP_WINDOW_PATHS: usize = 100_000;

/// The smallest path budget a replay gets, whatever the index's size. An index
/// this small walks in seconds either way, so the floor only stops a near-empty
/// or unreadable entry count from refusing every replay.
pub(super) const REPLAY_PATH_BUDGET_FLOOR: u64 = 100_000;

/// The dedup buffer for replay's phase 1: one merged event per path.
pub(super) struct ReplayWindow {
    pending: HashMap<String, watcher::FsChangeEvent>,
    capacity: usize,
}

impl ReplayWindow {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            pending: HashMap::new(),
            capacity,
        }
    }

    /// Merge one event (its path already canonical) into the window. Returns
    /// whether the window now holds its capacity of distinct paths and should be
    /// drained before the next event.
    pub(super) fn add(&mut self, event: watcher::FsChangeEvent) -> bool {
        match self.pending.get_mut(&event.path) {
            Some(existing) => *existing = merge_fs_events(existing, &event),
            None => {
                self.pending.insert(event.path.clone(), event);
            }
        }
        self.pending.len() >= self.capacity
    }

    /// Distinct paths waiting to be processed.
    pub(super) fn len(&self) -> usize {
        self.pending.len()
    }

    /// The buffered events, for the drain to process and empty.
    pub(super) fn pending_mut(&mut self) -> &mut HashMap<String, watcher::FsChangeEvent> {
        &mut self.pending
    }
}

/// How many paths a replay may process before a full reconcile is the cheaper way
/// to bring the index current.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::indexing) struct ReplayBudget {
    limit: u64,
    spent: u64,
}

impl ReplayBudget {
    /// The budget for an index holding `entry_count` rows (`None` when the count
    /// couldn't be read). A full reconcile visits every entry once, so a replay
    /// that would process more paths than the index holds costs more than the
    /// walk it replaces.
    pub(in crate::indexing) fn for_index(entry_count: Option<u64>) -> Self {
        Self::with_limit(entry_count.unwrap_or(0).max(REPLAY_PATH_BUDGET_FLOOR))
    }

    /// A budget with an explicit limit, for tests that drive a tiny one.
    pub(in crate::indexing) fn with_limit(limit: u64) -> Self {
        Self { limit, spent: 0 }
    }

    /// Whether processing `paths` more would go past the budget. Asked BEFORE a
    /// window drains, so an over-budget replay stops without doing that window's
    /// work.
    pub(super) fn would_exceed(&self, paths: u64) -> bool {
        self.spent.saturating_add(paths) > self.limit
    }

    /// Record `paths` as processed.
    pub(super) fn spend(&mut self, paths: u64) {
        self.spent = self.spent.saturating_add(paths);
    }

    pub(super) fn spent(&self) -> u64 {
        self.spent
    }

    pub(super) fn limit(&self) -> u64 {
        self.limit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modified(path: &str, event_id: u64) -> watcher::FsChangeEvent {
        watcher::FsChangeEvent {
            path: path.to_string(),
            event_id,
            flags: watcher::FsEventFlags {
                item_modified: true,
                item_is_file: true,
                ..Default::default()
            },
        }
    }

    /// The build case from #751: the same files rewritten far more than any
    /// event-count batch holds still collapse to one entry each.
    #[test]
    fn repeats_far_apart_collapse_to_one_entry_per_path() {
        let mut window = ReplayWindow::new(REPLAY_DEDUP_WINDOW_PATHS);
        let mut event_id = 1;
        for _round in 0..50 {
            for file in 0..1_000 {
                assert!(!window.add(modified(&format!("/target/debug/deps/f{file}.o"), event_id)));
                event_id += 1;
            }
        }
        assert_eq!(window.len(), 1_000, "50,000 events over 1,000 files are 1,000 writes");
        let kept = &window.pending_mut()["/target/debug/deps/f0.o"];
        assert_eq!(kept.event_id, 49_001, "the merge keeps the newest id");
    }

    #[test]
    fn the_window_reports_full_at_its_distinct_path_capacity() {
        let mut window = ReplayWindow::new(3);
        assert!(!window.add(modified("/a", 1)));
        assert!(!window.add(modified("/b", 2)));
        assert!(!window.add(modified("/a", 3)), "a repeat doesn't grow the window");
        assert!(window.add(modified("/c", 4)), "the third distinct path fills it");
        window.pending_mut().clear();
        assert_eq!(window.len(), 0);
        assert!(!window.add(modified("/d", 5)), "a drained window starts over");
    }

    #[test]
    fn the_budget_is_the_index_size_with_a_floor() {
        assert_eq!(ReplayBudget::for_index(Some(4_000_000)).limit(), 4_000_000);
        assert_eq!(ReplayBudget::for_index(Some(12)).limit(), REPLAY_PATH_BUDGET_FLOOR);
        assert_eq!(
            ReplayBudget::for_index(None).limit(),
            REPLAY_PATH_BUDGET_FLOOR,
            "an unreadable count still lets a replay run"
        );
    }

    #[test]
    fn the_budget_refuses_the_window_that_would_go_past_it() {
        let mut budget = ReplayBudget::with_limit(250);
        assert!(!budget.would_exceed(100));
        budget.spend(100);
        assert!(!budget.would_exceed(150), "landing exactly on the limit is within it");
        budget.spend(150);
        assert_eq!(budget.spent(), 250);
        assert!(budget.would_exceed(1));
    }
}
