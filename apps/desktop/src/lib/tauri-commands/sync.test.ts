/** The Synchronize folders wrappers: a refused save keeps its typed reason instead of collapsing into a string. */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('#lib/ipc/bindings.js', () => ({
  commands: {
    getSyncProfiles: vi.fn(),
    saveSyncProfile: vi.fn(),
    checkSyncKeepPolicy: vi.fn(),
  },
}))

import { commands } from '#lib/ipc/bindings.js'
import { asProfileSaveRefusal, checkSyncKeepPolicy, saveSyncProfile, SyncProfileSaveFailure } from './sync'
import type { SyncProfile } from './sync'

const mocked = commands as unknown as {
  getSyncProfiles: ReturnType<typeof vi.fn>
  saveSyncProfile: ReturnType<typeof vi.fn>
  checkSyncKeepPolicy: ReturnType<typeof vi.fn>
}

const profile = { id: 'a', name: 'Photos' } as SyncProfile

describe('sync wrappers', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('answers the list after a save', async () => {
    mocked.saveSyncProfile.mockResolvedValue({ status: 'ok', data: [{ type: 'valid', profile }] })
    await expect(saveSyncProfile(profile)).resolves.toEqual([{ type: 'valid', profile }])
  })

  it('throws a typed failure when a save is refused', async () => {
    mocked.saveSyncProfile.mockResolvedValue({ status: 'error', error: { type: 'listFull', max: 200 } })
    const caught = await saveSyncProfile(profile).catch((e: unknown) => e)
    expect(caught).toBeInstanceOf(SyncProfileSaveFailure)
    expect(asProfileSaveRefusal(caught)).toEqual({ type: 'listFull', max: 200 })
  })

  it('passes the keep check through', async () => {
    mocked.checkSyncKeepPolicy.mockResolvedValue('badTemplate')
    await expect(checkSyncKeepPolicy({ place: 'trashOnly', location: { custom: { template: '' } } })).resolves.toBe(
      'badTemplate',
    )
  })
})
