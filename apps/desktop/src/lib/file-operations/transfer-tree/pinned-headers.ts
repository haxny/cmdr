/**
 * The open folders a tree pins at its top once their own rows scrolled away
 * (sticky headers, like VS Code's sticky scroll): which ones, from where the
 * viewport's top row sits. Pure, so it's testable without a layout engine.
 */

import type { TransferTreeAncestor, TransferTreeRow } from '#lib/tauri-commands/index.js'

/** The most headers stacked at once; deeper scrolled-past folders compact into the last one. */
export const MAX_PINNED = 3

/** A pinned folder: the flat index of its real row, and the row as the header shows it. */
export interface PinnedHeader {
  index: number
  /** `depth` is the header's level in the stack; a compacted header's `label` joins its folders with `/`. */
  row: TransferTreeRow
}

/**
 * The headers to pin over a viewport whose top row is `top`.
 *
 * Header `k` sits over the row at `top + k`. It's the ancestor (at level `k`)
 * of the row that would show right under the stack, if that ancestor's own
 * row has scrolled under the stack. The headers above must still enclose that
 * row; where they don't (a folder's last rows are passing under), the stack
 * stops there. The last slot compacts every deeper scrolled-past folder into
 * one header (`c/d/e`), so the row under the stack always has its parent in
 * sight.
 *
 * `ancestorsOf(index)` answers a row's open ancestor rows, outermost first,
 * empty when that row isn't loaded.
 */
export function pinnedHeaders(
  top: number,
  total: number,
  ancestorsOf: (index: number) => readonly TransferTreeAncestor[],
): PinnedHeader[] {
  const pinned: PinnedHeader[] = []
  for (let level = 0; level < MAX_PINNED; level++) {
    const under = top + level + 1
    if (under >= total) break
    const chain = ancestorsOf(under)
    const enclosing = pinned.findIndex((header, at) => chain[at]?.index !== header.index)
    if (enclosing >= 0) {
      pinned.length = enclosing
      break
    }
    const last = level === MAX_PINNED - 1
    const limit = last ? under : top + level
    const hidden = chain.slice(level).filter((ancestor) => ancestor.index < limit)
    if (hidden.length === 0) break
    if (!last) {
      pinned.push({ index: hidden[0].index, row: { ...hidden[0].row, depth: level } })
      continue
    }
    // A lone folder sitting right in the last slot shows as itself; a header over it adds nothing.
    if (hidden.length === 1 && hidden[0].index === top + level) break
    const deepest = hidden[hidden.length - 1]
    const label = hidden.map((ancestor) => ancestor.row.label).join('/')
    pinned.push({ index: deepest.index, row: { ...deepest.row, depth: level, label } })
  }
  return pinned
}
