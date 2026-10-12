# Transfer tree (frontend): details

Must-knows: `CLAUDE.md`. The plan this implements, until it ships: `docs/specs/transfer-tree-progress.md`. The view
command's contract: `apps/desktop/src-tauri/src/file_system/write_operations/transfer_tree/DETAILS.md` § "The view".

## Using it

```svelte
<TransferTree operationId={id} tick={session.progress} operation="copy" defaultRows={8} />
```

- `operationId` (a running operation) or `source` (a fixture), fixed for the tree's life: re-key the tree to switch.
- `tick`: anything that changes when the operation advanced; the session's `progress` is the natural one.
- `operation`: `'copy' | 'move' | 'compress'`, which words the active state ("Copying").
- `defaultRows`: the most rows tall it gets before the host gives it more (never below three). It takes only the rows it
  has (a single file is one row) and grows as rows arrive, never shrinking back: `--spacing-tree-rows` is the high-water
  mark of `min(totalRows, defaultRows)`, so a folder shutting as the work moves on doesn't bounce the host's layout. The
  root is a `flex: 1 1 auto` column with that height as its basis and `min(rows, 3)` rows as its floor, so a resizable
  dialog's growing child stretches it and a squeezed one keeps three rows.

It renders nothing for `not_a_transfer` (delete, trash), "Listing the files…" while `ready: false` (queued, scanning, or
a tree a bug turned off), and keeps its last rows once the operation has finished.

## The pull

`transfer-tree-view.svelte.ts` builds each request when it's SENT (`createCoalesced`: one in flight, the newest
waiting), so a toggle or scroll during a pending request rides along. Following asks `Follow { lead: 10 }`, so the rows
the pinned headers cover and a short scroll up are loaded; paused, it asks `At { offset: top − 10 }`. The limit is the
viewport's rows plus 10 above and below. Scroll-driven pulls wait for a frame.

`followTop` is where the viewport's top belongs while following: the work sits `max(2, min(3, its depth))` rows down
(one row per header its folders will pin, so the stack never covers it), clamped so the viewport stays full. The
component sets `scrollTop` to it after each answer. Pausing records that position as the top, so the first paused pull
windows the rows on screen even if no scroll event ever said where that was.

**Decision: intent pauses following, a scroll event never does.** **Why**: a programmatic follow scroll, the browser
clamping `scrollTop` when a folder above shut, and a user's drag all fire the same `scroll` event, and tagging the
programmatic ones still loses the clamp. A wheel, a press on the scroller itself (its scrollbar), and a keyboard move
are unambiguous. The Follow button (a mini `Button` floating bottom-right, in Tab order after the tree) and the F key
(the physical `KeyF`, untranslated like the decision prompts' letters) resume.

## Rendering and focus

- The probe element carries the row-height token (`--spacing-tree-row`: one line of small text plus padding), so the
  virtual window divides by the real pixel height and follows the text size. Before there's a layout (first frame,
  jsdom) the tree draws the loaded window instead (`range`).
- Rows sit in absolutely placed slots keyed by flat index (`role="none"`, so the treeitems stay the tree's children). A
  slot's row changes in place as the tree changes; ❌ keying by id would MOVE the focused element, which blurs it.
- The keyboard cursor is a flat index plus the row id it holds. When an answer puts that id at another index (a folder
  above opened or shut), the cursor moves with it and, if the tree has focus, focus follows. A keyboard target that
  isn't loaded yet (End on a long list) waits as `pendingFocus` and lands when its row arrives. The cursor's row is
  cached, so it stays rendered after the window scrolled away from it.
- Roving `tabindex`: the cursor's row, else the row at work, else the first drawn row. The tree element itself is a tab
  stop only when no row is drawn.

## Pinned headers

The open folders whose rows scrolled away stick to the top, stacked by depth (VS Code's sticky scroll): each is a real
row, with its icon, counts, and bar while at work. `pinned-headers.ts` picks them from the viewport's top row: header
`k` is the level-`k` ancestor of the row that would show right under the stack, if that ancestor's row has scrolled
under it. Headers that no longer enclose that row drop (a folder's last rows passing under). At most three; the third
compacts every deeper scrolled-past folder into one header (`apps/desktop/src/lib`), so the row under the stack always
has its parent in sight. `ancestorsOf` answers a row's chain from the loaded rows, then the view's `ancestors` (whole
rows from the backend, counts included) for whatever sits above the window.

The stack is a zero-height `position: sticky` layer at the scroller's top, so the rows start under it and its headers
float over the rows they cover. A click on a header moves the cursor to its folder's row (revealed just below the
headers its own folders make); a click on its chevron shuts the folder.

**Decision: the headers are `aria-hidden` mirrors, not treeitems.** **Why**: a pinned header is a second copy of a row
the tree already has; as a treeitem it would read twice and break `aria-posinset` order. A screen reader walks the real
tree, where Left reaches every parent; the pointer gets the header click. They carry no tab stop for the same reason.

## Keyboard

Up / Down, PageUp / PageDown, Home / End move the cursor (and pause following). A target under the pinned headers is out
of sight too: `reveal` scrolls it to just below the headers its own folders will make. Right expands a shut folder, or
moves to its first child when open. Left shuts an open folder, or moves to the parent: an earlier row in the window one
level up, or, when the parent is above the window, the view's `ancestors` entry for that depth (`ancestorsOf`). F
resumes following while paused. A click on the chevron toggles; a double-click on a folder row toggles. A toggle never
pauses following.

## What a row says (`row-format.ts`)

- Count line, joined by middle dots: a folder at work "312 / 1,000 files · 1.2 GB / 4.0 GB", a waiting or finished one
  "1,000 files · 4.0 GB", plus "3 skipped" and a red "1 failed". A file: its size, "1.0 MB / 4.0 MB" at work, "Skipped",
  or its failure's typed title in red. A failed folder keeps the progress form, so "999 / 1,000" says what's missing.
- Accessible name: `formatUnitList([label, state, …facts])`, "Photos 2024, Copying, 312 of 1,000 files, 1.2 GB of 4.0
  GB". Each item is a standalone catalog phrase, so no key is a sentence fragment. No `aria-live`: the header readout
  speaks; a 5 Hz tree would talk over everything.
- The bar: folders along the view's `axis` (`Files` or `Size` progress as its name), files by bytes, drawn only on rows
  at work, without the shimmer (eight shimmering bars in a parallel SMB copy were busy).
- The label middle-truncates (`useShortenMiddle`) and hovers its full relative path, rebuilt by `rowPaths` from the
  window's rows plus `ancestors`.

## Fixtures

`fixtures/catalog.ts` holds seven shapes (single file; 5,000 top-level files on the files axis; 5 folders × 1,000; deep
`a/b/c` plus `Photos/2024` compact chains; six levels of non-compacting folders, past the pinned-header cap; parallel
SMB with 8 in flight across 3 folders; failed and skipped rows). `scripted-transfer.ts` simulates the transfer and
answers views with the backend's semantics in plain O(rows) code. `fixture-player.svelte.ts` drives one into a tree:
`step()` for tests, `play()` at the backend's 200 ms cadence (looping) for Debug → Components → Transfer tree.

## Tests

`row-format.test.ts`, `pinned-headers.test.ts` (which folders pin, the cap's compaction, dropping a passed folder),
`transfer-tree-view.svelte.test.ts` (coalescer, follow and its lead under a deep stack, toggles, ancestors),
`TransferTree.test.ts` (fixtures: posinset / setsize / level, keyboard, Follow, the tab stop, failure rows, the
content-sized height), and `transfer-tree.a11y.test.ts` (axe over every fixture: preparing, mid-transfer, paused,
finished). Axe's contrast rule is off in jsdom; `pnpm check a11y-contrast` checks the rows' color pairs in both themes.
