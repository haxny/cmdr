/** Tier 3 a11y tests for `ApplyOutcome.svelte`: a clean write, and one that left files in every list. */

import { afterEach, describe, it, vi } from 'vitest'
import ApplyOutcome from './ApplyOutcome.svelte'
import type { ApplyOutcome as Outcome } from './apply-outcome'
import { mountInto } from './tag-cell.test-helper'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  document.body.innerHTML = ''
})

const CLEAN: Outcome = {
  written: 3,
  failed: [],
  changedOnDisk: [],
  skipped: [],
  undoUnavailable: false,
  sideFilesWritten: [],
  sideFilesNotWritten: [],
}

async function mountOutcome(outcome: Outcome): Promise<HTMLElement> {
  const mounted = await mountInto(ApplyOutcome, { outcome, someUndoable: false, onReload: vi.fn() })
  cleanup = mounted.cleanup
  return mounted.root
}

describe('ApplyOutcome a11y', () => {
  it('a clean write has no violations', async () => {
    await expectNoA11yViolations(await mountOutcome(CLEAN))
  })

  it('a write with every list has no violations', async () => {
    await expectNoA11yViolations(
      await mountOutcome({
        written: 1,
        failed: [{ path: '/b/02.mp3', error: { type: 'notSupported' } }],
        changedOnDisk: ['/b/03.mp3'],
        skipped: [{ path: '/b/04.mp3', items: ['TSOP'] }],
        undoUnavailable: true,
        sideFilesWritten: ['/b/desc.txt'],
        sideFilesNotWritten: ['/b/reader.txt'],
      }),
    )
  })
})
