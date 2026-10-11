# Transfer tree: details

Read this before any non-trivial work here. Must-knows: `CLAUDE.md`. The plan this implements, until it ships:
`docs/specs/transfer-tree-progress.md`.

## Where a tree comes from

Every transfer's scan builds one, and the operation installs it on `WriteOperationState::transfer_tree`
(`TransferTreeSlot::install`, once) as soon as the scan is done:

- **Local copy and cross-FS move**: `ScanResult::order_by_transfer_tree` (`../scan_cache.rs`). The preview's tree when a
  `ScanPreviewPurpose::Transfer` preview built one (`../scan_preview/local.rs`, after the walk and off it), else built
  right there from the walk's `files` and `dirs`. Either way each `FileInfo` learns its `tree_node`, and `files` is
  sorted into the tree's order.
- **Volume copy and cross-volume move**: `scan_volume_sources` (`../transfer/volume/preflight.rs`) hangs a
  `TreeRecorder` on the scan's `ScanBoundary`. A cached preview's tree is taken as is; a cached preview without one
  (a delete's, never consumed here today) degrades to `from_per_path`: each source a file or an opaque folder.
- **Volume previews**: `../scan_preview/volume.rs` records both branches: the backend's batch scan through the
  boundary, and the fresh-listing oracle's walk (`../scan.rs::scan_subtree_with_oracle`) directly. A cached top-level
  child the oracle can't name reaches the recorder as a subtree, since that branch never walks it.
- **Compress**: `from_archive_plan` over `FreshPlan::entries`, whose names are already relative to the selection's
  parent (`../archive_edit/fresh_compress.rs`). `from_archive_plan` also returns each entry's node, in plan order.
- **Same-FS and same-volume moves** don't scan (they're renames), so they install no tree.

## The arena

- `Node` (16 B): `parent`, a name as `(offset, len)` into one shared `String`, `kind`, a leaf `state`, and a `slot`
  into `file_sizes` (files) or `folders` (`FolderAgg`, 40 B: the child range plus file counts and bytes below it).
- **Breadth-first ids** make a folder's scanned children `first_child..first_child + child_count`, so a child's
  position is a subtraction and the arena carries no child list. Discovered children live in `extra_children`.
- `heap_bytes()` counts capacity. 47.0 B/node at 1,000,002 nodes of 23-byte names (verified 2026-10-11, the budget
  test, debug build): 16 for the node, 8 for the size, the name, and the folders' share.

### Building

`TreeBuilder` takes nodes in any order. `finish` groups children by parent, sorts each group by `TreeOrder`, numbers
breadth-first, then sums totals in one descending pass (every child's id is above its parent's). `PathTree` (behind
`TreeRecorder` and `from_local_scan`) maps paths to nodes: the root is the sources' longest common ancestor, each source
is a top-level node labelled by its path relative to it (`photos/2024/IMG_1.jpg`, no synthesized folder rows), and a
path whose folder hasn't arrived yet creates the missing folders on the way up to a known one. Only folders are in its
path map, so it stays small for a million-file scan. A source the scan never reported stays a 0-byte file row.

### Order

`TreeOrder` (`order.rs`): folders first, then files, each by the panes' collator (`listing/collation.rs`), with the
collator's raw-bytes tiebreak so the order is total. `sort` builds one key per item; `compare` is the one-off form.
`TreeOrder::bytewise` (test builds only) keeps the shape without the collator, for the budget cells, where building a
million keys in an unoptimized build is the whole test budget.

The drivers start files in this order too (`../transfer/DETAILS.md` § "Processing order"), through three helpers:
`TreeModel::tree_order_ranks` (each node's place in a depth-first walk, for the local engine to sort its flat file
list), `sort_sources` (the selection, ranked by the same labels its top-level rows show), and `TreeOrder::sort` itself
for a walker's level.

## Mutations and folder state

One call per file boundary: `begin` (with the driver's live byte counter), `complete`, `withdraw` (stopped without
landing: back to waiting), `skip`, `fail` (keeps the typed `WriteOperationError`), `folder_created` (only an EMPTY
folder reads it), `discover`. A second report on a settled file is ignored, so nothing counts twice.

Folder state is derived at read time, never stored: failed if any file below failed; else, once every file is settled,
skipped if all were skipped, done otherwise; else active if anything below is in flight or landed; else waiting. An
empty folder is done once created.

## The view (`view.rs`)

`flatten(model, request)` answers one window of at most 200 rows:

1. **Effective expansion**: the folders holding the focus files (the active ones, or between two files the last one
   started, so a folder doesn't flicker shut in the gap), plus the user's `expanded`, minus their `collapsed` (each
   capped at 2,000). Ids are mapped to row keys first.
2. **Open rows**: expanded rows whose every ancestor row is expanded. Each gets a listing size (its children, plus every
   open child's size), computed deepest first.
3. **Locate** the window's first row by descending through the open children's sizes, then **walk forward** in tree
   order with a stack of `(parent row, position)`.

Rows are keyed by the deepest folder of a compact chain (`row_key`), labelled with the chain's names joined by `/`.
A row's `pos_in_set` / `set_size` come from its parent's child count, for `aria-posinset` / `aria-setsize`. A folder's
`bytes_done` adds the in-flight bytes of every active file below it (`in_flight_by_folder`, one pass over the active
set per request).

`Follow { lead }` puts the focus row `lead` rows from the top, clamped so the window fills; `At { offset }` clamps to
the row count. `focus_row` is the topmost focus file's row, or the shut folder hiding it.
