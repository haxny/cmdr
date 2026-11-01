//! Finding a row of the grid's view without paging it to the dialog: the next
//! or previous folder (⌘PageDown, ⌘PageUp) and type-to-jump by name.

use serde::{Deserialize, Serialize};

use crate::{Plan, PlanItem, ViewRow};

/// What to look for, from view row `from` (the cursor).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RowQuery {
    /// The first row of the next folder below `from`.
    NextFolder {
        /// The cursor's view row.
        from: u32,
    },
    /// The first row of the folder before the one `from` is in (the root's rows count as the first folder).
    PreviousFolder {
        /// The cursor's view row.
        from: u32,
    },
    /// The next item row, from `from` on and wrapping, whose name on either side starts with `prefix`
    /// (ignoring case).
    Name {
        /// Where the search starts (inclusive).
        from: u32,
        /// What the person typed.
        prefix: String,
    },
}

fn is_header(row: &ViewRow) -> bool {
    matches!(row, ViewRow::Folder { .. })
}

/// The first item row of the folder whose header sits at `header`.
fn first_row_after(rows: &[ViewRow], header: usize) -> Option<u32> {
    let at = header + 1;
    (at < rows.len() && !is_header(&rows[at])).then(|| u32::try_from(at).unwrap_or(u32::MAX))
}

fn next_folder(rows: &[ViewRow], from: usize) -> Option<u32> {
    let header = (from + 1..rows.len()).find(|&index| is_header(&rows[index]))?;
    first_row_after(rows, header)
}

fn previous_folder(rows: &[ViewRow], from: usize) -> Option<u32> {
    let own = (0..from.min(rows.len())).rev().find(|&index| is_header(&rows[index]))?;
    match (0..own).rev().find(|&index| is_header(&rows[index])) {
        Some(previous) => first_row_after(rows, previous),
        // Above the first header are the root's own rows, if any.
        None => (own > 0).then_some(0),
    }
}

fn name_starts(item: &PlanItem, prefix: &str) -> bool {
    [item.left.as_ref(), item.right.as_ref()]
        .into_iter()
        .flatten()
        .any(|entry| {
            let name = entry.rel_path.rsplit('/').next().unwrap_or(&entry.rel_path);
            // allowed-error-string-match: a file name against what the person typed (type-to-jump), not an error or state
            name.to_lowercase().starts_with(prefix)
        })
}

fn by_name(plan: &Plan, rows: &[ViewRow], from: usize, prefix: &str) -> Option<u32> {
    let prefix = prefix.to_lowercase();
    if prefix.is_empty() || rows.is_empty() {
        return None;
    }
    let start = from.min(rows.len() - 1);
    (start..rows.len()).chain(0..start).find_map(|at| match &rows[at] {
        ViewRow::Item { index } => plan
            .items
            .get(*index as usize)
            .filter(|item| name_starts(item, &prefix))
            .map(|_| u32::try_from(at).unwrap_or(u32::MAX)),
        ViewRow::Folder { .. } => None,
    })
}

/// The view row `query` lands on, or `None` when there is none.
pub fn find_row(plan: &Plan, rows: &[ViewRow], query: &RowQuery) -> Option<u32> {
    match query {
        RowQuery::NextFolder { from } => next_folder(rows, *from as usize),
        RowQuery::PreviousFolder { from } => previous_folder(rows, *from as usize),
        RowQuery::Name { from, prefix } => by_name(plan, rows, *from as usize, prefix),
    }
}
