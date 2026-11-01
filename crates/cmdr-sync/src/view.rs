//! The grid's view of a plan: show-filters, folder header rows, and the
//! summary bar. Hiding a row never changes its action.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Action, Category, EntryKey, EntryKind, Plan, PlanItem, Side};

/// The show-filter toggles (TC's set plus `?` and warnings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ShowFilters {
    /// `→` rows.
    pub to_right: bool,
    /// `=` rows (off by default).
    pub equal: bool,
    /// `≠` rows: skipped rows that differ.
    pub different: bool,
    /// `←` rows, removals on either side included.
    pub to_left: bool,
    /// Rows with both sides present.
    pub duplicates: bool,
    /// Rows with one side present.
    pub singles: bool,
    /// `?` rows.
    pub unknown: bool,
    /// Only rows that carry a warning.
    pub warnings_only: bool,
}

impl Default for ShowFilters {
    fn default() -> Self {
        Self {
            to_right: true,
            equal: false,
            different: true,
            to_left: true,
            duplicates: true,
            singles: true,
            unknown: true,
            warnings_only: false,
        }
    }
}

/// Which show-filter a row answers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RowClass {
    /// Copies to the right.
    ToRight,
    /// Copies to the left, or removes on either side.
    ToLeft,
    /// Equal.
    Equal,
    /// Skipped, and not unknown.
    Different,
    /// Undecided, or skipped because nothing could tell.
    Unknown,
}

/// The class of a row, from its action.
pub fn row_class(item: &PlanItem) -> RowClass {
    match item.action {
        Action::CopyRight => RowClass::ToRight,
        Action::CopyLeft | Action::RemoveRight | Action::RemoveLeft => RowClass::ToLeft,
        Action::Equal => RowClass::Equal,
        Action::Undecided => RowClass::Unknown,
        Action::Skip if item.verdict.category == Category::Unknown => RowClass::Unknown,
        Action::Skip => RowClass::Different,
    }
}

/// One grid row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewRow {
    /// A folder header ("folder `ordinal` of `of`"), above that folder's rows.
    /// The sync root's own rows come first, with no header.
    Folder {
        /// The folder's key.
        key: EntryKey,
        /// Its path from the root, in the left spelling when there is one.
        rel_dir: String,
        /// Its position among the visible headers, from 1.
        ordinal: u32,
        /// How many headers are visible.
        of: u32,
    },
    /// A plan row.
    Item {
        /// Its `PlanItem::index`.
        index: u32,
    },
}

fn class_shown(class: RowClass, filters: &ShowFilters) -> bool {
    match class {
        RowClass::ToRight => filters.to_right,
        RowClass::ToLeft => filters.to_left,
        RowClass::Equal => filters.equal,
        RowClass::Different => filters.different,
        RowClass::Unknown => filters.unknown,
    }
}

/// Whether the filters show this row.
pub fn is_visible(item: &PlanItem, filters: &ShowFilters) -> bool {
    let both = item.left.is_some() && item.right.is_some();
    let presence = if both { filters.duplicates } else { filters.singles };
    let warning = !filters.warnings_only || item.older_overwrites_newer;
    class_shown(row_class(item), filters) && presence && warning
}

/// The row's path, in the left spelling when there is one.
fn display_path(item: &PlanItem) -> String {
    match item.left.as_ref().or(item.right.as_ref()) {
        Some(entry) => entry.rel_path.clone(),
        None => {
            let EntryKey::Name(parts) = &item.key;
            parts.join("/")
        }
    }
}

fn parent_dir(path: &str) -> String {
    path.rsplit_once('/')
        .map_or_else(String::new, |(dir, _)| dir.to_owned())
}

/// The visible rows grouped under folder headers: root rows first, then each
/// folder in key order.
pub fn view_rows(plan: &Plan, filters: &ShowFilters) -> Vec<ViewRow> {
    let mut groups: BTreeMap<EntryKey, Vec<&PlanItem>> = BTreeMap::new();
    for item in plan.items.iter().filter(|item| is_visible(item, filters)) {
        let parent = item.key.parent().unwrap_or_else(EntryKey::root);
        groups.entry(parent).or_default().push(item);
    }
    let root = EntryKey::root();
    let of = groups.keys().filter(|key| **key != root).count() as u32;
    let mut ordinal = 0;
    let mut rows = Vec::with_capacity(plan.items.len() + groups.len());
    for (key, items) in groups {
        if key != root {
            ordinal += 1;
            let rel_dir = parent_dir(&display_path(items[0]));
            rows.push(ViewRow::Folder {
                key,
                rel_dir,
                ordinal,
                of,
            });
        }
        rows.extend(items.iter().map(|item| ViewRow::Item { index: item.index }));
    }
    rows
}

/// Copies and removals headed into one side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SideTotals {
    /// New files copied into this side (nothing there yet).
    pub copy_files: u64,
    /// Their bytes.
    pub copy_bytes: u64,
    /// Files copied over an existing file on this side; the old one is kept
    /// (Trash or versions folder), so the confirm says where it goes.
    pub replace_files: u64,
    /// The incoming bytes of those replacements.
    pub replace_bytes: u64,
    /// Files removed from this side (a folder row counts the files in it).
    pub removals: u64,
    /// Folders removed from this side, empty ones included.
    pub removed_folders: u64,
}

/// The summary bar's numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanSummary {
    /// What lands on, or leaves, the left.
    pub into_left: SideTotals,
    /// What lands on, or leaves, the right.
    pub into_right: SideTotals,
    /// Rows skipped (not counting unknown ones).
    pub skipped: u64,
    /// Rows nobody could decide (`?`).
    pub undecided: u64,
    /// Rows whose sides are equal.
    pub equal: u64,
    /// Rows present on both sides that aren't equal.
    pub different: u64,
    /// Rows only on the left.
    pub unique_left: u64,
    /// Rows only on the right.
    pub unique_right: u64,
    /// Name clash, type clash, and unreadable rows: no comparison was possible.
    pub not_comparable: u64,
    /// Rows that act but are hidden by the filters.
    pub hidden_actions: u64,
    /// Rows whose action you changed.
    pub changed_by_you: u64,
}

fn add_action(summary: &mut PlanSummary, item: &PlanItem) {
    let Some(target) = item.action.target() else {
        return;
    };
    let totals = match target {
        Side::Left => &mut summary.into_left,
        Side::Right => &mut summary.into_right,
    };
    let is_folder = |side| {
        item.entry(side)
            .is_some_and(|entry| entry.facts.kind == EntryKind::Folder)
    };
    if item.action.is_removal() {
        totals.removals += item.files_on(target);
        totals.removed_folders += u64::from(is_folder(target));
    } else if let Some(source) = item.action.needs() {
        if item.entry(target).is_some() {
            totals.replace_files += item.files_on(source);
            totals.replace_bytes += item.bytes;
        } else {
            totals.copy_files += item.files_on(source);
            totals.copy_bytes += item.bytes;
        }
    }
}

fn add_presence(summary: &mut PlanSummary, item: &PlanItem) {
    let comparable = !matches!(
        item.verdict.category,
        Category::NameClash | Category::TypeClash | Category::Unreadable
    );
    if !comparable {
        summary.not_comparable += 1;
        return;
    }
    match (&item.left, &item.right) {
        (Some(_), Some(_)) if item.verdict.category == Category::Equal => summary.equal += 1,
        (Some(_), Some(_)) => summary.different += 1,
        (Some(_), None) => summary.unique_left += 1,
        (None, Some(_)) => summary.unique_right += 1,
        (None, None) => {}
    }
}

/// The summary bar for a plan under `filters`. Hidden rows still count.
pub fn summarize(plan: &Plan, filters: &ShowFilters) -> PlanSummary {
    let mut summary = PlanSummary::default();
    for item in &plan.items {
        add_action(&mut summary, item);
        add_presence(&mut summary, item);
        match row_class(item) {
            RowClass::Unknown => summary.undecided += 1,
            RowClass::Different => summary.skipped += 1,
            RowClass::ToRight | RowClass::ToLeft | RowClass::Equal => {}
        }
        let acts = item.action.target().is_some();
        summary.hidden_actions += u64::from(acts && !is_visible(item, filters));
        summary.changed_by_you += u64::from(item.changed_by_you());
    }
    summary
}
