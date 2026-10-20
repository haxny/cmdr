# `cmdr-restic` details

The spec and the research behind it: GitHub issue #422. This crate holds the storage adapter, unlock, the snapshot list,
detection, and the read-only view (`ResticVolume` over the shared `ResticRepos` hub). The host route, the listing
overlay, the unlock UI, and copy-out live in the app.

Written against `rustic_core` 0.13.0 (default features off). The issue was researched on 0.12.0; what moved is in §
"Against 0.13.0".

## The view (`volume.rs`)

`ResticVolume::new(hub, volume_id, parent, repo_root)` is the view rooted at `<repo>/Browse snapshots`
(`VIRTUAL_ENTRY_NAME`). It's the same shape as `GitPortalVolume` (`crates/cmdr-git/DETAILS.md` § "The portal is a routed
volume"): the host routes a path here, and the volume classifies the whole path itself (`path::classify`), so a resolved
path stays the caller's path.

- **The root** lists one folder per snapshot plus `latest`, sorted by name (dated names start with a digit, so `latest`
  sorts last). A row's Modified is the backup's time; its Size is the summary's `total_bytes_processed`, or none for a
  snapshot without a summary (restic before 0.17). Every root listing re-reads `snapshots/` (rustic's
  `update_all_snapshots`: only new files are read, deleted ones drop), so a manual refresh shows a new backup.
- **A snapshot folder** holds that snapshot's tree. Restic snapshots of a Mac start at `/`, so the tree under a folder
  is `Users/…`; it's shown as-is.
- **Read-only by construction**: `is_writable` false, every mutation at the trait's `NotSupported` default, and
  `create_directory_all` overridden the way git's is. `routes_over_a_parent` true, `can_watch_listings` false,
  `supports_local_fs_access` false, `paths_are_os_visible` false, `local_path` none. Lane key and space answers are the
  parent's. `metadata_caps` is `MODE_REPORTER` (nodes carry xattrs, but nothing here reports them yet).
- **Cancel**: `list_directory_with_cancel` is overridden, since it's the only way the pane's token reaches a volume. A
  cancelled listing answers `Cancelled` at once; the blocking work it started finishes on its own and is dropped.
- **Scans** go through `cmdr_fs::volume::scan_walk::ScanSource`. `max_concurrent_scan_listings` is 4: each listing below
  a snapshot is a tree blob read from the parent, and a copy's pre-scan of a whole snapshot would otherwise fan out
  without bound.
- **Errors**: a path not in a snapshot is `NotFound` with the caller's path, ❌ never a damaged-repo error. No password
  yet is `NeedsPassword { wrong_attempt: false }`; one that stopped working is `{ wrong_attempt: true }`. A parent
  failure surfaces as the parent's own `VolumeError` (a dropped share stays `DeviceDisconnected`, not a generic I/O
  error). `ResticError::into_volume_error` is the one mapping.

### Trees (`tree.rs`)

- A walk reads one tree blob per folder (`get_tree`) and matches each component against `Node::name()`, which unescapes
  rustic's stored name (❌ never the raw `name` field).
- **Mode**: restic stores Go `os.FileMode` and rustic 0.13.0 hands it through raw (its `map_mode_from_go` is
  `pub(crate)`). A row reports `mode & 0o777`: the type comes from `NodeType`, and Go's setuid, setgid, and sticky bits
  sit at other positions, which this drops on purpose (a restore shouldn't recreate setuid binaries). No mode recorded
  is `0`, the "no permission concept" sentinel.
- **Kinds**: symlinks are rows with `is_symlink`, ❌ never followed; devices, fifos, and sockets are non-directory rows
  with size 0. Owner and group are the user and group names the node recorded. Dates are whole Unix seconds; a date
  before 1970 is left out.
- **Odd names** pass through as stored (NFD, double spaces, 255 bytes). A name that isn't UTF-8 (a Linux machine's
  backup) lists lossily, the way `LocalPosixVolume` lists one (`to_string_lossy`). The lossy name then finds nothing
  (`NotFound`), and when a real sibling's stored name IS that lossy spelling, the lookup answers `AmbiguousName` rather
  than hand over the wrong file.

### Reads (`read_stream.rs`)

`open_read_stream` opens the node (`open_file`, which needs the full index) and reads 1 MiB windows (`read_file_at`) on
the blocking pool; `read_range` is one `read_file_at`. Each window reads only the blobs it spans, and each blob read is
a `read_range` window of the pack on the parent, never the whole pack. A chunk that comes back empty before the recorded
size is `SourceChanged`: a short file never lands silently. The stream reports the node's date and permission bits.
Directories answer `IsADirectory`; symlinks and special files `NotSupported`.

### `latest`

Copies address files by path, so `latest` rebinding mid-copy would mix two snapshots in one restore, silently. The hub
pins which snapshot `latest` shows (`LatestPin`) and moves it only on a root refresh, and only when nothing has touched
`latest/…` for the quiet period (30 s): a listing, a stat, a stream open, or any chunk of a stream opened there. Listing
the root only shows the `latest` row and doesn't count, so refreshing to see a new backup moves it. The pin also moves
at once when its snapshot is gone from the repo (a prune), since there's nothing left to show. A dated folder is the
exact pin; `latest` is the convenience. With several hosts in one repo, `latest` is still the newest snapshot overall.

## The hub (`hub.rs`)

`ResticRepos` is a value the host parks once and shares with every `ResticVolume` (git's `wiring::portal()` pattern, ❌
never a static). A volume is cheap (hub, slot, parent); the hub holds what's costly to get again, so a routing cache
dropping a volume loses nothing.

- **Slots**, one per repo, keyed by `(volume id, repo root)` exactly as spelled (❌ no `canonicalize`: it's a `stat`,
  and on a network parent a slow one). A slot is never dropped, so its password outlives the volumes over it. It holds
  the password (`Zeroizing`), the opened repo for the snapshot list, the indexed repo, the snapshot list, and the
  `latest` pin.
- **Locks**: std mutexes for quick state, never held across an `.await`. Two per-repo tokio mutexes (opened, indexed)
  are held across an open or an index load, so a second caller for the SAME repo waits instead of starting another; no
  other repo waits on them, and ❌ no hub-wide lock is held across scrypt or a parent read.
- **A replaced parent re-opens.** An SMB share reconnecting replaces the parent `Arc` under the same volume id. Each
  opened value remembers the parent it was opened through (`Bound`, compared by `Arc` address), and a call arriving with
  a different parent opens again; the password survives.
- **The index** loads on the first tree access: a fresh open with the slot's password, then `to_indexed()` (the full
  index, since reads need `IndexedFull`), on one blocking thread. It records which snapshots it covers (those listed
  before the load: restic writes a backup's index files before its snapshot file). A snapshot outside that set (a backup
  made since) loads the index again; readers holding the old `Arc` finish on it.
- **Unlock** (`ResticRepos::unlock`): opens the repo with the password and keeps both only after the open returns, so a
  caller dropping the future mid-scrypt leaves no trace. A wrong password keeps nothing. Reached through
  `test_fixtures::unlock` until a `PasswordLocked` implementation calls it, hence its `testing` gate.

### Is this folder a repo?

The host's route is lexical (`restic_route`: the folder holding the FIRST `Browse snapshots` component), so it confirms
the candidate before routing:

- `confirm_repo(volume_id, parent, root)` answers from the memo, or lists `root` once (bounded by a 5 s deadline) and
  runs `is_restic_repo`. A yes is remembered for good; a no for 30 s, so a real non-repo folder named `Browse snapshots`
  doesn't cost a listing per call; a timeout or a failing parent not at all.
- `overlay_row(volume_id, root)` is the row the host's overlay adds to a pane's listing of a repo it recognized from the
  entries, and it confirms the repo in the memo for free.
- `serves_confirmed(volume_id, root)` is the memo alone, zero I/O, for a synchronous seam: `Some(true)`, `Some(false)`,
  or `None` (never asked, or the no went stale).
- The memo holds 1,024 answers; when full, the "no"s go first.

## The storage adapter (`backend.rs`)

`VolumeBackend` implements rustic's blocking `ReadBackend` + `WriteBackend` over a parent `Arc<dyn Volume>`, the repo's
folder on it, and a runtime `Handle`. It's the bridge `cmdr-archive`'s `VolumeByteSource` uses: each method `block_on`s
the parent's async call. That's only sound off the runtime's workers, so every rustic call runs on `spawn_blocking`;
rustic's own worker threads (index loading uses them) are outside the runtime too.

Layout, per restic's repo design:

- `FileType::Config` is the single file `<root>/config`, listed as one entry with the default id.
- `Key`, `Snapshot`, `Index` are `<root>/{keys,snapshots,index}/<id>`.
- `Pack` is `<root>/data/<first two hex digits>/<id>`. Listing reads `data/` once and then only the subfolders that
  exist, never 256 blind lookups.
- A listed name that isn't a 64-digit hex id (`.DS_Store`, a stray temp) is skipped. A missing folder lists as empty.

Reads:

- `read_full` streams the whole file (`open_read_stream`). It serves the small files: config, keys, snapshots, index.
- `read_partial` is ONE `read_range` of exactly the requested window, since pack files are read at random offsets and a
  whole-pack read over SMB would be ruinous. A parent without `read_range` (`NotSupported`) falls back to streaming from
  the start and cutting the window out; correct, not cheap. A window that comes back short is an error, so a pack
  truncated under us fails loudly instead of feeding rustic short bytes.

Writes: `create`, `write_bytes`, and `remove` all refuse and count the attempt (`write_attempts`, test-only), so a test
can pin that browsing never even tries.

`warmup_path` returns an empty string and `needs_warm_up` keeps its `false` default: cold-storage warm-up never applies.

### Hostile repos

Anyone can leave a folder that looks like a repo on a shared drive, and rustic 0.13.0 passes a key file's own scrypt
`N`, `r`, `p` (plaintext JSON) straight to scrypt with no ceiling, trying every key it lists. So `list_with_size(Key)`
offers rustic at most 32 key files (the first by id), skips any over 64 KiB or not a key file, and skips any whose cost
`N·r·p` exceeds 2^23: 8 times rustic's own default (`N = 2^17`, `r = 8`, `p = 1`), and a 1 GiB ceiling on scrypt's
`128·N·r` bytes. rustic finds keys only through this listing (it passes no key hint on open), so a key skipped here is
never derived. A crafted repo with 32 tolerable decoys sorting before the real key would refuse the password; that's the
price of not pinning a core.

## Opening a repo (`repo.rs`)

`ResticRepo::open(parent, root, password, cache_dir, host)` builds the adapter, then on one blocking thread runs
`Repository::new(..).open(&Credentials::password(..))`. That lists the config, tries the password against each key
(scrypt, deliberately about a second per key), and reads the config. It does NOT load the index. The password copy the
blocking task carries is a `Zeroizing<String>`; rustic's `Credentials` makes its own plain copy, which we can't avoid.
`refresh_snapshots(current)` is `update_all_snapshots`.

`IndexedRepo::open` is a separate open plus `to_indexed()`, on one blocking thread, because `to_indexed` consumes the
open repo and `OpenStatus` isn't `Clone`. A compile-time assertion pins `Repository<IndexedFullStatus>` as
`Send + Sync`, which concurrent reads through one `Arc<IndexedRepo>` rely on.

`cache_dir: None` sets `no_cache`; `Some(dir)` lets rustic cache snapshot and index files there (never in the repo). The
hub passes `None` today.

No restic lock is taken on read: rustic 0.13.0 has no lock file type at all (see below), and the zero-write tests pin an
empty `locks/` too.

### Typed errors

`ResticError` is what a host words: `WrongPassword`, `NotARepository`, `Volume(VolumeError)`, and
`Unreadable { detail }` (rustic's `display_log()`, for the log and the technical details only). `classify` sorts a
`RusticError` by type:

1. **Wrong password**: `RusticError::is_incorrect_password()`. rustic attaches its own error code `C002` when no key
   accepts the password (`find_key_in_backend`), and this accessor is rustic's public, typed way to ask for it. The
   error's `kind` field isn't readable outside rustic in 0.13.0, so the code is the only typed signal there is. Pinned
   by `a_wrong_password_is_typed_and_the_right_one_then_opens`; if a rustic bump drops the code, that test fails.
2. **Parent failure**: the adapter wraps every `VolumeError` with `RusticError::with_source`, so it rides in the error's
   source chain; `volume_error_in` finds it by `downcast_ref`. Pinned by
   `a_parent_that_cant_answer_surfaces_its_typed_error`.
3. **Not a repo**: anything else, when the config is gone (one more typed `list_with_size(Config)` asks the parent).
4. **Unreadable** otherwise: a damaged repo, an unsupported version, or a crashed blocking task.

A panic inside a blocking task comes back as a `JoinError`, which maps to `Unreadable`, ❌ never a crash.

## Snapshot folders (`snapshots.rs`, `path.rs`)

A snapshot folder name is its local time plus its short id: `2026-10-12 21.00.03 (a1b2c3d4)` (David's decision on #422).
Dots in the time, since `:` isn't a valid name character in Finder.

- **Order** is by time, then id. `SnapshotList::latest()` is the last one, so a same-instant tie always resolves the
  same way.
- **Unique across the repo**: the id part is the shortest prefix, eight digits at least, that no other snapshot's id
  starts with. Only the ids that clash grow. (The neighbors in sort order are the only candidates, so naming stays O(n
  log n).)
- **Resolved by the suffix alone** (`short_id_of`, `SnapshotList::by_short_id`): the time prefix is local time, so a
  time zone change or DST would otherwise orphan every path in history and favorites. A suffix must be 8–64 lowercase
  hex digits. A prefix matching several snapshots (a newer backup now shares a saved prefix) resolves to nothing, ❌
  never the wrong snapshot; a longer saved suffix still resolves.
- **Sortable**: sorting by name sorts by time, to the second. During a DST fall-back the repeated hour's two passes
  interleave by name (the list order stays true; only a name sort differs).
- **`latest`**: `LATEST_FOLDER_NAME`. A dated name starts with a digit, so they can't collide.

`snapshot_list` is a pure function of the raw snapshots and a `TimeZone`, so tests pin a zone (Stockholm) rather than
depend on the machine's.

`restic_route` is lexical: the folder holding the path's first `Browse snapshots` component (a repo backed up inside a
snapshot still routes at the outer boundary). It's a candidate, not a verdict; § "Is this folder a repo?".

## Detection (`detect.rs`)

A folder is a restic repo when its listing holds a regular file named `config` and the folders `data`, `index`, `keys`,
and `snapshots`. `locks` is optional: restic creates it lazily, and a copied repo often drops it. A symlink counts as
neither: restic never writes one, and its target is unknown from the listing alone. Names match exactly.

## Fixtures (`test_fixtures.rs`)

Under the `testing` feature (this crate turns it on for its own tests through a self dev-dependency), so the host's
tests build repos the same way. `FixtureRepo::init` (v2) and `init_v1`, then `add_snapshot` (an empty-tree snapshot with
no summary, for list tests) or `backup` (a real folder, for trees and contents). `write_into(volume, root)` lays the
repo out on any `Volume`, `LocalPosixVolume` included, and skips files already there, so writing again after another
`backup` adds just the new ones. `files()` hands back the raw layout. `unlock(volume, password)` is the door to an
unlocked view.

- rustic's `init` refuses `set_version = 1` as a downgrade, so `init_v1` hands `init_with_config` a v1 `ConfigFile`
  directly, with restic's test chunker polynomial and an all-zero repository id (the id type isn't public).
- The fixture holds the repo ids-indexed, since `backup` needs that and `OpenStatus` can't be cloned to get there twice.
- scrypt is slow unoptimized (about 8 s per unlock), so the workspace builds `scrypt`, `salsa20`, and `pbkdf2` at
  `opt-level = 3` even in debug (root `Cargo.toml`). Without that this suite takes minutes; with it, under 2 s.
- A backup's source files are real files: macOS refuses a name that isn't UTF-8, so that case is a `tree.rs` cell over
  hand-built nodes.

## What the next layers build on

- **The unlock seam**: implement `PasswordLocked` on `ResticVolume` over `ResticRepos::unlock` (then drop its `testing`
  gate), plus Keychain through `cmdr_fs::volume::secret_store`.
- **The index lifecycle**: today the per-repo `indexed` mutex is the single flight, and a cancelled waiter just drops
  its future (the load runs on). Byte-honest progress, cancel of the load itself, the memory cap and idle drop, and the
  `cache_dir` seam replace that; the dependencies they need (`tokio` `sync` and `time`, `tokio-util`) are already here.
- **The host route** calls `restic_route`, then `confirm_repo` (or `serves_confirmed` on a sync seam), and builds a
  `ResticVolume` over the parked hub. The overlay calls `is_restic_repo` on the entries, then `overlay_row`.
- `ResticVolume::backend_kind` keeps the trait's `Local` default until `cmdr-fs` gains a restic kind; the host's matches
  on `BackendKind` (favorites, the error report, the stall watch, the index) need one.

## Against 0.13.0

What differs from the issue's 0.12.0 research, read from the 0.13.0 source:

- `WriteBackend::write_bytes` takes a `BytesList`, not `Bytes`.
- `FileType` has no `Lock` variant (only `Config`, `Index`, `Key`, `Snapshot`, `Pack`), so there's nothing to map to
  `locks/`. Detection still allows the folder.
- `ReadBackend` adds `warmup_path` (required) next to `needs_warm_up` / `warm_up` (defaulted).
- `Repository<OpenStatus>` isn't `Clone`.
- `SnapshotOptions::to_snapshot` fills an all-zero summary; restic before 0.17 wrote none.
- MSRV 1.91; new to the lockfile against 0.12.0's list: `itertools` 0.15 (a third major next to 0.13 and 0.14).

## The public surface is capped

`index-crate-isolation` holds this crate to five root promises, no public module, and no public items inside one,
exactly what it exposes. Each has a caller in the host:

- Detection: `is_restic_repo` (the overlay asks it per listing).
- The route: `restic_route` and `VIRTUAL_ENTRY_NAME` (the host's lexical gate, and the row's name).
- The hub: `ResticRepos`, with `new`, `confirm_repo`, `serves_confirmed`, and `overlay_row`.
- The view: `ResticVolume`, with `new`.

`test_fixtures` is `testing`-gated and counts apart. Opening, the snapshot list, trees, and the adapter stay
crate-private: the volume that needs them lives here.
