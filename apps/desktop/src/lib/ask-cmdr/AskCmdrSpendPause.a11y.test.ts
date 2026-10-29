/**
 * Tier 3 a11y tests for `AskCmdrSpendPause.svelte`.
 *
 * The card a running Ask Cmdr answer shows when its estimated spend passed the user's threshold.
 * What this guards is the tinted card's contrast (the question and the smaller caveat under it),
 * the group being named by its question, and Keep going and Stop being real, named buttons.
 */

import { describe, it, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

import AskCmdrSpendPause from './AskCmdrSpendPause.svelte'

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('AskCmdrSpendPause a11y', () => {
  for (const costMicros of [2_140_000, 12_500_000]) {
    it(`a pause at ${String(costMicros)} micro-USD has no violations`, async () => {
      const target = document.createElement('div')
      document.body.appendChild(target)
      mount(AskCmdrSpendPause, { target, props: { costMicros, onKeepGoing: vi.fn(), onStop: vi.fn() } })
      await tick()
      await expectNoA11yViolations(target)
    })
  }
})
