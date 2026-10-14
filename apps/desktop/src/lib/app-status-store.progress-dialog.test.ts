/**
 * The progress dialog's remembered height (#428): what a drag left on disk, and
 * what reading it back hands the dialog. A shared `disk` Map backs a fake
 * `@tauri-apps/plugin-store`, as in `app-status-store.first-run.test.ts`.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { loadProgressDialogHeight, saveProgressDialogHeight } from './app-status-store'

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

describe('the progress dialog height', () => {
  beforeEach(() => {
    disk.clear()
  })

  it('is nothing until the user has dragged one', async () => {
    expect(await loadProgressDialogHeight()).toBeNull()
  })

  it('reads back what a drag saved, in whole pixels', async () => {
    await saveProgressDialogHeight(512.4)
    expect(await loadProgressDialogHeight()).toBe(512)
  })

  it('ignores a stored value that isn’t a usable height', async () => {
    disk.set('progressDialogHeight', 'tall')
    expect(await loadProgressDialogHeight()).toBeNull()
    disk.set('progressDialogHeight', -20)
    expect(await loadProgressDialogHeight()).toBeNull()
  })
})
