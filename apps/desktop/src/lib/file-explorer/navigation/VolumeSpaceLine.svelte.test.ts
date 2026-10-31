/**
 * What a switcher row's space line renders. The state machine behind it is
 * `volume-space-manager.test.ts`; this pins which of its states a row shows.
 */
import { describe, it, expect, beforeAll, afterAll, afterEach } from 'vitest'
import { mount, unmount, flushSync } from 'svelte'
import { _setLocaleForTests } from '#lib/intl/locale.js'
import VolumeSpaceLine from './VolumeSpaceLine.svelte'
import { createVolumeSpaceManager, type VolumeSpaceManager } from './volume-space-manager.svelte'
import type { VolumeInfo } from '../types'

const SPACE = { kind: 'bounded', totalBytes: 1_000_000, availableBytes: 500_000, usedBytes: 500_000 } as const

const PHONE: VolumeInfo = {
  id: 'adb-phone',
  name: 'Pixel 9 Pro XL',
  path: 'adb://phone',
  category: 'mobile_device',
  isEjectable: true,
  fsType: 'adb',
}
const DISK: VolumeInfo = {
  id: 'disk',
  name: 'Disk',
  path: '/Volumes/Disk',
  category: 'attached_volume',
  isEjectable: true,
}

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

let mounted: ReturnType<typeof mount> | undefined

function render(volume: VolumeInfo, spaceManager: VolumeSpaceManager): HTMLElement {
  const target = document.createElement('div')
  mounted = mount(VolumeSpaceLine, { target, props: { volume, spaceManager } })
  flushSync()
  return target
}

afterEach(() => {
  if (mounted) void unmount(mounted)
  mounted = undefined
})

describe('VolumeSpaceLine', () => {
  it('shows a phone its figure', () => {
    const manager = createVolumeSpaceManager()
    manager.volumeSpaceMap.set(PHONE.id, SPACE)
    expect(render(PHONE, manager).querySelector('.volume-space-info')).not.toBeNull()
  })

  it('shows no dashed bar or "Unavailable" on a phone whose query came up empty', () => {
    const manager = createVolumeSpaceManager()
    manager.spaceTimedOutSet.add(PHONE.id)
    const line = render(PHONE, manager)
    expect(line.querySelector('.volume-space-info')).toBeNull()
    expect(line.textContent).not.toContain('Unavailable')
  })

  it('shows no retry spinner on a phone either', () => {
    const manager = createVolumeSpaceManager()
    manager.spaceRetryingSet.add(PHONE.id)
    expect(render(PHONE, manager).querySelector('.volume-space-info')).toBeNull()
  })

  it('keeps the click-to-retry placeholder on a disk', () => {
    const manager = createVolumeSpaceManager()
    manager.spaceTimedOutSet.add(DISK.id)
    expect(render(DISK, manager).textContent).toContain('Unavailable')
  })

  it('keeps the retry spinner on a disk', () => {
    const manager = createVolumeSpaceManager()
    manager.spaceRetryingSet.add(DISK.id)
    expect(render(DISK, manager).querySelector('.volume-space-info')).not.toBeNull()
  })
})
