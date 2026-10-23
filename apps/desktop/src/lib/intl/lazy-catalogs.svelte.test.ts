/**
 * A window holds English and the catalogs its language reads, and loads the rest on demand.
 * What has to hold: switching shows the old language until the new one is complete (never a
 * frame of English or of a half-switched UI), the latest pick wins however the loads land, an
 * overlay brings its base, and catalogs the window stopped reading are dropped.
 *
 * Each test starts from a window's startup shape (`_unloadCatalogsForTests`) and controls when
 * a catalog lands (`_setCatalogLoaderForTests`); `afterEach` restores the preloaded runtime that
 * `test-setup.ts` gives every other test.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount, unmount } from 'svelte'
import { loaders } from 'virtual:cmdr-locale-manifest'
import {
  _loadAllCatalogsForTests,
  _loadedCatalogsForTests,
  _setCatalogLoaderForTests,
  _unloadCatalogsForTests,
  availableLocales,
  manifestMessage,
  setLocale,
  tString,
  tStringIn,
} from './messages.svelte'
import { getUiLocale } from './locale'
import { _forgetStartupCatalogForTests } from './startup-catalog'
import Fixture from './messages-reactivity-fixture.svelte'

/** A loader for `tag`'s real catalog that lands only when the test says so. */
function heldLoader(tag: string): { release: () => void; fail: () => void } {
  let release = (): void => {}
  let fail = (): void => {}
  const gate = new Promise<void>((resolve, reject) => {
    release = resolve
    fail = () => {
      reject(new Error(`chunk for ${tag} did not load`))
    }
  })
  _setCatalogLoaderForTests(tag, () => gate.then(() => loaders[tag]()))
  return { release, fail }
}

const HELD = ['de', 'fr', 'hu', 'ru']

beforeEach(async () => {
  await setLocale('en')
  _unloadCatalogsForTests()
})

afterEach(async () => {
  for (const tag of HELD) _setCatalogLoaderForTests(tag, null)
  await _loadAllCatalogsForTests()
  await setLocale(null)
})

describe('what a window holds', () => {
  it('starts with English only, yet lists every shipped language', () => {
    expect(_loadedCatalogsForTests()).toEqual(['en'])
    expect(availableLocales()).toEqual(expect.arrayContaining(['en', 'de', 'hu', 'ru', 'zh', 'zh-Hant', 'en-GB']))
  })

  it('reads the Language label of any language from the manifest, loading nothing', () => {
    expect(manifestMessage('ru', 'settings.appearance.language.label')).toBe('Язык')
    expect(manifestMessage('hu', 'settings.appearance.language.label')).toBe('Nyelv')
    // An overlay that doesn't fork the key inherits it down its chain, like `t()` does.
    expect(manifestMessage('es-419', 'settings.appearance.language.label')).toBe(
      manifestMessage('es', 'settings.appearance.language.label'),
    )
    expect(_loadedCatalogsForTests()).toEqual(['en'])
  })

  it('loads an overlay together with the base it inherits from', async () => {
    await setLocale('es-419')
    expect(_loadedCatalogsForTests()).toEqual(['en', 'es', 'es-419'])
    expect(tString('settings.appearance.language.label')).toBe(
      manifestMessage('es', 'settings.appearance.language.label'),
    )
  })

  it('refuses text in a language the window doesn’t hold, which would silently read English', async () => {
    expect(() => tStringIn('de', 'settings.appearance.language.label')).toThrow(/not loaded/)
    expect(tStringIn('en', 'settings.appearance.language.label')).toBe('Language')
    await setLocale('de')
    expect(tStringIn('de', 'settings.appearance.language.label')).toBe('Sprache')
  })

  it('drops the catalogs the window no longer reads', async () => {
    await setLocale('de')
    await setLocale('sv')
    expect(_loadedCatalogsForTests()).toEqual(['en', 'sv'])
  })
})

describe('switching', () => {
  it('keeps the OLD language on screen until the new catalog lands, then swaps in one step', async () => {
    await setLocale('fr')
    const target = document.createElement('div')
    document.body.appendChild(target)
    const app = mount(Fixture, { target, props: { messageKey: 'settings.appearance.language.label' } })
    const text = (): string | null | undefined => target.querySelector('[data-test="trans-text"]')?.textContent
    expect(text()).toBe('Langue')

    const de = heldLoader('de')
    const switching = setLocale('de')
    flushSync()
    // Mid-load: all French. No English, no blank, no half-switched locale.
    expect(getUiLocale()).toBe('fr')
    expect(document.documentElement.lang).toBe('fr')
    expect(text()).toBe('Langue')

    de.release()
    await switching
    flushSync()
    expect(getUiLocale()).toBe('de')
    expect(document.documentElement.lang).toBe('de')
    expect(text()).toBe('Sprache')
    await unmount(app)
  })

  it('lets the latest pick win when an older load lands last (arrowing through the picker)', async () => {
    const ru = heldLoader('ru')
    const hu = heldLoader('hu')
    const first = setLocale('ru')
    const second = setLocale('hu')
    hu.release()
    await second
    ru.release()
    await first
    expect(getUiLocale()).toBe('hu')
    expect(tString('settings.appearance.language.label')).toBe('Nyelv')
  })

  it('switches synchronously when the catalogs are already here', async () => {
    await setLocale('de')
    // The current language (an OS-locale re-apply does this) and English are always held.
    void setLocale('de')
    expect(tString('settings.appearance.language.label')).toBe('Sprache')
    void setLocale('en')
    expect(getUiLocale()).toBe('en')
    expect(tString('settings.appearance.language.label')).toBe('Language')
  })

  it('keeps the old language when the new catalog fails to load', async () => {
    await setLocale('hu')
    const fr = heldLoader('fr')
    const switching = setLocale('fr')
    fr.fail()
    await switching
    expect(getUiLocale()).toBe('hu')
    expect(tString('settings.appearance.language.label')).toBe('Nyelv')
  })
})

describe('the catalog a new window starts with', () => {
  // Every Cmdr window shares one origin, so what one window applied is what the next window
  // (or the next launch) most likely speaks.
  const HINT_KEY = 'cmdr.i18n.startupCatalog' // `startup-catalog.ts`

  beforeEach(() => {
    // The file's own `beforeEach` switched to English, which remembered `en`.
    _forgetStartupCatalogForTests()
  })

  /** A fresh runtime, as a newly opened window gets one. */
  async function newWindow(): Promise<typeof import('./messages.svelte') & typeof import('./locale')> {
    vi.resetModules()
    const messages = await import('./messages.svelte')
    const locale = await import('./locale')
    return { ...messages, ...locale }
  }

  it('remembers the catalog a switch landed on', async () => {
    await setLocale('hu')
    expect(localStorage.getItem(HINT_KEY)).toBe('hu')
    await setLocale('pt-PT')
    expect(localStorage.getItem(HINT_KEY)).toBe('pt')
  })

  it('loads the remembered catalog before the first render, and speaks it', async () => {
    localStorage.setItem(HINT_KEY, 'de')
    const win = await newWindow()
    expect(win._loadedCatalogsForTests()).toContain('de')
    expect(win.getUiLocale()).toBe('de')
    expect(win.tString('settings.appearance.language.label')).toBe('Sprache')
    expect(document.documentElement.lang).toBe('de')
  })

  it('starts on the webview language when nothing is remembered', async () => {
    const win = await newWindow()
    expect(win._loadedCatalogsForTests()).toEqual(['en'])
    expect(win.getUiLocale()).toBe(win.defaultUiLocale())
  })

  it('ignores a remembered tag that no longer ships', async () => {
    localStorage.setItem(HINT_KEY, 'tlh')
    const win = await newWindow()
    expect(win._loadedCatalogsForTests()).toEqual(['en'])
    expect(win.getUiLocale()).toBe(win.defaultUiLocale())
  })
})
