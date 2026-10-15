/**
 * Keyboard focus holds a transient toast the way the pointer does: while focus is
 * inside it, the auto-dismiss timer is off and the ring is frozen; once focus leaves,
 * the toast goes at `max(natural deadline, focus left + HOVER_LEAVE_GRACE_MS)`.
 * Without this a keyboard or screen-reader user could reach Undo only to have the
 * toast vanish under them (WCAG 2.2.1).
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import ToastItem from './ToastItem.svelte'
import { HOVER_LEAVE_GRACE_MS } from './toast-store.svelte'

function mountTransient(onTimeout: (id: string) => void): { toast: HTMLElement; close: HTMLButtonElement } {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(ToastItem, {
    target,
    props: {
      id: 'focus',
      content: 'Moved 3 files to the trash',
      level: 'info',
      dismissal: 'transient',
      timeoutMs: 4000,
      postedAt: Date.now(),
      onTimeout,
      onUserDismiss: vi.fn(),
    },
  })
  const toast = target.querySelector<HTMLElement>('.toast')
  const close = target.querySelector<HTMLButtonElement>('.toast-close')
  if (!toast || !close) throw new Error('Toast frame missing')
  return { toast, close }
}

function outsideButton(): HTMLButtonElement {
  const outside = document.createElement('button')
  document.body.appendChild(outside)
  return outside
}

describe('ToastItem held by keyboard focus', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('stays up past its deadline while focus is inside it', async () => {
    const onTimeout = vi.fn()
    const { close } = mountTransient(onTimeout)
    await tick()

    vi.advanceTimersByTime(1000)
    close.focus()
    vi.advanceTimersByTime(60_000)

    expect(onTimeout).not.toHaveBeenCalled()
  })

  it('goes one grace second after focus leaves, once its deadline has passed', async () => {
    const onTimeout = vi.fn()
    const { close } = mountTransient(onTimeout)
    const outside = outsideButton()
    await tick()

    close.focus()
    vi.advanceTimersByTime(20_000)
    outside.focus()

    vi.advanceTimersByTime(HOVER_LEAVE_GRACE_MS - 1)
    expect(onTimeout).not.toHaveBeenCalled()
    vi.advanceTimersByTime(2)
    expect(onTimeout).toHaveBeenCalledWith('focus')
  })

  it('keeps the rest of its natural window when focus leaves early', async () => {
    const onTimeout = vi.fn()
    const { close } = mountTransient(onTimeout)
    const outside = outsideButton()
    await tick()

    vi.advanceTimersByTime(500)
    close.focus()
    vi.advanceTimersByTime(500)
    outside.focus()

    vi.advanceTimersByTime(2999)
    expect(onTimeout).not.toHaveBeenCalled()
    vi.advanceTimersByTime(2)
    expect(onTimeout).toHaveBeenCalledWith('focus')
  })

  it('stays held while focus is inside even after the pointer leaves', async () => {
    const onTimeout = vi.fn()
    const { toast, close } = mountTransient(onTimeout)
    await tick()

    toast.dispatchEvent(new PointerEvent('pointerenter'))
    close.focus()
    toast.dispatchEvent(new PointerEvent('pointerleave'))
    vi.advanceTimersByTime(60_000)

    expect(onTimeout).not.toHaveBeenCalled()
  })

  it('freezes its countdown ring while focus is inside', async () => {
    const { toast, close } = mountTransient(vi.fn())
    await tick()

    vi.advanceTimersByTime(1000)
    close.focus()
    await tick()

    expect(toast.querySelector('.toast-timer')?.getAttribute('data-state')).toBe('paused')
  })
})

describe('ToastItem under a click', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })

  function mouseDown(target: Element): MouseEvent {
    const event = new MouseEvent('mousedown', { bubbles: true, cancelable: true })
    target.dispatchEvent(event)
    return event
  }

  // Regression: WebKit moved focus to `<body>` on a click on the toast's text, and every pane
  // key died until Tab or a click (VoiceOver listening run, 2026-10-15).
  it('keeps focus where it is when the click lands on its text', async () => {
    const { toast } = mountTransient(vi.fn())
    await tick()

    const message = toast.querySelector('.toast-message')
    if (!message) throw new Error('Toast message missing')
    expect(mouseDown(message).defaultPrevented).toBe(true)
  })

  it('still lets a click focus its buttons', async () => {
    const { close } = mountTransient(vi.fn())
    await tick()

    expect(mouseDown(close).defaultPrevented).toBe(false)
  })
})
