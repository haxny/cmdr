/**
 * Tests for the reserved icon ids `FileIcon.svelte` draws as its own glyphs
 * instead of an OS icon: the git portal's `git:*` rows and a restic repo's
 * `Browse snapshots` row (`restic:snapshots`, set by `cmdr-restic`'s hub).
 */
import { describe, it, expect, vi, afterEach } from 'vitest'
import { mount, unmount, flushSync } from 'svelte'
import type { IconName } from '#lib/ui/icons/icon-map.js'

// Every id resolves, so a row that isn't reserved renders the live `<img>`.
vi.mock('#lib/icon-cache.js', async () => {
  const { writable } = await import('svelte/store')
  return {
    getCachedIcon: () => '/icons/stub.svg',
    getCachedCustomFolderIcon: () => undefined,
    iconCacheVersion: writable(0),
  }
})

vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({
  getFileSizeFormat: () => 'binary',
  getIsCmdrGold: () => false,
}))

import FileIcon from './FileIcon.svelte'
import Icon from '#lib/ui/Icon.svelte'

const baseEntry = {
  name: 'Browse snapshots',
  path: '/Users/test/backup/Browse snapshots',
  isDirectory: true,
  isSymlink: false,
  size: 0,
  modifiedAt: 1710000000,
  iconId: 'dir',
  permissions: 420,
  owner: 'test',
  group: 'staff',
}

const mounted: Array<{ instance: ReturnType<typeof mount>; target: HTMLElement }> = []

// `unknown`, so `FileIcon` and `Icon` both fit: their union reads as a duplicate to
// type-aware ESLint and as two prop shapes to svelte-check, which reject it either way.
function render(component: unknown, props: Record<string, unknown>): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(component as never, { target, props: props as never })
  flushSync()
  mounted.push({ instance, target })
  return target
}

/** The `<svg>` markup `Icon` renders for a registry glyph, to compare against. */
function glyphMarkup(name: IconName): string {
  return render(Icon, { name, size: 16 }).querySelector('svg')?.innerHTML ?? ''
}

afterEach(() => {
  for (const { instance, target } of mounted.splice(0)) {
    void unmount(instance)
    target.remove()
  }
})

describe('FileIcon reserved glyphs', () => {
  it("draws a restic repo's `Browse snapshots` row with the history glyph, not a folder icon", () => {
    const target = render(FileIcon, { file: { ...baseEntry, iconId: 'restic:snapshots' } })
    expect(target.querySelector('img')).toBeNull()
    expect(target.querySelector('svg')?.innerHTML).toBe(glyphMarkup('history'))
  })

  it('keeps drawing git portal ids with their git glyphs', () => {
    const target = render(FileIcon, { file: { ...baseEntry, iconId: 'git:branch' } })
    expect(target.querySelector('img')).toBeNull()
    expect(target.querySelector('svg')?.innerHTML).toBe(glyphMarkup('git-branch'))
  })

  it('draws an ordinary folder from the icon cache', () => {
    const target = render(FileIcon, { file: baseEntry })
    expect(target.querySelector('img')?.getAttribute('src')).toBe('/icons/stub.svg')
  })
})
