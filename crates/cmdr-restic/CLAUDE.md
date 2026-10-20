# `cmdr-restic`

Reads a restic backup repository, wherever it lives, as a read-only source of snapshots (#422). Built on `rustic_core`,
read THROUGH the repo's parent `Volume`, so a repo on SMB, SFTP, S3, or a phone opens like a local one. No `tauri`, no
English a user reads.

## Module map

- `detect.rs`: `is_restic_repo`, the pure detector a host runs on a listing it already has.
- `backend.rs`: `VolumeBackend`, rustic's storage trait over the parent volume. Ranged pack reads, every write refused.
- `repo.rs`: `ResticRepo` (open + unlock, snapshot list) and `ResticError`. `snapshots.rs`: naming, order, `latest`.
- `test_fixtures.rs` (`testing` only): tiny repos built with rustic itself, v1 and v2.

## Must-knows

- **Read-only, always.** Every `WriteBackend` method refuses; tests pin zero write attempts across open and list.
- **❌ Never call rustic on an async worker.** The adapter `block_on`s the parent and panics there. Every call goes
  through `host.runtime().spawn_blocking` (`repo.rs`).
- **❌ Never classify a rustic error by its text.** Wrong password is `is_incorrect_password()`, a parent failure is the
  `VolumeError` in the source chain (`volume_error_in`). DETAILS § "Typed errors".
- **❌ Never detect by trying a password**: the config is encrypted. Detection reads the folder's listing only.
- **No `rustic_backend`.** The repo is read through Cmdr's own volumes.
- **Snapshot folder names are unique and sort by time** to the second; `latest` can't collide. DETAILS § "Snapshot
  folders".
- **`to_indexed` consumes the open repo** (`OpenStatus` isn't `Clone` in 0.13.0). The index step must take ownership.
- The public surface is capped by `index-crate-isolation`; an addition is argued in `DETAILS.md`.

The adapter, opening, errors, naming, fixtures, what the next layers build on, and the capped surface: `DETAILS.md`.
