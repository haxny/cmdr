/** Tier 3 a11y tests for `WouldDropDialog.svelte`: asking, and while the answer is written. */

import { afterEach, describe, it, vi } from 'vitest'
import WouldDropDialog from './WouldDropDialog.svelte'
import { mountInto } from './tag-cell.test-helper'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  document.body.innerHTML = ''
})

async function mountDialog(busy: boolean): Promise<HTMLElement> {
  const mounted = await mountInto(WouldDropDialog, {
    rows: [{ path: '/b/01.mp3', items: ['TSOP', 'COMM:ces'] }],
    busy,
    onWriteAnyway: vi.fn(),
    onSkip: vi.fn(),
  })
  cleanup = mounted.cleanup
  return mounted.root
}

describe('WouldDropDialog a11y', () => {
  it('asking has no violations', async () => {
    await expectNoA11yViolations(await mountDialog(false))
  })

  it('busy has no violations', async () => {
    await expectNoA11yViolations(await mountDialog(true))
  })
})
