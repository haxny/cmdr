/**
 * Tier 3 a11y tests for `TagsTab.svelte`: the placeholder while the tags are read, with the track count, and with an error.
 */

import { afterEach, describe, it, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import TagsTab from './TagsTab.svelte'
import type { TagEditorState } from './tag-editor-state.svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  document.body.innerHTML = ''
})

async function mountTab(over: Partial<TagEditorState>): Promise<HTMLElement> {
  const state = {
    view: null,
    error: null,
    busy: false,
    open: vi.fn(() => Promise.resolve()),
    ...over,
  } as TagEditorState
  const root = document.createElement('div')
  document.body.appendChild(root)
  const tab = mount(TagsTab, { target: root, props: { state } })
  cleanup = () => {
    void unmount(tab)
  }
  await tick()
  return root
}

describe('TagsTab a11y', () => {
  it('while reading has no violations', async () => {
    await expectNoA11yViolations(await mountTab({}))
  })

  it('with the track count has no violations', async () => {
    await expectNoA11yViolations(await mountTab({ view: { tracks: [{}, {}] } as never }))
  })

  it('with an error has no violations', async () => {
    await expectNoA11yViolations(await mountTab({ error: { kind: 'noSession' } }))
  })
})
