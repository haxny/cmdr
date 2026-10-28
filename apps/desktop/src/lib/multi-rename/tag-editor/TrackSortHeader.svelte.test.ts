/** `TrackSortHeader`: one sortable column header of the Tracks pane (a button, its direction caret, its spoken state). */
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { TrackSort, TrackSortKey } from '#lib/ipc/bindings.js'
import TrackSortHeader from './TrackSortHeader.svelte'
import { mountInto } from './tag-cell.test-helper'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

async function mountHeader(current: TrackSort | null) {
  const onClick = vi.fn()
  const mounted = await mountInto(TrackSortHeader, {
    sortKey: 'title' as TrackSortKey,
    label: 'Title',
    current,
    onClick,
  })
  cleanup = mounted.cleanup
  const button = mounted.root.querySelector('button')
  if (!button) throw new Error('no button')
  return { button, onClick }
}

describe('TrackSortHeader', () => {
  it('a click asks to sort by its key', async () => {
    const { button, onClick } = await mountHeader(null)
    button.click()
    expect(onClick).toHaveBeenCalledWith('title')
  })

  it('says "not sorted" when another column is the active one, and shows no caret', async () => {
    const { button } = await mountHeader({ key: 'length', descending: false })
    expect(button.getAttribute('aria-label')).toBe('Title, not sorted')
    expect(button.classList.contains('is-active')).toBe(false)
    expect(button.querySelector<HTMLElement>('.sort-indicator')?.hidden).toBe(true)
  })

  it('says "not sorted" when no column is active', async () => {
    const { button } = await mountHeader(null)
    expect(button.getAttribute('aria-label')).toBe('Title, not sorted')
  })

  it('the active header shows its direction', async () => {
    const asc = await mountHeader({ key: 'title', descending: false })
    expect(asc.button.getAttribute('aria-label')).toBe('Title, sorted ascending')
    expect(asc.button.querySelector('.sort-indicator')?.textContent.trim()).toBe('▲')
    cleanup?.()
    const desc = await mountHeader({ key: 'title', descending: true })
    expect(desc.button.getAttribute('aria-label')).toBe('Title, sorted descending')
    expect(desc.button.querySelector('.sort-indicator')?.textContent.trim()).toBe('▼')
  })
})
