# Live transfer tree

**A copy of six folders shows one bar for the whole job, so the user can't tell which folder is untouched, which
`index.html` is in flight, or whether folders 1–3 landed.** One shared, live tree of the selection, with per-folder and
per-file progress, in the progress dialog and inline in the queue window. Issue:
[#382](https://github.com/vdavid/cmdr/issues/382). Design comment draft for the issue:
`transfer-tree-progress-issue-comment.md`.

Researched on `f1aac1c90` (2026-10-11). Jiri's closed PR [#395](https://github.com/vdavid/cmdr/pull/395) is NOT a
dependency; nothing from it is needed for v1.

## UX decisions (David, 2026-10-11)

1. **Rows are the selection, shown as a tree.** Root = the folder the transfer started from; every label is relative to
   it. Folder rows: own small bar plus "312 / 1,000 files · 1.2 / 4.0 GB". A file in flight: a small per-file bytes bar.
   Row states: waiting, active (copying / moving / compressing, worded per op), done (checkmark + final counts),
   skipped, failed (reason from the typed error pipeline, ❌ never new prose).
2. **Compact folders**: a single-child folder chain is one row (`a/b`), like VS Code and GitHub. A file child never
   compacts into its parent. Deterministic, no heuristics.
3. **The tree follows the work**: folders holding the work auto-expand, and a folder auto-collapses once the work has
   left it. A folder the user toggled stops auto-toggling. A user scroll or keyboard move pauses following and shows a
   "Follow" button (log-viewer style).
4. **Virtual scrolling, everything reachable**: 5,000 top-level files is a 5,000-row list. Finished rows stay (with a
   checkmark), ❌ never folded into "N done".
5. **Header**: today's dual bars (files + size), plus "N left", speed, and ETA. Shared with the queue through
   `TransferProgressReadout.svelte`.
6. **Folder bars follow the operation's bottleneck axis**: the backend already computes
   `ETA = max(ETA_bytes, ETA_files)`; every folder bar fills along whichever axis dominates (with hysteresis), the same
   axis on every row, and the bar's accessible name says which. File rows are bytes-only.
7. **Parallel transfers** (the SMB concurrent driver and the shared `FileWindow`): several active file rows at once, in
   whatever folders they sit.
8. **The dialog is resizable**, ~8 tree rows by default.
9. **Queue**: the tree sits inline under each running row, in the same card, indented, always visible (no chevron to see
   current work). Waiting rows below read clearly as separate items. With 2+ running operations, each running row's tree
   gets a collapse toggle (default open).
10. **Keyboard-first and a11y**: a proper ARIA tree (Up/Down, Left/Right collapse/expand and parent/child, Home/End),
    the Follow button in Tab order, AA contrast in both themes, `prefers-reduced-motion` respected.
11. **Out of scope**: "Skip the rest of this folder"; reusing the tree in the finished summary (#261); delete and trash
    trees; archive edits INTO an existing zip (`archive_edit` mutator ops). All noted as follow-ups below.

## What the code does today (verified on `f1aac1c90`, by reading the source)

- **`WriteProgressEvent.current_file` is a bare file name, one value** (`types/events.rs`); `activity.in_flight` is only
  a count. No in-flight set crosses IPC.
- **Per-source completion**: `write-source-item-done` comes from local copy/move, same-volume move (`move_same.rs`),
  delete, trash, bulk rename, and archive move-out. ❌ NOT from the volume copy drivers (`copy_serial.rs`,
  `copy_concurrent*.rs`) or the cross-volume move (`move_cross.rs`), so an SMB/MTP copy never deselects gradually.
- **Volume scans keep only per-TOP-LEVEL totals** (`BatchScanResult::per_path`). Nested folder totals exist nowhere.
  Every production scan already calls `ScanBoundary::file(size)` per leaf and `dir()` per listing (`scan_walk.rs` for
  ADB/SFTP/WebDAV/git/S3, plus SMB's and MTP's own walks), but without a path.
- **Per-leaf byte accounting already exists** for every volume path: `transfer_driver/progress.rs`'s `LeafProgress` (one
  RAII handle per file, high-water bytes, `complete` / `Drop` / `skip_leaf`), minted at exactly three sites:
  `volume/strategy.rs` (top-level file), `volume/merge.rs::copy_leaf`, `volume/sequential_extract.rs`.
- **The queue CAN run several operations at once**: admission budgets one op per LANE (`manager.rs`), so ops on disjoint
  lanes (local → NAS while phone → local) run concurrently.
- **Compress** (`archive_edit/fresh_*`) plans a full entry list up front (`fresh_plan.rs`, an explicit-stack DFS in
  listing order), then one serial producer (`fresh_zip.rs::produce`) reads entry by entry and reports `entries_done` +
  source bytes. A remote destination uploads the finished zip as step 2 of 2.

### Processing order today (the "upcoming rows must not lie" question)

- **Local copy/move (sync engine)**: ONE flat file list for the whole selection, sorted by the PANE's sort column on the
  FILE NAME alone (`scan_source_tracker.rs::sort_files`, `name_lower()`). So with name sort, `b/aaa.txt` copies before
  `a/zzz.txt`: folders interleave, and every folder finishes near the end. (This is also why gradual deselection is
  mostly a big bang at the end today.)
- **Volume serial**: sources in the order given; each folder walked depth-first in the backend's RAW listing order
  (unsorted on SMB and MTP); file children go to the operation-wide `FileWindow` (width up to 32), so they START in
  listing order and FINISH in any order.
- **Volume concurrent**: up to `W` top-level sources in flight, spawned in order.
- **Solid-archive extract** (`sequential_extract.rs`): the archive's member order, fixed by the stream.
- **Compress**: plan order (DFS, listing order per level).

**Decision: one canonical TREE ORDER, and every driver STARTS files in it.** Folders first, then files, each group by
the panes' name collation (`listing/collation.rs::NameCollator`), depth-first; top-level sources in the same order. The
tree displays exactly this order, so "rows below the active one" really are upcoming. Concurrency makes files FINISH out
of order, which is honest (each row claims only its own state). The one exception is a solid-archive extract, which
follows the archive's member order: its rows still show only their own state, and the exception is documented where the
extractor lives. **Why**: any other choice makes the tree lie or makes the display unpredictable. A tree-ordered copy
also finishes folders one by one, which is what the panes' gradual deselection and the user's "did folders 1–3 land"
both want. **Consequence David should know**: the pane's sort column stops affecting copy order (today it only did on
local copies, and only as a flat filename sort). `sort_column` / `sort_order` leave the transfer configs.

## Backend data model

New module `apps/desktop/src-tauri/src/file_system/write_operations/transfer_tree/` (colocated `CLAUDE.md` +
`DETAILS.md`): `model.rs` (arena + mutation), `build.rs` (builders), `order.rs` (the comparator), `view.rs` (flattening
for the view command), tests beside each.

### The arena

- `TransferTree` is an append-only arena of nodes, `NodeId(u32)` = index, root = 0. Ids are stable for the operation's
  life; the frontend keys rows and toggles by them.
- Compact per node: `parent: u32`, name as `(offset: u32, len: u16)` into ONE shared `String` arena, `kind: u8` (file /
  folder), `state: u8`, plus a `u32` index into either `file_sizes: Vec<u64>` or `folders: Vec<FolderAgg>`. `FolderAgg`
  = `files_total`, `bytes_total`, `files_done`, `bytes_done` (landed + skipped), `skipped`, `failed` (descendant
  counts). Children in CSR form (`child_offsets` + `child_ids`, sorted by tree order) built once at scan end; a child
  discovered later goes to a side map appended after the sorted siblings (rare: a file that appeared mid-copy, or an
  opaque subtree, below).
- **Budget: ≤ 64 bytes per node on average, names included**, pinned by a test that builds a 1M-node synthetic tree and
  asserts `heap_bytes() / nodes`. That's ~45 MB for a 1M-file job (the local engine's own `Vec<FileInfo>` already holds
  ~4× that). Built only for copy, move, and compress; ❌ never for delete or trash.
- **Root**: the sources' common parent. If sources have different parents (a search-results selection), root = their
  longest common ancestor, and each top-level row's label is its path relative to it (`photos/2024/IMG_1.jpg`); ❌ no
  synthesized intermediate folder rows, since those aren't part of the selection.
- **Opaque subtrees**: a scan path that reports a subtree's totals without walking it (`ScanBoundary::subtree`, the
  default `scan_for_copy_batch_with_boundary`) yields a folder with known totals and unknown children; the driver's live
  listing fills the children in as it walks (`tree.discover(parent, name, kind, size)`). Same mechanism covers a file
  that appeared after the scan.
- **Ownership**: the tree hangs off `WriteOperationState` (`OnceLock<Arc<TransferTree>>`, set after the scan), so it
  goes away with the state at `on_settled`. Mutations take one `Mutex` per file BOUNDARY (begin / complete / skip), ❌
  never per chunk: an active file's bytes live in the `LeafProgress` handle's existing high-water atomic, which the tree
  reads at view time.

### Building it

- **Local engine**: from the scan's `Vec<FileInfo>` + `dirs` after the walk. Each `FileInfo` gains `tree_node: NodeId`,
  so the driver never looks a path up.
- **Volume engine**: `ScanBoundary` becomes path-aware (`file(path, size)`, `dir(path)`) and optionally carries a
  `ScanRecorder` (trait object in `cmdr-fs`, no `tauri`) that receives `(path, kind, size)`. The compiler walks every
  call site (~16: `scan_walk.rs`, SMB `scan.rs`, MTP `scan.rs` + `bulk_ops.rs`, the trait default), so no backend can
  forget. `local_posix` and `cmdr-archive` scans feed it too; anything still on the default path yields opaque nodes.
  The app-side oracle walk (`scan::scan_subtree_with_oracle`) records the entries it already lists.
- **Preview reuse**: the scan preview builds the tree when its request says the dialog is a transfer, and
  `CachedScanResult` carries it to the operation, exactly as `per_path` travels today. A cache miss re-scans and builds
  it there.
- **Compress**: from `FreshPlan::entries` (entry names are already root-relative paths). The plan's DFS sorts each level
  by tree order before pushing.

### How drivers report (one hook per unit of work)

- **Volume**: `SourceProgress::begin_leaf(node)` → the `LeafProgress` handle IS the row's lifetime: begin marks the row
  active, `on_chunk` already updates the high-water bytes the view reads, `complete` marks it done and adds to every
  ancestor's `FolderAgg` (O(depth)), `Drop` without `complete` puts it back to waiting (a cancel or a failed attempt;
  bytes withdrawn, exactly as the bar does today), and `skip_leaf(node, …)` marks skipped. The merge walker resolves a
  child's node through a per-level name index built when it lists that level (`tree.level_index(parent)`, dropped with
  the level), ❌ no global path map.
- **Subtree-at-once paths**: the async driver's top-level Skip, the bulk pre-known-conflict skip, a same-volume
  rename-merge (`move_same.rs`), and a local same-FS move each mark a whole subtree done or skipped in one call.
- **Local sync engine**: `copy_single_item`'s per-file entry and exit (with its `FileVerdict`), and the chunked-copy
  byte callback, through a small `TreeLeaf` handle shaped like `LeafProgress`.
- **Compress**: `produce` reports the entry index it starts and finishes; one active row at a time.
- **Cross-FS move**: the tree tracks the COPY phase; the source sweep that follows is a phase in the header, and rows
  stay done.
- **Failure**: `WriteErrorEvent::new` (the one place every `write-error` is built, already reading live status for
  `progress_at_stop`) marks the node at the error's path `failed` with the typed `WriteOperationError`, through one
  typed path accessor on the error (❌ never parsing a message). The row is visible while the error is on screen;
  keeping it past settle belongs to #261.
- **Per-source completion for every driver**: the volume copy drivers and `move_cross.rs` start emitting
  `write-source-item-done` (Done / Skipped / Failed, with the last-event-wins rule), so SMB/MTP copies deselect
  gradually too. The tree doesn't need the event (it derives folder state from its leaves); the panes do.

### Folder state, derived at read time

- Waiting: nothing landed, nothing active beneath. Active: anything active or landed beneath, not finished. Done: every
  descendant settled and at least one landed (skips shown as a count: "Done · 3 skipped"). Skipped: every descendant
  skipped. Failed: a descendant failed. An empty folder is done once the driver has created it.
- The view adds in-flight bytes of active descendants to `bytes_done` at read time, so folder bars move smoothly between
  file completions.

### The bottleneck axis

`eta.rs` already computes `eta_bytes` and `eta_files`. `EtaStats` gains `bottleneck: ProgressAxis` (`Bytes` / `Files`)
with hysteresis (the other axis must lead by more than 10% for 3 s of working time before it flips; `Bytes` while
warming up), and `enrich_progress` puts it on `WriteProgressEvent` as `bottleneck: Option<ProgressAxis>`. One decision,
one place, the same answer on every surface.

## IPC

**Decision: no new event. One command, pulled on the existing push.** Each view calls
`get_transfer_tree_view(operationId, request)` on mount (the mid-job snapshot) and again whenever its session's
`write-progress` tick advances, with at most one request in flight per view (a tick that lands while one is pending only
marks it dirty). A scroll or toggle asks once per animation frame. **Why**: a broadcast tick can't know what each view
has expanded or scrolled to, so it would either flood every window with rows nobody shows or need per-view backend
registrations that leak when windows close. Pull-on-push keeps the backend stateless per view, bounds every payload by
the viewport, and makes attach and update the same code path. `write-progress` stays the push ("subscribe, don't poll"),
❌ no timer.

```rust
struct TransferTreeViewRequest {
    expanded: Vec<u32>,   // rows the user expanded by hand (≤ 2,000)
    collapsed: Vec<u32>,  // rows the user collapsed by hand (≤ 2,000)
    window: TreeWindow,   // Follow { lead: u32 } | At { offset: u32 }
    limit: u32,           // ≤ 200
}
struct TransferTreeView {
    ready: bool,              // false while scanning: no tree yet
    revision: u64,            // bumps on every mutation batch; equal → nothing changed
    total_rows: u32,          // flattened row count under the effective expansion
    offset: u32,              // where `rows` starts (chosen by the backend under Follow)
    rows: Vec<TransferTreeRow>,
    focus_row: Option<u32>,   // flat index of the topmost active row
    axis: ProgressAxis,
}
struct TransferTreeRow {
    id: u32, depth: u16, label: String,          // label is compacted: "a/b"
    kind: TreeRowKind, state: TreeRowState,
    expanded: bool, has_children: bool,
    pos_in_set: u32, set_size: u32,              // for aria-posinset / aria-setsize
    files_done: u64, files_total: u64,           // folders; 1/1 or 0/1 for files
    bytes_done: u64, bytes_total: u64,           // in-flight bytes included
    skipped: u32, failed: u32,                   // descendant counts (folders)
    failure: Option<WriteOperationError>,        // failed rows only
}
enum TransferTreeViewError { UnknownOperation, NotATransfer }
```

- **Effective expansion is the backend's call**: `(ancestors of the focus files ∪ expanded) − collapsed`. The focus
  files are the active ones, or, between two files, the last one started (so a folder doesn't flicker shut in the gap
  between the local engine's files). That makes "auto-expand while the work is inside, auto-collapse once it left, a
  hand-toggled row stays put" fall out of one set expression, with no auto-state on either side.
- **Compaction happens at flatten time**: a folder whose only child is a folder merges into one row, labeled with the
  joined names and keyed by the deepest id. Query-time compaction stays right when a discovered child breaks a chain.
- **Flattening cost is O(|expansion| + limit)**, never O(children): visible-subtree sizes are summed over the expanded
  set only, and the offset is found by descending through them. A 1M-row flat list answers a window in microseconds
  (pinned by a test with a time budget generous enough for CI).
- Payload: ≤ 200 rows of ~150 bytes, a few KB per view per tick at most 5/s.

## Frontend architecture

New feature dir `apps/desktop/src/lib/file-operations/transfer-tree/` (with `CLAUDE.md` + `DETAILS.md`):

- `TransferTree.svelte`: the ARIA tree over a virtual window, the Follow button, the empty / scanning states. Props: an
  operation session (for ticks and the op type) and a `TransferTreeSource`, plus `defaultRows`.
- `TransferTreeRow.svelte`: icon (lucide via `unplugin-icons`; state glyphs via `StatusGlyph` where it fits), label with
  middle truncation + `use:tooltip` for the full relative path, the small bar (`ProgressBar` primitive), and the count
  text.
- `transfer-tree-view.svelte.ts`: per-view state: the manual `expanded` / `collapsed` sets, `following`, the scroll
  offset, the last response, and the request coalescer. ❌ No tree model here: the backend owns it.
- `transfer-tree-source.ts`: the `TransferTreeSource` interface (`view(request) → Promise<TransferTreeView>`), the
  IPC-backed implementation (`tauri-commands/write-operations.ts` wrapper), and nothing else.
- `row-format.ts`: pure helpers: bar fraction by `axis`, the count line, the row's accessible name.
- `fixtures/`: scripted sources that replay a timeline of views (no IPC), for the Debug catalog and tests.
- Virtualization reuses `file-explorer/views/virtual-scroll.ts`'s `calculateVirtualWindow` with a fixed row height (the
  `SuggestedOpsDialog` pattern: data in the backend, the frontend fetches only its window).
- **Follow mechanics**: programmatic scrolls are tagged, so only a user scroll, wheel, scrollbar drag, or keyboard move
  inside the tree flips `following` off. Follow → `window: Follow { lead: 2 }`; the button and its shortcut (a letter
  key while the tree has focus, decided with the strings in M4) flip it back on.
- **ARIA**: `role="tree"`, rows `role="treeitem"` with `aria-level`, `aria-expanded` (folders), `aria-posinset` /
  `aria-setsize` (from the response, because virtualization hides the siblings), roving `tabindex`, and an accessible
  name like "Photos 2024, copying, 312 of 1,000 files, 1.2 of 4.0 GB". The focused row is kept rendered even when
  scrolled off the window. ❌ No `aria-live` on rows (a 5 Hz tree would talk over everything); the header readout keeps
  its existing announcements.
- **Strings**: every label and accessible name in `messages/en/fileOperations.json` with `@key` descriptions; the
  active-row verb keyed by op type (copying / moving / compressing). Translations come from translator agents
  (`.claude/rules/translations-by-a-translator-agent.md`) in the SAME milestone, because `desktop-i18n-coverage` is an
  error-level gate.

### The dialog (`TransferProgressDialog.svelte`)

- The tree renders below the readout for copy, move, and compress once `ready`; delete, trash, and archive edits keep
  today's layout (the command answers `NotATransfer`).
- `ModalDialog`'s existing `resizable` prop, on only when the tree shows; the tree region is the flex child that grows,
  with a 3-row minimum and an 8-row default. The size isn't persisted in v1.
- The readout gains "N left" (`filesTotal − filesDone`, noun from `countKind`) on both surfaces.
- A single-file transfer still shows its one row: the layout stays the same for every transfer, and the row carries the
  relative path the header doesn't. (Worth a look in the gallery review.)

### The queue (`QueueRow.svelte`)

- A running row (`running` / `paused`, phase past scanning) renders the tree inside its card, indented under the
  readout, ~6 rows by default. A queued or failed row renders none.
- With 2+ running rows, each running row gets a collapse toggle (`Button`, `aria-expanded`), default open. One running
  row: no toggle.
- Each row's view binds its own session through `bindOperationSession` (the existing rule) and its own
  `transfer-tree-view` state; attaching mid-job is the first `view` call.

## Bounds for a 1M-file job

- Backend memory: ≤ 64 B/node (test-pinned), freed at settle. No per-view backend state.
- IPC: ≤ 200 rows per response, ≤ 5 responses/s per view, request inputs capped (2,000 + 2,000 ids, clamped by the
  frontend, which drops the oldest hand toggles beyond that).
- DOM: the virtual window only (~10–40 rows).
- CPU: mutation O(depth) per file boundary; view O(|expansion| + 200).

## Test strategy

- **Rust unit**: tree order comparator (folders first, collation, multi-parent roots); arena build from `FileInfo` and
  from a recorder; compaction (`a/b/c/1` → `a/b/c` + `1`; a discovered sibling breaks a chain); folder state derivation;
  flatten + offset + Follow window over 5,000 flat files and 5 × 1,000; the 64 B/node budget at 1M nodes; the bottleneck
  hysteresis (`eta_tests.rs`).
- **Rust integration** (existing test sinks and `InMemoryVolume`): a local copy STARTS files in tree order whatever the
  pane sort was; a volume merge whose listing comes back shuffled still starts files in tree order; a concurrent copy
  shows several active rows in different folders, and every row ends done; skip-all, a mid-flight skip, a failure that
  names its file, and a cancel leaving rows waiting; per-source `write-source-item-done` from the volume copy and
  cross-volume move (pin the last-event rule); compress rows follow the plan.
- **Vitest**: `row-format.ts`; the view state's coalescer (one in flight, latest wins), the follow flag (user scroll
  pauses, programmatic doesn't, Follow resumes), toggle bookkeeping; `TransferTree.svelte` against scripted sources
  (keyboard: Up/Down, Left/Right, Home/End; posinset/setsize; Follow button appears and works).
- **a11y tier**: `transfer-tree.a11y.test.ts` (axe over each fixture, both themes), and the existing
  `transfer-dialogs.a11y.test.ts` / `QueueRow.a11y.test.ts` extended.
- **Fixtures** (scripted sources, shown in Debug → Components so David can review them live): single file; 5,000
  top-level files; 5 folders × 1,000; deep `a/b/c/1` with compact folders; parallel SMB (8 active rows across 3
  folders); failed and skipped rows. The `transfer-progress` gallery row (today `not-triggerable`) becomes a
  real-operation row over dev fixture trees, slowed by the existing copy throttle (`test_mode.rs`), for the local
  shapes; the parallel and failed shapes stay catalog-only, and the row's `reason` says so.
- **Playwright E2E** (`transfer-tree.spec.ts`): a throttled local copy of 5 × 20 files: the tree appears, the active row
  sits in the expected folder, earlier folders collapse with a checkmark, a wheel scroll shows Follow and Follow brings
  the active row back, Left/Right collapse and expand. Plus a line in `operation-queue.spec.ts`: a backgrounded copy's
  queue row shows its tree. `dialog-resize.spec.ts` covers the new resizable dialog.

## Milestones

Each one is green on `pnpm check` alone and commits on its own. **Big** ones get an `--include-slow` run after.

1. **Tree model and scan recording** (big). `transfer_tree/` (arena, comparator, builders, compaction, flatten, budget
   test), the path-aware `ScanBoundary` + `ScanRecorder` across `cmdr-fs` and every scan, `CachedScanResult` carrying
   the tree, the tree on `WriteOperationState` for copy/move/compress. No driver or UI change yet. Done when: every
   backend's scan builds a correct tree in tests, and the 1M budget test passes.
2. **One processing order** (small). Local sort → tree order; the merge walker sorts each level; the concurrent and
   serial volume drivers sort sources; the compress plan sorts levels; `sort_column` / `sort_order` leave
   `WriteOperationConfig` / `VolumeCopyConfig` and the frontend dispatch (and the delete dialog's scan request, which
   passes them to the same scan). Done when: the order tests pass on every driver, and the transfer and volume
   `DETAILS.md` describe the order and the solid-archive exception.
3. **Drivers report, and the view command** (big). `LeafProgress` ↔ node, the sync engine's `TreeLeaf`, subtree-at-once
   paths, compress, failure marking, `write-source-item-done` from the volume copy and cross-volume move, the bottleneck
   axis, `get_transfer_tree_view` + typed error + bindings. Done when: the integration tests above pass and an MCP/dev
   call against a running copy returns sane windows.
4. **The shared component** (big). `transfer-tree/` (component, row, view state, source, `row-format.ts`, fixtures), the
   Debug Components section with all six fixtures, strings + translations. Done when: Vitest, a11y, and keyboard tests
   pass on every fixture, in both themes.
5. **The dialog** (small). Embed in `TransferProgressDialog`, resizable with the 8-row default, "N left" in the readout,
   compress included; update the dialog tests and `dialog-resize.spec.ts`. Done when: a real local and SMB copy show a
   following tree in `pnpm dev`.
6. **The queue** (small). Inline tree for running rows, the 2+ running collapse toggle, `queue-window.ts` sizing; update
   `QueueRow` tests. Done when: two concurrent ops on different lanes each show their own tree.
7. **E2E, gallery, and the doc sweep** (big). `transfer-tree.spec.ts` and the queue line, the real-operation gallery
   row, the i18n capture including a mid-copy tree, `docs/architecture.md`'s map line, and every touched `C+D.md`. Done
   when: `--include-slow` is green and the spec's durable intent lives beside the code (ready to wipe this spec).

## Docs to update

- New: `write_operations/transfer_tree/CLAUDE.md` + `DETAILS.md`, `file-operations/transfer-tree/CLAUDE.md` +
  `DETAILS.md`.
- `write_operations/DETAILS.md` (events list, "Per-source outcomes" now covering the volume drivers),
  `transfer/DETAILS.md` + `volume/DETAILS.md` (the order, the solid-archive exception), `transfer_driver/DETAILS.md` (a
  leaf handle is its row), `archive_edit/DETAILS.md` (compress rows), `crates/cmdr-fs/DETAILS.md` (the path-aware
  boundary and recorder), `tauri-commands/DETAILS.md`, `file-operations/transfer/DETAILS.md`, `queue/DETAILS.md`,
  `dialog-gallery/DETAILS.md` (the real-operation row), `docs/testing.md` if a new hook appears, `docs/architecture.md`
  (one map line).

## Risks

- **Memory on huge remote scans**: the recorder holds what volume scans never held. The budget test and a 1M-node
  measurement in M1 decide; the fallback is folder-only recording past a node cap, with files discovered live.
- **The order change touches every driver**: isolated in M2 so it bisects cleanly; the merge invariant suites
  (`safety_grid_tests.rs`) and the conflict prompt order tests must stay green.
- **`ScanBoundary` signature churn across five crates**: mechanical, compiler-guided, in one milestone.
- **Follow jitter**: a parallel window spread across folders could thrash expansion. The focus rule (topmost active row)
  and the sticky last file keep it stable; the parallel fixture is where David judges it.
- **Lock contention on many tiny local files**: one uncontended mutex per file boundary; measure in M3 against the local
  copy benches before and after.
- **i18n gate per milestone**: string-adding milestones (4, 5, 6) need a translator fan-out before their commit.
- **Queue window height**: inline trees make running rows tall; `queue-window.ts` sizing and the 6-row default may need
  David's eye.

## Follow-ups (issues to file when this ships)

- "Skip the rest of this folder" (needs a per-subtree cancel token in both engines).
- Keep the tree after the operation ends, for the finished summary (#261) and a failed queue row's details.
- Trees for delete and trash, and for archive edits into an existing zip.
- Expose the active rows to MCP (`cmdr://state` operation rows) so an agent can say which folder is in flight.
- Persist the dialog's resized height.
