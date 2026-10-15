/**
 * The one-frame-late `aria-activedescendant`. VoiceOver reads only the container ("menu Volume
 * switcher") when focus and the attribute land in the same tick, and reads the row when the
 * attribute follows a frame later (heard on macOS 27.0, VoiceOver 10, 2026-10-15).
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { afterNextFrame, createDeferredActiveDescendant } from './deferred-active-descendant.svelte'

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
})
