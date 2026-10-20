/**
 * The NFS half of the add form: the export, and the uid and gid every request acts as.
 *
 * ❗ The guess is a starting value, ❌ never the person's: a uid someone typed is
 * theirs, and a later guess (a new export picked) must not overwrite it.
 */

import { describe, expect, it } from 'vitest'
import { parseServerAddress } from './address-parser'
import {
  emptyNfsFields,
  nfsFieldsFromAppPath,
  nfsIdentityIsComplete,
  nfsIdentityWarns,
  parseNfsId,
  withGuessedIdentity,
  withTypedIdentity,
} from './nfs-form'
import type { SavedServer } from '#lib/ipc/bindings.js'
import {
  applyParsedAddress,
  emptyServerForm,
  formFromNfsPlace,
  formFromPrefill,
  nameFallbackOf,
  serverTargetFrom,
} from './server-form'

describe('parseNfsId', () => {
  it.each([
    ['1000', 1000],
    ['0', 0],
    [' 501 ', 501],
    ['4294967295', 4294967295],
  ])('reads %j as %d', (text, id) => {
    expect(parseNfsId(text)).toBe(id)
  })

  it.each([[''], ['-1'], ['1.5'], ['1e3'], ['4294967296'], ['abc'], ['10 00']])('refuses %j', (text) => {
    expect(parseNfsId(text)).toBeNull()
  })
})

describe('the guessed identity', () => {
  const guess = { uid: 1000, gid: 100, source: 'most_common_owner' as const }

  it('fills empty fields, and says where the guess came from', () => {
    expect(withGuessedIdentity(emptyNfsFields(), guess)).toMatchObject({
      uid: '1000',
      gid: '100',
      identityFromGuess: true,
      identitySource: 'most_common_owner',
    })
  })

  it('replaces an earlier guess, since nobody chose that one', () => {
    const first = withGuessedIdentity(emptyNfsFields(), { uid: 501, gid: 20, source: 'this_mac' })
    expect(withGuessedIdentity(first, guess)).toMatchObject({ uid: '1000', identitySource: 'most_common_owner' })
  })

  it('never overwrites what the person typed', () => {
    const typed = withTypedIdentity(emptyNfsFields(), { uid: '1026' })
    expect(typed).toMatchObject({ uid: '1026', identityFromGuess: false, identitySource: null })
    expect(withGuessedIdentity(typed, guess)).toEqual(typed)
  })

  it('is complete once both ids read as ids', () => {
    expect(nfsIdentityIsComplete(emptyNfsFields())).toBe(false)
    expect(nfsIdentityIsComplete(withGuessedIdentity(emptyNfsFields(), guess))).toBe(true)
    expect(nfsIdentityIsComplete(withTypedIdentity(withGuessedIdentity(emptyNfsFields(), guess), { gid: 'x' }))).toBe(
      false,
    )
  })

  it('warns about user ID 0, which most servers squash to nobody, and blocks nothing', () => {
    const root = withTypedIdentity(emptyNfsFields(), { uid: '0', gid: '0' })
    expect(nfsIdentityWarns(root)).toBe(true)
    expect(nfsIdentityIsComplete(root)).toBe(true)
    expect(nfsIdentityWarns(withTypedIdentity(emptyNfsFields(), { uid: '1000' }))).toBe(false)
  })
})

/** A form with NFS selected, an address typed, and an identity filled. */
function nfsForm(address: string) {
  const form = { ...emptyServerForm(), protocol: 'nfs' as const, address }
  const parsed = applyParsedAddress(form, parseServerAddress(address))
  return { ...parsed, nfs: withTypedIdentity(parsed.nfs, { uid: '1000', gid: '100' }) }
}

describe('an NFS address in the form', () => {
  it('fills the export from the path the address carried, whole', () => {
    expect(nfsForm('nfs://nas.local/volume1/data/photos').nfs.exportPath).toBe('/volume1/data/photos')
    expect(nfsForm('nas.local:/Public').nfs.exportPath).toBe('/Public')
  })

  it('keeps a picked export when the address carries no path', () => {
    const form = { ...emptyServerForm(), protocol: 'nfs' as const, nfs: { ...emptyNfsFields(), exportPath: '/Public' } }
    expect(applyParsedAddress(form, parseServerAddress('nas.local')).nfs.exportPath).toBe('/Public')
  })

  it('takes no path from an address that names another protocol', () => {
    expect(nfsForm('sftp://nas.local/srv').nfs.exportPath).toBe('')
  })
})

describe('serverTargetFrom: NFS', () => {
  it('sends the host, the typed port or none, the whole export path, and the identity', () => {
    expect(serverTargetFrom(nfsForm('nfs://nas.local/volume1/data/photos'))).toEqual({
      protocol: 'nfs',
      displayName: '',
      host: 'nas.local',
      port: null,
      export: '/volume1/data/photos',
      uid: 1000,
      gid: 100,
      startFolder: null,
      autoReconnect: true,
    })
    expect(serverTargetFrom(nfsForm('nfs://nas.local:15481/insecure'))).toMatchObject({
      port: 15481,
      export: '/insecure',
    })
    expect(serverTargetFrom(nfsForm('nas.local:2049/Public'))).toMatchObject({ port: 2049 })
  })

  it("drops a port another protocol's scheme named", () => {
    const form = { ...nfsForm('sftp://nas.local:2222'), nfs: { ...nfsForm('x').nfs, exportPath: '/Public' } }
    expect(serverTargetFrom(form)).toMatchObject({ host: 'nas.local', port: null })
  })

  it('names no target while the export or an id is missing', () => {
    expect(serverTargetFrom(nfsForm('nfs://nas.local'))).toBeNull()
    const noUid = nfsForm('nfs://nas.local/Public')
    expect(serverTargetFrom({ ...noUid, nfs: withTypedIdentity(noUid.nfs, { uid: '' }) })).toBeNull()
  })
})

describe('nameFallbackOf: NFS', () => {
  it("reads as the classic `host:/export` spelling, the backend's stand-in label", () => {
    expect(nameFallbackOf(nfsForm('nfs://nas.local/Public'))).toBe('nas.local:/Public')
    expect(nameFallbackOf(nfsForm('nfs://nas.local'))).toBe('nas.local')
  })
})

describe('an NFS prefill', () => {
  it('opens on NFS with the URL as typed, its path the export to split', () => {
    const form = formFromPrefill('nfs://nas.local/volume1/data/photos')
    expect(form).toMatchObject({ protocol: 'nfs', address: 'nfs://nas.local/volume1/data/photos' })
    expect(form.nfs.exportPath).toBe('/volume1/data/photos')
  })

  it('reads an app path whose export is escaped as its export and the folder under it', () => {
    expect(nfsFieldsFromAppPath('nfs://nas.local:2049/volume1%2Fdata/photos/2026')).toEqual({
      address: 'nfs://nas.local:2049',
      exportPath: '/volume1/data',
      startFolder: '/photos/2026',
    })
    const form = formFromPrefill('nfs://nas.local:2049/%2F/etc')
    expect(form).toMatchObject({ protocol: 'nfs', address: 'nfs://nas.local:2049', startFolder: '/etc' })
    expect(form.nfs.exportPath).toBe('/')
  })

  it('leaves an unescaped one to the address reading: a person may have typed it', () => {
    // `nfs://nas:2049/share/Public/x` could be export `/share/Public` typed by hand,
    // so only the server's export list can say where the export ends.
    expect(nfsFieldsFromAppPath('nfs://nas:2049/share/Public/x')).toBeNull()
    expect(nfsFieldsFromAppPath('sftp://ada@nas:22/srv')).toBeNull()
  })
})

describe('formFromNfsPlace', () => {
  const server: SavedServer = {
    id: 'nfs-data-nas-1a2b',
    protocol: 'nfs',
    displayName: 'Photos',
    nameSource: 'user',
    address: 'nas.local',
    username: null,
    pinned: true,
    lastConnectedAt: null,
    autoReconnect: false,
    places: [
      {
        volumeId: 'nfs-data-nas-1a2b',
        name: 'Photos',
        pinned: true,
        connected: false,
        appRoot: 'nfs://nas.local:2049/volume1%2Fdata',
        username: null,
        autoReconnect: false,
      },
    ],
  }

  it('reads the address, the export, the name, and the switch off the listing', () => {
    expect(formFromNfsPlace(server, 'nfs-data-nas-1a2b')).toMatchObject({
      protocol: 'nfs',
      address: 'nfs://nas.local:2049',
      displayName: 'Photos',
      autoReconnect: false,
      nfs: { exportPath: '/volume1/data', uid: '', gid: '' },
    })
  })

  it('opens the ids as typed, so no guess lands over the saved ones', () => {
    const form = formFromNfsPlace(server, 'nfs-data-nas-1a2b')
    expect(form?.nfs.identityFromGuess).toBe(false)
    expect(withGuessedIdentity(form!.nfs, { uid: 0, gid: 0, source: 'this_mac' }).uid).toBe('')
  })

  it('opens with an empty name for a place nobody named', () => {
    expect(formFromNfsPlace({ ...server, nameSource: 'fallback' }, 'nfs-data-nas-1a2b')?.displayName).toBe('')
  })
})
