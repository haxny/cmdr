/**
 * The Album pane: one row per shared field, drawn with the file panes' row classes and row height, a cursor the
 * list keys move, and F2 / Enter editing the cursor row's value in place (Enter commits through `onEdit`, Esc
 * cancels). «multiple values» untouched sends nothing; Comment and Description edit multiline.
 */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { tick } from 'svelte'
import type { SharedRow } from '#lib/ipc/bindings.js'
import type { TagEdit } from '#lib/tauri-commands/index.js'
import AlbumPane from './AlbumPane.svelte'
import { mountInto, press, typeInto } from './tag-cell.test-helper'

vi.mock('#lib/settings/reactive-settings.svelte.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getRowHeight: () => 23,
  getStripedRows: () => true,
}))

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

const ROWS: SharedRow[] = [
  { field: 'artist', value: { same: ['Karel Capek'] }, choices: [], choicesMore: 0 },
  {
    field: 'album',
    value: 'multiple',
    choices: [
      { values: ['Book one'], files: 2 },
      { values: ['Book two'], files: 1 },
    ],
    choicesMore: 0,
  },
  { field: 'date', value: { same: ['2010-20-09'] }, choices: [], choicesMore: 0 },
  { field: 'comment', value: 'empty', choices: [], choicesMore: 0 },
]

interface Over {
  rows?: SharedRow[]
  editable?: boolean
  badDates?: number
  cursor?: number
  focused?: boolean
}

async function mountPane(over: Over = {}) {
  const onEdit = vi.fn((_edits: TagEdit[]) => Promise.resolve())
  const onCursorChange = vi.fn()
  const onSwitchPane = vi.fn()
  const mounted = await mountInto(AlbumPane, {
    rows: over.rows ?? ROWS,
    editable: over.editable ?? true,
    badDates: over.badDates ?? 0,
    focused: over.focused ?? true,
    cursor: over.cursor ?? 0,
    onCursorChange,
    onEdit,
    onSwitchPane,
  })
  cleanup = mounted.cleanup
  const list = mounted.root.querySelector<HTMLElement>('[role="listbox"]')
  if (!list) throw new Error('no listbox')
  return { root: mounted.root, list, onEdit, onCursorChange, onSwitchPane }
}

const rowsOf = (root: HTMLElement) => [...root.querySelectorAll<HTMLElement>('[role="option"]')]
const editorOf = (root: HTMLElement) => root.querySelector<HTMLInputElement | HTMLTextAreaElement>('.tag-value-editor')

describe('AlbumPane look', () => {
  it('rows carry the file-list classes and the row height from the density setting', async () => {
    const { root } = await mountPane({ cursor: 1 })
    expect(root.querySelector('.full-list-container.is-focused')).not.toBeNull()
    const rows = rowsOf(root)
    expect(rows).toHaveLength(4)
    for (const row of rows) {
      expect(row.classList.contains('file-entry')).toBe(true)
      expect(row.style.height).toBe('23px')
    }
    expect(rows[1].classList.contains('is-striped')).toBe(true)
    expect(rows[0].classList.contains('is-striped')).toBe(false)
    expect(rows[1].classList.contains('is-under-cursor')).toBe(true)
    expect(rows[1].getAttribute('aria-selected')).toBe('true')
  })

  it('columns are fixed except the value, which flexes, so nothing scrolls sideways', async () => {
    const { root } = await mountPane()
    const template = rowsOf(root)[0].style.gridTemplateColumns
    expect(template).toContain('minmax(0, 1fr)')
    expect(template).not.toMatch(/max-content|auto/)
    const value = rowsOf(root)[0].querySelector<HTMLElement>('.col-value')
    expect(value?.classList.contains('cut')).toBe(true)
  })

  it('a «multiple values» row shows the marker and every value', async () => {
    const { root } = await mountPane()
    expect(rowsOf(root)[1].textContent).toContain('‹multiple values› Book one; Book two')
  })

  it('an invalid date is red with its hint, on open', async () => {
    const { root } = await mountPane({ badDates: 2 })
    const date = rowsOf(root)[2]
    expect(date.classList.contains('is-invalid')).toBe(true)
    expect(date.getAttribute('aria-describedby')).toBe('tag-album-date-hint')
    expect(root.querySelector('#tag-album-date-hint')?.textContent).toContain('This date doesn')
  })
})

describe('AlbumPane keys', () => {
  it('arrows, End and Home move the cursor', async () => {
    const { list, onCursorChange } = await mountPane({ cursor: 1 })
    const down = await press(list, 'ArrowDown')
    expect(onCursorChange).toHaveBeenLastCalledWith(2)
    expect(down.defaultPrevented).toBe(true)
    await press(list, 'End')
    expect(onCursorChange).toHaveBeenLastCalledWith(3)
    await press(list, 'Home')
    expect(onCursorChange).toHaveBeenLastCalledWith(0)
  })

  it('Tab asks for the other pane', async () => {
    const { list, onSwitchPane } = await mountPane()
    const tab = await press(list, 'Tab')
    expect(onSwitchPane).toHaveBeenCalledTimes(1)
    expect(tab.defaultPrevented).toBe(true)
  })

  it('F2 opens the editor on the cursor row; Enter commits through onEdit and closes it', async () => {
    const { root, list, onEdit } = await mountPane()
    await press(list, 'F2')
    const editor = editorOf(root)
    if (!editor) throw new Error('no editor')
    expect(editor.value).toBe('Karel Capek')
    await typeInto(editor, 'Josef Capek')
    await press(editor, 'Enter')
    expect(onEdit).toHaveBeenCalledWith([{ kind: 'setAll', field: 'artist', values: ['Josef Capek'] }])
    await tick()
    expect(editorOf(root)).toBeNull()
  })

  it('Enter opens the editor too, and Esc cancels without sending and claims the key', async () => {
    const { root, list, onEdit } = await mountPane()
    await press(list, 'Enter')
    const editor = editorOf(root)
    if (!editor) throw new Error('no editor')
    await typeInto(editor, 'Changed')
    const esc = await press(editor, 'Escape')
    await tick()
    expect(esc.defaultPrevented).toBe(true)
    expect(onEdit).not.toHaveBeenCalled()
    expect(editorOf(root)).toBeNull()
  })

  it('an untouched «multiple values» row sends nothing on Enter', async () => {
    const { root, list, onEdit } = await mountPane({ cursor: 1 })
    await press(list, 'F2')
    const editor = editorOf(root)
    if (!editor) throw new Error('no editor')
    expect(editor.value).toBe('‹multiple values› Book one; Book two')
    await press(editor, 'Enter')
    expect(onEdit).not.toHaveBeenCalled()
  })

  it('Comment edits multiline: ⌥Enter adds a line, Enter commits', async () => {
    const { root, list, onEdit } = await mountPane({ cursor: 3 })
    await press(list, 'F2')
    const editor = editorOf(root)
    if (!(editor instanceof HTMLTextAreaElement)) throw new Error('no textarea')
    await typeInto(editor, 'Line one')
    editor.setSelectionRange(8, 8)
    const alt = await press(editor, 'Enter', { altKey: true })
    expect(alt.defaultPrevented).toBe(true)
    expect(editor.value).toBe('Line one\n')
    expect(onEdit).not.toHaveBeenCalled()
    await typeInto(editor, 'Line one\nLine two')
    await press(editor, 'Enter')
    expect(onEdit).toHaveBeenCalledWith([{ kind: 'setAll', field: 'comment', values: ['Line one\nLine two'] }])
  })

  it('read-only: F2 opens nothing', async () => {
    const { root, list } = await mountPane({ editable: false })
    await press(list, 'F2')
    expect(editorOf(root)).toBeNull()
  })

  it('Clear empties the cursor row’s field on every file', async () => {
    const { root, onEdit } = await mountPane()
    const clear = root.querySelector<HTMLButtonElement>('button[aria-label="Clear Artist for all files"]')
    clear?.click()
    expect(onEdit).toHaveBeenCalledWith([{ kind: 'setAll', field: 'artist', values: [] }])
  })
})

describe('AlbumPane row actions', () => {
  it('Values… on a «multiple values» cursor row sets the picked value on every file', async () => {
    const { root, onEdit } = await mountPane({ cursor: 1 })
    root.querySelector<HTMLButtonElement>('button[aria-label="Album values"]')?.click()
    await tick()
    // The picker's own list (a popover), not the pane's rows.
    const option = [...document.querySelectorAll<HTMLElement>('.choices [role="option"]')].at(1)
    expect(option?.textContent.trim()).toBe('Book two (1)')
    option?.click()
    await tick()
    expect(onEdit).toHaveBeenCalledWith([{ kind: 'setAll', field: 'album', values: ['Book two'] }])
  })

  it('a row with one value has no Values… button, and read-only locks Clear', async () => {
    const { root } = await mountPane({ editable: false })
    expect(root.querySelector('button[aria-label="Artist values"]')).toBeNull()
    expect(root.querySelector<HTMLButtonElement>('button[aria-label="Clear Artist for all files"]')?.disabled).toBe(
      true,
    )
  })
})
