# File system listing module

Backend directory reading, caching, sorting, and streaming: 100k+ entries, non-blocking, with progress.

## Module map

- **engine/** (`Listings`; `mod.rs` re-exports the API). Read: **streaming.rs**, **stall.rs**, **pipeline.rs**, **reread.rs**. App seams: **host/**. Cache and mutate:
  **cached_listing.rs**, **caching.rs**, **operations.rs**. Project and publish: **visible_rows.rs**, **path_index.rs**, **name_filter.rs**,
  **quick_filter.rs**, **diff.rs**, **diff_emitter.rs**. Sort: **sorting.rs**, **collation.rs**.
  `FileEntry` comes from `cmdr-fs`; the `std::fs` reader is the local backend's (`volume/backends/`).

## Invariants and gotchas

- **Neither a row number nor a path indexes `entries`.** Rows drop hidden, filtered-out, and scratch entries, so
  `CachedListing::rows` is the ONLY filter point, on READ; by-path callers go through `index_of_path` /
  `indices_of_paths`. ❗ A MUTATING caller resolves BEFORE `entries_mut`, which drops both maps. `entries` stays
  private: accessors that grew their own filter were each a row off.
- **Mutation owners allocate revisions and enqueue stamped batches under the cache write lock**: only `Publish`,
  borrowed from `write_cache`, writes the queue. The emitter coalesces transport, never boundaries or revisions.
  Rows use committed exact-path scratch decisions. Reconcile and publish scratch drift BEFORE an entry mutation or
  guarded index consumption, then check the expected revision. `DETAILS.md` § "Diff event coalescing".
- **Quick-filter changes share that revision** and drop superseded pending batches under the cache write lock,
  never after unlocking. Guard selection remapping before changing the filter. `DETAILS.md` § "The quick filter and in-flight diffs".
- **No full re-read lands after a newer one**: each carries the generation it STARTED reading at. `DETAILS.md` §
  "Read generations".
- **Overlay rows fold in through `pipeline.rs`'s `Prepared`**, the only entries a listing stores. `DETAILS.md` §
  "The overlay step".
- **A close notifies the observers (`host/observers.rs`) AFTER the cache removal**, ❌ never before: an observer's detached arm
  reconciles against cache membership.
- **The orphan reaper keys on `last_accessed_ms`**, bumped by reads, cache patches, and the panes'
  `keep_listings_alive` heartbeat (idle panes make no reads). ❌ Never from `refresh_listing_index_sizes`.
- **A streaming read's foreground hold lives in its `ReadSession`** (`streaming.rs`), for exactly the read's body.
- ❌ **A listing never aborts a read**, on cancel or when a retry wins: detaching lets it unwind; aborting wedges an
  MTP phone. A read quiet for `stall_after` emits `listing-stalled` and keeps waiting, ❌ never a deadline that ends
  it. `DETAILS.md` § "Stalled listings".
- **Every spawn goes through the engine's runtime**: a bare `tokio::spawn` panics on a watcher's OS thread.
- **Sorting has ONE comparator**, `entry_comparator` over `SortableEntry` (`FileEntry` plus a search-results row). ❌
  Never add a second. A sort change invalidates the frontend's cached range, so bump `cacheGeneration`.
- **Names rank by Unicode collation (`collation.rs`), ❌ never code points or `to_lowercase`** (macOS holds NFC and NFD
  side by side). Both readings, live `compare` and prebuilt `key`, end with a raw-bytes tiebreak, else two spellings of
  one name tie and the watcher sees a phantom `Move`. ❌ Never persist a `NameKey`.
- **A listing's path is a `ListingPath`, built only by `ListingPath::on_volume`** (the volume's spelling,
  `Volume::listing_path`). ❌ Never compare a raw path to it: MTP reports `mtp://…` where the pane holds `/DCIM`.
- **Only a pane open or a Finder drop/paste resolves a foreign spelling** (`list_as_stored`, `stored_spellings`): ❌
  never a walker, scan, or refresh, where a miss means gone and a resolve returns a look-alike twin. `DETAILS.md` § "A
  pane path the volume stores another way".
- **New listing state is an `Inner` field, ❌ never a `static`**, reached through `self`, ❌ never
  `listing::installed()`. Fixtures: `caching_test_support::TestListing`.
- **Finder tags are deferred**: `list_directory_core` never reads them, and every modify path calls
  `carry_forward_tags` BEFORE storing, else an mtime touch blanks a file's dots. ❌ Never route enrich through it.

Read `DETAILS.md` before non-trivial work here.
