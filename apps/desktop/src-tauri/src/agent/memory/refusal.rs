//! Why a memory operation was turned down.
//!
//! Its own module so the jail and the store can both name it without depending on each other.

/// Why a memory operation was turned down.
///
/// Typed, and every variant reaches the model as a machine-readable token beside its sentence:
/// a refusal it has to parse out of prose is a refusal it will misread on the first copy edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryRefusal {
    /// The path pointed outside the memory folder: absolute, a `..`, or a symlink.
    OutsideMemory,
    /// Memory holds Markdown and nothing else.
    NotMarkdown,
    /// The path was blank, or named nothing after the `.` segments came out.
    NoPath,
    /// The folder is at its cap. ❌ Never a silent failure: the model is told to prune.
    DirectoryFull { used: u64, cap: u64, wanted: u64 },
    /// `memory_edit` on a file that isn't there.
    NoSuchFile,
    /// `memory_edit` whose `oldString` appears nowhere in the file.
    NoMatch,
    /// `memory_edit` whose `oldString` appears more than once. Guessing which one the model
    /// meant would rewrite a line it never looked at, in a file that rides every later turn.
    NotUnique { matches: usize },
    /// The disk said no. The detail is for the model to relay, never to branch on.
    Unwritable(String),
}

impl MemoryRefusal {
    /// The stable token the tool result carries. What anything downstream matches on.
    pub fn token(&self) -> &'static str {
        self.kind().token()
    }

    /// The fieldless kind, for a reader that has only the stored token (the rail's tool line).
    /// Never [`MemoryRefusalKind::Unavailable`] or [`MemoryRefusalKind::Other`]: those name
    /// what no `MemoryRefusal` can be.
    pub fn kind(&self) -> MemoryRefusalKind {
        match self {
            MemoryRefusal::OutsideMemory => MemoryRefusalKind::OutsideMemory,
            MemoryRefusal::NotMarkdown => MemoryRefusalKind::NotMarkdown,
            MemoryRefusal::NoPath => MemoryRefusalKind::NoPath,
            MemoryRefusal::DirectoryFull { .. } => MemoryRefusalKind::DirectoryFull,
            MemoryRefusal::NoSuchFile => MemoryRefusalKind::NoSuchFile,
            MemoryRefusal::NoMatch => MemoryRefusalKind::NoMatch,
            MemoryRefusal::NotUnique { .. } => MemoryRefusalKind::NotUnique,
            MemoryRefusal::Unwritable(_) => MemoryRefusalKind::Unwritable,
        }
    }
}

/// [`MemoryRefusal`] without its data, plus the two refusals a stored result can carry that
/// no `MemoryRefusal` is: the folder being unreachable, and a token this build doesn't know.
///
/// The token is the contract: a tool result stores `refused: <token>`, and
/// [`from_token`](Self::from_token) reads it back, so the rail can say which refusal it was
/// without reading the model-facing sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum MemoryRefusalKind {
    OutsideMemory,
    NotMarkdown,
    NoPath,
    DirectoryFull,
    NoSuchFile,
    NoMatch,
    NotUnique,
    Unwritable,
    /// The memory folder couldn't be reached this session (the data dir didn't resolve).
    Unavailable,
    /// A token this build doesn't know (a newer build wrote it).
    Other,
}

impl MemoryRefusalKind {
    /// Every kind, so a test can round-trip them all.
    pub const ALL: [MemoryRefusalKind; 10] = [
        MemoryRefusalKind::OutsideMemory,
        MemoryRefusalKind::NotMarkdown,
        MemoryRefusalKind::NoPath,
        MemoryRefusalKind::DirectoryFull,
        MemoryRefusalKind::NoSuchFile,
        MemoryRefusalKind::NoMatch,
        MemoryRefusalKind::NotUnique,
        MemoryRefusalKind::Unwritable,
        MemoryRefusalKind::Unavailable,
        MemoryRefusalKind::Other,
    ];

    /// The stable token a tool result carries as `refused`.
    pub fn token(self) -> &'static str {
        match self {
            MemoryRefusalKind::OutsideMemory => "outsideMemory",
            MemoryRefusalKind::NotMarkdown => "notMarkdown",
            MemoryRefusalKind::NoPath => "noPath",
            MemoryRefusalKind::DirectoryFull => "memoryFull",
            MemoryRefusalKind::NoSuchFile => "noSuchFile",
            MemoryRefusalKind::NoMatch => "noMatch",
            MemoryRefusalKind::NotUnique => "notUnique",
            MemoryRefusalKind::Unwritable => "unwritable",
            MemoryRefusalKind::Unavailable => "unavailable",
            MemoryRefusalKind::Other => "other",
        }
    }

    /// The kind a stored token names. Total: an unknown token is [`MemoryRefusalKind::Other`].
    pub fn from_token(token: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|kind| *kind != MemoryRefusalKind::Other && kind.token() == token)
            .unwrap_or(MemoryRefusalKind::Other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_round_trips_through_its_token() {
        for kind in MemoryRefusalKind::ALL {
            if kind == MemoryRefusalKind::Other {
                continue;
            }
            assert_eq!(MemoryRefusalKind::from_token(kind.token()), kind, "{kind:?}");
        }
    }

    #[test]
    fn an_unknown_token_reads_as_other() {
        assert_eq!(MemoryRefusalKind::from_token("fromTheFuture"), MemoryRefusalKind::Other);
        assert_eq!(MemoryRefusalKind::from_token("other"), MemoryRefusalKind::Other);
    }

    #[test]
    fn every_refusal_keeps_its_token_and_reads_back_as_its_own_kind() {
        let refusals = [
            (MemoryRefusal::OutsideMemory, "outsideMemory"),
            (MemoryRefusal::NotMarkdown, "notMarkdown"),
            (MemoryRefusal::NoPath, "noPath"),
            (
                MemoryRefusal::DirectoryFull {
                    used: 1,
                    cap: 2,
                    wanted: 3,
                },
                "memoryFull",
            ),
            (MemoryRefusal::NoSuchFile, "noSuchFile"),
            (MemoryRefusal::NoMatch, "noMatch"),
            (MemoryRefusal::NotUnique { matches: 2 }, "notUnique"),
            (MemoryRefusal::Unwritable("disk".into()), "unwritable"),
        ];
        for (refusal, token) in refusals {
            assert_eq!(refusal.token(), token, "the stored token never changes: {refusal:?}");
            assert_eq!(MemoryRefusalKind::from_token(refusal.token()), refusal.kind());
            assert!(!matches!(
                refusal.kind(),
                MemoryRefusalKind::Unavailable | MemoryRefusalKind::Other
            ));
        }
    }
}
