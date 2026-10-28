/**
 * The Tags window's opener: one window per session, labelled by it, and a second open of the same session raises the
 * window it has instead of making another.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

const tauri = vi.hoisted(() => ({
  existing: null as object | null,
  created: [] as { label: string; options: Record<string, unknown> }[],
  raiseChildWindow: vi.fn(() => Promise.resolve()),
}))

vi.mock('@tauri-apps/api/webviewWindow', () => ({
  // eslint-disable-next-line @typescript-eslint/no-extraneous-class -- a stand-in for Tauri's class: a static lookup and a constructor
  WebviewWindow: class {
    static getByLabel = vi.fn(() => Promise.resolve(tauri.existing))
    constructor(label: string, options: Record<string, unknown>) {
      tauri.created.push({ label, options })
    }
  },
}))
vi.mock('#lib/tauri-commands/index.js', () => ({ raiseChildWindow: tauri.raiseChildWindow }))
vi.mock('#lib/app-mode.js', () => ({
  decorateChildWindowTitle: (title: string) => title,
  isE2eRun: () => false,
  orderChildWindowToBackInE2e: () => Promise.resolve(),
}))
vi.mock('#lib/window-positioning.js', () => ({
  readMainRect: () => Promise.resolve(null),
  readMonitors: () => Promise.resolve([]),
  readSavedRect: () => Promise.resolve(null),
  resolveChildPosition: () => null,
}))

import { openTagsWindow, tagsWindowLabel, tagsWindowUrl } from './tags-window'

describe('openTagsWindow', () => {
  beforeEach(() => {
    tauri.existing = null
    tauri.created = []
    vi.clearAllMocks()
  })

  it('labels the window by its session and passes the session and folder in the URL', () => {
    expect(tagsWindowLabel('multi-rename-7')).toBe('tags-multi-rename-7')
    expect(tagsWindowUrl('multi-rename-7', 'Čapek & co')).toBe(
      '/tags?session=multi-rename-7&folder=%C4%8Capek%20%26%20co',
    )
  })

  it('opens a new window titled after the folder', async () => {
    await openTagsWindow('multi-rename-7', 'Dracula')

    expect(tauri.created).toHaveLength(1)
    expect(tauri.created[0].label).toBe('tags-multi-rename-7')
    expect(tauri.created[0].options.url).toBe('/tags?session=multi-rename-7&folder=Dracula')
    expect(tauri.created[0].options.title).toBe('Tags — Dracula')
    expect(tauri.raiseChildWindow).not.toHaveBeenCalled()
  })

  it('raises the window the session already has instead of opening another', async () => {
    tauri.existing = {}

    await openTagsWindow('multi-rename-7', 'Dracula')

    expect(tauri.created).toEqual([])
    expect(tauri.raiseChildWindow).toHaveBeenCalledWith('tags-multi-rename-7')
  })
})
