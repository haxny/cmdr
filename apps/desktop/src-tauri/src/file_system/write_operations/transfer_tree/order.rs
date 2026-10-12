//! Tree order: the one order a transfer's tree shows its selection in.
//!
//! Folders first, then files, each group by the panes' name collation
//! (`cmdr_listing::NameCollator`), depth-first. The tree displays exactly this order,
//! so the rows below the active one really are the upcoming ones, because every
//! driver starts files in it too (`../transfer/DETAILS.md` § "Processing order").

use std::cmp::Ordering;
use std::sync::Arc;

use cmdr_listing::NameCollator;

/// Ranks sibling entries in tree order.
pub(crate) struct TreeOrder {
    /// `None` ranks names by raw bytes, which only the budget cells ask for
    /// (`bytewise`): a million collation keys in an unoptimized build is the
    /// whole test budget, and the arena's size doesn't depend on the order.
    collator: Option<Arc<NameCollator>>,
}

impl TreeOrder {
    /// Tree order for the language the user reads now, the same collator the
    /// panes sort with.
    pub(crate) fn active() -> Self {
        Self {
            collator: Some(crate::listing_host::listings().collator()),
        }
    }

    /// Tree order under a given collator, for a test that pins a locale.
    #[cfg(test)]
    pub(crate) fn with_collator(collator: Arc<NameCollator>) -> Self {
        Self {
            collator: Some(collator),
        }
    }

    /// Folders first, then names by raw bytes: the shape of tree order without
    /// its cost, for the size and speed budgets.
    #[cfg(test)]
    pub(crate) fn bytewise() -> Self {
        Self { collator: None }
    }

    /// Ranks two siblings: a folder before any file, then by name, with the
    /// collator's raw-bytes tiebreak so two distinct names never tie.
    pub(crate) fn compare(&self, a_is_folder: bool, a_name: &str, b_is_folder: bool, b_name: &str) -> Ordering {
        b_is_folder.cmp(&a_is_folder).then_with(|| match &self.collator {
            Some(collator) => collator.compare(a_name, b_name),
            None => a_name.as_bytes().cmp(b_name.as_bytes()),
        })
    }

    /// Sorts `items` in tree order. `facts` says whether an item is a folder
    /// and what it's called. Builds one collation key per item, then compares
    /// keys: a bulk sort over a 10,000-file folder would otherwise run the
    /// collator ~130,000 times.
    pub(crate) fn sort<T>(&self, items: &mut [T], facts: impl Fn(&T) -> (bool, &str)) {
        if items.len() < 2 {
            return;
        }
        let Some(collator) = &self.collator else {
            items.sort_by(|a, b| {
                let (a_is_folder, a_name) = facts(a);
                let (b_is_folder, b_name) = facts(b);
                self.compare(a_is_folder, a_name, b_is_folder, b_name)
            });
            return;
        };
        let mut keyed: Vec<(bool, _, usize)> = items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let (is_folder, name) = facts(item);
                (is_folder, collator.key(name), index)
            })
            .collect();
        keyed.sort_by(|a, b| {
            b.0.cmp(&a.0).then_with(|| a.1.compare(&b.1)).then_with(|| {
                let a_name = facts(&items[a.2]).1;
                let b_name = facts(&items[b.2]).1;
                a_name.as_bytes().cmp(b_name.as_bytes())
            })
        });
        let order: Vec<usize> = keyed.into_iter().map(|(_, _, index)| index).collect();
        apply_permutation(items, order);
    }
}

/// Reorders `items` so position `i` holds what was at `order[i]`, in place,
/// following each cycle once.
fn apply_permutation<T>(items: &mut [T], mut order: Vec<usize>) {
    for start in 0..items.len() {
        let mut current = start;
        while order[current] != start {
            let next = order[current];
            items.swap(current, next);
            order[current] = current;
            current = next;
        }
        order[current] = current;
    }
}

#[cfg(test)]
#[path = "order_tests.rs"]
mod tests;
