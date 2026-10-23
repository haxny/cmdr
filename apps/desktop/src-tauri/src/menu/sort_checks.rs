//! View > Sort by's checkmarks: which column and which order the focused pane ASKED for.
//!
//! One state (`MenuState.requested_sort`, pushed by the frontend's `update_sort_menu`), one
//! predicate ([`SortCheck::is_on`]), and one writer ([`sync_sort_check_states`]), which the build
//! seeds from too. A date sort the location can't honor falls back to Modified in the backend, and
//! that ❌ never reaches here: the menu shows the request, like the pane's header and MCP's `sort`.

use cmdr_listing::{SortColumn, SortOrder};
use tauri::Runtime;

use crate::ignore_poison::IgnorePoison as _;

use super::menu_bar::MENU_BAR;
use super::menu_spec::{CheckRole, EntryKind, SortCheck, SubmenuRole, SubmenuSpec};
use super::{MenuState, cloned_handle};

/// The sort a pane asked for: what View > Sort by checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RequestedSort {
    pub column: SortColumn,
    pub order: SortOrder,
}

impl SortCheck {
    /// Whether this item is checked while the focused pane asks for `sort`. `None` is a search
    /// pane in its ranked order: no column and no order to point at, so nothing is.
    pub(crate) fn is_on(self, sort: Option<RequestedSort>) -> bool {
        match (self, sort) {
            (_, None) => false,
            (Self::Column(column), Some(sort)) => sort.column == column,
            (Self::Order(order), Some(sort)) => sort.order == order,
        }
    }
}

/// Every Sort by check item's id and whether it's checked for `sort`, read off `MENU_BAR`'s Sort
/// by rows, so a new row can't be missed here.
pub(crate) fn sort_check_states(sort: Option<RequestedSort>) -> Vec<(&'static str, bool)> {
    sort_by_spec()
        .entries
        .iter()
        .filter_map(|entry| match entry.kind {
            EntryKind::Check(check) => match check.role {
                CheckRole::Sort(role) => Some((check.id, role.is_on(sort))),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// Sets every Sort by check from `MenuState.requested_sort`, the one writer of their checked state.
///
/// Each item is found by id in the live submenu at call time, so a rebind's replacement
/// (`update_menu_item_accelerator`) is the one reached, ❌ never a stale handle. Also what undoes
/// muda's own toggle on a click: the check moves only when the frontend's sort lands and re-pushes.
pub(crate) fn sync_sort_check_states<R: Runtime>(menu_state: &MenuState<R>) {
    let sort = *menu_state.requested_sort.lock_ignore_poison();
    // A handle, so no lock is held across `set_checked`, which waits on the main thread.
    let Some(submenu) = cloned_handle(&menu_state.sort_submenu) else {
        return;
    };
    for (id, checked) in sort_check_states(sort) {
        let Some(kind) = submenu.get(id) else {
            log::warn!(target: "menu", "View > Sort by holds no `{id}` item to check");
            continue;
        };
        match kind.as_check_menuitem() {
            Some(item) => {
                if let Err(e) = item.set_checked(checked) {
                    log::warn!(target: "menu", "Couldn't set the check on `{id}`: {e}");
                }
            }
            None => log::warn!(target: "menu", "View > Sort by's `{id}` isn't a check item"),
        }
    }
}

/// The Sort by submenu's spec, which `menu_bar_test.rs` pins to exist on both platforms.
fn sort_by_spec() -> &'static SubmenuSpec {
    fn find(entries: &'static [super::menu_spec::Entry]) -> Option<&'static SubmenuSpec> {
        entries.iter().find_map(|entry| match &entry.kind {
            EntryKind::Submenu(nested) if nested.role == SubmenuRole::SortBy => Some(nested),
            EntryKind::Submenu(nested) => find(nested.entries),
            _ => None,
        })
    }
    MENU_BAR
        .iter()
        .find_map(|bar_menu| find(bar_menu.submenu.entries))
        .expect("`MENU_BAR` has a Sort by submenu")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checked(sort: Option<RequestedSort>) -> Vec<&'static str> {
        sort_check_states(sort)
            .into_iter()
            .filter_map(|(id, on)| on.then_some(id))
            .collect()
    }

    fn sort(column: SortColumn, order: SortOrder) -> Option<RequestedSort> {
        Some(RequestedSort { column, order })
    }

    #[test]
    fn every_column_checks_its_own_item_and_the_order() {
        use super::super::{
            SORT_ASCENDING_ID, SORT_BY_ADDED_ID, SORT_BY_CREATED_ID, SORT_BY_EXTENSION_ID, SORT_BY_MODIFIED_ID,
            SORT_BY_NAME_ID, SORT_BY_SIZE_ID, SORT_DESCENDING_ID,
        };
        let cases = [
            (SortColumn::Name, SORT_BY_NAME_ID),
            (SortColumn::Extension, SORT_BY_EXTENSION_ID),
            (SortColumn::Modified, SORT_BY_MODIFIED_ID),
            (SortColumn::Size, SORT_BY_SIZE_ID),
            (SortColumn::Created, SORT_BY_CREATED_ID),
            (SortColumn::Added, SORT_BY_ADDED_ID),
        ];
        for (column, id) in cases {
            assert_eq!(
                checked(sort(column, SortOrder::Ascending)),
                [id, SORT_ASCENDING_ID],
                "{column:?}"
            );
            assert_eq!(
                checked(sort(column, SortOrder::Descending)),
                [id, SORT_DESCENDING_ID],
                "{column:?}"
            );
        }
    }

    #[test]
    fn a_ranked_search_pane_checks_nothing() {
        assert_eq!(checked(None), Vec::<&str>::new());
    }

    #[test]
    fn covers_all_eight_sort_by_rows() {
        assert_eq!(sort_check_states(None).len(), 8);
    }
}
