/** Tier 3 a11y tests for `CoverPanel.svelte`: a shared cover, none, multiple, the open picker, and locked. */
import { afterEach, describe, it, vi } from 'vitest'
import { tick } from 'svelte'
import type { CoverSummary } from '#lib/ipc/bindings.js'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import CoverPanel from './CoverPanel.svelte'
import { mountInto } from './tag-cell.test-helper'

vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({ getFileSizeFormat: () => 'binary' }))

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

async function mountPanel(cover: CoverSummary, editable = true): Promise<HTMLElement> {
  const mounted = await mountInto(CoverPanel, {
    cover,
    editable,
    listCandidates: () => Promise.resolve([{ fileName: 'cover.jpg', size: 2048 }]),
    onPick: () => {},
    onEdit: () => {},
  })
  cleanup = mounted.cleanup
  return mounted.root
}

describe('CoverPanel a11y', () => {
  it('a shared cover has no violations', async () => {
    await expectNoA11yViolations(
      await mountPanel({ kind: 'same', mime: 'image/png', bytes: 5000, width: 10, height: 10 }),
    )
  })

  it('no cover and multiple covers have no violations', async () => {
    await expectNoA11yViolations(await mountPanel({ kind: 'empty' }))
    cleanup?.()
    await expectNoA11yViolations(await mountPanel({ kind: 'multiple' }))
  })

  it('with the folder picker open has no violations', async () => {
    const root = await mountPanel({ kind: 'empty' })
    root.querySelector('button')?.click()
    await new Promise((resolve) => setTimeout(resolve, 0))
    await tick()
    await expectNoA11yViolations(root)
  })

  it('read-only has no violations', async () => {
    await expectNoA11yViolations(await mountPanel({ kind: 'multiple' }, false))
  })
})
