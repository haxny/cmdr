/** Tier 3 a11y tests for `CoverPanel.svelte`: covers with counts, none, the open picker, a notice, and read-only. */
import { afterEach, describe, it, vi } from 'vitest'
import { tick } from 'svelte'
import type { CoverChoice, CoverSummary } from '#lib/ipc/bindings.js'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import CoverPanel from './CoverPanel.svelte'
import { mountInto } from './tag-cell.test-helper'

vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({ getFileSizeFormat: () => 'binary' }))

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

const FRONT: CoverChoice = { key: 'k1', mime: 'image/png', bytes: 5000, width: 10, height: 10, files: 2 }
const NONE: CoverChoice = { key: 'none', mime: '', bytes: 0, width: null, height: null, files: 1 }

const settle = async (): Promise<void> => {
  await tick()
  await new Promise((resolve) => setTimeout(resolve, 0))
  await tick()
}

async function mountPanel(cover: CoverSummary, choices: CoverChoice[], editable = true): Promise<HTMLElement> {
  const mounted = await mountInto(CoverPanel, {
    cover,
    editable,
    revision: 1,
    listChoices: () => Promise.resolve(choices),
    loadThumbnail: () => Promise.resolve('data:image/png;base64,iVBORw0KGgo='),
    listCandidates: () => Promise.resolve([{ fileName: 'cover.jpg', size: 2048 }]),
    onView: () => {},
    onPick: () => {},
    onEdit: () => {},
    onPaste: () => {},
    onDrop: () => {},
    subscribeDrop: () => Promise.resolve(() => {}),
  })
  cleanup = mounted.cleanup
  await settle()
  return mounted.root
}

describe('CoverPanel a11y', () => {
  it('covers with their counts, and the files without one, have no violations', async () => {
    await expectNoA11yViolations(await mountPanel({ kind: 'multiple' }, [FRONT, NONE]))
  })

  it('no cover has no violations', async () => {
    await expectNoA11yViolations(await mountPanel({ kind: 'empty' }, [{ ...NONE, files: 3 }]))
  })

  it('with the folder picker open has no violations', async () => {
    const root = await mountPanel({ kind: 'empty' }, [NONE])
    ;[...root.querySelectorAll('button')].find((b) => b.textContent.includes('From folder'))?.click()
    await settle()
    await expectNoA11yViolations(root)
  })

  it('with a paste notice has no violations', async () => {
    const root = await mountPanel({ kind: 'multiple' }, [FRONT, NONE])
    root.querySelector<HTMLElement>('section.cover')?.focus()
    const event = new Event('paste', { bubbles: true, cancelable: true })
    Object.defineProperty(event, 'clipboardData', { value: { items: [] } })
    root.querySelector('section.cover')?.dispatchEvent(event)
    await settle()
    await expectNoA11yViolations(root)
  })

  it('read-only has no violations', async () => {
    await expectNoA11yViolations(await mountPanel({ kind: 'multiple' }, [FRONT, NONE], false))
  })
})
