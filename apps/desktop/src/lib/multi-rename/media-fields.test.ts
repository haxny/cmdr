import { describe, it, expect } from 'vitest'
import type { MediaField, MediaFieldOverview } from '#lib/ipc/bindings.js'
import { MEDIA_FIELDS } from './media-token'
import { MEDIA_FIELD_LABELS, insertField, pickerGroups } from './media-fields'

const overview = (counts: Partial<Record<MediaField, number>>): MediaFieldOverview[] =>
  (Object.keys(MEDIA_FIELDS) as MediaField[]).map((field) => ({
    field,
    example: counts[field] ? `${field} value` : null,
    filesWithIt: counts[field] ?? 0,
  }))

/** Words a key as its own name, so the filter tests read on keys. */
const word = (key: string): string => key.split('.').at(-1) ?? key

describe('MEDIA_FIELD_LABELS', () => {
  it('names every field', () => {
    expect(Object.keys(MEDIA_FIELD_LABELS).sort()).toEqual(Object.keys(MEDIA_FIELDS).sort())
  })
})

describe('pickerGroups', () => {
  it('puts photos first by default, each field with its example or none', () => {
    const groups = pickerGroups(overview({ exifTaken: 2 }), '', word)
    expect(groups.map((g) => g.source)).toEqual(['exif', 'tag'])
    const taken = groups[0].fields[0]
    expect(taken).toMatchObject({ field: 'exifTaken', name: 'exif.taken', example: 'exifTaken value' })
    expect(groups[0].fields[1]).toMatchObject({ field: 'exifCamera', example: null })
    expect(groups[1].fields.map((f) => f.field)[0]).toBe('tagTitle')
  })

  it('puts the group more files have first', () => {
    const groups = pickerGroups(overview({ exifTaken: 1, tagArtist: 5, tagTitle: 5 }), '', word)
    expect(groups.map((g) => g.source)).toEqual(['tag', 'exif'])
  })

  it('filters on the label, the mask name, and the group heading, any case', () => {
    expect(pickerGroups(overview({}), 'ARTIST', word).flatMap((g) => g.fields.map((f) => f.field))).toEqual([
      'tagArtist',
      'tagAlbumArtist',
    ])
    expect(pickerGroups(overview({}), 'exif.iso', word).flatMap((g) => g.fields.map((f) => f.field))).toEqual([
      'exifIso',
    ])
    const byHeading = pickerGroups(overview({}), 'music', word)
    expect(byHeading.map((g) => g.source)).toEqual(['tag'])
    expect(byHeading[0].fields).toHaveLength(9)
  })

  it('leaves out a group nothing in it matches', () => {
    expect(pickerGroups(overview({}), 'zzz', word)).toEqual([])
  })
})

describe('insertField', () => {
  it('inserts the token at the caret', () => {
    expect(insertField({ mask: '[N]', caret: 0, token: '[=exif.taken:d]', after: null })).toEqual({
      mask: '[=exif.taken:d][N]',
      caret: 15,
    })
  })

  it('puts a space between consecutive inserts', () => {
    const first = insertField({ mask: '', caret: 0, token: '[=tag.artist]', after: null })
    const second = insertField({ mask: first.mask, caret: first.caret, token: '[=tag.title]', after: first })
    expect(second).toEqual({ mask: '[=tag.artist] [=tag.title]', caret: 26 })
  })

  it('adds no space once the mask or caret moved since the last insert', () => {
    const first = insertField({ mask: '', caret: 0, token: '[=tag.artist]', after: null })
    expect(insertField({ mask: `${first.mask}-`, caret: 14, token: '[=tag.title]', after: first }).mask).toBe(
      '[=tag.artist]-[=tag.title]',
    )
    expect(insertField({ mask: first.mask, caret: 0, token: '[=tag.title]', after: first }).mask).toBe(
      '[=tag.title][=tag.artist]',
    )
  })
})
