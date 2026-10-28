/** The Album pane's edits over the Tracks pane's selection: a `setAll` becomes a `setOne` per readable selected track. */
import { describe, expect, it } from 'vitest'
import type { TagEdit } from '#lib/tauri-commands/index.js'
import { scopedAlbumEdits } from './album-scope.svelte'

const SET_ALBUM: TagEdit = { kind: 'setAll', field: 'album', values: ['Book'] }

describe('scopedAlbumEdits', () => {
  it('keeps the edits as they are with nothing selected', () => {
    expect(scopedAlbumEdits([SET_ALBUM], null, new Set(['/a.mp3']))).toEqual([SET_ALBUM])
  })

  it('sets each selected readable track alone, leaving an unreadable one out', () => {
    expect(scopedAlbumEdits([SET_ALBUM], ['/a.mp3', '/bad.mp3', '/c.mp3'], new Set(['/a.mp3', '/c.mp3']))).toEqual([
      { kind: 'setOne', path: '/a.mp3', field: 'album', values: ['Book'] },
      { kind: 'setOne', path: '/c.mp3', field: 'album', values: ['Book'] },
    ])
  })

  it('passes an edit that already names a track through', () => {
    const one: TagEdit = { kind: 'setOne', path: '/a.mp3', field: 'title', values: ['T'] }
    expect(scopedAlbumEdits([one], ['/c.mp3'], new Set(['/a.mp3', '/c.mp3']))).toEqual([one])
  })
})
