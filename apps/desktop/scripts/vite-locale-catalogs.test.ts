/**
 * The catalog plugin's two virtual modules, built from a fixture messages dir.
 *
 * What matters: a non-locale dir never becomes a language (or reaches the bundle), metadata never
 * ships, a catalog is the merge of its files, and the manifest carries exactly the keys every
 * window needs from every catalog.
 */

import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import {
  CATALOG_MODULE_PREFIX,
  MANIFEST_MODULE_ID,
  catalogModuleCode,
  localeCatalogs,
  manifestModuleCode,
  mergedCatalog,
  shippedLocaleTags,
} from './vite-locale-catalogs.ts'

let root = ''

function write(tag: string, file: string, content: Record<string, unknown>): void {
  mkdirSync(join(root, tag), { recursive: true })
  writeFileSync(join(root, tag, file), JSON.stringify(content))
}

beforeAll(() => {
  root = mkdtempSync(join(tmpdir(), 'cmdr-locale-catalogs-'))
  write('en', 'settings.json', {
    'settings.appearance.language.label': 'Language',
    '@settings.appearance.language.label': { description: 'The Language row.' },
  })
  write('en', 'menu.json', { 'menu.app.quit': 'Quit Cmdr', '@menu.app.quit': { description: 'VERB.' } })
  write('de', 'settings.json', {
    'settings.appearance.language.label': 'Sprache',
    '@settings.appearance.language.label': { sourceHash: 'abc1234' },
  })
  write('de', 'menu.json', { 'menu.app.quit': 'Cmdr beenden' })
  write('en-GB', 'menu.json', { 'menu.app.quit': 'Quit Cmdr, mate' })
  write('screenshots', 'capture-report.json', { surfaces: ['a'] })
})

afterAll(() => {
  rmSync(root, { recursive: true, force: true })
})

/** Evaluates generated module code, so assertions read values rather than source text. */
async function evaluate(code: string): Promise<Record<string, unknown>> {
  // Loader bodies reference virtual ids no Node resolver knows; they're asserted as text.
  const runnable = code.replace(/\(\) => import\(("[^"]+")\)/g, '$1')
  return (await import(`data:text/javascript,${encodeURIComponent(runnable)}`)) as Record<string, unknown>
}

describe('which dirs are locales', () => {
  it('ships the locale dirs and never the screenshots tooling dir', () => {
    expect(shippedLocaleTags(root)).toEqual(['de', 'en', 'en-GB'])
  })
})

describe('a catalog module', () => {
  it('merges every file of the locale and drops the @key metadata', () => {
    expect(mergedCatalog(root, 'en')).toEqual({
      'settings.appearance.language.label': 'Language',
      'menu.app.quit': 'Quit Cmdr',
    })
  })

  it('exports the merged catalog as its default, non-ASCII and ICU text byte-for-byte', async () => {
    write('hu', 'a.json', { 'a.icu': '{count, plural, one {# fájl} other {# fájl}}', 'a.zh': '关于 Cmdr' })
    const mod = await evaluate(catalogModuleCode(root, 'hu'))
    expect(mod.default).toEqual({ 'a.icu': '{count, plural, one {# fájl} other {# fájl}}', 'a.zh': '关于 Cmdr' })
    rmSync(join(root, 'hu'), { recursive: true })
  })
})

describe('the manifest module', () => {
  it('lists the shipped tags, the manifest keys from each catalog, and a loader per non-English tag', async () => {
    const code = manifestModuleCode(root)
    const mod = await evaluate(code)
    expect(mod.shippedLocales).toEqual(['de', 'en', 'en-GB'])
    expect(mod.manifestMessages).toEqual({
      de: { 'settings.appearance.language.label': 'Sprache' },
      en: { 'settings.appearance.language.label': 'Language' },
      // An overlay that doesn't fork the key carries nothing; the runtime walks its chain.
      'en-GB': {},
    })
    expect(Object.keys(mod.loaders as object)).toEqual(['de', 'en-GB'])
    expect(code).toContain(`() => import("${CATALOG_MODULE_PREFIX}de")`)
    expect(code).not.toContain('screenshots')
  })
})

describe('the plugin', () => {
  const plugin = localeCatalogs({ messagesRoot: '' })

  it('claims only its own virtual ids', () => {
    expect(plugin.resolveId(MANIFEST_MODULE_ID)).toBe(`\0${MANIFEST_MODULE_ID}`)
    expect(plugin.resolveId(`${CATALOG_MODULE_PREFIX}de`)).toBe(`\0${CATALOG_MODULE_PREFIX}de`)
    expect(plugin.resolveId('./messages/en/menu.json')).toBeNull()
  })

  it('serves a catalog and watches its files, and refuses a tag that does not ship', () => {
    const own = localeCatalogs({ messagesRoot: root })
    const watched: string[] = []
    const ctx = { addWatchFile: (file: string) => watched.push(file) }
    expect(own.load.call(ctx, `\0${CATALOG_MODULE_PREFIX}de`)).toBe(catalogModuleCode(root, 'de'))
    expect(watched).toEqual([join(root, 'de', 'menu.json'), join(root, 'de', 'settings.json')])
    expect(() => own.load.call(ctx, `\0${CATALOG_MODULE_PREFIX}screenshots`)).toThrow()
  })
})
