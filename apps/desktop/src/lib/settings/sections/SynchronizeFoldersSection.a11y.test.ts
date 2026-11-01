/**
 * Tier 3 a11y tests for `SynchronizeFoldersSection.svelte`: all eight rows render, and axe finds nothing in
 * the default state, with the custom folder chosen, and with a problem shown under the versions folder field.
 * The settings store and the backend check are mocked so it mounts without either.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

const { values, check } = vi.hoisted(() => {
  const stored: Record<string, string | number | boolean> = {
    'sync.keep.policy': 'trashElseVersions',
    'sync.keep.location': 'targetRoot',
    'sync.keep.folderName': '.cmdr-sync/versions',
    'sync.keep.customFolder': '',
    'sync.compare.timeTolerance': 2,
    'sync.compare.ignoreHourShift': false,
    'sync.guards.maxRemovePercent': 50,
    'sync.guards.maxRemoveCount': 0,
  }
  return { values: stored, check: vi.fn<(policy: unknown) => Promise<string | null>>() }
})

vi.mock('#lib/settings/settings-store.js', () => ({
  getSetting: vi.fn((key: string): string | number | boolean | undefined => values[key]),
  setSetting: vi.fn(() => Promise.resolve()),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))
vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  checkSyncKeepPolicy: check,
}))

import SynchronizeFoldersSection from './SynchronizeFoldersSection.svelte'

const ids = Object.keys(values)

function mountSection(): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(SynchronizeFoldersSection, { target, props: { searchQuery: '' } })
  return target
}

describe('SynchronizeFoldersSection a11y', () => {
  beforeEach(() => {
    values['sync.keep.location'] = 'targetRoot'
    check.mockReset().mockResolvedValue(null)
  })
  afterEach(() => {
    document.body.innerHTML = ''
  })

  it('renders all eight rows and has no a11y violations', async () => {
    const target = mountSection()
    await tick()
    for (const id of ids) {
      expect(target.querySelector(`[id="setting-${id}"]`), id).not.toBeNull()
    }
    await expectNoA11yViolations(target)
  })

  it('has no a11y violations with a custom folder chosen', async () => {
    values['sync.keep.location'] = 'customFolder'
    const target = mountSection()
    await tick()
    await expectNoA11yViolations(target)
  })

  it('shows the problem the backend names, under the field', async () => {
    check.mockResolvedValue('badTemplate')
    const target = mountSection()
    await tick()
    await tick()
    const alert = target.querySelector('[role="alert"]')
    expect(alert?.textContent).toContain('.cmdr-sync/')
    await expectNoA11yViolations(target)
  })
})
