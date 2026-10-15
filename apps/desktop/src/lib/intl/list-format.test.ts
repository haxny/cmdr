import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { _setLocaleForTests } from './locale'
import {
  _clearListFormatCacheForTests,
  formatConjunctionList,
  formatDisjunctionList,
  formatUnitList,
} from './list-format'

describe('formatDisjunctionList', () => {
  afterEach(() => {
    _setLocaleForTests(null)
  })

  it('reads as one name, two joined by "or", and an Oxford-comma list from three', () => {
    _setLocaleForTests('en-US')
    expect(formatDisjunctionList([])).toBe('')
    expect(formatDisjunctionList(['dates'])).toBe('dates')
    expect(formatDisjunctionList(['dates', 'tags'])).toBe('dates or tags')
    expect(formatDisjunctionList(['dates', 'tags', 'flags'])).toBe('dates, tags, or flags')
  })

  it('joins in the UI language', () => {
    _setLocaleForTests('de-DE')
    expect(formatDisjunctionList(['Daten', 'Tags'])).toBe('Daten oder Tags')
  })
})

describe('formatConjunctionList', () => {
  afterEach(() => {
    _setLocaleForTests(null)
  })

  it('reads as one name, two joined by "and", and an Oxford-comma list from three', () => {
    _setLocaleForTests('en-US')
    expect(formatConjunctionList([])).toBe('')
    expect(formatConjunctionList(['Preview'])).toBe('Preview')
    expect(formatConjunctionList(['Preview', 'Warp'])).toBe('Preview and Warp')
    expect(formatConjunctionList(['Preview', 'Warp', 'Photos'])).toBe('Preview, Warp, and Photos')
  })

  it('joins in the UI language, so a German toast never reads an English "and"', () => {
    _setLocaleForTests('de-DE')
    expect(formatConjunctionList(['Preview', 'Warp'])).toBe('Preview und Warp')
  })

  it('spaces a Chinese conjunction off the Latin names beside it, in both scripts', () => {
    // CLDR joins tight (`Warp和其他 App`); both Chinese style guides space Han
    // against Latin. The enumeration comma is full-width punctuation: no space.
    _setLocaleForTests('zh-Hant')
    expect(formatConjunctionList(['Preview', 'Warp', 'Photos', '其他 App'])).toBe('Preview、Warp、Photos 和其他 App')
    _setLocaleForTests('zh')
    expect(formatConjunctionList(['Preview', 'Warp'])).toBe('Preview 和 Warp')
    expect(formatConjunctionList(['预览', 'Warp'])).toBe('预览和 Warp')
  })

  it('leaves Japanese tight, since it never spaces kana against Latin', () => {
    _setLocaleForTests('ja')
    expect(formatConjunctionList(['Preview', 'Warp'])).toBe(
      new Intl.ListFormat('ja', { type: 'conjunction' }).format(['Preview', 'Warp']),
    )
  })
})

describe('formatUnitList', () => {
  afterEach(() => {
    _setLocaleForTests(null)
  })

  it('lists standalone facts with no conjunction, in the UI language', () => {
    _setLocaleForTests('en-US')
    expect(formatUnitList([])).toBe('')
    expect(formatUnitList(['Photos', 'copying', '3 of 10 files'])).toBe('Photos, copying, 3 of 10 files')
    _setLocaleForTests('ja')
    expect(formatUnitList(['写真', 'コピー中'])).toBe('写真、コピー中')
  })

  // CLDR's `unit` list ends "B und C" in German and runs Chinese items together with no
  // separator; either breaks an accessible name that has to carry visible words in order.
  it('never adds a conjunction word, and always separates', () => {
    _setLocaleForTests('de')
    expect(formatUnitList(['A', 'B', 'C'])).toBe('A, B, C')
    _setLocaleForTests('zh')
    expect(formatUnitList(['A', 'B', 'C'])).toBe('A、B、C')
  })
})

describe('formatConjunctionList (memoization)', () => {
  beforeEach(() => {
    _clearListFormatCacheForTests()
  })

  afterEach(() => {
    _setLocaleForTests(null)
    vi.unstubAllGlobals()
  })

  it('reuses one Intl.ListFormat instance for a locale, and builds a fresh one when the locale changes', () => {
    // A stand-in constructor swapped in wholesale, rather than a spy on the real
    // one: a spied constructor hands back an instance carrying the SPY's
    // prototype, so the stand-in's own `format` never reaches the caller.
    const builtFor: string[] = []
    class CountingListFormat {
      constructor(locale: string) {
        builtFor.push(locale)
      }
      format(items: string[]): string {
        return items.join(' + ')
      }
    }
    vi.stubGlobal('Intl', { ...Intl, ListFormat: CountingListFormat })

    _setLocaleForTests('en-US')
    for (let i = 0; i < 20; i++) formatConjunctionList(['Preview', 'Warp'])
    expect(builtFor).toEqual(['en-US'])

    _setLocaleForTests('de-DE')
    formatConjunctionList(['Preview', 'Warp'])
    expect(builtFor).toEqual(['en-US', 'de-DE'])
  })
})
