/** Tier 3 a11y tests for `TrackFilterBar.svelte`: empty, narrowing the list, and hiding selected tracks. */
import { afterEach, describe, it } from 'vitest'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import TrackFilterBar from './TrackFilterBar.svelte'
import { mountInto } from './tag-cell.test-helper'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

async function mountBar(text: string, shownCount: number, hiddenCount: number): Promise<HTMLElement> {
  const mounted = await mountInto(TrackFilterBar, {
    text,
    shownCount,
    total: 50,
    hiddenCount,
    onChange: () => {},
    onClose: () => {},
    onLeave: () => {},
  })
  cleanup = mounted.cleanup
  return mounted.root
}

describe('TrackFilterBar a11y', () => {
  it('an empty filter has no violations', async () => {
    await expectNoA11yViolations(await mountBar('', 50, 0))
  })

  it('a filter narrowing the list has no violations', async () => {
    await expectNoA11yViolations(await mountBar('disc: 2', 12, 0))
  })

  it('a filter hiding selected tracks has no violations', async () => {
    await expectNoA11yViolations(await mountBar('kapitola', 0, 3))
  })
})
