/**
 * The confirm, run, and result flow over a fake IPC: the run names the preview it was shown, a changed plan shows
 * the confirm again with why, a stale compare can't run, progress and the report land only for this session, and
 * leaving the result compares again.
 */
import { describe, expect, it, vi } from 'vitest'
import { SyncCommandFailure, type SyncRunFinished, type SyncRunProgress } from '#lib/tauri-commands/sync.js'
import { createSyncRun, type SyncRunIpc } from './sync-run.svelte'
import { preview, report, settle } from './sync-test-fixtures'

const DEFAULTS = {
  keep: { place: 'trashElseVersions' as const, location: { targetRoot: { folderName: '.cmdr-sync/versions' } } },
  timeToleranceSecs: 2,
  ignoreHourShift: false,
  guards: { maxRemovePercent: 50, maxRemoveCount: 0, sentinel: null },
}

function setup(opts: { now?: () => number } = {}) {
  let finished: ((p: SyncRunFinished) => void) | undefined
  const ipc: SyncRunIpc = {
    getSyncConfirm: vi.fn(() =>
      Promise.resolve(
        preview({
          groups: [
            { key: 'copyRight', files: 2, bytes: 20, keep: null },
            { key: 'removeRight', files: 1, bytes: 5, keep: { type: 'trash' } },
          ],
        }),
      ),
    ),
    startSyncRun: vi.fn(() => Promise.resolve()),
    setSyncRunPaused: vi.fn(() => Promise.resolve()),
    cancelSyncRun: vi.fn(() => Promise.resolve()),
    onSyncRunProgress: vi.fn(() => Promise.resolve(() => {})),
    onSyncRunFinished: vi.fn((h: (p: SyncRunFinished) => void) => {
      finished = h
      return Promise.resolve(() => {})
    }),
  }
  const session = {
    sessionId: 's1',
    filters: {
      toRight: true,
      equal: false,
      different: true,
      toLeft: true,
      duplicates: true,
      singles: true,
      unknown: true,
      warningsOnly: false,
    },
    compare: vi.fn(() => Promise.resolve()),
    refresh: vi.fn(),
  }
  const run = createSyncRun({ ipc, session, readDefaults: () => DEFAULTS, now: opts.now })
  return { run, ipc, session, finish: (p: SyncRunFinished) => finished?.(p) }
}

describe('sync run', () => {
  it('runs the ticked groups of the preview it showed, with its Settings', async () => {
    const { run, ipc } = setup()
    await run.open()
    await run.openConfirm()
    expect(run.phase).toBe('confirming')
    expect(run.accepted).toEqual(['copyRight', 'removeRight'])
    run.toggleGroup('removeRight')
    await run.accept()
    expect(ipc.startSyncRun).toHaveBeenCalledWith('s1', {
      revision: 7,
      groups: ['copyRight'],
      acknowledged: [],
      defaults: DEFAULTS,
    })
    expect(run.phase).toBe('running')
  })

  it('a plan changed since the confirm shows it again with why, and runs nothing', async () => {
    const { run, ipc } = setup()
    await run.openConfirm()
    vi.mocked(ipc.startSyncRun).mockRejectedValueOnce(new SyncCommandFailure({ type: 'planChanged' }))
    vi.mocked(ipc.getSyncConfirm).mockResolvedValueOnce(
      preview({ revision: 8, groups: [{ key: 'copyRight', files: 3, bytes: 30, keep: null }] }),
    )
    run.toggleGroup('removeRight')
    await run.accept()
    expect(run.phase).toBe('confirming')
    expect(run.notice).toBe('planChanged')
    expect(run.preview?.revision).toBe(8)
    expect(run.accepted).toEqual(['copyRight'])
    expect(ipc.getSyncConfirm).toHaveBeenCalledTimes(2)
  })

  it('a stale compare says compare again, which closes the confirm and compares', async () => {
    const { run, ipc, session } = setup()
    await run.openConfirm()
    vi.mocked(ipc.startSyncRun).mockRejectedValueOnce(new SyncCommandFailure({ type: 'staleCompare' }))
    await run.accept()
    expect(run.notice).toBe('staleCompare')
    expect(run.phase).toBe('confirming')
    await run.accept()
    expect(ipc.startSyncRun).toHaveBeenCalledOnce()
    await run.compareAgain()
    expect(run.phase).toBe('idle')
    expect(session.compare).toHaveBeenCalledOnce()
  })

  it('another refusal is shown by its variant', async () => {
    const { run, ipc } = setup()
    await run.openConfirm()
    vi.mocked(ipc.startSyncRun).mockRejectedValueOnce(
      new SyncCommandFailure({ type: 'volumeNotConnected', side: 'right', volumeId: 'smb-1' }),
    )
    await run.accept()
    expect(run.problem).toEqual({ type: 'volumeNotConnected', side: 'right', volumeId: 'smb-1' })
  })

  it('the report of this session ends the run, refreshes the summary, and leaving it compares again', async () => {
    const { run, session, finish } = setup()
    await run.open()
    await run.openConfirm()
    await run.accept()
    finish({ sessionId: 'other', report: report() })
    expect(run.phase).toBe('running')
    finish({
      sessionId: 's1',
      report: report({
        items: [
          { item: 0, result: 'done' },
          { item: 3, result: 'changedSinceCompare' },
        ],
      }),
    })
    expect(run.phase).toBe('finished')
    expect(run.notDone).toEqual([3])
    expect(session.refresh).toHaveBeenCalledOnce()
    await run.dismissResult()
    await settle()
    expect(session.compare).toHaveBeenCalledOnce()
    expect(run.phase).toBe('idle')
  })

  it('a run that finishes before its start answers still ends finished, with its progress', async () => {
    const { run, ipc, finish } = setup()
    let progress: ((p: SyncRunProgress) => void) | undefined
    vi.mocked(ipc.onSyncRunProgress).mockImplementation((h) => {
      progress = h
      return Promise.resolve(() => {})
    })
    await run.open()
    await run.openConfirm()
    // The backend spawns the run, so its events can overtake the answer to `startSyncRun`.
    vi.mocked(ipc.startSyncRun).mockImplementationOnce(() => {
      progress?.({
        sessionId: 's1',
        progress: { stepsDone: 1, stepsTotal: 2, itemsDone: 1, itemsTotal: 2, bytesDone: 10, bytesTotal: 20 },
      })
      expect(run.progress?.itemsDone).toBe(1)
      finish({ sessionId: 's1', report: report() })
      return Promise.resolve()
    })
    await run.accept()
    expect(run.phase).toBe('finished')
    expect(run.report).not.toBeNull()
  })

  it('a group no button releases is never ticked and can’t be ticked', async () => {
    const { run, ipc } = setup()
    vi.mocked(ipc.getSyncConfirm).mockResolvedValueOnce(
      preview({
        groups: [
          { key: 'copyLeft', files: 2, bytes: 20, keep: null },
          { key: 'removeLeft', files: 1, bytes: 5, keep: { type: 'trash' } },
        ],
        blocks: [{ type: 'emptyRoot', side: 'right' }],
      }),
    )
    await run.openConfirm()
    expect(run.accepted).toEqual(['copyLeft'])
    run.toggleGroup('removeLeft')
    expect(run.accepted).toEqual(['copyLeft'])
  })

  it('Show in grid reaches the rows a guard held too', async () => {
    const { run, finish } = setup()
    await run.open()
    await run.openConfirm()
    await run.accept()
    finish({
      sessionId: 's1',
      report: report({ heldByGuard: [{ item: 9, block: { type: 'emptyRoot', side: 'right' } }] }),
    })
    expect(run.notDone).toEqual([9])
  })

  it('the ETA leaves out the time the run sat paused', async () => {
    let now = 0
    const { run, ipc } = setup({ now: () => now })
    let progress: ((p: SyncRunProgress) => void) | undefined
    vi.mocked(ipc.onSyncRunProgress).mockImplementation((h) => {
      progress = h
      return Promise.resolve(() => {})
    })
    await run.open()
    await run.openConfirm()
    await run.accept()
    now = 10_000
    await run.setPaused(true)
    now = 110_000
    await run.setPaused(false)
    progress?.({
      sessionId: 's1',
      progress: { stepsDone: 1, stepsTotal: 2, itemsDone: 1, itemsTotal: 2, bytesDone: 50, bytesTotal: 100 },
    })
    // 10 s running for half the bytes: 10 s left, whatever the 100 s paused.
    expect(run.etaSeconds).toBe(10)
  })

  it('a pause says it waits for the step under way until that step ends', async () => {
    const { run, ipc } = setup()
    let progress: ((p: SyncRunProgress) => void) | undefined
    vi.mocked(ipc.onSyncRunProgress).mockImplementation((h) => {
      progress = h
      return Promise.resolve(() => {})
    })
    await run.open()
    await run.openConfirm()
    await run.accept()
    await run.setPaused(true)
    expect(run.pausing).toBe(true)
    progress?.({
      sessionId: 's1',
      progress: { stepsDone: 1, stepsTotal: 2, itemsDone: 1, itemsTotal: 2, bytesDone: 50, bytesTotal: 100 },
    })
    expect(run.pausing).toBe(false)
    expect(run.phase).toBe('paused')
  })

  it('cancel says it is stopping and asks only once', async () => {
    const { run, ipc } = setup()
    await run.openConfirm()
    await run.accept()
    await run.cancel()
    expect(run.cancelling).toBe(true)
    await run.cancel()
    expect(ipc.cancelSyncRun).toHaveBeenCalledOnce()
  })

  it('pause asks the backend and says it lands between steps', async () => {
    const { run, ipc } = setup()
    await run.openConfirm()
    await run.accept()
    await run.setPaused(true)
    expect(ipc.setSyncRunPaused).toHaveBeenCalledWith('s1', true)
    expect(run.phase).toBe('paused')
    await run.cancel()
    expect(ipc.cancelSyncRun).toHaveBeenCalledWith('s1')
  })
})
