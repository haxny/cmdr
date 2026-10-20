# `cmdr-restic`

Reads a restic backup repository, wherever it lives, as a read-only volume of snapshots (#422). Built on `rustic_core`,
read THROUGH the repo's parent `Volume`, so a repo on SMB, SFTP, S3, or a phone opens like a local one. No `tauri`, no
English a user reads.

## Module map

- `volume.rs`: `ResticVolume`, the view at `<repo>/Browse snapshots`. `hub.rs`: `ResticRepos`, the shared state
  (passwords, opened repos, indexes, snapshot lists, `latest`, the is-it-a-repo memo).
- `path.rs` (`restic_route`, the classifier), `snapshots.rs` (naming, `latest`), `tree.rs` (nodes to rows),
  `read_stream.rs`, `repo.rs` (open, index, typed errors), `backend.rs` (rustic's storage over the parent, packs through
  held readers), `detect.rs`.
- `test_fixtures.rs` (`testing` only): real repos built with rustic, v1 and v2, plus an unlock door.

## Must-knows

- **Read-only, always.** Every `WriteBackend` method refuses; tests pin zero write attempts across browse and read.
- **❌ Never call rustic on an async worker.** The adapter `block_on`s the parent and panics there. Every call goes
  through `host.runtime().spawn_blocking`.
- **❌ Never classify a rustic error by its text.** Wrong password is `is_incorrect_password()`, a parent failure the
  `VolumeError` in the source chain. DETAILS § "Typed errors".
- **❌ Never detect by trying a password**: the config is encrypted. Detection reads the folder's listing only.
- **No `rustic_backend`.** The repo is read through Cmdr's own volumes.
- **A dated folder resolves by its id suffix, ❌ never the time prefix** (local time moves with the zone). An ambiguous
  suffix is `NotFound`, never a guess.
- **`latest` doesn't move while anything reads under it**: copies address files by path. DETAILS § "`latest`".
- **`to_indexed` consumes the open repo**, so the index is its own open (`IndexedRepo`), ❌ never pried out of the
  snapshot list's `ResticRepo`.
- **Mode is `& 0o777` of Go's `os.FileMode`**, ❌ never the raw value. Names go through `Node::name()`.
- **A hostile repo can't reach scrypt**: the adapter caps key count and cost before rustic lists a key.
- **The heavy state is the hub's**, keyed by `(volume id, repo root)` as spelled; ❌ no `canonicalize`, ❌ no static.
- The public surface is capped by `index-crate-isolation`; an addition is argued in `DETAILS.md`.

The view, the hub, the adapter, errors, naming, fixtures, what the next layers build on, and the capped surface:
`DETAILS.md`.
