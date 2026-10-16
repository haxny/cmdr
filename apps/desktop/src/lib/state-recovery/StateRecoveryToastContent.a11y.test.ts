/**
 * Tier 3 a11y tests for `StateRecoveryToastContent.svelte`: the three shapes (with a settings
 * button, with a copy, with nothing kept).
 */

import { describe, it, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import type { StateRecovery } from '#lib/ipc/bindings.js'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

vi.mock('#lib/tauri-commands/index.js', () => ({ showInFinder: () => Promise.resolve() }))
vi.mock('#lib/ui/toast/index.js', () => ({ dismissToast: (): void => {} }))
vi.mock('#lib/settings/settings-window.js', () => ({ openSettingsWindow: () => Promise.resolve() }))

import StateRecoveryToastContent from './StateRecoveryToastContent.svelte'

const recoveries: StateRecovery[] = [
  { file: 'settings', keptCopy: '/data/settings.json.broken-2026-10-16T101500' },
  { file: 'operation_log', keptCopy: '/data/operation-log.db.broken-2026-10-16T101500' },
  { file: 'favorites', keptCopy: null },
]

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('StateRecoveryToastContent a11y', () => {
  for (const recovery of recoveries) {
    it(`${recovery.file} has no a11y violations`, async () => {
      const target = document.createElement('div')
      document.body.appendChild(target)
      mount(StateRecoveryToastContent, { target, props: { toastId: 'toast-1', recovery } })
      await tick()
      await expectNoA11yViolations(target)
    })
  }
})
