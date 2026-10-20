# cmdr-nfs details

Must-knows and the module map: `CLAUDE.md`. This file carries the decisions. The full design is
`docs/specs/nfs-support-plan.md`.

## The model

A server is the ACCOUNT and an export is its PLACE (SMB's host and shares, S3's account and buckets): one saved entry
and one volume per export. `NfsServer` is the account half (host, the port the address named, and the fixture-only
portmap and MOUNT port overrides); `NfsConnectionParams` adds the export, the identity, the start folder, and the
auto-reconnect switch.

**Decision: the uid and gid are a setting of the place, not its identity.** `AUTH_SYS` carries no secret, so they're a
preference like SMB's username, and fixing a uid typo must not orphan the place's tabs and favorites. A changed identity
redials over a new connection under the same volume id.

**Decision: the identity port is the canonical one.** `NfsServer::identity_port` answers the port the address named,
else 2049, ❌ never what portmap answered: that can move when the server restarts.

## App paths

`cmdr_fs::volume::nfs_app_root(host, port, export)` mints `nfs://<host>:<port>/<export segment>`. The export is made
canonical first (`canonical_nfs_export`: `normalize_remote_path`'s spelling, its bytes otherwise kept, no case or
Unicode folding), then escaped into ONE segment: the leading `/` dropped, `%` → `%25`, `/` → `%2F`, and the root export
`/` as `%2F`. Why: `server_of_path` mints a volume id from a path alone (index routing, error classification), and with
nested exports (`/share` and `/share/Public`) an unescaped path can't say which export it belongs to. The cost,
accepted: a file under nested exports has two spellings, one per place. A volume's `RemoteRoot` is rooted at that prefix
with `/` under it, so the paths the volume speaks are export-relative. No account rides in the authority.

## Connecting

`connect_nfs_volume(name, params, host, cancel)` takes what the person picked or typed and resolves the real export
(longest match against `EXPORT`, or `MNT` on ever-shorter prefixes when listing is refused); the rest of a typed path
becomes the start folder when none was given. So the caller learns the id from the returned volume
(`NfsVolume::volume_id`) and saves its resolved `NfsVolume::params`. Until the client lands, every dial answers
`NfsConnectError::Transport`.

`list_exports` answers the canonical export paths, or `ListingRefused` when the server won't list them to this client (a
typed export may still mount). `guess_identity` takes this Mac's identity as its fallback and answers it as
`NfsIdentitySource::ThisMac` when nothing at the export root has a non-zero owner; otherwise `MostCommonOwner` (the most
common non-zero owner among the root's entries) or `RootOwner` (the root's own).

## The public surface is capped

Every module is private, so all 10 promises arrive as root re-exports (`cmdr-git`'s shape), and both other buckets are
zero: `NfsServer`, `NfsIdentity`, `NfsConnectionParams`, `NfsConnectError`, `NfsVolume`, `connect_nfs_volume`,
`list_exports`, `guess_identity`, `NfsIdentityGuess`, and `NfsIdentitySource`. The app needs each one: the params to
dial and save a place, the error to map onto `ServerConnectOutcome`, the volume to register and to edit live
(`sharing_connection`, `set_auto_reconnect`), and the two discovery calls for the add form. `index-crate-isolation` is
pinned at exactly 10 / 0 / 0 (measured 2026-10-20). A `testing` module for the app's Docker suites raises the module
count, with its reason.
