//! Moving a held file's unwritten edits onto the disk state after something else rewrote the file
//! (an undo of a write): what the user didn't edit follows the disk, what they edited stays.

use std::collections::{BTreeMap, BTreeSet};

use cmdr_tags::FileTags;

use super::session::Entry;
use super::view::CoverKeys;

/// `held` takes the file's new disk state `now`, keeping its unwritten edits.
pub(super) fn follow_disk(held: &mut Entry, now: &Entry) {
    match (held, now) {
        (Entry::Loaded(held), Entry::Loaded(now)) => {
            let mut keys = CoverKeys::default();
            keys.remember(&held.edited.cover, held.cover_key);
            keys.remember(&now.edited.cover, now.cover_key);
            held.edited = rebase(&held.disk, &held.edited, &now.disk);
            held.cover_key = keys.key_of(&held.edited.cover);
            held.disk = now.disk.clone();
            held.fingerprint = now.fingerprint.clone();
            held.duration_ms = now.duration_ms;
        }
        (held, now) => *held = now.clone(),
    }
}

/// `edited` moved from `old` onto `now`: each field, the cover, and each extra the user left as
/// `old` had it takes `now`'s value; one the user changed keeps the edit.
pub(super) fn rebase(old: &FileTags, edited: &FileTags, now: &FileTags) -> FileTags {
    FileTags {
        fields: rebase_map(&old.fields, &edited.fields, &now.fields),
        cover: if edited.cover == old.cover {
            now.cover.clone()
        } else {
            edited.cover.clone()
        },
        extras: rebase_map(&old.extras, &edited.extras, &now.extras),
    }
}

fn rebase_map<K: Ord + Clone, V: PartialEq + Clone>(
    old: &BTreeMap<K, V>,
    edited: &BTreeMap<K, V>,
    now: &BTreeMap<K, V>,
) -> BTreeMap<K, V> {
    let keys: BTreeSet<&K> = old.keys().chain(edited.keys()).chain(now.keys()).collect();
    keys.into_iter()
        .filter_map(|key| {
            let value = if edited.get(key) == old.get(key) {
                now.get(key)
            } else {
                edited.get(key)
            };
            value.map(|value| (key.clone(), value.clone()))
        })
        .collect()
}
