# Transfer tree (frontend)

The live tree of a copy's, move's, or compress's selection: every file and folder with its own progress, as an ARIA tree
over a virtual window. The progress dialog and the queue window embed `TransferTree.svelte`. Backend contract:
`apps/desktop/src-tauri/src/file_system/write_operations/transfer_tree/CLAUDE.md`.

## Module map

- `TransferTree.svelte` (the tree, keyboard, follow, the Follow button), `TransferTreeRow.svelte` (one row),
  `transfer-tree-view.svelte.ts` (per-view state: hand toggles, following, scroll, the coalesced pull),
  `transfer-tree-source.ts` (the `view(request)` interface and its IPC implementation), `row-format.ts` (what a row
  says), `pinned-headers.ts` (which scrolled-past folders stick to the top). `fixtures/`: scripted transfers for Debug →
  Components → Transfer tree and the tests.

## Must-knows

- **❌ No tree model here.** The backend owns expansion, compaction, windows, and folder state; a view sends only its
  hand toggles and where it's scrolled. `fixtures/scripted-transfer.ts` mirrors the contract for tests only.
- **Pull on push, one request in flight** (`createCoalesced`): on mount and on each `tick`, ❌ never a timer.
  `unknown_operation` means "finished" (keep the last rows, stop asking); `not_a_transfer` renders nothing.
- **A scroll alone never pauses following**: the tree's own follow scrolls and the browser clamping a shrunk list fire
  the same event. INTENT pauses it: a wheel, a scrollbar press, a keyboard move.
- **Rows are slots keyed by flat index**, and the focused row stays rendered wherever it scrolled to; the cursor follows
  its row by id when rows shift above it. ❌ Never key slots by row id: a keyed move blurs the focused row.
- **Failure text is the typed pipeline's title** (`getUserFriendlyMessage`), ❌ never new prose.
- **Sizes in rows are plain text**, ❌ not `<Size>`: its tier colors turned a tree of them loud.
- **Folder bars fill along the view's `axis`**, file bars by bytes; only rows at work draw a bar.
- **Pinned headers are `aria-hidden` mirrors**, ❌ never treeitems: a second copy of a row confuses a screen reader,
  which reaches every parent with Left anyway. A row hidden under them counts as out of sight (`reveal`, `followTop`).
- **The height only grows** (`--spacing-tree-rows`, up to `defaultRows`): a folder shutting as the work moves on must
  not bounce the host's layout every tick.

Keyboard model, follow mechanics, the full-path tooltip, and decisions: `DETAILS.md`. Read it before any non-trivial
work here: editing, planning, reorganizing, or advising.
