/**
 * The `security-notice-raised` subscription hands its listener the bare payload.
 */

import { describe, it, expect, vi } from 'vitest'
import type { SecurityNoticeRaised } from '#lib/ipc/bindings.js'

const { listen } = vi.hoisted(() => ({
  listen: vi.fn((handler: (event: { payload: SecurityNoticeRaised }) => void) => {
    handler({ payload: { notice: { kind: 'mcpForeignOrigin' }, count: 3 } })
    return Promise.resolve(() => {})
  }),
}))
vi.mock('#lib/ipc/bindings.js', () => ({ events: { securityNoticeRaised: { listen } } }))

import { onSecurityNoticeRaised } from './security-notices'

describe('onSecurityNoticeRaised', () => {
  it('unwraps the payload for its listener', async () => {
    const heard = vi.fn()
    await onSecurityNoticeRaised(heard)
    expect(heard).toHaveBeenCalledWith({ notice: { kind: 'mcpForeignOrigin' }, count: 3 })
  })
})
