import { describe, expect, it, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '#lib/intl/locale.js'
import type { VolumeInfo } from '#lib/file-explorer/types.js'
import { deviceRowDefersToMtpTwin, deviceVolumeLabel, withUnnamedPhonesNamed } from './adb-volume-label'

// The suffixed label resolves through the i18n catalog (`tString`); pin the
// base locale so the asserted en copy is deterministic.
beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

function volume(overrides: Partial<VolumeInfo> & Pick<VolumeInfo, 'id' | 'name'>): VolumeInfo {
  return {
    path: `mtp://${overrides.id}`,
    category: 'mobile_device',
    isEjectable: true,
    mountIsReadOnly: false,
    ...overrides,
  }
}

const ADB_PIXEL = volume({ id: 'adb-R58M12345', name: 'Pixel 9', path: 'adb://R58M12345', fsType: 'adb' })
const MTP_PIXEL = volume({ id: 'mtp-dev:65537', name: 'Pixel 9' })

describe('deviceRowDefersToMtpTwin', () => {
  const WAITING = { ...ADB_PIXEL, deviceReadiness: { kind: 'waiting_for_authorization' } as const }
  const OFFLINE = { ...ADB_PIXEL, deviceReadiness: { kind: 'unavailable', reason: 'offline' } as const }
  const READY = { ...ADB_PIXEL, deviceReadiness: { kind: 'ready' } as const }

  it('defers an ADB row that can not be used yet to the MTP row of the same phone', () => {
    expect(deviceRowDefersToMtpTwin(WAITING, [WAITING, MTP_PIXEL])).toBe(true)
    expect(deviceRowDefersToMtpTwin(OFFLINE, [OFFLINE, MTP_PIXEL])).toBe(true)
  })

  it('keeps an ADB row that is ready: one row per protocol', () => {
    expect(deviceRowDefersToMtpTwin(READY, [READY, MTP_PIXEL])).toBe(false)
  })

  it('keeps a waiting row with no twin: it is the only way to see that phone', () => {
    expect(deviceRowDefersToMtpTwin(WAITING, [WAITING])).toBe(false)
  })

  it('never defers the MTP row', () => {
    expect(deviceRowDefersToMtpTwin(MTP_PIXEL, [WAITING, MTP_PIXEL])).toBe(false)
  })

  it('leaves one row for a phone whose MTP session is still connecting: its words cover both prompts', () => {
    const connecting = { ...MTP_PIXEL, id: 'mtp-dev:waiting', deviceReadiness: { kind: 'connecting' } as const }
    expect(deviceRowDefersToMtpTwin(WAITING, [WAITING, connecting])).toBe(true)
    expect(deviceRowDefersToMtpTwin(connecting, [WAITING, connecting])).toBe(false)
  })
})

describe('withUnnamedPhonesNamed', () => {
  const NAMELESS = volume({ id: 'adb-r58', name: 'R58M12345', path: 'adb://R58M12345', fsType: 'adb' })

  it('never lets a serial stand in for a phone’s name', () => {
    const [named] = withUnnamedPhonesNamed([NAMELESS])
    expect(named.name).toBe('Android phone')
    expect(named.path).toBe('adb://R58M12345')
    expect(named.id).toBe('adb-r58')
  })

  it('leaves a phone with a real name alone', () => {
    expect(withUnnamedPhonesNamed([ADB_PIXEL])[0].name).toBe('Pixel 9')
  })

  it('leaves everything that is not an ADB row alone, even one named like its path', () => {
    const disk = volume({ id: 'disk', name: 'disk', path: '/Volumes/disk', category: 'attached_volume' })
    const mtp = volume({ id: 'mtp-a:1', name: 'a', path: 'mtp://a/1' })
    expect(withUnnamedPhonesNamed([disk, mtp]).map((v) => v.name)).toEqual(['disk', 'a'])
  })

  it('hands back the same list when there is nothing to name, so consumers keep their identity', () => {
    const list = [ADB_PIXEL, MTP_PIXEL]
    expect(withUnnamedPhonesNamed(list)).toBe(list)
  })
})

describe('deviceVolumeLabel', () => {
  it('suffixes the ADB entry when an MTP twin shares its name', () => {
    expect(deviceVolumeLabel(ADB_PIXEL, [ADB_PIXEL, MTP_PIXEL])).toBe('Pixel 9 (ADB)')
  })

  it('leaves a phone reachable only over ADB with its plain name', () => {
    expect(deviceVolumeLabel(ADB_PIXEL, [ADB_PIXEL])).toBe('Pixel 9')
  })

  it('never suffixes the MTP half, even when the twin is there', () => {
    expect(deviceVolumeLabel(MTP_PIXEL, [ADB_PIXEL, MTP_PIXEL])).toBe('Pixel 9')
  })

  it('does not pair devices with different names', () => {
    const otherPhone = volume({ id: 'mtp-dev:65538', name: 'Galaxy S24' })
    expect(deviceVolumeLabel(ADB_PIXEL, [ADB_PIXEL, otherPhone])).toBe('Pixel 9')
  })

  it('does not treat a second ADB volume as an MTP twin', () => {
    const adbTwin = volume({ id: 'adb-OTHER', name: 'Pixel 9', path: 'adb://OTHER', fsType: 'adb' })
    expect(deviceVolumeLabel(ADB_PIXEL, [ADB_PIXEL, adbTwin])).toBe('Pixel 9')
  })

  it('does not pair with a same-named volume that is not a device', () => {
    const disk = volume({ id: 'ext', name: 'Pixel 9', path: '/Volumes/Pixel 9', category: 'attached_volume' })
    expect(deviceVolumeLabel(ADB_PIXEL, [ADB_PIXEL, disk])).toBe('Pixel 9')
  })
})
