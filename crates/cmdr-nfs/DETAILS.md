# cmdr-nfs details

Must-knows and the module map: `CLAUDE.md`. This file carries the decisions. The full design is
`docs/specs/nfs-support-plan.md`.

## The model

A server is the ACCOUNT and an export is its PLACE (SMB's host and shares, S3's account and buckets): one saved entry
and one volume per export. `NfsServer` is the account half (host, the port the address named, and the fixture-only
portmap port override; MOUNT's port always comes from portmap); `NfsConnectionParams` adds the export, the identity, the
start folder, and the auto-reconnect switch.

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
first (`discovery::resolve_export`: longest match against `EXPORT`, or `MNT` on ever-shorter prefixes when listing is
refused); the rest of a typed path becomes the start folder when none was given. So the caller learns the id from the
returned volume (`NfsVolume::volume_id`) and saves its resolved `NfsVolume::params`. Until the client lands, every dial
answers `NfsConnectError::Transport`.

`list_exports` answers the canonical export paths, or `ListingRefused` when the server won't list them to this client (a
typed export may still mount). `guess_identity` takes this Mac's identity as its fallback and answers it as
`NfsIdentitySource::ThisMac` when nothing at the export root has a non-zero owner; otherwise `MostCommonOwner` (the most
common non-zero owner among the root's entries) or `RootOwner` (the root's own).

## The protocol layer

Our own ONC RPC client, no dependency (the plan's § "The dependency decision": `nfs3_types` allocates what the wire says
and needs a license allowlist change).

- **`xdr.rs`**: a `Decoder` over one received record (`Bytes`). Every length is checked against the field's limit and
  then against what's left before anything is sliced; opaque data (a READ payload) is a zero-copy slice. An array count
  is checked against its smallest element first, so a 4 GiB claim in a 100-byte record reserves nothing.
- **`transport.rs`**: `RpcConnection` is one TCP connection with a writer task and a reader task; calls pipeline and
  find their reply by XID (process-wide, randomly seeded, so no repeat across reconnects). The rules:
  - Dropping a call's future forgets its XID; a late reply is dropped. v3 is stateless, so ❌ never drop the connection
    for it.
  - A framing error (a mark past `record_cap`, 1 MiB + 64 KiB; a close or reset mid-record) poisons the connection:
    every waiting call answers `Disconnected` at once. A framed reply that won't decode fails only its call.
  - Every byte heard and every record sent feeds `cmdr_fs::volume::liveness`, with NFS's 5 s quiet / 5 s probe / two
    probes; the probe is `NULL` on a FRESH connection from an ephemeral port. The ladder giving up poisons.
  - ❗ **A reply that never comes**: a call unanswered for `stuck_after` (20 s) while a call sent AFTER it was answered
    is resent once, same XID, if `Repeat::Safe`; `Repeat::Never` (CREATE, MKDIR, REMOVE, RMDIR, RENAME, SETATTR) fails
    `Unanswered` (`IoError` `ETIMEDOUT`) and RETIRES the connection (new calls answer `Retired`, in-flight ones finish);
    a resent call silent again poisons. With nothing later answered it's the ladder's case and nothing is resent.
  - The reserved port: random in 600–1023 then walking down, `SO_REUSEADDR`, `EADDRINUSE` / `EADDRNOTAVAIL` move on at
    `bind` and at `connect`, 32 tries; `EACCES` / `EPERM` or exhaustion falls back to ephemeral as
    `ReservedPort::Unavailable`. MOUNT and NFS take one; portmap and the probe don't.
  - `RpcConnection::over_stream` takes any byte stream and a `Probe` closure: the seam an in-process fake rides (the
    transport cells use `tokio::io::duplex` on a paused clock). `src/proto/server.rs` (test-only) is the server half of
    the framing plus `nfs3_wire` encoders, the seed for the volume's in-process fake NFS server.
- **`src/proto/nfs3/`**: `Nfs3Client`, one typed method per procedure, answering `Result<_, NfsError>` (`Status` by
  number, `Call`, `NoProgress`). Only the success arm decodes. `JUKEBOX` is retried after 1 s, three times. Metadata and
  mutations carry the client's budget (60 s); READ, WRITE, and COMMIT none. `list_dir` is the whole listing: READDIRPLUS
  with the verifier echoed and the next cookie taken from the page's LAST entry (even a dropped `..`), the READDIR +
  LOOKUP fallback on a first-page `NOTSUPP`, missing handles and attributes filled (16 in flight), an entry gone by its
  LOOKUP left out, `BAD_COOKIE` restarting (signalled as `ListingEvent::Restarted`) up to three times.
- **Why 20 s**: it sits well inside the 60 s metadata budget, so a GETATTR, LOOKUP, or READDIRPLUS whose reply was
  dropped is resent and answered instead of timing out (`transport_test.rs` pins it on a paused clock).

## The dial

`dial.rs`: resolve (IPv4 and IPv6, each address in order) → portmap `GETPORT` for MOUNT v3 and NFS v3 over TCP (an
explicit port skips the NFS lookup; MOUNT always comes from portmap) → `EXPORT` and `MNT` → `FSINFO` and `GETATTR` on
the root, under a 15 s budget with 5 s per connect. It answers `MountedExport` (the client, the addresses and credential
a redial needs, the resolved split, the root, `FsInfo`); `connect_nfs` redials NFS alone (no new `MNT`: the root handle
outlives the connection), `unmount` is best-effort `UMNT`.

- **The split**: the longest listed export the typed path is at or under; a listing with none that fits is
  `ExportNotFound`. Only when `EXPORT` is refused does `MNT` walk ever-shorter prefixes, and a refusal on the walk moves
  on, deciding the answer only when no prefix mounts. Why: knfsd mounts a SUBFOLDER of an export, and answers
  `MNT3ERR_ACCES` for a path it doesn't export (verified against the fixture, 2026-10-20).
- **Refusals by number**: knfsd refuses a non-reserved port with `MNT3ERR_ACCES` on `MNT` and `NFS3ERR_PERM` on later
  calls (`apps/desktop/test/nfs-servers/README.md` § "Reaching /secure"), others with `AUTH_TOOWEAK` / `AUTH_BADCRED`.
  Each is `ReservedPortRequired` when the connection's port was `Unavailable`, else `ExportRefused`. `PROG_UNAVAIL` /
  `PROG_MISMATCH` or a portmap with no v3 registration is `NoNfsV3`. An `MNT` naming no flavors counts as `AUTH_SYS`
  (the Linux client's reading); one listing only others is `ExportRefused`.
- **Decoding fixtures**: `src/proto/testdata/knfsd/` holds real knfsd replies, captured by
  `dial_fixture_test::capture_knfsd_replies_as_decoding_fixtures` with `NFS_CAPTURE_DIR` set, and decoded to the last
  byte in `src/proto/knfsd_replies_test.rs`.

## The public surface is capped

Every module is private, so all 10 promises arrive as root re-exports (`cmdr-git`'s shape), and both other buckets are
zero: `NfsServer`, `NfsIdentity`, `NfsConnectionParams`, `NfsConnectError`, `NfsVolume`, `connect_nfs_volume`,
`list_exports`, `guess_identity`, `NfsIdentityGuess`, and `NfsIdentitySource`. The app needs each one: the params to
dial and save a place, the error to map onto `ServerConnectOutcome`, the volume to register and to edit live
(`sharing_connection`, `set_auto_reconnect`), and the two discovery calls for the add form. `index-crate-isolation` is
pinned at exactly 10 / 0 / 0 (measured 2026-10-20). A `testing` module for the app's Docker suites raises the module
count, with its reason.
