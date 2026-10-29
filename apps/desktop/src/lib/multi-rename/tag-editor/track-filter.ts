/**
 * The Tracks pane's filter (⌘F), pure. Plain text matches any column (#, Disc, Title, Length, File); `field: text`
 * matches one: `title`, `disc`, `#` or `track`, `file`, `length`. Both sides fold case and diacritics (`cast` finds
 * `Část`, a decomposed macOS file name too). A number on # or Disc matches the value's first number (`disc: 2` finds
 * `2`, `02`, `CD2`, `2/3`, not `12`). An unknown field name is plain text, so `Part 1: Intro` still matches its title.
 * `TrackRow` carries no other tags, so the Album fields (artist, album…) can't be filtered on here.
 */
import type { TrackRow } from '#lib/ipc/bindings.js'
import { foldForSearch } from '#lib/utils/unicode-fuzzy.js'
import { formatTrackLength } from './shared-field-input'

export type TrackFilterField = 'title' | 'disc' | 'track' | 'file' | 'length'

/** A parsed filter: one field or any (`null`), and the folded text to find. */
export interface TrackFilter {
  field: TrackFilterField | null
  text: string
}

/** The names `field: text` takes, folded. */
const FIELD_NAMES: Readonly<Record<string, TrackFilterField>> = {
  title: 'title',
  disc: 'disc',
  '#': 'track',
  track: 'track',
  file: 'file',
  length: 'length',
}

const FIELD_SYNTAX = /^\s*([^:\s]+)\s*:\s*(.*)$/su
const DIGITS_ONLY = /^\d+$/u
const FIRST_NUMBER = /\d+/u

/** Lowercased, accents dropped, whether the text came composed or decomposed. */
function fold(text: string): string {
  return foldForSearch(text.normalize('NFC'))
}

/** The filter `input` means, or `null` when it's blank (no filter). */
export function parseTrackFilter(input: string): TrackFilter | null {
  if (input.trim() === '') return null
  const syntax = FIELD_SYNTAX.exec(input)
  const field = syntax === null ? undefined : FIELD_NAMES[fold(syntax[1])]
  if (syntax !== null && field !== undefined) return { field, text: fold(syntax[2].trim()) }
  return { field: null, text: fold(input.trim()) }
}

function valueOf(track: TrackRow, field: TrackFilterField): string {
  switch (field) {
    case 'title':
      return track.title ?? ''
    case 'disc':
      return track.disc ?? ''
    case 'track':
      return track.track ?? ''
    case 'file':
      return track.fileName
    case 'length':
      return formatTrackLength(track.durationMs)
  }
}

/** Whether a # or Disc value's first number is `wanted` (`07` is 7). */
function sameNumber(value: string, wanted: string): boolean {
  const found = FIRST_NUMBER.exec(value)
  return found !== null && Number(found[0]) === Number(wanted)
}

function fieldMatches(track: TrackRow, field: TrackFilterField, text: string): boolean {
  const value = valueOf(track, field)
  if ((field === 'track' || field === 'disc') && DIGITS_ONLY.test(text)) return sameNumber(value, text)
  return fold(value).includes(text)
}

const ANY_COLUMN: readonly TrackFilterField[] = ['track', 'disc', 'title', 'length', 'file']

/** Whether `track` shows under `filter`. A field with no text yet matches every track. */
export function trackMatches(track: TrackRow, filter: TrackFilter): boolean {
  if (filter.text === '') return true
  if (filter.field !== null) return fieldMatches(track, filter.field, filter.text)
  return ANY_COLUMN.some((field) => fold(valueOf(track, field)).includes(filter.text))
}

/** The indices of the tracks `input` shows, in view order; `null` when there is no filter (every track shows). */
export function shownTrackIndices(tracks: readonly TrackRow[], input: string | null): number[] | null {
  const filter = input === null ? null : parseTrackFilter(input)
  if (filter === null) return null
  return tracks.flatMap((track, index) => (trackMatches(track, filter) ? [index] : []))
}
