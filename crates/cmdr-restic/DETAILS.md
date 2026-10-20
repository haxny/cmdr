# `cmdr-restic` details

The spec and the research behind it: GitHub issue #422. This crate is its foundation: the storage adapter, unlock, the
snapshot list, and detection. The virtual tree (`ResticVolume`), host routing, unlock UI, and copy-out build on it.

Written against `rustic_core` 0.13.0 (default features off). The issue was researched on 0.12.0; what moved is in §
"Against 0.13.0".

## The storage adapter (`backend.rs`)

`VolumeBackend` implements rustic's blocking `ReadBackend` + `WriteBackend` over a parent `Arc<dyn Volume>`, the repo's
folder on it, and a runtime `Handle`. It's the bridge `cmdr-archive`'s `VolumeByteSource` uses: each method `block_on`s
the parent's async call. That's only sound off the runtime's workers, so every rustic call runs on `spawn_blocking`
(`repo.rs`); rustic's own worker threads (index loading uses them) are outside the runtime too.

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

## Opening a repo (`repo.rs`)

`ResticRepo::open(parent, root, password, cache_dir, host)` builds the adapter, then on one blocking thread runs
`Repository::new(..).open(&Credentials::password(..))`. That lists the config, tries the password against each key
(scrypt, deliberately about a second per key), and reads the config. It does NOT load the index. The password copy the
blocking task carries is a `Zeroizing<String>`; rustic's `Credentials` makes its own plain copy, which we can't avoid.

`cache_dir: None` sets `no_cache`; `Some(dir)` lets rustic cache snapshot and index files there (never in the repo). The
host decides which after measuring a second open over SMB (#422 § 6).

No restic lock is taken on read: rustic 0.13.0 has no lock file type at all (see below), and
`opening_and_listing_never_write_to_the_repo` pins both zero write attempts and an empty `locks/`.

`snapshots()` calls `get_all_snapshots()`, which reads only `snapshots/<id>` files, and hands the result to
`snapshot_list` with the system time zone.

### Typed errors

`ResticError` is what a host words: `WrongPassword`, `NotARepository`, `Volume(VolumeError)`, and
`Unreadable { detail }` (rustic's `display_log()`, for the log only). `classify` sorts a `RusticError` by type:

1. **Wrong password**: `RusticError::is_incorrect_password()`. rustic attaches its own error code `C002` when no key
   accepts the password (`find_key_in_backend`), and this accessor is rustic's public, typed way to ask for it. The
   error's `kind` field isn't readable outside rustic in 0.13.0, so the code is the only typed signal there is. Pinned
   by `a_wrong_password_is_typed_and_the_right_one_then_opens`; if a rustic bump drops the code, that test fails.
2. **Parent failure**: the adapter wraps every `VolumeError` with `RusticError::with_source`, so it rides in the error's
   source chain; `volume_error_in` finds it by `downcast_ref`. Pinned by
   `a_parent_that_cant_answer_surfaces_its_typed_error`.
3. **Not a repo**: anything else, when the config is gone (one more typed `list_with_size(Config)` asks the parent).
4. **Unreadable** otherwise: a damaged repo, an unsupported version, or a crashed blocking task.

## Snapshot folders (`snapshots.rs`)

A snapshot folder name is its local time plus its short id: `2026-10-12 21.00.03 (a1b2c3d4)` (David's decision on #422).
Dots in the time, since `:` isn't a valid name character in Finder.

- **Order** is by time, then id. `latest()` is the last one, so a same-instant tie always resolves the same way and the
  `latest` folder never flips between refreshes.
- **Unique**: the id part is the shortest prefix, eight digits at least, that no other snapshot in the same local second
  starts with. Only the ids that clash grow; one in a different second keeps the short form.
- **Sortable**: sorting by name sorts by time, to the second. Within one second it's by id, and during a DST fall-back
  the repeated hour's two passes interleave by name (the list order stays true; only a name sort differs).
- **`latest`**: `LATEST_FOLDER_NAME` (David's decision 4). A dated name starts with a digit, so they can't collide.
  `SnapshotList::find` resolves either kind of name.

`snapshot_list` is a pure function of the raw snapshots and a `TimeZone`, so tests pin a zone (Stockholm) rather than
depend on the machine's.

## Detection (`detect.rs`)

A folder is a restic repo when its listing holds a regular file named `config` and the folders `data`, `index`, `keys`,
and `snapshots`. `locks` is optional: restic creates it lazily, and a copied repo often drops it. A symlink counts as
neither: restic never writes one, and its target is unknown from the listing alone. Names match exactly.

## Fixtures (`test_fixtures.rs`)

Under the `testing` feature (this crate turns it on for its own tests through a self dev-dependency), so the host's
tests build repos the same way. `FixtureRepo::init` (v2) and `init_v1`, then `add_snapshot` (an empty-tree snapshot, for
list tests) or `backup` (a real folder, for trees and contents). `write_into(volume, root)` lays the repo out on any
`Volume`; `files()` hands back the raw layout.

- rustic's `init` refuses `set_version = 1` as a downgrade, so `init_v1` hands `init_with_config` a v1 `ConfigFile`
  directly, with restic's test chunker polynomial and an all-zero repository id (the id type isn't public).
- The fixture holds the repo ids-indexed, since `backup` needs that and `OpenStatus` can't be cloned to get there twice.
- scrypt is slow unoptimized (about 8 s per unlock), so the workspace builds `scrypt`, `salsa20`, and `pbkdf2` at
  `opt-level = 3` even in debug (root `Cargo.toml`). Without that this suite takes 24 s; with it, 0.6 s.

## What the next layers build on

- **Index loading** (`to_indexed`) consumes `Repository<OpenStatus>`, and `OpenStatus` isn't `Clone`. `ResticRepo` holds
  it in an `Arc` for `snapshots()`, so the index step needs sole ownership: add a consuming method (for example
  `ResticRepo::into_indexed`), or keep it in a `Mutex<Option<..>>` and `take()` it. The ranged-read test does the
  `Arc::try_unwrap` by hand today.
- Tree browse and reads need `get_tree`, `node_from_path`, `open_file`, and `read_file_at` on the full index. All of it
  rides this adapter, so a pack read is a `read_range` window (pinned by
  `reading_a_tree_uses_ranged_reads_of_the_pack_never_the_whole_pack`).
- `Snapshot` carries what a row needs: `folder_name`, `time` (the row's modified date), `hostname`, `paths`, `tags`, and
  the full `id`.
- `ResticError::WrongPassword` is what maps to `VolumeError::NeedsPassword { wrong_attempt: true }` host-side.

## Against 0.13.0

What differs from the issue's 0.12.0 research, read from the 0.13.0 source:

- `WriteBackend::write_bytes` takes a `BytesList`, not `Bytes`.
- `FileType` has no `Lock` variant (only `Config`, `Index`, `Key`, `Snapshot`, `Pack`), so there's nothing to map to
  `locks/`. Detection still allows the folder.
- `ReadBackend` adds `warmup_path` (required) next to `needs_warm_up` / `warm_up` (defaulted).
- `Repository<OpenStatus>` isn't `Clone`.
- MSRV 1.91; new to the lockfile against 0.12.0's list: `itertools` 0.15 (a third major next to 0.13 and 0.14).

## The public surface is capped

`index-crate-isolation` holds this crate to six root promises, no public module, and five public items, exactly what it
exposes:

- Detection: `is_restic_repo` (the host's overlay asks it per listing).
- Unlocking: `ResticRepo`, with `open` and `snapshots`, and `ResticError`.
- The snapshot folders: `SnapshotList`, with `snapshots`, `latest`, and `find`; `Snapshot`; `LATEST_FOLDER_NAME`.

`test_fixtures` is `testing`-gated and counts apart. `VolumeBackend`, `snapshot_list`, and `RawSnapshot` stay
crate-private: the `ResticVolume` that needs them will live in this crate.
