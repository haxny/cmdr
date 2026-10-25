/**
 * What a quit keeps. The store plugin holds each store in Rust memory and writes
 * every loaded store to disk at `RunEvent::Exit`, so a change survives a quit once
 * it has reached the plugin's memory, whatever the frontend's disk-save debounce
 * was doing. These tests split the fake plugin into `memory` (what `set` /
 * `delete` touch, and what the Exit save writes) and `disk` (what `save()` writes),
 * and read `memory` as "what the next launch sees".
 *
 * Regression anchor for #698: a Settings change made inside the 500 ms debounce
 * before a quit came back at its earlier value (the language, after arrowing
 * through a few to preview them, and `keepProgressOpenWhenDone`).
 */
import { describe, it, expect, vi, beforeAll, beforeEach, afterEach } from 'vitest'
import type * as SettingsStore from './settings-store'
import { SCHEMA_VERSION } from './settings-migrations'

const memory = vi.hoisted(() => new Map<string, unknown>())
const disk = vi.hoisted(() => new Map<string, unknown>())

/** When set, the next `set` of this key waits for `release()` before it lands. */
const gate = vi.hoisted(() => ({ key: null as string | null, release: null as (() => void) | null }))

vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn(() =>
    Promise.resolve({
      get: (key: string) => Promise.resolve(memory.get(key)),
      set: (key: string, value: unknown) => {
        if (gate.key === key) {
          gate.key = null
          return new Promise<void>((resolve) => {
            gate.release = () => {
              memory.set(key, value)
              resolve()
            }
          })
        }
        memory.set(key, value)
        return Promise.resolve()
      },
      delete: (key: string) => Promise.resolve(memory.delete(key)),
      has: (key: string) => Promise.resolve(memory.has(key)),
      keys: () => Promise.resolve([...memory.keys()]),
      save: () => {
        disk.clear()
        for (const [k, v] of memory) disk.set(k, v)
        return Promise.resolve()
      },
    }),
  ),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
  emit: vi.fn(() => Promise.resolve()),
}))

vi.mock('./store-path', () => ({
  resolveStorePath: (name: string) => Promise.resolve(name),
}))

vi.mock('#lib/tauri-commands/settings.js', () => ({
  recordSettingsDefaults: vi.fn(() => Promise.resolve()),
  getRestrictedWindowSettings: vi.fn(() => Promise.resolve({})),
  persistRestrictedWindowSetting: vi.fn(() => Promise.resolve({ status: 'ok' })),
}))

vi.mock('./settings-registry', async (importOriginal) => await importOriginal())
vi.mock('./settings-migrations', async (importOriginal) => await importOriginal())
vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => await importOriginal())

// Warm Vite's transform cache (see `settings-store.persistence.test.ts`).
beforeAll(async () => {
  await import('./settings-store')
})

/** A fresh window's module state: `vi.resetModules()` + a dynamic import. */
async function openSettingsWindow(): Promise<typeof SettingsStore> {
  vi.resetModules()
  const store = await import('./settings-store')
  await store.initializeSettings()
  return store
}

/** Lets every pending promise chain run, without moving the clock. */
async function settleIpc(): Promise<void> {
  for (let i = 0; i < 20; i++) await Promise.resolve()
}

beforeEach(() => {
  memory.clear()
  disk.clear()
  gate.key = null
  gate.release = null
})

afterEach(() => {
  vi.useRealTimers()
})

describe('a quit inside the save debounce', () => {
  it('keeps the language picked last, not the last one previewed', async () => {
    const settings = await openSettingsWindow()
    vi.useFakeTimers()

    // Arrowing through the picker: each highlighted language is applied, and the
    // user lingers long enough on Deutsch for its debounced save to land.
    settings.setSetting('appearance.language', 'hu')
    settings.setSetting('appearance.language', 'de')
    await vi.advanceTimersByTimeAsync(600)
    expect(disk.get('appearance.language')).toBe('de')

    // Then English, and ⌘Q right away: no timer gets to fire after this.
    settings.setSetting('appearance.language', 'en')
    await settleIpc()

    expect(memory.get('appearance.language')).toBe('en')
  })

  it('keeps a switch flipped right before the quit', async () => {
    const settings = await openSettingsWindow()

    settings.setSetting('fileOperations.keepProgressOpenWhenDone', true)
    await settleIpc()

    expect(memory.get('fileOperations.keepProgressOpenWhenDone')).toBe(true)
    // Stamped with it, so the next launch doesn't re-run every migration over it.
    expect(memory.get('_schemaVersion')).toBe(SCHEMA_VERSION)
  })

  it('keeps a reset made right before the quit', async () => {
    memory.set('fileOperations.keepProgressOpenWhenDone', true)
    const settings = await openSettingsWindow()

    settings.resetSetting('fileOperations.keepProgressOpenWhenDone')
    await settleIpc()

    expect(memory.has('fileOperations.keepProgressOpenWhenDone')).toBe(false)
  })

  it('lands writes in the order they were made, even when an earlier one is slow', async () => {
    const settings = await openSettingsWindow()

    // The plugin's `set` is an async command, so two in flight could finish in
    // either order. A slow 'de' must not land on top of the 'en' made after it.
    gate.key = 'appearance.language'
    settings.setSetting('appearance.language', 'de')
    await settleIpc()
    settings.setSetting('appearance.language', 'en')
    await settleIpc()
    gate.release?.()
    await settleIpc()

    expect(memory.get('appearance.language')).toBe('en')
  })
})
