/**
 * The recovery queue's two IPC pieces: the drain, and the nudge that carries nothing.
 */

import { describe, it, expect, vi } from 'vitest'
import type { StateRecovery } from '#lib/ipc/bindings.js'

const queued: StateRecovery[] = [{ file: 'favorites', keptCopy: null }]
const { listen, takeStateRecoveriesCommand } = vi.hoisted(() => ({
  listen: vi.fn((handler: () => void) => {
    handler()
    return Promise.resolve(() => {})
  }),
  takeStateRecoveriesCommand: vi.fn(),
}))
vi.mock('#lib/ipc/bindings.js', () => ({
  commands: { takeStateRecoveries: takeStateRecoveriesCommand },
  events: { stateRecoveryQueued: { listen } },
}))

import { onStateRecoveryQueued, takeStateRecoveries } from './state-recovery'

describe('takeStateRecoveries', () => {
  it('hands back what the backend queued', async () => {
    takeStateRecoveriesCommand.mockResolvedValueOnce(queued)
    await expect(takeStateRecoveries()).resolves.toEqual(queued)
  })
})

describe('onStateRecoveryQueued', () => {
  it('calls its listener on each nudge', async () => {
    const heard = vi.fn()
    await onStateRecoveryQueued(heard)
    expect(heard).toHaveBeenCalledOnce()
  })
})
