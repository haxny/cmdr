/**
 * Toggling the function key bar re-places a toast stack that sits above it, at once: the
 * container follows the `appearance.showFunctionKeyBar` setting (subscribed, not polled) and
 * asks the placement watcher to measure again.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { flushSync, mount, tick, unmount } from 'svelte'

const placement = vi.hoisted(() => ({ remeasure: vi.fn(), stop: vi.fn() }))
vi.mock('./toast-placement', () => ({
  watchToastPlacement: () => ({ stop: placement.stop, remeasure: placement.remeasure }),
}))

vi.mock('#lib/settings/reactive-settings.svelte.js', async () => {
  const { showBar } = await import('./toast-fkeybar-fixture.svelte')
  return {
    getKeepNotificationsUntilClosed: () => false,
    getShowFunctionKeyBar: () => showBar.show,
  }
})

import ToastContainer from './ToastContainer.svelte'
import { showBar } from './toast-fkeybar-fixture.svelte'
import { addToast, clearAllToasts } from './toast-store.svelte'

let component: ReturnType<typeof mount> | null = null

beforeEach(() => {
  clearAllToasts()
  showBar.show = true
})

afterEach(() => {
  if (component) void unmount(component)
  component = null
  clearAllToasts()
  document.body.innerHTML = ''
})

describe('the function key bar toggling under the toasts', () => {
  it('re-places the stack as soon as the bar is shown or hidden', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    component = mount(ToastContainer, { target })
    addToast('Copied 3 files.', { dismissal: 'persistent' })
    await tick()
    placement.remeasure.mockClear()

    showBar.show = false
    flushSync()
    expect(placement.remeasure).toHaveBeenCalledTimes(1)

    showBar.show = true
    flushSync()
    expect(placement.remeasure).toHaveBeenCalledTimes(2)
  })
})
