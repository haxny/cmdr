import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushSync } from 'svelte'
import { createQueueDrag, type QueueDragDeps } from './queue-drag.svelte'

/** Three waiting rows, 40 px tall, stacked from y = 0: midpoints at 20, 60, 100. */
function rows(ids: string[]): Map<string, HTMLElement> {
  const els = new Map<string, HTMLElement>()
  ids.forEach((id, i) => {
    const el = document.createElement('div')
    el.getBoundingClientRect = () => ({ top: i * 40, height: 40, bottom: i * 40 + 40 }) as DOMRect
    els.set(id, el)
  })
  return els
}

function setup(overrides: Partial<QueueDragDeps> = {}) {
  const ids = ['a', 'b', 'c']
  const els = rows(ids)
  const onDrop = vi.fn((_id: string, _position: number) => Promise.resolve(true))
  const onLift = vi.fn()
  let drag!: ReturnType<typeof createQueueDrag>
  const cleanup = $effect.root(() => {
    drag = createQueueDrag({
      waitingIds: () => ids,
      rowElement: (id) => els.get(id),
      scroller: () => undefined,
      onLift,
      onDrop,
      ...overrides,
    })
  })
  return { drag, onDrop, onLift, cleanup }
}

function press(drag: ReturnType<typeof createQueueDrag>, id: string, y: number): void {
  const grip = document.createElement('span')
  const event = new PointerEvent('pointerdown', { button: 0, clientY: y, pointerId: 1 })
  Object.defineProperty(event, 'currentTarget', { value: grip })
  drag.start(id, event)
}

function pointer(type: 'pointermove' | 'pointerup', y: number): void {
  window.dispatchEvent(new PointerEvent(type, { clientY: y, pointerId: 1 }))
  flushSync()
}

let cleanups: (() => void)[] = []
afterEach(() => {
  for (const cleanup of cleanups) cleanup()
  cleanups = []
})

describe('createQueueDrag', () => {
  it('a press that never travels the threshold is a click: no lift, no drop', () => {
    const { drag, onDrop, onLift, cleanup } = setup()
    cleanups.push(cleanup)
    press(drag, 'a', 20)
    pointer('pointermove', 22)
    pointer('pointerup', 22)
    expect(drag.active).toBe(false)
    expect(onLift).not.toHaveBeenCalled()
    expect(onDrop).not.toHaveBeenCalled()
  })

  it('dragging the first row below the last puts it last, in one call', () => {
    const { drag, onDrop, onLift, cleanup } = setup()
    cleanups.push(cleanup)
    press(drag, 'a', 20)
    pointer('pointermove', 115)
    expect(drag.active).toBe(true)
    expect(drag.draggingId).toBe('a')
    expect(onLift).toHaveBeenCalledWith('a')
    expect(drag.offsetY).toBe(95)
    pointer('pointerup', 115)
    expect(onDrop).toHaveBeenCalledExactlyOnceWith('a', 3)
  })

  it('dragging the last row above the first puts it first', () => {
    const { drag, onDrop, cleanup } = setup()
    cleanups.push(cleanup)
    press(drag, 'c', 100)
    pointer('pointermove', 5)
    pointer('pointerup', 5)
    expect(onDrop).toHaveBeenCalledExactlyOnceWith('c', 1)
  })

  it('shows the drop line in the gap the pointer is over, and none over its own gaps', () => {
    const { drag, cleanup } = setup()
    cleanups.push(cleanup)
    press(drag, 'a', 20)
    pointer('pointermove', 70)
    expect(drag.dropSlot).toBe(2)
    pointer('pointermove', 115)
    expect(drag.dropSlot).toBe(3)
    // Back over its own slot: dropping there would change nothing.
    pointer('pointermove', 30)
    expect(drag.dropSlot).toBeNull()
  })

  it('a drop where it started sends nothing and lets the row go', () => {
    const { drag, onDrop, cleanup } = setup()
    cleanups.push(cleanup)
    press(drag, 'b', 60)
    pointer('pointermove', 70)
    pointer('pointerup', 70)
    expect(onDrop).not.toHaveBeenCalled()
    expect(drag.draggingId).toBeNull()
    expect(drag.offsetY).toBe(0)
  })

  it('a dropped row holds its place until the list changes', () => {
    const { drag, cleanup } = setup()
    cleanups.push(cleanup)
    press(drag, 'a', 20)
    pointer('pointermove', 115)
    pointer('pointerup', 115)
    expect(drag.active).toBe(false)
    expect(drag.draggingId).toBe('a')
    expect(drag.offsetY).toBe(95)
    drag.listChanging()
    expect(drag.draggingId).toBeNull()
    expect(drag.offsetY).toBe(0)
  })

  it('Escape mid-drag puts the row back and sends nothing', () => {
    const { drag, onDrop, cleanup } = setup()
    cleanups.push(cleanup)
    press(drag, 'a', 20)
    pointer('pointermove', 115)
    expect(drag.cancel()).toBe(true)
    pointer('pointerup', 115)
    expect(onDrop).not.toHaveBeenCalled()
    expect(drag.draggingId).toBeNull()
    expect(drag.cancel()).toBe(false)
  })

  it('a row that is not waiting does not start a drag', () => {
    const { drag, onLift, cleanup } = setup()
    cleanups.push(cleanup)
    press(drag, 'running-op', 20)
    pointer('pointermove', 115)
    expect(drag.active).toBe(false)
    expect(onLift).not.toHaveBeenCalled()
  })
})
