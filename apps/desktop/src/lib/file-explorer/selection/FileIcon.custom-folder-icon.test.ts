/**
 * Tests that `FileIcon.svelte` DRAWS a folder's Finder custom icon.
 *
 * The regression this pins: a custom-icon folder keeps the generic `dir` iconId
 * (the backend defers the `kHasCustomIcon` getxattr off the bulk-listing hot
 * path), so its icon is cached under a `path:{dir}` key that no entry points at.
 * Looking up `iconId` alone drew the generic folder over an icon
 * `prefetchCustomFolderIcons` had already fetched and cached — the icon arrived
 * and was silently thrown away.
 *
 * Also pins which custom icons get the gold recolor: only a Customize Folder
 * symbol or emoji macOS tinted with the system accent, never pasted artwork. An
 * emoji folder under gold is drawn as the gold plain folder plus the emoji on
 * top, since the grayscale-first filter would turn the emoji gold-sepia (#814).
 */
import { describe, it, expect, vi, afterEach } from 'vitest'
import { mount, unmount, flushSync } from 'svelte'

const GENERIC_URL = '/icons/generic-folder.svg'
const CUSTOM_URL = '/icons/custom-artwork.svg'
const CUSTOM_PATH = '/Users/test/Projects'
const SYMBOL_URL = '/icons/accent-symbol-folder.svg'
const SYMBOL_PATH = '/Users/test/checkmark design'
const EMOJI_URL = '/icons/accent-emoji-folder.svg'
const EMOJI_PATH = '/Users/test/Pizza'

const gold = vi.hoisted(() => ({ on: true }))

vi.mock('#lib/icon-cache.js', async () => {
  const { writable } = await import('svelte/store')
  return {
    getCachedIcon: () => GENERIC_URL,
    // One folder carries pasted artwork, one a Customize Folder symbol on the accent color.
    getCachedCustomFolderIcon: (path: string): string | undefined =>
      ({ [CUSTOM_PATH]: CUSTOM_URL, [SYMBOL_PATH]: SYMBOL_URL, [EMOJI_PATH]: EMOJI_URL })[path],
    isAccentTintedCustomFolder: (path: string) => path === SYMBOL_PATH || path === EMOJI_PATH,
    getAccentTintedCustomFolderEmoji: (path: string) => (path === EMOJI_PATH ? '🍕' : undefined),
    iconCacheVersion: writable(0),
  }
})

// Gold is ON unless a test says otherwise, so the custom-icon cases also prove the
// recolor filter keeps its hands off the user's own artwork.
vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({
  getFileSizeFormat: () => 'binary',
  getIsCmdrGold: () => gold.on,
}))

import FileIcon from './FileIcon.svelte'

const baseEntry = {
  name: 'Projects',
  path: CUSTOM_PATH,
  isDirectory: true,
  isSymlink: false,
  size: 0,
  modifiedAt: 1710000000,
  iconId: 'dir',
  permissions: 420,
  owner: 'test',
  group: 'staff',
}

let target: HTMLElement | undefined
let app: ReturnType<typeof mount> | undefined

function render(overrides: Partial<typeof baseEntry> = {}): HTMLImageElement {
  target = document.createElement('div')
  document.body.appendChild(target)
  app = mount(FileIcon, { target, props: { file: { ...baseEntry, ...overrides } } })
  flushSync()
  const img = target.querySelector('img.icon')
  if (!img) throw new Error('no icon img rendered')
  return img as HTMLImageElement
}

afterEach(() => {
  gold.on = true
  if (app) void unmount(app)
  app = undefined
  target?.remove()
  target = undefined
})

describe('FileIcon custom folder icons', () => {
  it('draws the custom icon for a folder that has one, despite its generic dir iconId', () => {
    expect(render().getAttribute('src')).toBe(CUSTOM_URL)
  })

  it('never gold-recolors pasted artwork (it would repaint the user artwork)', () => {
    expect(render().classList.contains('gold-folder')).toBe(false)
  })

  it('gold-recolors a Customize Folder symbol that macOS drew in the system accent (#110)', () => {
    const img = render({ name: 'checkmark design', path: SYMBOL_PATH })
    expect(img.getAttribute('src')).toBe(SYMBOL_URL)
    expect(img.classList.contains('gold-folder')).toBe(true)
  })

  it('draws an emoji folder as the gold plain folder with the emoji on top in its own colors (#814)', () => {
    const img = render({ name: 'Pizza', path: EMOJI_PATH })
    // The base is the plain folder, recolored like any other.
    expect(img.getAttribute('src')).toBe(GENERIC_URL)
    expect(img.classList.contains('gold-folder')).toBe(true)
    // The emoji is a sibling, so the filter never reaches it.
    const emoji = target?.querySelector('.folder-emoji')
    expect(emoji?.textContent).toBe('🍕')
    expect(emoji?.closest('.gold-folder')).toBeNull()
  })

  it('leaves an emoji folder as macOS drew it when gold is off', () => {
    gold.on = false
    const img = render({ name: 'Pizza', path: EMOJI_PATH })
    expect(img.getAttribute('src')).toBe(EMOJI_URL)
    expect(img.classList.contains('gold-folder')).toBe(false)
    expect(target?.querySelector('.folder-emoji')).toBeNull()
  })

  it('keeps a symbol folder as one recolored bitmap, with no emoji layer', () => {
    render({ name: 'checkmark design', path: SYMBOL_PATH })
    expect(target?.querySelector('.folder-emoji')).toBeNull()
  })

  it('still draws the generic folder icon for a folder without a custom one', () => {
    const img = render({ path: '/Users/test/Plain' })
    expect(img.getAttribute('src')).toBe(GENERIC_URL)
    // Untouched by the custom-icon path, so gold still applies.
    expect(img.classList.contains('gold-folder')).toBe(true)
  })

  it('leaves a symlinked directory on its generic icon', () => {
    // Mirrors `prefetchCustomFolderIcons`, which never asks about symlinks: the
    // link-badged glyph is the salient signal.
    const img = render({ isSymlink: true, iconId: 'symlink-dir' })
    expect(img.getAttribute('src')).toBe(GENERIC_URL)
  })

  it('leaves files alone even at a path that collides with a custom-icon folder', () => {
    const img = render({ isDirectory: false, iconId: 'ext:md' })
    expect(img.getAttribute('src')).toBe(GENERIC_URL)
  })
})
