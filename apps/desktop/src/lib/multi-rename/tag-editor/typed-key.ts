/**
 * Keys that start an edit without F2: a printable character opens the editor holding that character (spreadsheet
 * style), Delete and Backspace empty the field. Pure, so both panes read a key the same way.
 */

/** Keys that stay selection keys in the Tracks pane, so they never open an editor. */
const SELECTION_CHARS = new Set([' ', '*'])

/** A dialog is up: it owns the keys. */
export function dialogIsOpen(): boolean {
  return document.querySelector('[role="dialog"], [role="alertdialog"]') !== null
}

/** The character a keypress types into a field, or `null` for any other key (commands, IME, Space, `*`, ⌘/⌃/⌥). */
export function typedSeed(e: KeyboardEvent): string | null {
  if (e.isComposing || e.key === 'Process' || e.key === 'Dead') return null
  if (e.metaKey || e.ctrlKey || e.altKey) return null
  if (e.key.length !== 1 || SELECTION_CHARS.has(e.key)) return null
  return e.key
}

/** Whether a keypress is Delete or Backspace with no modifier: it empties the field under the cursor. */
export function isClearKey(e: KeyboardEvent): boolean {
  if (e.isComposing || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return false
  return e.key === 'Delete' || e.key === 'Backspace'
}
