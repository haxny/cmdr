# Synchronize folders (dialog)

⇧F12: Total Commander's Synchronize directories. The two panes' folders, compared by the backend (`src-tauri/src/sync/`,
`crates/cmdr-sync`), shown as an arrow grid you review and override row by row.

## Module map

- `SyncFoldersDialog.svelte` (shell, keys) over `SyncOptionsStrip`, `SyncFilterBar`, `SyncGrid`, `SyncSummaryBar`.
- `sync-session.svelte.ts`: the backend session (compare, pages, actions, close); `sync-ipc.ts` its app seams.
- `sync-keys.ts` (what a key means), `sync-grid-controller.svelte.ts` (what it does), `sync-row-menu.ts`.
- `row-label.ts` (words for a row), `action-refusal.ts`, `sync-errors.ts` (`SyncError` in words), `pane-roots.ts`,
  `sync-row-files.ts` (F3/F4 paths), `sync-menu-claims.ts`, `sync-options.ts` (defaults mirrored from `cmdr-sync`).

## Must-knows

- **Only the newest compare speaks.** Ignore events of another session or generation; an early one waits for
  `startSyncCompare` to answer.
- **Options never re-compare on their own**: they mark the grid stale ("Compare again"). Show-filters never compare;
  they re-page the same plan.
- **Hidden rows never change.** Bulk keys send the selection (else the cursor row); the backend keeps only the visible
  rows. ❌ Never filter that client-side instead: the backend is the guard.
- **Never silent.** `set_sync_actions` answers the rows it left alone (by `RowSkipReason`) and the hidden ones; the
  notice says them (`actionsNotice`). ❌ Never re-derive legality here. A refusal shows by variant (`syncProblemText`).
- **Act only on the current view.** Kept rows draw while a page reloads; selection, keys, and F3/F4 read `currentRow` /
  `rowAt`. Late answers drop: a lower compare generation, a lower state `revision`.
- **Menu accelerators win in AppKit**: a key that is also a menu item's (⌘D ⌘I ⌘U ⌘C ⌘A ⌘⇧E F2 F3 F4 ⇧F4) is claimed
  while open (`sync-menu-claims.ts`), double fires dropped. ⌘M is Minimize, unclaimable: skip is ⌘N.
- **Never colour alone.** Every row has an accessible name in words (`rowAccessibleName`, via `ColumnList`'s
  `rowLabel`); arrows, icons, and marks repeat it.
- **Esc stops a running compare first, then closes.** Closing remembers the options for the pair and closes the session;
  a write that didn't land says so.
- **Keys match whole combos** and read letters off `code` (⌥ composes). Row keys only count on the grid, so the options'
  fields keep their arrows, `-`, Space, and ⌘⌫.

Flows, the key map as built, and decisions: `DETAILS.md`.
