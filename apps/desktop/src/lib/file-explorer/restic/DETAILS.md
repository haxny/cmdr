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
