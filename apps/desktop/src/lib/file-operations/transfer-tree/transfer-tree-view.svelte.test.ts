import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import {
  getTransferTreeView,
  type TransferTreeAnswer,
  type TransferTreeView,
  type TransferTreeViewRequest,
} from '$lib/tauri-commands'
import { ipcTransferTreeSource, type TransferTreeSource } from './transfer-tree-source'
import { TRANSFER_TREE_FIXTURES } from './fixtures/catalog'
import { createFixturePlayer } from './fixtures/fixture-player.svelte'

vi.mock('$lib/tauri-commands', () => ({
  getTransferTreeView: vi.fn(() => Promise.resolve({ refusal: { type: 'unknown_operation' } })),
}))
import { FOLLOW_LEAD, MAX_TOGGLES, OVERSCAN_ROWS, createTransferTreeViewState } from './transfer-tree-view.svelte'

/** A source whose answers the test releases by hand, recording every request. */
function manualSource() {
  const requests: TransferTreeViewRequest[] = []
  const pending: ((answer: TransferTreeAnswer) => void)[] = []
  const source: TransferTreeSource = {
    view: (request) => {
      requests.push(structuredClone(request))
      return new Promise((resolve) => pending.push(resolve))
    },
  }
  return {
    source,
    requests,
    get inFlight() {
      return pending.length
    },
    /** Answers the oldest pending request. */
    async answer(answer: TransferTreeAnswer) {
      const resolve = pending.shift()
      if (!resolve) throw new Error('no request in flight')
      resolve(answer)
      await settle()
    },
  }
}

async function settle(): Promise<void> {
  for (let i = 0; i < 5; i++) await Promise.resolve()
}

function view(overrides: Partial<TransferTreeView> = {}): TransferTreeView {
  return {
    ready: true,
    revision: 1,
    totalRows: 1_000,
    offset: 0,
    rows: [],
    ancestors: [],
    focusRow: 500,
    axis: 'bytes',
    ...overrides,
  }
}

/** Runs scheduled frame work at once, so a scroll's pull is synchronous here. */
const immediately = (run: () => void): void => {
  run()
}

let src: ReturnType<typeof manualSource>

beforeEach(() => {
  src = manualSource()
})

function create() {
  const state = createTransferTreeViewState(src.source, { schedule: immediately })
  state.setViewportRows(8)
  return state
}

describe('the request coalescer', () => {
  it('keeps one request in flight, and a tick during it asks once more with the latest state', async () => {
    const state = create()
    state.refresh()
    state.refresh()
    state.refresh()
    expect(src.requests).toHaveLength(1)

    state.toggle(42, false)
    expect(src.requests).toHaveLength(1)

    await src.answer(view())
    expect(src.requests).toHaveLength(2)
    expect(src.requests[1].expanded).toEqual([42])

    await src.answer(view({ revision: 2 }))
    expect(src.requests).toHaveLength(2)
    expect(src.inFlight).toBe(0)
    expect(state.view?.revision).toBe(2)
  })

  it('stops asking once the operation has finished, and keeps the last rows', async () => {
    const state = create()
    state.refresh()
    await src.answer(view({ revision: 7 }))
    state.refresh()
    await src.answer({ refusal: { type: 'unknown_operation' } })

    expect(state.ended).toBe('finished')
    expect(state.view?.revision).toBe(7)
    state.refresh()
    expect(src.requests).toHaveLength(2)
  })

  it('says when the operation never builds a tree', async () => {
    const state = create()
    state.refresh()
    await src.answer({ refusal: { type: 'not_a_transfer' } })
    expect(state.ended).toBe('notATransfer')
  })
})

describe('following', () => {
  it('asks for the window around the work and scrolls the focus row to the lead', async () => {
    const state = create()
    state.refresh()
    expect(src.requests[0].window).toEqual({ kind: 'follow', lead: FOLLOW_LEAD })
    expect(src.requests[0].limit).toBe(8 + 2 * OVERSCAN_ROWS)

    await src.answer(view({ focusRow: 500, offset: 498 }))
    expect(state.followTop).toBe(500 - FOLLOW_LEAD)
  })

  it('clamps the follow position so the viewport stays full near the end', async () => {
    const state = create()
    state.refresh()
    await src.answer(view({ focusRow: 999, totalRows: 1_000 }))
    expect(state.followTop).toBe(1_000 - 8)
  })

  it('pauses on a user scroll and windows where the user is, and Follow resumes', async () => {
    const state = create()
    state.refresh()
    await src.answer(view())

    state.pauseFollowing()
    state.scrolledTo(120)
    expect(state.following).toBe(false)
    expect(state.followTop).toBeNull()
    expect(src.requests.at(-1)?.window).toEqual({ kind: 'at', offset: 120 - OVERSCAN_ROWS })
    await src.answer(view({ offset: 110 }))

    state.resumeFollowing()
    expect(state.following).toBe(true)
    expect(src.requests.at(-1)?.window).toEqual({ kind: 'follow', lead: FOLLOW_LEAD })
  })

  it('pauses where the follow had scrolled to, so the next tick windows the rows on screen', async () => {
    const state = create()
    state.refresh()
    await src.answer(view({ focusRow: 500 }))

    state.pauseFollowing()
    state.refresh()
    expect(src.requests.at(-1)?.window).toEqual({ kind: 'at', offset: 500 - FOLLOW_LEAD - OVERSCAN_ROWS })
  })

  it('keeps following through its own programmatic scrolls', async () => {
    const state = create()
    state.refresh()
    await src.answer(view())

    state.scrolledTo(498)
    expect(state.following).toBe(true)
    expect(src.requests).toHaveLength(1)
  })
})

describe('hand toggles', () => {
  it('records a collapse and an expand, each undoing the other', () => {
    const state = create()
    state.toggle(5, true)
    expect([state.expanded, state.collapsed]).toEqual([[], [5]])
    state.toggle(5, false)
    expect([state.expanded, state.collapsed]).toEqual([[5], []])
    state.toggle(9, false)
    state.toggle(5, true)
    expect([state.expanded, state.collapsed]).toEqual([[9], [5]])
  })

  it('drops the oldest toggles past the backend’s cap', () => {
    const state = create()
    for (let id = 0; id < MAX_TOGGLES + 3; id++) state.toggle(id, false)
    expect(state.expanded).toHaveLength(MAX_TOGGLES)
    expect(state.expanded[0]).toBe(3)
  })
})

describe('parentIndexOf', () => {
  it('finds a row’s parent in the window, or among the ancestors above it', async () => {
    const state = create()
    state.refresh()
    const row = { kind: 'file', state: 'waiting', label: 'x' } as const
    await src.answer(
      view({
        offset: 40,
        ancestors: [{ id: 1, label: 'a', index: 3 }],
        rows: [
          { ...base(), ...row, id: 10, depth: 1 },
          { ...base(), id: 11, depth: 1, kind: 'folder', label: 'b' },
          { ...base(), ...row, id: 12, depth: 2 },
        ],
      }),
    )
    expect(state.parentIndexOf(42)).toBe(41)
    expect(state.parentIndexOf(41)).toBe(3)
    expect(state.parentIndexOf(40)).toBe(3)
  })
})

describe('ipcTransferTreeSource', () => {
  it('asks the backend for its own operation, passing the request and the answer through', async () => {
    const request: TransferTreeViewRequest = {
      expanded: [3],
      collapsed: [],
      window: { kind: 'at', offset: 4 },
      limit: 20,
    }
    const answer = await ipcTransferTreeSource('op-7').view(request)
    expect(getTransferTreeView).toHaveBeenCalledWith('op-7', request)
    expect(answer).toEqual({ refusal: { type: 'unknown_operation' } })
  })
})

describe('the fixture player', () => {
  afterEach(() => {
    vi.useRealTimers()
  })

  it('plays a fixture at the progress cadence, ticking as it goes, and starts it over once it finishes', () => {
    vi.useFakeTimers()
    const fixture = TRANSFER_TREE_FIXTURES.find((candidate) => candidate.id === 'single-file')
    if (!fixture) throw new Error('no single-file fixture')
    const player = createFixturePlayer(fixture)
    const first = player.transfer

    player.play()
    vi.advanceTimersByTime(1_000)
    expect(player.props.tick).toBe(5)

    for (let guard = 0; guard < 500 && player.transfer === first; guard++) vi.advanceTimersByTime(200)
    expect(first.finished).toBe(true)
    expect(player.transfer).not.toBe(first)

    player.stop()
    const tick = player.props.tick
    vi.advanceTimersByTime(1_000)
    expect(player.props.tick).toBe(tick)
  })
})

function base() {
  return {
    id: 0,
    depth: 0,
    label: '',
    kind: 'folder' as const,
    state: 'waiting' as const,
    expanded: false,
    hasChildren: false,
    posInSet: 1,
    setSize: 1,
    filesDone: 0,
    filesTotal: 1,
    bytesDone: 0,
    bytesTotal: 1,
    skipped: 0,
    failed: 0,
    failure: null,
  }
}
