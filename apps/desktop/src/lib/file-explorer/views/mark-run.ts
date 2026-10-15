/** Where a marked row sits in its run of consecutive marked rows. */
export interface MarkRunEdges {
  /** The row opens its run: the marked-file bar rounds its top end here. */
  isRunStart: boolean
  /** The row closes its run: the bar rounds its bottom end here. */
  isRunEnd: boolean
}

/**
 * Run edges for the marked-file bar (`app-file-list.css`), read from the marked set by global
 * index, never from the rendered neighbors: a run crossing the virtual window's edge must not
 * show a false rounded end there. Two set lookups, so O(1) per row. `columnSize` is Brief's
 * rows per column, where a run ends at the column's foot; Full view leaves it unbounded.
 * Meaningful only for a marked row.
 */
export function markRunEdges(index: number, marked: ReadonlySet<number>, columnSize = Infinity): MarkRunEdges {
  const row = index % columnSize
  return {
    isRunStart: row === 0 || !marked.has(index - 1),
    isRunEnd: row === columnSize - 1 || !marked.has(index + 1),
  }
}
