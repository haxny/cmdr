/** Tier 3 a11y tests for `TagsFunctionKeyBar.svelte`: every command on, some off, and the Shift row. */

import { afterEach, describe, it } from 'vitest'
import { flushSync, mount, unmount } from 'svelte'
import TagsFunctionKeyBar from './TagsFunctionKeyBar.svelte'
import { TAGS_BAR_COMMANDS, type TagsBarCommandId } from './tags-function-keys'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  document.body.innerHTML = ''
})

function mountBar(enabled: ReadonlySet<TagsBarCommandId>): HTMLElement {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const bar = mount(TagsFunctionKeyBar, { target: root, props: { enabled, onCommand: () => {} } })
  flushSync()
  cleanup = () => {
    void unmount(bar)
  }
  return root
}

describe('TagsFunctionKeyBar a11y', () => {
  it('with every command available has no violations', async () => {
    await expectNoA11yViolations(mountBar(new Set(TAGS_BAR_COMMANDS)))
  })

  it('read-only (only Play and Open in player) has no violations', async () => {
    await expectNoA11yViolations(mountBar(new Set<TagsBarCommandId>(['tags.play', 'tags.openInPlayer'])))
  })

  it('with the Shift row showing has no violations', async () => {
    const root = mountBar(new Set(TAGS_BAR_COMMANDS))
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Shift' }))
    flushSync()
    await expectNoA11yViolations(root)
  })
})
