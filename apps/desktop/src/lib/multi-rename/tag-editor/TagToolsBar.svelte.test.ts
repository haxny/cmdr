/** The clean-up tools row: each button sends its exact tool; Renumber and Remove extras follow the tag canon. */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { tick } from 'svelte'
import type { TagField } from '#lib/ipc/bindings.js'
import TagToolsBar from './TagToolsBar.svelte'
import { DIACRITICS_FIELDS } from './tag-tools'
import { mountInto } from './tag-cell.test-helper'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

async function mountBar(
  over: { trackCount?: number; extras?: string[]; focusedField?: TagField | null; disabled?: boolean } = {},
) {
  const onTool = vi.fn()
  const mounted = await mountInto(TagToolsBar, {
    trackCount: over.trackCount ?? 3,
    extras: over.extras ?? [],
    focusedField: over.focusedField ?? null,
    disabled: over.disabled ?? false,
    onTool,
  })
  cleanup = mounted.cleanup
  return { root: mounted.root, onTool }
}

function button(root: HTMLElement, text: string): HTMLButtonElement | undefined {
  return [...root.querySelectorAll('button')].find((b) => b.textContent.trim() === text)
}

async function click(root: HTMLElement, text: string): Promise<void> {
  const target = button(root, text)
  if (!target) throw new Error(`no button ${text}`)
  target.click()
  await tick()
}

describe('TagToolsBar', () => {
  it.each([
    ['Remove diacritics', { kind: 'stripDiacritics', fields: [...DIACRITICS_FIELDS] }],
    ['Strip track prefix', { kind: 'stripTrackPrefix' }],
    ['Renumber', { kind: 'renumber', start: 1 }],
    ['Album artist = Artist', { kind: 'albumArtistFromArtist' }],
    ['Narrator from text', { kind: 'narratorFromText' }],
    ['Clear title = album', { kind: 'clearRedundantTitles' }],
  ])('%s sends %j', async (text, tool) => {
    const { root, onTool } = await mountBar()
    await click(root, text)
    expect(onTool).toHaveBeenCalledWith(tool)
  })

  it('renumber_hidden_for_single_file', async () => {
    const { root } = await mountBar({ trackCount: 1 })
    expect(button(root, 'Renumber')).toBeUndefined()
    cleanup?.()
    const two = await mountBar({ trackCount: 2 })
    expect(button(two.root, 'Renumber')).toBeDefined()
  })

  it('Case with nothing focused changes the five name fields, and says so', async () => {
    const { root, onTool } = await mountBar()
    await click(root, 'Case')
    expect(root.textContent).toContain('Changes: Title, Album, Artist, Album artist, Performer')
    await click(root, 'UPPER CASE')
    expect(onTool).toHaveBeenCalledWith({
      kind: 'case',
      case: 'upper',
      fields: ['title', 'album', 'artist', 'albumArtist', 'performer'],
    })
    // The choices close once one is taken.
    expect(button(root, 'lower case')).toBeUndefined()
  })

  it('Case with a field focused changes that field only', async () => {
    const { root, onTool } = await mountBar({ focusedField: 'album' })
    await click(root, 'Case')
    expect(root.textContent).toContain('Changes: Album')
    await click(root, 'Sentence case')
    expect(onTool).toHaveBeenCalledWith({ kind: 'case', case: 'sentence', fields: ['album'] })
  })

  it('remove_extras_requires_explicit_choice', async () => {
    const { root, onTool } = await mountBar({ extras: ['COMM:eng', 'TXXX:MOOD'] })
    await click(root, 'Remove extras…')
    const boxes = [...root.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')]
    expect(boxes).toHaveLength(2)
    expect(boxes.every((box) => !box.checked)).toBe(true)
    const remove = [...root.querySelectorAll('button')].find((b) => b.textContent.includes('Remove 0'))
    expect(remove?.disabled).toBe(true)
    boxes[1].click()
    await tick()
    await click(root, 'Remove 1 extra tag')
    expect(onTool).toHaveBeenCalledTimes(1)
    expect(onTool).toHaveBeenCalledWith({ kind: 'removeExtras', keys: ['TXXX:MOOD'] })
  })

  it('Remove extras is off when the files carry none', async () => {
    const { root } = await mountBar({ extras: [] })
    expect(button(root, 'Remove extras…')?.disabled).toBe(true)
  })

  it('a read-only session locks every tool', async () => {
    const { root } = await mountBar({ disabled: true, extras: ['K'] })
    for (const b of root.querySelectorAll('button')) expect(b.disabled).toBe(true)
  })
})
