/**
 * The profiles list over a fake IPC: save as asks before replacing a taken name, loading hands back show-filters only
 * when the profile asks for them, and every refusal comes back typed (never thrown at the menu).
 */
import { describe, expect, it, vi } from 'vitest'
import {
  SyncProfileEditFailure,
  SyncProfileSaveFailure,
  type ProfileEntry,
  type SyncProfile,
} from '#lib/tauri-commands/sync.js'
import { createSyncProfiles, profileView, type ProfileSource, type SyncProfilesIpc } from './profiles.svelte'
import { DEFAULT_SHOW_FILTERS, NO_OVERRIDES, defaultSyncOptions } from './sync-options'
import { profile } from './sync-test-fixtures'

const SOURCE: ProfileSource = {
  left: { volumeId: 'root', path: '/a' },
  right: { volumeId: 'root', path: '/b' },
  options: defaultSyncOptions(),
  showFilters: { ...DEFAULT_SHOW_FILTERS, equal: true },
  overrides: { ...NO_OVERRIDES },
}

function setup(entries: ProfileEntry[]) {
  const ipc: SyncProfilesIpc = {
    getSyncProfiles: vi.fn(() => Promise.resolve(entries)),
    saveSyncProfile: vi.fn((p: SyncProfile) => Promise.resolve([...entries, { type: 'valid' as const, profile: p }])),
    deleteSyncProfile: vi.fn(() => Promise.resolve([])),
    renameSyncProfile: vi.fn(() => Promise.resolve(entries)),
    moveSyncProfile: vi.fn(() => Promise.resolve(entries)),
  }
  return { ipc, profiles: createSyncProfiles(ipc, () => 'new-id') }
}

describe('sync profiles', () => {
  it('save as with a taken name asks before replacing', async () => {
    const { ipc, profiles } = setup([{ type: 'valid', profile: profile('p1', 'Photos') }])
    await profiles.refresh()
    const asked = await profiles.saveAs(' photos ', SOURCE)
    expect(asked).toEqual({ type: 'nameTaken', name: 'Photos' })
    expect(ipc.saveSyncProfile).not.toHaveBeenCalled()
    const saved = await profiles.saveAs(' photos ', SOURCE, { replace: true })
    expect(saved).toEqual({ type: 'saved' })
    // Replacing keeps the old profile's id, so it keeps its place in the list.
    expect(vi.mocked(ipc.saveSyncProfile).mock.calls[0][0]).toMatchObject({ id: 'p1', name: 'photos' })
  })

  it('a new name saves at the end with a fresh id and no credentials, only volume ids and paths', async () => {
    const { ipc, profiles } = setup([])
    await profiles.refresh()
    expect(await profiles.saveAs('Backup', SOURCE, { restoreShowFilters: true })).toEqual({ type: 'saved' })
    const saved = vi.mocked(ipc.saveSyncProfile).mock.calls[0][0]
    expect(saved).toEqual({
      id: 'new-id',
      name: 'Backup',
      left: SOURCE.left,
      right: SOURCE.right,
      options: SOURCE.options,
      showFilters: SOURCE.showFilters,
      restoreShowFilters: true,
      overrides: SOURCE.overrides,
    })
    expect(profiles.entries).toHaveLength(1)
  })

  it('loading restores show-filters only when asked', async () => {
    const plain = profile('p1', 'Plain')
    const asks = { ...profile('p2', 'Asks'), restoreShowFilters: true }
    const { profiles } = setup([
      { type: 'valid', profile: plain },
      { type: 'valid', profile: asks },
    ])
    await profiles.refresh()
    expect(profiles.load('p1')?.showFilters).toBeNull()
    expect(profiles.load('p2')?.showFilters).toEqual(asks.showFilters)
    expect(profiles.load('missing')).toBeNull()
  })

  it('an invalid profile never loads, and is listed with its problem', async () => {
    const { profiles } = setup([{ type: 'invalid', id: 'bad', name: 'Broken', problem: { type: 'unreadable' } }])
    await profiles.refresh()
    expect(profiles.load('bad')).toBeNull()
    expect(profileView(profiles.entries[0])).toEqual({
      id: 'bad',
      name: 'Broken',
      problem: { type: 'unreadable' },
    })
  })

  it('refusals come back typed: a full list, a name taken by a rename, a write that did not land', async () => {
    const { ipc, profiles } = setup([{ type: 'valid', profile: profile('p1', 'Photos') }])
    await profiles.refresh()
    vi.mocked(ipc.saveSyncProfile).mockRejectedValueOnce(new SyncProfileSaveFailure({ type: 'listFull', max: 200 }))
    expect(await profiles.saveAs('New', SOURCE)).toEqual({ type: 'refused', refusal: { type: 'listFull', max: 200 } })
    vi.mocked(ipc.renameSyncProfile).mockRejectedValueOnce(new SyncProfileEditFailure({ type: 'nameTaken' }))
    expect(await profiles.rename('p1', 'Other')).toEqual({ type: 'refused', refusal: { type: 'nameTaken' } })
    vi.mocked(ipc.deleteSyncProfile).mockRejectedValueOnce(new SyncProfileEditFailure({ type: 'writeFailed' }))
    expect(await profiles.remove('p1')).toEqual({ type: 'refused', refusal: { type: 'writeFailed' } })
    vi.mocked(ipc.moveSyncProfile).mockRejectedValueOnce(new Error('boom'))
    expect(await profiles.move('p1', 'down')).toEqual({ type: 'refused', refusal: { type: 'unexpected' } })
  })

  it('a rename onto a taken name, once confirmed, saves over the other profile', async () => {
    const photos = profile('p1', 'Photos')
    const { ipc, profiles } = setup([
      { type: 'valid', profile: photos },
      { type: 'valid', profile: profile('p2', 'Music') },
    ])
    await profiles.refresh()
    expect(await profiles.rename('p1', 'music')).toEqual({ type: 'nameTaken', name: 'Music' })
    expect(await profiles.rename('p1', 'music', { replace: true })).toEqual({ type: 'saved' })
    expect(ipc.saveSyncProfile).toHaveBeenCalledWith({ ...photos, name: 'music' })
    expect(ipc.renameSyncProfile).not.toHaveBeenCalled()
  })
})
