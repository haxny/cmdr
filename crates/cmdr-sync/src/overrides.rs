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
    /// Leave alone (⌘M).
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
        Action::CopyRight | Action::CopyLeft | Action::RemoveRight | Action::RemoveLeft => !matches!(
            category,
            Category::NameClash | Category::Unreadable | Category::TypeClash
        ),
    }
}

fn caps_of<'a>(side: Side, left: &'a SyncCaps, right: &'a SyncCaps) -> &'a SyncCaps {
    match side {
        Side::Left => left,
        Side::Right => right,
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
        && !caps_of(side, left, right).can_write
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

/// The changes `bulk` makes to `items`: rows where it is illegal, changes
/// nothing, or that don't exist are dropped silently.
///
/// ❗ `items` must be the rows the person selected AND can see (`is_visible`
/// under the current show-filters); the caller filters, this function trusts
/// it. Within that selection, `CopyRight`, `CopyLeft`, and `RemoveTarget` also
/// reach `?` rows: selecting a row and pressing the key is the explicit choice
/// those rows wait for.
pub fn bulk_changes(plan: &Plan, items: &[u32], bulk: BulkAction) -> Vec<ActionChange> {
    items
        .iter()
        .filter_map(|&index| plan.items.get(index as usize))
        .filter_map(|item| {
            let action = wanted(item, bulk)?;
            let legal = action != item.action && refusal_in(plan, item, action).is_none();
            legal.then_some(ActionChange {
                item: item.index,
                action,
            })
        })
        .collect()
}
