/**
 * The pure half of the shared-fields table: what the user typed, as the values a field holds, and what a
 * shared value looks like as text.
 */
import { afterEach, describe, it, expect } from 'vitest'
import { _clearCompiledCacheForTests, _setCatalogForTests, setLocale } from '#lib/intl/messages.svelte.js'
import {
  formatFieldValue,
  formatTrackLength,
  isMultiLineField,
  multipleText,
  parseFieldText,
  parseSharedEdit,
} from './shared-field-input'

const MARKER = '‹multiple values›'

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

describe('multipleText', () => {
  it('lists every value after the marker, in the order of the counts', () => {
    const choices = [
      { values: ['Book one'], files: 2 },
      { values: ['Book two'], files: 1 },
    ]
    expect(multipleText(choices)).toBe(`${MARKER} Book one; Book two`)
  })

  it('flattens multi-value choices and drops repeats', () => {
    const choices = [
      { values: ['audiokniha', 'detektivka'], files: 2 },
      { values: ['audiokniha', 'horor'], files: 1 },
    ]
    expect(multipleText(choices)).toBe(`${MARKER} audiokniha; detektivka; horor`)
  })

  it('is the bare marker when no choices are known', () => {
    expect(multipleText([])).toBe(MARKER)
  })
})

describe('parseSharedEdit', () => {
  const original = `${MARKER} Book one; Book two`

  it('is null when the text is unchanged', () => {
    expect(parseSharedEdit('album', original, original)).toBeNull()
  })

  it('is null while the marker is still in the text, however the rest changed', () => {
    expect(parseSharedEdit('album', original, `${MARKER} Book one`)).toBeNull()
    expect(parseSharedEdit('album', original, `${MARKER} Book one; Book two; more`)).toBeNull()
  })

  it('removing the marker sets what is left on every file', () => {
    expect(parseSharedEdit('album', original, 'Book one; Book two')).toEqual({
      kind: 'setAll',
      field: 'album',
      values: ['Book one; Book two'],
    })
  })

  it('splits a multi-value field into its values', () => {
    expect(parseSharedEdit('genre', `${MARKER} audiokniha; detektivka`, 'audiokniha; mluvene slovo')).toEqual({
      kind: 'setAll',
      field: 'genre',
      values: ['audiokniha', 'mluvene slovo'],
    })
  })

  it('an empty box clears the field on every file', () => {
    expect(parseSharedEdit('album', original, '')).toEqual({ kind: 'setAll', field: 'album', values: [] })
  })

  it('a row with one shared value: unchanged is null, typed is a setAll', () => {
    expect(parseSharedEdit('artist', 'Karel Čapek', 'Karel Čapek')).toBeNull()
    expect(parseSharedEdit('artist', 'Karel Čapek', 'Josef Čapek')).toEqual({
      kind: 'setAll',
      field: 'artist',
      values: ['Josef Čapek'],
    })
  })
})

describe('a list cut at the cap', () => {
  const choices = [
    { values: ['Chapter 1'], files: 1 },
    { values: ['Chapter 2'], files: 1 },
  ]
  const original = multipleText(choices, 10)

  it('ends with the tail that counts the values left out', () => {
    expect(original).toBe(`${MARKER} Chapter 1; Chapter 2; …and 10 more`)
  })

  it('has no tail when nothing was cut', () => {
    expect(multipleText(choices, 0)).toBe(`${MARKER} Chapter 1; Chapter 2`)
  })

  it('removing the marker but keeping the tail sends nothing (the rest would be erased)', () => {
    expect(parseSharedEdit('album', original, 'Chapter 1; Chapter 2; …and 10 more', 10)).toBeNull()
  })

  it('removing marker and tail is a deliberate setAll of what is left', () => {
    expect(parseSharedEdit('album', original, 'Chapter 1', 10)).toEqual({
      kind: 'setAll',
      field: 'album',
      values: ['Chapter 1'],
    })
  })
})

describe('marker detection', () => {
  const original = `${MARKER} Book one; Book two`

  it('only a marker at the start counts; a half-deleted marker is a value', () => {
    expect(parseSharedEdit('album', original, '‹multiple values')).toEqual({
      kind: 'setAll',
      field: 'album',
      values: ['‹multiple values'],
    })
    expect(parseSharedEdit('album', original, `Book one ${MARKER}`)).toEqual({
      kind: 'setAll',
      field: 'album',
      values: [`Book one ${MARKER}`],
    })
  })
})

describe('in another language', () => {
  afterEach(() => {
    void setLocale(null)
    _setCatalogForTests('zz', null)
    _clearCompiledCacheForTests()
  })

  it('the box and the parser both use the translated marker', async () => {
    _setCatalogForTests('zz', {
      'multiRename.tags.multipleValues': '‹vice hodnot›',
      'multiRename.tags.choicesMore': '…a dalsich {countText}',
    })
    await setLocale('zz')
    const text = multipleText([{ values: ['A'], files: 1 }], 3)
    expect(text).toBe('‹vice hodnot› A; …a dalsich 3')
    expect(parseSharedEdit('album', text, '‹vice hodnot› A', 3)).toBeNull()
    expect(parseSharedEdit('album', text, 'A; …a dalsich 3', 3)).toBeNull()
    expect(parseSharedEdit('album', text, 'A', 3)).toEqual({ kind: 'setAll', field: 'album', values: ['A'] })
  })
})
