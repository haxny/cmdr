/**
 * The one-frame-late `aria-activedescendant`. VoiceOver reads only the container ("menu Volume
 * switcher") when focus and the attribute land in the same tick, and reads the row when the
 * attribute follows a frame later (heard on macOS 27.0, VoiceOver 10, 2026-10-15).
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { afterNextFrame, createDeferredActiveDescendant, focusLaidOut } from './deferred-active-descendant.svelte'

beforeEach(() => {
  vi.useFakeTimers()
})

afterEach(() => {
  vi.useRealTimers()
  document.body.innerHTML = ''
})

function focusable(): HTMLElement {
  const el = document.createElement('div')
  el.tabIndex = -1
  document.body.appendChild(el)
  return el
}

describe('afterNextFrame', () => {
  it('runs once, on the next animation frame', () => {
    const run = vi.fn()
    afterNextFrame(run)
    expect(run).not.toHaveBeenCalled()
    vi.advanceTimersToNextFrame()
    expect(run).toHaveBeenCalledTimes(1)
    vi.advanceTimersByTime(1000)
    expect(run).toHaveBeenCalledTimes(1)
  })

  // ❗ WKWebView starves animation frames in a window without focus, and a menu can open in
  // one (an E2E run opens windows unfocused). The id must still land.
  it('still runs when no frame comes', () => {
    const run = vi.fn()
    const raf = vi.spyOn(window, 'requestAnimationFrame').mockImplementation(() => 0)
    afterNextFrame(run)
    vi.advanceTimersByTime(200)
    expect(run).toHaveBeenCalledTimes(1)
    raf.mockRestore()
  })

  it('runs nothing once cancelled', () => {
    const run = vi.fn()
    const cancel = afterNextFrame(run)
    cancel()
    vi.advanceTimersByTime(1000)
    expect(run).not.toHaveBeenCalled()
  })
})

describe('focusLaidOut', () => {
  // ❗ An element focused before layout has no AX object yet: WebKit posts focus on the web
  // area and ignores its `aria-activedescendant` moves, so VoiceOver stays silent.
  it('reads the layout before it focuses, without scrolling', () => {
    const el = focusable()
    const calls: string[] = []
    vi.spyOn(el, 'getBoundingClientRect').mockImplementation(() => {
      calls.push('layout')
      return new DOMRect()
    })
    const focus = vi.spyOn(el, 'focus').mockImplementation(() => calls.push('focus'))
    focusLaidOut(el)
    expect(calls).toEqual(['layout', 'focus'])
    expect(focus).toHaveBeenCalledWith({ preventScroll: true })
  })

  it('lays out before focusing for the deferred descendant too', () => {
    const el = focusable()
    const calls: string[] = []
    vi.spyOn(el, 'getBoundingClientRect').mockImplementation(() => {
      calls.push('layout')
      return new DOMRect()
    })
    vi.spyOn(el, 'focus').mockImplementation(() => calls.push('focus'))
    createDeferredActiveDescendant().focus(el, 'row-1')
    expect(calls).toEqual(['layout', 'focus'])
  })
})

describe('createDeferredActiveDescendant', () => {
  it('names nothing before its element has taken focus', () => {
    const descendant = createDeferredActiveDescendant()
    expect(descendant.of('row-1')).toBeUndefined()
  })

  it('focuses without scrolling, and names the cursor only one frame after focus lands', () => {
    const descendant = createDeferredActiveDescendant()
    const el = focusable()
    const focus = vi.spyOn(el, 'focus')
    descendant.focus(el, 'row-1')
    expect(focus).toHaveBeenCalledWith({ preventScroll: true })
    expect(document.activeElement).toBe(el)
    expect(descendant.of('row-1')).toBeUndefined()
    vi.advanceTimersToNextFrame()
    expect(descendant.of('row-1')).toBe('row-1')
  })

  // A cursor move after focus is a change of its own, which VoiceOver reads like any ↓.
  it('names a cursor that moved after focus at once', () => {
    const descendant = createDeferredActiveDescendant()
    descendant.focus(focusable(), 'row-1')
    expect(descendant.of('row-2')).toBe('row-2')
  })

  it('holds back again after a reset, until the next focus and frame', () => {
    const descendant = createDeferredActiveDescendant()
    const el = focusable()
    descendant.focus(el, 'row-1')
    vi.advanceTimersToNextFrame()
    descendant.reset()
    expect(descendant.of('row-1')).toBeUndefined()
    // A frame left pending by the reset must not release the next hold early.
    descendant.focus(el, 'row-1')
    expect(descendant.of('row-1')).toBeUndefined()
    vi.advanceTimersToNextFrame()
    expect(descendant.of('row-1')).toBe('row-1')
  })

  it('drops a pending frame on reset', () => {
    const descendant = createDeferredActiveDescendant()
    descendant.focus(focusable(), 'row-1')
    descendant.reset()
    vi.advanceTimersByTime(1000)
    expect(descendant.of('row-1')).toBeUndefined()
  })

  // A file list takes focus by more paths than one `.focus()` call (a click lands on it, the
  // browser's mousedown default), so it reports focus from its own `focus` event.
  it('holds an id for a frame when the element reports it took focus some other way', () => {
    const descendant = createDeferredActiveDescendant()
    descendant.tookFocus('row-1')
    expect(descendant.of('row-1')).toBeUndefined()
    vi.advanceTimersToNextFrame()
    expect(descendant.of('row-1')).toBe('row-1')
  })

  // ❗ Entering a folder replaces the cursor row's node under an unchanged id, so WebKit's
  // focus stays on the dead node and VoiceOver says nothing; re-landing the id a frame later
  // made it read the new row (VoiceOver spike, macOS 27.0, VoiceOver 10, 2026-10-15).
  it('holds a renewed id for a frame while focused', () => {
    const descendant = createDeferredActiveDescendant()
    descendant.tookFocus('row-0')
    vi.advanceTimersToNextFrame()
    descendant.renew('row-0')
    expect(descendant.of('row-0')).toBeUndefined()
    vi.advanceTimersToNextFrame()
    expect(descendant.of('row-0')).toBe('row-0')
  })

  it('ignores a renew without focus', () => {
    const descendant = createDeferredActiveDescendant()
    descendant.renew('row-0')
    vi.advanceTimersByTime(1000)
    expect(descendant.of('row-0')).toBeUndefined()
  })

  // A file pane says "Left pane, Documents" through a live region AFTER the row is read, so
  // it waits for the moment the held id lands.
  it('reports each landing of a held id, once, and nothing for a reset hold', () => {
    const onLand = vi.fn()
    const descendant = createDeferredActiveDescendant({ onLand })
    descendant.tookFocus('row-0')
    expect(onLand).not.toHaveBeenCalled()
    vi.advanceTimersToNextFrame()
    expect(onLand).toHaveBeenCalledTimes(1)

    descendant.renew('row-0')
    vi.advanceTimersByTime(1000)
    expect(onLand).toHaveBeenCalledTimes(2)

    descendant.tookFocus('row-0')
    descendant.reset()
    vi.advanceTimersByTime(1000)
    expect(onLand).toHaveBeenCalledTimes(2)
  })
})
