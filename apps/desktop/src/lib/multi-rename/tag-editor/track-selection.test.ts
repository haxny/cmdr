/**
 * The selected tracks of the Tags window's Tracks pane, file-pane style (`createSelectionState`): a tool works on
 * them alone, and a reload keeps the ones still listed (by path) and drops the rest.
 */
import { describe, expect, it } from 'vitest'
import { createTrackSelection } from './track-selection.svelte'

const ALL = ['/b/01.mp3', '/b/02.mp3', '/b/03.mp3', '/b/04.mp3']

function fresh(paths: readonly string[] = ALL) {
  const selection = createTrackSelection()
  selection.sync(paths)
  return selection
}

describe('createTrackSelection', () => {
  it('scope is all while nothing is selected, so a tool takes every track', () => {
    expect(fresh().scope()).toEqual({ kind: 'all' })
  })

  it('toggleAt selects and deselects a row, and scope lists the selected paths in view order', () => {
    const selection = fresh()
    selection.toggleAt(2)
    selection.toggleAt(0)
    expect(selection.scope()).toEqual({ kind: 'some', paths: ['/b/01.mp3', '/b/03.mp3'] })
    expect(selection.has(2)).toBe(true)
    selection.toggleAt(2)
    selection.toggleAt(0)
    expect(selection.scope()).toEqual({ kind: 'all' })
  })

  it('extend toggles the row the cursor leaves and fills what it jumps over (⇧↓ from row 0 selects row 0)', () => {
    const selection = fresh()
    selection.extend({ from: 0, to: 1, overflow: false })
    expect([...selection.selected]).toEqual(['/b/01.mp3'])
    selection.extend({ from: 1, to: 2, overflow: false })
    expect(selection.scope()).toEqual({ kind: 'some', paths: ['/b/01.mp3', '/b/02.mp3'] })
  })

  it('selectAll, invert, and deselectAll work over every listed track', () => {
    const selection = fresh()
    selection.selectAll()
    expect([...selection.selected].sort()).toEqual(ALL)
    selection.toggleAt(1)
    selection.invert()
    expect(selection.scope()).toEqual({ kind: 'some', paths: ['/b/02.mp3'] })
    selection.deselectAll()
    expect(selection.scope()).toEqual({ kind: 'all' })
  })

  it('setAll selects or clears every track (the header box)', () => {
    const selection = fresh()
    selection.setAll(true)
    expect(selection.count).toBe(4)
    selection.setAll(false)
    expect(selection.count).toBe(0)
  })

  it('sync keeps the selected tracks by path when the list changes order or shrinks', () => {
    const selection = fresh()
    selection.toggleAt(1)
    selection.toggleAt(3)
    selection.sync(['/b/04.mp3', '/b/03.mp3', '/b/02.mp3'])
    expect(selection.has(0)).toBe(true)
    expect(selection.has(2)).toBe(true)
    expect(selection.scope()).toEqual({ kind: 'some', paths: ['/b/04.mp3', '/b/02.mp3'] })
  })

  it('a selection whose tracks all vanished is gone, not all; acting again or clearGone resets it', () => {
    const selection = fresh()
    selection.toggleAt(0)
    selection.sync(['/b/09.mp3'])
    expect(selection.scope()).toEqual({ kind: 'gone' })
    selection.clearGone()
    expect(selection.scope()).toEqual({ kind: 'all' })
    selection.sync(ALL)
    selection.toggleAt(0)
    selection.sync(['/b/09.mp3'])
    expect(selection.gone).toBe(true)
    selection.toggleAt(0)
    expect(selection.scope()).toEqual({ kind: 'some', paths: ['/b/09.mp3'] })
  })

  it('a partial vanish leaves the rest selected and is not gone', () => {
    const selection = fresh()
    selection.selectAll()
    selection.sync(['/b/02.mp3'])
    expect(selection.gone).toBe(false)
    expect(selection.scope()).toEqual({ kind: 'some', paths: ['/b/02.mp3'] })
  })
})
