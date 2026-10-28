/**
 * Tier 3 a11y tests for `TagsWindow.svelte`: while the tags are read, and with the tags of one track.
 */

import { afterEach, describe, it, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import TagsWindow from './TagsWindow.svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

const ipc = vi.hoisted(() => ({
  openTagEditor: vi.fn(),
  closeTagEditor: vi.fn(() => Promise.resolve()),
  releaseTagsSession: vi.fn(() => Promise.resolve()),
  closeMultiRename: vi.fn(() => Promise.resolve()),
  tagTrackAudioUrl: vi.fn(() => Promise.resolve({ ok: false, error: { kind: 'unknownTrack' } })),
  releaseTagTrackAudio: vi.fn(() => Promise.resolve()),
  onTagsSelectAll: vi.fn(() => Promise.resolve(() => {})),
  onTagsMenuKey: vi.fn(() => Promise.resolve(() => {})),
  onTagsDeselectAll: vi.fn(() => Promise.resolve(() => {})),
}))
vi.mock('#lib/tauri-commands/index.js', () => ipc)
vi.mock('#lib/child-window-close.js', () => ({ closeSelfWindow: vi.fn(() => Promise.resolve()) }))

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  document.body.innerHTML = ''
})

async function mountWindow(): Promise<HTMLElement> {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const window = mount(TagsWindow, { target: root, props: { sessionId: 'multi-rename-5' } })
  cleanup = () => {
    void unmount(window)
  }
  for (let i = 0; i < 3; i++) {
    await tick()
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
  return root
}

describe('TagsWindow a11y', () => {
  it('while reading has no violations', async () => {
    ipc.openTagEditor.mockReturnValue(new Promise(() => {}))
    await expectNoA11yViolations(await mountWindow())
  })

  it('with tags has no violations', async () => {
    ipc.openTagEditor.mockResolvedValue({
      ok: true,
      value: {
        editable: true,
        readOnlyReason: null,
        shared: [{ field: 'album', value: { same: ['Dracula'] }, choices: [], choicesMore: 0 }],
        tracks: [
          {
            path: '/m/a.mp3',
            fileName: 'a.mp3',
            track: '1',
            title: 'One',
            durationMs: 1000,
            changed: false,
            readError: null,
          },
        ],
        cover: { kind: 'empty' },
        extras: [],
        dirty: false,
        diacritics: [],
        badDates: 0,
      },
    })
    await expectNoA11yViolations(await mountWindow())
  })
})
