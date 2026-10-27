/** The track list: inline title and number edits go out as `SetOne` by path, and a single file has no # column. */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { tick } from 'svelte'
import type { TrackRow } from '#lib/ipc/bindings.js'
import type { TagEdit } from '#lib/tauri-commands/index.js'
import TrackList from './TrackList.svelte'
import { byId, leave, mountInto, press, typeInto } from './tag-cell.test-helper'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

function row(over: Partial<TrackRow> & { path: string }): TrackRow {
  return {
    fileName: over.path.split('/').pop() ?? over.path,
    track: null,
    title: null,
    durationMs: null,
    changed: false,
    readError: null,
    ...over,
  }
}

const TRACKS: TrackRow[] = [
  row({ path: '/b/02.mp3', track: '2', title: 'Druhá', durationMs: 61_000 }),
  row({ path: '/b/01.mp3', track: '1', title: 'První', durationMs: 3_725_000 }),
]

async function mountList(tracks: TrackRow[] = TRACKS, editable = true, checked: string[] = []) {
  const onEdit = vi.fn((_edits: TagEdit[]) => {})
  const onToggle = vi.fn((_path: string) => {})
  const onToggleAll = vi.fn((_on: boolean) => {})
  const mounted = await mountInto(TrackList, {
    tracks,
    editable,
    selected: new Set(checked),
    onEdit,
    onToggle,
    onToggleAll,
  })
  cleanup = mounted.cleanup
  return { root: mounted.root, onEdit, onToggle, onToggleAll }
}

function box(root: HTMLElement, label: string): HTMLInputElement {
  const found = root.querySelector<HTMLInputElement>(`input[aria-label="${label}"]`)
  if (!found) throw new Error(`no checkbox ${label}`)
  return found
}

describe('TrackList', () => {
  it('shows number, title, length and file name in the session order', async () => {
    const { root } = await mountList()
    const heads = [...root.querySelectorAll('th')].map((th) => th.textContent.trim())
    expect(heads).toEqual(['', '#', 'Title', 'Length', 'File name'])
    const rows = [...root.querySelectorAll('tbody tr')]
    expect(rows.map((r) => r.querySelector('.file')?.textContent.trim())).toEqual(['02.mp3', '01.mp3'])
    expect(rows.map((r) => r.querySelector('.length')?.textContent.trim())).toEqual(['1:01', '1:02:05'])
  })

  it('track_title_inline_edit_sends_set_one', async () => {
    const { root, onEdit } = await mountList()
    const title = byId<HTMLInputElement>(root, 'tag-track-title-1')
    expect(title.value).toBe('První')
    await typeInto(title, 'Kapitola první')
    await press(title, 'Enter')
    expect(onEdit).toHaveBeenCalledTimes(1)
    expect(onEdit).toHaveBeenCalledWith([
      { kind: 'setOne', path: '/b/01.mp3', field: 'title', values: ['Kapitola první'] },
    ])
  })

  it('a track number edits by path too, and an emptied title removes it', async () => {
    const { root, onEdit } = await mountList()
    await typeInto(byId<HTMLInputElement>(root, 'tag-track-number-0'), '5')
    await leave(byId(root, 'tag-track-number-0'))
    await typeInto(byId<HTMLInputElement>(root, 'tag-track-title-0'), '')
    await leave(byId(root, 'tag-track-title-0'))
    expect(onEdit.mock.calls.map((c) => c[0])).toEqual([
      [{ kind: 'setOne', path: '/b/02.mp3', field: 'track', values: ['5'] }],
      [{ kind: 'setOne', path: '/b/02.mp3', field: 'title', values: [] }],
    ])
  })

  it('untouched cells send nothing', async () => {
    const { root, onEdit } = await mountList()
    const title = byId<HTMLInputElement>(root, 'tag-track-title-0')
    title.focus()
    await press(title, 'Enter')
    await leave(title)
    expect(onEdit).not.toHaveBeenCalled()
  })

  it('single_file_hides_track_column', async () => {
    const { root } = await mountList([row({ path: '/b/kniha.m4b', title: null, durationMs: 7_200_000 })])
    const heads = [...root.querySelectorAll('th')].map((th) => th.textContent.trim())
    expect(heads).toEqual(['', 'Title', 'Length', 'File name'])
    expect(root.querySelector('[id^="tag-track-number-"]')).toBeNull()
    expect(byId<HTMLInputElement>(root, 'tag-track-title-0').value).toBe('')
  })

  it('read_only_disables_inputs', async () => {
    const { root } = await mountList(TRACKS, false)
    for (const box of root.querySelectorAll<HTMLInputElement>('input')) expect(box.disabled).toBe(true)
  })

  it('an unreadable file is greyed, locked and says why', async () => {
    const { root } = await mountList([...TRACKS, row({ path: '/b/bad.mp3', readError: 'unreadable' })])
    expect(byId<HTMLInputElement>(root, 'tag-track-title-2').disabled).toBe(true)
    expect(byId<HTMLInputElement>(root, 'tag-track-title-0').disabled).toBe(false)
    expect(root.textContent).toContain('Can’t read the tags of this file')
  })

  it('the checkbox column comes first and each box is named by its file', async () => {
    const { root } = await mountList()
    const first = root.querySelector('tbody tr td:first-child input')
    expect(first?.getAttribute('aria-label')).toBe('Select 02.mp3')
    expect(box(root, 'Select 01.mp3').checked).toBe(false)
  })

  it('checking a track reports its path, and a checked track shows checked', async () => {
    const { root, onToggle } = await mountList(TRACKS, true, ['/b/01.mp3'])
    expect(box(root, 'Select 01.mp3').checked).toBe(true)
    expect(box(root, 'Select 02.mp3').checked).toBe(false)
    box(root, 'Select 02.mp3').click()
    await tick()
    expect(onToggle).toHaveBeenCalledWith('/b/02.mp3')
  })

  it('the header box is mixed while some are checked, and checked once all are', async () => {
    const some = await mountList(TRACKS, true, ['/b/01.mp3'])
    expect(some.root.querySelector('[data-state="indeterminate"]')).not.toBeNull()
    some.root.remove()
    cleanup?.()
    const all = await mountList(TRACKS, true, ['/b/01.mp3', '/b/02.mp3'])
    expect(box(all.root, 'Select all tracks').checked).toBe(true)
    expect(all.root.querySelector('[data-state="indeterminate"]')).toBeNull()
  })

  it('the header box checks every track, or unchecks them all once all are checked', async () => {
    const none = await mountList()
    box(none.root, 'Select all tracks').click()
    await tick()
    expect(none.onToggleAll).toHaveBeenCalledWith(true)
    cleanup?.()
    const all = await mountList(TRACKS, true, ['/b/01.mp3', '/b/02.mp3'])
    box(all.root, 'Select all tracks').click()
    await tick()
    expect(all.onToggleAll).toHaveBeenCalledWith(false)
  })
})
