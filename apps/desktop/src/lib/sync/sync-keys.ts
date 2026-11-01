/**
 * The Synchronize folders dialog's keys (spec 4.4): Total Commander's Synchronize dirs keys with ⌘ for its Ctrl,
 * except where macOS owns the key (⌘W closes windows, so reverse is ⌘I; Backspace is macOS Delete, so ⌘⌫ removes).
 *
 * Pure: a keydown in, what it means out. The dialog acts on it. Keys that would edit text (arrows, `-`, Space, ⌘⌫)
 * only count on the grid, so a field in the options strip keeps them.
 *
 * A key is matched as its WHOLE combo (`⌘⇧L`), so a superset (`⌥⌘L`) never fires it. Letters are read off the
 * physical key (`code`) where the event has one, because ⌥ composes another character (`⌥⌘R` types `®`).
 */
import type { BulkAction, ShowFilters, Side } from '#lib/tauri-commands/index.js'

export type SyncKeyAction =
  | { kind: 'bulk'; bulk: BulkAction }
  | { kind: 'filter'; filter: keyof ShowFilters }
  | { kind: 'compare' }
  | { kind: 'confirm' }
  | { kind: 'close' }
  | { kind: 'profiles' }
  | { kind: 'saveProfile' }
  | { kind: 'swap' }
  | { kind: 'view'; side: Side }
  | { kind: 'edit'; side: Side }
  | { kind: 'quickLook' }
  | { kind: 'compareContent' }
  | { kind: 'folder'; step: 1 | -1 }
  | { kind: 'copyRows' }
  | { kind: 'contextMenu' }

/** Where focus is: a field or button of the options strip, or the grid. */
export type SyncKeyArea = 'options' | 'grid'

export type SyncKeyEvent = Pick<KeyboardEvent, 'key' | 'metaKey' | 'shiftKey' | 'altKey' | 'ctrlKey'> & {
  code?: string
  isComposing?: boolean
}

/** Keys that work wherever focus is in the dialog. */
const ANYWHERE: Readonly<Partial<Record<string, SyncKeyAction>>> = {
  Escape: { kind: 'close' },
  '⌘Enter': { kind: 'confirm' },
  '⌥⌘R': { kind: 'compare' },
  F2: { kind: 'profiles' },
  '⌘S': { kind: 'saveProfile' },
  '⌘U': { kind: 'swap' },
  '⇧⌘L': { kind: 'filter', filter: 'toRight' },
  '⇧⌘E': { kind: 'filter', filter: 'equal' },
  '⇧⌘N': { kind: 'filter', filter: 'different' },
  '⇧⌘R': { kind: 'filter', filter: 'toLeft' },
  '⇧⌘D': { kind: 'filter', filter: 'duplicates' },
  '⇧⌘S': { kind: 'filter', filter: 'singles' },
  '⇧⌘U': { kind: 'filter', filter: 'unknown' },
  '⇧⌘W': { kind: 'filter', filter: 'warningsOnly' },
}

const bulk = (action: BulkAction): SyncKeyAction => ({ kind: 'bulk', bulk: action })

/** Keys that act on rows, so only while the grid has focus. */
const GRID: Readonly<Partial<Record<string, SyncKeyAction>>> = {
  Enter: { kind: 'compareContent' },
  '⌘F3': { kind: 'compareContent' },
  '⌘D': bulk('defaultDirection'),
  '⌘L': bulk('copyRight'),
  ArrowRight: bulk('copyRight'),
  '⌘R': bulk('copyLeft'),
  ArrowLeft: bulk('copyLeft'),
  '⌘I': bulk('reverse'),
  '⌘M': bulk('skip'),
  '-': bulk('skip'),
  Space: bulk('toggleSkip'),
  '⌘Backspace': bulk('removeTarget'),
  F3: { kind: 'view', side: 'left' },
  '⇧F3': { kind: 'view', side: 'right' },
  F4: { kind: 'edit', side: 'left' },
  '⇧F4': { kind: 'edit', side: 'right' },
  '⌥Space': { kind: 'quickLook' },
  '⌘PageDown': { kind: 'folder', step: 1 },
  '⌥⌘ArrowDown': { kind: 'folder', step: 1 },
  '⌘PageUp': { kind: 'folder', step: -1 },
  '⌥⌘ArrowUp': { kind: 'folder', step: -1 },
  '⌘C': { kind: 'copyRows' },
  '⇧F10': { kind: 'contextMenu' },
}

/** Options-strip only: Enter in a field compares (on a row it compares content instead). */
const OPTIONS: Readonly<Partial<Record<string, SyncKeyAction>>> = {
  Enter: { kind: 'compare' },
}

/** The key's name with its letter read off the physical key: `R` for `⌥⌘R` even though it typed `®`. */
function baseKey(event: SyncKeyEvent): string {
  const letter = /^Key([A-Z])$/.exec(event.code ?? '')
  if (letter) return letter[1]
  if (event.code === 'Space' || event.key === ' ') return 'Space'
  return event.key.length === 1 ? event.key.toUpperCase() : event.key
}

/** The whole combo, modifiers in macOS order: `⇧⌘L`, `⌥⌘R`, `F3`. `null` with ⌃, which none of these use. */
function comboOf(event: SyncKeyEvent): string | null {
  if (event.ctrlKey) return null
  const key = baseKey(event)
  const mods = `${event.altKey ? '⌥' : ''}${event.shiftKey ? '⇧' : ''}${event.metaKey ? '⌘' : ''}`
  return `${mods}${key}`
}

/** What the key does in the dialog, or `null` to leave it to whatever has focus. */
export function syncKeyAction(event: SyncKeyEvent, area: SyncKeyArea): SyncKeyAction | null {
  if (event.isComposing) return null
  const combo = comboOf(event)
  if (combo === null) return null
  const own = area === 'grid' ? GRID : OPTIONS
  return ANYWHERE[combo] ?? own[combo] ?? null
}
