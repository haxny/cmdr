//! A queue row's right-click menu, in the operation queue window.
//!
//! The FRONTEND decides which items a row lists, in what order, and which are live
//! (`lib/file-operations/queue/queue-row-menu.ts`): every fact behind that lives in the
//! queue window (the row's status, its session, the selection). This side only turns those
//! entries into a native menu, labelled and keyed, and hands a pick back to the window as
//! `QueueRowContextAction`, which runs the same action the key does.

use tauri::{
    AppHandle, Runtime,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
};

use super::menu_structure::ContextMenuShortcuts;
use crate::intl::menu_t;

/// One thing a queue row's menu can do. Pause and Resume share a key but are separate
/// actions, so the label can say which one this click is, and so a pick that went stale
/// while the menu was up can be told apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum QueueRowMenuAction {
    MoveEarlier,
    MoveLater,
    Select,
    Cancel,
    Dismiss,
    Show,
    Pause,
    Resume,
    Rollback,
}

impl QueueRowMenuAction {
    const ALL: [Self; 9] = [
        Self::MoveEarlier,
        Self::MoveLater,
        Self::Select,
        Self::Cancel,
        Self::Dismiss,
        Self::Show,
        Self::Pause,
        Self::Resume,
        Self::Rollback,
    ];

    /// The native item's id. Unmapped in `menu_id_to_command`: `handle_menu_event` routes
    /// these to the queue window ahead of that table.
    fn menu_id(self) -> &'static str {
        match self {
            Self::MoveEarlier => "queue_row_move_earlier",
            Self::MoveLater => "queue_row_move_later",
            Self::Select => "queue_row_select",
            Self::Cancel => "queue_row_cancel",
            Self::Dismiss => "queue_row_dismiss",
            Self::Show => "queue_row_show",
            Self::Pause => "queue_row_pause",
            Self::Resume => "queue_row_resume",
            Self::Rollback => "queue_row_rollback",
        }
    }

    /// The action a clicked item id names, or `None` when the id isn't one of this menu's.
    pub fn from_menu_id(menu_id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|action| action.menu_id() == menu_id)
    }

    fn label_key(self) -> &'static str {
        match self {
            Self::MoveEarlier => "menu.queue.moveUp",
            Self::MoveLater => "menu.queue.moveDown",
            Self::Select => "menu.queue.select",
            Self::Cancel => "menu.queue.cancel",
            Self::Dismiss => "menu.queue.dismiss",
            Self::Show => "menu.queue.show",
            Self::Pause => "menu.queue.pause",
            Self::Resume => "menu.queue.resume",
            Self::Rollback => "menu.queue.rollback",
        }
    }

    /// The `Operation queue window` command whose key this item runs, so its label shows
    /// that key (`lib/commands/sources/queue-window.ts`).
    fn command_id(self) -> &'static str {
        match self {
            Self::MoveEarlier => "queue.moveEarlier",
            Self::MoveLater => "queue.moveLater",
            Self::Select => "queue.toggleSelected",
            Self::Cancel => "queue.cancel",
            // ⌫ cancels a live row and dismisses a failure; on a failure's menu it's the Dismiss key.
            Self::Dismiss => "queue.cancelOrDismiss",
            Self::Show => "queue.showInMainWindow",
            Self::Pause | Self::Resume => "queue.pauseOrResume",
            Self::Rollback => "queue.rollback",
        }
    }
}

/// One line of a queue row's menu, as the window lists it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum QueueRowMenuEntry {
    Item {
        action: QueueRowMenuAction,
        enabled: bool,
    },
    /// An item that toggles, ticked while on: Select, ticked on a selected row, the way the
    /// row's checkbox is.
    Check {
        action: QueueRowMenuAction,
        enabled: bool,
        checked: bool,
    },
    Separator,
}

/// Builds a queue row's menu from the window's entries.
///
/// The accelerators are display text: a popup's key equivalents are never registered with
/// the app, so a bare `C` or `Space` here can't eat that key while the window is focused
/// ([`ContextMenuShortcuts`]). While the menu is open, pressing one picks its item, which
/// is AppKit's own menu behavior.
pub fn build_queue_row_context_menu<R: Runtime>(
    app: &AppHandle<R>,
    entries: &[QueueRowMenuEntry],
    shortcuts: &ContextMenuShortcuts,
) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    for entry in entries {
        match *entry {
            QueueRowMenuEntry::Item { action, enabled } => {
                let accelerator = shortcuts.for_command(action.command_id());
                menu.append(&MenuItem::with_id(
                    app,
                    action.menu_id(),
                    menu_t(action.label_key()),
                    enabled,
                    accelerator.as_deref(),
                )?)?;
            }
            QueueRowMenuEntry::Check {
                action,
                enabled,
                checked,
            } => {
                let accelerator = shortcuts.for_command(action.command_id());
                menu.append(&CheckMenuItem::with_id(
                    app,
                    action.menu_id(),
                    menu_t(action.label_key()),
                    enabled,
                    checked,
                    accelerator.as_deref(),
                )?)?;
            }
            QueueRowMenuEntry::Separator => menu.append(&PredefinedMenuItem::separator(app)?)?,
        }
    }
    Ok(menu)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_round_trips_through_its_menu_id() {
        for action in QueueRowMenuAction::ALL {
            assert_eq!(QueueRowMenuAction::from_menu_id(action.menu_id()), Some(action));
        }
        assert_eq!(QueueRowMenuAction::from_menu_id("tab_close"), None);
    }

    /// An id the main bar or another popup also uses would route a click to the wrong window.
    #[test]
    fn no_menu_id_belongs_to_another_command() {
        for action in QueueRowMenuAction::ALL {
            assert_eq!(super::super::menu_id_to_command(action.menu_id()), None, "{action:?}");
        }
    }

    /// The queue's single-letter keys show beside their items, bare, the way the row's
    /// button chips print them.
    #[test]
    fn a_bare_queue_key_labels_its_item() {
        let shortcuts: ContextMenuShortcuts = serde_json::from_str(
            r#"{"queue.cancel": "C", "queue.toggleSelected": "Space", "queue.moveEarlier": "⌥↑"}"#,
        )
        .expect("shortcuts parse");
        assert_eq!(
            shortcuts
                .for_command(QueueRowMenuAction::Cancel.command_id())
                .as_deref(),
            Some("C")
        );
        assert_eq!(
            shortcuts
                .for_command(QueueRowMenuAction::Select.command_id())
                .as_deref(),
            Some("Space")
        );
        assert_eq!(
            shortcuts
                .for_command(QueueRowMenuAction::MoveEarlier.command_id())
                .as_deref(),
            Some("Alt+Up")
        );
    }

    #[test]
    fn entries_read_the_window_s_wire_shape() {
        let entries: Vec<QueueRowMenuEntry> = serde_json::from_str(
            r#"[{"kind": "item", "action": "moveEarlier", "enabled": false}, {"kind": "separator"},
                {"kind": "check", "action": "select", "enabled": true, "checked": true}]"#,
        )
        .expect("entries parse");
        assert_eq!(
            entries,
            [
                QueueRowMenuEntry::Item {
                    action: QueueRowMenuAction::MoveEarlier,
                    enabled: false
                },
                QueueRowMenuEntry::Separator,
                QueueRowMenuEntry::Check {
                    action: QueueRowMenuAction::Select,
                    enabled: true,
                    checked: true
                }
            ]
        );
    }
}
