/** Tier-3 a11y: the toast a job sent to the background leaves behind. */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

vi.mock('#lib/file-operations/queue/queue-window.js', () => ({ openQueueWindow: () => Promise.resolve() }))
vi.mock('#lib/ui/toast/index.js', () => ({ dismissToast: () => {} }))

import QueueHandoffToastContent from './QueueHandoffToastContent.svelte'

describe('QueueHandoffToastContent a11y', () => {
  it('has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(QueueHandoffToastContent, { target, props: { toastId: 'toast-1', message: 'Sent to the background.' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
