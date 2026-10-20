# File explorer › restic: details

Pull-tier docs for `restic/`. Must-knows live in `CLAUDE.md`.

## A pane inside the view

`capabilitiesForPane` gives a path at or below `<repo>/Browse snapshots` the `restic` row, a copy of the `git-portal`
row: a real backend listing, `..`, and copy-out, with every write refused up front. Paste-into, F7, ⇧F4, F2, delete, and
Multi-rename get the `fileExplorer.readOnly.restic*` alert; ⌘C/⌘X point at F5
(`fileExplorer.restic.useTransferToCopyOut`, F5 only, since a move out of a backup is refused). The repo folder and its
real files keep the drive's row.

- **No deleted-folder poll** (`pollsForDeletedFolder: false`): the view has no inode, so a stat would evict the user.
- **`..` at the view's top** walks back to the repo folder: the listing reports `<repo>/Browse snapshots` as its root,
  so `../pane/has-parent.ts::effectiveVolumeRootFor` uses the drive's mount for every routed kind.
- **Not favoritable, not shareable, no editor** (`kindCanBeFavorited`, `rowIsOsVisible`): nothing in the view is on
  disk. The `Browse snapshots` row itself counts as inside the view.
- **Open terminal here** opens the repo folder (`#lib/open-terminal/terminal-target.ts`), the nearest folder on disk,
  and never the `Browse snapshots` row under the cursor.
- **Select all and invert leave the `Browse snapshots` row out**, like `..` and the git portal's category rows
  (`../pane/overlay-rows.ts`, one predicate: a reserved view icon id in a pane that isn't inside a view). "Select all,
  copy" in a repo folder means the repo's real files; picking the row on its own still restores through the view.
- **The `Browse snapshots` row's icon** is Lucide `history`, drawn by `../selection/FileIcon.svelte` for the reserved
  `restic:snapshots` icon id the `cmdr-restic` hub puts on the row (`OVERLAY_ROW_ICON_ID`), the same way the git
  portal's `git:*` ids get their glyphs. It inherits the row's text color.

## Index progress

Every folder inside a snapshot needs the repo's index first, which on a NAS takes seconds. The backend reports it as a
typed `ListingPhase::loadingIndex` (bytes and files, read so far and in all) on `listing-progress`
(`crates/cmdr-restic/DETAILS.md` § "The index lifecycle"). `../pane/listing-loader.ts` hands it to the pane
(`setLoadingPhase`, cleared at every load start and end, and by a progress event without one), and
`#lib/ui/LoadingIcon.svelte` words it: "Loading the backup index: 48 MB of 312 MB" over a bar. Escape cancels it like
any load; the backend drops the index load when its last waiter leaves.

- **QA repro**: Debug window (⌘⇧D) → Components → Loading has a static "48 of 312 MB" cell and a scripted 20 s load on a
  loop. Judge the static one: a moving bar can't be judged from a frame.
