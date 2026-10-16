/**
 * How a recovered state file reaches the person (#461): the main window drains the backend's
 * queue at start and on each nudge, and shows one persistent warning per recovery, once.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { StateRecovery } from '#lib/ipc/bindings.js'

const { addToast, takeStateRecoveries } = vi.hoisted(() => ({
  addToast: vi.fn(),
  takeStateRecoveries: vi.fn<() => Promise<StateRecovery[]>>(),
}))
vi.mock('#lib/ui/toast/index.js', () => ({ addToast, dismissToast: vi.fn() }))

const unlisten = vi.fn()
let nudge: (() => void) | null = null
vi.mock('#lib/tauri-commands/index.js', () => ({
  takeStateRecoveries,
  onStateRecoveryQueued: (callback: () => void) => {
    nudge = callback
    return Promise.resolve(unlisten)
  },
}))

import { announceStateRecovery, startStateRecoveryNotices, stopStateRecoveryNotices } from './state-recovery'
import StateRecoveryToastContent from './StateRecoveryToastContent.svelte'

const settings: StateRecovery = {
  file: 'settings',
  keptCopy: '/data/settings.json.broken-2026-10-16T101500',
}
const favorites: StateRecovery = { file: 'favorites', keptCopy: null }

beforeEach(() => {
  stopStateRecoveryNotices()
  vi.clearAllMocks()
  nudge = null
})

describe('announceStateRecovery', () => {
  it('raises a persistent warning carrying the recovery', () => {
    announceStateRecovery(settings)
    const [content, options] = addToast.mock.calls.at(-1) as [unknown, Record<string, unknown>]
    expect(content).toBe(StateRecoveryToastContent)
    expect(options).toMatchObject({ level: 'warn', dismissal: 'persistent', props: { recovery: settings } })
  })

  it('gives two recoveries of the same file two toasts', () => {
    announceStateRecovery(favorites)
    announceStateRecovery(favorites)
    const ids = addToast.mock.calls.map((call) => (call[1] as { id: string }).id)
    expect(new Set(ids).size).toBe(2)
  })
})

describe('startStateRecoveryNotices', () => {
  it('shows what launch found once, and nothing when a nudge finds the queue empty', async () => {
    takeStateRecoveries.mockResolvedValueOnce([settings, favorites]).mockResolvedValue([])
    await startStateRecoveryNotices()
    expect(addToast).toHaveBeenCalledTimes(2)

    nudge?.()
    await vi.waitFor(() => {
      expect(takeStateRecoveries).toHaveBeenCalledTimes(2)
    })
    expect(addToast).toHaveBeenCalledTimes(2)
  })

  it('shows a recovery found later when the nudge arrives', async () => {
    takeStateRecoveries.mockResolvedValueOnce([]).mockResolvedValueOnce([favorites])
    await startStateRecoveryNotices()
    expect(addToast).not.toHaveBeenCalled()

    nudge?.()
    await vi.waitFor(() => {
      expect(addToast).toHaveBeenCalledOnce()
    })
  })

  it('keeps going when the queue can’t be read, and stops listening on stop', async () => {
    takeStateRecoveries.mockRejectedValueOnce(new Error('IPC down'))
    await startStateRecoveryNotices()
    expect(addToast).not.toHaveBeenCalled()
    stopStateRecoveryNotices()
    expect(unlisten).toHaveBeenCalledOnce()
  })
})
