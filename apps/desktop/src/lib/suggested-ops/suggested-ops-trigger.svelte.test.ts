/**
 * The AI suggestions state slice: what the dialog reads, what it holds while the user decides,
 * and the behaviours the design leans on — every group's list showing at once, each paging a
 * window at a time, deselection surviving a scroll, live facts read only for drawn rows, and a
 * group the agent changed being ANNOUNCED rather than swapped under the reader.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { SuggestedOpPage, SuggestedSweepView, SuggestionsChanged } from '#lib/tauri-commands/index.js'

const listMock = vi.fn<() => Promise<SuggestedSweepView[]>>()
const pageMock = vi.fn<(payload: { groupId: number; offset: number; limit: number }) => Promise<SuggestedOpPage>>()
const rejectMock = vi.fn<(g: number) => Promise<{ kind: string }>>()
const approveMock = vi.fn<(g: number, deselected: number[]) => Promise<{ kind: string }>>()
const factsMock = vi.fn<(payload: { groupId: number; offset: number; limit: number }) => Promise<unknown>>()

/** The subscriber the store installs while the dialog is open. */
let emit: ((payload: SuggestionsChanged) => void) | null = null

vi.mock('#lib/tauri-commands/index.js', () => ({
  listSuggestedOps: () => listMock(),
  pageSuggestedOps: (g: number, o: number, l: number) => pageMock({ groupId: g, offset: o, limit: l }),
  rejectSuggestedGroup: (g: number) => rejectMock(g),
  approveSuggestedGroup: (g: number, deselected: number[]) => approveMock(g, deselected),
  liveSuggestedOpFacts: (g: number, o: number, l: number) => factsMock({ groupId: g, offset: o, limit: l }),
  onSuggestionsChanged: (cb: (p: SuggestionsChanged) => void) => {
    emit = cb
    return Promise.resolve(() => {
      emit = null
    })
  },
}))
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import { SuggestedOpsFailure } from './suggested-ops-failure'
import {
  INITIAL_ROWS,
  SHOW_MORE_STEP,
  approvableCount,
  approveGroup,
  closeSuggestedOps,
  ensureOpWindow,
  opAt,
  openSuggestedOps,
  refreshSuggestions,
  rejectGroup,
  reloadGroup,
  reviewOf,
  showAllRows,
  showMoreRows,
  showRange,
  shownRows,
  suggestedOpsState,
  toggleOp,
} from './suggested-ops-trigger.svelte'

function group(groupId: number, liveOpCount: number, overrides: Record<string, unknown> = {}) {
  return {
    groupId,
    sweepId: 1,
    verb: 'move',
    status: 'pending',
    displayName: 'five invoices',
    rationale: 'They all look like invoices.',
    sourceVolumeId: 'root',
    sourceFolder: '/Users/someone/Downloads',
    destination: '/Users/someone/Documents/Invoices',
    destinationVolumeName: 'Macintosh HD',
    reversible: 'restoreMove',
    destinationState: 'willBeCreated',
    liveOpCount,
    totalOpCount: liveOpCount,
    fromSelector: false,
    ...overrides,
  } as unknown as SuggestedSweepView['groups'][number]
}

function sweep(groups: SuggestedSweepView['groups']): SuggestedSweepView {
  return { sweepId: 1, createdAt: 100, rationale: 'Ten new files in Downloads.', groups }
}

function ops(from: number, count: number) {
  return Array.from({ length: count }, (_, i) => ({
    opId: from + i,
    sourcePath: `/Users/someone/Downloads/file-${String(from + i)}.dmg`,
    newName: null,
    status: 'pending',
    snapshotSize: 1024,
    snapshotModified: 1_780_000_000,
  }))
}

/** The group the default sweep holds: 60,000 ops, which is the size nothing may load whole. */
const BIG = 7

beforeEach(() => {
  vi.clearAllMocks()
  closeSuggestedOps()
  suggestedOpsState.sweeps = []
  suggestedOpsState.loadError = false
  listMock.mockResolvedValue([sweep([group(BIG, 60_000)])])
  pageMock.mockImplementation(({ offset }) =>
    Promise.resolve({ ops: ops(offset, 200), offset, total: 60_000 } as unknown as SuggestedOpPage),
  )
  factsMock.mockImplementation(({ offset, limit }) =>
    Promise.resolve({
      kind: 'facts',
      facts: Array.from({ length: limit }, (_, i) => ({
        opId: offset + i,
        state: { kind: 'present', size: 2048, modified: 1_790_000_000 },
      })),
    }),
  )
  rejectMock.mockResolvedValue({ kind: 'rejected' })
  approveMock.mockResolvedValue({ kind: 'started' })
})

describe('opening and reading', () => {
  it('opens once however many times the menu, palette, and shortcut fire', async () => {
    await openSuggestedOps()
    await openSuggestedOps()

    expect(suggestedOpsState.open).toBe(true)
    expect(listMock).toHaveBeenCalledTimes(1)
  })

  it('opens even when the read throws, and says so instead of showing an empty list', async () => {
    listMock.mockRejectedValueOnce(new Error('no store'))

    await openSuggestedOps()

    expect(suggestedOpsState.open).toBe(true)
    expect(suggestedOpsState.loadError).toBe(true)
    expect(suggestedOpsState.sweeps).toEqual([])
  })

  it('shows every group’s files right away, one page each', async () => {
    listMock.mockResolvedValue([sweep([group(7, 3), group(8, 3)])])

    await openSuggestedOps()

    expect(pageMock.mock.calls.map(([call]) => call.groupId).sort()).toEqual([7, 8])
    expect(opAt(7, 0)).not.toBeNull()
    expect(opAt(8, 0)).not.toBeNull()
  })
})

describe('the op window', () => {
  it('never loads a whole group to show it', async () => {
    await openSuggestedOps()

    expect(pageMock).toHaveBeenCalledTimes(1)
    expect(pageMock.mock.calls[0]?.[0]?.limit).toBeLessThanOrEqual(200)
    expect(reviewOf(BIG)?.window?.total).toBe(60_000)
    expect(reviewOf(BIG)?.window?.ops.length).toBe(200)
  })

  it('reads rows already held without another round trip', async () => {
    await openSuggestedOps()
    pageMock.mockClear()

    await ensureOpWindow(BIG, 10)

    expect(pageMock).not.toHaveBeenCalled()
    expect(opAt(BIG, 10)?.opId).toBe(10)
  })

  it('fetches a new window when the list jumps past the one it holds', async () => {
    await openSuggestedOps()
    pageMock.mockClear()

    await ensureOpWindow(BIG, 5_000)

    expect(pageMock).toHaveBeenCalledTimes(1)
    expect(opAt(BIG, 5_000)).not.toBeNull()
  })

  it('answers null for a row whose window is not loaded, rather than a wrong row', async () => {
    await openSuggestedOps()

    expect(opAt(BIG, 50_000)).toBeNull()
  })

  it('keeps each group’s window to itself', async () => {
    listMock.mockResolvedValue([sweep([group(7, 60_000), group(8, 60_000)])])
    await openSuggestedOps()

    await ensureOpWindow(8, 5_000)

    expect(opAt(7, 0)).not.toBeNull()
    expect(opAt(8, 0)).toBeNull()
    expect(opAt(8, 5_000)).not.toBeNull()
  })
})

describe('how many rows a list shows', () => {
  it('starts at the first few, then grows on request', async () => {
    await openSuggestedOps()
    const big = suggestedOpsState.sweeps[0].groups[0]

    expect(shownRows(big)).toBe(INITIAL_ROWS)
    showMoreRows(BIG)
    expect(shownRows(big)).toBe(INITIAL_ROWS + SHOW_MORE_STEP)
    showAllRows(BIG)
    expect(shownRows(big)).toBe(60_000)
  })

  it('never shows more rows than the group has', async () => {
    listMock.mockResolvedValue([sweep([group(7, 3)])])
    pageMock.mockResolvedValue({ ops: ops(0, 3), offset: 0, total: 3 } as unknown as SuggestedOpPage)
    await openSuggestedOps()

    showMoreRows(7)

    expect(shownRows(suggestedOpsState.sweeps[0].groups[0])).toBe(3)
  })

  it('“Show all” on 60,000 still reads one window, not the group', async () => {
    await openSuggestedOps()
    pageMock.mockClear()

    showAllRows(BIG)

    expect(pageMock).not.toHaveBeenCalled()
  })
})

describe('live facts', () => {
  it('checks only the rows the list drew, once each', async () => {
    await openSuggestedOps()

    expect(factsMock).toHaveBeenCalledTimes(1)
    expect(factsMock).toHaveBeenCalledWith({ groupId: BIG, offset: 0, limit: INITIAL_ROWS })

    factsMock.mockClear()
    await showRange(BIG, 0, INITIAL_ROWS)

    expect(factsMock).not.toHaveBeenCalled()
  })

  it('checks newly drawn rows as the list scrolls', async () => {
    await openSuggestedOps()
    factsMock.mockClear()

    await showRange(BIG, 5, 15)

    expect(factsMock).toHaveBeenCalledWith({ groupId: BIG, offset: 10, limit: 5 })
    expect(reviewOf(BIG)?.liveFacts.get(12)).toEqual({ kind: 'present', size: 2048, modified: 1_790_000_000 })
  })

  it('stops asking a source that isn’t connected, and says so once for the group', async () => {
    factsMock.mockResolvedValueOnce({ kind: 'refused', reason: { type: 'sourceNotConnected' } })
    await openSuggestedOps()
    factsMock.mockClear()

    await showRange(BIG, 5, 25)

    expect(reviewOf(BIG)?.sourceUnreachable).toBe('notConnected')
    expect(factsMock).not.toHaveBeenCalled()
  })

  it('asks again after a reload, in case the source came back', async () => {
    factsMock.mockResolvedValueOnce({ kind: 'refused', reason: { type: 'sourceGone' } })
    await openSuggestedOps()

    await reloadGroup(BIG)

    expect(reviewOf(BIG)?.sourceUnreachable).toBeNull()
    expect(reviewOf(BIG)?.liveFacts.size).toBe(INITIAL_ROWS)
  })

  it('asks again on the next scroll when the store refused', async () => {
    factsMock.mockResolvedValueOnce({ kind: 'refused', reason: { type: 'store', detail: 'database is locked' } })
    await openSuggestedOps()
    expect(reviewOf(BIG)?.liveFacts.size).toBe(0)

    await showRange(BIG, 0, INITIAL_ROWS)

    expect(reviewOf(BIG)?.liveFacts.size).toBe(INITIAL_ROWS)
  })

  it('asks again on the next scroll when a check threw', async () => {
    factsMock.mockRejectedValueOnce(new Error('share went away'))
    await openSuggestedOps()
    expect(reviewOf(BIG)?.liveFacts.size).toBe(0)

    await showRange(BIG, 0, INITIAL_ROWS)

    expect(reviewOf(BIG)?.liveFacts.size).toBe(INITIAL_ROWS)
  })
})

describe('approving', () => {
  it('sends the ids the user turned OFF, never the ones they kept', async () => {
    await openSuggestedOps()
    toggleOp(BIG, 4)
    toggleOp(BIG, 9)

    await approveGroup(BIG)

    expect(approveMock).toHaveBeenCalledWith(BIG, [4, 9])
  })

  it('carries an empty list when the whole group is approved, at any size', async () => {
    await openSuggestedOps()

    await approveGroup(BIG)

    expect(approveMock).toHaveBeenCalledWith(BIG, [])
  })

  it('sends only that group’s deselection', async () => {
    listMock.mockResolvedValue([sweep([group(7, 3), group(8, 3)])])
    await openSuggestedOps()
    toggleOp(8, 1)

    await approveGroup(7)

    expect(approveMock).toHaveBeenCalledWith(7, [])
  })

  it('closes the dialog once nothing is left to decide', async () => {
    await openSuggestedOps()
    listMock.mockResolvedValue([])

    await approveGroup(BIG)

    expect(suggestedOpsState.open).toBe(false)
    expect(suggestedOpsState.reviews.size).toBe(0)
  })

  it('stays open when other suggestions are still waiting', async () => {
    listMock.mockResolvedValue([sweep([group(7, 3), group(8, 3)])])
    await openSuggestedOps()
    listMock.mockResolvedValue([sweep([group(8, 3)])])

    await approveGroup(7)

    expect(suggestedOpsState.open).toBe(true)
    expect(reviewOf(7)).toBeUndefined()
    expect(reviewOf(8)).toBeDefined()
  })

  it('raises the changed notice when the op set moved under the approval', async () => {
    approveMock.mockResolvedValueOnce({ kind: 'listChanged' })
    await openSuggestedOps()

    await approveGroup(BIG)

    expect(reviewOf(BIG)?.changedUnderReview).toBe(true)
    expect(suggestedOpsState.open).toBe(true)
  })

  it('leaves the dialog usable when approving throws', async () => {
    approveMock.mockRejectedValueOnce(new Error('gone'))
    await openSuggestedOps()

    await approveGroup(BIG)

    expect(suggestedOpsState.busyGroupId).toBeNull()
    expect(suggestedOpsState.open).toBe(true)
  })
})

describe('an approval that refuses to start', () => {
  const notConnected = { kind: 'refused', error: { type: 'source_not_connected', path: '/DCIM/one.jpg' } }

  it('keeps the group on the list with the reason under it', async () => {
    approveMock.mockResolvedValueOnce(notConnected)
    await openSuggestedOps()

    await approveGroup(BIG)

    expect(suggestedOpsState.open).toBe(true)
    expect(suggestedOpsState.sweeps[0].groups.map((g) => g.groupId)).toEqual([BIG])
    expect(suggestedOpsState.refusals.get(BIG)).toEqual(notConnected)
    expect(reviewOf(BIG)).toBeDefined()
  })

  it('gives every refusal to start a reason, not only a volume one', async () => {
    for (const result of [{ kind: 'nothingToRun' }, { kind: 'couldNotStart' }]) {
      approveMock.mockResolvedValueOnce(result)
      await openSuggestedOps()

      await approveGroup(BIG)

      expect(suggestedOpsState.refusals.get(BIG)).toEqual(result)
      closeSuggestedOps()
    }
  })

  it('takes the reason down once the group is approved again', async () => {
    approveMock.mockResolvedValueOnce(notConnected)
    await openSuggestedOps()
    await approveGroup(BIG)
    listMock.mockResolvedValue([sweep([group(BIG, 3), group(8, 2)])])

    await approveGroup(BIG)

    expect(suggestedOpsState.refusals.has(BIG)).toBe(false)
  })

  it('takes the reason down when the group is rejected', async () => {
    approveMock.mockResolvedValueOnce(notConnected)
    await openSuggestedOps()
    await approveGroup(BIG)

    await rejectGroup(BIG)

    expect(suggestedOpsState.refusals.has(BIG)).toBe(false)
  })

  it('forgets the reasons when the dialog closes', async () => {
    approveMock.mockResolvedValueOnce(notConnected)
    await openSuggestedOps()
    await approveGroup(BIG)

    closeSuggestedOps()

    expect(suggestedOpsState.refusals.size).toBe(0)
  })

  it('says nothing under a group somebody else already answered', async () => {
    approveMock.mockResolvedValueOnce({ kind: 'alreadyAnswered' })
    await openSuggestedOps()

    await approveGroup(BIG)

    expect(suggestedOpsState.refusals.size).toBe(0)
  })
})

describe('deselection', () => {
  it('survives the row scrolling out of the loaded window', async () => {
    await openSuggestedOps()
    toggleOp(BIG, 3)

    await ensureOpWindow(BIG, 5_000)

    expect(opAt(BIG, 3)).toBeNull()
    expect(reviewOf(BIG)?.deselected.has(3)).toBe(true)
  })

  it('counts what would actually run from the COUNT, not from the rows in memory', async () => {
    await openSuggestedOps()

    toggleOp(BIG, 1)
    toggleOp(BIG, 2)

    expect(approvableCount(BIG)).toBe(59_998)
  })

  it('turns a row back on', async () => {
    await openSuggestedOps()

    toggleOp(BIG, 1)
    toggleOp(BIG, 1)

    expect(reviewOf(BIG)?.deselected.has(1)).toBe(false)
  })

  it('keeps each group’s deselection to itself', async () => {
    listMock.mockResolvedValue([sweep([group(7, 3), group(8, 3)])])
    await openSuggestedOps()

    toggleOp(7, 1)

    expect(approvableCount(7)).toBe(2)
    expect(approvableCount(8)).toBe(3)
  })
})

describe('being told a group changed', () => {
  /**
   * `amended` and `approved` carry the SAME group id and mean opposite things: one says the
   * list under the reader moved, the other says the review is over because they ended it.
   * Keying on the id alone raises "this changed" on the user's own approval, which reads as a
   * glitch rather than a bug and would be found late.
   */
  it('raises the notice for an amendment to a group on screen', async () => {
    await openSuggestedOps()

    emit?.({ pendingGroupCount: 1, pendingOpCount: 3, groupId: BIG, reason: 'amended' })

    expect(reviewOf(BIG)?.changedUnderReview).toBe(true)
  })

  it("stays quiet on the user's own approval of the group", async () => {
    await openSuggestedOps()

    emit?.({ pendingGroupCount: 0, pendingOpCount: 0, groupId: BIG, reason: 'approved' })

    expect(reviewOf(BIG)?.changedUnderReview).toBe(false)
  })

  it('raises it only on the group that was amended', async () => {
    listMock.mockResolvedValue([sweep([group(7, 3), group(8, 3)])])
    await openSuggestedOps()

    emit?.({ pendingGroupCount: 2, pendingOpCount: 9, groupId: 8, reason: 'amended' })

    expect(reviewOf(7)?.changedUnderReview).toBe(false)
    expect(reviewOf(8)?.changedUnderReview).toBe(true)
  })

  it('stays quiet for a whole sweep landing, which names no group', async () => {
    await openSuggestedOps()

    emit?.({ pendingGroupCount: 3, pendingOpCount: 20, groupId: null, reason: 'proposed' })

    expect(reviewOf(BIG)?.changedUnderReview).toBe(false)
  })

  it('stops listening once the dialog closes', async () => {
    await openSuggestedOps()
    closeSuggestedOps()

    expect(emit).toBeNull()
  })
})

describe('a group that changed under the review', () => {
  it('announces the change and leaves the rows exactly where they are', async () => {
    await openSuggestedOps()
    const before = reviewOf(BIG)?.window?.ops[0]?.opId

    // The agent amended the group: same id, different op count.
    listMock.mockResolvedValueOnce([sweep([group(BIG, 9)])])
    await refreshSuggestions()

    expect(reviewOf(BIG)?.changedUnderReview).toBe(true)
    expect(reviewOf(BIG)?.window?.ops[0]?.opId).toBe(before)
  })

  it('stays quiet when nothing about the group moved', async () => {
    await openSuggestedOps()

    await refreshSuggestions()

    expect(reviewOf(BIG)?.changedUnderReview).toBe(false)
  })

  it('drops the review when the group the user was reading is gone', async () => {
    await openSuggestedOps()

    listMock.mockResolvedValueOnce([])
    await refreshSuggestions()

    expect(reviewOf(BIG)).toBeUndefined()
  })

  it('reloads on request, and keeps what the user turned off', async () => {
    await openSuggestedOps()
    toggleOp(BIG, 2)
    emit?.({ pendingGroupCount: 1, pendingOpCount: 3, groupId: BIG, reason: 'amended' })
    pageMock.mockClear()

    await reloadGroup(BIG)

    expect(reviewOf(BIG)?.changedUnderReview).toBe(false)
    expect(pageMock).toHaveBeenCalledTimes(1)
    expect(reviewOf(BIG)?.deselected.has(2)).toBe(true)
  })
})

describe('rejecting', () => {
  it('records the no and re-reads', async () => {
    await openSuggestedOps()
    listMock.mockResolvedValueOnce([])

    await rejectGroup(BIG)

    expect(rejectMock).toHaveBeenCalledWith(BIG)
    expect(reviewOf(BIG)).toBeUndefined()
    expect(suggestedOpsState.sweeps).toEqual([])
    expect(suggestedOpsState.busyGroupId).toBeNull()
  })

  it('re-reads rather than insisting when somebody already answered the group', async () => {
    rejectMock.mockResolvedValueOnce({ kind: 'alreadyAnswered' })
    await openSuggestedOps()

    await rejectGroup(BIG)

    expect(listMock).toHaveBeenCalledTimes(2)
    expect(suggestedOpsState.loadError).toBe(false)
  })

  it('leaves the dialog usable when the rejection throws', async () => {
    rejectMock.mockRejectedValueOnce(new Error('gone'))
    await openSuggestedOps()

    await rejectGroup(BIG)

    expect(suggestedOpsState.busyGroupId).toBeNull()
    expect(suggestedOpsState.open).toBe(true)
  })
})

describe('a read or an answer that does not happen', () => {
  it('says the files could not load, and loads them when asked again', async () => {
    pageMock.mockRejectedValueOnce(new SuggestedOpsFailure({ type: 'store', detail: 'database is locked' }))

    await openSuggestedOps()

    expect(reviewOf(BIG)?.windowError).toBe(true)

    await ensureOpWindow(BIG, 0)

    expect(reviewOf(BIG)?.windowError).toBe(false)
    expect(opAt(BIG, 0)).not.toBeNull()
  })

  it('re-reads after an approval nobody can vouch for, and says to check the queue', async () => {
    // The worker thread died, so whether the group was claimed is unknown. The re-read shows
    // what the store holds; the notice keeps the person from approving twice on a guess.
    approveMock.mockRejectedValueOnce(new SuggestedOpsFailure({ type: 'approvalDidntFinish' }))
    await openSuggestedOps()
    listMock.mockClear()

    await approveGroup(BIG)

    expect(listMock).toHaveBeenCalledTimes(1)
    expect(suggestedOpsState.decisionNotice).toBe('suggestedOps.approvalUnsure')
    expect(suggestedOpsState.busyGroupId).toBeNull()
  })

  it('re-reads after an approval the store refused, and asks to try again', async () => {
    approveMock.mockRejectedValueOnce(new SuggestedOpsFailure({ type: 'store', detail: 'disk I/O error' }))
    await openSuggestedOps()
    listMock.mockClear()

    await approveGroup(BIG)

    expect(listMock).toHaveBeenCalledTimes(1)
    expect(suggestedOpsState.decisionNotice).toBe('suggestedOps.decisionNotRecorded')
  })

  it('re-reads after a rejection that did not record, and says so', async () => {
    rejectMock.mockRejectedValueOnce(new SuggestedOpsFailure({ type: 'storeNotOpen' }))
    await openSuggestedOps()
    listMock.mockClear()

    await rejectGroup(BIG)

    expect(listMock).toHaveBeenCalledTimes(1)
    expect(suggestedOpsState.decisionNotice).toBe('suggestedOps.decisionNotRecorded')
  })

  it('takes the notice down once the next answer lands', async () => {
    approveMock.mockRejectedValueOnce(new SuggestedOpsFailure({ type: 'store', detail: 'disk I/O error' }))
    listMock.mockResolvedValue([sweep([group(7, 3), group(8, 3)])])
    await openSuggestedOps()
    await approveGroup(7)

    await rejectGroup(8)

    expect(suggestedOpsState.decisionNotice).toBeNull()
  })
})

describe('closing', () => {
  it('drops every review and its deselection', async () => {
    await openSuggestedOps()
    toggleOp(BIG, 1)

    closeSuggestedOps()

    expect(suggestedOpsState.open).toBe(false)
    expect(suggestedOpsState.reviews.size).toBe(0)
  })
})
