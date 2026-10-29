/** The in-place editor over a pane row: Enter commits a change, an unchanged value sends nothing, click-away commits. */
import { afterEach, describe, expect, it, vi } from 'vitest'
import TagValueEditor from './TagValueEditor.svelte'
import editorSource from './TagValueEditor.svelte?raw'
import { leave, mountInto, press, typeInto } from './tag-cell.test-helper'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

/** A row that refuses any text still holding `BLOCK` (as «multiple values» refuses its marker). */
const blockedHint = (text: string): string | null =>
  text.includes('BLOCK') ? `Delete BLOCK (${String(text.length)})` : null

async function mountEditor(value = 'Old') {
  const onCommit = vi.fn()
  const onClose = vi.fn()
  const mounted = await mountInto(TagValueEditor, { value, ariaLabel: 'Title', blockedHint, onCommit, onClose })
  cleanup = mounted.cleanup
  const input = mounted.root.querySelector<HTMLInputElement>('input')
  if (!input) throw new Error('no input')
  return { input, onCommit, onClose }
}

describe('TagValueEditor', () => {
  it('takes focus with its text selected', async () => {
    const { input } = await mountEditor()
    expect(document.activeElement).toBe(input)
    expect(input.selectionStart).toBe(0)
    expect(input.selectionEnd).toBe(3)
  })

  it('Enter on an unchanged value closes without sending', async () => {
    const { input, onCommit, onClose } = await mountEditor()
    await press(input, 'Enter')
    expect(onCommit).not.toHaveBeenCalled()
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('leaving the editor (a click elsewhere) commits the change once', async () => {
    const { input, onCommit, onClose } = await mountEditor()
    await typeInto(input, 'New')
    await leave(input)
    await press(input, 'Enter')
    expect(onCommit).toHaveBeenCalledTimes(1)
    expect(onCommit).toHaveBeenCalledWith('New')
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('the hint follows the typing: it updates while the text is still refused, and goes once it can be sent', async () => {
    const { input } = await mountEditor('BLOCK a; b')
    await typeInto(input, 'BLOCK a')
    await press(input, 'Enter')
    const hint = () => input.parentElement?.querySelector('.tag-value-hint')
    expect(hint()?.textContent).toBe('Delete BLOCK (7)')
    await typeInto(input, 'BLOCK ab')
    expect(hint()?.textContent).toBe('Delete BLOCK (8)')
    await typeInto(input, 'a')
    expect(hint()).toBeNull()
    expect(input.getAttribute('aria-describedby')).toBeNull()
  })

  it('a click away while the text is refused keeps the box, the typing and the hint, and sends nothing', async () => {
    const { input, onCommit, onClose } = await mountEditor('BLOCK a; b')
    await typeInto(input, 'BLOCK a')
    await leave(input)
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(onCommit).not.toHaveBeenCalled()
    expect(onClose).not.toHaveBeenCalled()
    expect(input.isConnected).toBe(true)
    expect(input.value).toBe('BLOCK a')
    expect(input.parentElement?.querySelector('.tag-value-hint')?.textContent).toBe('Delete BLOCK (7)')
    expect(document.activeElement).toBe(input)
    // Fixed, it goes as any edit does.
    await typeInto(input, 'a')
    await press(input, 'Enter')
    expect(onCommit).toHaveBeenCalledWith('a')
  })

  it('a click away from an untouched refused value closes without sending, as before', async () => {
    const { input, onCommit, onClose } = await mountEditor('BLOCK a; b')
    await leave(input)
    expect(onCommit).not.toHaveBeenCalled()
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('scrolls the hint into view when it appears, not only when the box opens', async () => {
    const scroll = vi.fn()
    const had = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'scrollIntoView')
    HTMLElement.prototype.scrollIntoView = scroll
    try {
      const { input } = await mountEditor('BLOCK a; b')
      const onOpen = scroll.mock.calls.length
      await typeInto(input, 'BLOCK a')
      await press(input, 'Enter')
      await new Promise((resolve) => setTimeout(resolve, 0))
      expect(scroll.mock.calls.length).toBeGreaterThan(onOpen)
      expect(scroll).toHaveBeenLastCalledWith({ block: 'nearest' })
    } finally {
      if (had) Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', had)
      else delete (HTMLElement.prototype as { scrollIntoView?: unknown }).scrollIntoView
    }
  })

  it('refused text never takes focus back from a dialog (its focus trap would fight it)', async () => {
    const { input, onClose } = await mountEditor('BLOCK a; b')
    await typeInto(input, 'BLOCK a')
    const dialog = document.createElement('div')
    dialog.setAttribute('role', 'dialog')
    const field = document.createElement('button')
    dialog.appendChild(field)
    document.body.appendChild(dialog)
    try {
      field.focus()
      input.dispatchEvent(new FocusEvent('blur', { relatedTarget: field }))
      await new Promise((resolve) => setTimeout(resolve, 0))
      expect(document.activeElement).toBe(field)
      expect(onClose).not.toHaveBeenCalled()
      expect(input.value).toBe('BLOCK a')
    } finally {
      dialog.remove()
    }
  })
})

describe('TagValueEditor seed', () => {
  it('starts with exactly the seed, caret at the end and nothing selected, and commits it on Enter', async () => {
    const onCommit = vi.fn()
    const mounted = await mountInto(TagValueEditor, {
      value: 'Old',
      seed: 'H',
      ariaLabel: 'Title',
      onCommit,
      onClose: vi.fn(),
    })
    cleanup = mounted.cleanup
    const input = mounted.root.querySelector<HTMLInputElement>('input')
    if (!input) throw new Error('no input')
    expect(input.value).toBe('H')
    expect(input.selectionStart).toBe(1)
    expect(input.selectionEnd).toBe(1)
    await press(input, 'Enter')
    expect(onCommit).toHaveBeenCalledWith('H')
  })
})

describe('TagValueEditor looks like the cell', () => {
  const style = editorSource.slice(editorSource.indexOf('<style>'))

  it('takes the cell box: the single line gets its height, the multiline starts at it and may grow', async () => {
    const place = { top: 50, left: 20, width: 120, height: 22 }
    const line = await mountInto(TagValueEditor, {
      value: 'a',
      ariaLabel: 'T',
      place,
      onCommit: vi.fn(),
      onClose: vi.fn(),
    })
    const frame = line.root.querySelector<HTMLElement>('.tag-value-frame')
    expect(frame?.style.top).toBe('50px')
    expect(frame?.style.left).toBe('20px')
    expect(frame?.style.width).toBe('120px')
    expect(line.root.querySelector<HTMLInputElement>('input')?.style.height).toBe('22px')
    line.cleanup()
    const multi = await mountInto(TagValueEditor, {
      value: 'a',
      ariaLabel: 'T',
      multiline: true,
      place,
      onCommit: vi.fn(),
      onClose: vi.fn(),
    })
    cleanup = multi.cleanup
    const area = multi.root.querySelector<HTMLTextAreaElement>('textarea')
    expect(area?.style.minHeight).toBe('22px')
    expect(area?.style.height).toBe('')
    expect(area?.classList.contains('tag-value-editor')).toBe(true)
  })

  it('inherits the cell text and adds no border width or inline padding that would shift it', () => {
    const rule = /\.tag-value-editor \{[^}]*\}/.exec(style)?.[0] ?? ''
    expect(rule).toContain('font: inherit')
    expect(rule).toContain('letter-spacing: inherit')
    expect(rule).toContain('border: 0')
    expect(rule).toMatch(/padding: 0\.2em 0;/)
    expect(rule).not.toMatch(/border: [1-9]/)
  })

  it('shows focus as the accent glow inside the cell, calmed by reduced motion', () => {
    expect(style).toMatch(/box-shadow: inset [^;]*var\(--color-accent\)/)
    expect(style).toMatch(/aria-invalid='true'\] \{[^}]*var\(--color-error\)/)
    const media = style.indexOf('@media (prefers-reduced-motion: no-preference)')
    expect(media).toBeGreaterThan(-1)
    expect(style.indexOf('animation:')).toBeGreaterThan(media)
  })
})
