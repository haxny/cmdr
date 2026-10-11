import { describe, it, expect, vi } from 'vitest'

import { asListingLookupError, onListingGone, throwListingLookupError } from './listing-gone'

describe('asListingLookupError', () => {
  it('reads back the typed refusal a listing read threw, and nothing else', () => {
    const caught = (() => {
      try {
        throwListingLookupError({ type: 'changed', listingId: 'abc' })
      } catch (error) {
        return error
      }
    })()
    expect(asListingLookupError(caught)).toEqual({ type: 'changed', listingId: 'abc' })
    // A look-alike shape or a plain error isn't one: classification is by type, never by fields or words.
    expect(asListingLookupError({ type: 'changed', listingId: 'abc' })).toBeNull()
    expect(asListingLookupError(new Error('changed'))).toBeNull()
  })
})

describe('throwListingLookupError', () => {
  it('tells every listener which listing is gone, then throws the typed refusal', () => {
    const heard = vi.fn()
    const unsubscribe = onListingGone(heard)

    expect(() => throwListingLookupError({ type: 'gone', listingId: 'abc' })).toThrow(/abc/)
    expect(heard).toHaveBeenCalledWith('abc')

    unsubscribe()
    expect(() => throwListingLookupError({ type: 'gone', listingId: 'def' })).toThrow()
    expect(heard).toHaveBeenCalledTimes(1)
  })
})
