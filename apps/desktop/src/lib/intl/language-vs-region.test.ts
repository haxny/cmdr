/**
 * Language and region are two settings, and this is the test that says how the
 * app splits them.
 *
 * macOS keeps them apart (System Settings > General > Language & Region). The
 * app gives each the job it reads best at: NUMBERS follow the app language,
 * because they sit inside its sentences ("612.00 MB", "$0.83", "54.5s" in an
 * English UI), while DATES, times, and the week start follow the region, the
 * conventions people chose and care about (ISO-like dates, Monday first).
 *
 * The scenarios: a Hungarian speaker in Sweden (UI `hu`, OS `sv-SE`) and David's
 * own Mac (English UI, Swedish region, `en-SE`).
 */
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import {
  _setFormatLocaleForTests,
  _setLocaleForTests,
  getFormatLocale,
  getNumberLocale,
  getUiLocale,
  setOsFormatLocale,
} from './locale'
import { setLocale, tString } from './messages.svelte'
import { formatDateForDisplay } from '#lib/settings/format-utils.js'
import { formatFileSizeWithFormat } from '#lib/units/index.js'
import { formatNumber } from '#lib/file-explorer/selection/selection-info-utils.js'

const swedishDate = (ts: number): string =>
  new Intl.DateTimeFormat('sv-SE', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  })
    .formatToParts(new Date(ts * 1000))
    .map((p) => p.value)
    .join('')

describe('a Hungarian speaker on a Swedish Mac', () => {
  beforeEach(() => {
    _setLocaleForTests('hu')
    _setFormatLocaleForTests('sv-SE')
  })
  afterEach(() => {
    _setLocaleForTests(null)
    _setFormatLocaleForTests(null)
  })

  it('reads three locales apart', () => {
    expect(getUiLocale()).toBe('hu')
    expect(getNumberLocale()).toBe('hu')
    expect(getFormatLocale()).toBe('sv-SE')
  })

  it('shows Hungarian copy', () => {
    expect(tString('fileExplorer.columns.size')).toBe('Méret')
    expect(tString('fileExplorer.columns.modified')).toBe('Módosítva')
  })

  it('shows Hungarian numbers', () => {
    expect(formatFileSizeWithFormat(1536, 'binary')).toBe('1,50 KiB')
    expect(formatNumber(1234567)).toBe(new Intl.NumberFormat('hu').format(1234567))
  })

  it("shows a Swedish 'system' date, not a Hungarian one", () => {
    const ts = new Date(2024, 2, 15, 14, 30, 45).getTime() / 1000
    expect(formatDateForDisplay(ts, 'system', '').text).toBe(swedishDate(ts))
  })
})

describe('a US-English Mac with a Swedish region', () => {
  // David's own machine, and the case that motivated composing the tag in Rust:
  // `AppleLocale = en_US@rg=sezzzz`. Rust hands us `en-SE`, which reproduces
  // Finder's dates. Numbers stay English: "1,234,567", "612.00 MB".
  beforeEach(() => {
    setOsFormatLocale('en-SE')
  })
  afterEach(() => {
    setOsFormatLocale(null)
    void setLocale(null)
    _setLocaleForTests(null)
  })

  it('writes English numbers inside English copy', () => {
    void setLocale('en')
    expect(getFormatLocale()).toBe('en-SE')
    expect(getNumberLocale()).toBe('en')
    expect(formatNumber(1234567)).toBe('1,234,567')
    expect(formatFileSizeWithFormat(612_000_000, 'si')).toBe('612.00 MB')
  })

  it('writes the date Finder writes', () => {
    const ts = new Date(2024, 2, 15, 14, 30, 45).getTime() / 1000
    expect(formatDateForDisplay(ts, 'system', '').text).toBe('2024-03-15, 14:30')
  })

  it('moves numbers with the app language and keeps dates on the region', () => {
    void setLocale('hu')

    expect(tString('fileExplorer.columns.size')).toBe('Méret')
    expect(getNumberLocale()).toBe('hu')
    expect(formatNumber(1234567)).toBe(new Intl.NumberFormat('hu').format(1234567))
    const ts = new Date(2024, 2, 15, 14, 30, 45).getTime() / 1000
    expect(formatDateForDisplay(ts, 'system', '').text).toBe('2024-03-15, 14:30')
  })

  it('falls back to the webview default when the OS has no answer', () => {
    // An unreadable or missing region composes nothing rather than a malformed
    // tag, and off macOS there's no answer at all. Either way the webview's own
    // locale stands, which is a working answer.
    setOsFormatLocale(null)

    expect(getFormatLocale()).toBe(new Intl.NumberFormat().resolvedOptions().locale)
  })
})

describe('the number locale is the resolved CATALOG tag', () => {
  afterEach(() => {
    void setLocale(null)
  })

  it('keeps a regional overlay', () => {
    void setLocale('en-GB')
    expect(getNumberLocale()).toBe('en-GB')
  })

  it('writes English numbers where no catalog covers the language', () => {
    // A Japanese Mac reads English text (no `ja` catalog), so its numbers are English too.
    void setLocale('ja-JP')
    expect(getNumberLocale()).toBe('en')
  })
})

describe('the Language picker', () => {
  afterEach(() => {
    void setLocale(null)
    _setFormatLocaleForTests(null)
  })

  it('switches copy and numbers, and leaves the dates alone', () => {
    _setFormatLocaleForTests('sv-SE')
    void setLocale('en')
    expect(formatNumber(1234567)).toBe('1,234,567')
    const ts = new Date(2024, 2, 15, 14, 30, 45).getTime() / 1000
    const swedish = swedishDate(ts)

    void setLocale('de')

    expect(tString('fileExplorer.columns.size')).toBe('Größe')
    expect(getFormatLocale()).toBe('sv-SE')
    expect(formatNumber(1234567)).toBe('1.234.567')
    expect(formatDateForDisplay(ts, 'system', '').text).toBe(swedish)
  })
})
