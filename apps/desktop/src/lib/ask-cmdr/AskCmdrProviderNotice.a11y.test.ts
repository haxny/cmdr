/**
 * Tier 3 a11y tests for `AskCmdrProviderNotice.svelte`, plus the link's one job.
 *
 * The yellow "no AI provider is ready" strip at the top of the rail. What this guards is the
 * warning tint's contrast and the settings link staying a real, named button, for every gap.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import type { ProviderReadiness } from '#lib/tauri-commands/index.js'

const { openSettingsWindow } = vi.hoisted(() => ({
  openSettingsWindow: vi.fn<(...args: unknown[]) => Promise<void>>(() => Promise.resolve()),
}))
vi.mock('#lib/settings/settings-window.js', () => ({ openSettingsWindow }))

import AskCmdrProviderNotice from './AskCmdrProviderNotice.svelte'

const GAPS: Exclude<ProviderReadiness, 'ready'>[] = [
  'off',
  'cloudNeedsKey',
  'cloudNeedsEndpoint',
  'localNotSetUp',
  'localNotRunning',
]

function mountNotice(gap: Exclude<ProviderReadiness, 'ready'>): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AskCmdrProviderNotice, { target, props: { gap } })
  return target
}

beforeEach(() => {
  document.body.innerHTML = ''
  openSettingsWindow.mockClear()
})

describe('AskCmdrProviderNotice a11y', () => {
  for (const gap of GAPS) {
    it(`the ${gap} notice has no a11y violations`, async () => {
      const target = mountNotice(gap)
      await tick()
      await expectNoA11yViolations(target)
    })
  }

  it('its link opens Settings > AI > Provider', async () => {
    const target = mountNotice('cloudNeedsKey')
    await tick()
    target.querySelector('button')?.click()
    expect(openSettingsWindow).toHaveBeenCalledWith('ask-cmdr-provider-notice', ['AI', 'Provider'])
  })
})
