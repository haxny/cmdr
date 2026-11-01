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

Anywhere: Esc (stop, then close), ⌘↩ (confirm), ⌥⌘R (compare), F2 (profiles menu), ⌘S (save as profile), ⌘U (swap and
compare), ⌘⇧L / E / N / R / D / S / U / W (show-filters →, =, ≠, ←, both sides, one side, ?, warnings). Options strip:
Enter compares. Grid: Enter / ⌘F3 / double click (compare content), ⌘D, ⌘L or →, ⌘R or ←, ⌘I, ⌘M or `-`, Space (planned
↔ skip), ⌘⌫ (remove on the right), F3 / ⇧F3, F4 / ⇧F4, ⌥Space, ⌘PageDown / ⌥⌘↓, ⌘PageUp / ⌥⌘↑, ⌘C, ⇧F10 (row menu);
selection: click, ⌘-click, ⇧-click, ⇧↑ / ⇧↓, ⌘A.

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

## Confirm, run, and result (`sync-run.svelte.ts`)

Phases: `idle` (the grid) → `confirming` → `running` / `paused` → `finished` → `idle`.

- **Confirm.** ⌘↩ or Synchronize (only on a fresh, finished compare) reads `getSyncConfirm` with the Settings as they
  are now and keeps both. `SyncConfirmDialog` (soft dialog `sync-confirm`, `alertdialog` when anything warns) shows, in
  order: why it's shown again (`planChanged`, `staleCompare`), the "Older replaces newer" rows with both dates, the `?`
  rows the person gave an action (`chosenUnknown`, by name, same row shape), the groups with a checkbox each and where
  each group's replaced or removed files go (Trash, or the versions folder path; "slower" when `keepIsSlow`), hidden
  actions, a clock skew, then every guard in words with its second yes as a checkbox ("Remove anyway", "Copy anyway").
  Focus lands on the first group: a focused Synchronize would take Enter past `enterMayConfirm` (`ModalDialog` lets a
  focused button have Enter).
- **Holds** (`groupHold`, mirroring the backend's `gate`). A missing sentinel holds every group, an empty root every
  removal: such a group is shown disabled with the guard's reason and never ticked (`isHardHeld`). Removals past the
  limit wait for "Remove anyway", copies into an empty root for "Copy anyway". `confirmBlock` turns Synchronize
  `aria-disabled`, with the reason under the groups, while nothing runnable is ticked, every group is held for good, or
  a ticked group waits for its second yes ("61 files wait for Remove anyway. Tick it, or untick that group.").
- **Enter.** `enterMayConfirm`: something ticked, nothing ticked held for good, no removal group ticked, no replace
  group ticked while older-replaces-newer rows exist, and no acknowledgement needed. Otherwise Enter says to press
  Synchronize. These rules only decide what the person must press: the backend's `gate` decides what runs.
- **Accept.** `startSyncRun` with the preview's `revision`, the ticked groups, the acknowledgements, and the Settings
  the preview was read with. `planChanged` → the preview is read again, ticks kept for groups still there, and the
  confirm says why; `staleCompare` → "Compare again" (closes the confirm and compares); any other refusal is shown by
  variant. Nothing is retried unseen. The backend spawns the run, so its progress and even its report can overtake the
  answer to `startSyncRun`: they count from the moment the call goes out (`starting`), and a report that came first
  leaves the phase `finished`. A confirm marked `staleCompare` never sends again.
- **Run.** `sync-run-progress` of this session feeds one bar (bytes, else items) with items, bytes, and an ETA from the
  bytes rate so far. Pause and Cancel reach the backend at once but take effect between steps (a step is one engine
  batch); the panel says so rather than pretending: "Pausing after this step…" until a step ends while paused, and
  Cancel turns into a disabled "Stopping…" until the report lands. The ETA leaves out time spent paused. While a run is
  up, Esc and × only say "pause or cancel first", the grid's keys rest, and menu claims swallow their key (declining
  would run the main window's menu item).
- **Result.** `sync-run-finished` ends it and refreshes the summary (now `stale`). Lines per typed end (`run-result.ts`
  over `RunItemResult`, plus `heldByGuard`: rows a guard held never reach the run, copies included), the folders the run
  leaves (`emptyFolders`, `leftoverFolders`: "these stay"), the versions folder with "Show in Cmdr" (closes the dialog,
  then `explorerRef.navigate` opens it in the pane on its volume, the right when unsure), "Show in grid" (back to the
  stale list with every filter on and the rows that didn't run selected, guard holds too), and "Compare again".
  Decision: the re-compare (TC's `SyncRecompareAfterCopy`) runs when the person leaves the result, not at once: "Show in
  grid" needs the plan the run's row indices belong to, which a re-compare would replace. Esc on the result compares
  again too.

## Profiles (`profiles.svelte.ts`, `SyncProfilesControl`)

The footer's `[Profiles ▾ <name>]` (F2; ⌘S opens Save as), the multi-rename presets pattern: numbered 1–9, each with
Rename, Move up, Move down, Delete, then Save as profile. The palette's "Synchronize folders: profiles…"
(`sync.openProfiles`, no key) opens the dialog with the menu up.

- **Save as** keeps both roots (volume id and path, never a credential), the options, the show-filters, the overrides,
  and "Restore the show-filters when loading". A taken name (trimmed, any case) answers `nameTaken` first; the popover
  says so and the button becomes Replace, which saves over that profile keeping its id and place. A rename onto a taken
  name does the same through `saveSyncProfile` (the store's `rename` refuses `nameTaken`).
- **Load** sets both roots (no pane selections), options, overrides, the name (for `{profile}`), and the show-filters
  only when the profile asks, then compares. A volume that isn't connected is the compare's `volumeNotConnected`; the
  dialog says which side and offers "Use the left/right pane's folder" (`usePaneFolder`). Phase 2: the sign-in sheet.
- **Invalid profiles** are listed with their problem in the label ("Old backup: this version of Cmdr can't read it");
  picking one says why in a toast, and its submenu still renames, moves, or deletes it.
- **Refusals** come back typed (`ProfileAnswer`): a full list and a profile that fails the checks keep the popover open
  with why; a file that couldn't be written, or anything untyped, is a toast.

## Compare races

- The first compare opens the session; a compare asked for before it answered waits for it, so both land in one session
  (no orphan). A session that answers after the dialog closed is closed at once.
- A compare's refusal counts only if no newer compare was asked for since (`requestSeq`).
- A ⇧-click range reads rows page by page; if the view changes on the way, the range is dropped and the notice says
  "select them again" instead of mixing rows of two views.

## Not yet here

Root paths are shown, not edited. The "exact one-hour shift" toggle follows Settings, and a profile's overrides are kept
but not edited here (the profile editor with inherited values greyed is phase 2). Per-profile palette commands are phase
2 (`COMMAND_IDS` is static). The confirm doesn't list the empty folders a run would create.
