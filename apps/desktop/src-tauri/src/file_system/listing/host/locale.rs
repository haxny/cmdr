//! Which language names collate in.

/// Answers the reading language, pulled once per sort.
pub trait ReadingLocale: Send + Sync {
    /// A BCP 47 tag (`"en"`, `"hu"`).
    fn locale(&self) -> String;
}

/// No language preference: English.
pub struct NoLocale;

impl ReadingLocale for NoLocale {
    fn locale(&self) -> String {
        "en".to_string()
    }
}
