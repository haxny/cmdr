/** The Tags window's player: ⌥Space plays the Tracks pane's cursor row, ▶ toggles, "Open in player" takes the selection. */
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { tick } from 'svelte'

// The registry reads `isMacOS()` at module load, so the UA is stubbed before the imports.
vi.hoisted(() => {
  vi.stubGlobal('navigator', { userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' })
})

const urlCalls = vi.hoisted(() => ({ fail: false, minted: 0 }))
const tagTrackAudioUrl = vi.hoisted(() => vi.fn())
const releaseTagTrackAudio = vi.hoisted(() => vi.fn(() => Promise.resolve()))
const openTagsInPlayer = vi.hoisted(() => vi.fn())
const tagPlayerChoices = vi.hoisted(() => vi.fn())
const chooseTagPlayer = vi.hoisted(() => vi.fn())
const settings = vi.hoisted(() => ({ player: 'default' }))
const readClipboardText = vi.hoisted(() => vi.fn(() => Promise.resolve('Pasted')))
const copyToClipboard = vi.hoisted(() => vi.fn(() => Promise.resolve()))

vi.mock('#lib/settings/index.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getSetting: (id: string) => (id === 'multiRename.tagEditor.player' ? settings.player : false),
  setSetting: (id: string, value: string) => {
    if (id === 'multiRename.tagEditor.player') settings.player = value
  },
}))

vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({
  getFileSizeFormat: () => 'binary',
  getRowHeight: () => 20,
  getStripedRows: () => true,
}))
vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  tagTrackAudioUrl,
  releaseTagTrackAudio,
  openTagsInPlayer,
  tagPlayerChoices,
  chooseTagPlayer,
  readClipboardText,
  copyToClipboard,
}))

import type { TagEditorView } from '#lib/tauri-commands/index.js'
import TagsContent from './TagsContent.svelte'
import type { TagEditorState } from './tag-editor-state.svelte'
import { mountInto, paneList, press, toggleTrack, trackRow } from './tag-cell.test-helper'

class FakeAudio {
  src = ''
  play = vi.fn(() => Promise.resolve())
  pause = vi.fn()
  load = vi.fn()
  removeAttribute = vi.fn()
  addEventListener = vi.fn()
  removeEventListener = vi.fn()
}

beforeAll(() => {
  vi.stubGlobal('Audio', FakeAudio)
})

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  vi.clearAllMocks()
  urlCalls.fail = false
  urlCalls.minted = 0
  settings.player = 'default'
})

function track(n: string, over: Partial<TagEditorView['tracks'][number]> = {}): TagEditorView['tracks'][number] {
  return {
    path: `/b/${n}.mp3`,
    fileName: `${n}.mp3`,
    track: n,
    disc: null,
    title: `T${n}`,
    durationMs: 1000,
    changed: false,
    readError: null,
    ...over,
  }
}

const VIEW: TagEditorView = {
  editable: true,
  readOnlyReason: null,
  shared: [{ field: 'album', value: { same: ['Book'] }, choices: [], choicesMore: 0 }],
  tracks: [track('01'), track('02'), track('03', { readError: 'unreadable' })],
  cover: { kind: 'empty' },
  extras: [],
  dirty: false,
  diacritics: [],
  badDates: 0,
  canUndoEdit: false,
  canRedoEdit: false,
  scopeCount: null,
  viewSeq: 0,
  sort: null,
}

async function mountTab(over: { view?: TagEditorView | null } = {}) {
  tagTrackAudioUrl.mockImplementation(() =>
    Promise.resolve(
      urlCalls.fail
        ? { ok: false, error: { kind: 'unknownTrack' } }
        : { ok: true, value: `cmdr-media://localhost/tok${String(++urlCalls.minted)}` },
    ),
  )
  openTagsInPlayer.mockImplementation(() => Promise.resolve({ ok: true, value: { fellBack: false } }))
  // As the backend answers: no tracks are loaded (so no apps) until the tags are read.
  tagPlayerChoices.mockImplementation(() =>
    Promise.resolve({
      ok: true,
      value:
        state.view === null
          ? []
          : [
              { bundleId: 'com.apple.Music', name: 'Music', isDefault: true },
              { bundleId: 'org.videolan.vlc', name: 'VLC', isDefault: false },
            ],
    }),
  )
  const state = $state({
    sessionId: 's1',
    view: over.view === undefined ? VIEW : over.view,
    error: null,
    busy: false,
    open: vi.fn(() => Promise.resolve()),
    edit: vi.fn(() => Promise.resolve()),
    coverCandidates: vi.fn(() => Promise.resolve({ ok: true, value: [] })),
    coverImage: vi.fn(() => Promise.resolve(null)),
    coverChoices: vi.fn(() => Promise.resolve([])),
    coverThumbnail: vi.fn(() => Promise.resolve(null)),
    undoable: null,
    settled: vi.fn(() => Promise.resolve()),
    tool: vi.fn(() => Promise.resolve()),
    writing: false,
    writeNow: vi.fn((options?: { onWriting?: () => void }) => {
      options?.onWriting?.()
      return Promise.resolve({ kind: 'done', written: 1, failed: 0, changedOnDisk: 0 })
    }),
  })
  const mounted = await mountInto(TagsContent, { state: asTagState(state) })
  cleanup = mounted.cleanup
  return { root: mounted.root, state, unmount: mounted.cleanup, content: mounted.instance as unknown as Content }
}

/** What the window (`TagsWindow`) calls on the content for the native menu's roads. */
interface Content {
  hasDialog: () => boolean
  runCommand: (id: 'tags.replace' | 'tags.filter') => boolean
  selectAllTracks: () => void
  clipboard: (action: 'cut' | 'copy' | 'paste') => Promise<void>
}

/** The mock stands in for the state; only what the window reads is there. */
function asTagState(mock: object): TagEditorState {
  return mock as TagEditorState
}

/** ⌥Space the way macOS sends it: the key is a non-breaking space, the code says Space. */
function altSpace(target: HTMLElement): KeyboardEvent {
  const event = new KeyboardEvent('keydown', {
    key: '\u00a0',
    code: 'Space',
    altKey: true,
    bubbles: true,
    cancelable: true,
  })
  target.dispatchEvent(event)
  return event
}

const settle = async () => {
  await new Promise((resolve) => setTimeout(resolve, 0))
  await tick()
}

/** A click on a track's ▶ cell. */
function clickPlay(root: HTMLElement, path: string): void {
  trackRow(root, path)
    .querySelector('[data-col="play"]')
    ?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
}

describe('Tags window player', () => {
  it('⌥Space in the Tracks pane plays the cursor row and claims the key, so no non-breaking space is typed', async () => {
    const { root } = await mountTab()
    const tracks = paneList(root, 'Tracks')
    await press(tracks, 'ArrowDown')
    const reached = vi.fn()
    document.addEventListener('keydown', reached)
    const event = altSpace(tracks)
    document.removeEventListener('keydown', reached)
    await settle()
    expect(tagTrackAudioUrl).toHaveBeenCalledWith('s1', '/b/02.mp3')
    expect(event.defaultPrevented).toBe(true)
    expect(reached).not.toHaveBeenCalled()
  })

  it('a second ⌥Space stops the track and releases its URL', async () => {
    const { root } = await mountTab()
    const tracks = paneList(root, 'Tracks')
    altSpace(tracks)
    await settle()
    expect(tagTrackAudioUrl).toHaveBeenCalledWith('s1', '/b/01.mp3')
    altSpace(tracks)
    await settle()
    expect(releaseTagTrackAudio).toHaveBeenCalledWith('cmdr-media://localhost/tok1')
  })

  it('⌥Space in the Album pane does nothing and leaves the key alone', async () => {
    const { root } = await mountTab()
    const event = altSpace(paneList(root, 'Album tags'))
    await settle()
    expect(tagTrackAudioUrl).not.toHaveBeenCalled()
    expect(event.defaultPrevented).toBe(false)
  })

  it('a click on ▶ plays the track and a second one stops it; the row says Playing meanwhile', async () => {
    const { root } = await mountTab()
    clickPlay(root, '/b/02.mp3')
    await settle()
    expect(tagTrackAudioUrl).toHaveBeenCalledWith('s1', '/b/02.mp3')
    expect(trackRow(root, '/b/02.mp3').textContent).toContain('Playing')
    clickPlay(root, '/b/02.mp3')
    await settle()
    expect(releaseTagTrackAudio).toHaveBeenCalledWith('cmdr-media://localhost/tok1')
    expect(trackRow(root, '/b/02.mp3').textContent).not.toContain('Playing')
  })

  it('an unreadable track has no ▶ and a click on its cell plays nothing', async () => {
    const { root } = await mountTab()
    expect(trackRow(root, '/b/03.mp3').querySelector('[data-col="play"]')?.textContent).toBe('')
    clickPlay(root, '/b/03.mp3')
    await settle()
    expect(tagTrackAudioUrl).not.toHaveBeenCalled()
  })

  it('a write at once (the bar’s Write, ⌘⏎) stops the track before the files are written', async () => {
    const { root, state } = await mountTab()
    clickPlay(root, '/b/01.mp3')
    await settle()
    const write = [...root.querySelectorAll<HTMLButtonElement>('[role="toolbar"] button')].find(
      (b) => b.querySelector('span')?.textContent === 'Write',
    )
    write?.click()
    await settle()
    expect(state.writeNow).toHaveBeenCalledTimes(1)
    expect(releaseTagTrackAudio).toHaveBeenCalledWith('cmdr-media://localhost/tok1')
    expect(trackRow(root, '/b/01.mp3').textContent).not.toContain('Playing')
  })

  it('unmounting (the window closes) stops the track and releases its URL', async () => {
    const { root, unmount } = await mountTab()
    clickPlay(root, '/b/01.mp3')
    await settle()
    unmount()
    cleanup = undefined
    expect(releaseTagTrackAudio).toHaveBeenCalledWith('cmdr-media://localhost/tok1')
  })

  it('a failed URL request words the failure and plays nothing', async () => {
    const { root } = await mountTab()
    urlCalls.fail = true
    clickPlay(root, '/b/01.mp3')
    await settle()
    expect(root.textContent).toContain('Couldn’t play that track.')
    expect(trackRow(root, '/b/01.mp3').textContent).not.toContain('Playing')
  })
})

describe('Open in player', () => {
  const openButton = (root: HTMLElement, name = 'Open in Music') =>
    [...root.querySelectorAll('button')].find((b) => b.textContent.trim() === name)
  const menuRow = (name: string) =>
    [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].find((r) => r.textContent.includes(name))

  it('says the default app’s name on the button', async () => {
    const { root } = await mountTab()
    expect(openButton(root)).toBeDefined()
  })

  it('with nothing selected opens every readable track in the default app (the backend refuses an unreadable one)', async () => {
    const { root } = await mountTab()
    openButton(root)?.click()
    await settle()
    expect(openTagsInPlayer).toHaveBeenCalledWith('s1', ['/b/01.mp3', '/b/02.mp3'], 'default')
  })

  it('with only an unreadable track selected sends nothing and says so', async () => {
    const { root } = await mountTab()
    await toggleTrack(root, '/b/03.mp3')
    openButton(root)?.click()
    await settle()
    expect(openTagsInPlayer).not.toHaveBeenCalled()
    expect(root.textContent).toContain('None of those tracks can be read')
  })

  it('with a readable and an unreadable track selected opens the readable one', async () => {
    const { root } = await mountTab()
    for (const n of ['01', '03']) await toggleTrack(root, `/b/${n}.mp3`)
    openButton(root)?.click()
    await settle()
    expect(openTagsInPlayer).toHaveBeenCalledWith('s1', ['/b/01.mp3'], 'default')
  })

  it('with tracks selected opens those alone', async () => {
    const { root } = await mountTab()
    await toggleTrack(root, '/b/02.mp3')
    openButton(root)?.click()
    await settle()
    expect(openTagsInPlayer).toHaveBeenCalledWith('s1', ['/b/02.mp3'], 'default')
  })

  it('words a missing player', async () => {
    const { root } = await mountTab()
    openTagsInPlayer.mockImplementation(() => Promise.resolve({ ok: false, error: { kind: 'playerUnavailable' } }))
    openButton(root)?.click()
    await settle()
    expect(root.textContent).toContain('Couldn’t open the tracks in a player.')
  })

  it('names the app once the tags are read: asked then, not at mount, when no track is loaded yet', async () => {
    const { root, state } = await mountTab({ view: null })
    await settle()
    expect([...root.querySelectorAll('button')].some((b) => b.textContent.trim() === 'Open in Music')).toBe(false)
    state.view = VIEW
    await settle()
    expect(openButton(root)).toBeDefined()
  })

  it('the arrow says it opens a menu', async () => {
    const { root } = await mountTab()
    const arrow = root.querySelector('button[aria-label="Choose the app to open the tracks in"]')
    expect(arrow?.getAttribute('aria-haspopup')).toBe('menu')
    expect(arrow?.getAttribute('aria-expanded')).toBe('false')
  })

  it('uses the remembered app and names it on the button', async () => {
    settings.player = 'org.videolan.vlc'
    const { root } = await mountTab()
    await settle()
    openButton(root, 'Open in VLC')?.click()
    await settle()
    expect(openTagsInPlayer).toHaveBeenCalledWith('s1', ['/b/01.mp3', '/b/02.mp3'], 'org.videolan.vlc')
  })

  it('says so when the chosen app is gone and the default opened instead', async () => {
    settings.player = 'gone.app'
    const { root } = await mountTab()
    openTagsInPlayer.mockImplementation(() => Promise.resolve({ ok: true, value: { fellBack: true } }))
    await settle()
    openButton(root)?.click()
    await settle()
    expect(root.textContent).toContain('isn’t installed any more')
  })

  it('the arrow lists the default app, the others and Choose…; a pick is remembered and opens', async () => {
    const { root } = await mountTab()
    root.querySelector<HTMLElement>('button[aria-label="Choose the app to open the tracks in"]')?.click()
    await settle()
    expect(menuRow('Music (default)')).toBeDefined()
    expect(menuRow('Choose…')).toBeDefined()
    menuRow('VLC')?.click()
    await settle()
    expect(settings.player).toBe('org.videolan.vlc')
    expect(openTagsInPlayer).toHaveBeenCalledWith('s1', ['/b/01.mp3', '/b/02.mp3'], 'org.videolan.vlc')
    expect(openButton(root, 'Open in VLC')).toBeDefined()
  })

  it('the open app menu holds back every native-menu road, as a dialog does', async () => {
    const { root, state, content } = await mountTab()
    paneList(root, 'Tracks').focus()
    await settle()
    expect(content.hasDialog()).toBe(false)
    root.querySelector<HTMLElement>('button[aria-label="Choose the app to open the tracks in"]')?.click()
    await settle()
    expect(menuRow('VLC')).toBeDefined()
    expect(content.hasDialog()).toBe(true)
    expect(content.runCommand('tags.replace')).toBe(false)
    expect(content.runCommand('tags.filter')).toBe(false)
    await content.clipboard('paste')
    await content.clipboard('cut')
    await content.clipboard('copy')
    expect(readClipboardText).not.toHaveBeenCalled()
    expect(copyToClipboard).not.toHaveBeenCalled()
    expect(state.edit).not.toHaveBeenCalled()
    expect(document.querySelector('[role="dialog"]')).toBeNull()
  })

  it('Choose… remembers the app the picker gave', async () => {
    chooseTagPlayer.mockResolvedValue({
      ok: true,
      value: { bundleId: 'com.foobar2000.mac', name: 'foobar2000', isDefault: false },
    })
    const { root } = await mountTab()
    root.querySelector<HTMLElement>('button[aria-label="Choose the app to open the tracks in"]')?.click()
    await settle()
    menuRow('Choose…')?.click()
    await settle()
    expect(settings.player).toBe('com.foobar2000.mac')
    expect(openTagsInPlayer).toHaveBeenCalledWith('s1', ['/b/01.mp3', '/b/02.mp3'], 'com.foobar2000.mac')
  })
})
