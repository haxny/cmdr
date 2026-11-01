# Synchronize folders (dialog) details

Design: https://github.com/vdavid/cmdr/issues/871 (the dialog, its keys, and accessibility). Backend:
`apps/desktop/src-tauri/src/sync/CLAUDE.md`.

## Opening

`sync.openDialog` (⇧F12, File menu, palette) → `showSyncFolders` in `+page.svelte` → `DualPaneExplorer.getSyncRoots()` →
`syncRootsOf` (`pane-roots.ts`). Each side is the pane's volume and folder plus its selection in the backend's row
numbering (no `..`) and the listing sequence it was read at. A pane without a folder listing (network hub, search
results) answers `null` and a toast says so. The selections ride along always; the session sends them only while "Only
selected" is on.

## Session (`sync-session.svelte.ts`)

- `open()` subscribes to the three compare events, reads `getSyncLastOptions` for the pair (defaults otherwise: Both
  ways, date and size, subfolders, `DEFAULT_EXCLUDES`), and compares.
- `compare()` sends the options, Settings as `SyncDefaults` (`readSyncDefaults`), no profile overrides, and the pane
  setting for hidden files. It records the session id and generation it got back, and the compared mode (a key's
  legality is checked against the plan's mode, not the options that may have moved on).
- Rows page in blocks of 200 (`ensureRange`, from `ColumnList`'s windowed source). Every plan change re-reads the state
  and re-pages: `refresh()` on a finished compare and `sync-rows-changed`, `invalidate()` after an action or a content
  compare. A response for an older view (`viewVersion`) is dropped. `allRows()` reads the whole view in blocks of 2,000
  for ⌘A and for copying a large selection.
- `close()` saves the last options (a `writeFailed` refusal becomes a warning toast) and closes the session; the backend
  also closes sessions when their window goes.

## Key map as built (`sync-keys.ts`)

Anywhere: Esc (stop, then close), ⌘↩ (confirm), ⌥⌘R (compare), F2 / ⌘S (profiles, wired with the profiles menu), ⌘U
(swap and compare), ⌘⇧L / E / N / R / D / S / U / W (show-filters →, =, ≠, ←, both sides, one side, ?, warnings).
Options strip: Enter compares. Grid: Enter / ⌘F3 / double click (compare content), ⌘D, ⌘L or →, ⌘R or ←, ⌘I, ⌘M or `-`,
Space (planned ↔ skip), ⌘⌫ (remove on the right), F3 / ⇧F3, F4 / ⇧F4, ⌥Space, ⌘PageDown / ⌥⌘↓, ⌘PageUp / ⌥⌘↑, ⌘C, ⇧F10
(row menu); selection: click, ⌘-click, ⇧-click, ⇧↑ / ⇧↓, ⌘A.

Decision: ⌘ for TC's Ctrl, except ⌘W (closes windows on macOS), so reverse is ⌘I; ⌘⌫ because Backspace is macOS Delete.
Row keys only count on the grid so a text field keeps them. The keys are not registered commands: the dialog owns them
(like a modal's Enter), so Settings can't rebind them yet.

## Grid

`ColumnList` with `semantics: 'grid'` (no treegrid; expanding a one-sided folder is phase 2). Its `aria-selected`
follows the CURSOR row (the grid's own contract); the person's multi-row selection is said in each row's name ("…,
selected") and shown by the ✓ column. Folder header rows are `rowheader`s ("Photos/2024, folder 3 of 12"); root rows
come first with no header. Each data row's `aria-label` is `rowAccessibleName` plus "selected". The action cell shows
the category icon, TC's arrow, a "changed by you" dot, a warning triangle for older-replaces-newer, a pending mark for a
running content compare, and the rung badge (T size and date, S size, C content) with its words in a tooltip and for
screen readers.

F3 / F4 find the file's folder from the header above it (`sync-row-files.ts`). Gap: the header holds the left spelling,
so a right folder that differs only in case or Unicode form on a case-sensitive volume can miss.

## Notices and errors

A key the cursor row can't take is checked locally first (`rowRefusal`: nothing on that side, the mode, the category,
else "that side may not take changes"), because `bulk_changes` silently leaves such rows alone. With a selection the
backend decides per row (TC's behaviour). Backend refusals are `SyncError` variants mapped one by one in
`syncProblemText`; anything untyped is logged and shown as `unexpected`.

## Not yet here

The confirm and run (⌘↩, the Synchronize button) and profiles (F2, ⌘S) arrive with the next tasks; until then the button
explains itself when there's no fresh compare and the keys do nothing. Root paths are shown, not edited. The "exact
one-hour shift" toggle and per-profile overrides follow Settings.
