/** The numbering wizard's options: defaults, begin-at validation, and the field-by-field fallback on load. */
import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  DEFAULT_DISC,
  DEFAULT_NUMBERING,
  loadNumbering,
  normalizeNumbering,
  parseBegin,
  saveNumbering,
} from './numbering-options'

const stored = vi.hoisted((): { value: unknown } => ({ value: null }))
vi.mock('#lib/app-status-store.js', () => ({
  loadTagNumbering: vi.fn(() => Promise.resolve(stored.value)),
  saveTagNumbering: vi.fn((value: unknown) => {
    stored.value = value
    return Promise.resolve()
  }),
}))

beforeEach(() => {
  stored.value = null
})

describe('defaults', () => {
  it('match the plan', () => {
    expect(DEFAULT_NUMBERING).toEqual({ trackStart: 1, trackTotal: true, trackPad: true, disc: null })
    expect(DEFAULT_DISC).toEqual({
      start: 1,
      total: true,
      pad: false,
      nextOnFolder: true,
      nextOnField: null,
      resetTrack: false,
      resetDiscOnField: null,
    })
  })
})

describe('parseBegin', () => {
  it.each([
    ['0', 0],
    ['1', 1],
    ['12', 12],
    [' 7 ', 7],
  ])('accepts %j', (text, value) => {
    expect(parseBegin(text)).toBe(value)
  })

  it.each(['', '-1', '1.5', 'a', '1e3', '99999999999'])('rejects %j', (text) => {
    expect(parseBegin(text)).toBeNull()
  })
})

describe('normalizeNumbering', () => {
  it('falls back to the defaults for something that is not an object', () => {
    expect(normalizeNumbering(null)).toEqual(DEFAULT_NUMBERING)
    expect(normalizeNumbering('x')).toEqual(DEFAULT_NUMBERING)
    expect(normalizeNumbering([1])).toEqual(DEFAULT_NUMBERING)
  })

  it('falls back field by field and keeps the valid ones', () => {
    const result = normalizeNumbering({ trackStart: 'three', trackTotal: false, trackPad: 1, disc: 'on' })
    expect(result).toEqual({ trackStart: 1, trackTotal: false, trackPad: true, disc: null })
  })

  it('rejects a negative or fractional begin', () => {
    expect(normalizeNumbering({ trackStart: -2 }).trackStart).toBe(1)
    expect(normalizeNumbering({ trackStart: 2.5 }).trackStart).toBe(1)
    expect(normalizeNumbering({ trackStart: 0 }).trackStart).toBe(0)
  })

  it('validates the disc section field by field', () => {
    const result = normalizeNumbering({
      disc: {
        start: 3,
        total: 'yes',
        pad: true,
        nextOnFolder: false,
        nextOnField: 'nonsense',
        resetTrack: true,
        resetDiscOnField: 'album',
      },
    })
    expect(result.disc).toEqual({
      start: 3,
      total: true,
      pad: true,
      nextOnFolder: false,
      nextOnField: null,
      resetTrack: true,
      resetDiscOnField: 'album',
    })
  })
})

describe('load and save', () => {
  it('loads the defaults before anything is saved', async () => {
    expect(await loadNumbering()).toEqual(DEFAULT_NUMBERING)
  })

  it('reads back what was saved', async () => {
    const options = {
      trackStart: 5,
      trackTotal: false,
      trackPad: false,
      disc: { ...DEFAULT_DISC, nextOnField: 'album' as const },
    }
    await saveNumbering(options)
    expect(await loadNumbering()).toEqual(options)
  })
})
