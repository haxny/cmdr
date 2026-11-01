import { describe, it, expect } from 'vitest'
import { keepPolicyFromSettings } from './sync-keep-policy'

describe('keepPolicyFromSettings', () => {
  it('builds a target-root policy from the folder name', () => {
    expect(
      keepPolicyFromSettings({
        policy: 'trashElseVersions',
        location: 'targetRoot',
        folderName: '.cmdr-sync/versions',
        customFolder: '/ignored',
      }),
    ).toEqual({ place: 'trashElseVersions', location: { targetRoot: { folderName: '.cmdr-sync/versions' } } })
  })

  it('builds a custom policy from the template', () => {
    expect(
      keepPolicyFromSettings({
        policy: 'versionsAlways',
        location: 'customFolder',
        folderName: '.cmdr-sync/versions',
        customFolder: '/Backups/{profile}',
      }),
    ).toEqual({ place: 'versionsAlways', location: { custom: { template: '/Backups/{profile}' } } })
  })
})
