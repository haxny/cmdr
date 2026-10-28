/** The in-place editor over a pane row: Enter commits a change, an unchanged value sends nothing, click-away commits. */
import { afterEach, describe, expect, it, vi } from 'vitest'
import TagValueEditor from './TagValueEditor.svelte'
import { leave, mountInto, press, typeInto } from './tag-cell.test-helper'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

async function mountEditor() {
  const onCommit = vi.fn()
  const onClose = vi.fn()
  const mounted = await mountInto(TagValueEditor, { value: 'Old', ariaLabel: 'Title', onCommit, onClose })
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
})
