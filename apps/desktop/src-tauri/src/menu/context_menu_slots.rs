//! The context menu's late-row vocabulary (macOS): the parts a builder makes for a slow fact
//! still out when the menu goes up, which `context_menu_live.rs` fills in or reveals while the
//! menu is open. Every sub-builder (`open_with.rs`, `share_submenu.rs`, `file_provider_items.rs`)
//! and `file_context_menu.rs` makes them, so they sit below all of those and below the live menu.
//!
//! Also [`find_title_run`], how both a [`SlotGroup`] and `context_menu_icons.rs` find their rows
//! in the live `NSMenu` by title.

use objc2::rc::Retained;
use objc2_app_kit::{NSMenu, NSMenuItem};
use tauri::Runtime;
use tauri::menu::{MenuItem, PredefinedMenuItem, Submenu};

use super::macos_appkit::plain_title;

/// A submenu that opened before its contents were known: the disabled line standing in for
/// them, and the separator between that line and the submenu's fixed last item.
pub struct Placeheld<R: Runtime> {
    pub submenu: Submenu<R>,
    pub placeholder: MenuItem<R>,
    pub separator: PredefinedMenuItem<R>,
}

/// A group of context-menu rows built before their fact answered, hidden when the menu
/// starts tracking and revealed when it does. The group's leading separator is the row just
/// above its first item.
///
/// Built with their final IDs, so a click routes exactly as it would on a group built with
/// the answer in hand, and with titles that tell them apart ([`find_title_run`] finds the
/// group by them, as one run).
pub struct SlotGroup<R: Runtime> {
    pub items: Vec<MenuItem<R>>,
    /// The live rows, separator first, found when the menu starts tracking. Empty until
    /// then, and for good if the group wasn't found, in which case nothing is revealed.
    rows: Vec<Retained<NSMenuItem>>,
}

impl<R: Runtime> SlotGroup<R> {
    pub fn new(items: Vec<MenuItem<R>>) -> Self {
        Self {
            items,
            rows: Vec::new(),
        }
    }

    /// Finds the group's rows in `root` and hides them, before AppKit lays the menu out.
    pub(super) fn hide(&mut self, root: &NSMenu) {
        let titles: Option<Vec<String>> = self.items.iter().map(|item| item.text().ok()).collect();
        let all: Vec<Retained<NSMenuItem>> = (0..root.numberOfItems())
            .filter_map(|index| root.itemAtIndex(index))
            .collect();
        let live_titles: Vec<String> = all.iter().map(|item| item.title().to_string()).collect();
        let Some(start) = titles.and_then(|titles| find_title_run(&live_titles, &titles)) else {
            log::warn!(target: "menu", "A right-click group that was still loading isn't in the menu, so it stays as built");
            return;
        };
        let Some(separator) = start
            .checked_sub(1)
            .and_then(|index| all.get(index))
            .filter(|row| row.isSeparatorItem())
        else {
            log::warn!(target: "menu", "A right-click group that was still loading has no separator above it, so it stays as built");
            return;
        };
        self.rows = std::iter::once(separator.clone())
            .chain(all[start..start + self.items.len()].iter().cloned())
            .collect();
        for row in &self.rows {
            row.setHidden(true);
        }
    }

    /// Shows the separator and the items at `which`.
    pub(super) fn reveal(&self, which: impl IntoIterator<Item = usize>) {
        let Some((separator, items)) = self.rows.split_first() else {
            return;
        };
        let mut any = false;
        for index in which {
            if let Some(row) = items.get(index) {
                row.setHidden(false);
                any = true;
            }
        }
        if any {
            separator.setHidden(false);
        }
    }
}

/// Where `run` starts as one contiguous stretch of `titles`, or `None` when it doesn't.
///
/// Titles compare without the display-accelerator run `display_accelerators.rs` may have
/// put after a TAB. An empty run, or one holding an empty title, matches nothing: a
/// separator's title is empty, and a match on it would be a match on nothing.
pub(super) fn find_title_run<S: AsRef<str>>(titles: &[S], run: &[String]) -> Option<usize> {
    if run.is_empty() || run.iter().any(String::is_empty) {
        return None;
    }
    titles.windows(run.len()).position(|window| {
        window
            .iter()
            .zip(run)
            .all(|(title, want)| plain_title(title.as_ref()) == want)
    })
}

#[cfg(test)]
#[path = "context_menu_slots_test.rs"]
mod context_menu_slots_test;
