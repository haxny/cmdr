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
- `defaultRows`: the height before the host gives it more (never below three). The root is a `flex: 1 1 auto` column
  with that height as its basis, so a resizable dialog's growing child stretches it.

It renders nothing for `not_a_transfer` (delete, trash), "Listing the files…" while `ready: false` (queued, scanning, or
a tree a bug turned off), and keeps its last rows once the operation has finished.

## The pull

`transfer-tree-view.svelte.ts` builds each request when it's SENT (`createCoalesced`: one in flight, the newest
waiting), so a toggle or scroll during a pending request rides along. Following asks `Follow { lead: 2 }`; paused, it
asks `At { offset: top − 10 }`. The limit is the viewport's rows plus 10 above and below. Scroll-driven pulls wait for a
frame.

`followTop` is where the viewport's top belongs while following: `focusRow − 2`, clamped so the viewport stays full. The
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

## Keyboard

Up / Down, PageUp / PageDown, Home / End move the cursor (and pause following). Right expands a shut folder, or moves to
its first child when open. Left shuts an open folder, or moves to the parent: an earlier row in the window one level up,
or, when the parent is above the window, the view's `ancestors` entry for that depth. F resumes following while paused.
A click on the chevron toggles; a double-click on a folder row toggles. A toggle never pauses following.

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

`fixtures/catalog.ts` holds the six shapes (single file; 5,000 top-level files on the files axis; 5 folders × 1,000;
deep `a/b/c` plus `Photos/2024` compact chains; parallel SMB with 8 in flight across 3 folders; failed and skipped
rows). `scripted-transfer.ts` simulates the transfer and answers views with the backend's semantics in plain O(rows)
code. `fixture-player.svelte.ts` drives one into a tree: `step()` for tests, `play()` at the backend's 200 ms cadence
(looping) for Debug → Components → Transfer tree.

## Tests

`row-format.test.ts`, `transfer-tree-view.svelte.test.ts` (coalescer, follow, toggles, parent lookup),
`TransferTree.test.ts` (fixtures: posinset / setsize / level, keyboard, Follow, the tab stop, failure rows), and
`transfer-tree.a11y.test.ts` (axe over every fixture: preparing, mid-transfer, paused, finished). Axe's contrast rule is
off in jsdom; `pnpm check a11y-contrast` checks the rows' color pairs in both themes.
