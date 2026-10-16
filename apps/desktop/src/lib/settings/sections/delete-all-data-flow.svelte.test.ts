/**
 * The "Delete all Cmdr data" flow: what the dialog shows at each step, and that nothing reaches
 * the backend's delete before a confirm, or twice.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { DataWipeSummary, WipeReport } from '#lib/tauri-commands/index.js'
import { DataWipeFailure } from '#lib/tauri-commands/index.js'
import { createDataWipeFlow } from './delete-all-data-flow.svelte'

const { summaryMock, deleteMock, quitMock } = vi.hoisted(() => ({
  summaryMock: vi.fn<() => Promise<DataWipeSummary>>(),
  deleteMock: vi.fn<() => Promise<WipeReport>>(),
  quitMock: vi.fn<() => Promise<boolean>>(),
}))

vi.mock('#lib/tauri-commands/index.js', async () => {
  const real = await vi.importActual<typeof import('#lib/tauri-commands/index.js')>('#lib/tauri-commands/index.js')
  return {
    ...real,
    getDataWipeSummary: summaryMock,
    deleteAllAppData: deleteMock,
    quitAfterDataWipe: quitMock,
  }
})

const SUMMARY: DataWipeSummary = { totalBytes: 1_200_000_000, modelBytes: 4_000_000, hasLicense: true }

beforeEach(() => {
  summaryMock.mockReset().mockResolvedValue(SUMMARY)
  deleteMock.mockReset().mockResolvedValue({ leftovers: [] })
  quitMock.mockReset().mockResolvedValue(true)
})

describe('the data wipe flow', () => {
  it('opens on the confirm view with the summary, and deletes nothing yet', async () => {
    const flow = createDataWipeFlow()
    await flow.open()
    expect(flow.view).toEqual({ kind: 'confirm', summary: SUMMARY, refusal: null })
    expect(deleteMock).not.toHaveBeenCalled()
  })

  it('still opens when the summary doesn’t come back, just without numbers', async () => {
    summaryMock.mockRejectedValue(new DataWipeFailure({ type: 'timedOut' }))
    const flow = createDataWipeFlow()
    await flow.open()
    expect(flow.view).toEqual({ kind: 'confirm', summary: null, refusal: null })
  })

  it('cancel closes it without deleting', async () => {
    const flow = createDataWipeFlow()
    await flow.open()
    flow.cancel()
    expect(flow.view).toBeNull()
    expect(deleteMock).not.toHaveBeenCalled()
  })

  it('confirm shows deleting, then the outcome', async () => {
    let finish: (report: WipeReport) => void = () => {}
    deleteMock.mockReturnValue(new Promise((resolve) => (finish = resolve)))
    const flow = createDataWipeFlow()
    await flow.open()
    const confirming = flow.confirm()
    expect(flow.view).toEqual({ kind: 'deleting', summary: SUMMARY })
    const report: WipeReport = { leftovers: [{ type: 'path', path: '/Users/x/Library/Logs/cmdr' }] }
    finish(report)
    await confirming
    expect(flow.view).toEqual({ kind: 'done', report })
  })

  it('a second confirm while deleting sends nothing', async () => {
    deleteMock.mockReturnValue(new Promise(() => {}))
    const flow = createDataWipeFlow()
    await flow.open()
    void flow.confirm()
    void flow.confirm()
    expect(deleteMock).toHaveBeenCalledTimes(1)
  })

  it('cancel does nothing once deleting started, nor after', async () => {
    deleteMock.mockReturnValue(new Promise(() => {}))
    const flow = createDataWipeFlow()
    await flow.open()
    void flow.confirm()
    flow.cancel()
    expect(flow.view?.kind).toBe('deleting')
  })

  it('a refusal goes back to confirm and says why', async () => {
    deleteMock.mockRejectedValue(new DataWipeFailure({ type: 'fileOperationsRunning', count: 2 }))
    const flow = createDataWipeFlow()
    await flow.open()
    await flow.confirm()
    expect(flow.view).toEqual({
      kind: 'confirm',
      summary: SUMMARY,
      refusal: { type: 'fileOperationsRunning', count: 2 },
    })
  })

  it('an untyped throw reads as interrupted, never as success', async () => {
    deleteMock.mockRejectedValue(new Error('bridge gone'))
    const flow = createDataWipeFlow()
    await flow.open()
    await flow.confirm()
    expect(flow.view).toEqual({ kind: 'confirm', summary: SUMMARY, refusal: { type: 'interrupted' } })
  })

  it('quit asks the backend to quit', async () => {
    const flow = createDataWipeFlow()
    await flow.open()
    await flow.confirm()
    await flow.quit()
    expect(quitMock).toHaveBeenCalledTimes(1)
  })
})
