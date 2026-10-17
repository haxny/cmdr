//! The write engine's name order, answered by the listing engine: the collator
//! the panes sort with, for the language the person reads now.

use std::sync::Arc;

use cmdr_collation::NameCollator;

use cmdr_operations::host::NameOrder;

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

    /// The engine's own tests build their local volumes on
    /// `host::testing::pane_order_host`, and its walkers take a listing's order
    /// as is, so that host must list a directory in the order the app's host
    /// does, or a test's scenario would quietly run in another order than
    /// production's.
    #[test]
    fn the_engine_test_host_lists_in_the_order_the_apps_host_does() {
        use cmdr_fs::entry::FileEntry;
        use cmdr_fs::volume::host::listings::ListingHost;

        let _lock = crate::intl::native_strings::lock_active_locale_for_tests();
        let _ = crate::intl::set_language_preference(Some("en".to_string()));

        let names = [
            ("file10.txt", false),
            ("File2.txt", false),
            ("beta", true),
            ("Alpha", true),
            ("äpple.txt", false),
            ("zeta", false),
            (".hidden", false),
            ("a b", true),
            ("a-b", false),
        ];
        let entries: Vec<FileEntry> = names
            .iter()
            .map(|(name, is_dir)| FileEntry::new(name.to_string(), format!("/x/{name}"), *is_dir, false))
            .collect();

        let mut by_app = entries.clone();
        crate::listing_host::backends::AppListings.sort_listing(&mut by_app);
        let mut by_test_host = entries;
        cmdr_operations::host::testing::pane_order_host()
            .listings()
            .sort_listing(&mut by_test_host);

        let _ = crate::intl::set_language_preference(None);
        let order = |entries: &[FileEntry]| entries.iter().map(|e| e.name.clone()).collect::<Vec<_>>();
        assert_eq!(order(&by_test_host), order(&by_app));
    }
}
