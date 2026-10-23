/**
 * Tests for the file list header's Sort by popup wrapper and its pick event.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('#lib/ipc/bindings.js', () => ({
  commands: {
    showDateSortMenu: vi.fn(),
  },
  events: {
    dateSortMenuPick: { listen: vi.fn() },
  },
}))

import { commands, events, type DateSortMenuPick } from '#lib/ipc/bindings.js'
import { showDateSortMenu, onDateSortMenuPick } from './date-sort-menu'

beforeEach(() => {
  vi.clearAllMocks()
})

describe('showDateSortMenu', () => {
  it('passes the pane, the requested column, and the anchor through', async () => {
    vi.mocked(commands.showDateSortMenu).mockResolvedValueOnce({ status: 'ok', data: null })
    await expect(showDateSortMenu('right', 'added', { x: 12, y: 34 })).resolves.toBeUndefined()
    expect(commands.showDateSortMenu).toHaveBeenCalledWith('right', 'added', { x: 12, y: 34 })
  })

  it('opens at the pointer with no anchor, for a ranked search pane too', async () => {
    vi.mocked(commands.showDateSortMenu).mockResolvedValueOnce({ status: 'ok', data: null })
    await showDateSortMenu('left', null, null)
    expect(commands.showDateSortMenu).toHaveBeenCalledWith('left', null, null)
  })

  it('throws the backend error on failure', async () => {
    vi.mocked(commands.showDateSortMenu).mockResolvedValueOnce({ status: 'error', error: 'no window' })
    await expect(showDateSortMenu('left', 'modified', null)).rejects.toThrow('no window')
  })
})

describe('onDateSortMenuPick', () => {
  it('hands the pick to the handler and gives back the unlisten', async () => {
    const unlisten = vi.fn()
    let deliver: ((event: { payload: DateSortMenuPick }) => void) | undefined
    vi.mocked(events.dateSortMenuPick.listen).mockImplementation((cb: unknown) => {
      deliver = cb as (event: { payload: DateSortMenuPick }) => void
      return Promise.resolve(unlisten)
    })

    const handler = vi.fn()
    const stop = await onDateSortMenuPick(handler)

    deliver?.({ payload: { pane: 'right', column: 'created' } })
    expect(handler).toHaveBeenCalledExactlyOnceWith({ pane: 'right', column: 'created' })

    stop()
    expect(unlisten).toHaveBeenCalledOnce()
  })
})
