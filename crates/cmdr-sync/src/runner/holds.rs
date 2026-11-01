//! Which removals wait because a copy didn't land. In Mirror a rename (or a
//! move to another folder) is a copy plus a removal; the removal must not run
//! alone. A removal waits when it sits beside a copy that didn't land, or
//! when it reads like that copy's source (the same file under its old name).

use super::Runner;
use crate::{Entry, EntryKey, EntryKind, RunItemResult, Side};

/// A copy row that didn't end `Done`.
pub(super) struct Hold {
    side: Side,
    parent: Option<EntryKey>,
    source: Entry,
}

/// Whether two entries look like the same content: kind, size, and time
/// (folders: file count and bytes).
fn same_content(a: &Entry, b: &Entry) -> bool {
    if a.facts.kind != b.facts.kind {
        return false;
    }
    match a.facts.kind {
        EntryKind::Folder => match (a.subtree, b.subtree) {
            (Some(x), Some(y)) => x.files == y.files && x.bytes == y.bytes,
            _ => false,
        },
        EntryKind::File | EntryKind::Symlink => a.facts.size == b.facts.size && a.facts.mtime == b.facts.mtime,
    }
}

impl Hold {
    /// Whether this hold keeps `side`'s removal of `entry` at `key` waiting.
    pub(super) fn holds(&self, side: Side, key: &EntryKey, entry: &Entry) -> bool {
        self.side == side && (key.parent() == self.parent || same_content(entry, &self.source))
    }
}

impl Runner<'_> {
    /// Every copy row of the run that didn't end `Done` (failed, changed,
    /// refused, blocked, unplaced, or never started).
    pub(super) fn unlanded_copies(&self) -> Vec<Hold> {
        self.acting
            .iter()
            .map(|&index| self.item(index))
            .filter(|item| item.action.is_copy() && self.results.get(&item.index) != Some(&RunItemResult::Done))
            .filter_map(|item| {
                Some(Hold {
                    side: item.action.target()?,
                    parent: item.key.parent(),
                    source: item.entry(item.action.needs()?)?.clone(),
                })
            })
            .collect()
    }
}
