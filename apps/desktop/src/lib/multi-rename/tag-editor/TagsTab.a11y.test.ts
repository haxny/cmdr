/**
 * Tier 3 a11y tests for `TagsTab.svelte`: while the tags are read, with the full tab (shared fields, tracks, cover), read-only, and with errors.
 */

import { afterEach, describe, it, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import TagsTab from './TagsTab.svelte'
import type { TagEditorState } from './tag-editor-state.svelte'
import type { TagEditorView } from '#lib/tauri-commands/index.js'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  document.body.innerHTML = ''
})

const VIEW: TagEditorView = {
  editable: true,
  readOnlyReason: null,
  shared: [
    { field: 'artist', value: { same: ['Karel Capek'] }, choices: [] },
    {
      field: 'album',
      value: 'multiple',
      choices: [
        { values: ['Book one'], files: 2 },
        { values: ['Book two'], files: 1 },
      ],
    },
    { field: 'comment', value: 'empty', choices: [] },
  ],
  tracks: [
    {
      path: '/b/01.mp3',
      fileName: '01.mp3',
      track: '1',
      title: 'Prvni',
      durationMs: 61_000,
      changed: false,
      readError: null,
    },
    {
      path: '/b/02.mp3',
      fileName: '02.mp3',
      track: '2',
      title: null,
      durationMs: null,
      changed: true,
      readError: 'unreadable',
    },
  ],
  cover: { kind: 'same', mime: 'image/jpeg', bytes: 2048, width: 600, height: 400, key: 'k' },
  extras: [],
  dirty: false,
  diacritics: [],
}

async function mountTab(over: Partial<TagEditorState>): Promise<HTMLElement> {
  const state = {
    view: null,
    error: null,
    busy: false,
    open: vi.fn(() => Promise.resolve()),
    edit: vi.fn(() => Promise.resolve()),
    coverCandidates: vi.fn(() => Promise.resolve({ ok: true, value: [] })),
    coverFromFile: vi.fn(() => Promise.resolve()),
    coverImage: vi.fn(() => Promise.resolve(null)),
    undoable: null,
    settled: vi.fn(() => Promise.resolve()),
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

  it('with tags has no violations', async () => {
    await expectNoA11yViolations(await mountTab({ view: VIEW }))
  })

  it('read-only has no violations', async () => {
    await expectNoA11yViolations(await mountTab({ view: { ...VIEW, editable: false, readOnlyReason: 'remoteVolume' } }))
  })

  it('with an edit error over the tags has no violations', async () => {
    await expectNoA11yViolations(await mountTab({ view: VIEW, error: { kind: 'coverTooLarge' } }))
  })

  it('with an error has no violations', async () => {
    await expectNoA11yViolations(await mountTab({ error: { kind: 'noSession' } }))
  })
})
