# Transfer tree

A copy's, move's, or compress's selection as a live tree, for the progress dialog and the queue to show which folder is
in flight, what landed, and what's next. The backend owns the model; a view asks for a window of rows.

## Module map

- `model.rs` (the arena and the per-file-boundary mutations), `build.rs` (`TreeRecorder` for a volume scan,
  `from_local_scan`, `from_archive_plan`, `from_per_path`, `from_sources`, `sort_sources`), `order.rs` (tree order),
  `report.rs` (how drivers report: `TreeReporter`, `TreeLeaf`, `TreeLevel`), `view.rs` (flattening into a window).
  `mod.rs`: `TransferTree` (the shared lock) and `TransferTreeSlot` (where `WriteOperationState` keeps it). The IPC
  answer is `../tree_view.rs`.

## Must-knows

- **Observation only.** Every report returns nothing a driver could branch on, is a no-op without a tree or a row, and a
  panicking one is caught (`TransferTree::guarded`) and turns the tree off. Building is caught too (`built`): every
  builder, recorded entry, and the local sort answer `None`, and the transfer runs as with no tree. ❌ Never build or
  rank outside `built`, and never let a transfer read the tree's STATE back.
- **≤ 64 B per node, names included**, pinned at 1M nodes by `budget_tests.rs`. ❗ `Node` is 16 bytes on purpose; grow
  it or add a per-node side vector and the budget test says by how much. ❌ Never a global path map: the only paths kept
  are the selected sources' and their landing folders (`TreeLanding`).
- **Ids are breadth-first, assigned once by `TreeBuilder::finish`**, so a folder's scanned children are one contiguous
  range in tree order. A child found later goes to `extra_children`, after them. ❌ Never renumber after `finish`:
  views key rows and hand toggles by id.
- **One lock per file BOUNDARY** (begin, complete, skip, fail), ❌ never per chunk: an active file's bytes are its
  `TreeLeaf`'s counter, read at view time. A walker looks children up through one `TreeLevel` per level.
- **A view is O(|expansion| + limit), ❌ never O(children)** (`view.rs`). Effective expansion is `(ancestors of the
  focus files ∪ expanded) − collapsed`, computed per request; ❌ no per-view state on the backend.
- **Compaction happens at flatten time**: a folder whose only child is a folder shares its row, keyed by the deepest
  id. ❌ Never compact in the model: a discovered child breaks a chain.
- **Built for copy, move, and compress only**, ❌ never delete or trash (`ScanPreviewPurpose`). A rename move's tree is
  its items alone (`from_sources`).
- **Every driver STARTS files in tree order** (`../transfer/DETAILS.md` § "Processing order"). A new walk sorts each
  level with `TreeOrder`, ❌ never in listing or pane-sort order.

Layout, the view algorithm, folder states, what each driver reports, and decisions: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
