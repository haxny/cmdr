/**
 * Tier 3 a11y tests for `PlaceholderChips.svelte`: the placeholder buttons and `media ▾`, closed and
 * with the field picker open.
 */

import { afterEach, describe, it } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import PlaceholderChips from './PlaceholderChips.svelte'
import type { MultiRenameState } from './multi-rename-state.svelte'
import { DEFAULT_SPEC } from './spec'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

async function settle(): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await tick()
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  document.body.innerHTML = ''
})

/** The few parts of the sheet's state the buttons read. */
const tool = {
  spec: { ...DEFAULT_SPEC },
  media: { read: 1, total: 2, done: false },
  update: () => {},
  mediaOverview: () =>
    Promise.resolve({
      progress: { read: 1, total: 2, done: false },
      fields: [{ field: 'exifTaken', example: '2026-07-14 09.05.30', filesWithIt: 1 }],
    }),
} as unknown as MultiRenameState

async function mountChips(): Promise<HTMLElement> {
  const root = document.createElement('div')
  root.setAttribute('role', 'dialog')
  root.setAttribute('aria-label', 'Multi-rename')
  document.body.appendChild(root)
  const mask = document.createElement('input')
  mask.setAttribute('aria-label', 'Name mask')
  root.appendChild(mask)
  const chips = mount(PlaceholderChips, {
    target: root,
    props: { tool, maskInput: mask, examples: new Map(), announce: () => {} },
  })
  cleanup = () => {
    void unmount(chips)
  }
  await settle()
  return root
}

describe('PlaceholderChips a11y', () => {
  it('the buttons have no violations', async () => {
    await expectNoA11yViolations(await mountChips())
  })

  it('with the field picker open has no violations', async () => {
    const root = await mountChips()
    root.querySelector<HTMLButtonElement>('button[aria-haspopup="dialog"]')?.click()
    await settle()
    if (!document.querySelector('.ui-popover [role="listbox"]')) throw new Error('the picker did not open')
    await expectNoA11yViolations(document.body)
  })
})
