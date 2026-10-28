/**
 * The Tags window's function-key bar: one button per tag command, each showing the key bound to it now (the
 * shortcuts store, as the file panes' bar), Shift showing the Shift row, and a click sending its command.
 */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount, unmount } from 'svelte'

vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn(() =>
    Promise.resolve({
      get: vi.fn(() => Promise.resolve(undefined)),
      set: vi.fn(() => Promise.resolve()),
      save: vi.fn(() => Promise.resolve()),
      keys: vi.fn(() => Promise.resolve([])),
      delete: vi.fn(() => Promise.resolve()),
    }),
  ),
}))
vi.mock('#lib/ipc/bindings.js', () => ({
  commands: { updateMenuAccelerator: vi.fn(() => Promise.resolve({ status: 'ok' })) },
}))

import TagsFunctionKeyBar from './TagsFunctionKeyBar.svelte'
import { TAGS_BAR_COMMANDS, type TagsBarCommandId } from './tags-function-keys'
import { getFirstShiftShortcutReactive, getFirstShortcutReactive } from '#lib/shortcuts/reactive-shortcuts.svelte.js'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

function mountBar(enabled: ReadonlySet<TagsBarCommandId> = new Set(TAGS_BAR_COMMANDS)) {
  const onCommand = vi.fn<(id: TagsBarCommandId) => void>()
  const target = document.createElement('div')
  document.body.appendChild(target)
  const bar = mount(TagsFunctionKeyBar, { target, props: { enabled, onCommand } })
  flushSync()
  cleanup = () => {
    void unmount(bar)
    target.remove()
  }
  const buttons = () => [...target.querySelectorAll('button')]
  return { target, onCommand, buttons }
}

const labelOf = (button: HTMLButtonElement) => button.querySelector('span')?.textContent ?? ''
const chipOf = (button: HTMLButtonElement) => button.querySelector('kbd')?.textContent ?? ''

async function shift(down: boolean): Promise<void> {
  document.dispatchEvent(new KeyboardEvent(down ? 'keydown' : 'keyup', { key: 'Shift' }))
  await new Promise((resolve) => setTimeout(resolve, 0))
  flushSync()
}

describe('TagsFunctionKeyBar', () => {
  it('shows F2…F8 and ⌘⏎ Write with their tag commands, each key read from the shortcuts store', () => {
    const { buttons } = mountBar()
    expect(buttons().map(labelOf)).toEqual([
      'Edit',
      'Play',
      'Replace',
      'Format',
      'From name',
      'Number',
      'Clear',
      'Write',
    ])
    expect(buttons().slice(0, 7).map(chipOf)).toEqual(['F2', 'F3', 'F4', 'F5', 'F6', 'F7', 'F8'])
    expect(chipOf(buttons()[7])).toBe(getFirstShortcutReactive('tags.write'))
  })

  it('a click sends the button’s command', () => {
    const { buttons, onCommand } = mountBar()
    for (const button of buttons()) button.click()
    expect(onCommand.mock.calls.map(([id]) => id)).toEqual([
      'tags.edit',
      'tags.play',
      'tags.replace',
      'tags.format',
      'tags.fromName',
      'tags.number',
      'tags.clearField',
      'tags.write',
    ])
  })

  it('with Shift held shows the Shift row: ⇧F2 Edit #, ⇧F3 Open in player, ⇧F4 Case, ⇧F5 Clean up, ⇧F8 Remove extras, ⇧⌘S Review', async () => {
    const { buttons, onCommand } = mountBar()
    await shift(true)
    expect(buttons().map(labelOf)).toEqual([
      'Edit #',
      'Open in player',
      'Case',
      'Clean up',
      '',
      '',
      'Remove extras',
      'Review',
    ])
    expect(chipOf(buttons()[7])).toBe(getFirstShiftShortcutReactive('tags.review'))
    for (const button of buttons()) if (!button.disabled) button.click()
    expect(onCommand.mock.calls.map(([id]) => id)).toEqual([
      'tags.editNumber',
      'tags.openInPlayer',
      'tags.case',
      'tags.cleanUp',
      'tags.removeExtras',
      'tags.review',
    ])
    await shift(false)
    expect(labelOf(buttons()[0])).toBe('Edit')
  })

  it('a command that can’t act now is disabled', () => {
    const { buttons } = mountBar(new Set<TagsBarCommandId>(['tags.edit', 'tags.write']))
    expect(buttons().map((b) => b.disabled)).toEqual([false, true, true, true, true, true, true, false])
  })

  it('every chip is its command’s binding as the shortcuts store has it, in both rows', async () => {
    const { buttons } = mountBar()
    const plain: TagsBarCommandId[] = [
      'tags.edit',
      'tags.play',
      'tags.replace',
      'tags.format',
      'tags.fromName',
      'tags.number',
      'tags.clearField',
      'tags.write',
    ]
    expect(buttons().map(chipOf)).toEqual(plain.map((id) => getFirstShortcutReactive(id)))
    await shift(true)
    const shifted: TagsBarCommandId[] = ['tags.editNumber', 'tags.openInPlayer', 'tags.case', 'tags.cleanUp']
    expect(buttons().slice(0, 4).map(chipOf)).toEqual(shifted.map((id) => getFirstShiftShortcutReactive(id)))
    expect(
      buttons()
        .slice(0, 4)
        .map((b) => b.getAttribute('aria-keyshortcuts')),
    ).toEqual(['Shift+F2', 'Shift+F3', 'Shift+F4', 'Shift+F5'])
    await shift(false)
  })

  it('never takes focus: every button is out of the tab order and a press keeps focus where it was', () => {
    const { target, buttons } = mountBar()
    expect(buttons().every((b) => b.tabIndex === -1)).toBe(true)
    const press = new MouseEvent('mousedown', { bubbles: true, cancelable: true })
    target.querySelector('[role="toolbar"]')?.dispatchEvent(press)
    expect(press.defaultPrevented).toBe(true)
  })
})
