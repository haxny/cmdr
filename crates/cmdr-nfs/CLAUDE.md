# cmdr-nfs

The NFSv3 backend: our own user-space client (ONC RPC over TCP, MOUNT, NFS) and a `Volume` per export, so a `secure`
export opens from an unprivileged process with no OS mount. Same lifecycle shape as `crates/cmdr-webdav`, minus the
secret. The plan it's built from: `docs/specs/nfs-support-plan.md`.

## Module map

- `params.rs`: `NfsServer` (host, port, the fixture's portmap and MOUNT port overrides), `NfsIdentity` (uid, gid),
  `NfsConnectionParams` (the place: server, canonical export, identity, start folder, auto-reconnect).
- `errors.rs`: `NfsConnectError`, every variant from a number the server sent.
- `volume/`: `mod.rs` (`NfsVolume`, `connect_nfs_volume`), `discovery.rs` (`list_exports`, `guess_identity`),
  `volume_impl.rs` (the `Volume` trait).

## Must-knows

- ❗ **v3 and `AUTH_SYS` only.** No v4, no Kerberos, no NLM, no UDP. A server without v3 is `NoNfsV3`, ❌ never a
  fallback.
- ❌ **Never classify by message.** `nfsstat3`, `mountstat3`, and the RPC stats map by number; the `String` in a variant
  is a log-only diagnostic.
- ❗ **Identity is `(host, canonical port, export)`**, minted only by `cmdr_fs::volume::nfs_volume_id` and
  `nfs_app_root` through `NfsConnectionParams`. The uid and gid are a SETTING: ❌ never in an id or a path. The port is
  the one the address named, else 2049, ❌ never what portmap answered.
- ❗ **The app path carries the export as ONE escaped segment** (`nfs://nas:2049/volume1%2Fdata/photos`), so a path
  alone names its volume. `cmdr_fs::volume::remote_paths` owns the grammar; `server-path-utils.ts` mirrors it.
- ❗ **`connect_nfs_volume` resolves the export/folder split.** Register under the returned volume's `volume_id()` and
  save its `params()`, ❌ never the id of what was passed in.
- ❌ **No `tauri`, no user-facing words** (`index-crate-isolation`). The public surface is capped: DETAILS § "The public
  surface is capped".

Decisions and the surface audit: `DETAILS.md`.
