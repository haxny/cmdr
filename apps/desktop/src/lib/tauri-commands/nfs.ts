// NFS servers: what the add form asks a server before a place exists, its exports
// and who owns the files in one, and the saved place an edit form prefills from. Connecting, cancelling, disconnecting, pinning,
// and forgetting go through the protocol-agnostic `servers.ts`.
//
// The model (a server is the account, each export a place) and the app-path
// grammar: `crates/cmdr-nfs/DETAILS.md`.

import { commands } from '#lib/ipc/bindings.js'
import type { GuessedNfsIdentity, NfsExportList, NfsIdentitySourceWire, SavedNfsPlace } from '#lib/ipc/bindings.js'

export type { GuessedNfsIdentity, NfsExportList, NfsIdentitySourceWire, SavedNfsPlace }

/**
 * The exports the server at `host` lists. ❗ `listing_refused` is not a dead end: an
 * export typed by hand may still mount, so the form keeps the field editable.
 */
export async function listNfsExports(host: string, port: number | null): Promise<NfsExportList> {
  return commands.listNfsExports(host, port)
}

/**
 * The uid and gid to act as on `exportPath`, with where they came from. Always
 * answers: an export that can't be read falls back to this Mac's own ids.
 */
export async function guessNfsIdentity(
  host: string,
  port: number | null,
  exportPath: string,
): Promise<GuessedNfsIdentity> {
  return commands.guessNfsIdentity(host, port, exportPath)
}

/**
 * The saved NFS place a volume id names, or `null` when none does. It carries the
 * uid, gid, and start folder the servers listing doesn't: what an edit form needs.
 * `displayName` is the SERVER's name, empty when nobody named it.
 *
 * ❗ By the `volumeId` the backend published, ❌ never a frontend twin of the id
 * Rust mints from `(host, port, export)`.
 */
export async function knownNfsPlaceOf(volumeId: string): Promise<SavedNfsPlace | null> {
  return (await commands.getKnownNfsPlaces()).find((place) => place.volumeId === volumeId) ?? null
}
