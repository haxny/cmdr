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

/// Two facts agree, or one is unknown: an unknown never rules a match out, so
/// a hold errs on the side of waiting.
fn agrees<T: PartialEq>(a: Option<T>, b: Option<T>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a == b,
        _ => true,
    }
}

/// Whether two entries could be the same content: kind, size, and time
/// (folders: file count and bytes), each unknown counting as a match.
fn same_content(a: &Entry, b: &Entry) -> bool {
    if a.facts.kind != b.facts.kind {
        return false;
    }
    match a.facts.kind {
        EntryKind::Folder => {
            agrees(a.subtree.map(|t| t.files), b.subtree.map(|t| t.files))
                && agrees(a.subtree.map(|t| t.bytes), b.subtree.map(|t| t.bytes))
        }
        EntryKind::File | EntryKind::Symlink => {
            agrees(a.facts.size, b.facts.size) && agrees(a.facts.mtime, b.facts.mtime)
        }
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

#[cfg(test)]
mod tests {
    use super::same_content;
    use crate::{Entry, EntryFacts, EntryKind, SubtreeFacts};

    fn entry(kind: EntryKind, size: Option<u64>, mtime: Option<u64>, subtree: Option<SubtreeFacts>) -> Entry {
        Entry {
            key: crate::EntryKey::root(),
            rel_path: "x".to_owned(),
            facts: EntryFacts { kind, size, mtime },
            subtree,
        }
    }

    #[test]
    fn an_unknown_fact_never_rules_a_match_out() {
        let known = entry(EntryKind::File, Some(5), Some(9), None);
        assert!(same_content(&known, &entry(EntryKind::File, Some(5), None, None)));
        assert!(same_content(&known, &entry(EntryKind::File, None, None, None)));
        assert!(!same_content(&known, &entry(EntryKind::File, Some(6), None, None)));
        assert!(!same_content(
            &known,
            &entry(EntryKind::Symlink, Some(5), Some(9), None)
        ));
        let counted = SubtreeFacts {
            files: 2,
            bytes: 7,
            ..SubtreeFacts::default()
        };
        let folder = entry(EntryKind::Folder, None, None, Some(counted));
        assert!(same_content(&folder, &entry(EntryKind::Folder, None, None, None)));
        let other = SubtreeFacts { files: 3, ..counted };
        assert!(!same_content(
            &folder,
            &entry(EntryKind::Folder, None, None, Some(other))
        ));
    }
}
