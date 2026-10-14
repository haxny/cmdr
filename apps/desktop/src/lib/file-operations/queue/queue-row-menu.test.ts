/**
 * A queue row's right-click menu: which items it lists, in what order, which are live,
 * and which keyboard action each pick runs.
 */
import { describe, it, expect } from 'vitest'
import {
  queueRowMenuEntries,
  queueRowMenuKeyAction,
  queueRowMenuOffers,
  queuedNeighbors,
  type QueueRowMenuFacts,
} from './queue-row-menu'

const waiting: QueueRowMenuFacts = {
  active: false,
  paused: false,
  canShow: false,
  canRollback: false,
  canCancel: true,
  canDismiss: false,
  selectable: true,
  selected: false,
  canMoveEarlier: true,
  canMoveLater: true,
}

const running: QueueRowMenuFacts = {
  ...waiting,
  active: true,
  canShow: true,
  canRollback: true,
  canMoveEarlier: false,
  canMoveLater: false,
}

/** The entries as a compact picture: `-` is a divider, a `✓` suffix a ticked item, a `!`
 *  suffix a greyed-out one. */
function picture(facts: QueueRowMenuFacts): string[] {
  return queueRowMenuEntries(facts).map((entry) => {
    if (entry.kind === 'separator') return '-'
    const ticked = entry.kind === 'check' && entry.checked ? '✓' : ''
    return `${entry.action}${ticked}${entry.enabled ? '' : '!'}`
  })
}

describe('queueRowMenuEntries', () => {
  it('lists move, select, and cancel for a waiting row', () => {
    expect(picture(waiting)).toEqual(['moveEarlier', 'moveLater', '-', 'select', '-', 'cancel'])
  })

  it('greys Move up on the first waiting row and Move down on the last', () => {
    expect(picture({ ...waiting, canMoveEarlier: false })).toContain('moveEarlier!')
    expect(picture({ ...waiting, canMoveLater: false })).toContain('moveLater!')
  })

  it('ticks Select on a selected row, the way its checkbox is', () => {
    expect(picture({ ...waiting, selected: true })).toContain('select✓')
    expect(picture(waiting)).toContain('select')
  })

  it('leads a running row with Show and Pause, and ends it with Cancel and Rollback', () => {
    expect(picture(running)).toEqual([
      'show',
      'pause',
      '-',
      'moveEarlier!',
      'moveLater!',
      '-',
      'select',
      '-',
      'cancel',
      'rollback',
    ])
  })

  it('offers Resume on a paused row', () => {
    const items = picture({ ...running, paused: true })
    expect(items).toContain('resume')
    expect(items).not.toContain('pause')
  })

  it('greys what the running row itself would refuse', () => {
    const items = picture({ ...running, canShow: false, canRollback: false })
    expect(items).toContain('show!')
    expect(items).toContain('rollback!')
  })

  it('offers Dismiss in Cancel’s place on a failure, greying the rest', () => {
    const failed = {
      ...waiting,
      canCancel: false,
      canDismiss: true,
      selectable: false,
      canMoveEarlier: false,
      canMoveLater: false,
    }
    expect(picture(failed)).toEqual(['moveEarlier!', 'moveLater!', '-', 'select!', '-', 'dismiss'])
  })
})

describe('queueRowMenuKeyAction', () => {
  // The menu runs the very actions the keys do, so the two can't drift.
  it("maps every pick onto the window's keyboard action", () => {
    expect(queueRowMenuKeyAction('moveEarlier')).toEqual({ kind: 'move', direction: 'earlier' })
    expect(queueRowMenuKeyAction('moveLater')).toEqual({ kind: 'move', direction: 'later' })
    expect(queueRowMenuKeyAction('select')).toEqual({ kind: 'toggleSelected' })
    expect(queueRowMenuKeyAction('cancel')).toEqual({ kind: 'cancel' })
    expect(queueRowMenuKeyAction('dismiss')).toEqual({ kind: 'cancelOrDismiss' })
    expect(queueRowMenuKeyAction('show')).toEqual({ kind: 'show' })
    expect(queueRowMenuKeyAction('pause')).toEqual({ kind: 'pauseOrResume' })
    expect(queueRowMenuKeyAction('resume')).toEqual({ kind: 'pauseOrResume' })
    expect(queueRowMenuKeyAction('rollback')).toEqual({ kind: 'rollback' })
  })
})

describe('queueRowMenuOffers', () => {
  // The pick arrives after the menu closed; the row may have moved on meanwhile.
  it('refuses a Pause picked after the row got paused some other way', () => {
    expect(queueRowMenuOffers({ ...running, paused: true }, 'pause')).toBe(false)
    expect(queueRowMenuOffers({ ...running, paused: true }, 'resume')).toBe(true)
  })

  it('refuses an item that went grey', () => {
    expect(queueRowMenuOffers({ ...waiting, canMoveEarlier: false }, 'moveEarlier')).toBe(false)
  })

  it('accepts a live item', () => {
    expect(queueRowMenuOffers(waiting, 'cancel')).toBe(true)
  })
})

describe('queuedNeighbors', () => {
  const rows = [
    { id: 'run', status: 'running' },
    { id: 'a', status: 'queued' },
    { id: 'paused', status: 'paused' },
    { id: 'b', status: 'queued' },
    { id: 'failed', status: 'failed' },
  ] as const

  // Only waiting operations trade places, and only with each other.
  it('lets the first waiting row move later only', () => {
    expect(queuedNeighbors(rows, 'a')).toEqual({ canMoveEarlier: false, canMoveLater: true })
  })

  it('lets the last waiting row move earlier only, past a paused one', () => {
    expect(queuedNeighbors(rows, 'b')).toEqual({ canMoveEarlier: true, canMoveLater: false })
  })

  it('moves nothing that is not waiting', () => {
    expect(queuedNeighbors(rows, 'run')).toEqual({ canMoveEarlier: false, canMoveLater: false })
    expect(queuedNeighbors(rows, 'failed')).toEqual({ canMoveEarlier: false, canMoveLater: false })
  })
})
