/** Which clean-up tools apply to which selection, and the exact tool each button sends. */
import { describe, expect, it } from 'vitest'
import { TAG_FIELDS } from './field-labels'
import { DEFAULT_CASE_FIELDS, DIACRITICS_FIELDS, caseTool, removeExtrasTool, showsRenumber } from './tag-tools'

describe('tag-tools', () => {
  it.each([
    [null, DEFAULT_CASE_FIELDS],
    ['album', ['album']],
    ['title', ['title']],
    ['comment', ['comment']],
  ] as const)('Case with %j focused changes %j', (focused, fields) => {
    expect(caseTool('title', focused)).toEqual({ kind: 'case', case: 'title', fields })
  })

  it('Case with nothing focused changes Title, Album, Artist, Album artist and Performer', () => {
    expect(DEFAULT_CASE_FIELDS).toEqual(['title', 'album', 'artist', 'albumArtist', 'performer'])
  })

  it('Remove diacritics covers every field except Comment and Description', () => {
    expect(DIACRITICS_FIELDS).toEqual(TAG_FIELDS.filter((f) => f !== 'comment' && f !== 'description'))
    expect(DIACRITICS_FIELDS).not.toContain('comment')
    expect(DIACRITICS_FIELDS).not.toContain('description')
    expect(DIACRITICS_FIELDS).toHaveLength(15)
  })

  it.each([
    [0, false],
    [1, false],
    [2, true],
    [30, true],
  ])('renumber_hidden_for_single_file: %i tracks shows Renumber: %j', (count, shown) => {
    expect(showsRenumber(count)).toBe(shown)
  })

  it('remove_extras_requires_explicit_choice: nothing checked sends nothing', () => {
    expect(removeExtrasTool([])).toBeNull()
    expect(removeExtrasTool(['TXXX:MOOD', 'COMM:eng', 'TXXX:MOOD'])).toEqual({
      kind: 'removeExtras',
      keys: ['COMM:eng', 'TXXX:MOOD'],
    })
  })
})
