/**
 * Reaching the toasts from the keyboard: "Move focus to the notifications" lands on the newest
 * toast, the arrows walk every toast and its buttons, Enter and Space stay with the focused
 * button, and Escape goes back where focus came from.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import ToastContainer from './ToastContainer.svelte'
import ToastIdFixture from './toast-id-fixture.svelte'
import { addToast, clearAllToasts, getToasts } from './toast-store.svelte'
import { focusNewestToast, toastStackOwnsKey } from './toast-focus'

let component: ReturnType<typeof mount> | null = null

beforeEach(() => {
  clearAllToasts()
})

afterEach(() => {
  if (component) void unmount(component)
  component = null
  clearAllToasts()
  document.body.innerHTML = ''
})

async function setup(): Promise<{ target: HTMLElement; origin: HTMLButtonElement }> {
  const origin = document.createElement('button')
  origin.textContent = 'Files'
  document.body.appendChild(origin)
  origin.focus()
  const target = document.createElement('div')
  document.body.appendChild(target)
  component = mount(ToastContainer, { target })
  await tick()
  return { target, origin }
}

/** What has focus, the way a person would name it. */
function focused(): string {
  const active = document.activeElement
  if (!(active instanceof HTMLElement)) return ''
  if (active.classList.contains('toast')) return `toast: ${active.getAttribute('aria-label') ?? ''}`
  return active.getAttribute('aria-label') ?? active.textContent.trim()
}

function press(key: string): KeyboardEvent {
  const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true })
  document.activeElement?.dispatchEvent(event)
  return event
}

async function settle(): Promise<void> {
  await tick()
  await new Promise((resolve) => setTimeout(resolve, 0))
  await tick()
}

describe('Move focus to the notifications', () => {
  it('lands on the newest toast, named by what it says', async () => {
    await setup()
    addToast('Copied 3 files.', { dismissal: 'persistent' })
    addToast('Moved 2 files to the trash.', { dismissal: 'persistent' })
    await tick()

    expect(focusNewestToast()).toBe(true)

    expect(focused()).toBe('toast: Moved 2 files to the trash.')
    expect(document.activeElement?.getAttribute('role')).toBe('group')
  })

  it('says there is nothing to reach when no toast is up, and leaves focus alone', async () => {
    const { target, origin } = await setup()

    expect(focusNewestToast()).toBe(false)
    await settle()
    await new Promise((resolve) => setTimeout(resolve, 150))

    expect(document.activeElement).toBe(origin)
    expect(target.querySelector('[data-toast-announcer="polite"]')?.textContent).toBe('No notifications')
  })

  it('holds a transient toast while focus is on it', async () => {
    vi.useFakeTimers()
    try {
      await setup()
      addToast('Copied 3 files.', { timeoutMs: 4000 })
      await tick()

      focusNewestToast()
      vi.advanceTimersByTime(60_000)

      expect(getToasts()).toHaveLength(1)
    } finally {
      vi.useRealTimers()
    }
  })
})

describe('walking the toasts', () => {
  it('↓ walks from the newest toast through its buttons to the older ones, and ↑ walks back', async () => {
    await setup()
    addToast('Copied 3 files.', { dismissal: 'persistent' })
    addToast(ToastIdFixture, { id: 'newest', dismissal: 'persistent' })
    await tick()
    focusNewestToast()

    const walk: string[] = [focused()]
    for (let i = 0; i < 4; i++) {
      press('ArrowDown')
      walk.push(focused())
    }
    expect(walk).toEqual([
      expect.stringMatching(/^toast: /),
      'Dismiss notification',
      'Dismiss notification',
      'toast: Copied 3 files.',
      'Dismiss notification',
    ])

    press('ArrowUp')
    expect(focused()).toBe('toast: Copied 3 files.')
  })

  it('claims the arrow keys it acts on, so no pane command runs behind them', async () => {
    await setup()
    addToast('Copied 3 files.', { dismissal: 'persistent' })
    await tick()
    focusNewestToast()

    expect(press('ArrowDown').defaultPrevented).toBe(true)
  })

  it('keeps Enter and Space for the focused button, away from the command dispatcher', async () => {
    await setup()
    addToast('Copied 3 files.', { dismissal: 'persistent' })
    await tick()
    focusNewestToast()
    press('ArrowDown')

    expect(toastStackOwnsKey('Enter')).toBe(true)
    expect(toastStackOwnsKey('Space')).toBe(true)
    expect(toastStackOwnsKey('Tab')).toBe(false)
    expect(toastStackOwnsKey('⌘C')).toBe(false)
  })

  it('owns no key while focus is outside the stack', async () => {
    await setup()
    addToast('Copied 3 files.', { dismissal: 'persistent' })
    await tick()

    expect(toastStackOwnsKey('Enter')).toBe(false)
  })
})

describe('leaving the toasts', () => {
  it('Escape goes back where focus came from', async () => {
    const { origin } = await setup()
    addToast('Copied 3 files.', { dismissal: 'persistent' })
    await tick()
    focusNewestToast()
    press('ArrowDown')

    const escape = press('Escape')

    expect(escape.defaultPrevented).toBe(true)
    expect(document.activeElement).toBe(origin)
  })

  it('closing the focused toast moves focus to the newest one left', async () => {
    await setup()
    addToast('Copied 3 files.', { dismissal: 'persistent' })
    addToast('Moved 2 files to the trash.', { dismissal: 'persistent' })
    await tick()
    focusNewestToast()
    press('ArrowDown')

    ;(document.activeElement as HTMLButtonElement).click()
    await settle()

    expect(focused()).toBe('toast: Copied 3 files.')
  })

  it('closing the last focused toast goes back where focus came from', async () => {
    const { origin } = await setup()
    addToast('Copied 3 files.', { dismissal: 'persistent' })
    await tick()
    focusNewestToast()
    press('ArrowDown')

    ;(document.activeElement as HTMLButtonElement).click()
    await settle()

    expect(document.activeElement).toBe(origin)
  })
})
