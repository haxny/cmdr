//! Resolving what the user attached to a turn, by reference only.
//!
//! Kinds come from live [`PaneStateStore`] entries — the same source the envelope reads —
//! so nothing here stats the filesystem, and an attachment is always path + kind, never
//! contents.

use std::collections::HashMap;

use tauri::{AppHandle, Manager};

use super::{AttachmentKindView, AttachmentRef};
use crate::mcp::PaneStateStore;
use crate::mcp::pane_state::PaneState;

/// "Ask about selection": attachment refs for the focused pane's current selection, or
/// its cursor item when nothing is selected. Reads [`PaneStateStore`] — the same live
/// source the envelope uses — so kinds come from known pane state, with no filesystem
/// stat. Empty when no pane state is registered.
#[tauri::command]
#[specta::specta]
pub fn ask_cmdr_selection_attachments(app: AppHandle) -> Vec<AttachmentRef> {
    let Some(store) = app.try_state::<PaneStateStore>() else {
        return Vec::new();
    };
    let pane = if store.get_focused_pane() == "right" {
        store.get_right()
    } else {
        store.get_left()
    };
    selection_attachments(&pane)
}

/// The attachment refs for `pane`'s selection, or its cursor row with nothing selected.
///
/// `selected_indices` and `cursor_index` are GLOBAL listing rows while `files` holds only
/// the loaded window from `loaded_start`, so each row converts first (the same
/// `checked_sub` as `mcp::executor` and `agent::tools::read::state`). Read with a global
/// row, the window hands back whatever file sits at that offset: silently the wrong one.
fn selection_attachments(pane: &PaneState) -> Vec<AttachmentRef> {
    let indices = if pane.selected_indices.is_empty() {
        vec![pane.cursor_index]
    } else {
        pane.selected_indices.clone()
    };
    indices
        .into_iter()
        .filter_map(|i| i.checked_sub(pane.loaded_start).and_then(|i| pane.files.get(i)))
        .filter(|entry| !entry.path.is_empty())
        .map(pane_entry_to_attachment)
        .collect()
}

/// Resolve dragged local paths into typed attachment refs. Kinds come from the known
/// pane files (left + right) — no filesystem stat — defaulting to `File` for an unknown
/// path. The frontend only calls this for LOCAL drags; virtual-volume drag paths
/// mis-resolve after the pasteboard round-trip and are not supported in v1.
#[tauri::command]
#[specta::specta]
pub fn ask_cmdr_resolve_attachments(app: AppHandle, paths: Vec<String>) -> Vec<AttachmentRef> {
    let mut is_dir_by_path: HashMap<String, bool> = HashMap::new();
    if let Some(store) = app.try_state::<PaneStateStore>() {
        for pane in [store.get_left(), store.get_right()] {
            for entry in pane.files {
                is_dir_by_path.insert(entry.path, entry.is_directory);
            }
        }
    }
    paths
        .into_iter()
        .filter(|path| !path.is_empty())
        .map(|path| {
            let is_dir = is_dir_by_path.get(&path).copied().unwrap_or(false);
            AttachmentRef {
                kind: if is_dir {
                    AttachmentKindView::Folder
                } else {
                    AttachmentKindView::File
                },
                path,
            }
        })
        .collect()
}

/// Map a known pane file entry to an attachment ref (kind straight from `is_directory`).
fn pane_entry_to_attachment(entry: &crate::mcp::pane_state::PaneFileEntry) -> AttachmentRef {
    AttachmentRef {
        path: entry.path.clone(),
        kind: if entry.is_directory {
            AttachmentKindView::Folder
        } else {
            AttachmentKindView::File
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::pane_state::PaneFileEntry;

    fn file(name: &str) -> PaneFileEntry {
        PaneFileEntry {
            name: name.to_string(),
            path: format!("/x/{name}"),
            ..Default::default()
        }
    }

    fn paths(refs: Vec<AttachmentRef>) -> Vec<String> {
        refs.into_iter().map(|r| r.path).collect()
    }

    #[test]
    fn a_selection_in_a_scrolled_window_attaches_the_selected_rows_not_the_rows_at_that_offset() {
        // `selected_indices` and `cursor_index` are GLOBAL listing rows, while `files`
        // is only the loaded window from `loaded_start`. Reading the window with a
        // global row attached whatever file sat at that offset inside the window.
        let pane = PaneState {
            files: vec![file("row1"), file("row2"), file("row3")],
            loaded_start: 1,
            loaded_end: 4,
            selected_indices: vec![2, 3],
            ..Default::default()
        };
        assert_eq!(paths(selection_attachments(&pane)), vec!["/x/row2", "/x/row3"]);
    }

    #[test]
    fn the_cursor_row_in_a_scrolled_window_attaches_that_row() {
        let pane = PaneState {
            files: vec![file("row1"), file("row2"), file("row3")],
            loaded_start: 1,
            cursor_index: 2,
            ..Default::default()
        };
        assert_eq!(paths(selection_attachments(&pane)), vec!["/x/row2"]);
    }
}
