/** How the Write dialog words Audiobookshelf's side files, and which replacements its list hands back. */
import { describe, expect, it, vi } from 'vitest'
import type { SideFileChange } from '#lib/tauri-commands/index.js'

vi.mock('#lib/settings/index.js', () => ({ getSetting: () => false }))

import { SIDE_TEXT_SHOWN, quoted, replacementsShown, replacing, sideFilesOn, sideRowOf, sideWrites } from './side-files'

const CREATE: SideFileChange = { kind: 'create', file: 'reader', path: '/b/reader.txt', text: 'Vladimir Kudla' }
const REPLACE: SideFileChange = { kind: 'replace', file: 'desc', path: '/b/desc.txt', old: 'Old\nblurb\n', text: 'New' }
const WITHHELD: SideFileChange = { kind: 'withheld', file: 'reader', reason: 'tracksDisagree' }

describe('side files', () => {
  it('is off by default', () => {
    expect(sideFilesOn()).toBe(false)
  })

  it('words each row by its file and what happens to it', () => {
    expect(sideRowOf(CREATE)).toEqual({
      key: 'multiRename.tags.write.sideCreate',
      params: { fileName: 'reader.txt', text: 'Vladimir Kudla' },
    })
    expect(sideRowOf(REPLACE)).toEqual({
      key: 'multiRename.tags.write.sideReplace',
      params: { fileName: 'desc.txt', old: 'Old blurb', text: 'New' },
    })
    expect(sideRowOf(WITHHELD)).toEqual({
      key: 'multiRename.tags.write.sideWithheld',
      params: { fileName: 'reader.txt' },
      why: 'multiRename.tags.write.sideWhy.readerDisagree',
    })
    expect(sideRowOf({ kind: 'withheld', file: 'desc', reason: 'tracksDisagree' })).toMatchObject({
      why: 'multiRename.tags.write.sideWhy.descDisagree',
    })
  })

  it('quotes a long text shortened on one line', () => {
    const shown = quoted('x'.repeat(SIDE_TEXT_SHOWN + 10))
    expect(shown).toHaveLength(SIDE_TEXT_SHOWN + 1)
    expect(shown.endsWith('…')).toBe(true)
  })

  it('hands back only the replacements shown, each with the full old text', () => {
    expect(replacementsShown([CREATE, REPLACE, WITHHELD])).toEqual([{ path: '/b/desc.txt', old: 'Old\nblurb\n' }])
    expect(replacing([CREATE, REPLACE, WITHHELD])).toBe(1)
  })

  it('a withheld file alone writes nothing', () => {
    expect(sideWrites([WITHHELD])).toBe(false)
    expect(sideWrites([WITHHELD, CREATE])).toBe(true)
  })
})
