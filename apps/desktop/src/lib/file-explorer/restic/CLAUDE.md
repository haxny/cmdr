# File explorer › restic

The frontend half of browsing a restic backup: which paths are inside a repo's snapshot view, and what a pane there may
do. Backend: `apps/desktop/src-tauri/src/file_system/restic/CLAUDE.md` (the route and the row) and
`crates/cmdr-restic/CLAUDE.md` (everything that reads a repo).

## Module map

- `path-detection.ts`: `isVirtualResticPath`, `resticRepoRoot`, `resticViewComponentIndex`, and
  `RESTIC_VIEW_FOLDER_NAME`. Pure and lexical.
- `forget-password.ts`: the "Forget backup password" command (DETAILS § "Forgetting a saved password").
- The pane's `restic` capability row lives in `../pane/volume-capabilities.ts`; the unlock dialog in
  `../pane/unlock-flow.svelte.ts`.

## Must-knows

- **The detector must agree with the backend's `restic_route`** (`crates/cmdr-fs/src/restic_view.rs`): the FIRST
  `Browse snapshots` component marks the view, exact match. Change one, change both.
- **`Browse snapshots` is a path component, ❌ never translate it.** History, favorites, and the agent address paths.
- **A real folder named `Browse snapshots` outside a repo reads as read-only here** while the backend passes it through.
  Accepted: confirming needs I/O, and the git portal has the same edge.
- **Routed kinds nest, and the leftmost boundary wins** (`capabilitiesForPane`), as in the backend's `resolve`.

What a pane inside the view may do, the row's icon, index progress, and its QA repro: `DETAILS.md`.
