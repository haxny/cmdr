/**
 * The toast a job sent to the background leaves behind: where it went, and a
 * way to the queue window, which no longer opens by itself.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushSync } from 'svelte'

const openQueueWindow = vi.fn(() => Promise.resolve())
vi.mock('#lib/file-operations/queue/queue-window.js', () => ({
  openQueueWindow: (): Promise<void> => openQueueWindow(),
}))

const dismissToast = vi.fn<(id: string) => void>()
vi.mock('#lib/ui/toast/index.js', () => ({
  dismissToast: (id: string): void => {
    dismissToast(id)
  },
}))

import QueueHandoffToastContent from './QueueHandoffToastContent.svelte'

let target: HTMLElement

beforeEach(() => {
  document.body.innerHTML = ''
  openQueueWindow.mockClear()
  dismissToast.mockClear()
  target = document.createElement('div')
  document.body.appendChild(target)
  mount(QueueHandoffToastContent, { target, props: { toastId: 'toast-1', message: 'Sent to the background.' } })
  flushSync()
})

describe('QueueHandoffToastContent', () => {
  it('says where the job went', () => {
    expect(target.textContent).toContain('Sent to the background.')
  })

  it('opens the queue window, focused, and gets out of the way', () => {
    const action = [...target.querySelectorAll('button')].find((b) => b.textContent.includes('Show in operation queue'))
    action?.click()
    flushSync()
    expect(openQueueWindow).toHaveBeenCalledTimes(1)
    expect(dismissToast).toHaveBeenCalledWith('toast-1')
  })
})
