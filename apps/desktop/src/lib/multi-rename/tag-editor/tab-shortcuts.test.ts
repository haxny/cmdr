/**
 * The tag editor's commands are in the registry for every user, setting on or off, so Settings › Keyboard shortcuts
 * shows them. Their keys must not add a conflict there: upstream ships none on macOS, and `getConflictCount()` drives
 * the badge, the "Conflicts" filter and each row's warning icon.
 */

import { describe, it, expect, vi } from 'vitest'

// The registry reads `isMacOS()` at module load, so the UA is stubbed before the imports (as `registry-conflicts.test.ts`).
vi.hoisted(() => {
  vi.stubGlobal('navigator', { userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' })
})

import {
  eventMatchesCommand,
  resolveKeyCombo,
  findConflictsForShortcut,
  getConflictCount,
  getConflictingCommandIds,
} from '#lib/shortcuts/index.js'
import { getDefaultShortcuts } from '#lib/shortcuts/shortcuts-store.js'
import { getPaletteCommands } from '#lib/commands/index.js'

describe('the tag editor’s commands in Settings › Keyboard shortcuts', () => {
  it('add no conflict: the count stays at upstream’s zero', () => {
    expect(getConflictCount()).toBe(0)
    for (const id of ['file.editTags', 'tags.close', 'tags.write', 'tags.play'] as const)
      expect(getConflictingCommandIds().has(id)).toBe(false)
  })

  it('file.editTags is ⌘⇧E, and no other command in reach of the file list takes it', () => {
    expect(getDefaultShortcuts('file.editTags')).toEqual(['⌘⇧E'])
    expect(findConflictsForShortcut('⌘⇧E', 'Main window/File list').map((c) => c.id)).toEqual(['file.editTags'])
  })

  it('the palette lists Edit tags only while the tag editor setting is on', () => {
    const listed = (on: boolean) =>
      getPaletteCommands((condition) => (condition === 'tagEditorOn' ? on : true)).some((c) => c.id === 'file.editTags')
    expect(listed(false)).toBe(false)
    expect(listed(true)).toBe(true)
  })

  it('tags.close is Escape and ⌘W, alone in the Tags window', () => {
    expect(getDefaultShortcuts('tags.close')).toEqual(['Escape', '⌘W'])
    for (const key of ['Escape', '⌘W']) {
      expect(findConflictsForShortcut(key, 'Tags window').map((c) => c.id)).toEqual(['tags.close'])
    }
  })

  it.each([
    ['tags.switchPane', ['Tab']],
    ['tags.edit', ['F2', 'Enter']],
    ['tags.editNumber', ['⇧F2']],
    ['tags.play', ['F3', '⌥Space']],
    ['tags.openInPlayer', ['⇧F3']],
    ['tags.replace', ['F4']],
    ['tags.case', ['⇧F4']],
    ['tags.format', ['F5']],
    ['tags.cleanUp', ['⇧F5']],
    ['tags.fromName', ['F6']],
    ['tags.number', ['F7']],
    ['tags.clearField', ['F8']],
    ['tags.removeExtras', ['⇧F8']],
    ['tags.write', ['⌘S']],
    ['tags.undo', ['⌘Z']],
    ['tags.redo', ['⌘⇧Z']],
  ] as const)('%s is %j, and no other command in the Tags window or App takes those keys', (id, keys) => {
    expect(getDefaultShortcuts(id)).toEqual(keys)
    for (const key of keys) expect(findConflictsForShortcut(key, 'Tags window').map((c) => c.id)).toEqual([id])
    expect(getConflictingCommandIds().has(id)).toBe(false)
  })
})

describe('the Tags window’s play command', () => {
  it('matches the key macOS sends for ⌥Space (a non-breaking space) and no plain Space', () => {
    const alt = new KeyboardEvent('keydown', { key: '\u00a0', code: 'Space', altKey: true })
    const plain = new KeyboardEvent('keydown', { key: ' ', code: 'Space' })
    expect(eventMatchesCommand(alt, 'tags.play')).toBe(true)
    expect(eventMatchesCommand(plain, 'tags.play')).toBe(false)
    // The bare-Space commands (file list, queue) never see ⌥Space.
    expect(resolveKeyCombo(alt)).toBe('⌥Space')
  })
})
