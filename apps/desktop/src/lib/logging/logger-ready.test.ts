/**
 * `whenLoggerReady` holds a line until `initLogger` has configured LogTape.
 *
 * LogTape drops every record logged before `configure`, and `initLogger` only runs from the
 * root layout's `onMount`, which comes AFTER a child's first render. So a crash in that
 * first render, the one that swaps the window for the error page, would otherwise leave no
 * line at all: exactly what a Catalina user's 500 screen did in v0.54.0.
 */
import { describe, expect, it, vi } from 'vitest'

vi.mock('@tauri-apps/plugin-store', () => ({
  load: () => Promise.resolve({ get: () => Promise.resolve(undefined) }),
}))
vi.mock('#lib/settings/store-path.js', () => ({ resolveStorePath: (name: string) => Promise.resolve(name) }))
vi.mock('./log-bridge', () => ({ getTauriBridgeSink: () => () => {}, startBridge: () => {} }))

import { initLogger, whenLoggerReady } from './logger'

describe('whenLoggerReady', () => {
  it('holds a line until the logger is configured, then runs it once', async () => {
    const emit = vi.fn()
    whenLoggerReady(emit)
    expect(emit).not.toHaveBeenCalled()

    await initLogger()
    expect(emit).toHaveBeenCalledTimes(1)

    // Already configured: runs right away, and the drained queue doesn't replay.
    const later = vi.fn()
    whenLoggerReady(later)
    expect(later).toHaveBeenCalledTimes(1)
    expect(emit).toHaveBeenCalledTimes(1)
  })
})
