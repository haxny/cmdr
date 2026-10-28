/**
 * The Tracks pane: ☐ ▶ # Title Length File rows drawn like a file pane, selection that works like files (Space,
 * Insert, ⇧↓, ⌘A, through `classifySelectionKey` + `createSelectionState`), F2 / ⇧F2 editing Title / # in place,
 * and ⌥Space playing the cursor row.
 */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'

// The registry reads `isMacOS()` at module load, so the UA is stubbed before the imports (⌘A, ⌥Space).
vi.hoisted(() => {
  vi.stubGlobal('navigator', { userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' })
})

vi.mock('#lib/settings/reactive-settings.svelte.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getRowHeight: () => 23,
  getStripedRows: () => true,
}))

import type { TrackRow } from '#lib/ipc/bindings.js'
import type { TagEdit } from '#lib/tauri-commands/index.js'
import TracksPane from './TracksPane.svelte'
import { createTrackSelection } from './track-selection.svelte'
import { mountInto, press, typeInto } from './tag-cell.test-helper'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

function track(n: string, over: Partial<TrackRow> = {}): TrackRow {
  return {
    path: `/b/${n}.mp3`,
    fileName: `${n}.mp3`,
    track: n,
    title: `T${n}`,
    durationMs: 61_000,
    changed: false,
    readError: null,
    ...over,
  }
}

const TRACKS = [track('01'), track('02'), track('03'), track('04', { readError: 'unreadable' })]

async function mountPane(over: { tracks?: TrackRow[]; editable?: boolean; cursor?: number } = {}) {
  const tracks = over.tracks ?? TRACKS
  const selection = createTrackSelection()
  selection.sync(tracks.map((t) => t.path))
  const onEdit = vi.fn((_edits: TagEdit[]) => Promise.resolve())
  const onCursorChange = vi.fn()
  const onSwitchPane = vi.fn()
  const onPlay = vi.fn()
  const mounted = await mountInto(TracksPane, {
    tracks,
    editable: over.editable ?? true,
    focused: true,
    cursor: over.cursor ?? 0,
    onCursorChange,
    selection,
    playing: null,
    onPlay,
    onEdit,
    onSwitchPane,
  })
  cleanup = mounted.cleanup
  const list = mounted.root.querySelector<HTMLElement>('[role="listbox"]')
  if (!list) throw new Error('no listbox')
  return { root: mounted.root, list, selection, onEdit, onCursorChange, onSwitchPane, onPlay }
}

const rowsOf = (root: HTMLElement) => [...root.querySelectorAll<HTMLElement>('[role="option"]')]
const editorOf = (root: HTMLElement) => root.querySelector<HTMLInputElement>('.tag-value-editor')

describe('TracksPane look', () => {
  it('rows carry the file-list classes, the setting’s row height, and the selection', async () => {
    const { root, list, selection } = await mountPane({ cursor: 2 })
    expect(list.getAttribute('aria-multiselectable')).toBe('true')
    selection.toggleAt(1)
    await tick()
    const rows = rowsOf(root)
    expect(rows).toHaveLength(4)
    for (const row of rows) {
      expect(row.classList.contains('file-entry')).toBe(true)
      expect(row.style.height).toBe('23px')
    }
    expect(rows[1].classList.contains('is-selected')).toBe(true)
    expect(rows[1].getAttribute('aria-selected')).toBe('true')
    expect(rows[1].classList.contains('is-striped')).toBe(true)
    expect(rows[2].classList.contains('is-under-cursor')).toBe(true)
  })

  it('the columns are ☐ ▶ # Title Length File; only Title flexes, the file name is cut with …', async () => {
    const { root } = await mountPane()
    const template = rowsOf(root)[0].style.gridTemplateColumns
    expect(template.match(/minmax\(0, 1fr\)/g)).toHaveLength(1)
    expect(template).not.toMatch(/max-content|auto/)
    const cells = [...rowsOf(root)[0].children].map((cell) => cell.getAttribute('data-col'))
    expect(cells).toEqual(['mark', 'play', 'number', 'title', 'length', 'file'])
    expect(rowsOf(root)[0].querySelector('[data-col="file"]')?.classList.contains('cut')).toBe(true)
  })

  it('a single-file book has no # column', async () => {
    const { root } = await mountPane({ tracks: [track('01')] })
    expect(rowsOf(root)[0].querySelector('[data-col="number"]')).toBeNull()
  })
})

describe('TracksPane keys', () => {
  it('arrows move the cursor, Tab asks for the other pane', async () => {
    const { list, onCursorChange, onSwitchPane } = await mountPane()
    await press(list, 'ArrowDown')
    expect(onCursorChange).toHaveBeenLastCalledWith(1)
    await press(list, 'Tab')
    expect(onSwitchPane).toHaveBeenCalledTimes(1)
  })

  it('Space toggles the cursor row, Insert toggles and moves down', async () => {
    const { list, selection, onCursorChange } = await mountPane({ cursor: 1 })
    const space = await press(list, ' ', { code: 'Space' })
    expect(space.defaultPrevented).toBe(true)
    expect(selection.scope()).toEqual({ kind: 'some', paths: ['/b/02.mp3'] })
    await press(list, 'Insert')
    expect(selection.scope()).toEqual({ kind: 'all' })
    expect(onCursorChange).toHaveBeenLastCalledWith(2)
  })

  it('⇧↓ extends the selection as the cursor moves', async () => {
    const { list, selection, onCursorChange } = await mountPane()
    await press(list, 'ArrowDown', { shiftKey: true })
    expect(onCursorChange).toHaveBeenLastCalledWith(1)
    expect(selection.scope()).toEqual({ kind: 'some', paths: ['/b/01.mp3'] })
  })

  it('⌘A selects every track', async () => {
    const { list, selection } = await mountPane()
    await press(list, 'a', { metaKey: true })
    expect(selection.count).toBe(4)
  })

  it('F2 edits the Title of the cursor row, Enter commits through onEdit', async () => {
    const { root, list, onEdit } = await mountPane({ cursor: 1 })
    await press(list, 'F2')
    const editor = editorOf(root)
    if (!editor) throw new Error('no editor')
    expect(editor.value).toBe('T02')
    await typeInto(editor, 'Konec')
    await press(editor, 'Enter')
    expect(onEdit).toHaveBeenCalledWith([{ kind: 'setOne', path: '/b/02.mp3', field: 'title', values: ['Konec'] }])
  })

  it('⇧F2 edits #, and Esc cancels', async () => {
    const { root, list, onEdit } = await mountPane()
    await press(list, 'F2', { shiftKey: true })
    const editor = editorOf(root)
    if (!editor) throw new Error('no editor')
    expect(editor.value).toBe('01')
    await typeInto(editor, '7')
    await press(editor, 'Escape')
    await tick()
    expect(onEdit).not.toHaveBeenCalled()
    expect(editorOf(root)).toBeNull()
  })

  it('an unreadable track opens no editor', async () => {
    const { root, list } = await mountPane({ cursor: 3 })
    await press(list, 'F2')
    expect(editorOf(root)).toBeNull()
  })

  it('⌥Space plays the cursor row and claims the key', async () => {
    const { list, onPlay } = await mountPane({ cursor: 2 })
    const event = await press(list, ' ', { code: 'Space', altKey: true })
    expect(onPlay).toHaveBeenCalledWith('/b/03.mp3')
    expect(event.defaultPrevented).toBe(true)
  })

  it('⌥Space on an unreadable track plays nothing', async () => {
    const { list, onPlay } = await mountPane({ cursor: 3 })
    await press(list, ' ', { code: 'Space', altKey: true })
    expect(onPlay).not.toHaveBeenCalled()
  })
})

describe('TracksPane mouse', () => {
  it('a click on ☐ toggles that track, a click on ▶ plays it', async () => {
    const { root, selection, onPlay } = await mountPane()
    rowsOf(root)[2]
      .querySelector<HTMLElement>('[data-col="mark"]')
      ?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    expect(selection.scope()).toEqual({ kind: 'some', paths: ['/b/03.mp3'] })
    rowsOf(root)[1]
      .querySelector<HTMLElement>('[data-col="play"]')
      ?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    expect(onPlay).toHaveBeenCalledWith('/b/02.mp3')
  })

  it('the header box selects every track, then none', async () => {
    const { root, selection } = await mountPane()
    const all = root.querySelector<HTMLInputElement>('input[aria-label="Select all tracks"]')
    all?.click()
    await tick()
    expect(selection.count).toBe(4)
    all?.click()
    await tick()
    expect(selection.count).toBe(0)
  })
})

describe('TracksPane order and edits', () => {
  it('after a reorder, F2 on the first row edits the new first file (edits are keyed by path)', async () => {
    const onEdit = vi.fn()
    const first = track('01')
    const second = track('02')
    const selection = createTrackSelection()
    const props = $state({
      tracks: [first, second],
      editable: true,
      focused: true,
      cursor: 0,
      onCursorChange: () => {},
      selection,
      playing: null,
      onPlay: () => {},
      onEdit,
      onSwitchPane: () => {},
    })
    const root = document.createElement('div')
    document.body.appendChild(root)
    const pane = mount(TracksPane, { target: root, props })
    cleanup = () => {
      void unmount(pane)
      root.remove()
    }
    await tick()
    props.tracks = [second, first]
    await tick()
    const list = root.querySelector<HTMLElement>('[role="listbox"]')
    if (!list) throw new Error('no listbox')
    await press(list, 'F2')
    const editor = editorOf(root)
    if (!editor) throw new Error('no editor')
    expect(editor.value).toBe('T02')
    await typeInto(editor, 'Beta')
    await press(editor, 'Enter')
    expect(onEdit).toHaveBeenCalledWith([{ kind: 'setOne', path: '/b/02.mp3', field: 'title', values: ['Beta'] }])
  })

  it('an emptied title removes it from that file', async () => {
    const { root, list, onEdit } = await mountPane()
    await press(list, 'F2')
    const editor = editorOf(root)
    if (!editor) throw new Error('no editor')
    await typeInto(editor, '')
    await press(editor, 'Enter')
    expect(onEdit).toHaveBeenCalledWith([{ kind: 'setOne', path: '/b/01.mp3', field: 'title', values: [] }])
  })
})

describe('TracksPane after a click', () => {
  function mouseDown(el: Element, init: MouseEventInit = {}): MouseEvent {
    const event = new MouseEvent('mousedown', { bubbles: true, cancelable: true, ...init })
    el.dispatchEvent(event)
    return event
  }

  it('a click on a row keeps focus on the list, so ↓, Space, and F2 act', async () => {
    const { root, list, selection, onCursorChange } = await mountPane()
    const row = rowsOf(root)[1]
    expect(row.hasAttribute('tabindex')).toBe(false)
    expect(mouseDown(row).defaultPrevented).toBe(true)
    expect(document.activeElement).toBe(list)
    expect(onCursorChange).toHaveBeenLastCalledWith(1)
    const target = document.activeElement as HTMLElement
    await press(target, 'ArrowDown')
    expect(onCursorChange).toHaveBeenCalledTimes(2)
    await press(target, ' ', { code: 'Space' })
    expect(selection.count).toBe(1)
    await press(target, 'F2')
    expect(editorOf(root)).not.toBeNull()
  })

  it('⌘-click on ☐ toggles the track once, not twice', async () => {
    const { root, selection } = await mountPane()
    const mark = rowsOf(root)[2].querySelector<HTMLElement>('[data-col="mark"]')
    if (!mark) throw new Error('no mark')
    mouseDown(mark, { metaKey: true })
    mark.dispatchEvent(new MouseEvent('click', { bubbles: true, metaKey: true }))
    expect(selection.scope()).toEqual({ kind: 'some', paths: ['/b/03.mp3'] })
  })

  it('⌘-click on the row elsewhere toggles it, as on files', async () => {
    const { root, selection } = await mountPane()
    mouseDown(rowsOf(root)[0], { metaKey: true })
    expect(selection.scope()).toEqual({ kind: 'some', paths: ['/b/01.mp3'] })
  })
})
