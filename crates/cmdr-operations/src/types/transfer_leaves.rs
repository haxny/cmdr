//! Where each top-level source of a copy or move lands, by name.

use std::ffi::OsStr;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// The name each top-level source of a copy or move takes inside the
/// destination folder: its own, or, for a single source, the leaf its caller
/// chose. The destination argument still names the parent.
///
/// Every route asks [`Self::leaf_of`] (or [`Self::chosen_for`] where another
/// rename may also apply), so no route can honor a name another forgets. The
/// routed start validates it once, before the operation is admitted
/// (`validation::validate_transfer_leaves`), and nothing downstream checks it
/// again. On the wire it is the optional `destinationName`, a single leaf.
/// `transfer/DETAILS.md` § "Named destinations".
// DEFAULT-OK: every source keeps its own name, which is what a transfer that
// named nothing asks for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Option<String>", into = "Option<String>")]
pub struct TransferLeaves {
    /// The leaf the ONE source lands as; `None` keeps every name.
    single: Option<String>,
}

impl TransferLeaves {
    /// The one source lands as `leaf`.
    pub fn single(leaf: impl Into<String>) -> Self {
        Self {
            single: Some(leaf.into()),
        }
    }

    /// The single-source leaf, for the validation that refuses it on a batch.
    pub(crate) fn single_leaf(&self) -> Option<&str> {
        self.single.as_deref()
    }

    /// The name `source` lands as when its caller chose one; `None` when it
    /// keeps its own.
    pub(crate) fn chosen_for(&self, _source: &Path) -> Option<&OsStr> {
        self.single.as_deref().map(OsStr::new)
    }

    /// The name `source` lands as: the chosen one, else its own. `None` only
    /// for a path with no final component, which no route transfers by name.
    pub(crate) fn leaf_of<'a>(&'a self, source: &'a Path) -> Option<&'a OsStr> {
        let own = source.file_name()?;
        Some(self.chosen_for(source).unwrap_or(own))
    }
}

// The bindings describe exactly what serde reads and writes: one optional leaf.
impl specta::Type for TransferLeaves {
    fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
        <Option<String> as specta::Type>::definition(types)
    }
}

impl From<Option<String>> for TransferLeaves {
    fn from(single: Option<String>) -> Self {
        Self { single }
    }
}

impl From<TransferLeaves> for Option<String> {
    fn from(leaves: TransferLeaves) -> Self {
        leaves.single
    }
}

#[cfg(test)]
#[path = "transfer_leaves_wire_tests.rs"]
mod wire_tests;
