//! The marker table for user-derived values in log lines: one opener per kind, one closer.
//!
//! A log site that knows a value is a path, a name, a volume label, a host, or a user wraps it
//! between its kind's opener and [`CLOSER`], so the error reporter hashes exactly that span
//! instead of guessing where a name ends. The markers are Unicode noncharacters from
//! [`RESERVED`]: invisible in most viewers, never typed by a person, and still valid scalar
//! values, so a real file name CAN hold one. The reader of a marked line (the app's `redact`
//! module) treats every char of the range as structure and fails closed on anything it can't
//! pair. This table is the single source of truth both sides read; nothing else lists the code
//! points.

use std::ops::RangeInclusive;

/// Every char a marked line may use as structure. A producer replaces any of them it finds in
/// text it didn't mark, so only its own markers are ever legitimate.
pub const RESERVED: RangeInclusive<char> = '\u{FDD0}'..='\u{FDEF}';

/// Ends a span of any kind.
pub const CLOSER: char = '\u{FDD1}';

/// The top of [`RESERVED`], never an opener or the closer: a reader may use these chars for its
/// own bookkeeping while it rewrites a line it has already consumed every marker from.
pub const READER_SCRATCH: RangeInclusive<char> = '\u{FDE0}'..='\u{FDEF}';

/// What a marked value is. The producer's kind decides how a report redacts it: a file path
/// keeps its extension, a dir path never gets one, and an identity gets its own token domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LogKind {
    /// A path whose leaf kind the site doesn't know.
    Path,
    /// A path to a file.
    FilePath,
    /// A path to a directory.
    DirPath,
    /// A file or folder name the site doesn't know the kind of.
    Name,
    /// A file name.
    FileName,
    /// A folder name.
    DirName,
    /// A volume label or a share name.
    Volume,
    /// A host: a server, a Bonjour instance, a device on the network.
    Host,
    /// An account name.
    User,
}

impl LogKind {
    /// Every kind, in table order.
    pub const ALL: [LogKind; 9] = [
        LogKind::Path,
        LogKind::FilePath,
        LogKind::DirPath,
        LogKind::Name,
        LogKind::FileName,
        LogKind::DirName,
        LogKind::Volume,
        LogKind::Host,
        LogKind::User,
    ];

    /// The char that opens a span of this kind.
    pub const fn opener(self) -> char {
        match self {
            LogKind::Path => '\u{FDD0}',
            LogKind::FilePath => '\u{FDD2}',
            LogKind::DirPath => '\u{FDD3}',
            LogKind::Name => '\u{FDD4}',
            LogKind::FileName => '\u{FDD5}',
            LogKind::DirName => '\u{FDD6}',
            LogKind::Volume => '\u{FDD7}',
            LogKind::Host => '\u{FDD8}',
            LogKind::User => '\u{FDD9}',
        }
    }

    /// The kind `c` opens, or `None` when it opens nothing (the closer, an unassigned reserved
    /// char, or any other char).
    pub fn from_opener(c: char) -> Option<LogKind> {
        LogKind::ALL.into_iter().find(|kind| kind.opener() == c)
    }
}

/// Whether `c` is in [`RESERVED`].
pub fn is_reserved(c: char) -> bool {
    RESERVED.contains(&c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_marker_is_a_distinct_reserved_char_outside_the_reader_scratch() {
        let mut seen = HashSet::new();
        for c in LogKind::ALL.iter().map(|kind| kind.opener()).chain([CLOSER]) {
            assert!(is_reserved(c), "{c:?}");
            assert!(!READER_SCRATCH.contains(&c), "{c:?}");
            assert!(seen.insert(c), "{c:?} used twice");
        }
        assert!(READER_SCRATCH.start() >= RESERVED.start() && READER_SCRATCH.end() <= RESERVED.end());
    }

    #[test]
    fn from_opener_inverts_opener_and_rejects_everything_else() {
        for kind in LogKind::ALL {
            assert_eq!(LogKind::from_opener(kind.opener()), Some(kind));
        }
        for c in [CLOSER, *READER_SCRATCH.start(), *RESERVED.end(), 'a', '\u{FFFD}'] {
            assert_eq!(LogKind::from_opener(c), None, "{c:?}");
        }
    }

    /// The reader recognizes a marker that went through `{:?}` by its escaped spelling, which
    /// is Rust's, in lowercase hex (verified on rustc 1.99.0, 2026-10-19).
    #[test]
    fn debug_formatting_escapes_every_reserved_char_in_lowercase_hex() {
        for c in RESERVED {
            let escaped = format!("{:?}", c.to_string());
            assert_eq!(escaped, format!("\"\\u{{{:x}}}\"", c as u32), "{c:?}");
        }
    }
}
