/**
 * "Keep notifications until I close them" (`behavior.notifications.keepUntilClosed`): with it on,
 * no toast goes on a timer, so a slower reader or a screen-reader user can take their time
 * (WCAG 2.2.1). The toast store is untouched: pane navigation still clears a pane's own stale
 * toasts.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'

const settings = vi.hoisted(() => ({ keepUntilClosed: false }))
vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({
  getKeepNotificationsUntilClosed: () => settings.keepUntilClosed,
  getShowFunctionKeyBar: () => true,
}))

import ToastContainer from './ToastContainer.svelte'
import { addToast, clearAllToasts, getToasts } from './toast-store.svelte'

let component: ReturnType<typeof mount> | null = null

beforeEach(() => {
  clearAllToasts()
  vi.useFakeTimers()
})

afterEach(() => {
  if (component) void unmount(component)
  component = null
  clearAllToasts()
  vi.useRealTimers()
  document.body.innerHTML = ''
})

async function setup(): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  component = mount(ToastContainer, { target })
  await tick()
  return target
}

describe('Keep notifications until I close them', () => {
  it('lets a transient toast go on its timer while off', async () => {
    settings.keepUntilClosed = false
    await setup()
    addToast('Copied 3 files.', { timeoutMs: 4000 })
    await tick()

    vi.advanceTimersByTime(4001)

    expect(getToasts()).toHaveLength(0)
  })

  it('keeps every toast up, with no countdown ring, while on', async () => {
    settings.keepUntilClosed = true
    const target = await setup()
    addToast('Copied 3 files.', { timeoutMs: 4000 })
    await tick()

    vi.advanceTimersByTime(600_000)

    expect(getToasts()).toHaveLength(1)
    expect(target.querySelector('.toast-timer')).toBeNull()
  })
})
