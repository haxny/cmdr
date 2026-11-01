//! Actions you set by hand: which ones a row allows, and the grid's bulk keys.

use serde::{Deserialize, Serialize};

use crate::plan::settle;
use crate::{Action, Category, Mode, Plan, PlanItem, Side, SyncCaps};

/// Set one row's action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ActionChange {
    /// The row's `PlanItem::index`.
    pub item: u32,
    /// The action it should take.
    pub action: Action,
}

/// The grid's commands on a set of rows (spec 4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum BulkAction {
    /// Back to the mode's action (⌘D).
    DefaultDirection,
    /// Copy left to right (→, ⌘L).
    CopyRight,
    /// Copy right to left (←, ⌘R).
    CopyLeft,
    /// Swap a copy's direction (⌘I); a row that can't flip is left alone.
    Reverse,
    /// Leave alone (⌘N; ⌘M is the system's Minimize).
    Skip,
    /// Planned ↔ skip (Space).
    ToggleSkip,
    /// Remove on the target, the right side (⌘⌫).
    RemoveTarget,
}

/// Why a change was refused. One refusal refuses the whole batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum OverrideRefusal {
    /// No row has this index.
    NoSuchItem {
        /// The index asked for.
        item: u32,
    },
    /// The mode doesn't allow this action (a removal in Update or Both ways,
    /// a write to the left in Update or Mirror).
    IllegalForMode {
        /// The row.
        item: u32,
        /// The action refused.
        action: Action,
    },
    /// The row's category doesn't allow this action: clash and unreadable rows
    /// take `Skip` only, `=` needs an equal row, `?` an unknown one.
    IllegalForCategory {
        /// The row.
        item: u32,
        /// The action refused.
        action: Action,
    },
    /// The action would write to a side that takes no writes.
    SideReadOnly {
        /// The row.
        item: u32,
        /// The read-only side.
        side: Side,
    },
    /// The side the action reads from (or removes) has nothing at this row.
    NothingToCopy {
        /// The row.
        item: u32,
        /// The empty side.
        side: Side,
    },
}

/// Whether `mode` ever allows `action`, whatever the row.
fn mode_allows(mode: Mode, action: Action) -> bool {
    match mode {
        Mode::CompareOnly => true,
        Mode::BothWays => !action.is_removal(),
        Mode::Update => !action.is_removal() && action != Action::CopyLeft,
        Mode::Mirror => !matches!(action, Action::CopyLeft | Action::RemoveLeft),
    }
}

/// Whether the row's category allows `action`: a row we only partly know takes
/// `Skip` alone; `=` only on equal rows, `?` only on unknown ones.
fn category_allows(category: Category, action: Action) -> bool {
    match action {
        Action::Skip => true,
        Action::Equal => category == Category::Equal,
        Action::Undecided => category == Category::Unknown,
        Action::CopyRight | Action::CopyLeft | Action::RemoveRight | Action::RemoveLeft => category.is_comparable(),
    }
}

/// Why `item` may not take `action`, if it may not. Shared with `reclassify`.
pub(crate) fn refusal(
    mode: Mode,
    item: &PlanItem,
    action: Action,
    left: &SyncCaps,
    right: &SyncCaps,
) -> Option<OverrideRefusal> {
    let index = item.index;
    if !mode_allows(mode, action) {
        return Some(OverrideRefusal::IllegalForMode { item: index, action });
    }
    if !category_allows(item.verdict.category, action) {
        return Some(OverrideRefusal::IllegalForCategory { item: index, action });
    }
    if let Some(side) = action.needs()
        && item.entry(side).is_none()
    {
        return Some(OverrideRefusal::NothingToCopy { item: index, side });
    }
    if let Some(side) = action.target()
        && !side.pick(left, right).can_write
    {
        return Some(OverrideRefusal::SideReadOnly { item: index, side });
    }
    None
}

const EVERY_ACTION: [Action; 7] = [
    Action::CopyRight,
    Action::CopyLeft,
    Action::RemoveRight,
    Action::RemoveLeft,
    Action::Equal,
    Action::Skip,
    Action::Undecided,
];

/// Why a row of `plan` may not take `action`, judged with the plan's own mode
/// and volume capabilities (never a caller's).
fn refusal_in(plan: &Plan, item: &PlanItem, action: Action) -> Option<OverrideRefusal> {
    refusal(plan.mode, item, action, &plan.left_caps, &plan.right_caps)
}

/// Every action a row of `plan` may take.
pub fn legal_actions(plan: &Plan, item: &PlanItem) -> Vec<Action> {
    EVERY_ACTION
        .into_iter()
        .filter(|&action| refusal_in(plan, item, action).is_none())
        .collect()
}

/// A new plan with `changes` applied, or the first refusal (and no change).
pub fn apply_overrides(plan: &Plan, changes: &[ActionChange]) -> Result<Plan, OverrideRefusal> {
    let mut items = plan.items.clone();
    for change in changes {
        let item = items
            .get(change.item as usize)
            .ok_or(OverrideRefusal::NoSuchItem { item: change.item })?;
        if let Some(refused) = refusal_in(plan, item, change.action) {
            return Err(refused);
        }
        items[change.item as usize] = settle(PlanItem {
            action: change.action,
            ..item.clone()
        });
    }
    Ok(Plan { items, ..plan.clone() })
}

/// What `bulk` asks of one row, before legality.
fn wanted(item: &PlanItem, bulk: BulkAction) -> Option<Action> {
    let acts = |action: Action| action.is_copy() || action.is_removal();
    match bulk {
        BulkAction::DefaultDirection => Some(item.default_action),
        BulkAction::CopyRight => Some(Action::CopyRight),
        BulkAction::CopyLeft => Some(Action::CopyLeft),
        BulkAction::Skip => Some(Action::Skip),
        BulkAction::RemoveTarget => Some(Action::RemoveRight),
        BulkAction::ToggleSkip if acts(item.action) => Some(Action::Skip),
        BulkAction::ToggleSkip if item.action == Action::Skip && acts(item.default_action) => Some(item.default_action),
        BulkAction::ToggleSkip => None,
        BulkAction::Reverse => reversed(item),
    }
}

/// A copy between two present sides flips; a removal becomes the copy that
/// restores it. A one-sided copy would have to become a removal, so it stays.
fn reversed(item: &PlanItem) -> Option<Action> {
    let both = item.left.is_some() && item.right.is_some();
    match item.action {
        Action::CopyRight if both => Some(Action::CopyLeft),
        Action::CopyLeft if both => Some(Action::CopyRight),
        Action::RemoveRight => Some(Action::CopyLeft),
        Action::RemoveLeft => Some(Action::CopyRight),
        _ => None,
    }
}

/// Why a bulk key left a row as it was. One per kind of refusal, so the dialog
/// can say it once for every row it applies to ("3 rows weren't changed: the
/// right side takes no changes").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RowSkipReason {
    /// The mode doesn't allow the action (a removal outside Mirror, a write to
    /// the left in Update or Mirror).
    IllegalForMode {
        /// The action asked for.
        action: Action,
    },
    /// The row's category doesn't allow the action: name clash, type clash,
    /// and unreadable rows take `Skip` only, `=` needs an equal row, `?` an
    /// unknown one.
    IllegalForCategory {
        /// The action asked for.
        action: Action,
    },
    /// The side the action reads from (or removes) has nothing there.
    NothingToCopy {
        /// The empty side.
        side: Side,
    },
    /// The action would write to a side that takes no writes.
    SideReadOnly {
        /// The read-only side.
        side: Side,
    },
    /// Reverse on a row that has no copy between two present sides and no removal.
    CantReverse,
    /// Space on a row with no planned action to skip or restore.
    NothingToToggle,
}

/// How many rows a bulk key left alone for one reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SkippedRows {
    /// Why.
    pub reason: RowSkipReason,
    /// How many rows.
    pub count: u32,
}

/// What a bulk key does to a set of rows: the changes, and every row it left
/// alone grouped by why. A row that already has the action counts in neither.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BulkOutcome {
    /// The legal changes, for `apply_overrides`.
    pub changes: Vec<ActionChange>,
    /// The rows left alone, by reason, in the order the reasons first came up.
    pub skipped: Vec<SkippedRows>,
}

/// What a bulk key does with one row, given why it may not take the action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowVerdict {
    /// The row takes the change.
    Change(ActionChange),
    /// The row is left alone, counted under this reason.
    Skip(RowSkipReason),
    /// The row isn't there: neither changed nor counted.
    Drop,
}

/// A refusal turned into what happens to the row. ❗ A row that doesn't exist
/// is dropped, never treated as legal.
pub(crate) fn row_verdict(refused: Option<OverrideRefusal>, change: ActionChange) -> RowVerdict {
    let Some(refused) = refused else {
        return RowVerdict::Change(change);
    };
    RowVerdict::Skip(match refused {
        OverrideRefusal::NoSuchItem { .. } => return RowVerdict::Drop,
        OverrideRefusal::IllegalForMode { action, .. } => RowSkipReason::IllegalForMode { action },
        OverrideRefusal::IllegalForCategory { action, .. } => RowSkipReason::IllegalForCategory { action },
        OverrideRefusal::NothingToCopy { side, .. } => RowSkipReason::NothingToCopy { side },
        OverrideRefusal::SideReadOnly { side, .. } => RowSkipReason::SideReadOnly { side },
    })
}

/// What `bulk` asks of one row: a change, a reason it can't, or nothing to do.
fn judge(plan: &Plan, item: &PlanItem, bulk: BulkAction) -> Result<Option<ActionChange>, RowSkipReason> {
    let Some(action) = wanted(item, bulk) else {
        return Err(match bulk {
            BulkAction::Reverse => RowSkipReason::CantReverse,
            _ => RowSkipReason::NothingToToggle,
        });
    };
    if action == item.action {
        return Ok(None);
    }
    let change = ActionChange {
        item: item.index,
        action,
    };
    match row_verdict(refusal_in(plan, item, action), change) {
        RowVerdict::Change(change) => Ok(Some(change)),
        RowVerdict::Skip(reason) => Err(reason),
        RowVerdict::Drop => Ok(None),
    }
}

/// The changes `bulk` makes to `items`, and the rows it leaves alone with why.
/// Rows that don't exist are left out of both.
///
/// ❗ `items` must be the rows the person selected AND can see (`is_visible`
/// under the current show-filters); the caller filters, this function trusts
/// it. Within that selection, `CopyRight`, `CopyLeft`, and `RemoveTarget` also
/// reach `?` rows: selecting a row and pressing the key is the explicit choice
/// those rows wait for.
pub fn bulk_outcome(plan: &Plan, items: &[u32], bulk: BulkAction) -> BulkOutcome {
    let mut outcome = BulkOutcome::default();
    for item in items.iter().filter_map(|&index| plan.items.get(index as usize)) {
        match judge(plan, item, bulk) {
            Ok(Some(change)) => outcome.changes.push(change),
            Ok(None) => {}
            Err(reason) => match outcome.skipped.iter_mut().find(|rows| rows.reason == reason) {
                Some(rows) => rows.count += 1,
                None => outcome.skipped.push(SkippedRows { reason, count: 1 }),
            },
        }
    }
    outcome
}

/// The legal changes of [`bulk_outcome`]: rows where `bulk` is illegal, changes
/// nothing, or that don't exist are left out (the outcome says why).
#[cfg(test)]
pub(crate) fn bulk_changes(plan: &Plan, items: &[u32], bulk: BulkAction) -> Vec<ActionChange> {
    bulk_outcome(plan, items, bulk).changes
}
