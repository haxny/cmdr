/** Tier 3 a11y tests for `NumberingDialog.svelte`: the track section alone, and with the disc section on. */

import { afterEach, describe, it, vi } from 'vitest'
import { tick } from 'svelte'
import NumberingDialog from './NumberingDialog.svelte'
import { mountInto } from './tag-cell.test-helper'
import { asTags, boxNamed, fakeRunScoped, fakeTags } from './tag-tools-panel.test-helper'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

vi.mock('#lib/app-status-store.js', () => ({
  loadTagNumbering: vi.fn(() => Promise.resolve(null)),
  saveTagNumbering: vi.fn(() => Promise.resolve()),
}))
vi.mock('#lib/tauri-commands/index.js', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  document.body.innerHTML = ''
})

async function settle(): Promise<void> {
  await tick()
  await new Promise((resolve) => setTimeout(resolve, 0))
  await tick()
}

async function mountDialog(): Promise<HTMLElement> {
  const mounted = await mountInto(NumberingDialog, {
    trackCount: 3,
    state: asTags(fakeTags()),
    runScoped: fakeRunScoped(),
    onClose: vi.fn(),
  })
  cleanup = mounted.cleanup
  await settle()
  return document.body
}

describe('NumberingDialog a11y', () => {
  it('open with the disc section off has no violations', async () => {
    await expectNoA11yViolations(await mountDialog())
  })

  it('with the disc section on has no violations', async () => {
    const root = await mountDialog()
    boxNamed(root, 'Disc number').click()
    await settle()
    await expectNoA11yViolations(root)
  })
})
