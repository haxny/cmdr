/**
 * The NFS half of the add form: the export, and the uid and groups every request acts
 * as. Pure, like `server-form.ts`, which holds these under `nfs`.
 *
 * ❗ **A server is the account, an export is its place**, so the export is part of
 * the identity and the uid is ❌ not: `AUTH_SYS` carries no secret, so "Act as user
 * ID" is a setting like SMB's username, and fixing it later keeps the place's tabs
 * and favorites. That's also why NFS asks for no password and keeps no Remember box.
 *
 * ❗ **The export field may hold more than the export.** A pasted
 * `nfs://nas/volume1/data/photos` can't say where `/volume1/data` ends and
 * `/photos` begins (QNAP nests `/share` and `/share/Public`), so the field keeps
 * the whole path and the backend splits it against the server's own export list,
 * with the rest becoming the start folder.
 */

import type { GuessedNfsIdentity, NfsIdentitySourceWire } from '#lib/ipc/bindings.js'
import { parseServerPath } from './server-path-utils'

/** What the NFS form holds besides the address, the name, and Advanced. */
export interface NfsFormFields {
  /** The export as picked from the list or typed, possibly with a folder under it. */
  exportPath: string
  /** "User ID", as typed: `parseNfsId` reads it. */
  uid: string
  /** "Group ID", as typed. */
  gid: string
  /** "Other group IDs", as typed: `readNfsOtherGids` reads it. */
  otherGids: string
  /**
   * Whether the ids still hold a guess (or nothing yet), so the next guess may
   * replace them. ❗ Typing into any of them makes them all the person's.
   */
  identityFromGuess: boolean
  /** Where the guess on screen came from, for the line under the fields; `null` once typed over. */
  identitySource: NfsIdentitySourceWire | null
}

/** A blank NFS half: no export picked, no identity guessed yet. */
export function emptyNfsFields(): NfsFormFields {
  return { exportPath: '', uid: '', gid: '', otherGids: '', identityFromGuess: true, identitySource: null }
}

/** The largest id `AUTH_SYS` carries: an unsigned 32-bit number. */
const MAX_NFS_ID = 4294967295

/** A typed uid or gid as the number the target carries, or `null` when it isn't one. */
export function parseNfsId(text: string): number | null {
  const trimmed = text.trim()
  if (!/^\d{1,10}$/.test(trimmed)) return null
  const id = Number(trimmed)
  return id <= MAX_NFS_ID ? id : null
}

/**
 * The most other groups a place lists: what `AUTH_SYS` carries in its `gids`
 * (RFC 5531). The credential's twin of this limit is `MAX_GIDS` in `cmdr-nfs`.
 */
const MAX_NFS_OTHER_GIDS = 16

/** What the "Other group IDs" field says. */
export type NfsOtherGidsReading =
  | { kind: 'ids'; ids: number[] }
  | { kind: 'not_ids' }
  | { kind: 'too_many'; count: number }

/** The typed other groups, split on commas and spaces, each id once in the typed order. Empty reads as none. */
export function readNfsOtherGids(text: string): NfsOtherGidsReading {
  const ids = new Set<number>()
  for (const part of text.split(/[\s,]+/)) {
    if (part === '') continue
    const id = parseNfsId(part)
    if (id === null) return { kind: 'not_ids' }
    ids.add(id)
  }
  if (ids.size > MAX_NFS_OTHER_GIDS) return { kind: 'too_many', count: ids.size }
  return { kind: 'ids', ids: [...ids] }
}

/** The fields with a guess filled in, unless the person already typed their own. */
export function withGuessedIdentity(fields: NfsFormFields, guess: GuessedNfsIdentity): NfsFormFields {
  if (!fields.identityFromGuess) return fields
  return {
    ...fields,
    uid: String(guess.uid),
    gid: String(guess.gid),
    otherGids: guess.otherGids.join(', '),
    identitySource: guess.source,
  }
}

/** The fields after the person typed into one of the ids, which makes them all theirs. */
export function withTypedIdentity(
  fields: NfsFormFields,
  typed: { uid?: string; gid?: string; otherGids?: string },
): NfsFormFields {
  return { ...fields, ...typed, identityFromGuess: false, identitySource: null }
}

/**
 * Whether to warn under the ids: user ID 0, which most servers squash to `nobody`
 * (`root_squash`, the Linux default). ❌ Never a block: on an export with
 * `no_root_squash` it's exactly what someone means.
 */
export function nfsIdentityWarns(fields: NfsFormFields): boolean {
  return parseNfsId(fields.uid) === 0
}

/**
 * A pasted NFS APP path whose export segment carries an escape (`%2F`, `%25`), read
 * back into the address, the export, and the folder under it; `null` otherwise.
 *
 * ❗ Only an escaped one is certainly Cmdr's own spelling: `nfs://nas:2049/share/Public/x`
 * may be export `/share/Public` typed by hand, which `parseServerPath` would read as
 * export `/share`. That one stays an address, and the backend splits it.
 */
export function nfsFieldsFromAppPath(
  path: string,
): { address: string; exportPath: string; startFolder: string } | null {
  const parsed = parseServerPath(path)
  if (parsed?.protocol !== 'nfs') return null
  const segment = path.slice(`nfs://`.length).split('/')[1] ?? ''
  if (!segment.includes('%')) return null
  return {
    address: `nfs://${parsed.host}:${String(parsed.port)}`,
    exportPath: parsed.export,
    startFolder: parsed.path === '' ? '' : `/${parsed.path}`,
  }
}
