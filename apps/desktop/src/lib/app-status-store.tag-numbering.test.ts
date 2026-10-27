/**
 * The numbering wizard's remembered choices (fork): what a save left on disk, and what reading it back hands the wizard. Same
 * fake `@tauri-apps/plugin-store` rig as `app-status-store.progress-dialog.test.ts`.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { loadTagNumbering, saveTagNumbering } from './app-status-store'

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

describe('the tag numbering choices', () => {
  beforeEach(() => {
    disk.clear()
  })

  it('is nothing until the wizard saved some', async () => {
    expect(await loadTagNumbering()).toBeNull()
  })

  it('reads back what was saved', async () => {
    const options = { trackStart: 2, trackTotal: false, trackPad: true, disc: null }
    await saveTagNumbering(options)
    expect(await loadTagNumbering()).toEqual(options)
    expect(disk.get('tagNumbering')).toEqual(options)
  })
})
