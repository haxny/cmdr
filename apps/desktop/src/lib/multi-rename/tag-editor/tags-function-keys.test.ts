/**
 * The Tags window's key helpers: which commands can act on a view, which Tags command a native menu key means (only
 * while the item still holds that key), and the echo that keeps one press by both roads from acting twice.
 */
import { describe, expect, it } from 'vitest'
import type { CommandId } from '#lib/commands/index.js'
import type { TagEditorView } from '#lib/tauri-commands/index.js'
import { createTagsKeyEcho, enabledTagsCommands, menuKeyCommand } from './tags-function-keys'

const track = (path: string, readError: TagEditorView['tracks'][number]['readError'] = null) => ({
  path,
  fileName: path,
  track: null,
  title: null,
  durationMs: null,
  changed: false,
  readError,
})

const VIEW: TagEditorView = {
  editable: true,
  readOnlyReason: null,
  shared: [],
  tracks: [track('a'), track('b')],
  cover: { kind: 'empty' },
  extras: [],
  dirty: false,
  diacritics: [],
  badDates: 0,
  canUndoEdit: false,
  canRedoEdit: false,
  scopeCount: null,
  sort: null,
}

describe('enabledTagsCommands', () => {
  it('offers nothing before the tags are read', () => {
    expect(enabledTagsCommands(null).size).toBe(0)
  })

  it('read-only leaves Play and Open in player', () => {
    expect([...enabledTagsCommands({ ...VIEW, editable: false })].sort()).toEqual(['tags.openInPlayer', 'tags.play'])
  })

  it('Edit # needs two tracks, Remove extras needs extras, Play a readable track', () => {
    const one = enabledTagsCommands({ ...VIEW, tracks: [track('a', 'unreadable')] })
    expect(one.has('tags.editNumber')).toBe(false)
    expect(one.has('tags.play')).toBe(false)
    expect(one.has('tags.removeExtras')).toBe(false)
    const full = enabledTagsCommands({ ...VIEW, extras: ['TXXX:MOOD'] })
    expect(full.has('tags.editNumber')).toBe(true)
    expect(full.has('tags.removeExtras')).toBe(true)
    expect(full.has('tags.write')).toBe(true)
    expect(full.has('tags.review')).toBe(true)
  })
})

describe('menuKeyCommand', () => {
  const store: Partial<Record<string, string[]>> = {
    'file.edit': ['F4'],
    'file.copy': ['⌘C'],
    'tags.replace': ['F4'],
    'tags.format': ['F5'],
  }
  const shortcutsOf = (id: CommandId): string[] => store[id] ?? []

  it('names the Tags command whose key the menu item holds', () => {
    expect(menuKeyCommand({ commandId: 'tags.replace', menuCommandId: 'file.edit' }, shortcutsOf)).toBe('tags.replace')
  })

  it('a file command rebound to another key never runs the tag tool', () => {
    expect(menuKeyCommand({ commandId: 'tags.format', menuCommandId: 'file.copy' }, shortcutsOf)).toBeNull()
  })

  it('a command the menu route doesn’t carry is ignored', () => {
    expect(menuKeyCommand({ commandId: 'tags.write', menuCommandId: 'file.edit' }, shortcutsOf)).toBeNull()
    expect(menuKeyCommand({ commandId: 'file.delete', menuCommandId: 'file.edit' }, shortcutsOf)).toBeNull()
  })
})

describe('createTagsKeyEcho', () => {
  it('the other road within the window is the same press; the same road again is a new press', () => {
    let now = 1000
    const isEcho = createTagsKeyEcho(() => now)
    expect(isEcho('tags.play', 'menu')).toBe(false)
    now += 20
    expect(isEcho('tags.play', 'keyboard')).toBe(true)
    now += 20
    expect(isEcho('tags.play', 'menu')).toBe(false)
    now += 1000
    expect(isEcho('tags.play', 'keyboard')).toBe(false)
  })

  it('keeps each command apart', () => {
    const isEcho = createTagsKeyEcho(() => 0)
    expect(isEcho('tags.play', 'menu')).toBe(false)
    expect(isEcho('tags.replace', 'keyboard')).toBe(false)
  })
})
