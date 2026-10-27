/**
 * Tier 3 a11y tests for `SheetTabs.svelte`: the Multi-Rename tab strip with Rename selected and with Tags selected.
 */

import { afterEach, describe, it } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import SheetTabs from './SheetTabs.svelte'
import type { SheetTab } from './sheet-tab'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  document.body.innerHTML = ''
})

async function mountTabs(active: SheetTab): Promise<HTMLElement> {
  const root = document.createElement('div')
  document.body.appendChild(root)
  // The selected tab controls its panel, which the sheet renders beside the strip.
  const panel = document.createElement('div')
  panel.id = `sheet-panel-${active}`
  panel.setAttribute('role', 'tabpanel')
  panel.setAttribute('aria-labelledby', `sheet-tab-${active}`)
  root.appendChild(panel)
  const tabs = mount(SheetTabs, { target: root, props: { active, onSelect: () => {} } })
  cleanup = () => {
    void unmount(tabs)
  }
  await tick()
  return root
}

describe('SheetTabs a11y', () => {
  it('with Rename selected has no violations', async () => {
    await expectNoA11yViolations(await mountTabs('rename'))
  })

  it('with Tags selected has no violations', async () => {
    await expectNoA11yViolations(await mountTabs('tags'))
  })
})
