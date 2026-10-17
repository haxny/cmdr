//! The collator the panes sort names with, so a transfer's tree lists its
//! items in the order the person reads them in their panes. Cmdr's answer is
//! `operations_host::name_order`, the listing engine's collator for the current
//! language.

use std::sync::{Arc, OnceLock};

use cmdr_collation::NameCollator;

/// Where the engine gets its name order.
pub trait NameOrder: Send + Sync {
    /// The collator for the language the person reads now. Asked once per walk
    /// or tree, ❌ never per name: building one loads ICU data.
    fn collator(&self) -> Arc<NameCollator>;
}

/// The answer for a host with no panes: English collation, built on first use
/// and kept.
pub(super) struct EnglishNameOrder {
    collator: OnceLock<Arc<NameCollator>>,
}

impl EnglishNameOrder {
    pub(super) fn new() -> Self {
        Self {
            collator: OnceLock::new(),
        }
    }
}

impl NameOrder for EnglishNameOrder {
    fn collator(&self) -> Arc<NameCollator> {
        Arc::clone(self.collator.get_or_init(|| Arc::new(NameCollator::new("en"))))
    }
}
