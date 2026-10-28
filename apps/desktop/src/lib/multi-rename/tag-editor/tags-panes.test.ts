/** Which of the Tags window's two panes (Album | Tracks) has focus, and Tab's switch between them. */
import { describe, expect, it, vi } from 'vitest'
import { createTagsPanes } from './tags-panes.svelte'

describe('createTagsPanes', () => {
  it('starts with no pane active and both cursors on the first row', () => {
    const panes = createTagsPanes({ focus: vi.fn() })
    expect(panes.active).toBeNull()
    expect(panes.albumCursor).toBe(0)
    expect(panes.tracksCursor).toBe(0)
  })

  it('switchPane focuses the other pane and makes it the active one', () => {
    const focus = vi.fn()
    const panes = createTagsPanes({ focus })
    panes.noteFocus('album')
    expect(panes.switchPane()).toBe('tracks')
    expect(focus).toHaveBeenLastCalledWith('tracks')
    expect(panes.active).toBe('tracks')
    expect(panes.switchPane()).toBe('album')
    expect(focus).toHaveBeenLastCalledWith('album')
  })

  it('with no pane active yet, switchPane goes to the album', () => {
    const focus = vi.fn()
    const panes = createTagsPanes({ focus })
    expect(panes.switchPane()).toBe('album')
    expect(focus).toHaveBeenCalledWith('album')
  })

  it('noteFocus follows focus, and null forgets the pane (focus left both)', () => {
    const panes = createTagsPanes({ focus: vi.fn() })
    panes.noteFocus('tracks')
    expect(panes.active).toBe('tracks')
    panes.noteFocus(null)
    expect(panes.active).toBeNull()
  })

  it('keeps each pane’s cursor apart', () => {
    const panes = createTagsPanes({ focus: vi.fn() })
    panes.albumCursor = 3
    panes.tracksCursor = 7
    expect(panes.albumCursor).toBe(3)
    expect(panes.tracksCursor).toBe(7)
  })
})
