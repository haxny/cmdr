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
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
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
} from './messages.svelte'
import { getUiLocale } from './locale'
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
