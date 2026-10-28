/** Tier 3 a11y tests for `CoverViewer.svelte`: a cover shown large, and one that can't be shown. */
import { afterEach, describe, it, vi } from 'vitest'
import { tick } from 'svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import CoverViewer from './CoverViewer.svelte'
import { mountInto } from './tag-cell.test-helper'

vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({ getFileSizeFormat: () => 'binary' }))
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

async function check(image: string | null): Promise<void> {
  const mounted = await mountInto(CoverViewer, {
    choice: { key: 'k1', mime: 'image/png', bytes: 5000, width: 10, height: 10, files: 2 },
    total: 3,
    image,
    editable: true,
    onUseForAll: () => {},
    onClose: () => {},
  })
  cleanup = mounted.cleanup
  await tick()
  await expectNoA11yViolations(document.body)
}

describe('CoverViewer a11y', () => {
  it('a cover shown large has no violations', async () => {
    await check('data:image/png;base64,iVBORw0KGgo=')
  })

  it('a cover that can’t be shown has no violations', async () => {
    await check(null)
  })
})
