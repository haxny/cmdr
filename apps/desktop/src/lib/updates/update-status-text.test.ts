import { describe, expect, it } from 'vitest'
import { describeUpdateFailure, updateFailureOffersReport } from './update-status-text'

describe('an update that isn’t a genuine Cmdr release', () => {
  it('reads like its security notice and offers a report (#457)', () => {
    expect(describeUpdateFailure({ phase: 'notGenuine' })).toBe(
      'Cmdr didn’t install this update: it isn’t a genuine Cmdr release.',
    )
    expect(updateFailureOffersReport({ phase: 'notGenuine' })).toBe(true)
  })
})
