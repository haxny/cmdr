# `cmdr-restic`

Reads a restic backup repository, wherever it lives, as a read-only source of snapshots (#422). Built on `rustic_core`,
read through the repo's parent `Volume`. No `tauri`, no English a user reads.

## Module map

- `detect.rs`: `is_restic_repo`, the pure detector a host runs on a listing it already has.

## Must-knows

- **Read-only, always.** Browsing must never write to a user's backup.
- **No `rustic_backend`.** The repo is read through Cmdr's own volumes.
- ❌ **Never detect by trying a password**: the config is encrypted, so detection reads the folder's listing only.
- The public surface is capped by `index-crate-isolation`; an addition is argued in `DETAILS.md`.

The design, the public surface, and decisions: `DETAILS.md`.
