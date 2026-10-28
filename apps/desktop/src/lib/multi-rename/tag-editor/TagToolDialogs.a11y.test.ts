/** Tier 3 a11y tests for `TagToolDialogs.svelte`: each tool's dialog. */

import { afterEach, describe, it, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import TagToolDialogs from './TagToolDialogs.svelte'
import type { TagToolId } from './tag-tools'
import { asTags, fakeRunScoped, fakeTags } from './tag-tools-panel.test-helper'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

vi.mock('#lib/app-status-store.js', () => ({
  TAG_NAME_PATTERNS_MAX: 10,
  loadTagNamePatterns: vi.fn(() => Promise.resolve([])),
  saveTagNamePatterns: vi.fn(() => Promise.resolve()),
  loadTagNumbering: vi.fn(() => Promise.resolve(null)),
  saveTagNumbering: vi.fn(() => Promise.resolve()),
}))
vi.mock('#lib/tauri-commands/index.js', async (original) => ({
  ...(await original<object>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  document.body.innerHTML = ''
})

async function mountTool(open: TagToolId): Promise<HTMLElement> {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const dialogs = mount(TagToolDialogs, {
    target: root,
    props: {
      open,
      field: 'title',
      trackCount: 3,
      selectedCount: 1,
      extras: ['TXXX:MOOD'],
      state: asTags(fakeTags()),
      runScoped: fakeRunScoped(),
      scopeKey: 'k',
      onTool: () => {},
      onClear: () => {},
      onClose: () => {},
    },
  })
  cleanup = () => {
    void unmount(dialogs)
  }
  await tick()
  return document.body
}

describe('TagToolDialogs a11y', () => {
  it.each(['replace', 'format', 'fromName', 'case', 'cleanUp', 'extras', 'clearField'] as const)(
    '%s has no violations',
    async (open) => {
      await expectNoA11yViolations(await mountTool(open))
    },
  )
})
