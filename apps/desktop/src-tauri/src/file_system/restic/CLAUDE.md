# File system › restic

The app's side of restic browsing: the two seams that reach `crates/cmdr-restic` and the one hub every view shares.
Everything that reads a repository lives in that crate; read its `CLAUDE.md` before touching a listing, the index, or
unlock.

## Module map

- `overlay.rs`: the `ListingOverlay` that adds the `Browse snapshots` row to a repo folder's PANE listing.
- `wiring.rs`: the parked hub (`repos()`), and the two confirmed "is this inside a view?" checks for seams that bypass
  `resolve`.
- `snapshot_watch.rs`: the `ListingObserver` that watches a repo's real `snapshots/` while a pane shows its view root,
  and re-lists that pane when a backup lands (#626).
- The route is `apps/desktop/src-tauri/src/file_system/volume/manager/restic_routing.rs`, ordered against git and
  archives in `routing.rs`.

## Must-knows

- **Two seams, no hooks: no volume backend names restic, and ❌ must never.** At and below `<repo>/Browse snapshots`
  is a ROUTE (`resolve` → `cmdr_restic::ResticVolume`); the row itself is an OVERLAY reaching a pane and nothing else.
- **The repo's real files are ordinary files.** ❌ Never add a guard on `config`, `data/`, … (the git portal's old
  `.git/config` guard half-deleted repos).
- **The route confirms; the path predicate doesn't.** `path_routes_over_its_parent` is lexical and permissive (a cheap
  gate). ❌ Never refuse a write on it alone: a real folder named `Browse snapshots` outside a repo takes ordinary
  writes. The write guards use `local_path_is_in_a_restic_view` (memo, then a bounded `std::fs` read) or
  `memo_says_in_a_restic_view` (memo only, for a validator that must not probe).
- **❌ No confirm listing on a sync path.** `resolve_local_only` routes restic from the hub's memo alone, and the
  fresh-listing oracle declines an unrouted path under the row. A `read_dir` on a wedged OS mount blocks for minutes.
- **The hub is parked once, ❌ never rebuilt**: it holds every password and index. Memo keys are `(volume id, repo
  folder)` as spelled, ❌ no `canonicalize`, and the overlay must key with the listing's own volume id.
- **No walker but a pane and a selection-driven scan enters the view.** `walker_exposure_tests.rs` pins each one.
- **A new backup re-lists the root, nothing more.** The hub decides what moves (`latest` holds while anything reads
  under it). ❌ No polling on a parent with no watcher: its pane waits for a manual refresh.

The seams, the route's order, the guards, the walker answers, and the snapshot watch: `DETAILS.md`.
