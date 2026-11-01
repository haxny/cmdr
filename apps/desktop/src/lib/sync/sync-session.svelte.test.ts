/**
 * The dialog's session over a mocked IPC: what starts a compare and what never does, which compare's events count,
 * how the grid pages follow the show-filters and the plan, and what closing does.
 */
import { describe, expect, it, vi } from 'vitest'
import type {
  ShowFilters,
  SyncCompareFinished,
  SyncCompareProgress,
  SyncRowsChanged,
  SyncRowsPage,
  SyncState,
} from '#lib/tauri-commands/sync.js'
import { SyncCommandFailure, SyncProfileEditFailure } from '#lib/tauri-commands/sync.js'
import { createSyncSession, type SyncIpc } from './sync-session.svelte'
import { DEFAULT_SHOW_FILTERS } from './sync-options'

const STATE: SyncState = {
  summary: {
    intoLeft: { copyFiles: 0, copyBytes: 0, replaceFiles: 0, replaceBytes: 0, removals: 0, removedFolders: 0 },
    intoRight: { copyFiles: 1, copyBytes: 10, replaceFiles: 0, replaceBytes: 0, removals: 0, removedFolders: 0 },
    skipped: 0,
    undecided: 0,
    equal: 0,
    different: 1,
    uniqueLeft: 0,
    uniqueRight: 0,
    notComparable: 0,
    hiddenActions: 0,
    changedByYou: 0,
  },
  guards: { blocks: [], olderOverwritesNewer: 0, skew: null },
  stale: false,
  revision: 5,
}

function itemRow(index: number) {
  return {
    type: 'item' as const,
    index,
    leftName: `f${String(index)}`,
    rightName: `f${String(index)}`,
    leftRelPath: `f${String(index)}`,
    rightRelPath: `f${String(index)}`,
    leftKind: 'file' as const,
    rightKind: 'file' as const,
    leftSize: 1,
    rightSize: 1,
    leftMtime: 2,
    rightMtime: 1,
    category: 'newerLeft' as const,
    rung: 'sizeAndTime' as const,
    content: 'notAsked' as const,
    sameSize: true,
    action: 'copyRight' as const,
    defaultAction: 'copyRight' as const,
    olderOverwritesNewer: false,
    legal: ['copyRight' as const, 'skip' as const],
  }
}

function page(start: number, end: number, total = 3): SyncRowsPage {
  const rows = []
  for (let i = start; i < Math.min(end, total); i++) rows.push({ viewIndex: i, row: itemRow(i) })
  return { total, rows }
}

interface Harness {
  ipc: SyncIpc
  finish: (payload: SyncCompareFinished) => void
  progress: (payload: SyncCompareProgress) => void
  rowsChanged: (payload: SyncRowsChanged) => void
}

function harness(): Harness {
  const handlers: {
    finished?: (p: SyncCompareFinished) => void
    progress?: (p: SyncCompareProgress) => void
    rows?: (p: SyncRowsChanged) => void
  } = {}
  let generation = 0
  const ipc: SyncIpc = {
    startSyncCompare: vi.fn(() => Promise.resolve({ sessionId: 's1', generation: ++generation })),
    setSyncComparePaused: vi.fn(() => Promise.resolve()),
    cancelSyncCompare: vi.fn(() => Promise.resolve()),
    getSyncRows: vi.fn((_id: string, start: number, end: number) => Promise.resolve(page(start, end))),
    getSyncState: vi.fn(() => Promise.resolve(STATE)),
    setSyncActions: vi.fn(() => Promise.resolve({ state: STATE, skipped: [], hidden: 0 })),
    findSyncRow: vi.fn(() => Promise.resolve(null)),
    compareSyncItemContent: vi.fn(() => Promise.resolve()),
    closeSyncSession: vi.fn(() => Promise.resolve()),
    getSyncLastOptions: vi.fn(() => Promise.resolve(null)),
    saveSyncLastOptions: vi.fn(() => Promise.resolve()),
    onSyncCompareFinished: vi.fn((h: (p: SyncCompareFinished) => void) => {
      handlers.finished = h
      return Promise.resolve(() => {})
    }),
    onSyncCompareProgress: vi.fn((h: (p: SyncCompareProgress) => void) => {
      handlers.progress = h
      return Promise.resolve(() => {})
    }),
    onSyncRowsChanged: vi.fn((h: (p: SyncRowsChanged) => void) => {
      handlers.rows = h
      return Promise.resolve(() => {})
    }),
  }
  return {
    ipc,
    finish: (p) => handlers.finished?.(p),
    progress: (p) => handlers.progress?.(p),
    rowsChanged: (p) => handlers.rows?.(p),
  }
}

const ROOTS = {
  left: { root: { volumeId: 'root', path: '/a' }, selection: { listingId: 'L', indices: [1], expectedSequence: 3 } },
  right: { root: { volumeId: 'root', path: '/b' }, selection: null },
}

function session(h: Harness) {
  return createSyncSession({
    ipc: h.ipc,
    roots: ROOTS,
    readDefaults: () => ({
      keep: { place: 'trashElseVersions', location: { targetRoot: { folderName: '.cmdr-sync/versions' } } },
      timeToleranceSecs: 2,
      ignoreHourShift: false,
      guards: { maxRemovePercent: 50, maxRemoveCount: 0, sentinel: null },
    }),
    includeHidden: () => false,
  })
}

async function settle(): Promise<void> {
  for (let i = 0; i < 5; i++) await Promise.resolve()
}

async function opened(h: Harness) {
  const s = session(h)
  await s.open()
  h.finish({ sessionId: 's1', generation: 1, outcome: { type: 'done' } })
  await settle()
  s.ensureRange(0, 3)
  await settle()
  return s
}

describe('createSyncSession', () => {
  it('opens on the remembered options, compares both ways by default, and sends selections only when asked', async () => {
    const h = harness()
    const s = session(h)
    await s.open()
    expect(h.ipc.getSyncLastOptions).toHaveBeenCalledWith(ROOTS.left.root, ROOTS.right.root)
    const request = vi.mocked(h.ipc.startSyncCompare).mock.calls[0][0]
    expect(request.options.mode).toBe('bothWays')
    expect(request.sessionId).toBeNull()
    expect(request.left.selection).toBeNull()
    expect(s.phase).toBe('comparing')
  })

  it('shows the rows once its own compare finishes, and ignores an older generation', async () => {
    const h = harness()
    const s = session(h)
    await s.open()
    await s.compare()
    h.finish({ sessionId: 's1', generation: 1, outcome: { type: 'done' } })
    await settle()
    expect(s.phase).toBe('comparing')
    h.finish({ sessionId: 's1', generation: 2, outcome: { type: 'done' } })
    await settle()
    expect(s.phase).toBe('done')
    expect(s.state?.summary.different).toBe(1)
    s.ensureRange(0, 3)
    await settle()
    expect(s.total).toBe(3)
    expect(s.getRow(1)).toMatchObject({ type: 'item', index: 1 })
  })

  it('keeps a typed refusal of the compare', async () => {
    const h = harness()
    vi.mocked(h.ipc.startSyncCompare).mockRejectedValueOnce(
      new SyncCommandFailure({ type: 'rootUnreachable', side: 'right' }),
    )
    const s = session(h)
    await s.open()
    expect(s.phase).toBe('refused')
    expect(s.error).toEqual({ type: 'rootUnreachable', side: 'right' })
  })

  it('a matching toggle marks the grid stale without re-comparing', async () => {
    const h = harness()
    const s = await opened(h)
    for (const [key, value] of [
      ['subfolders', false],
      ['compareBy', 'content'],
      ['masks', '*.pdf'],
      ['emptyFolders', true],
      ['onlySelected', true],
    ] as const) {
      s.setOption(key, value)
      expect(s.stale).toBe(true)
    }
    expect(h.ipc.startSyncCompare).toHaveBeenCalledTimes(1)
    await s.compare()
    expect(s.stale).toBe(false)
    const request = vi.mocked(h.ipc.startSyncCompare).mock.calls[1][0]
    expect(request.sessionId).toBe('s1')
    expect(request.left.selection).toEqual(ROOTS.left.selection)
    expect(request.options.masks).toBe('*.pdf')
  })

  it('a show-filter change re-pages rows and keeps actions', async () => {
    const h = harness()
    const s = await opened(h)
    vi.mocked(h.ipc.getSyncRows).mockClear()
    s.setFilters({ ...DEFAULT_SHOW_FILTERS, equal: true })
    await settle()
    const filters = vi.mocked(h.ipc.getSyncRows).mock.calls.at(-1)?.[3] as ShowFilters
    expect(filters.equal).toBe(true)
    expect(h.ipc.getSyncState).toHaveBeenLastCalledWith('s1', filters)
    expect(h.ipc.startSyncCompare).toHaveBeenCalledTimes(1)
    expect(h.ipc.setSyncActions).not.toHaveBeenCalled()
    expect(s.stale).toBe(false)
  })

  it('sends a bulk change with the filters and re-pages after it', async () => {
    const h = harness()
    const s = await opened(h)
    vi.mocked(h.ipc.getSyncRows).mockClear()
    expect(await s.setActions([0, 2], 'reverse')).toEqual({ type: 'done', skipped: [], hidden: 0 })
    expect(h.ipc.setSyncActions).toHaveBeenCalledWith('s1', expect.any(Number), [0, 2], 'reverse', DEFAULT_SHOW_FILTERS)
    await settle()
    expect(h.ipc.getSyncRows).toHaveBeenCalled()
  })

  it('drops the old plan’s selection and rows the moment a new compare starts, and keys name their compare', async () => {
    const h = harness()
    const s = await opened(h)
    s.selected.add(0)
    s.selected.add(2)
    const before = s.viewVersionNow
    const again = s.compare()
    expect(s.selected.size).toBe(0)
    expect(s.viewVersionNow).toBeGreaterThan(before)
    await again
    h.finish({ sessionId: 's1', generation: 2, outcome: { type: 'done' } })
    await settle()
    await s.setActions([0], 'skip')
    expect(h.ipc.setSyncActions).toHaveBeenLastCalledWith('s1', 2, [0], 'skip', DEFAULT_SHOW_FILTERS)
  })

  it('keys name the plan on screen: a compare that was stopped leaves the older plan’s generation', async () => {
    const h = harness()
    const s = await opened(h)
    await s.compare()
    h.finish({ sessionId: 's1', generation: 2, outcome: { type: 'stopped' } })
    await settle()
    await s.setActions([0], 'skip')
    expect(h.ipc.setSyncActions).toHaveBeenLastCalledWith('s1', 1, [0], 'skip', DEFAULT_SHOW_FILTERS)
  })

  it('answers the typed refusal of an action change', async () => {
    const h = harness()
    const s = await opened(h)
    const refusal = { type: 'override' as const, refusal: { type: 'noSuchItem' as const, item: 9 } }
    vi.mocked(h.ipc.setSyncActions).mockRejectedValueOnce(new SyncCommandFailure(refusal))
    expect(await s.setActions([9], 'skip')).toEqual({ type: 'refused', problem: refusal })
  })

  it('re-pages when the backend says rows changed in this session only', async () => {
    const h = harness()
    const s = await opened(h)
    vi.mocked(h.ipc.getSyncRows).mockClear()
    h.rowsChanged({ sessionId: 'other' })
    await settle()
    expect(h.ipc.getSyncRows).not.toHaveBeenCalled()
    h.rowsChanged({ sessionId: 's1' })
    await settle()
    expect(h.ipc.getSyncRows).toHaveBeenCalled()
    expect(s.phase).toBe('done')
  })

  it('pauses, stops, and swaps the sides into a new compare', async () => {
    const h = harness()
    const s = session(h)
    await s.open()
    await s.setPaused(true)
    expect(h.ipc.setSyncComparePaused).toHaveBeenCalledWith('s1', true)
    expect(s.phase).toBe('paused')
    await s.stop()
    expect(h.ipc.cancelSyncCompare).toHaveBeenCalledWith('s1')
    await s.swap()
    const request = vi.mocked(h.ipc.startSyncCompare).mock.calls[1][0]
    expect(request.left.root).toEqual(ROOTS.right.root)
    expect(request.right.root).toEqual(ROOTS.left.root)
  })

  it('remembers the options and closes the session on close, saying when they weren’t saved', async () => {
    const h = harness()
    const s = await opened(h)
    s.setOption('masks', '*.txt')
    vi.mocked(h.ipc.saveSyncLastOptions).mockRejectedValueOnce(new SyncProfileEditFailure({ type: 'writeFailed' }))
    const outcome = await s.close()
    expect(h.ipc.saveSyncLastOptions).toHaveBeenCalledWith(
      ROOTS.left.root,
      ROOTS.right.root,
      expect.objectContaining({ masks: '*.txt' }),
    )
    expect(h.ipc.closeSyncSession).toHaveBeenCalledWith('s1')
    expect(outcome).toBe(false)
    // Idempotent: the dialog's destroy may call it again.
    expect(await s.close()).toBe(true)
    expect(h.ipc.closeSyncSession).toHaveBeenCalledOnce()
  })

  it('any save failure is reported, not only a typed refusal', async () => {
    const h = harness()
    const s = await opened(h)
    vi.mocked(h.ipc.saveSyncLastOptions).mockRejectedValueOnce(new Error('disk full'))
    expect(await s.close()).toBe(false)
  })

  it('acts only on rows of the current view: after a filter change and after a new compare', async () => {
    const h = harness()
    const s = await opened(h)
    expect(s.currentRow(1)).toMatchObject({ index: 1 })
    let release: () => void = () => {}
    vi.mocked(h.ipc.getSyncRows).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          release = () => {
            resolve(page(0, 3))
          }
        }),
    )
    s.setFilters({ ...DEFAULT_SHOW_FILTERS, equal: true })
    // The old row still draws, but nothing may act on it.
    expect(s.getRow(1)).toMatchObject({ index: 1 })
    expect(s.currentRow(1)).toBeUndefined()
    release()
    await settle()
    expect(s.currentRow(1)).toMatchObject({ index: 1 })

    await s.compare()
    h.finish({ sessionId: 's1', generation: 2, outcome: { type: 'done' } })
    expect(s.currentRow(1)).toBeUndefined()
    expect(await s.rowAt(1)).toMatchObject({ index: 1 })
  })

  it('a compare that answers after a newer one is ignored, and a late summary never replaces a newer one', async () => {
    const h = harness()
    const s = await opened(h)
    let answerOlder: () => void = () => {}
    vi.mocked(h.ipc.startSyncCompare)
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            answerOlder = () => {
              resolve({ sessionId: 's1', generation: 2 })
            }
          }),
      )
      .mockImplementationOnce(() => Promise.resolve({ sessionId: 's1', generation: 3 }))
    const older = s.compare()
    await s.compare()
    answerOlder()
    await older
    h.finish({ sessionId: 's1', generation: 3, outcome: { type: 'done' } })
    await settle()
    expect(s.phase).toBe('done')

    vi.mocked(h.ipc.setSyncActions).mockResolvedValueOnce({
      state: { ...STATE, revision: 9, summary: { ...STATE.summary, changedByYou: 1 } },
      skipped: [],
      hidden: 0,
    })
    await s.setActions([0], 'skip')
    vi.mocked(h.ipc.getSyncState).mockResolvedValueOnce({ ...STATE, revision: 7 })
    s.refresh()
    await settle()
    expect(s.state?.summary.changedByYou).toBe(1)
  })

  it('a second compare while the first one has no session yet waits for it, so one session holds both', async () => {
    const h = harness()
    let answerFirst: () => void = () => {}
    vi.mocked(h.ipc.startSyncCompare)
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            answerFirst = () => {
              resolve({ sessionId: 's1', generation: 1 })
            }
          }),
      )
      .mockImplementationOnce(() => Promise.resolve({ sessionId: 's1', generation: 2 }))
    const s = session(h)
    const first = s.compare()
    const second = s.compare()
    await settle()
    expect(h.ipc.startSyncCompare).toHaveBeenCalledOnce()
    answerFirst()
    await first
    await second
    expect(vi.mocked(h.ipc.startSyncCompare).mock.calls[1][0].sessionId).toBe('s1')
  })

  it('a superseded compare’s refusal never shows', async () => {
    const h = harness()
    const s = await opened(h)
    let refuseOlder: () => void = () => {}
    vi.mocked(h.ipc.startSyncCompare)
      .mockImplementationOnce(
        () =>
          new Promise((_resolve, reject) => {
            refuseOlder = () => {
              reject(new SyncCommandFailure({ type: 'compareRunning' }))
            }
          }),
      )
      .mockImplementationOnce(() => Promise.resolve({ sessionId: 's1', generation: 3 }))
    const older = s.compare()
    await s.compare()
    refuseOlder()
    await older
    expect(s.phase).toBe('comparing')
    expect(s.error).toBeNull()
  })

  it('a session that answers after the dialog closed is closed too', async () => {
    const h = harness()
    let answer: () => void = () => {}
    vi.mocked(h.ipc.startSyncCompare).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          answer = () => {
            resolve({ sessionId: 'late', generation: 1 })
          }
        }),
    )
    const s = session(h)
    const compare = s.compare()
    await s.close()
    answer()
    await compare
    expect(h.ipc.closeSyncSession).toHaveBeenCalledWith('late')
  })

  it('a loaded profile sets the roots, options, overrides, and its name, and shows its filters only when asked', async () => {
    const h = harness()
    const s = await opened(h)
    const overrides = { keep: null, timeToleranceSecs: 4, ignoreHourShift: null, timeOffsetSecs: 0, guards: null }
    const options = { ...s.options, mode: 'mirror' as const }
    await s.useProfile({
      id: 'p1',
      name: 'Photos',
      left: { volumeId: 'root', path: '/p' },
      right: { volumeId: 'nas', path: '/q' },
      options,
      showFilters: null,
      overrides,
    })
    const request = vi.mocked(h.ipc.startSyncCompare).mock.calls.at(-1)?.[0]
    expect(request?.left).toEqual({ root: { volumeId: 'root', path: '/p' }, selection: null })
    expect(request?.right.root).toEqual({ volumeId: 'nas', path: '/q' })
    expect(request?.options.mode).toBe('mirror')
    expect(request?.overrides).toEqual(overrides)
    expect(request?.profileName).toBe('Photos')
    expect(s.profileName).toBe('Photos')
    expect(s.filters).toEqual(DEFAULT_SHOW_FILTERS)
    await s.useProfile({
      id: 'p1',
      name: 'Photos',
      left: { volumeId: 'root', path: '/p' },
      right: { volumeId: 'nas', path: '/q' },
      options,
      showFilters: { ...DEFAULT_SHOW_FILTERS, equal: true },
      overrides,
    })
    expect(s.filters.equal).toBe(true)
  })

  it('a profile whose volume is gone says so, and the pane’s own folder can stand in', async () => {
    const h = harness()
    const s = await opened(h)
    vi.mocked(h.ipc.startSyncCompare).mockRejectedValueOnce(
      new SyncCommandFailure({ type: 'volumeNotConnected', side: 'right', volumeId: 'nas' }),
    )
    await s.useProfile({
      id: 'p1',
      name: 'Photos',
      left: { volumeId: 'root', path: '/p' },
      right: { volumeId: 'nas', path: '/q' },
      options: s.options,
      showFilters: null,
      overrides: { keep: null, timeToleranceSecs: null, ignoreHourShift: null, timeOffsetSecs: 0, guards: null },
    })
    expect(s.error).toEqual({ type: 'volumeNotConnected', side: 'right', volumeId: 'nas' })
    await s.usePaneFolder('right')
    const request = vi.mocked(h.ipc.startSyncCompare).mock.calls.at(-1)?.[0]
    expect(request?.right.root).toEqual(ROOTS.right.root)
    expect(request?.left.root).toEqual({ volumeId: 'root', path: '/p' })
  })
})
