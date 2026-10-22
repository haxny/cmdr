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
first (the dial's split, § "The dial": longest match against `EXPORT`, or `MNT` on ever-shorter prefixes when listing is
refused); the rest of a typed path becomes the start folder when none was given. So the caller learns the id from the
returned volume (`NfsVolume::volume_id`) and saves its resolved `NfsVolume::params`. A cancel at any point closes what
the attempt opened and answers `Cancelled`.

`list_exports` answers the canonical export paths, or `ListingRefused` when the server won't list them to this client (a
typed export may still mount). `guess_identity` takes this Mac's identity as its fallback and answers it as
`NfsIdentitySource::ThisMac` when nothing at the export root has a non-zero owner; otherwise `MostCommonOwner` (the most
common non-zero owner among the root's entries, ties to the lower uid) or `RootOwner` (the root's own, which is also the
answer when a squashed probe is refused the listing).

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
a redial needs, the resolved split, the root, `FsInfo`); `connect_nfs` dials NFS alone, `unmount` is best-effort `UMNT`.
The volume redials through the whole `mount` (§ "The volume").

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

## The volume

`NfsVolume` is WebDAV's shape (`{ name, root, inner }`, `sharing_connection` sharing `inner`) over
`cmdr_fs::volume::http_session`, with one RPC connection as the session's client (`reconnect.rs`'s `NfsClient`: the
`Nfs3Client`, the export's root handle, `FsInfo`, the `NULL` probe, and where `UMNT` goes).

- **No secret.** `credential_key` is `None`, so the lifecycle dials unattended with nothing and never reads the store;
  every redial refusal is `DialRefusal::Transient` (a refused export on a REDIAL keeps backing off), so the volume never
  rests in `NeedsCredentials`, and `sign_in_prompt` is `Nothing`.
- **One ladder.** `SessionClient::liveness` is the transport's own `Liveness`, and the session takes NFS's 5/5/2 timings
  (`HttpSession::with_silence_timings`): whichever watch starts first runs, so both must agree. A silent server is
  `DeviceDisconnected` 15 s in, every waiting call at once (`fake_volume_test.rs`, paused clock).
- **A redial is a whole dial** (`Dialer`, portmap through `GETATTR`), ❌ not `connect_nfs` alone: a restarted server may
  have moved its ports, and a stale root needs a fresh `MNT` anyway. The handle cache is cleared on each.
- **`on_unmount`** ends the session (the connection closes when the last call riding it ends) and sends `UMNT`, best
  effort, from the target the last dial recorded.
- **`Dialer`** is the one way in, first connect and redial alike: `over_tcp` in production, `fake_nfs.rs`'s
  `duplex_dialer` for paused-clock cells.

### Handles

`handles.rs`. A walk from the root handle, LOOKUP per component, with a bounded cache (8,192 folders, the older half
forgotten at once) of FOLDER handles under their REAL path. Files are looked up fresh every time: their attributes are
the length and date the caller wants.

- **Links**: a link in the middle of a path is always followed; at the end per `Follow` (`Never` for `entry_kind`,
  `IfInside` for `get_metadata`, which answers the link itself when it can't go, `Always` for open and list, where it
  can't go is `NotSupported`). Only a relative target that stays inside the export is followed (`paths::follow_link`,
  checked per `..`), at most 40 hops. Listings show a link as a link (SFTP's shape).
- **`STALE`** on a cached handle forgets it and its subtree and walks once more uncached; a second is `NotFound`. A
  stale ROOT is `DeviceDisconnected`, so the lifecycle redials (a fresh `MNT`).
- **A handle that moved without going stale** (another client renamed its folder) is caught by
  `NfsVolume::dir_for_mutation`: each cached folder carries its parent's `(mtime, ctime)` from the LOOKUP that found it,
  and one pipelined round of `GETATTR`s on the chain must match every one; the first level that moved is forgotten and
  re-LOOKUPed. A folder only a LISTING found (`HandleCache::offer`) has no stamp and so always re-LOOKUPs for a
  mutation. `HandleCache::note_dir_changed` moves the stamps our own mutation's `wcc_data` vouches for.

### Listings and reads

- **Listings**: `list_dir` pages of `dtpref` clamped to 8–128 KiB (about 600 entries each), `on_progress` per page, a
  child folder's handle offered to the cache. A name that isn't UTF-8 is counted and left out (one `debug!`). A pane's
  listing goes through `list_for_pane`, the poller's cue; a scan lists through `list_directory_impl` and never starts
  it.
- **The read window** (`streams.rs`): SFTP's `ChunkWindow` over positioned READs of `rtpref` within `rtmax` and 1 MiB,
  depth 8 (SFTP's measured optimum; re-measure on a NAS). A short answer is re-asked from where it stopped; only `eof`
  or an empty answer ends a chunk. The length is the fresh LOOKUP's `size` (always present in `fattr3`, so `Known`).
  `read_range` and `open_read_stream_at_offset` run the same window. 200 MiB off the knfsd fixture streams at about 76
  MB/s in a debug build over loopback (2026-10-20, `fixture_test.rs`).
- **Space** is `FSSTAT` on the root (`abytes` is this identity's room, used is total minus free), polled every 5 s.
  `write_access_at` is one `ACCESS` asking `MODIFY | EXTEND`: `EXTEND` granted is `Writable`, anything else
  `Unwritable { Unexplained }` (a read-only export and a refused permission look the same), a failure `Unknown`.

### Writes

`writes.rs`. The engine stages every write (`write_is_single_shot` stays `false`), so a write lands on its temp and
takes the user's name only through the landing rename.

- **Spans.** The source goes out in 8 MiB spans (`COMMIT_EVERY`) of `UNSTABLE` WRITEs, 8 in flight, each of `wtpref`
  within `wtmax` and the record cap; each span ends in a COMMIT and stays in memory until that COMMIT is good, since a
  resend needs it. Decision (revisit in review) on the 8 MiB: it bounds both the memory per upload and the bytes a
  server restart can cost. `FILE_SYNC` replaces `UNSTABLE` when `FSINFO` reports no write sizes at all.
- **The verifier.** The reference is the first verifier this file's WRITEs answered. A span whose `UNSTABLE` WRITEs
  disagree, or whose verifier or COMMIT isn't the reference, is resent whole with the new verifier as the reference; the
  second change fails the write (`EIO`). A span answered all `FILE_SYNC` / `DATA_SYNC` needs no COMMIT. So nothing is
  reported written that a COMMIT didn't vouch for (RFC 1813 § 3.3.21).
- **Short and empty WRITEs.** The rest goes again from the offset the server answered; a WRITE that takes nothing fails
  rather than spins.
- **The CREATE carries the mode alone**: `GUARDED` for a name that must be free (`create_file`, `CreateNew`),
  `UNCHECKED` with size 0 for the engine's own temp. The file streams as `0600`; the source's date (as mtime and atime)
  and its mode, folded by `routed_mode`, go on in one SETATTR after the last COMMIT (`copy.rs`). Why after: a WRITE
  bumps the mtime, and the landing rename touches only the `ctime`.
- **Every failure removes the partial**, cancel and a byte count that disagrees with the declared length included,
  through `remove_if_ours`: a LOOKUP first, and the REMOVE only while the name still holds the `fileid` this write made.

### Mutations

`mutation.rs`. Every one acts in a folder `dir_for_mutation` just proved (§ "Handles"), by `dir handle + name`.

- **Rename without clobbering.** v3 `RENAME` replaces, so `force = false` LOOKUPs the target (taken by another file:
  `AlreadyExists`, no call sent), then claims the name with `CREATE GUARDED` or, for a folder, `MKDIR`, and `RENAME`s
  onto the empty claim. A failed rename removes the claim only while it's still the claim's `fileid` (a `RENAME` whose
  reply never came may have landed, and the name then holds the user's file); a claim that can't be removed is an
  `IoError` naming its path. A target that is the source itself (same `fileid` and `fsid`) claims nothing.
  `force = true` is one `RENAME`, atomic, and a folder only ever replaces an empty one.
- **Delete** is `REMOVE` or `RMDIR` by a fresh LOOKUP's own attributes, so a link goes as the link; ❌ never recursive.
- **After a success**, the reply's `wcc_data` refreshes the folder's cached stamps, and a folder that moved or went is
  forgotten with its subtree: its handle would still answer, for the new place.
- **Folders**: `MKDIR` with this Mac's default folder mode; `create_directory_all` is the shared leaf-first walk. A walk
  through a file names the FILE (`NotADirectory`), the thing to move aside.
- **Metadata caps**: a file's and a folder's mode and a folder's date. No `CREATED` (v3 has none) and no `XATTRS`
  (dropped, ❌ no `._` sidecars), so the copy's loss note names them. `apply_metadata` takes a fresh handle by LOOKUP.

### Which side a test lives on

- `fake_nfs.rs` (in-process, over TCP or `tokio::io::duplex`): listings over both READDIR shapes, the cache rules, the
  window under reordered and short READs, links, the redial, the silence ladder on a paused clock, and the write side
  (`writes_test.rs`, `mutation_test.rs`). Its knobs (`fake_nfs/knobs.rs`): `rotate_verifier` and
  `rotate_verifier_on(procedure, nth)`, `answer_writes_short`, `set_write_max`, `refuse_next(procedure, status)`,
  `touch`, `drop_one_reply`, `go_silent`, `reset_mid_reply`, `refuse_dials`, `require_reserved_port`,
  `refuse_readdirplus`; and what a cell reads back: `calls(procedure)`, `call_log`, `last_create`, `largest_write`.
- `conformance_test.rs` (`#[ignore]`d, `/insecure`): every applicable `cmdr_fs::volume::conformance` cell, plus a 9 MiB
  `UNSTABLE` round trip. The link cells don't run: this backend makes no links.
- `fixture_test.rs` (`#[ignore]`d, the knfsd fixture): the seeded tree byte-exact, 200 MiB streamed, `read_range`,
  links, the split, a missing export, `/secure` refused through a published port and accepted on the Linux direct path,
  `list_exports`, `guess_identity` answering 1000.
- `volume::testing` (`testing` feature, also re-exported at the root): the fixture's server, params, `connect_insecure`,
  `scratch_dir`, and `clean_deep`, for the app's suites (`nfs_transfer_*` in
  `apps/desktop/src-tauri/src/operations_host/backend_suites/`).

## The public surface is capped

Every module is private, so all 10 promises arrive as root re-exports (`cmdr-git`'s shape), and both other buckets are
zero: `NfsServer`, `NfsIdentity`, `NfsConnectionParams`, `NfsConnectError`, `NfsVolume`, `connect_nfs_volume`,
`list_exports`, `guess_identity`, `NfsIdentityGuess`, and `NfsIdentitySource`. The app needs each one: the params to
dial and save a place, the error to map onto `ServerConnectOutcome`, the volume to register and to edit live
(`sharing_connection`, `set_auto_reconnect`), and the two discovery calls for the add form. `index-crate-isolation` is
pinned at exactly 10 / 0 / 0 (measured 2026-10-20). `volume::testing` rides the root as `testing` only under the
`testing` feature, for the app's Docker suites.
