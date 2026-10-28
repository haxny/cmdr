/**
 * Tier 3 a11y tests for `TagsButton.svelte`: the Multi-Rename sheet's "Tags…" button.
 */

import { afterEach, describe, it, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import TagsButton from './TagsButton.svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

vi.mock('./open-tags', () => ({ openTagsForSession: vi.fn(() => Promise.resolve()) }))

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  document.body.innerHTML = ''
})

describe('TagsButton a11y', () => {
  it('has no violations', async () => {
    const root = document.createElement('div')
    document.body.appendChild(root)
    const button = mount(TagsButton, { target: root, props: { sessionId: 'S' } })
    cleanup = () => {
      void unmount(button)
    }
    await tick()
    await expectNoA11yViolations(root)
  })
})
