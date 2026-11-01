/**
 * What a `bind:this` on `SyncGrid` can call. In a `.ts` file, since the typed lint can't resolve a component's
 * exports into calls.
 */
export interface SyncGridApi {
  focus: () => void
  scrollIndexIntoView: (index: number) => void
  /** Where a cursor key takes the cursor from row `from` (`ColumnList`'s rule), or `null` for another key. */
  cursorTarget: (event: KeyboardEvent, from: number) => number | null
}
