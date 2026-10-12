//! The app's reading language, answering which language names collate in.

use crate::file_system::listing::host::ReadingLocale;

/// Answers `intl::active_locale`, which re-resolves on an OS locale change, so
/// the next sort after a language switch collates in the new language.
pub(crate) struct AppLocale;

impl ReadingLocale for AppLocale {
    fn locale(&self) -> String {
        crate::intl::active_locale()
    }
}
