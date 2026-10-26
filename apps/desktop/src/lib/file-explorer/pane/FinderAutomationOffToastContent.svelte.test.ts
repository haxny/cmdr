/**
 * Tests for Get info's "Cmdr can't control Finder" toast body: it says what to
 * switch, and its button opens the Automation pane of System Settings.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushSync } from 'svelte'
import { tString } from '#lib/intl/messages.svelte.js'
import { systemStrings } from '#lib/system-strings.svelte.js'

const { openAutomationSettings } = vi.hoisted(() => ({
  openAutomationSettings: vi.fn<() => Promise<void>>(() => Promise.resolve()),
}))
vi.mock('#lib/tauri-commands/index.js', () => ({ openAutomationSettings }))

import FinderAutomationOffToastContent from './FinderAutomationOffToastContent.svelte'

function render() {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(FinderAutomationOffToastContent, { target, props: {} })
  flushSync()
  const button = target.querySelector('button')
  if (!button) throw new Error('the toast has no button to press')
  return { target, button }
}

beforeEach(() => {
  openAutomationSettings.mockClear()
})

describe('FinderAutomationOffToastContent', () => {
  it('says what to switch back on', () => {
    const { target } = render()

    expect(target.textContent).toContain(tString('commands.handler.getInfo.automationOff', { ...systemStrings }))
  })

  it('opens the Automation pane of System Settings', () => {
    const { button } = render()

    expect(button.textContent).toContain(
      tString('commands.handler.getInfo.openAutomationSettings', { automation: systemStrings.automation }),
    )
    button.click()
    expect(openAutomationSettings).toHaveBeenCalledOnce()
  })

  describe('on a Mac set to another language', () => {
    // The pane names follow the Mac's own language (what System Settings shows), not Cmdr's.
    const english = { ...systemStrings }
    afterEach(() => {
      Object.assign(systemStrings, english)
    })

    it('names the panes the way System Settings shows them', () => {
      Object.assign(systemStrings, {
        systemSettings: 'Rendszerbeállítások',
        privacyAndSecurity: 'Adatvédelem és biztonság',
        automation: 'Automatizálás',
      })
      const { target, button } = render()

      expect(target.textContent).toContain('Rendszerbeállítások > Adatvédelem és biztonság > Automatizálás')
      expect(button.textContent).toContain('Automatizálás')
    })
  })
})
