/**
 * The Synchronize folders dialog's key map (spec 4.4): Total Commander's keys with ⌘ for its Ctrl, except where
 * macOS owns the key (⌘W closes windows, so ⌘I reverses). One case per row of the table.
 */
import { describe, expect, it } from 'vitest'
import { syncKeyAction, type SyncKeyEvent } from './sync-keys'

/** A keydown as macOS sends it: `code` is the physical key, `key` what it typed (⌥ composes). */
function k(key: string, init: Partial<SyncKeyEvent> = {}): SyncKeyEvent {
  const code = init.code ?? (key.length === 1 && /[a-z]/i.test(key) ? `Key${key.toUpperCase()}` : undefined)
  return { key, metaKey: false, shiftKey: false, altKey: false, ctrlKey: false, ...init, code }
}

const cmd = { metaKey: true }
const cmdShift = { metaKey: true, shiftKey: true }

describe('syncKeyAction', () => {
  it('maps ⌘I to reverse, not ⌘W', () => {
    expect(syncKeyAction(k('i', cmd), 'grid')).toEqual({ kind: 'bulk', bulk: 'reverse' })
    expect(syncKeyAction(k('w', cmd), 'grid')).toBeNull()
  })

  it('Enter compares in the options strip and compares content on a row', () => {
    expect(syncKeyAction(k('Enter'), 'options')).toEqual({ kind: 'compare' })
    expect(syncKeyAction(k('Enter'), 'grid')).toEqual({ kind: 'compareContent' })
    expect(syncKeyAction(k('F3', cmd), 'grid')).toEqual({ kind: 'compareContent' })
  })

  it('⌥⌘R compares again from anywhere, reading the physical key under ⌥', () => {
    const optionCmdR = k('®', { metaKey: true, altKey: true, code: 'KeyR' })
    expect(syncKeyAction(optionCmdR, 'grid')).toEqual({ kind: 'compare' })
    expect(syncKeyAction(optionCmdR, 'options')).toEqual({ kind: 'compare' })
  })

  it('⌘Enter opens the confirm and Esc closes', () => {
    expect(syncKeyAction(k('Enter', cmd), 'grid')).toEqual({ kind: 'confirm' })
    expect(syncKeyAction(k('Enter', cmd), 'options')).toEqual({ kind: 'confirm' })
    expect(syncKeyAction(k('Escape'), 'grid')).toEqual({ kind: 'close' })
  })

  it('F2 opens the profiles, ⌘S saves one, ⌘U swaps the sides', () => {
    expect(syncKeyAction(k('F2'), 'grid')).toEqual({ kind: 'profiles' })
    expect(syncKeyAction(k('s', cmd), 'options')).toEqual({ kind: 'saveProfile' })
    expect(syncKeyAction(k('u', cmd), 'grid')).toEqual({ kind: 'swap' })
  })

  it.each([
    ['d', 'defaultDirection'],
    ['l', 'copyRight'],
    ['r', 'copyLeft'],
    ['i', 'reverse'],
    ['m', 'skip'],
  ] as const)('⌘%s on rows is the bulk %s', (letter, bulk) => {
    expect(syncKeyAction(k(letter, cmd), 'grid')).toEqual({ kind: 'bulk', bulk })
  })

  it('→ and ← set the direction, - skips, Space toggles planned and skip, ⌘⌫ removes on the target', () => {
    expect(syncKeyAction(k('ArrowRight'), 'grid')).toEqual({ kind: 'bulk', bulk: 'copyRight' })
    expect(syncKeyAction(k('ArrowLeft'), 'grid')).toEqual({ kind: 'bulk', bulk: 'copyLeft' })
    expect(syncKeyAction(k('-'), 'grid')).toEqual({ kind: 'bulk', bulk: 'skip' })
    expect(syncKeyAction(k(' ', { code: 'Space' }), 'grid')).toEqual({ kind: 'bulk', bulk: 'toggleSkip' })
    expect(syncKeyAction(k('Backspace', cmd), 'grid')).toEqual({ kind: 'bulk', bulk: 'removeTarget' })
  })

  it('leaves row keys to a text field in the options strip', () => {
    for (const event of [k('l', cmd), k('ArrowRight'), k('-'), k(' ', { code: 'Space' }), k('Backspace', cmd)]) {
      expect(syncKeyAction(event, 'options')).toBeNull()
    }
  })

  it('F3 and ⇧F3 view, F4 and ⇧F4 edit, ⌥Space opens Quick Look', () => {
    expect(syncKeyAction(k('F3'), 'grid')).toEqual({ kind: 'view', side: 'left' })
    expect(syncKeyAction(k('F3', { shiftKey: true }), 'grid')).toEqual({ kind: 'view', side: 'right' })
    expect(syncKeyAction(k('F4'), 'grid')).toEqual({ kind: 'edit', side: 'left' })
    expect(syncKeyAction(k('F4', { shiftKey: true }), 'grid')).toEqual({ kind: 'edit', side: 'right' })
    expect(syncKeyAction(k(' ', { altKey: true, code: 'Space' }), 'grid')).toEqual({ kind: 'quickLook' })
  })

  it('⌘PageDown and ⌥⌘↓ go to the next folder, ⌘PageUp and ⌥⌘↑ to the previous one', () => {
    expect(syncKeyAction(k('PageDown', cmd), 'grid')).toEqual({ kind: 'folder', step: 1 })
    expect(syncKeyAction(k('ArrowDown', { metaKey: true, altKey: true }), 'grid')).toEqual({ kind: 'folder', step: 1 })
    expect(syncKeyAction(k('PageUp', cmd), 'grid')).toEqual({ kind: 'folder', step: -1 })
    expect(syncKeyAction(k('ArrowUp', { metaKey: true, altKey: true }), 'grid')).toEqual({ kind: 'folder', step: -1 })
  })

  it('⌘C copies the rows as text, ⇧F10 opens the row menu', () => {
    expect(syncKeyAction(k('c', cmd), 'grid')).toEqual({ kind: 'copyRows' })
    expect(syncKeyAction(k('F10', { shiftKey: true }), 'grid')).toEqual({ kind: 'contextMenu' })
  })

  it.each([
    ['l', 'toRight'],
    ['e', 'equal'],
    ['n', 'different'],
    ['r', 'toLeft'],
    ['d', 'duplicates'],
    ['s', 'singles'],
  ] as const)('⌘⇧%s toggles the %s show-filter', (letter, filter) => {
    expect(syncKeyAction(k(letter.toUpperCase(), { ...cmdShift, code: `Key${letter.toUpperCase()}` }), 'grid')).toEqual(
      { kind: 'filter', filter },
    )
    expect(
      syncKeyAction(k(letter.toUpperCase(), { ...cmdShift, code: `Key${letter.toUpperCase()}` }), 'options'),
    ).toEqual({ kind: 'filter', filter })
  })

  it('maps ⌘⇧U and ⌘⇧W to the unknown and warnings filters', () => {
    expect(syncKeyAction(k('U', { ...cmdShift, code: 'KeyU' }), 'grid')).toEqual({ kind: 'filter', filter: 'unknown' })
    expect(syncKeyAction(k('W', { ...cmdShift, code: 'KeyW' }), 'grid')).toEqual({
      kind: 'filter',
      filter: 'warningsOnly',
    })
  })

  it('ignores ⌃ combos, plain letters, and keys mid-composition', () => {
    expect(syncKeyAction(k('l', { ctrlKey: true }), 'grid')).toBeNull()
    expect(syncKeyAction(k('l'), 'grid')).toBeNull()
    expect(syncKeyAction(k('Enter', { isComposing: true }), 'options')).toBeNull()
  })
})
