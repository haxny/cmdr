/**
 * The focus-model cases both file lists owe a screen reader, run once per view from its
 * a11y test file (each owns the module stubs its view needs):
 *
 *     describe('BriefList focus model', () => {
 *       listFocusCases((options) => mountBriefList(options))
 *     })
 *
 * A `.svelte.ts` so a case can hold live props in `$state` (a navigation, a cursor move).
 */

import { expect, it, vi } from 'vitest'
import { tick, unmount } from 'svelte'
import { getFileRange } from '#lib/tauri-commands/index.js'
import { assertFocusModel } from '#lib/test-focus-model.js'
import { registerFocusHome } from '#lib/ui/focus-restore.js'
import { fileEntry } from './test-full-list'
import type { FileEntry } from '../types'

/** The props every case may change after mount; both views take them. */
interface LiveListProps {
  listingId: string
  cursorIndex: number
}

export interface MountedFocusableList {
  component: { focusList: () => boolean }
  target: HTMLElement
  rows: () => HTMLElement[]
  rowNames: () => string[]
  settle: (until: () => boolean, reason: string) => Promise<void>
}

export type MountFocusableList = (options: {
  entries: FileEntry[]
  liveProps?: Partial<LiveListProps>
}) => Promise<MountedFocusableList>

const ENTRIES = [fileEntry({ name: 'a.txt' }), fileEntry({ name: 'b.txt' }), fileEntry({ name: 'c.txt' })]

function listboxOf(list: MountedFocusableList): HTMLElement {
  const listbox = list.target.querySelector<HTMLElement>('[role="listbox"]')
  if (!listbox) throw new Error('the list rendered no listbox')
  return listbox
}

function activeDescendant(listbox: HTMLElement): string | null {
  return listbox.getAttribute('aria-activedescendant')
}

async function focusAndLand(list: MountedFocusableList): Promise<HTMLElement> {
  const listbox = listboxOf(list)
  expect(list.component.focusList()).toBe(true)
  await list.settle(() => activeDescendant(listbox) !== null, 'the cursor row to land as the active descendant')
  return listbox
}

export function listFocusCases(mountList: MountFocusableList): void {
  it('renders row ids that stay unique with two lists mounted', async () => {
    const first = await mountList({ entries: ENTRIES })
    const second = await mountList({ entries: ENTRIES })
    const ids = [...first.rows(), ...second.rows()].map((row) => row.id)
    expect(ids).toHaveLength(ENTRIES.length * 2)
    expect(ids.filter((id, at) => ids.indexOf(id) !== at)).toEqual([])
  })

  it('focuses its listbox and names the cursor row one frame later, never at once', async () => {
    const list = await mountList({ entries: ENTRIES })
    const listbox = listboxOf(list)
    expect(activeDescendant(listbox)).toBeNull()

    expect(list.component.focusList()).toBe(true)
    expect(document.activeElement).toBe(listbox)
    await tick()
    expect(activeDescendant(listbox)).toBeNull()

    await list.settle(() => activeDescendant(listbox) !== null, 'the cursor row to land as the active descendant')
    assertFocusModel(list.target)
    expect(document.getElementById(activeDescendant(listbox) ?? '')?.dataset.filename).toBe('a.txt')
  })

  it('names no row once focus leaves it', async () => {
    const list = await mountList({ entries: ENTRIES })
    const listbox = await focusAndLand(list)
    listbox.blur()
    await tick()
    expect(activeDescendant(listbox)).toBeNull()
  })

  it('names the new cursor row at once when the cursor moves', async () => {
    const live = $state<Partial<LiveListProps>>({})
    const list = await mountList({ entries: ENTRIES, liveProps: live })
    const listbox = await focusAndLand(list)
    const seen: (string | null)[] = []
    new MutationObserver(() => seen.push(activeDescendant(listbox))).observe(listbox, {
      attributeFilter: ['aria-activedescendant'],
    })

    live.cursorIndex = 1
    await tick()
    await Promise.resolve()

    expect(document.getElementById(activeDescendant(listbox) ?? '')?.dataset.filename).toBe('b.txt')
    expect(seen).not.toContain(null)
  })

  // ❗ Entering a folder renders a new row 0 under the same id; without landing the id again
  // WebKit keeps focus on the dead node and VoiceOver says nothing (heard in the 2026-10-15 VoiceOver spike).
  it('lands the id again when a navigation replaces the cursor row under it', async () => {
    const live = $state<Partial<LiveListProps>>({})
    const list = await mountList({ entries: ENTRIES, liveProps: live })
    const listbox = await focusAndLand(list)
    const rowZeroId = activeDescendant(listbox)
    const seen: (string | null)[] = []
    new MutationObserver(() => seen.push(activeDescendant(listbox))).observe(listbox, {
      attributeFilter: ['aria-activedescendant'],
    })

    // As many rows as before: `totalCount` stays, and a listing shorter than it never settles.
    const next = ['x.txt', 'y.txt', 'z.txt'].map((name) => fileEntry({ name, path: `/root/sub/${name}` }))
    vi.mocked(getFileRange).mockImplementation((_listingId, start: number, count: number) =>
      Promise.resolve(next.slice(start, start + count)),
    )
    live.listingId = 'listing-2'
    await list.settle(
      () => list.rowNames()[0] === 'x.txt' && activeDescendant(listbox) !== null,
      'the new folder rows and their cursor row',
    )

    expect(activeDescendant(listbox)).toBe(rowZeroId)
    expect(seen).toContain(null)
    assertFocusModel(list.target)
  })

  // ❗ A Brief/Full toggle remounts the list: focus fell to `<body>` and ↓ did nothing until
  // `Tab` (2026-10-15 VoiceOver spike). The new list picks focus up when its pane is focused.
  it('takes focus when it mounts in the focused pane while focus is adrift', async () => {
    const unregister = registerFocusHome({ root: () => undefined, refocus: () => {} })
    try {
      document.body.focus()
      const list = await mountList({ entries: ENTRIES })
      await list.settle(() => document.activeElement === listboxOf(list), 'the new list to take focus')
    } finally {
      unregister()
    }
  })

  it('puts focus home when it unmounts holding it', async () => {
    const refocus = vi.fn()
    const unregister = registerFocusHome({ root: () => undefined, refocus })
    try {
      const list = await mountList({ entries: ENTRIES })
      list.component.focusList()
      refocus.mockClear()
      await unmount(list.component)
      await tick()
      expect(refocus).toHaveBeenCalledTimes(1)
    } finally {
      unregister()
    }
  })
}
