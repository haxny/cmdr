import { afterAll, afterEach, beforeAll, describe, expect, it } from 'vitest'

import { formatNarrowDuration } from './duration-format'
import { _setLocaleForTests } from './locale'
import {
  _clearListFormatCacheForTests,
  conjunctionListParts,
  formatConjunctionList,
  formatDisjunctionList,
  formatUnitList,
} from './list-format'

// Regression anchor: cmdr-reports#80 (ERR-5DKYE). Catalina's WebKit has no
// `Intl.ListFormat` at all, even Safari 15.6.1's staged copy, and every file row's
// accessible name joins through this module, so the main window crashed at startup.
// Deleting the constructor is exactly what that engine looks like.
const listFormatDescriptor = Object.getOwnPropertyDescriptor(Intl, 'ListFormat')

describe('without Intl.ListFormat (Catalina WebKit)', () => {
  beforeAll(() => {
    Reflect.deleteProperty(Intl, 'ListFormat')
  })

  afterAll(() => {
    if (listFormatDescriptor) Object.defineProperty(Intl, 'ListFormat', listFormatDescriptor)
  })

  afterEach(() => {
    _setLocaleForTests(null)
    _clearListFormatCacheForTests()
  })

  it('joins "and" and "or" lists with the enumeration comma', () => {
    _setLocaleForTests('en-US')
    expect(formatConjunctionList([])).toBe('')
    expect(formatConjunctionList(['Preview'])).toBe('Preview')
    expect(formatConjunctionList(['Preview', 'Warp', 'Photos'])).toBe('Preview, Warp, Photos')
    expect(formatDisjunctionList(['dates', 'tags'])).toBe('dates, tags')
  })

  it('uses the full-width enumeration comma in Chinese', () => {
    _setLocaleForTests('zh-Hant')
    expect(formatConjunctionList(['Preview', 'Warp'])).toBe('Preview、Warp')
    _setLocaleForTests('zh')
    expect(formatUnitList(['照片', '正在复制'])).toBe('照片、正在复制')
  })

  it('splits a list into its items and enumeration commas', () => {
    _setLocaleForTests('en-US')
    expect(conjunctionListParts(['Documents', 'Desktop'])).toEqual([
      { kind: 'item', index: 0 },
      { kind: 'literal', text: ', ' },
      { kind: 'item', index: 1 },
    ])
  })

  it('still lists standalone facts for an accessible name', () => {
    _setLocaleForTests('de')
    expect(formatUnitList(['Photos', 'copying', '3 of 10 files'])).toBe('Photos, copying, 3 of 10 files')
  })

  it('words narrow durations with a space between the units', () => {
    _setLocaleForTests('en-US')
    expect(
      formatNarrowDuration([
        { unit: 'minute', value: 8 },
        { unit: 'second', value: 12 },
      ]),
    ).toBe('8m 12s')
  })
})
