/** The value picker of a «multiple values» row: lists each distinct value with its file count, and a pick sends it. */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { tick } from 'svelte'
import type { ValueCount } from '#lib/ipc/bindings.js'
import ValueChoices from './ValueChoices.svelte'
import { mountInto, press } from './tag-cell.test-helper'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

const CHOICES: ValueCount[] = [
  { values: ['Kniha 1'], files: 12 },
  { values: ['Kniha 1 CD2'], files: 8 },
]

async function mountChoices(over: { choices?: ValueCount[]; disabled?: boolean } = {}) {
  const onPick = vi.fn((_values: string[]) => {})
  const mounted = await mountInto(ValueChoices, {
    field: 'Album',
    choices: over.choices ?? CHOICES,
    disabled: over.disabled ?? false,
    onPick,
  })
  cleanup = mounted.cleanup
  const button = mounted.root.querySelector<HTMLButtonElement>('button')
  if (!button) throw new Error('no button')
  return { root: mounted.root, button, onPick }
}

const options = () => [...document.querySelectorAll<HTMLElement>('[role="option"]')]

describe('ValueChoices', () => {
  it('has an accessible name that names the field, and the list is closed at first', async () => {
    const { button } = await mountChoices()
    expect(button.getAttribute('aria-label')).toBe('Album values')
    expect(options()).toHaveLength(0)
  })

  it('lists each distinct value with how many files carry it', async () => {
    const { button } = await mountChoices()
    button.click()
    await tick()
    expect(options().map((o) => o.textContent.trim())).toEqual(['Kniha 1 (12)', 'Kniha 1 CD2 (8)'])
    expect(document.querySelector('[role="listbox"]')).not.toBeNull()
  })

  it('shows a multi-value set joined with "; "', async () => {
    const { button } = await mountChoices({ choices: [{ values: ['audiokniha', 'detektivka'], files: 3 }] })
    button.click()
    await tick()
    expect(options().map((o) => o.textContent.trim())).toEqual(['audiokniha; detektivka (3)'])
  })

  it('picking one sends its value list and closes the list', async () => {
    const { button, onPick } = await mountChoices()
    button.click()
    await tick()
    options()[1]?.click()
    await tick()
    expect(onPick).toHaveBeenCalledTimes(1)
    expect(onPick).toHaveBeenCalledWith(['Kniha 1 CD2'])
    expect(options()).toHaveLength(0)
  })

  it('Enter on an option picks it too', async () => {
    const { button, onPick } = await mountChoices()
    button.click()
    await tick()
    const first = options()[0]
    if (!first) throw new Error('no option')
    await press(first, 'Enter')
    expect(onPick).toHaveBeenCalledWith(['Kniha 1'])
  })

  it('a disabled picker does not open', async () => {
    const { button } = await mountChoices({ disabled: true })
    expect(button.disabled).toBe(true)
  })
})
