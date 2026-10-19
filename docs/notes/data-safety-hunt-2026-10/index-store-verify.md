# Verify: drive index, store side (`index-store`)

Both excerpts are verbatim and current at `22918588b`.

### index-store-1

- **Verdict:** confirmed
- **Severity:** medium (agree)
- **Evidence (traced from `recursive_size` to the rendered dialog):**
  - `apps/desktop/src-tauri/src/operations_host/index.rs:30-36`: `recursive_size` keeps only `s.recursive_size` from
    `index().dir_stats(..)`; no `is_fresh` gate (unlike `search_leaves` at line 43) and no completeness check (unlike
    `expected_totals`).
  - `crates/cmdr-index/src/indexing/handle/mod.rs:541` → `read/queries.rs:325` `get_dir_stats_on_volume`: answers `Some`
    whenever a read pool exists and the path resolves to a row with `dir_stats`; only `dir_stats_from`
    (`queries.rs:421-433`) marks `recursive_size_complete` / `_stale` / `_pending`, all dropped at `index.rs:35`.
  - `crates/cmdr-operations/src/conflict.rs:356-360`: destination `size_for_dir = lookup_indexed_size(..)`
    (`durability.rs:301-302`, pass-through). `build_conflict_event` (`conflict.rs:609-617`) copies it into
    `destination_size` and `size_difference`.
  - `crates/cmdr-operations/src/types/events.rs:698-746`: `WriteConflictEvent` has no completeness, stale, or pending
    field. Its doc comment (line 714-719) promises `Some` means "covered by the drive index", nothing about a floor.
  - FE: the event goes straight into `TransferConflictDialog.svelte`; no surface re-queries the index (searched
    `operation-conflict.svelte.ts`, `operation-session/*.ts`; no `getDirStats`/`dirStats` use).
    `TransferConflictDialog.svelte:106-109` formats `destinationSize` with `formatByteSize`, and lines 233-238 render it
    plain; the only alternate rendering is `sizeUnknown` when it's `null`. No `≥`, stale, or hourglass marker exists in
    the component. The `(larger)` annotation (lines 240, 260) is computed from the same lower bound, so the incoming
    file can be badged "larger" than a far bigger folder.
- **Refutation attempts:** the cross-type consent rule (`conflict.rs:251-272`) does refuse
  `OverwriteSmaller`/`OverwriteOlder` across types, so the floor can't drive an automatic size-conditional replacement;
  only a person's plain Overwrite (or its latched "all") replaces. That limits it to a misleading cue at a consent
  point, under a red "replace a folder with a file" warning, which is why medium (not high) holds. No test pins a
  lower-bound size in the event.
- **Small correction to the hunter's trigger wording:** the dialog never says "smaller than the incoming file"; it
  badges the NEW side "(larger)". Same effect.

### index-store-2

- **Verdict:** confirmed (the hunter's untraced step now traced)
- **Severity:** low (agree)
- **Evidence:**
  - `apps/desktop/src/lib/file-explorer/pane/file-operation-commands.ts:720-727`: the pane delete path builds
    `DeleteSourceItem` from the `FileEntry` with `recursiveSize: e.recursiveSize` and drops `recursiveSizeComplete`
    (present on the wire, `bindings.ts:262`). `delete-dialog-utils.ts:13-20` has no field for it.
  - `DeleteDialog.svelte:326-328` and line 418 render `<Size bytes={itemSizeBytes(item)} />` plainly.
  - The search-results delete path (`file-operation-commands.ts:601-608`) sets `recursiveSize: undefined`, so it's
    unaffected.
  - Also feeds `dialog-state.svelte.ts:541-544` `itemSizes` for trash progress (progress only, no decision).
- **Why low:** the dialog runs its own walked scan preview (`DeleteDialog.svelte:99`, `createDeleteScanState`), so the
  headline totals are honest; only the per-row annotation can be a floor. No write depends on it.
