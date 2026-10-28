/** The Tags window's player: ⌥Space plays the Tracks pane's cursor row, ▶ toggles, "Open in foobar2000" takes the selection. */
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
})

function track(n: string, over: Partial<TagEditorView['tracks'][number]> = {}): TagEditorView['tracks'][number] {
  return {
    path: `/b/${n}.mp3`,
    fileName: `${n}.mp3`,
    track: n,
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
  sort: null,
}

async function mountTab() {
  tagTrackAudioUrl.mockImplementation(() =>
    Promise.resolve(
      urlCalls.fail
        ? { ok: false, error: { kind: 'unknownTrack' } }
        : { ok: true, value: `cmdr-media://localhost/tok${String(++urlCalls.minted)}` },
    ),
  )
  openTagsInPlayer.mockImplementation(() => Promise.resolve({ ok: true, value: null }))
  const state = {
    sessionId: 's1',
    view: VIEW,
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
  }
  const mounted = await mountInto(TagsContent, { state: asTagState(state) })
  cleanup = mounted.cleanup
  return { root: mounted.root, state, unmount: mounted.cleanup }
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

describe('Open in foobar2000', () => {
  const openButton = (root: HTMLElement) =>
    [...root.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Open in foobar2000')

  it('with nothing selected opens every readable track (the backend refuses an unreadable one)', async () => {
    const { root } = await mountTab()
    openButton(root)?.click()
    await settle()
    expect(openTagsInPlayer).toHaveBeenCalledWith('s1', ['/b/01.mp3', '/b/02.mp3'])
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
    expect(openTagsInPlayer).toHaveBeenCalledWith('s1', ['/b/01.mp3'])
  })

  it('with tracks selected opens those alone', async () => {
    const { root } = await mountTab()
    await toggleTrack(root, '/b/02.mp3')
    openButton(root)?.click()
    await settle()
    expect(openTagsInPlayer).toHaveBeenCalledWith('s1', ['/b/02.mp3'])
  })

  it('words a missing player', async () => {
    const { root } = await mountTab()
    openTagsInPlayer.mockImplementation(() => Promise.resolve({ ok: false, error: { kind: 'playerUnavailable' } }))
    openButton(root)?.click()
    await settle()
    expect(root.textContent).toContain('Couldn’t open the tracks in a player.')
  })
})
