//! What a queue row says an operation acts on: the summary a starter hands the
//! manager, and the items that summary spells out. Both ride the thin
//! `operations-changed` snapshot, so they're vocabulary, not manager state.

use super::ItemKinds;

/// Best-effort human-readable source/destination summary for the queue window.
// DEFAULT-OK: both sides `None` means "couldn't summarize", and the queue window renders
// that absence rather than an empty string.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub struct OperationSummaryText {
    pub source: Option<String>,
    pub destination: Option<String>,
    /// What the operation acts on, spelled out for the queue row. `None` where
    /// the starter has no top-level items to name (a rename, a new folder, an
    /// edit inside an archive): the row falls back to the two names above.
    pub items: Option<OperationItems>,
}

/// The items an operation acts on and where they go, for the queue row's
/// "Copying 2 folders and 1 file Dropbox → cmdr-s3-test/target/folder".
///
/// Static for the op's whole life and bounded (at most
/// [`ITEM_NAMES_SHOWN`](super::super::summary::ITEM_NAMES_SHOWN) names), which
/// is why it rides the thin snapshot: it changes only when membership does.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OperationItems {
    /// How many top-level items, files and folders together.
    pub count: u32,
    /// The same items split by kind, when the starter said; `None` reads as
    /// "3 items". Dropped when it doesn't add up to `count`.
    pub kinds: Option<ItemKinds>,
    /// The first top-level names, in order, for the row's tooltip. The rest is
    /// `count` minus these.
    pub names: Vec<String>,
    /// The folder they come from, as a person reads it (`~/Dropbox`,
    /// `cmdr-s3-test/photos`). `None` when the sources share no parent.
    pub from: Option<String>,
    /// The folder they go to, in the same form. `None` for a delete or trash.
    pub to: Option<String>,
}
