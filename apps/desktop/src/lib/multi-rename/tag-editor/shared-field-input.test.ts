/**
 * The pure half of the shared-fields table: what the user typed, as the values a field holds, and what a
 * shared value looks like as text.
 */
import { describe, it, expect } from 'vitest'
import { formatFieldValue, formatTrackLength, isMultiLineField, parseFieldText } from './shared-field-input'

describe('parseFieldText', () => {
  it.each([
    ['genre', 'audiokniha; detektivka', ['audiokniha', 'detektivka']],
    ['artist', 'Karel Capek;  Josef Capek ; ', ['Karel Capek', 'Josef Capek']],
    ['genre', 'audiokniha', ['audiokniha']],
    ['album', 'Dracula; or, The Un-Dead', ['Dracula; or, The Un-Dead']],
    ['comment', 'Cte: Jan Novák; hudba: X', ['Cte: Jan Novák; hudba: X']],
    ['album', '  Dracula  ', ['Dracula']],
    ['comment', '  Čte: Jan  ', ['  Čte: Jan  ']],
    ['album', '', []],
    ['genre', '', []],
    ['album', '   ', []],
    ['genre', ' ; ', []],
  ] as const)('%s %j gives %j', (field, text, expected) => {
    expect(parseFieldText(field, text)).toEqual(expected)
  })
})

describe('formatFieldValue', () => {
  it('an empty field is blank, not «multiple»', () => {
    expect(formatFieldValue('empty')).toEqual({ text: '', multiple: false })
  })
  it('a same value joins its items with "; "', () => {
    expect(formatFieldValue({ same: ['a', 'b'] })).toEqual({ text: 'a; b', multiple: false })
  })
  it('differing values are «multiple» with no text', () => {
    expect(formatFieldValue('multiple')).toEqual({ text: '', multiple: true })
  })
})

describe('isMultiLineField', () => {
  it('is true for comment and description only', () => {
    expect(isMultiLineField('comment')).toBe(true)
    expect(isMultiLineField('description')).toBe(true)
    expect(isMultiLineField('album')).toBe(false)
  })
})

describe('formatTrackLength', () => {
  it.each([
    [null, ''],
    [0, '0:00'],
    [61_000, '1:01'],
    [599_999, '9:59'],
    [3_600_000, '1:00:00'],
    [3_725_000, '1:02:05'],
  ])('%s ms is %j', (ms, text) => {
    expect(formatTrackLength(ms)).toBe(text)
  })
})
