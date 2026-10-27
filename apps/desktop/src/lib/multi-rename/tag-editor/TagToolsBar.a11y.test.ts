/** Tier 3 a11y tests for `TagToolsBar.svelte`: closed, with the Case choices open, with the extras checklist open, and locked. */

import { afterEach, describe, it, vi } from 'vitest'
import { tick } from 'svelte'
import TagToolsBar from './TagToolsBar.svelte'
import { mountInto } from './tag-cell.test-helper'
import { asTags, fakeRunScoped, fakeTags } from './tag-tools-panel.test-helper'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  document.body.innerHTML = ''
})

async function mountBar(disabled = false): Promise<HTMLElement> {
  const mounted = await mountInto(TagToolsBar, {
    trackCount: 3,
    extras: ['COMM:eng', 'TXXX:MOOD'],
    focusedField: null,
    disabled,
    onTool: vi.fn(),
    state: asTags(fakeTags()),
    runScoped: fakeRunScoped(),
  })
  cleanup = mounted.cleanup
  return mounted.root
}

async function open(root: HTMLElement, text: string): Promise<void> {
  ;[...root.querySelectorAll('button')].find((b) => b.textContent.trim() === text)?.click()
  await tick()
}

describe('TagToolsBar a11y', () => {
  it('closed has no violations', async () => {
    await expectNoA11yViolations(await mountBar())
  })

  it('with the Case choices open has no violations', async () => {
    const root = await mountBar()
    await open(root, 'Case')
    await expectNoA11yViolations(root)
  })

  it('with the extras checklist open has no violations', async () => {
    const root = await mountBar()
    await open(root, 'Remove extras…')
    await expectNoA11yViolations(root)
  })

  it('locked has no violations', async () => {
    await expectNoA11yViolations(await mountBar(true))
  })
})
