/** The checked tracks of the Tags tab: a tool works on them alone, and a reload drops the ones that are gone. */
import { describe, expect, it } from 'vitest'
import { createTrackSelection } from './track-selection.svelte'

const ALL = ['/b/01.mp3', '/b/02.mp3', '/b/03.mp3', '/b/04.mp3']

describe('createTrackSelection', () => {
  it('scope is all while nothing is checked, so a tool takes every track', () => {
    const selection = createTrackSelection()
    expect(selection.scope(ALL)).toEqual({ kind: 'all' })
  })

  it('toggle checks and unchecks a track, and scope lists the checked ones in view order', () => {
    const selection = createTrackSelection()
    selection.toggle('/b/03.mp3')
    selection.toggle('/b/01.mp3')
    expect(selection.scope(ALL)).toEqual({ kind: 'some', paths: ['/b/01.mp3', '/b/03.mp3'] })
    selection.toggle('/b/03.mp3')
    expect(selection.scope(ALL)).toEqual({ kind: 'some', paths: ['/b/01.mp3'] })
    selection.toggle('/b/01.mp3')
    expect(selection.scope(ALL)).toEqual({ kind: 'all' })
  })

  it('setAll checks or unchecks every given path', () => {
    const selection = createTrackSelection()
    selection.setAll(ALL, true)
    expect([...selection.selected].sort()).toEqual(ALL)
    selection.setAll(['/b/02.mp3'], false)
    expect(selection.scope(ALL)).toEqual({ kind: 'some', paths: ['/b/01.mp3', '/b/03.mp3', '/b/04.mp3'] })
    selection.setAll(ALL, false)
    expect(selection.scope(ALL)).toEqual({ kind: 'all' })
  })

  it('prune drops the paths a reload no longer lists', () => {
    const selection = createTrackSelection()
    selection.setAll(ALL, true)
    selection.prune(['/b/02.mp3', '/b/04.mp3'])
    expect([...selection.selected].sort()).toEqual(['/b/02.mp3', '/b/04.mp3'])
  })

  it('scope ignores a checked path the view does not list, and is null when none of them is', () => {
    const selection = createTrackSelection()
    selection.toggle('/gone.mp3')
    expect(selection.scope(ALL)).toEqual({ kind: 'all' })
  })

  it('a change replaces the set instead of mutating the one a reader holds', () => {
    const selection = createTrackSelection()
    selection.toggle('/b/01.mp3')
    const before = selection.selected
    selection.toggle('/b/02.mp3')
    expect([...before]).toEqual(['/b/01.mp3'])
  })

  it('a selection whose tracks all vanished is gone, not all; acting again or clearGone resets it', () => {
    const selection = createTrackSelection()
    selection.setAll(['/b/01.mp3', '/b/02.mp3'], true)
    selection.prune(['/b/03.mp3'])
    expect(selection.scope(['/b/03.mp3'])).toEqual({ kind: 'gone' })
    selection.clearGone()
    expect(selection.scope(['/b/03.mp3'])).toEqual({ kind: 'all' })
    selection.setAll(['/b/01.mp3'], true)
    selection.prune(['/b/03.mp3'])
    selection.toggle('/b/03.mp3')
    expect(selection.scope(['/b/03.mp3'])).toEqual({ kind: 'some', paths: ['/b/03.mp3'] })
  })

  it('a partial vanish leaves the rest checked and is not gone', () => {
    const selection = createTrackSelection()
    selection.setAll(ALL, true)
    selection.prune(['/b/02.mp3'])
    expect(selection.gone).toBe(false)
    expect(selection.scope(['/b/02.mp3'])).toEqual({ kind: 'some', paths: ['/b/02.mp3'] })
  })
})
