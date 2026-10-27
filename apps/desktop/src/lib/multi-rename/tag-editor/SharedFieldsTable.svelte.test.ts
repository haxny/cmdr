/**
 * The shared-fields table: a «multiple values» row the user doesn't change sends nothing (Review Focus 2), typing
 * sends `SetAll`, Esc reverts, Comment and Description are textareas, and a read-only session locks the boxes.
 */
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { SharedRow } from '#lib/ipc/bindings.js'
import SharedFieldsTable from './SharedFieldsTable.svelte'
import { byId, leave, mountInto, press, typeInto } from './tag-cell.test-helper'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

const ROWS: SharedRow[] = [
  { field: 'artist', value: { same: ['Karel Čapek'] } },
  { field: 'album', value: 'multiple' },
  { field: 'genre', value: { same: ['audiokniha', 'detektivka'] } },
  { field: 'comment', value: 'empty' },
  { field: 'description', value: 'empty' },
]

async function mountTable(over: { rows?: SharedRow[]; editable?: boolean } = {}) {
  const onEdit = vi.fn()
  const mounted = await mountInto(SharedFieldsTable, {
    rows: over.rows ?? ROWS,
    editable: over.editable ?? true,
    onEdit,
  })
  cleanup = mounted.cleanup
  return { root: mounted.root, onEdit }
}

describe('SharedFieldsTable', () => {
  it('multiple_values_row_untouched_sends_nothing', async () => {
    const { root, onEdit } = await mountTable()
    const album = byId<HTMLInputElement>(root, 'tag-field-album')
    expect(album.value).toBe('')
    expect(album.placeholder).toBe('‹multiple values›')

    // Focus and leave.
    album.focus()
    await leave(album)
    // Enter without typing, then leaving.
    album.focus()
    const enter = await press(album, 'Enter')
    await leave(album)
    // Typing and deleting again lands on the same blank.
    await typeInto(album, 'x')
    await typeInto(album, '')
    await leave(album)

    expect(onEdit).not.toHaveBeenCalled()
    expect(enter.defaultPrevented).toBe(true)
  })

  it('editing_shared_row_sends_set_all', async () => {
    const { root, onEdit } = await mountTable()
    const album = byId<HTMLInputElement>(root, 'tag-field-album')
    await typeInto(album, 'Dracula')
    await press(album, 'Enter')
    await leave(album)
    expect(onEdit).toHaveBeenCalledTimes(1)
    expect(onEdit).toHaveBeenCalledWith([{ kind: 'setAll', field: 'album', values: ['Dracula'] }])
  })

  it('a multi-value field splits on "; " and blur commits', async () => {
    const { root, onEdit } = await mountTable()
    const genre = byId<HTMLInputElement>(root, 'tag-field-genre')
    expect(genre.value).toBe('audiokniha; detektivka')
    await typeInto(genre, 'audiokniha; horor; ')
    await leave(genre)
    expect(onEdit).toHaveBeenCalledWith([{ kind: 'setAll', field: 'genre', values: ['audiokniha', 'horor'] }])
  })

  it('emptying a row with a value removes the field', async () => {
    const { root, onEdit } = await mountTable()
    await typeInto(byId<HTMLInputElement>(root, 'tag-field-artist'), '')
    await leave(byId(root, 'tag-field-artist'))
    expect(onEdit).toHaveBeenCalledWith([{ kind: 'setAll', field: 'artist', values: [] }])
  })

  it('Esc reverts the typed text and sends nothing', async () => {
    const { root, onEdit } = await mountTable()
    const artist = byId<HTMLInputElement>(root, 'tag-field-artist')
    await typeInto(artist, 'Someone else')
    const esc = await press(artist, 'Escape')
    expect(artist.value).toBe('Karel Čapek')
    expect(esc.defaultPrevented).toBe(true)
    await leave(artist)
    expect(onEdit).not.toHaveBeenCalled()
  })

  it('Esc on an untouched box is left to the sheet', async () => {
    const { root } = await mountTable()
    const esc = await press(byId(root, 'tag-field-artist'), 'Escape')
    expect(esc.defaultPrevented).toBe(false)
  })

  it('does not send the same text twice (Enter, then blur)', async () => {
    const { root, onEdit } = await mountTable()
    const album = byId<HTMLInputElement>(root, 'tag-field-album')
    await typeInto(album, 'Dracula')
    await press(album, 'Enter')
    await leave(album)
    expect(onEdit).toHaveBeenCalledTimes(1)
  })

  it('comment_and_description_are_textareas_with_diacritics_allowed', async () => {
    const { root, onEdit } = await mountTable()
    const comment = byId<HTMLTextAreaElement>(root, 'tag-field-comment')
    expect(comment.tagName).toBe('TEXTAREA')
    expect(byId(root, 'tag-field-description').tagName).toBe('TEXTAREA')
    expect(byId(root, 'tag-field-album').tagName).toBe('INPUT')

    await typeInto(comment, 'Čte: Jan Novák\nHudba: Žofie')
    // Plain Enter is a new line, not a commit.
    const enter = await press(comment, 'Enter')
    expect(enter.defaultPrevented).toBe(false)
    expect(onEdit).not.toHaveBeenCalled()
    await leave(comment)
    expect(onEdit).toHaveBeenCalledWith([
      { kind: 'setAll', field: 'comment', values: ['Čte: Jan Novák\nHudba: Žofie'] },
    ])
  })

  it('never fills a blank Title row from Album: it only edits the fields it is given', async () => {
    const { root, onEdit } = await mountTable()
    await typeInto(byId<HTMLInputElement>(root, 'tag-field-album'), 'Dracula')
    await leave(byId(root, 'tag-field-album'))
    expect(onEdit.mock.calls.flat(2).every((edit: { field: string }) => edit.field !== 'title')).toBe(true)
  })

  it('read_only_disables_inputs', async () => {
    const { root } = await mountTable({ editable: false })
    const boxes = root.querySelectorAll<HTMLInputElement | HTMLTextAreaElement>('input, textarea')
    expect(boxes.length).toBe(ROWS.length)
    for (const box of boxes) expect(box.disabled).toBe(true)
  })
})
