/** The Multi-Rename sheet's tabs, and the keys that pick one. Pure. */

export type SheetTab = 'rename' | 'tags'

/** ⌘1 is Rename, ⌘2 is Tags; nothing else (⌥, ⇧ or ⌃ with it belongs to other commands). */
export function sheetTabOfKey(
  e: Pick<KeyboardEvent, 'key' | 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey' | 'isComposing'>,
): SheetTab | null {
  if (!e.metaKey || e.ctrlKey || e.altKey || e.shiftKey || e.isComposing) return null
  if (e.key === '1') return 'rename'
  if (e.key === '2') return 'tags'
  return null
}
