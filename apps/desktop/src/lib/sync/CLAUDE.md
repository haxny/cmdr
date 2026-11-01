# Synchronize folders (dialog)

⇧F12: Total Commander's Synchronize directories. The two panes' folders, compared by the backend (`src-tauri/src/sync/`,
`crates/cmdr-sync`), shown as an arrow grid you review and override row by row.

## Module map

- `SyncFoldersDialog.svelte` (shell, keys) over `SyncOptionsStrip`, `SyncFilterBar`, `SyncGrid`, `SyncSummaryBar`.
- `sync-session.svelte.ts`: the backend session (compare, pages, actions, close); `sync-ipc.ts` its app seams.
- `sync-keys.ts` (what a key means), `sync-grid-controller.svelte.ts` (what it does), `sync-row-menu.ts`.
- `row-label.ts` (words for a row), `action-refusal.ts`, `sync-errors.ts` (`SyncError` in words), `pane-roots.ts`,
  `sync-row-files.ts` (F3/F4 paths), `sync-options.ts` (defaults mirrored from `cmdr-sync`).

## Must-knows

- **Only the newest compare speaks.** Ignore events of another session or generation; an early one waits for
  `startSyncCompare` to answer.
- **Options never re-compare on their own**: they mark the grid stale ("Compare again"). Show-filters never compare;
  they re-page the same plan.
- **Hidden rows never change.** Bulk keys send the selection (else the cursor row); the backend keeps only the visible
  rows. ❌ Never filter that client-side instead: the backend is the guard.
- **Never silent.** `bulk_changes` drops a row that can't take an action, so a key on ONE row is checked first
  (`rowRefusal`) and the notice says why; a backend refusal is shown by its variant (`syncProblemText`).
- **Never colour alone.** Every row has an accessible name in words (`rowAccessibleName`, via `ColumnList`'s
  `rowLabel`); arrows, icons, and marks repeat it.
- **Esc stops a running compare first, then closes.** Closing remembers the options for the pair and closes the session;
  a write that didn't land says so.
- **Keys match whole combos** and read letters off `code` (⌥ composes). Row keys only count on the grid, so the options'
  fields keep their arrows, `-`, Space, and ⌘⌫.

Flows, the key map as built, and decisions: `DETAILS.md`.
