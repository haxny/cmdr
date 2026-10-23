//! The file list header's Sort by popup: Date modified, Date created, and Date added, the date
//! the pane ASKED for checked (none while it's in another column's order, or a search pane's
//! ranked order). Opened by a right-click on either view's header, and by the Full date header's
//! chevron.
//!
//! A pick comes back as [`DateSortMenuPick`], carrying the PANE: a right-click on the unfocused
//! pane's header sorts that pane, which `menu-sort` (the focused pane's lane) can't say. The
//! frontend applies it (`pane/sort-operations.ts`, `pickSortColumn`), so picking the checked
//! date does nothing there. ❗ Every item id is this menu's own, ❌ never the bar's `SORT_BY_*_ID`:
//! `handle_menu_event` serves both, and a bar id would sort the focused pane.

use cmdr_listing::SortColumn;
use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Runtime,
    menu::{CheckMenuItem, Menu},
};
use tauri_specta::Event;

use crate::intl::menu_t;

/// Which pane a header menu was opened over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum PaneSide {
    Left,
    Right,
}

/// `date-sort-menu-pick`: a date was picked from a file list header's Sort by popup. Emitted to
/// the main window, which sorts `pane` by `column` unless that pane already asked for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct DateSortMenuPick {
    pub pane: PaneSide,
    pub column: SortColumn,
}

/// The popup's dates, top to bottom, with their labels: View > Sort by's words, in its order.
const DATES: [(SortColumn, &str); 3] = [
    (SortColumn::Modified, "menu.sort.dateModified"),
    (SortColumn::Created, "menu.sort.dateCreated"),
    (SortColumn::Added, "menu.sort.dateAdded"),
];

const PANES: [PaneSide; 2] = [PaneSide::Left, PaneSide::Right];

/// One item of the popup, before it's a native item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DateSortMenuItem {
    pub id: String,
    pub label_key: &'static str,
    pub checked: bool,
}

/// The item id for `column` in `pane`'s popup, such as `date_sort_menu_left_added`.
fn item_id(pane: PaneSide, column: SortColumn) -> String {
    let pane = match pane {
        PaneSide::Left => "left",
        PaneSide::Right => "right",
    };
    let column = match column {
        SortColumn::Created => "created",
        SortColumn::Added => "added",
        _ => "modified",
    };
    format!("date_sort_menu_{pane}_{column}")
}

/// The popup's items for `pane`, the `requested` column's checked.
pub(crate) fn date_sort_menu_items(pane: PaneSide, requested: Option<SortColumn>) -> Vec<DateSortMenuItem> {
    DATES
        .iter()
        .map(|&(column, label_key)| DateSortMenuItem {
            id: item_id(pane, column),
            label_key,
            checked: requested == Some(column),
        })
        .collect()
}

impl DateSortMenuPick {
    /// The pick a clicked item id names, or `None` when the id isn't one of this popup's.
    pub(crate) fn from_menu_id(menu_id: &str) -> Option<Self> {
        PANES.into_iter().find_map(|pane| {
            DATES
                .iter()
                .find(|&&(column, _)| item_id(pane, column) == menu_id)
                .map(|&(column, _)| Self { pane, column })
        })
    }
}

/// Builds the popup for `pane`. Check items, so the requested date shows its checkmark; muda
/// toggles the clicked one, which nobody sees, since the menu is gone by then.
pub fn build_date_sort_menu<R: Runtime>(
    app: &AppHandle<R>,
    pane: PaneSide,
    requested: Option<SortColumn>,
) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    for item in date_sort_menu_items(pane, requested) {
        menu.append(&CheckMenuItem::with_id(
            app,
            item.id,
            menu_t(item.label_key),
            true,
            item.checked,
            None::<&str>,
        )?)?;
    }
    Ok(menu)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checked_ids(pane: PaneSide, requested: Option<SortColumn>) -> Vec<String> {
        date_sort_menu_items(pane, requested)
            .into_iter()
            .filter_map(|item| item.checked.then_some(item.id))
            .collect()
    }

    #[test]
    fn lists_the_three_dates_in_the_bars_order_and_words() {
        let labels: Vec<_> = date_sort_menu_items(PaneSide::Left, None)
            .into_iter()
            .map(|item| item.label_key)
            .collect();
        assert_eq!(
            labels,
            ["menu.sort.dateModified", "menu.sort.dateCreated", "menu.sort.dateAdded"]
        );
    }

    #[test]
    fn checks_the_requested_date_only() {
        for (column, _) in DATES {
            let checked = checked_ids(PaneSide::Right, Some(column));
            assert_eq!(checked, [item_id(PaneSide::Right, column)], "{column:?}");
        }
    }

    /// A Name sort, or a search pane in its ranked order, asked for no date.
    #[test]
    fn checks_nothing_when_no_date_was_requested() {
        assert!(checked_ids(PaneSide::Left, Some(SortColumn::Name)).is_empty());
        assert!(checked_ids(PaneSide::Left, Some(SortColumn::Size)).is_empty());
        assert!(checked_ids(PaneSide::Left, None).is_empty());
    }

    /// The pick carries the pane the menu opened over, so the unfocused pane's header sorts that pane.
    #[test]
    fn every_item_id_reads_back_as_its_pane_and_date() {
        for pane in PANES {
            for (column, _) in DATES {
                assert_eq!(
                    DateSortMenuPick::from_menu_id(&item_id(pane, column)),
                    Some(DateSortMenuPick { pane, column })
                );
            }
        }
        assert_eq!(DateSortMenuPick::from_menu_id("sort_by_added"), None);
        assert_eq!(DateSortMenuPick::from_menu_id("date_sort_menu_left_name"), None);
    }

    /// ❗ A bar id here would route the pick through `menu-sort`, sorting the FOCUSED pane.
    #[test]
    fn no_item_id_belongs_to_the_bar_or_a_command() {
        use super::super::{SORT_BY_ADDED_ID, SORT_BY_CREATED_ID, SORT_BY_MODIFIED_ID, menu_id_to_command};
        for pane in PANES {
            for item in date_sort_menu_items(pane, None) {
                assert!(![SORT_BY_MODIFIED_ID, SORT_BY_CREATED_ID, SORT_BY_ADDED_ID].contains(&item.id.as_str()));
                assert_eq!(menu_id_to_command(&item.id), None, "{}", item.id);
            }
        }
        let left = date_sort_menu_items(PaneSide::Left, None);
        let right = date_sort_menu_items(PaneSide::Right, None);
        assert!(left.iter().all(|l| right.iter().all(|r| l.id != r.id)));
    }
}
