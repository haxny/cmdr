/** The Tags tab's player: ⌥Space plays the focused row, the row buttons toggle, "Open in foobar2000" takes the scope. */
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

vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({ getFileSizeFormat: () => 'binary' }))
vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  tagTrackAudioUrl,
  releaseTagTrackAudio,
  openTagsInPlayer,
}))

import type { TagEditorView } from '#lib/tauri-commands/index.js'
import TagsTab from './TagsTab.svelte'
import type { TagEditorState } from './tag-editor-state.svelte'
import { byId, mountInto } from './tag-cell.test-helper'

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
  shared: [{ field: 'album', value: { same: ['Book'] }, choices: [] }],
  tracks: [track('01'), track('02'), track('03', { readError: 'unreadable' })],
  cover: { kind: 'empty' },
  extras: [],
  dirty: false,
  diacritics: [],
  badDates: 0,
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
    undoable: null,
    settled: vi.fn(() => Promise.resolve()),
    tool: vi.fn(() => Promise.resolve()),
  }
  const mounted = await mountInto(TagsTab, { state: state as unknown as TagEditorState })
  cleanup = mounted.cleanup
  return { root: mounted.root, state, unmount: mounted.cleanup }
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

describe('Tags tab player', () => {
  it('⌥Space in a track cell plays that row and claims the key, so no non-breaking space is typed', async () => {
    const { root } = await mountTab()
    const reached = vi.fn()
    document.addEventListener('keydown', reached)
    const event = altSpace(byId(root, 'tag-track-title-1'))
    document.removeEventListener('keydown', reached)
    await settle()
    expect(tagTrackAudioUrl).toHaveBeenCalledWith('s1', '/b/02.mp3')
    expect(event.defaultPrevented).toBe(true)
    expect(reached).not.toHaveBeenCalled()
  })

  it('⌥Space on a row checkbox plays that row, and a second press stops it and releases the URL', async () => {
    const { root } = await mountTab()
    const box = root.querySelector<HTMLInputElement>('input[aria-label="Select 01.mp3"]') as HTMLInputElement
    altSpace(box)
    await settle()
    expect(tagTrackAudioUrl).toHaveBeenCalledWith('s1', '/b/01.mp3')
    altSpace(box)
    await settle()
    expect(releaseTagTrackAudio).toHaveBeenCalledWith('cmdr-media://localhost/tok1')
  })

  it('⌥Space outside the track list does nothing and leaves the key alone', async () => {
    const { root } = await mountTab()
    const event = altSpace(byId(root, 'tag-field-album'))
    await settle()
    expect(tagTrackAudioUrl).not.toHaveBeenCalled()
    expect(event.defaultPrevented).toBe(false)
  })

  it('a row button plays and stops its track, named for its file', async () => {
    const { root } = await mountTab()
    const play = () => root.querySelector<HTMLButtonElement>('button[aria-label="Play 02.mp3"]')
    play()?.click()
    await settle()
    expect(tagTrackAudioUrl).toHaveBeenCalledWith('s1', '/b/02.mp3')
    const stop = root.querySelector<HTMLButtonElement>('button[aria-label="Stop 02.mp3"]')
    expect(stop?.getAttribute('aria-pressed')).toBe('true')
    stop?.click()
    await settle()
    expect(releaseTagTrackAudio).toHaveBeenCalledWith('cmdr-media://localhost/tok1')
    expect(play()?.getAttribute('aria-pressed')).toBe('false')
  })

  it('an unreadable track has a disabled play button', async () => {
    const { root } = await mountTab()
    expect(root.querySelector<HTMLButtonElement>('button[aria-label="Play 03.mp3"]')?.disabled).toBe(true)
  })

  it('unmounting the tab (the Rename tab shows, or the sheet closes) stops the track and releases its URL', async () => {
    const { root, unmount } = await mountTab()
    root.querySelector<HTMLButtonElement>('button[aria-label="Play 01.mp3"]')?.click()
    await settle()
    unmount()
    cleanup = undefined
    expect(releaseTagTrackAudio).toHaveBeenCalledWith('cmdr-media://localhost/tok1')
  })

  it('a failed URL request words the failure and plays nothing', async () => {
    const { root } = await mountTab()
    urlCalls.fail = true
    root.querySelector<HTMLButtonElement>('button[aria-label="Play 01.mp3"]')?.click()
    await settle()
    expect(root.textContent).toContain('Couldn’t play that track.')
    expect(root.querySelector('button[aria-label="Stop 01.mp3"]')).toBeNull()
  })
})

describe('Open in foobar2000', () => {
  const openButton = (root: HTMLElement) =>
    [...root.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Open in foobar2000')

  it('with nothing checked opens every readable track (the backend refuses an unreadable one)', async () => {
    const { root } = await mountTab()
    openButton(root)?.click()
    await settle()
    expect(openTagsInPlayer).toHaveBeenCalledWith('s1', ['/b/01.mp3', '/b/02.mp3'])
  })

  it('with only an unreadable track checked sends nothing and says so', async () => {
    const { root } = await mountTab()
    root.querySelector<HTMLInputElement>('input[aria-label="Select 03.mp3"]')?.click()
    await settle()
    openButton(root)?.click()
    await settle()
    expect(openTagsInPlayer).not.toHaveBeenCalled()
    expect(root.textContent).toContain('None of those tracks can be read')
  })

  it('with a readable and an unreadable track checked opens the readable one', async () => {
    const { root } = await mountTab()
    for (const n of ['01', '03']) root.querySelector<HTMLInputElement>(`input[aria-label="Select ${n}.mp3"]`)?.click()
    await settle()
    openButton(root)?.click()
    await settle()
    expect(openTagsInPlayer).toHaveBeenCalledWith('s1', ['/b/01.mp3'])
  })

  it('with tracks checked opens those alone', async () => {
    const { root } = await mountTab()
    root.querySelector<HTMLInputElement>('input[aria-label="Select 02.mp3"]')?.click()
    await settle()
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
