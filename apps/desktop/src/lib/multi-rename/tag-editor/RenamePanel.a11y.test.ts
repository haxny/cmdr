/**
 * Tier 3 a11y tests for `RenamePanel.svelte`: the Rename tabpanel beside the tab strip that labels and controls it.
 */

import { afterEach, describe, it } from 'vitest'
import { createRawSnippet, mount, tick, unmount } from 'svelte'
import RenamePanel from './RenamePanel.svelte'
import SheetTabs from './SheetTabs.svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  document.body.innerHTML = ''
})

describe('RenamePanel a11y', () => {
  it('with the tab strip has no violations', async () => {
    const root = document.createElement('div')
    document.body.appendChild(root)
    const tabs = mount(SheetTabs, { target: root, props: { active: 'rename', onSelect: () => {} } })
    const children = createRawSnippet(() => ({ render: () => '<p>Body</p>' }))
    const panel = mount(RenamePanel, { target: root, props: { children } })
    cleanup = () => {
      void unmount(tabs)
      void unmount(panel)
    }
    await tick()
    await expectNoA11yViolations(root)
  })
})
