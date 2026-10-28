/** Tier 3 a11y tests for `WriteDialog.svelte`: the change list. */

import { afterEach, describe, it, vi } from 'vitest'
import WriteDialog from './WriteDialog.svelte'
import { asState, fakeState, settle } from './apply-panel.test-helper'
import { mountInto } from './tag-cell.test-helper'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

vi.mock('#lib/app-status-store.js', () => ({
  loadTagWriteKind: vi.fn(() => Promise.resolve(null)),
  saveTagWriteKind: vi.fn(() => Promise.resolve()),
}))
vi.mock('#lib/tauri-commands/index.js', async (original) => ({
  ...(await original<object>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  document.body.innerHTML = ''
})

describe('WriteDialog a11y', () => {
  it('with the change list has no violations', async () => {
    const mounted = await mountInto(WriteDialog, {
      tags: asState(fakeState()),
      mode: 'write' as const,
      onClose: () => {},
    })
    cleanup = mounted.cleanup
    await settle()
    await expectNoA11yViolations(document.body)
  })
})
