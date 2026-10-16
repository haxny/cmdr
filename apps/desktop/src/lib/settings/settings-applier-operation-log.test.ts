/**
 * The settings-applier's live-apply wiring for `operationLog.enabled`: flipping the
 * switch reaches the backend's recording gate at once, with no restart. Rust seeds
 * the gate from the same persisted key at startup (`settings/loader.rs`).
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

const setOperationLogEnabled = vi.fn<(enabled: boolean) => Promise<void>>(() => Promise.resolve())

/** The change listener the applier registers, captured so the test can fire it. */
let changeListener: ((change: { id: string; value: unknown }) => void) | undefined

vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  const noop = () => Promise.resolve()
  const stubbed: Record<string, unknown> = {}
  for (const key of Object.keys(actual)) {
    stubbed[key] = typeof actual[key] === 'function' ? noop : actual[key]
  }
  stubbed.setOperationLogEnabled = (enabled: boolean) => setOperationLogEnabled(enabled)
  return stubbed
})

vi.mock('#lib/settings/index.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('#lib/settings/index.js')>()
  return {
    ...actual,
    initializeSettings: vi.fn().mockResolvedValue(undefined),
    onSettingChange: (listener: (change: { id: string; value: unknown }) => void) => {
      changeListener = listener
      return () => {
        changeListener = undefined
      }
    },
  }
})

import { initSettingsApplier, cleanupSettingsApplier } from './settings-applier'

beforeEach(() => {
  setOperationLogEnabled.mockClear()
  changeListener = undefined
})

afterEach(() => {
  cleanupSettingsApplier()
})

describe('settings-applier: operationLog.enabled', () => {
  it('pushes the switch to the backend when it turns off, and again when it turns on', async () => {
    await initSettingsApplier()
    setOperationLogEnabled.mockClear()
    changeListener?.({ id: 'operationLog.enabled', value: false })
    changeListener?.({ id: 'operationLog.enabled', value: true })
    expect(setOperationLogEnabled.mock.calls).toEqual([[false], [true]])
  })
})
