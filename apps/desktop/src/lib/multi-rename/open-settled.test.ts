import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { openSettled } from './open-settled'
import { ROW_SETTLE_BUDGET_MS } from '#lib/file-explorer/pane/pressed-rows.js'
import type { MultiRenameSelection } from '#lib/file-explorer/pane/pane-commands.js'
import type { MultiRenameResult } from '#lib/tauri-commands/index.js'

const STALE: MultiRenameResult<string> = { ok: false, error: { type: 'selectionChanged', listingId: 'L' } }
const OPENED: MultiRenameResult<string> = { ok: true, value: 'session' }

function targetAt(sequence: number, listingId = 'L'): MultiRenameSelection {
  return { listingId, selectedIndices: [2], expectedSequence: sequence }
}

describe('openSettled', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('tries again on the rows the pane has now when the folder moved under the press', async () => {
    let sequence = 1
    const open = vi.fn<(target: MultiRenameSelection) => Promise<MultiRenameResult<string>>>()
    open.mockResolvedValueOnce(STALE).mockResolvedValueOnce(OPENED)
    const opened = openSettled(() => targetAt(sequence++), open)
    await vi.runAllTimersAsync()
    expect(await opened).toEqual(OPENED)
    expect(open.mock.calls.map(([target]) => target.expectedSequence)).toEqual([1, 2])
  })

  it('gives up with the refusal once the budget runs out', async () => {
    const open = vi.fn(() => Promise.resolve(STALE))
    const opened = openSettled(() => targetAt(1), open)
    await vi.advanceTimersByTimeAsync(ROW_SETTLE_BUDGET_MS + 1000)
    expect(await opened).toEqual(STALE)
  })

  it('stops waiting when the pane leaves the folder', async () => {
    const listings = ['L', 'M']
    const open = vi.fn(() => Promise.resolve(STALE))
    const opened = openSettled(() => targetAt(1, listings.shift()), open)
    await vi.runAllTimersAsync()
    expect(await opened).toEqual(STALE)
    expect(open).toHaveBeenCalledTimes(1)
  })

  it('opens nothing when the target has gone, and says nothing of its own', async () => {
    const open = vi.fn(() => Promise.resolve(STALE))
    const targets = [targetAt(1), null]
    const opened = openSettled(() => targets.shift() ?? null, open)
    await vi.runAllTimersAsync()
    expect(await opened).toBeNull()
  })
})
