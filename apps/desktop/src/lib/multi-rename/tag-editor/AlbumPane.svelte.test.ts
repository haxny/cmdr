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
import { installLayoutMock } from '#lib/test-layout.js'
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
  scope?: { count: number; total: number } | null
  cursor?: number
  focused?: boolean
}

async function mountPane(over: Over = {}) {
  const onEdit = vi.fn((_edits: TagEdit[]) => Promise.resolve())
  const onCursorChange = vi.fn()
  const onSwitchPane = vi.fn()
  const onClear = vi.fn()
  const mounted = await mountInto(AlbumPane, {
    rows: over.rows ?? ROWS,
    editable: over.editable ?? true,
    badDates: over.badDates ?? 0,
    scope: over.scope ?? null,
    focused: over.focused ?? true,
    cursor: over.cursor ?? 0,
    onCursorChange,
    onEdit,
    onClear,
    onSwitchPane,
  })
  cleanup = mounted.cleanup
  const list = mounted.root.querySelector<HTMLElement>('[role="listbox"]')
  if (!list) throw new Error('no listbox')
  return { root: mounted.root, list, onEdit, onClear, onCursorChange, onSwitchPane }
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

describe('AlbumPane scope heading', () => {
  it('says how many of the tracks the rows cover while some are selected, as the list’s description', async () => {
    const { root, list } = await mountPane({ scope: { count: 5, total: 42 } })
    const heading = root.querySelector('.tag-pane-header .scope')
    expect(heading?.textContent).toBe('5 of 42 tracks')
    expect(list.getAttribute('aria-describedby')).toBe(heading?.id)
  })

  it('names the editor and Clear for the selected tracks they set, not for all files', async () => {
    const { root, list } = await mountPane({ scope: { count: 5, total: 42 } })
    expect(root.querySelector('button[aria-label="Clear Artist for 5 of 42 tracks"]')).not.toBeNull()
    await press(list, 'F2')
    expect(editorOf(root)?.getAttribute('aria-label')).toBe('Artist for 5 of 42 tracks')
  })

  it('says nothing while the rows cover every track', async () => {
    const { root, list } = await mountPane()
    expect(root.querySelector('.tag-pane-header .scope')).toBeNull()
    expect(list.hasAttribute('aria-describedby')).toBe(false)
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

/** A real click's first half on `el`: the row handler runs, and the default (focus, text selection) is cancelable. */
function mouseDown(el: Element): MouseEvent {
  const event = new MouseEvent('mousedown', { bubbles: true, cancelable: true })
  el.dispatchEvent(event)
  return event
}

describe('AlbumPane mouse', () => {
  it('after a click on a row the keys still work: the list keeps focus, ↓ and F2 act', async () => {
    const { root, list, onCursorChange } = await mountPane()
    const row = rowsOf(root)[1]
    expect(row.hasAttribute('tabindex')).toBe(false)
    const down = mouseDown(row)
    expect(down.defaultPrevented).toBe(true)
    expect(document.activeElement).toBe(list)
    expect(onCursorChange).toHaveBeenLastCalledWith(1)
    const target = document.activeElement as HTMLElement
    await press(target, 'ArrowDown')
    expect(onCursorChange).toHaveBeenCalledTimes(2)
    await press(target, 'F2')
    expect(editorOf(root)).not.toBeNull()
  })

  it('a double-click on a row opens its editor', async () => {
    const { root } = await mountPane()
    rowsOf(root)[0].dispatchEvent(new MouseEvent('dblclick', { bubbles: true }))
    await tick()
    expect(editorOf(root)?.value).toBe('Karel Capek')
  })
})

describe('AlbumPane date flag', () => {
  it('the red Date clears once a reload says every date can be written', async () => {
    const props = $state({
      rows: ROWS,
      editable: true,
      badDates: 2,
      focused: true,
      cursor: 0,
      onCursorChange: () => {},
      onEdit: () => {},
      onClear: () => {},
      onSwitchPane: () => {},
    })
    const mounted = await mountInto(AlbumPane, props)
    cleanup = mounted.cleanup
    expect(rowsOf(mounted.root)[2].classList.contains('is-invalid')).toBe(true)
    props.badDates = 0
    props.rows = ROWS.map((row) => (row.field === 'date' ? { ...row, value: { same: ['2010'] } } : row))
    await tick()
    expect(rowsOf(mounted.root)[2].classList.contains('is-invalid')).toBe(false)
    expect(mounted.root.querySelector('#tag-album-date-hint')).toBeNull()
  })
})

describe('AlbumPane editor placement and refusals', () => {
  it('the open editor follows its cell when the pane resizes', async () => {
    const layout = installLayoutMock({ '.tag-pane .full-list': { clientHeight: 300, clientWidth: 400 } })
    const { root, list } = await mountPane()
    const cell = rowsOf(root)[0].querySelector<HTMLElement>('.col-value')
    if (!cell) throw new Error('no value cell')
    let width = 200
    cell.getBoundingClientRect = () => DOMRect.fromRect({ x: 120, y: 0, width, height: 20 })
    await press(list, 'F2')
    const frame = () => root.querySelector<HTMLElement>('.tag-value-frame')
    expect(frame()?.style.width).toBe('200px')
    width = 340
    layout.resize('.tag-pane .full-list', { clientWidth: 560 })
    await tick()
    expect(frame()?.style.width).toBe('340px')
    expect(frame()?.style.left).toBe('120px')
  })

  it('Enter with the «multiple values» marker still in the text keeps the editor open and says why', async () => {
    const { root, list, onEdit } = await mountPane({ cursor: 1 })
    await press(list, 'F2')
    const editor = editorOf(root)
    if (!editor) throw new Error('no editor')
    await typeInto(editor, '‹multiple values› Book one; Book three')
    await press(editor, 'Enter')
    expect(onEdit).not.toHaveBeenCalled()
    expect(editorOf(root)).toBe(editor)
    const hint = root.querySelector('.tag-value-hint')
    expect(hint?.textContent).toContain('Delete ‹multiple values› to set what’s left on every file.')
    expect(editor.getAttribute('aria-describedby')).toBe(hint?.id)
    await typeInto(editor, 'Book three')
    await press(editor, 'Enter')
    expect(onEdit).toHaveBeenCalledWith([{ kind: 'setAll', field: 'album', values: ['Book three'] }])
  })
})

describe('AlbumPane type-to-edit and Delete', () => {
  it('typing a letter opens the editor holding exactly that letter, caret after it', async () => {
    const { root, list } = await mountPane()
    const key = await press(list, 'H')
    const editor = editorOf(root)
    if (!editor) throw new Error('no editor')
    expect(editor.value).toBe('H')
    expect(editor.selectionStart).toBe(1)
    expect(editor.selectionEnd).toBe(1)
    expect(key.defaultPrevented).toBe(true)
  })

  it('Space, ⌘-letter, ⌥-letter, composition and a dialog open nothing', async () => {
    const { root, list } = await mountPane()
    await press(list, ' ')
    await press(list, 'a', { metaKey: true })
    await press(list, 'a', { altKey: true })
    await press(list, 'a', { isComposing: true })
    expect(editorOf(root)).toBeNull()
    const dialog = document.createElement('div')
    dialog.setAttribute('role', 'dialog')
    document.body.appendChild(dialog)
    try {
      await press(list, 'x')
      await press(list, 'Delete')
    } finally {
      dialog.remove()
    }
    expect(editorOf(root)).toBeNull()
  })

  it('read-only: typing and Delete do nothing', async () => {
    const { root, list, onClear } = await mountPane({ editable: false })
    await press(list, 'H')
    await press(list, 'Delete')
    expect(editorOf(root)).toBeNull()
    expect(onClear).not.toHaveBeenCalled()
  })

  it('Delete and Backspace ask to empty the cursor row’s field, but not an already empty one', async () => {
    const { list, onClear } = await mountPane()
    await press(list, 'Delete')
    expect(onClear).toHaveBeenLastCalledWith({ field: 'artist', path: null })
    await press(list, 'Backspace')
    expect(onClear).toHaveBeenCalledTimes(2)
    const empty = await mountPane({ cursor: 3 })
    await press(empty.list, 'Delete')
    expect(empty.onClear).not.toHaveBeenCalled()
  })
})

describe('AlbumPane composition, Shift and multiple values', () => {
  it('a dead key opens an empty editor without claiming the key; Shift characters seed it', async () => {
    const { root, list, onEdit } = await mountPane()
    const dead = await press(list, 'Dead')
    expect(dead.defaultPrevented).toBe(false)
    const box = editorOf(root)
    expect(box?.value).toBe('')
    if (box) await press(box, 'Enter')
    expect(onEdit).not.toHaveBeenCalled()
    await press(list, 'Q', { shiftKey: true })
    expect(editorOf(root)?.value).toBe('Q')
  })

  it('typing on a «multiple values» row then Enter sets that value on the scope', async () => {
    const { root, list, onEdit } = await mountPane({ cursor: 1 })
    await press(list, 'X')
    const box = editorOf(root)
    expect(box?.value).toBe('X')
    if (box) await press(box, 'Enter')
    expect(onEdit).toHaveBeenCalledWith([{ kind: 'setAll', field: 'album', values: ['X'] }])
  })

  it('Delete and Backspace inside the open editor stay text editing', async () => {
    const { root, list, onClear } = await mountPane()
    await press(list, 'H')
    const box = editorOf(root)
    if (box) {
      await press(box, 'Backspace')
      await press(box, 'Delete')
    }
    expect(editorOf(root)).toBe(box)
    expect(onClear).not.toHaveBeenCalled()
  })
})
