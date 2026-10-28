/**
 * ⌘C / ⌘X / ⌘V on a field, the pure half: what a shared row copies, and what pasted text a field takes.
 */
import { describe, expect, it } from 'vitest'
import type { SharedRow } from '#lib/ipc/bindings.js'
import { PASTE_LIMIT_LINE, PASTE_LIMIT_PROSE, pasteRefusal, pastedValues, sharedCopyText } from './field-clipboard'

const row = (value: SharedRow['value'], choices: SharedRow['choices'] = [], choicesMore = 0): SharedRow => ({
  field: 'artist',
  value,
  choices,
  choicesMore,
})

describe('sharedCopyText', () => {
  it('copies a shared value as its text, several values joined with "; "', () => {
    expect(sharedCopyText(row({ same: ['Karel Čapek'] }))).toBe('Karel Čapek')
    expect(sharedCopyText(row({ same: ['Karel Čapek', 'Josef Čapek'] }))).toBe('Karel Čapek; Josef Čapek')
  })

  it('copies an empty field as an empty string', () => {
    expect(sharedCopyText(row('empty'))).toBe('')
  })

  it('copies «multiple values» as the distinct values without the marker or the «…and N more» tail', () => {
    const choices = [
      { values: ['Jan Novák'], files: 2 },
      { values: ['Eva Malá', 'Jan Novák'], files: 1 },
    ]
    expect(sharedCopyText(row('multiple', choices, 3))).toBe('Jan Novák; Eva Malá')
  })
})

describe('pastedValues', () => {
  it.each([
    ['album', 'Dracula\n', ['Dracula']],
    ['album', 'Dracula\r\n', ['Dracula']],
    ['album', 'Dracula\nsecond line\n', ['Dracula']],
    ['genre', 'audiokniha\ndetektivka\n', ['audiokniha', 'detektivka']],
    ['artist', 'Karel Čapek; Josef Čapek', ['Karel Čapek', 'Josef Čapek']],
    ['comment', 'Čte: Jan\nHudba: X\n', ['Čte: Jan\nHudba: X']],
    ['description', 'one\r\ntwo\n\n', ['one\ntwo\n']],
    ['track', '7\n', ['7']],
    ['album', '\n', []],
    ['title', '  ', []],
  ] as const)('%s takes %j as %j', (field, text, values) => {
    expect(pastedValues(field, text)).toEqual(values)
  })
})

describe('pasteRefusal', () => {
  it('takes up to 4 KiB into a one-line field and up to 64 KiB into Comment or Description', () => {
    expect(pasteRefusal('album', 'a'.repeat(PASTE_LIMIT_LINE))).toBeNull()
    expect(pasteRefusal('comment', 'a'.repeat(PASTE_LIMIT_PROSE))).toBeNull()
    expect(pasteRefusal('description', 'a'.repeat(PASTE_LIMIT_LINE + 1))).toBeNull()
  })

  it('refuses more, counted in UTF-8 bytes, so nothing changes', () => {
    expect(pasteRefusal('album', 'a'.repeat(PASTE_LIMIT_LINE + 1))).toBe('multiRename.tags.clipboard.tooLong')
    expect(pasteRefusal('comment', 'a'.repeat(PASTE_LIMIT_PROSE + 1))).toBe('multiRename.tags.clipboard.tooLong')
    // 2 049 two-byte letters are 4 098 bytes.
    expect(pasteRefusal('title', 'č'.repeat(PASTE_LIMIT_LINE / 2 + 1))).toBe('multiRename.tags.clipboard.tooLong')
  })
})
