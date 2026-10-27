# Saved servers: details

## Why this module sits below `commands`

- **Decision**: the cores behind the servers commands (the listing, an add, an edit, a dial) live here, and
  `commands/servers.rs` is a pass-through. **Why**: Ask Cmdr (`agent/`) reads saved servers, dials them
  (`connect_volume`), and saves one a user approved, and `agent/` must not reach into `commands/`. One copy of each
  core keeps the hub and the agent from drifting apart: a save the agent's approval makes is exactly the one "Add
  anyway" makes.
- The wire types (`wire.rs`) moved with the cores and are re-exported by `commands::servers`. Specta names a type by
  its Rust ident, so the move left `bindings.ts` alone; their doc comments still ship there.

## The listing

- `listing::listing(manual, known, hosts)` is a pure fold over the stores: the manual SMB list, the share store, and
  the discovery list come in as arguments (the manual store reads a FILE through an `AppHandle`), so a cell drives it
  without racing another over the process-global copies. SFTP, WebDAV, S3, and NFS are read from their stores inside.
- One `SavedServer` per account: SFTP and WebDAV are one place each (the account IS the place); an S3 account's places
  are its buckets plus its root (`s3_accounts.rs`); an NFS server's are its exports (`nfs_places.rs`); an SMB host's are
  its saved shares (`smb_hosts.rs`).
- A place's `app_root` comes from `server_volumes::server_places()`, the one place that mints the spelling, ❌ never
  re-derived here.

## SMB hosts

- ❗ **A host's identity is where it dials, address AND port** (`smb_hosts.rs`), ❌ never the name a person gave it and
  never the address alone: one machine can run several servers (the Docker fixtures are ten on `localhost`). Its
  sign-in history and shares are filed under its discovery name (`localhost:11482`), so the listing matches store rows
  by that name, or by the address and port a mount dialed. Matching by the typed name once split a named host in two,
  and the made-up second row then took edits meant for the first. Its `address` carries the port off 445. Its saved
  shares are its places (`docs/specs/saved-smb-shares.md`).
- **Manual entries go first**, because only a manual entry can carry a name a person typed; a share-history row for the
  same host then only lends it the time it was last used.
- **The listing is also what Edit and Forget act on.** They look a host up by the id the listing gave it
  (`smb_host_group`) and touch exactly the store rows it filed under that host, so "what the row showed" and "what
  Forget took" can't disagree. `edit_host` and `eject_and_edit_host` share `plan_edit`, so they can't read one edit two
  ways (`docs/notes/server-address-move.md` § "SMB: a pending move").

## Saving and editing

- `save_target` is "Add anyway": it builds the store entry (`entry_of`) and calls the wiring's
  `save_without_connecting`, which writes the store when the place isn't connected. A NEW SFTP or WebDAV entry is saved
  `pinned: true` with a fresh `last_connected_at`; `remember` keeps a stored pin on a replace.
- `save_edit` finds the saved entry by its id (`saved_by_id`): the same address saves in place, another one MOVES the
  server (`server_move.rs`). An S3 place or NFS export naming another place answers `AccountChanged`, since its address
  IS the place. A place nobody saved any more answers `Unreachable`, ❌ never a quiet re-save.
- A connected place's edit applies live over its session (`network/live_server_edit.rs`).
