/**
 * `trash-undo.ts`: reading an undo's report honestly, and saying it in en.
 *
 * The honesty rule is the point of these: an undo that left anything in the trash
 * must never render as a clean success. The rendered-string assertions double as
 * the en parity net for the `fileOperations.trash.*` keys, so a copy edit lands in
 * the catalog AND here together.
 */

import { describe, it, expect, beforeAll, afterAll, beforeEach, vi } from 'vitest'
import { _setLocaleForTests } from '#lib/intl/locale.js'
import {
  trashUndoOutcome,
  trashUndoMessage,
  trashUndoLevel,
  runTrashUndo,
  rememberTrashForUndo,
  trashCompleteToastId,
  undoLastTrash,
  trashUndoTarget,
  isTrashUndoAvailable,
  resyncTrashUndoMenu,
  trashUndoCommandLabel,
  _resetForTesting,
  type TrashUndoTarget,
} from './trash-undo'
import type { UndoReport, OperationUndoOutcome } from '#lib/tauri-commands/index.js'
import type { RollbackRefusal, SkipBreakdown } from '#lib/ipc/bindings.js'
import type { ToastOptions } from '#lib/ui/toast/toast-store.svelte.js'

const { undoOperations, updateUndoTrashMenu, addToast, dismissToast } = vi.hoisted(() => ({
  undoOperations: vi.fn<(ids: string[]) => Promise<UndoReport>>(),
  updateUndoTrashMenu: vi.fn<(target: TrashUndoTarget | null) => Promise<void>>(() => Promise.resolve()),
  addToast: vi.fn<(message: string, options?: ToastOptions) => string>(),
  dismissToast: vi.fn<(id: string) => void>(),
}))

vi.mock('#lib/tauri-commands/index.js', () => ({ undoOperations, updateUndoTrashMenu }))
vi.mock('#lib/ui/toast/index.js', () => ({ addToast, dismissToast }))

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

function operation(overrides: Partial<OperationUndoOutcome> = {}): OperationUndoOutcome {
  return {
    operationId: 'op-1',
    restored: 0,
    skipped: 0,
    skips: [],
    finalState: 'rolledBack',
    refusal: null,
    ...overrides,
  }
}

function report(overrides: Partial<UndoReport> = {}): UndoReport {
  return { operations: [operation()], restored: 0, skipped: 0, ...overrides }
}

describe('trashUndoOutcome', () => {
  it('reads a complete restore as restored', () => {
    expect(trashUndoOutcome(report({ restored: 3 }))).toEqual({ status: 'restored', restored: 3 })
  })

  it('reads anything left in the trash as partial, never as a success', () => {
    expect(trashUndoOutcome(report({ restored: 2, skipped: 1 }))).toEqual({
      status: 'partial',
      restored: 2,
      skipped: 1,
      gone: 0,
    })
  })

  it('treats a refusal as partial even when no item was skipped', () => {
    // A refusal carries no per-item numbers at all, so folding it into `skipped`
    // would understate what was missed: 2 came back, the rest never got a chance.
    const refusal: RollbackRefusal = { kind: 'volumeUnavailable', detail: { volumeId: 'usb-1' } }
    const outcome = trashUndoOutcome(
      report({ restored: 2, skipped: 0, operations: [operation({ restored: 2, refusal })] }),
    )
    expect(outcome).toEqual({ status: 'partial', restored: 2, skipped: 0, gone: 0 })
  })

  it('reads nothing-reversed-and-nothing-attempted as unavailable', () => {
    expect(trashUndoOutcome(report())).toEqual({ status: 'unavailable' })
  })

  it('does not call an all-skipped undo a success', () => {
    expect(trashUndoOutcome(report({ restored: 0, skipped: 4 })).status).toBe('partial')
  })

  it('reads items the trash no longer holds as gone, not as put back', () => {
    const skips: SkipBreakdown[] = [{ reason: 'vanished', count: 3, exampleName: 'a.txt' }]
    const outcome = trashUndoOutcome(
      report({ restored: 0, skipped: 3, operations: [operation({ skipped: 3, skips })] }),
    )
    expect(outcome).toEqual({ status: 'gone', gone: 3 })
  })

  it('keeps gone items apart from the ones that stayed in the trash', () => {
    const skips: SkipBreakdown[] = [
      { reason: 'vanished', count: 1, exampleName: 'a.txt' },
      { reason: 'restoreTargetOccupied', count: 2, exampleName: 'b.txt' },
    ]
    const outcome = trashUndoOutcome(
      report({ restored: 4, skipped: 3, operations: [operation({ restored: 4, skipped: 3, skips })] }),
    )
    expect(outcome).toEqual({ status: 'partial', restored: 4, skipped: 2, gone: 1 })
  })
})

describe('trashUndoMessage (en)', () => {
  it('words a single restored file in the singular', () => {
    expect(trashUndoMessage({ status: 'restored', restored: 1 })).toBe('Put back 1 file.')
  })

  it('words several restored files in the plural, with thousands separators', () => {
    expect(trashUndoMessage({ status: 'restored', restored: 1234 })).toBe('Put back 1,234 files.')
  })

  it('names both halves of a partial result', () => {
    expect(trashUndoMessage({ status: 'partial', restored: 2, skipped: 1, gone: 0 })).toBe(
      'Put back 2 files; 1 item stayed in the trash.',
    )
  })

  it('inflects each half on its own count', () => {
    // Both clauses carry a plural driver on purpose: a bare preformatted number
    // gives a translator nothing to agree with, which is what sent three
    // languages hunting for a workaround (`docs/i18n/*/style.md` § Plurals).
    expect(trashUndoMessage({ status: 'partial', restored: 1, skipped: 3, gone: 0 })).toBe(
      'Put back 1 file; 3 items stayed in the trash.',
    )
  })

  it('says plainly when the trash no longer holds what was trashed', () => {
    expect(trashUndoMessage({ status: 'gone', gone: 1 })).toBe('Nothing to put back: it’s no longer in the trash.')
    expect(trashUndoMessage({ status: 'gone', gone: 4 })).toBe('Nothing to put back: they’re no longer in the trash.')
  })

  it('adds what was gone to a partial result', () => {
    expect(trashUndoMessage({ status: 'partial', restored: 2, skipped: 0, gone: 1 })).toBe(
      'Put back 2 files. 1 item was no longer in the trash.',
    )
    expect(trashUndoMessage({ status: 'partial', restored: 2, skipped: 1, gone: 3 })).toBe(
      'Put back 2 files; 1 item stayed in the trash. 3 items were no longer in the trash.',
    )
  })

  it('explains an undo that had nothing to reverse', () => {
    expect(trashUndoMessage({ status: 'unavailable' })).toBe(
      'Nothing to put back. These items may already be back, or their drive isn’t connected.',
    )
  })
})

describe('trashUndoLevel', () => {
  it('only a complete restore reads as a success', () => {
    expect(trashUndoLevel({ status: 'restored', restored: 1 })).toBe('success')
    expect(trashUndoLevel({ status: 'partial', restored: 1, skipped: 1, gone: 0 })).toBe('info')
    expect(trashUndoLevel({ status: 'unavailable' })).toBe('info')
  })
})

describe('runTrashUndo', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  /** Every `addToast` call, as (message, options) pairs. */
  function raised(): { message: string; options: ToastOptions }[] {
    return addToast.mock.calls.map((call) => ({ message: call[0], options: call[1] ?? {} }))
  }

  it('holds a persistent progress toast while the restore runs', async () => {
    undoOperations.mockResolvedValue(report({ restored: 2 }))
    await runTrashUndo('op-1')

    // A restore is a queued operation and waits out the volume's lane, so it can
    // outlive any transient timeout. Nothing must be able to time it out.
    const progress = raised()[0]
    expect(progress.message).toBe('Putting them back…')
    expect(progress.options.dismissal).toBe('persistent')
  })

  it('undoes exactly the operation it was given', async () => {
    undoOperations.mockResolvedValue(report({ restored: 2 }))
    await runTrashUndo('op-1')
    expect(undoOperations).toHaveBeenCalledWith(['op-1'])
  })

  it('takes the progress toast down and reports what came back', async () => {
    undoOperations.mockResolvedValue(report({ restored: 2 }))
    await runTrashUndo('op-1')

    // Replacing in place can't work here: `addToast` replaces content and level
    // but never dismissal, so the persistent one has to go and a transient one
    // takes its place.
    expect(dismissToast).toHaveBeenCalledWith('trash-undo')
    const result = raised()[1]
    expect(result.message).toBe('Put back 2 files.')
    expect(result.options.level).toBe('success')
    expect(result.options.dismissal).toBeUndefined()
  })

  it('reports a partial restore as info, not success', async () => {
    undoOperations.mockResolvedValue(report({ restored: 2, skipped: 1 }))
    await runTrashUndo('op-1')

    const result = raised()[1]
    expect(result.message).toBe('Put back 2 files; 1 item stayed in the trash.')
    expect(result.options.level).toBe('info')
  })

  it('still clears the progress toast when the undo never runs', async () => {
    // A wedged volume or a dead journal rejects the IPC. Leaving a persistent
    // "Putting them back…" on screen forever is the one outcome that must not
    // happen, whatever else does.
    undoOperations.mockRejectedValue(new Error('the volume went away'))
    await runTrashUndo('op-1')

    expect(dismissToast).toHaveBeenCalledWith('trash-undo')
    expect(raised()[1].message).toBe(
      'Nothing to put back. These items may already be back, or their drive isn’t connected.',
    )
  })

  it('does not let a refused undo reject into the click handler', async () => {
    undoOperations.mockRejectedValue(new Error('nope'))
    await expect(runTrashUndo('op-1')).resolves.toBeUndefined()
  })
})

/** A trash of `paths`, every one of which went. */
function trashed(...paths: string[]): { sourcePaths: string[]; itemCount: number } {
  return { sourcePaths: paths, itemCount: paths.length }
}

/**
 * Edit > Undo trash and its command: the trash toast's Undo, reachable after the toast has
 * gone and without a pointer (#439).
 */
describe('undoLastTrash', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    _resetForTesting()
    undoOperations.mockResolvedValue(report({ restored: 1 }))
  })

  it('undoes the newest trash and takes its toast down', async () => {
    rememberTrashForUndo('op-old', trashed('/a/old.txt'))
    rememberTrashForUndo('op-new', trashed('/a/new.txt'))

    await undoLastTrash()

    expect(undoOperations).toHaveBeenCalledWith(['op-new'])
    expect(dismissToast).toHaveBeenCalledWith(trashCompleteToastId('op-new'))
  })

  it('does nothing at all once that trash is already back: the menu item is greyed out by then', async () => {
    rememberTrashForUndo('op-1', trashed('/a/doc.txt'))
    await runTrashUndo('op-1')
    vi.clearAllMocks()

    await undoLastTrash()

    expect(undoOperations).not.toHaveBeenCalled()
    expect(addToast).not.toHaveBeenCalled()
  })

  it('undoes each trash only once', async () => {
    rememberTrashForUndo('op-1', trashed('/a/doc.txt'))
    await undoLastTrash()
    await undoLastTrash()

    expect(undoOperations).toHaveBeenCalledTimes(1)
  })
})

/**
 * What Edit > Undo move to trash offers: the newest trash this session, named (one item) or
 * counted (several), or nothing, which greys the item out and hides the palette row.
 */
describe('the trash waiting to be undone', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    _resetForTesting()
    undoOperations.mockResolvedValue(report({ restored: 1 }))
  })

  it('starts with nothing to undo', () => {
    expect(trashUndoTarget()).toBeNull()
    expect(isTrashUndoAvailable()).toBe(false)
  })

  it('names a single trashed item', () => {
    rememberTrashForUndo('op-1', trashed('/Users/me/report.pdf'))
    expect(trashUndoTarget()).toEqual({ kind: 'one', name: 'report.pdf' })
    expect(isTrashUndoAvailable()).toBe(true)
  })

  it('counts several, with thousands separators', () => {
    const paths = Array.from({ length: 1234 }, (_, i) => `/a/f${String(i)}`)
    rememberTrashForUndo('op-1', trashed(...paths))
    expect(trashUndoTarget()).toEqual({ kind: 'many', countText: '1,234' })
  })

  it('names nothing when the one item that went is not knowable', () => {
    // Two asked, one refused by the OS: which one went isn't in the completion.
    rememberTrashForUndo('op-1', { sourcePaths: ['/a/x', '/a/y'], itemCount: 1 })
    expect(trashUndoTarget()).toEqual({ kind: 'unknown' })
  })

  it('counts what actually went, not what was asked', () => {
    rememberTrashForUndo('op-1', { sourcePaths: ['/a/x', '/a/y', '/a/z'], itemCount: 2 })
    expect(trashUndoTarget()).toEqual({ kind: 'many', countText: '2' })
  })

  it('remembers only the newest trash', () => {
    rememberTrashForUndo('op-old', trashed('/a/old.txt'))
    rememberTrashForUndo('op-new', trashed('/a/new.txt'))
    expect(trashUndoTarget()).toEqual({ kind: 'one', name: 'new.txt' })
  })

  it('forgets a trash once it has been undone', async () => {
    rememberTrashForUndo('op-1', trashed('/a/doc.txt'))
    await undoLastTrash()
    expect(trashUndoTarget()).toBeNull()
  })

  it('keeps the newest trash when an older one is undone from its toast', async () => {
    rememberTrashForUndo('op-old', trashed('/a/old.txt'))
    rememberTrashForUndo('op-new', trashed('/a/new.txt'))
    await runTrashUndo('op-old')
    expect(trashUndoTarget()).toEqual({ kind: 'one', name: 'new.txt' })
  })

  it('tells the menu bar on every change', async () => {
    rememberTrashForUndo('op-1', trashed('/a/doc.txt'))
    expect(updateUndoTrashMenu).toHaveBeenLastCalledWith({ kind: 'one', name: 'doc.txt' })
    await undoLastTrash()
    expect(updateUndoTrashMenu).toHaveBeenLastCalledWith(null)
  })

  it('tells a rebuilt menu bar again', () => {
    rememberTrashForUndo('op-1', trashed('/a/doc.txt'))
    vi.clearAllMocks()
    resyncTrashUndoMenu()
    expect(updateUndoTrashMenu).toHaveBeenCalledWith({ kind: 'one', name: 'doc.txt' })
  })

  it('labels the palette row the way the menu item reads', () => {
    expect(trashUndoCommandLabel()).toBe('Undo move to trash')
    rememberTrashForUndo('op-1', trashed('/a/report.pdf'))
    expect(trashUndoCommandLabel()).toBe('Undo move to trash of “report.pdf”')
    rememberTrashForUndo('op-2', trashed('/a/x', '/a/y'))
    expect(trashUndoCommandLabel()).toBe('Undo move to trash of 2 items')
    rememberTrashForUndo('op-3', { sourcePaths: ['/a/x', '/a/y'], itemCount: 1 })
    expect(trashUndoCommandLabel()).toBe('Undo move to trash')
  })
})
