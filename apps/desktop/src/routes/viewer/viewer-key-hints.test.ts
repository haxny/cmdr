import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import { _setLocaleForTests } from '#lib/intl/locale.js'
import { textModeHint } from './viewer-key-hints'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

// The status bar's text-mode hint names the keys the user has, not the defaults (#441).
describe('textModeHint', () => {
  it('reads as before with the default keys', () => {
    expect(textModeHint({ wrap: 'W', tail: 'F', search: '⌘F' })).toBe('W wrap · F tail · ⌘F search')
  })

  it('names a rebound key', () => {
    expect(textModeHint({ wrap: '⌃W', tail: 'F', search: '⌘F' })).toBe('⌃W wrap · F tail · ⌘F search')
  })

  it('leaves out a key the user turned off', () => {
    expect(textModeHint({ wrap: undefined, tail: 'F', search: '⌘F' })).toBe('F tail · ⌘F search')
  })
})
