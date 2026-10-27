/**
 * The Tags tab's remembered write kind (fork): what a pick left on disk, and what reading it back hands the tab. Same
 * fake `@tauri-apps/plugin-store` rig as `app-status-store.progress-dialog.test.ts`.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { loadTagWriteKind, saveTagWriteKind } from './app-status-store'

const disk = vi.hoisted(() => new Map<string, unknown>())

vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn(() =>
    Promise.resolve({
      get: (key: string) => Promise.resolve(disk.get(key)),
      set: (key: string, value: unknown) => {
        disk.set(key, value)
        return Promise.resolve()
      },
      save: () => Promise.resolve(),
    }),
  ),
}))

vi.mock('./settings/store-path', () => ({
  resolveStorePath: (name: string) => Promise.resolve(name),
}))

describe('the tag write kind', () => {
  beforeEach(() => {
    disk.clear()
  })

  it('is nothing until the user picks one', async () => {
    expect(await loadTagWriteKind()).toBeNull()
  })

  it('reads back what a pick saved', async () => {
    await saveTagWriteKind('music')
    expect(await loadTagWriteKind()).toBe('music')
  })

  it('ignores a stored value that isn’t text', async () => {
    disk.set('tagWriteKind', 3)
    expect(await loadTagWriteKind()).toBeNull()
  })
})
