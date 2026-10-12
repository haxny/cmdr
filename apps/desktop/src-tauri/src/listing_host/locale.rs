//! The app's reading language, answering which language names collate in.

use cmdr_listing::host::ReadingLocale;

/// Answers `intl::active_locale`, which re-resolves on an OS locale change, so
/// the next sort after a language switch collates in the new language.
pub(crate) struct AppLocale;

impl ReadingLocale for AppLocale {
    fn locale(&self) -> String {
        crate::intl::active_locale()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The engine pulls the language per sort, so it follows the user's
    /// setting the moment it changes, with nothing pushed. The order a language
    /// gives is `collation_test`'s, against a fixed locale.
    #[test]
    fn the_reading_language_follows_the_users_setting() {
        let _lock = crate::intl::native_strings::lock_active_locale_for_tests();

        let _ = crate::intl::set_language_preference(Some("sv".to_string()));
        assert_eq!(AppLocale.locale(), "sv");
        let _ = crate::intl::set_language_preference(Some("de".to_string()));
        assert_eq!(AppLocale.locale(), "de");

        // Back to following the OS, so nothing after this sees a pinned language.
        let _ = crate::intl::set_language_preference(None);
    }
}
