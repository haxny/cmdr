//! [`LatestPin`]: which snapshot `latest` shows, and when it may move
//! (DETAILS § "`latest`").

use crate::snapshots::SnapshotList;
use std::time::{Duration, Instant};

/// Which snapshot `latest` shows, and when something last read under it.
#[derive(Default)]
pub(super) struct LatestPin {
    id: Option<String>,
    touched: Option<Instant>,
}

impl LatestPin {
    /// The snapshot `latest` shows, once a refresh bound it.
    pub(super) fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    pub(super) fn touch(&mut self) {
        self.touched = Some(Instant::now());
    }

    /// After a refresh: moves to `list`'s newest snapshot when nothing has
    /// pinned it, nothing read under it for `quiet`, or its snapshot is gone.
    /// While a transfer holds the repo (`held`: a copy paused between files
    /// reads nothing, so only its hold says it's there) it moves only when its
    /// snapshot is gone, so the copy's remaining files come from the same one.
    pub(super) fn rebind(&mut self, list: &SnapshotList, quiet: Duration, held: bool) {
        let newest = list.latest().map(|s| s.id.clone());
        let idle = !held && self.touched.is_none_or(|at| at.elapsed() >= quiet);
        let gone = self.id.as_deref().is_none_or(|id| list.by_id(id).is_none());
        if idle || gone {
            self.id = newest;
        }
    }
}
