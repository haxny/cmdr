/** The track list follows the session's order: after a reorder, an edit of the first row goes to the new first file. */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import type { TrackRow } from '#lib/ipc/bindings.js'
import TrackList from './TrackList.svelte'
import { byId, leave, typeInto } from './tag-cell.test-helper'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  document.body.innerHTML = ''
})

function row(path: string, title: string): TrackRow {
  return {
    path,
    fileName: path.split('/').pop() ?? path,
    track: null,
    title,
    durationMs: null,
    changed: false,
    readError: null,
  }
}

describe('TrackList order', () => {
  it('an edit of index 0 after a reorder goes to the new first path', async () => {
    const onEdit = vi.fn()
    const first = row('/b/a.mp3', 'A')
    const second = row('/b/b.mp3', 'B')
    const props = $state({
      tracks: [first, second],
      editable: true,
      selected: new Set<string>(),
      onEdit,
      onToggle: () => {},
      onToggleAll: () => {},
      playing: null,
      onPlay: () => {},
    })
    const root = document.createElement('div')
    document.body.appendChild(root)
    const list = mount(TrackList, { target: root, props })
    cleanup = () => {
      void unmount(list)
    }
    await tick()

    props.tracks = [second, first]
    await tick()
    const title = byId<HTMLInputElement>(root, 'tag-track-title-0')
    expect(title.value).toBe('B')
    await typeInto(title, 'Beta')
    await leave(title)
    expect(onEdit).toHaveBeenCalledWith([{ kind: 'setOne', path: '/b/b.mp3', field: 'title', values: ['Beta'] }])
  })
})
