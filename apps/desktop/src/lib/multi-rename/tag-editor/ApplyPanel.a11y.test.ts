/** Tier 3 a11y tests for `ApplyPanel.svelte`: at rest with a diacritics warning and Undo, the change list, and the would-drop choice. */

import { afterEach, describe, it } from 'vitest'
import ApplyPanel from './ApplyPanel.svelte'
import { mountInto } from './tag-cell.test-helper'
import { PANEL_VIEW, applyReport, asState, fakeState, press } from './apply-panel.test-helper'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  document.body.innerHTML = ''
})

async function mountPanel(fake = fakeState()): Promise<HTMLElement> {
  const mounted = await mountInto(ApplyPanel, { tags: asState(fake) })
  cleanup = mounted.cleanup
  return mounted.root
}

describe('ApplyPanel a11y', () => {
  it('at rest with a diacritics warning and Undo has no violations', async () => {
    const fake = { ...fakeState({ ...PANEL_VIEW, diacritics: ['album'] }), undoable: { applyIds: ['A1'], written: 2 } }
    await expectNoA11yViolations(await mountPanel(fake))
  })

  it('with the change list has no violations', async () => {
    const root = await mountPanel()
    await press(root, 'Write tags…')
    await expectNoA11yViolations(root)
  })

  it('with the would-drop choice has no violations', async () => {
    const root = await mountPanel(
      fakeState(PANEL_VIEW, [applyReport({ wouldDrop: [{ path: '/b/02.mp3', items: ['TSOP'] }] })]),
    )
    await press(root, 'Write tags…')
    await press(root, 'Write tags')
    await expectNoA11yViolations(root)
  })

  it('read-only has no violations', async () => {
    await expectNoA11yViolations(await mountPanel(fakeState({ ...PANEL_VIEW, editable: false })))
  })
})
