# Operation queue window

The standalone macOS window for running, waiting, and couldn't-finish operations. Opens from View > Operation queue
(⌥⌘Q) or the palette. Backend: `crates/cmdr-operations/CLAUDE.md`.

## Module map

- `queue-window.ts` opens the window (perms in `src-tauri/capabilities/queue.json`), `operations-store.svelte.ts` is its
  single reactive source, `QueueRow.svelte` a row (+ `QueueRowSummary.svelte`), shell (and keyboard)
  `routes/queue/+page.svelte`.
- Pure helpers: `failure-reason.ts` (a retained failure's title/explanation/suggestion), `queue-backlog.ts`
  (`hasOtherQueuedWork`, behind the progress dialog's Background/Queue label), `row-tree.ts` (which rows show the live
  transfer tree, and when they get a collapse toggle), `queue-row-menu.ts` (right-click menu), `queue-drag.svelte.ts` +
  `queue-flip.ts` (dragging).

## Must-knows

- **It's a HARD window** (a `WebviewWindow` on `/queue`, sibling to Settings), never a modal.
- **Progress is DEFINED with the copy dialog.** Rows render `../TransferProgressReadout.svelte` (its fixed columns set
  `MIN_WIDTH`), every ESTIMATE comes from the row's session (`bindOperationSession`), ❌ never the raw tick, and ❌ no
  smoother in the store (membership and the latest tick only).
- **Two streams, never poll**: `operations-changed` is the THIN membership + status snapshot (❌ don't fatten it),
  `write-progress` drives the bars, pruned to membership, seeded at mount (a paused op emits nothing).
- **Rows cover the instant ops too** (`rename` / `create_folder` / `create_file`): no `write-progress`, so a spinner +
  label. `operation-icon.ts` takes the SNAKE_CASE wire values.
- **A row with `snapshot.reverses` is an UNDO: name it from `../reversal-wording.ts`, ❌ never `operationType`**
  (undoing a copy runs as a delete, so it would read "Deleting"). Its status cell keeps the lifecycle word.
- **The bar-is-moving truth is the SNAPSHOT `status`** (the backend's `LifecycleStatus`), and the status column names
  that LIFECYCLE except where nothing else says the op stopped or reversed: an in-flight rollback, and an unanswered
  clash (`session.awaitingAnswer`, ❌ never a raw `progress.activity`). DETAILS § "A row parked on a clash".
- **A running, PAUSED, or queued row can be `phase: 'scanning'`**: compact `ScanPhaseBody`, ❌ no dual bar (totals 0) or
  Rollback; Pause/Resume DO show. DETAILS § "A scanning row".
- **A failed row STAYS until dismissed**: the page hides only `done` / `cancelled` (`isHiddenSettledStatus`, ❌ not
  `isTerminalStatus`). ❌ No timer, no window close. DETAILS § "Retained failures".
- **A failure's reason comes from the error pipeline, ❌ never new prose** (`failure-reason.ts`). Its `message` is
  MARKUP through `{@html}`; the pipeline's title is dropped in the row.
- **Show hands a row's operation back to the main window's progress dialog**, over `foreground-operation` carrying the
  id alone. ❌ Not a command on the operation, ❌ never offered on a `queued` row. DETAILS § Show.
- **A row commands its own operation through its session** (`../operation-session/CLAUDE.md`); the page keeps only the
  fleet actions and Dismiss.
- **Cancel keeps partials, no confirm; Rollback is the opt-in undo**, shown ONLY where `supportsRollback` and
  `reversalWindowClosed` allow it, ❌ never inferred from the type.
- **Window perms fail SILENTLY**: `await` every Tauri call in try/catch + `log.warn`; smoke-test with `pnpm dev` after a
  perm change.
- **The list is a one-Tab-stop ARIA grid; its cursor is an operation ID**, ❌ not an index. ⌘A arrives as
  `queue-select-all` from the menu, ❌ never a keydown. DETAILS § Keyboard.
- **Drag-to-reorder: pointer events and our own FLIP**, ❌ never HTML5 DnD or `animate:flip`. DETAILS § "Dragging to
  reorder".
- **A running row's live tree is its own widget**: Tab from the cursor row enters it, Escape returns. ❌ The grid hears
  no key inside `[data-queue-tree]` (⌫ there would cancel the whole copy). DETAILS § "The inline tree".
- **One opener, one store per webview**: `openQueueWindow`, plus the main window's own
  (`main-window-operations.svelte.ts`). ❌ Backgrounding never opens it (`queue-handoff-toast.ts`).

Depth (store API, failures, keyboard, decisions): `DETAILS.md`. Read it before non-trivial work here.
