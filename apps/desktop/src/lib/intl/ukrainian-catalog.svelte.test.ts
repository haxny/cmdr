/** Integration coverage for the shipped Ukrainian catalog and its count-sensitive grammar. */
import { afterEach, describe, expect, it } from 'vitest'
import {
  _clearCompiledCacheForTests,
  availableLocales,
  getMessage,
  resolvedCatalogLocale,
  setLocale,
  tString,
} from './messages.svelte'
import { _setLocaleForTests } from './locale'

/** Restore the shared locale, compiled messages, and document language after each test. */
afterEach(() => {
  void setLocale(null)
  _setLocaleForTests(null)
  _clearCompiledCacheForTests()
  document.documentElement.lang = 'en'
})

/** Exercise production catalog discovery and formatting without injecting a test catalog. */
describe('the shipped Ukrainian catalog', () => {
  /** A regional OS language must discover Ukrainian and localize both HTML and native menus. */
  it('resolves uk-UA to Ukrainian and exposes translated raw menu labels', () => {
    void setLocale('uk-UA')
    expect(availableLocales()).toContain('uk')
    expect(resolvedCatalogLocale('uk-UA')).toBe('uk')
    expect(document.documentElement.lang).toBe('uk')
    expect(getMessage('menu.bar.file')).toBe('Файл')
    expect(getMessage('menu.bar.help')).toBe('Довідка')
  })

  /** Ukrainian `one` includes 21 and 101, `few` 2–4 and 22, `many` 5–20 and 25; every branch keeps the count. */
  it('renders the real file counter across Ukrainian plural boundaries', () => {
    void setLocale('uk')
    const cases: [number, string][] = [
      [1, 'файл'],
      [2, 'файли'],
      [5, 'файлів'],
      [11, 'файлів'],
      [21, 'файл'],
      [22, 'файли'],
      [25, 'файлів'],
      [101, 'файл'],
    ]
    for (const [count, noun] of cases) {
      expect(tString('transfer.delete', { count, countText: String(count) })).toBe(
        `Видалення завершено: ${String(count)} ${noun}`,
      )
    }
  })

  /** Only exactly one operation may use the countless wording; 21 is also CLDR `one` but shows its number. */
  it('distinguishes exactly one operation from 21 operations', () => {
    void setLocale('uk')
    expect(tString('main.quit.title', { count: 1, countText: '1' })).toBe('Завершити, поки виконується операція?')
    expect(tString('main.quit.title', { count: 21, countText: '21' })).toBe('Завершити, поки виконується 21 операція?')
  })
})
