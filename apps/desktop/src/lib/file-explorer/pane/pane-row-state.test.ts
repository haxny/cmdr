import { afterEach, describe, expect, it, vi } from 'vitest'
import { createPaneRowState } from './pane-row-state'

function readyRowState(overrides: { loading?: () => boolean; operationActive?: () => boolean } = {}) {
  const rowState = createPaneRowState({
    getListingId: () => 'listing-1',
    getLoading: overrides.loading ?? (() => false),
    getOperationActive: overrides.operationActive ?? (() => false),
  })
  rowState.initialize()
  return rowState
}

afterEach(() => {
  vi.useRealTimers()
})

// `whenReady` is how a command that needs settled rows (F5 / F6 with a selection) waits for
// them instead of dropping the press. It's driven by the transitions that can make the rows
// ready, never by polling.
describe('whenReady', () => {
  it('answers true at once when the rows are already settled', async () => {
    await expect(readyRowState().whenReady(1000)).resolves.toBe(true)
  })

  it('answers true as soon as in-flight row work ends', async () => {
    const rowState = readyRowState()
    const endWork = rowState.beginAsyncWork()
    let answer: boolean | undefined
    void rowState.whenReady(1000).then((value) => (answer = value))
    await Promise.resolve()
    expect(answer).toBeUndefined()

    endWork()
    await Promise.resolve()

    expect(answer).toBe(true)
  })

  it('answers true when an outside condition clears and the pane says so', async () => {
    let operationActive = true
    const rowState = readyRowState({ operationActive: () => operationActive })
    let answer: boolean | undefined
    void rowState.whenReady(1000).then((value) => (answer = value))

    operationActive = false
    rowState.recheck()
    await Promise.resolve()

    expect(answer).toBe(true)
  })

  it('answers false when the rows stay unsettled past the budget', async () => {
    vi.useFakeTimers()
    const rowState = readyRowState({ operationActive: () => true })
    const waiting = rowState.whenReady(1000)

    await vi.advanceTimersByTimeAsync(1000)

    await expect(waiting).resolves.toBe(false)
  })

  it('answers false at once when the pane is gone', async () => {
    const rowState = readyRowState({ operationActive: () => true })
    const waiting = rowState.whenReady(1000)

    rowState.dispose()

    await expect(waiting).resolves.toBe(false)
  })
})
