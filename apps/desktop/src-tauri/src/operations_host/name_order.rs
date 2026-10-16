//! The write engine's name order, answered by the listing engine: the collator
//! the panes sort with, for the language the person reads now.

use std::sync::Arc;

use cmdr_collation::NameCollator;

use crate::file_system::write_operations::host::NameOrder;

/// `listings().collator()`, asked per call: the language can change between
/// two transfers, and the listing engine keeps one collator per language.
pub(super) struct AppNameOrder;

impl NameOrder for AppNameOrder {
    fn collator(&self) -> Arc<NameCollator> {
        crate::listing_host::listings().collator()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The engine orders names with the very collator the panes sort with.
    #[test]
    fn the_engine_sorts_with_the_panes_collator() {
        // Warm the listing engine's per-language cache first, so a sibling's
        // concurrent first ask can't swap the entry between the two reads.
        let _ = crate::listing_host::listings().collator();
        let panes = crate::listing_host::listings().collator();
        assert!(Arc::ptr_eq(&AppNameOrder.collator(), &panes));
    }
}
