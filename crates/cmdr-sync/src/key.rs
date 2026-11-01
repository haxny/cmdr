//! How names pair up across two volumes.

use cmdr_fs::name_fold::{composed, fold_name};

/// Which spellings of a name count as one. Unicode form never matters (NFC).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameRule {
    /// Case folds too: at least one side is case-insensitive.
    FoldCase,
    /// Case matters: both sides are case-sensitive.
    ExactCase,
}

impl NameRule {
    /// The rule for a pair: `FoldCase` when either side is case-insensitive.
    pub fn for_pair(left: &crate::SyncCaps, right: &crate::SyncCaps) -> Self {
        if left.case_sensitive && right.case_sensitive {
            Self::ExactCase
        } else {
            Self::FoldCase
        }
    }

    /// The pairing key of one name component.
    pub fn key(self, name: &str) -> String {
        match self {
            Self::FoldCase => fold_name(name).into_owned(),
            Self::ExactCase => composed(name).into_owned(),
        }
    }
}

/// A path inside a sync root, as name keys from the root down. Orders parents
/// before their children.
#[derive(Ord, PartialOrd, Eq, PartialEq, Clone, Hash, Debug)]
pub enum EntryKey {
    /// Pairs by name key, one per path component.
    Name(Vec<String>),
}

impl EntryKey {
    /// The sync root itself.
    pub fn root() -> Self {
        Self::Name(Vec::new())
    }

    /// The key of a child called `component` (already a name key).
    pub fn child(&self, component: String) -> Self {
        let Self::Name(parts) = self;
        let mut next = Vec::with_capacity(parts.len() + 1);
        next.extend(parts.iter().cloned());
        next.push(component);
        Self::Name(next)
    }

    /// The containing folder's key, `None` for the root.
    pub fn parent(&self) -> Option<Self> {
        let Self::Name(parts) = self;
        let (_, rest) = parts.split_last()?;
        Some(Self::Name(rest.to_vec()))
    }

    /// Whether this key is `ancestor` or lies anywhere beneath it.
    pub fn is_within(&self, ancestor: &Self) -> bool {
        let (Self::Name(own), Self::Name(other)) = (self, ancestor);
        own.starts_with(other)
    }
}
