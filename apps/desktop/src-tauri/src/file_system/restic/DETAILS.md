# The app's restic wiring: details

Pull-tier docs for `file_system/restic/`. Must-knows live in `CLAUDE.md`. Everything that reads a repository (the
storage adapter, unlock, snapshots, trees, the hub's memo) is `crates/cmdr-restic/DETAILS.md`; this doesn't restate it.

## Two seams, no hooks

The same split as the git portal (`file_system/git/DETAILS.md`), for the same reason: a repo folder is a real folder
whose real entries must stay editable and deletable, so the view can't be a volume over it.

- **The route.** `VolumeManager::resolve` sends `<repo>/Browse snapshots` and everything below it to a
  `cmdr_restic::ResticVolume` once the hub confirms `<repo>` is a repo (`apps/desktop/src-tauri/src/file_system/volume/manager/restic_routing.rs`). The path
  stays the caller's; the view classifies it. The view is read-only by type, unwatchable, and `routes_over_a_parent`,
  so `mount_id_for_path` names the drive holding the repo.
- **The overlay.** `ResticOverlay` adds the one row to a pane's listing of a folder whose ENTRIES look like a repo
  (`is_restic_repo`: a `config` file plus `data`, `index`, `keys`, `snapshots`), on any volume a route didn't mint.
  Unlike git's local-only rule, a repo on SMB, SFTP, or a phone qualifies, since the crate reads it through `Volume`.
  A listing past `MAX_REPO_ROOT_ENTRIES` (32) answers on its length alone, so a 100k-entry folder costs one compare.
- **The overlay seeds the route.** Building the row confirms the repo in the hub's memo, keyed by the volume id the
  listing was read under (`ListingOverlay::extra_entries` carries it for exactly this), so entering the row costs no
  confirm listing. A typed path or a restored session pays one bounded listing of the repo folder, remembered either
  way.
- An incremental watcher `Added` doesn't re-run overlays, so a repo created by `restic init` under an open pane shows
  its row on the next full read (navigation or refresh). Acceptable; no hook for it.

## The route's order: leftmost confirmed boundary wins

Three routes can claim a path, and they nest both ways: a `.zip` or a `.git/branches` inside a restic snapshot is
backed-up data, and a restic repo inside a git snapshot is the portal's. `routing.rs` collects every lexical boundary
(the `.git` before a category, each `Browse snapshots` component, the first archive-named component), tries them left
to right, and takes the first one that confirms. An unconfirmed one (a real folder named `photos.zip`, a non-repo
`Browse snapshots`, the portal switched off) hands over to the next, so a repo inside a real folder named
`backups.zip/` still browses. Cells: `apps/desktop/src-tauri/src/file_system/volume/manager/restic_routing_tests.rs`.

A view is never a restic parent: a repo backed up inside a snapshot stays bytes in it, on both seams.

## A replaced parent gets a new view

An SMB share reconnecting (or an OS mount upgraded to a direct session) replaces the parent under the same volume id.
`register_if_absent` would keep a view minted over the retired parent, reading through it forever, so the restic route
remembers which parent minted each view (`ResticRoutes::parents`, a `Weak`, compared by address) and re-mints over the
current one, swapping it in place with `register`. The hub re-opens a repo on a changed parent on its own
(`crates/cmdr-restic/DETAILS.md` § "The hub"), and keeps the password.

## The routing LRU bounds registrations only

Four views stay registered (`RESTIC_LRU_CAP`), separate from the archive and git LRUs. A view is cheap: the hub holds
the passwords, opened repos, and indexes, so eviction never drops an index or forgets a password.

## Write guards

`VolumeManager`'s callers (copy, move, delete, trash, rename, create, bulk rename, rename-as-move) refuse through
`resolve` plus `RoutedKind::takes_writes` / `can_give_up_files`: a restic route takes no writes, and a move out of one
is refused before anything is copied, like git's. When the confirm can't decide (the repo folder's listing failed or
outran its deadline), `resolve` answers an UNDECIDED passthrough and those write gates refuse on it too
(`file_system/volume/DETAILS.md` § "Resolving a path: the three routes"). Two seams bypass `resolve` and ask here, each CONFIRMED (❌ never the
permissive path predicate, which would refuse a copy into a real folder named `Browse snapshots`):

- **The local write fast path** (`commands/file_system/write_ops.rs` `reject_if_routed_over_a_parent`) has paths with
  no volume id, so `local_path_is_in_a_restic_view` asks the memo for the boot volume and otherwise reads the candidate
  folder with `std::fs`, giving up past 32 entries. It's the write path, not the listing hot path, and an unconfirmed
  miss that wrote would put a real `Browse snapshots` folder inside someone's repo.
- **The agent's rename plan** (`agent/tools/propose/rename/plan.rs`) must never probe a mount, so
  `memo_says_in_a_restic_view` answers from the memo alone, under the focused pane's volume id. A pane only stands
  inside a view after the route confirmed its repo.

## What each walker sees

`walker_exposure_tests.rs` pins the view's blast radius; the answer for every non-pane walker is "nothing":

- A volume listing of the repo carries no row; a pane's decorated listing does.
- The delete walker lists through `Volume`, never meets the row, and removes the repo completely; the repo's real files
  stay fully mutable.
- The copy scan of the repo folder counts what's on disk.
- The fresh-listing oracle declines a decorated repo listing, and an unrouted path under a `Browse snapshots`
  component (`crates/cmdr-listing/src/authoritative.rs`).
- The drive index's `Volume`-trait walk of a share holding a repo never records the view. The local walker reads the
  disk, where the row doesn't exist.
- On-demand folder sizes (⌥⇧⏎) plan the pane's folder rows, which include the row: the plan leaves out a row a route
  serves when its listing's folder isn't routed (`listing_index_sizes/count/mod.rs` `is_a_view_row`, git's category
  rows included). Space on the row is the person asking and counts through the view.

Selecting the row and copying it is a legal, enormous restore through the routed volume; its pre-scan stays
cancelable, and nothing blocks it.

## Reading out of a snapshot: copy, F3, and the agent

Each rides the route, with no restic-specific code on the way. Cells on a real repo, backed up from a real folder,
written onto a local drive registered in the app's registry (`test_repo.rs`, whose `ReadGate` can hold a reader inside
an `index/` or `data/` read):

- **Copy-out** (`apps/desktop/src-tauri/src/operations_host/restic_copy_out_tests.rs`): a file and a folder land byte
  for byte with their dates and permission bits; a move out is refused before a byte is written. A symlink or a fifo
  inside a copied folder is one skipped item (`VolumeError::NoContent`); restoring a link AS a link needs a cross-backend
  symlink seam that doesn't exist yet. A Cancel during a whole-snapshot pre-scan lands inside the listing that waits on
  the index load (the view opts into `ScanSource::abandons_reads_on_stop`).
  A repo whose password the hub doesn't hold (remember off, then a restart) answers the copy with
  `WriteOperationError::ArchiveNeedsPassword`, the prompt the unlock dialog's repo variant answers. Hub eviction keeps
  the password, so it never prompts.
- **F3** (`apps/desktop/src-tauri/src/file_viewer/session_test.rs`): materializes the file through the view's stream into the bounded temp, like
  a file in a git snapshot; a restic read failure is a plain I/O error.
- **`inspect_file`** (`apps/desktop/src-tauri/src/agent/tools/read/inspect/restic_snapshot_tests.rs`): the same temp, a text window, and the temp
  gone afterwards. No new kind of content leaves.
