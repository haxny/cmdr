/** Tier 3 a11y test for `HistoryButton.svelte`: an icon-only button named by its label. */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import HistoryButton from './HistoryButton.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

describe('HistoryButton a11y', () => {
  it('has no violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(HistoryButton, { target, props: { onopen: vi.fn() } })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })
})
