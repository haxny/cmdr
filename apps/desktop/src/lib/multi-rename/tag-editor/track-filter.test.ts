/**
 * The Tracks pane's filter (⌘F): plain text matches any column, `field: text` one field, both with case and
 * diacritics folded; an unknown field name is plain text, so a title holding a colon still matches.
 */
import { describe, expect, it } from 'vitest'
import type { TrackRow } from '#lib/ipc/bindings.js'
import { parseTrackFilter, shownTrackIndices, trackMatches } from './track-filter'

function track(n: string, over: Partial<TrackRow> = {}): TrackRow {
  return {
    path: `/b/${n}.mp3`,
    fileName: `${n}.mp3`,
    track: n,
    disc: null,
    title: `T${n}`,
    durationMs: 61_000,
    changed: false,
    readError: null,
    ...over,
  }
}

const matches = (row: TrackRow, text: string): boolean => {
  const filter = parseTrackFilter(text)
  if (filter === null) throw new Error('blank filter')
  return trackMatches(row, filter)
}

describe('parseTrackFilter', () => {
  it('is null for blank text: no filter', () => {
    expect(parseTrackFilter('')).toBeNull()
    expect(parseTrackFilter('   ')).toBeNull()
  })

  it('reads `field: text` for title, disc, #, track and file, any case and spacing', () => {
    expect(parseTrackFilter('disc: 2')).toEqual({ field: 'disc', text: '2' })
    expect(parseTrackFilter('Title:část')).toEqual({ field: 'title', text: 'cast' })
    expect(parseTrackFilter('#: 1')).toEqual({ field: 'track', text: '1' })
    expect(parseTrackFilter('track : 03')).toEqual({ field: 'track', text: '03' })
    expect(parseTrackFilter('FILE: Disc 1/')).toEqual({ field: 'file', text: 'disc 1/' })
  })

  it('keeps an unknown field name as plain text, colon and all', () => {
    expect(parseTrackFilter('Part 1: Intro')).toEqual({ field: null, text: 'part 1: intro' })
  })
})

describe('trackMatches', () => {
  const row = track('07', { title: 'Část první', disc: 'CD2', fileName: 'Disc 2/07 Část.mp3', durationMs: 125_000 })

  it('plain text matches any column with case and diacritics folded', () => {
    expect(matches(row, 'cast PRVNI')).toBe(true)
    expect(matches(row, 'ČÁST')).toBe(true)
    expect(matches(row, 'disc 2/')).toBe(true)
    expect(matches(row, '2:05')).toBe(true)
    expect(matches(row, 'druhá')).toBe(false)
  })

  it('folds a decomposed (NFD) file name too, as macOS stores them', () => {
    const nfd = track('01', { title: null, fileName: 'Část.mp3' })
    expect(matches(nfd, 'část')).toBe(true)
  })

  it('a field matches that field alone', () => {
    expect(matches(row, 'title: část')).toBe(true)
    expect(matches(row, 'file: část')).toBe(true)
    expect(matches(row, 'title: disc')).toBe(false)
  })

  it('a number matches the number in # and Disc, not every value holding its digit', () => {
    expect(matches(row, 'disc: 2')).toBe(true)
    expect(matches(row, '#: 7')).toBe(true)
    expect(matches(row, '#: 07')).toBe(true)
    expect(matches(row, '#: 0')).toBe(false)
    expect(matches(track('17'), '#: 7')).toBe(false)
    expect(matches(track('3/12'), 'track: 3')).toBe(true)
    expect(matches(track('01', { disc: null }), 'disc: 1')).toBe(false)
  })

  it('a field with no text yet matches every track', () => {
    expect(matches(row, 'title:')).toBe(true)
  })
})

describe('shownTrackIndices', () => {
  const tracks = [track('01', { disc: '1' }), track('02', { disc: '2' }), track('03', { disc: '2' })]

  it('is null without a filter, and the matching rows’ indices in view order with one', () => {
    expect(shownTrackIndices(tracks, null)).toBeNull()
    expect(shownTrackIndices(tracks, '  ')).toBeNull()
    expect(shownTrackIndices(tracks, 'disc: 2')).toEqual([1, 2])
    expect(shownTrackIndices(tracks, 'nothing like it')).toEqual([])
  })
})
