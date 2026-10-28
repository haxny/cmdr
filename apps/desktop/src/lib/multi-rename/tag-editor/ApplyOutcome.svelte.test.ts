/** A write's outcome: the no-undo note says whether Undo still reaches the other files. */
import { afterEach, describe, expect, it, vi } from 'vitest'
import ApplyOutcome from './ApplyOutcome.svelte'
import type { ApplyOutcome as Outcome } from './apply-outcome'
import { mountInto } from './tag-cell.test-helper'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

const NO_UNDO: Outcome = {
  written: 2,
  failed: [],
  changedOnDisk: [],
  skipped: [],
  undoUnavailable: true,
  sideFilesWritten: [],
  sideFilesNotWritten: [],
}

async function textOf(someUndoable: boolean): Promise<string> {
  const mounted = await mountInto(ApplyOutcome, { outcome: NO_UNDO, someUndoable, onReload: vi.fn() })
  cleanup = mounted.cleanup
  return mounted.root.textContent
}

describe('ApplyOutcome', () => {
  it('a write that kept no old tags at all says it can’t be undone', async () => {
    expect(await textOf(false)).toContain('This write can’t be undone')
  })

  it('a write whose "Write anyway" part kept its old tags says Undo restores the others', async () => {
    const text = await textOf(true)
    expect(text).toContain('Some of these files can’t be undone')
    expect(text).not.toContain('This write can’t be undone')
  })
})
