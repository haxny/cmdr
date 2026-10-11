# Transfer tree

A copy's, move's, or compress's selection as a live tree, for the progress dialog and the queue to show which folder is
in flight, what landed, and what's next. The backend owns the model; a view asks for a window of rows.

## Module map

- `model.rs` (the arena and the per-file-boundary mutations), `build.rs` (`TreeRecorder` for a volume scan,
  `from_local_scan`, `from_archive_plan`, `from_per_path`, `sort_sources`), `order.rs` (tree order), `view.rs` (flattening into a
  window). `mod.rs`: `TransferTree` (the shared lock) and `TransferTreeSlot` (where `WriteOperationState` keeps it).

## Must-knows

- **≤ 64 B per node, names included**, pinned at 1M nodes by `budget_tests.rs`. ❗ `Node` is 16 bytes on purpose; grow
  it or add a per-node side vector and the budget test says by how much. ❌ Never a global path map in the model.
- **Ids are breadth-first, assigned once by `TreeBuilder::finish`**, so a folder's scanned children are one contiguous
  range in tree order. A child found later goes to `extra_children`, after them. ❌ Never renumber after `finish`:
  views key rows and hand toggles by id.
- **One lock per file BOUNDARY** (begin, complete, skip, fail), ❌ never per chunk: an active file's bytes are its
  driver's `Arc<AtomicU64>`, read at view time.
- **A view is O(|expansion| + limit), ❌ never O(children)** (`view.rs`). Effective expansion is `(ancestors of the
  focus files ∪ expanded) − collapsed`, computed per request; ❌ no per-view state on the backend.
- **Compaction happens at flatten time**: a folder whose only child is a folder shares its row, keyed by the deepest
  id. ❌ Never compact in the model: a discovered child breaks a chain.
- **An opaque folder's totals already count its children**: `discover` adds a found file's totals only up to the
  nearest opaque ancestor.
- **Paths reach the recorder in the caller's form** (`crates/cmdr-fs/DETAILS.md` § "The recorder"); an entry under no
  source is dropped, ❌ never guessed into place.
- **Built for copy, move, and compress only**, ❌ never delete or trash (`ScanPreviewPurpose`).
- **Every driver STARTS files in tree order** (`../transfer/DETAILS.md` § "Processing order"), so the rows below the
  active one are next. A new walk sorts each level with `TreeOrder`, ❌ never in listing or pane-sort order.

Layout, the view algorithm, folder states, and decisions: `DETAILS.md`. Read it before any non-trivial work here:
editing, planning, reorganizing, or advising.
