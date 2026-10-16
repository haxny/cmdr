/**
 * Tests for the "Delete all Cmdr data" command wrappers: a success passes through, and a refusal
 * survives the throw as its typed reason.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('#lib/ipc/bindings.js', () => ({
  commands: {
    dataWipeSummary: vi.fn(),
    deleteAllAppData: vi.fn(),
    quitAfterDataWipe: vi.fn(),
  },
}))

import { commands } from '#lib/ipc/bindings.js'
import { asDataWipeError, DataWipeFailure, deleteAllAppData, getDataWipeSummary, quitAfterDataWipe } from './data-wipe'

describe('data-wipe wrappers', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('returns the summary the backend computed', async () => {
    const summary = { totalBytes: 1_200_000_000, modelBytes: 800_000_000, hasLicense: true }
    vi.mocked(commands.dataWipeSummary).mockResolvedValueOnce({ status: 'ok', data: summary })
    expect(await getDataWipeSummary()).toEqual(summary)
  })

  it('throws a summary refusal as a typed failure', async () => {
    vi.mocked(commands.dataWipeSummary).mockResolvedValueOnce({ status: 'error', error: { type: 'timedOut' } })
    const caught: unknown = await getDataWipeSummary().catch((error: unknown) => error)
    expect(caught).toBeInstanceOf(DataWipeFailure)
    expect(asDataWipeError(caught)).toEqual({ type: 'timedOut' })
  })

  it('returns the wipe report, leftovers included', async () => {
    const report = { leftovers: [] }
    vi.mocked(commands.deleteAllAppData).mockResolvedValueOnce({ status: 'ok', data: report })
    expect(await deleteAllAppData()).toEqual(report)
  })

  it('keeps the running-operation count on a refused wipe', async () => {
    vi.mocked(commands.deleteAllAppData).mockResolvedValueOnce({
      status: 'error',
      error: { type: 'fileOperationsRunning', count: 2 },
    })
    const caught: unknown = await deleteAllAppData().catch((error: unknown) => error)
    expect(asDataWipeError(caught)).toEqual({ type: 'fileOperationsRunning', count: 2 })
  })

  it('reads an unrelated error as no wipe refusal', () => {
    expect(asDataWipeError(new Error('something else'))).toBeNull()
  })

  it('passes the quit gate answer through', async () => {
    vi.mocked(commands.quitAfterDataWipe).mockResolvedValueOnce(false)
    expect(await quitAfterDataWipe()).toBe(false)
  })
})
