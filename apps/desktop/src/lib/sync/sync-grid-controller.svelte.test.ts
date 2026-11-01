/**
 * The grid controller's range selection: rows are read page by page, so a view change halfway (a filter, a new
 * compare) would mix rows of two views. The range is dropped instead, and the notice says why.
 */
import { describe, expect, it, vi } from 'vitest'
import { SvelteSet } from 'svelte/reactivity'
import { SyncGridController } from './sync-grid-controller.svelte'
import type { SyncSession } from './sync-session.svelte'
import { itemRow } from './sync-test-fixtures'

function fakeSession(changeViewAt: number | null) {
  let version = 1
  const session = {
    selected: new SvelteSet<number>(),
    get viewVersionNow() {
      return version
    },
    currentRow: (index: number) => itemRow(index),
    rowAt: vi.fn((index: number) => {
      if (index === changeViewAt) version++
      return Promise.resolve(itemRow(index + version * 100))
    }),
  }
  return session
}

function controller(session: ReturnType<typeof fakeSession>) {
  return new SyncGridController({
    session: session as unknown as SyncSession,
    files: {
      view: vi.fn(),
      edit: vi.fn(),
      quickLook: vi.fn(),
      copyText: vi.fn(),
    },
    reveal: vi.fn(),
    dialog: { close: vi.fn(), confirm: vi.fn(), openRowMenu: vi.fn(), profiles: vi.fn(), saveProfile: vi.fn() },
  })
}

describe('SyncGridController range selection', () => {
  it('selects every row of one view', async () => {
    const session = fakeSession(null)
    const c = controller(session)
    c.moveCursor(0)
    await c.click(2, { metaKey: false, shiftKey: true })
    expect([...session.selected]).toEqual([100, 101, 102])
  })

  it('drops a range whose view changed while it was read, and says so', async () => {
    const session = fakeSession(1)
    const c = controller(session)
    c.moveCursor(0)
    await c.click(3, { metaKey: false, shiftKey: true })
    expect([...session.selected]).toEqual([])
    expect(c.notice).toBe('The rows changed while selecting. Select them again.')
  })
})
