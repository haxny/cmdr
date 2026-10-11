/**
 * The queue window's row cursor and the "Cancel selected" set it extends: the
 * part of the keyboard model that's pure bookkeeping over operation ids, so it's
 * pinned here without a DOM. The page wires it to keys and focus
 * (`routes/queue/queue-keyboard.svelte.test.ts`).
 */
import { describe, expect, it } from 'vitest'
import { SvelteSet } from 'svelte/reactivity'
import { createQueueCursor } from './queue-cursor.svelte'

/** A cursor over `ids`, where every id is selectable unless listed in `settled`. */
function setup(ids: string[], settled: string[] = []) {
  let current = ids
  const selection = new SvelteSet<string>()
  const cursor = createQueueCursor({
    ids: () => current,
    selection,
    isSelectable: (id) => !settled.includes(id),
  })
  return {
    cursor,
    selection,
    setIds(next: string[]) {
      current = next
      cursor.reconcile()
    },
    selected: () => [...selection].sort(),
  }
}

describe('moving the cursor', () => {
  it('starts nowhere, so the first ↓ lands on the first row and the first ↑ on the last', () => {
    const down = setup(['a', 'b', 'c'])
    expect(down.cursor.cursorId).toBeNull()
    expect(down.cursor.move('next')).toBe('a')

    const up = setup(['a', 'b', 'c'])
    expect(up.cursor.move('previous')).toBe('c')
  })

  it('steps one row and stops at either end without wrapping', () => {
    const { cursor } = setup(['a', 'b', 'c'])
    cursor.place('b')
    expect(cursor.move('next')).toBe('c')
    expect(cursor.move('next')).toBe('c')
    expect(cursor.move('previous')).toBe('b')
    expect(cursor.move('previous')).toBe('a')
    expect(cursor.move('previous')).toBe('a')
  })

  it('jumps to the first and last rows', () => {
    const { cursor } = setup(['a', 'b', 'c'])
    cursor.place('b')
    expect(cursor.move('last')).toBe('c')
    expect(cursor.move('first')).toBe('a')
  })

  it('has nowhere to go in an empty list', () => {
    const { cursor } = setup([])
    expect(cursor.move('next')).toBeNull()
    expect(cursor.move('last')).toBeNull()
  })

  it('never touches the selection on a plain move', () => {
    const { cursor, selected } = setup(['a', 'b', 'c'])
    cursor.move('next')
    cursor.move('next')
    expect(selected()).toEqual([])
  })
})

describe('following the rows as they change', () => {
  it('stays on its operation when the rows reorder around it', () => {
    const { cursor, setIds } = setup(['a', 'b', 'c'])
    cursor.place('c')
    setIds(['a', 'c', 'b'])
    expect(cursor.cursorId).toBe('c')
  })

  it('takes the row that slid into its place when its operation leaves', () => {
    const { cursor, setIds } = setup(['a', 'b', 'c'])
    cursor.place('b')
    setIds(['a', 'c'])
    expect(cursor.cursorId).toBe('c')
  })

  it('takes the new last row when the last one leaves', () => {
    const { cursor, setIds } = setup(['a', 'b', 'c'])
    cursor.place('c')
    setIds(['a', 'b'])
    expect(cursor.cursorId).toBe('b')
  })

  it('goes back to nowhere when the list empties', () => {
    const { cursor, setIds } = setup(['a'])
    cursor.place('a')
    setIds([])
    expect(cursor.cursorId).toBeNull()
  })

  it('stays nowhere when it was nowhere', () => {
    const { cursor, setIds } = setup(['a', 'b'])
    setIds(['b'])
    expect(cursor.cursorId).toBeNull()
  })
})

describe('Space and ⌘A', () => {
  it('toggles the row under the cursor', () => {
    const { cursor, selected } = setup(['a', 'b'])
    cursor.place('b')
    cursor.toggle()
    expect(selected()).toEqual(['b'])
    cursor.toggle()
    expect(selected()).toEqual([])
  })

  it('does nothing on a row that can no longer be canceled, or with no cursor', () => {
    const { cursor, selected } = setup(['a', 'failed'], ['failed'])
    cursor.toggle()
    cursor.place('failed')
    cursor.toggle()
    expect(selected()).toEqual([])
  })

  it('selects every row that can still be canceled', () => {
    const { cursor, selected } = setup(['a', 'failed', 'b'], ['failed'])
    cursor.selectAll()
    expect(selected()).toEqual(['a', 'b'])
  })
})

describe('⇧↑ / ⇧↓ extend from an anchor', () => {
  it('selects the row it leaves and the row it lands on', () => {
    const { cursor, selected } = setup(['a', 'b', 'c', 'd'])
    cursor.place('b')
    expect(cursor.extend('next')).toBe('c')
    expect(selected()).toEqual(['b', 'c'])
    cursor.extend('next')
    expect(selected()).toEqual(['b', 'c', 'd'])
  })

  it('takes rows back out when it turns back toward the anchor, like a macOS table', () => {
    const { cursor, selected } = setup(['a', 'b', 'c', 'd'])
    cursor.place('b')
    cursor.extend('next')
    cursor.extend('next')
    cursor.extend('previous')
    expect(selected()).toEqual(['b', 'c'])
    cursor.extend('previous')
    cursor.extend('previous')
    expect(selected()).toEqual(['a', 'b'])
  })

  it('keeps what was selected before the range began', () => {
    const { cursor, selected } = setup(['a', 'b', 'c', 'd'])
    cursor.place('a')
    cursor.toggle()
    cursor.move('next')
    cursor.move('next')
    cursor.extend('next')
    cursor.extend('previous')
    expect(selected()).toEqual(['a', 'c'])
  })

  it('starts a new range after a plain move', () => {
    const { cursor, selected } = setup(['a', 'b', 'c', 'd'])
    cursor.place('a')
    cursor.extend('next')
    cursor.move('next')
    cursor.extend('next')
    expect(selected()).toEqual(['a', 'b', 'c', 'd'])
    cursor.extend('previous')
    expect(selected()).toEqual(['a', 'b', 'c'])
  })

  it('passes over a row that can no longer be canceled without selecting it', () => {
    const { cursor, selected } = setup(['a', 'failed', 'c'], ['failed'])
    cursor.place('a')
    cursor.extend('next')
    cursor.extend('next')
    expect(selected()).toEqual(['a', 'c'])
  })

  it('from nowhere, starts on the row it lands on', () => {
    const { cursor, selected } = setup(['a', 'b'])
    expect(cursor.extend('next')).toBe('a')
    expect(selected()).toEqual(['a'])
  })

  it('re-anchors on the cursor when the anchor row leaves', () => {
    const { cursor, selected, setIds } = setup(['a', 'b', 'c', 'd'])
    cursor.place('a')
    cursor.extend('next')
    setIds(['b', 'c', 'd'])
    cursor.extend('next')
    expect(selected()).toEqual(['a', 'b', 'c'])
  })
})
