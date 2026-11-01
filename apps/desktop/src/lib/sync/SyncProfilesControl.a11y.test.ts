/** Tier 3 a11y tests for `SyncProfilesControl.svelte`: the button, and the menu open with a greyed (invalid) profile. */
import { describe, expect, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SyncProfilesControl from './SyncProfilesControl.svelte'
import { createSyncProfiles } from './profiles.svelte'
import { fakeSyncIpc, profile, settle } from './sync-test-fixtures'
import { DEFAULT_SHOW_FILTERS, NO_OVERRIDES, defaultSyncOptions } from './sync-options'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

async function mountControl() {
  const fake = fakeSyncIpc()
  fake.profiles.push(
    { type: 'valid', profile: profile('p1', 'Photos') },
    { type: 'invalid', id: 'bad', name: 'Old backup', problem: { type: 'unreadable' } },
  )
  const profiles = createSyncProfiles(fake.ipc)
  const target = document.createElement('div')
  document.body.appendChild(target)
  const control = mount(SyncProfilesControl, {
    target,
    props: {
      profiles,
      loadedId: 'p1',
      source: () => ({
        left: { volumeId: 'root', path: '/a' },
        right: { volumeId: 'root', path: '/b' },
        options: defaultSyncOptions(),
        showFilters: { ...DEFAULT_SHOW_FILTERS },
        overrides: { ...NO_OVERRIDES },
      }),
      onLoad: vi.fn(),
    },
  })
  await settle()
  await tick()
  return { target, control: control as unknown as { toggleMenu: () => void } }
}

describe('SyncProfilesControl a11y', () => {
  it('closed has no violations', async () => {
    const { target } = await mountControl()
    await expectNoA11yViolations(target)
  })

  it('open, with an invalid profile, has no violations and says why it is greyed', async () => {
    const { target, control } = await mountControl()
    control.toggleMenu()
    await settle()
    await tick()
    const menu = target.ownerDocument.querySelector('[role="menu"]')
    expect(menu?.textContent).toContain('Old backup: this version of Cmdr can’t read it')
    await expectNoA11yViolations(target.ownerDocument.body)
  })
})
