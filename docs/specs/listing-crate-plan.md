# The listing engine becomes `crates/cmdr-listing`: execution plan

Just-in-time plan for GitHub issue #301. It moves the directory listing engine
(`apps/desktop/src-tauri/src/file_system/listing/`) into its own Tauri-free crate and, while doing it, gives the engine
the shape every other extracted subsystem has: **one handle that owns its state, built with explicit host seams**. The
playbook this plan assumes is `docs/guides/multi-agent-refactors.md`. Read the listing module's `CLAUDE.md` and
`DETAILS.md` (they hold the invariants this plan must keep), `crates/cmdr-archive/DETAILS.md` § "The pattern, for the
next backend crate", `crates/cmdr-fs/src/volume/host/CLAUDE.md`, and `crates/cmdr-index/src/indexing/host/CLAUDE.md`
before executing any milestone.

David's brief: make the architecture super nice. A non-Tauri consumer must be able to drive the engine; every app
concern (Tauri events and `AppHandle`, analytics, MCP, fonts, intl, TCC, priority, the volume registry, the drive index)
is an explicit seam the app answers, never a reach upward. Prefer making wrong usage unrepresentable over documenting a
rule. Optimize for the ideal end state. No plugins, no TUI, no WASM build yet, but nothing in the library may make a
later WASM build harder for free.

## Loud rules (read before touching anything)

- **Byte-identical behavior, except the six changes in § "Behavior changes, called out"** and the standalone bug fix in
  § "Before M0". Each lands in its own commit with a red test first and is named in the commit body. Anything else that
  changes behavior is a bug in the milestone, not a judgment call.
- **The TS bindings must not move.** Every event name, payload shape, and type name the frontend sees stays the same.
  `desktop-bindings-fresh` regenerates `apps/desktop/src/lib/ipc/bindings.ts` and must report zero diff on every
  milestone. This is the proof that moving `tauri_specta` derives app-side lost nothing.
- **Characterize first.** M0 lands before any production change. Pin reality, not this plan's expectations, and verify
  every new timing- or order-sensitive pin goes red when you break the thing it pins.
- **`main` ships after every milestone.** Each milestone compiles, passes its lanes, and leaves the app working. The
  temporary shims in M3a exist for exactly this; they die in M4b. Each milestone is sized for one executor in a few
  hours; when one runs long, stop at a green commit boundary and hand the rest on, never leave `main` mid-milestone red.
- **Every invariant in `file_system/listing/CLAUDE.md` survives.** The footer lists them, plus the ones this refactor
  adds. Where this plan turns a rule into a type, the rule's doc line shrinks or goes; it never silently disappears.
- **No `tokio::spawn`, `tokio::task::spawn_blocking`, or `std::thread::spawn` in the engine.** Everything spawns on the
  injected runtime handle. A watcher OS thread has no reactor, and that's how `tokio::spawn` takes the app down.
- **The host is a value, not a static.** No new `static` in the engine, and every existing one becomes a field of the
  handle (M3a). The two process-wide registries the engine reads (`cmdr_fs::staging`'s in-flight set and the
  `NameCollator` data) stay where they are; neither is listing state.
- **A seam is called per read or per mutation, never per entry** (`crates/cmdr-fs/src/volume/host/CLAUDE.md` § "The
  dispatch rule"). Hoist and batch: `ListingIndex::enrich` takes a slice.
- **No seam, observer, or overlay is called while an engine lock is held**, and a closure the API runs under the cache
  lock (`with_rows`, the ONLY one left after M4a; see § "Public API") must not call back into `Listings`. Today's code
  honors the first half everywhere but ONE place: `refresh_listing_index_sizes` runs `index().enrich` (two SQLite
  queries) under the cache WRITE lock, once per index event per pane. That's behavior change 5, fixed in M3c. The
  index-size observer READS the cache from inside `listing_opened`, so an observer call under the write lock would
  deadlock on the first open. Resolve the collator (`ReadingLocale`) and the scratch visibility BEFORE taking the cache
  lock and pass them in as values.
- **Event payload types, their `tauri_specta` derives, and every English word stay app-side.** The engine reports typed
  facts; the app decides what the user sees. `index-crate-isolation` forbids `tauri-specta` in the crate anyway.
- **Never a bare `cd`** in an executor (it moves every agent). Use `git -C` or `(cd /abs && …)`, foreground only.
- **Allowlists:** `file-length` / `jscpd-rust` entries re-path when files move (the call is yours, with a `reason`);
  `lock-poison` entries for listing files must reach ZERO in M3a, never re-path; a new `module-cycles` home or a new
  `index-crate-isolation` ceiling entry needs David's OK (the initial ceiling is on the list below).

## Decisions for David (and for the review rounds to challenge)

1. **The engine becomes a handle, `cmdr_listing::Listings`**, built by `Listings::builder()`, cloned cheaply (`Arc`
   inside), held app-side in a `OnceLock` behind `listing_host::listings()`, exactly the `index_host::index()` shape.
   The alternative, keeping `LISTING_CACHE` and the five other statics as crate statics, would be a smaller diff and the
   wrong end state: it keeps every test sharing one cache (unique ids, scoped reaper sweeps, and the `ShowTempsGuard`
   mutex that serializes every scratch-visibility test), keeps the cache global for any future consumer, and contradicts
   the module's own "new listing state hangs off a struct" rule. (`TestListingGuard` itself is RAII teardown, not a
   lock.)
2. **The seams are eight small traits in `cmdr_listing::host`** (§ "Seam design"), mirroring `cmdr-index`'s
   `indexing/host/`, not one fat `ListingHost` trait. Each one answers one question, degrades to a no-op, and carries a
   recording fake under the `testing` feature.
3. **The engine reports facts; the app's event adapter owns all policy on them.** Error classification into
   `ListingError`, the damaged-archive and provider enrichment, the TCC restricted-empty-root hint, analytics, the MCP
   recent-errors ring, `restricted_paths` record/clear, and `note_root_failure` all move into the app's `ListingEvents`
   implementation. The engine knows nothing about TCC, PostHog, or MCP.
4. **The FSEvents watcher becomes the first registered `ListingObserver`**, folding the "arm a watch" and "tell the
   lifecycle observers" steps into one ordered list. Its disarm therefore moves after the cache removal (behavior change
   2 below). Rejected alternative: a dedicated `ListingWatches` seam, which would keep two lists for one concept.
5. **What stays app-side:** `reading.rs` and `mutation.rs` (they are the local backend's reader, not the engine),
   `brief_columns.rs` (pane layout over font metrics; it reads rows through the engine's public row view), the
   `ListingHost` adapter for backends, the watcher's OS half, and every wire event struct.
6. **Initial `index-crate-isolation` ceilings for `cmdr-listing`, measured at M5 with no headroom.** Precedent is that
   each extraction sets its own; this plan asks David to approve that entry as part of approving the plan.
7. **A listing id is a `ListingId` newtype in the Rust API**, never a bare `&str`. Today a listing id and a volume id
   are both `&str` and sit side by side in a dozen signatures (`listing_opened(listing_id, volume_id, …)`,
   `location(id) -> (volume_id, …)`), which is exactly the confusable-positional-params shape `AGENTS.md` bans. The IPC
   commands keep their `String` parameters and wrap at the boundary, so `bindings.ts` doesn't change (a `specta::Type`
   newtype would add an exported type and break `desktop-bindings-fresh`). Volume ids stay `&str`: they are `cmdr_fs`
   vocabulary, out of scope.
8. **`Listings::builder(volumes, events)` takes the two seams a real consumer can't do without as arguments**; the other
   six are optional with no-op defaults, and `Listings::detached()` (tests, tools) defaults all eight. This diverges
   from `Index::builder()` / `VolumeHost::builder()`, where every seam is optional, on purpose: an engine with no
   volumes lists nothing and one with no events leaves every pane on "loading", so forgetting either is a bug the
   compiler should catch, not a degraded mode.
9. **The engine answers `cmdr_fs`'s `ListingHost` itself, panes-only:**
   `Listings::listing_host() -> Arc<dyn ListingHost>`, holding a `Weak<Inner>` (a strong one would close a cycle
   `Listings` → `ListingVolumes` → volume → `VolumeHost` → `Listings` for any consumer that hands volumes over as
   values; it degrades to the no-op answers once the engine is gone). A non-Tauri consumer wires backends to the engine
   in one line. The app's `backends` adapter delegates the oracle, open-listing, and archive-refresh methods to it and
   keeps only `directory_changed`, which goes through the composer (index → badges → panes).

## Fresh survey (run 2026-10-11 on `1fd54c18f`, this worktree)

Sizes: 7,706 lines of production code in 28 non-test files, 9,879 lines of tests in 28 `*_test.rs` files plus
`caching_test_support.rs` and `sorting_test_support.rs`. The issue's count (about 6,100) predates `stall.rs`,
`stalled_on.rs`, and `foreign_path.rs`.

### `tauri` inside `listing/` (production code)

- `streaming.rs`: seven event structs with `tauri_specta::Event` derives (`listing-progress`, `-complete`, `-error`,
  `-cancelled`, `-read-complete`, `-opening`, `-stalled`); `TauriListingEventSink` holding an `AppHandle`;
  `list_directory_start_streaming(app: AppHandle, …)`; a bare `tokio::spawn`.
- `diff.rs`: `DirectoryDiff` derives `tauri_specta::Event` (`directory-diff`).
- `diff_emitter.rs`: `tauri::async_runtime::spawn` for the 50 ms flush; emits through `WATCHER_MANAGER.app_handle`.
- `foreign_path.rs`: `ListingRespelledEvent` (`listing-respelled`); two `tauri::async_runtime::spawn`; emits through
  `WATCHER_MANAGER.app_handle`.
- `caching.rs`: `tauri::async_runtime::spawn` in `spawn_full_refresh`; the `has_app` gate reads
  `WATCHER_MANAGER.app_handle`.
- `orphan_reaper.rs`: `tauri::async_runtime::spawn`.
- `listing_host.rs`: `tauri::async_runtime::block_on` (tests only).

### App-crate reaches out of `listing/` (production code), sorted into the issue's three kinds

(a) moves into the crate, (b) is or belongs in `cmdr-fs`, (c) stays in the app behind a seam.

- `file_system::volume::manager::get_volume_manager()` (`.get`, `.resolve`, `.resolve_local_only`, `RoutedKind`) in
  `cached_listing.rs`, `caching.rs` ×3, `foreign_path.rs` ×2, `operations.rs`, `streaming.rs`: **(c)** `ListingVolumes`;
  `RoutedKind` and `ResolvedVolume` **(b)** to `cmdr_fs::volume`.
- `file_system::volume::mount_registration::adopt_mount_serving` + `deadline::blocking_with_timeout` (`streaming.rs`)
  and `unregistered_volumes::why_unregistered` (`streaming.rs`): **(c)** one `ListingVolumes::unregistered` call.
- `file_system::volume::note_root_failure` (`streaming.rs`, test-only `operations.rs`): **(c)** the event adapter.
- `file_system::volume::friendly_error::*` (`streaming.rs`, `stall.rs`): already **(b)** (`cmdr_fs::volume`); the
  streaming uses move to the app adapter, `stall.rs`'s transient check stays.
- `index_host::index()` `.enrich` / `.verify_directory` (`caching.rs` ×5, `foreign_path.rs`, `operations.rs` ×3,
  `streaming.rs` ×2): **(c)** `ListingIndex`. `.apply_directory_change` (`caching.rs`): **(c)**, but it leaves the
  engine entirely; see the app composer in § "Seam design".
- `file_system::sync_status::invalidate_dir` (`caching.rs`): **(c)**, leaves the engine with `apply_directory_change`.
- `listing_overlays::decorate` (`caching.rs`, `foreign_path.rs`, `operations.rs`, `streaming.rs`, and
  `file_system/watcher.rs`): the trait **(a)**, the registry becomes a builder input. **Five call sites, not the three
  the docs claim**: the respell re-read and the watcher's full re-read are the undocumented two.
- `listing_lifecycle::{listing_opened, listing_closed}` (`operations.rs` ×2, `streaming.rs`): the trait **(a)**, the
  registry becomes a builder input.
- `file_system::watcher::{start_watching_detached, stop_watching, WATCHER_MANAGER}` (`operations.rs`, `streaming.rs`,
  `caching.rs`, `diff_emitter.rs`, `foreign_path.rs`): **(c)** the watcher becomes a `ListingObserver`; its `app_handle`
  uses become `ListingEvents`.
- `priority::foreground::lease_foreground_on` (`streaming.rs`): **(c)** `ForegroundLeases`.
- `intl::active_locale` (`collation.rs`): **(c)** `ReadingLocale`. ❗ `entry_comparator` is NOT pure today: it calls
  `active_collator()` inside, as does `sort_entries`. Both gain an explicit `&NameCollator` parameter, and every
  internal caller (`insert_entry_sorted`, `update_entry_sorted`, resort, `CachedListing` replace) resolves it from the
  handle before taking the cache lock. `commands/search.rs` passes `listings().collator()`.
- `volumes::mount_type_and_source_for` (macOS), `file_system::linux_mounts::mount_entry_for_path` (Linux)
  (`stalled_on.rs`): **(c)** `MountTable`. `file_system::index_provider::mount_is_local_disk` is a pure string
  allowlist: **(b)** `cmdr_fs::filesystem_kind`.
- `file_system::staging::{show_staging_temps, show_safe_save_files, could_be_hidden_from_listings, hidden_with_settings}`
  (`visible_rows.rs`): the pure name rules **(b)** `cmdr_fs::staging`; the two user settings become engine state
  (`Listings::set_scratch_visibility`), which the app's settings live-apply calls. ❗ `ScratchProjection::for_entries`
  and the drift checks are methods on `CachedListing` / `VisibleRows` that read the globals today, including from
  `CachedListing::new` OUTSIDE the cache lock; they take a `Copy` `ScratchVisibility` argument instead, read once per
  operation from the handle. `CachedListing` never holds a back-reference to the engine. Drift stays lazy
  (`reconciled_cache` passes the current value), exactly as today.
- `analytics::events::capture`, `mcp::listing_errors::record` ×2, `restricted_paths::{record_denial, clear_path}`
  (`streaming.rs`): **(c)** the app's `ListingEvents` adapter.
- `font_metrics::calculate_max_width_with_suffixes` (`brief_columns.rs`): `brief_columns.rs` stays in the app.
- `file_system::macos_metadata::get_macos_metadata` (`reading.rs`): `reading.rs` stays in the app.
- `benchmark::{reset_epoch, log_event, log_event_value}` (`operations.rs`, `reading.rs`, `streaming.rs`, and
  `commands/file_system/listing.rs`): pure std diagnostics gated on an env var, **(b)** `cmdr_fs::benchmark`.
- `cmdr_archive::path_targets_archive_file` (`streaming.rs`): moves to the app adapter with the rest of error
  classification. `cmdr_archive::archive_boundary_candidate` (`caching.rs`, the oracle's honesty guard): pure name
  slicing, **(b)** `cmdr_fs::archive_format` beside `has_supported_archive_extension`. The crate must not depend on
  `cmdr-archive` (it would drag every codec into the listing engine).
- `ignore_poison`, `name_fold`, `entry` (`FileEntry`, `TagRef`): already **(b)**, re-pointed at `cmdr_fs` directly.

Statics that become handle fields: `LISTING_CACHE`, `LISTING_EPOCH` (`cached_listing.rs`), `STREAMING_STATE`
(`streaming.rs`), `PENDING_DIFFS` (`diff_emitter.rs`), `REFRESH_SLOTS` (`caching.rs`), `PRODUCTION_READS` (`stall.rs`),
the collator cache (`collation.rs`), plus the app's `OVERLAYS` and `OBSERVERS` registries and the two scratch-visibility
`AtomicBool`s. `FAIL_NEXT_WIDTH_CALLS` leaves with `brief_columns.rs`.

### Inbound callers (outside `listing/`)

126 files name `file_system::listing`; 86 + 17 of the references are only `FileEntry`, which is `cmdr_fs::entry` under
an alias. The production callers that reach past the public functions into internals are the ones the API design has to
answer:

- **`LISTING_CACHE` directly:** `listing_index_sizes/{mod.rs, count/mod.rs, count/publish.rs}` (reads a listing's volume
  and path, reads folder entries by name, writes index sizes through `update_index_sizes_by_path`),
  `multi_rename/session.rs` (entries by name, `touch`), `file_system/watcher.rs` (membership, full re-read).
- **`ListingPath` / `snapshot_listings` to find a listing by directory:** `mcp/executor/tags.rs`.
- **Watcher primitives:** `file_system/watcher.rs` imports `has_entry`, `insert_entry_sorted`,
  `remove_entries_by_paths`, `update_entry_sorted`, `carry_forward_tags`, `get_listing_entries`,
  `update_listing_entries`, `get_listing_volume_id_and_path`, `flush_all_pending`, and runs its own enrich and
  `decorate`.
- **The fresh-listing oracle:**
  `write_operations/{scan.rs, scan_walker.rs, conflict_preflight.rs, look_alike.rs, delete/walker.rs, scan_preview/volume.rs}`
  via `try_get_authoritative_listing`; backends via `AppListings`.
- **Change intake:** `volume_host.rs` (`AppListings`), `file_system/git/wiring.rs` (`DirectoryChange`,
  `find_listings_for_path_on_volume`, `get_listing_path`), `file_system/git/arming.rs`,
  `volume/backends/ local_posix.rs` (`find_listings_for_path_on_volume`, `patch_listing_after_local_mutation`, the local
  reader), `network/smb_upgrade.rs` (`respell_listings_on_volume`).
- **Commands and IPC:** `commands/file_system/listing.rs` (most of the API),
  `commands/file_system/{drag.rs, stored_spelling.rs, e2e_support.rs}`, `commands/clipboard.rs`, `commands/search.rs`
  (`SortableEntry`, `entry_comparator`, the collator), `services_menu/selection.rs`, `ai/suggestions.rs`,
  `network/smb_pane_upgrade.rs`, `ipc.rs` (event and type registration), `menu/menu_handlers.rs` and
  `mcp/executor/tags.rs` (`apply_tags_to_listing`), `mcp/resources/mod.rs` and `priority/host_policy.rs`
  (`snapshot_listings`), `agent/tools/read/pane_listing.rs` (`get_cached_listing`),
  `file_system/write_operations/types.rs` (`SortColumn`, `SortOrder`).
- **Missed by the first survey (review round 1):**
  - `write_operations/create.rs::emit_synthetic_entry_diff` (production, also the paste writer): stats the new entry,
    enriches it itself, then `find_listings_for_path` (volume-LESS) + `insert_entry_sorted` per listing. It bypasses the
    composer (no index apply, no badge invalidation) and `notify_added`'s upsert. The volume-less match is a live bug (a
    local create at `/tmp/x` adds a row to an SFTP pane showing a remote `/tmp`), fixed in § "Before M0"; the re-route
    through the composer is behavior change 6 (M4a).
  - `commands/e2e.rs::flush_file_watcher` → `watcher::flush_all_watchers` → `handle_directory_change` per watch, then
    `diff_emitter::flush_all_pending`. E2E relies on the re-read having LANDED when it returns (behavior change 3).
  - `commands/settings.rs::set_show_virtual_git_portal` → git wiring's refresh; its "async is load-bearing because
    `notify_directory_changed` calls `tokio::spawn`" comment goes stale once spawns use the injected runtime (M2a
    updates it).
  - `listing/listing_host.rs` (`AppListings`, the backends' `cmdr_fs` `ListingHost`) collides by name with the new app
    module `crate::listing_host`. M2a moves it in as `crate::listing_host::backends`.
  - Test-support users outside `listing/`: `volume/manager/git_routing.rs` (`WatchCoverageVolume`),
    `volume/mount_registration.rs` (`unique_test_id`), `file_system/tags.rs` (`list_directory_core`, moves with M1).
- **Tests outside `listing/` that drive the engine:** `adb/volume_wiring_test.rs`, `server_volumes_test.rs`,
  `file_system/volume/inmemory_test.rs` (`STREAMING_STATE`, `read_directory_with_progress`), and about 20 files using
  `caching_test_support::{TestListing, TestListingGuard, unique_test_id, WatchCoverageVolume}`.

### Tests inside `listing/` that depend on the app (they stay app-side, as integration cells)

`archive_watch_integration_test.rs` (`cmdr_archive` + app routing), `smb_pane_close_watch_integration_test.rs`
(`cmdr_smb` + the app watcher), `mtp_listing_path_test.rs` (`virtual-mtp`), `overlay_refresh_test.rs` (the git overlay
and the watcher's re-read), `brief_columns_test.rs` (font metrics), the `smb_upgrade` half of `foreign_path_test.rs`,
and one lease cell from `streaming_test.rs` proving `AppLeases` reaches `priority::foreground`. Everything else moves
into the crate and stops touching the app once it builds its own engine.

## Target crate layout

```
crates/cmdr-listing/
  Cargo.toml          cmdr-fs, serde, specta (=2.0.0-rc.24), log, tokio (rt, sync, time, macros), tokio-util,
                      icu_collator (unstable), icu_locale_core. Features: `testing`, `playwright-e2e`.
                      Self dev-dependency for `testing`; dev tokio adds rt-multi-thread + test-util.
  clippy.toml         the root config plus `disallowed-methods` for tokio::spawn, tokio::task::spawn_blocking,
                      std::thread::spawn (see M5 landmine: verify clippy reads a member-level clippy.toml).
  CLAUDE.md, DETAILS.md
  src/lib.rs          #![deny(missing_docs)], #![warn(unused_crate_dependencies)]; the root re-exports ARE the API.
  src/engine/         `Listings`, `ListingsBuilder`, `Inner` (the state), and the handle's methods split by caller
                      group (open.rs, rows.rs, settings.rs, intake.rs, oracle.rs, housekeeping.rs).
  src/host/           pub mod: the eight seam traits, their no-op defaults, `testing` fakes. CLAUDE.md + DETAILS.md.
  src/cache/          cached_listing.rs, listing_path.rs, mutate.rs (was caching.rs's patch helpers), path_index.rs,
                      orphan_reaper.rs.
  src/rows/           visible_rows.rs, name_filter.rs, scratch.rs (the projection), compare.rs, fuzzy_jump.rs.
  src/publish/        diff.rs, emitter.rs (was diff_emitter.rs), the `Publish` token.
  src/read/           streaming.rs, stall.rs, stalled_on.rs, foreign_path.rs, refresh.rs (the turnstile),
                      pipeline.rs (the ONE enrich → overlay step, see M3b).
  src/sort/           sorting.rs, collation.rs.
  src/testing/        `testing`-gated: `TestEngine`, `TestListing`, `WatchCoverageVolume`, and one fake per seam
                      (decision 2): `FakeVolumes`, `RecordingEvents`, `FakeIndex` (scripted sizes, enrich
                      counter, optional gate to hold an enrich open), `StaticOverlay`, `RecordingObserver`
                      (records open/close and the cache membership it saw), `RecordingLeases`, `FixedLocale`,
                      `FakeMountTable`.
```

Only `host` (and the gated `testing`) are public modules; everything else reaches the root as a re-export, so the public
surface is the list in `lib.rs` and nothing a sibling module happens to mark `pub`. The `src/*/` grouping is a
suggestion for M5 to finalize against `module-cycles`: if a grouping creates a sibling tangle that the flat layout
didn't, keep the flat layout for that pair rather than asking for an allowlist entry.

## Public API (the shape M4a programs against, measured into ceilings at M5)

`Listings` methods, grouped by who calls them. Names are a proposal; the executor keeps the existing verb where one
exists so the IPC pass-throughs read one-to-one.

- **A pane opens and closes a listing:** `start(ListingRequest)` (spawns the streaming read on the host runtime, returns
  immediately), `cancel(id)`, `end(id)`, `keep_alive(ids) -> Vec<ListingId>`.
- **Row-space reads** (every index is a pane row; no entry index crosses this boundary, by construction): `file_range`,
  `total_count`, `find_file_index`, `find_file_indices`, `file_beside`, `file_at`, `paths_at_indices`,
  `files_at_indices`, `stats`, `selection_snapshot`, and `with_rows(id, include_hidden, |VisibleRows| …)` for app
  features that lay rows out (Brief columns).
- **Pane settings:** `resort`, `set_include_hidden`, `set_name_filter`, `set_name_filter_guarded`,
  `set_scratch_visibility` (engine-wide).
- **Features:** `compare_directories`, `fuzzy_find_first_match`.
- **Change intake:** `directory_changed(volume_id, parent, DirectoryChange)` (panes only; the app composes the index and
  badge halves around it), `observe_children(id, Vec<(PathBuf, Option<FileEntry>)>)` (the watcher's incremental path:
  the engine classifies add/remove/modify, enriches, carries tags forward, and publishes, in today's removes → adds →
  modifies order), `refresh(id) -> RefreshOutcome` (the watcher's and ⌘R's full re-read; NOT through the turnstile, see
  behavior change 3), `begin_read(id) -> Option<ReadTicket>` + `refresh_from_entries(ticket, entries)` (the watcher's
  unregistered-volume `std::fs` fallback; the ticket is taken BEFORE the app reads, see behavior change 3).
  `RefreshOutcome` is `Landed`, `Superseded` (a newer read already published), `DirectoryGone` (the app emits
  `directory-deleted` and stops the watch, as today), `NoVolume` (nothing serves the listing's volume id: the app's cue
  to run `begin_read` → `list_directory_core` → `refresh_from_entries`, which is today's in-function fallback split at
  the seam), `ListingGone`, or `Skipped` (`PermissionDenied` silently, any other read error logged at warn, as today).
  `refresh_archive_listings` (async), `respell_on_volume`, `apply_tags`,
  `update_index_sizes(id, &[(path, IndexSizes)])`, `refresh_index_sizes(id)`. `IndexSizes` is the seven `recursive_*`
  fields as one `Copy` value: data, not a closure, so the app can't touch a name or path under the cache write lock
  (today's `FnMut(usize, &mut FileEntry)` can, and a renamed entry would desync the path index).
- **Lookups and oracles:** `authoritative_listing`, `cached_entries(volume_id, path)`,
  `listings_for_path_on_volume(volume_id, path) -> Vec<ListingId>` (callers only use the ids; no sort tuple leaks),
  `listings_for_local_path(path) -> Vec<ListingId>` (today's volume-less `find_listings_for_path`, renamed for what it
  does: a verbatim path match across EVERY volume, for `LocalPosixVolume::listing_watch_coverage`, which doesn't know
  its own id), `location(id) -> Option<(String, PathBuf)>`, `contains(id)`, `volume_ids_with_listings`,
  `summaries() -> Vec<ListingSummary>`, `folder_entries_named(id, names) -> Vec<FileEntry>` and
  `entries_named(id, names) -> Vec<FileEntry>` for the index-size and multi-rename readers (by name, never by position;
  owned clones of a handful of rows, so no app closure runs under the lock).
- **Housekeeping:** `start_orphan_reaper()`, `collator() -> Arc<NameCollator>` (for search's sort), `listing_host()`
  (decision 9), `scratch_visibility() -> ScratchVisibility` (the settings command flips one flag of two),
  `flush_pending_diffs()` behind the `playwright-e2e` feature (precedent: `cmdr-fs`'s feature of the same name).
- **Pure functions at the root:** `entry_comparator(collator, …)`, `SortableEntry`, `stored_spellings(volume, paths)`.
- **Types at the root:** `Listings`, `ListingsBuilder`, `ListingRequest`, `SortColumn`, `SortOrder`,
  `DirectorySortMode`, `NameCollator`, `VisibleRows`, `DirectoryDiffBatch`, `DiffChange`, `DiffChangeType`,
  `ListingLookupError`, `ListingStats`, `ResortResult`, `RowBeside`, `SelectionSnapshot`, `NameFilterResult`,
  `CompareDirectoriesMode`, `CompareDirectoriesError`, `CompareDirectoriesResult`, `FuzzyJumpError`, `StalledOn`,
  `ListingSummary`, `ScratchVisibility`, `RefreshOutcome`, `ListingId`, `ReadTicket` (opaque, `!Clone`), `IndexSizes`,
  `OpenedListing`.

Stays private, by type: `CachedListing`, `ListingPath`, `OverlayRows`, `PathIndexCache`, `ScratchProjection`, every
entry index, the diff queue, the turnstile map, `HungReads`, `StallPolicy` (a `testing` builder knob only).

## Seam design (`cmdr_listing::host`)

Each trait is `Send + Sync`, answered app-side by one adapter in the new app module `listing_host/` (beside
`index_host.rs` and `volume_host.rs`), defaults to a no-op under `Listings::detached()`, and has a recording or scripted
fake under `testing`. The no-op defaults are public unit types in `host` (`NoVolumes`, `NoEvents`, …), so a caller can
pass one to `Listings::builder` explicitly: the app's test-binary fallback is `builder(AppVolumes, NoEvents)`.

1. **`ListingVolumes`: which volume serves a path.** `get(volume_id)`, `resolve(volume_id, path)` (async, routes archive
   and git-portal paths), `resolve_local_only(volume_id, path)` (sync, for the oracle), and
   `unregistered(volume_id, path) -> Unregistered` (async): `Adopted(Arc<dyn Volume>)`, `NotConnected`, or `Gone`. The
   last one folds today's mount adoption (with its 2 s deadline, app-side) and `why_unregistered` into ONE question,
   "nothing serves this id; what now?", so the engine can't adopt without asking why or vice versa. App: `AppVolumes`
   over `VolumeManager`. Default: nothing registered, `Gone`. Why a seam and not `cmdr-index`'s `VolumeProvider`: that
   one answers mount identity for scanning, this one answers routing for reading; sharing a trait would couple two
   crates through methods neither needs.
2. **`ListingEvents`: what happened to a listing.** `opening`, `stalled(StalledOn)`, `progress(loaded)`,
   `read_complete(total)`, `finished(ListingEnd)`, `diff(id, Vec<DirectoryDiffBatch>)`, `respelled(id, &Path)`. Every
   method takes `&ListingId` first. `ListingEnd` is
   `Complete { total_count, volume_root, stored_path, empty_at_volume_root }`, `Cancelled`, or
   `Failed { error: VolumeError }`, each with the volume id and path. Why the facts and not wire events: the
   restricted-empty-root hint, error classification, analytics, the MCP ring, and TCC bookkeeping are product policy,
   and pinning them in M0 against today's `TauriListingEventSink` lets the adapter prove it kept every one. App:
   `TauriListingEvents`, which also owns the wire structs (moved from `streaming.rs`, `diff.rs`, `foreign_path.rs`,
   names and derives unchanged). Default: drops everything.
3. **`ListingIndex`: what the drive index knows.** `enrich(volume_id, &mut [FileEntry])`,
   `verify_directory(volume_id, path)`. App: `AppListingIndex` over `index_host::index()`. Default: no-op. ❗ The trait
   doc states the contract behavior change 5 relies on: `enrich` writes only the seven `recursive_*` fields, and only on
   non-symlink directories (verified in `cmdr_index`'s `enrich_entries_with_index_on_volume` / `apply_dir_stats`,
   2026-10-11 on `7be64c4ff`); `FakeIndex` honors it. `apply_directory_change` is NOT here: it's a different subsystem
   hearing the same change, so the app's composer `listing_host::directory_changed` calls the index, then
   `sync_status::invalidate_dir`, then `listings().directory_changed`, in today's order. `AppListings` (the backends'
   `cmdr_fs` `ListingHost`), git wiring, the local mutation patch, and the rename command all call the composer.
4. **`ListingOverlay`: rows a pane sees that no volume holds.** Today's trait, unchanged in shape, moved in. Builder
   input `.overlay(Arc<dyn ListingOverlay>)`, in registration order. App: the git portal's `GitPortalOverlay`.
5. **`ListingObserver`: what keeps something alive while a pane shows a directory.** Today's `ListingLifecycle`, renamed
   for the crate's vocabulary. Builder input `.observer(…)`, called in registration order, outside every engine lock, on
   open (after the cache insert, and after the respell kick-off in the streaming path, as today) and close (after the
   cache removal). App, in this order: the FSEvents watcher (arms only when `volume.can_watch_listings()`), the git
   portal's `GitPortalListings`, `listing_index_sizes`' `IndexSizeListings`. ❗ `listing_opened(&OpenedListing)` takes
   one struct (`id: &ListingId`, `volume_id: &str`, `volume: &dyn Volume`, `path: &Path`, the stored spelling as a plain
   path since `ListingPath` is private), never positional `&str`s: `IndexSizeListings` reads the volume id and stored
   spelling back out of `LISTING_CACHE` today, and through a handle that reach would have to go via
   `listing_host::listings()`, which in a test engine is the WRONG engine. Passing the facts makes the reach
   unnecessary.
6. **`ForegroundLeases`: the user is waiting on this volume.** `hold(volume_id) -> ForegroundHold`, where
   `ForegroundHold` is a `#[must_use]` RAII box. App: `AppLeases` over `priority::foreground::lease_foreground_on`.
   Default: a hold that holds nothing. The engine stores the hold inside its `ReadSession` (M3b), so "never bind it to
   `_`" stops being a rule anyone can break.
7. **`ReadingLocale`: which language names collate in.** `locale() -> String`, pulled once per sort (today's "pulled,
   not pushed" semantics). App: `intl::active_locale`. Default: `"en"`. Tests pass a fixed locale instead of locking the
   app's process-wide one.
8. **`MountTable`: what a filesystem path is mounted on.** `mount_of(path) -> Option<MountFacts { fs_type, source }>`,
   called lexically first and then, only when the answer was a local disk, on the host runtime's blocking pool for the
   symlink-resolved path under the existing 500 ms bound. App: `getfsstat` on macOS, `/proc/mounts` on Linux. Default:
   `None` (`StalledOn::Unknown`). The network-type allowlist and the backend-kind mapping stay in the engine.

Plus the runtime: `.runtime(tokio::runtime::Handle)`, defaulting to `cmdr-fs`'s fallback handle, which is the same thing
`VolumeHost` does.

## Behavior changes, called out

Each one lands as its own commit with a red test first, and the commit body names it.

1. **No more "no `AppHandle` yet" gate** (`notify_directory_changed`'s `has_app`). Before setup finished, and in every
   test, a directory change used to no-op; now it patches the panes, and the events seam drops what nobody listens to.
   In production no listing exists before setup, so nothing observable changes; in tests, code paths that were silently
   skipped start running (M2b landmine).
2. **The FSEvents watch disarms after the cache removal, not before**, because it's now an observer. Safe for the same
   reason the existing close-after-removal rule exists: `arm_and_reconcile` already reconciles against membership, and
   an event landing in the gap finds no listing.
3. **No full re-read of a listing can land after a newer one already did.** Today the watcher's re-read (⌘R, the
   post-operation top-up, the over-500 / unknown-kind / root-identity fallbacks, `flush_file_watcher`) skips the
   per-directory turnstile, so it can interleave with a `FullRefresh` or a respell and land an older read last. ❌ Don't
   fix it by putting `refresh` through the turnstile (the first draft did); review round 1 found three breaks: (a)
   `notify_full_refresh` RETURNS EARLY when the waiting berth is taken, so an awaited `refresh` would return before its
   read landed and `flush_file_watcher` (E2E) would go flaky; (b) a turn held by a read that stalled on a dead mount is
   held forever (invariant 9: never aborted), so after `note_root_failure` moves the volume, ⌘R would queue behind the
   corpse instead of reading through the recovered mount; (c) the turnstile keys on the notifier's raw `parent_path`,
   while the watcher and respell key on the listing's `ListingPath`, which differ for MTP (`mtp://…` vs `/DCIM`), so the
   two would not even share a slot. **Instead:** every full re-read (FullRefresh, respell, `refresh`,
   `refresh_from_entries`, `Replaced`) takes a per-listing read generation from the handle at the moment it STARTS
   reading, and the replace step drops the result under the cache write lock when a newer generation already published.
   The turnstile stays for what it's good at (bounding a storm to two reads), unchanged. Precisely when "starts" is
   (review round 2), because a generation taken too early drops a FRESHER read:
   - `FullRefresh` takes it AFTER acquiring its turn, immediately before `list_directory`, never at spawn or while
     queued (a queued refresh holding an early generation would lose to a ⌘R that started later but read earlier).
   - `refresh` and the respell re-read take it after resolving the volume, immediately before `list_directory`.
   - `refresh_from_entries` takes it through `begin_read(id) -> Option<ReadTicket>` BEFORE the app's `std::fs` read, so
     the generation dates the app's read, not the hand-over. `None` means the listing is gone; the app skips the read.
   - `Replaced` takes it on arrival. The backend's own read predates that by a little; accepted, since a `cmdr_fs`
     signature change would be needed to date it and the backend serializes its own re-reads.
   - The streaming read's cache insert publishes generation 0 for the listing, so any later re-read wins.
   - Incremental patches (`Added` / `Removed` / `Modified` / `observe_children`) do NOT take or bump a generation: today
     a re-read that started before a patch can land after it and carry the older state, and that stays as is (bumping
     would drop every re-read during a busy burst). Name it in the commit body so nobody "fixes" it silently. Red test:
     a slow `FullRefresh` started first and a ⌘R started second that finishes first; the older read must not land.
     Second cell: a `FullRefresh` queued behind a held turn, a ⌘R that publishes while it waits; the queued one must
     still land (it reads later). Lands in M3c.
4. **A backend's `Replaced` re-read asks the overlays whether they apply.** ❗ The `Replaced` arm is SYNCHRONOUS today
   (`notify_directory_changed` runs on the backend's thread) and `decorate` is async (`extra_entries` on the blocking
   pool). Awaiting the full pipeline there would make the patch land later than an `Added` / `Removed` the same backend
   sends right after it, reordering the cache. So `Replaced` checks the sync, zero-I/O `applies_to` of each overlay:
   none applies (every device backend today, which M0 pins as a stored count of zero) → `Prepared::without_overlays`,
   applied synchronously exactly as today; one applies → escalate to a spawned full re-read of that directory instead of
   decorating in place. Red test for the escalation branch with a test overlay that applies to an in-memory volume.
   `applies_to` needs a `&dyn Volume`, which the arm doesn't hold today: take it from the sync `ListingVolumes::get`,
   outside every lock; `None` means no overlay can apply (today's path). No new lock edge: the arm already reads the
   volume registry on the backend's thread through `ListingPath::on_volume` (verified 2026-10-11 on `7be64c4ff`). The
   escalation IS the `FullRefresh` arm for those listings (same turnstile, same generation rule), and the backend's
   entries are dropped, not half-applied.
5. **`refresh_index_sizes` enriches outside the cache lock.** Today `refresh_listing_index_sizes` holds the cache WRITE
   lock across `index().enrich` (two indexed SQLite queries over every row), once per index event per open pane, so an
   index storm stalls every pane read and IPC accessor behind it; it also goes through `entries_mut`, which drops the
   path map and the row map on every index event. Instead, in this order:
   - Under a READ lock, clone the non-symlink directories (the only rows enrich writes; see seam 3) and the listing's
     PUBLISHED read generation (behavior change 3: the generation of the last re-read that landed, never the last one
     handed out, or an in-flight re-read that later fails or is superseded would drop these sizes with nothing replacing
     them). No dirs → return.
   - Enrich the clones with no lock held.
   - Under the WRITE lock, if the published generation moved, drop the result: the re-read that replaced the rows
     enriched them itself, at least as recently. Otherwise write the seven fields back by path as `IndexSizes` through
     the writer `update_index_sizes` uses (`update_index_sizes_by_path`: keeps both maps, doesn't re-sort, doesn't bump
     `last_accessed_ms`).
   - By path is order-independent, so a re-sort in between is harmless. A row added in between was enriched by its own
     insert; a row removed or renamed in between is skipped. A `Modified` patch in between (patches don't bump the
     generation) can get sizes one index event older written over its own enrich; the next index event corrects it,
     which is the same window today's code has between two events.
   - The kept maps are part of the change (no more full map rebuild per index event); name it in the commit body. Red
     tests: a `FakeIndex` whose enrich blocks on a gate, and a `file_range` on the same listing returns while it's held;
     a re-read that publishes while the gate is held, and the stale sizes are dropped.
6. **A Cmdr-made create reaches the panes the way every other local mutation does.** `emit_synthetic_entry_diff` (mkdir,
   mkfile, paste-as-file) becomes one call to the local backend's
   `patch_listing_after_local_mutation(volume_id, parent, MutationEvent::Created(name))`, the function
   `LocalPosixVolume::notify_mutation` already runs for deletes and renames. What changes: an entry the listing already
   holds (the watcher stat'd it first) is upserted with the create's fresher stat instead of dropped, tags carried
   forward, exactly `notify_added`'s documented reason; and the composer's index apply and cloud-badge invalidation now
   run for a create too. `should_emit_synthetic_diff` stays as the gate.
   - **`name` is `new_path.file_name()`** (the path `place_new_entry` or the paste retry loop picked, which is what
     today's stat uses), never the name the user typed, and `parent` is the same `expanded_path` / `dir` passed today,
     so the row's path stays byte-identical and `has_entry` matches a watcher-added row.
   - **Why upsert is safe for every caller** (verified 2026-10-11 on `f497bdfd8` by reading `create.rs`,
     `paste_clipboard.rs`, `caching.rs`): no create overwrites. mkdir and mkfile refuse a taken name (`place_new_entry`
     → `AlreadyExists`), and paste retries `pasted (N)` on O_EXCL `AlreadyExists`, so the path is always new on disk. A
     row already at that path is therefore either the watcher's row for this same file (the upsert refreshes it) or a
     stale row for a file deleted outside Cmdr whose removal hasn't landed (the upsert fixes its kind and size; today
     keeps the stale row, even a file row for a new folder). A create racing a watcher add can't duplicate:
     `notify_added`'s `has_entry` check and its insert take separate locks, but `insert_entry_sorted` re-checks the path
     under the write lock. The one loss is a watcher stat taken and applied in the microseconds between the create's
     stat and its upsert, which the next event corrects; every `notify_mutation(Created)` has it.
   - **The index apply is NOT a no-op off `root`.** `apply_directory_change` → `apply_smb_change` skips only `root`, so
     a create on an indexed `LocalExternal` drive now also upserts that drive's index, alongside its own FSEvents feed.
     Deletes and renames there already take this path through `notify_mutation`, and the apply resolves to an upsert, so
     it should be idempotent; the M4a executor pins it with a cell (the same `Added` applied twice to a running test
     index leaves `dir_stats` unchanged) and, if it isn't, skips the index half for local creates instead of changing
     the index. Name it in the commit body.
   - Red test: a listing holding the new path with a stale size gets the on-disk size (today it keeps the stale one);
     M0's insert-if-absent pin is updated in the same commit with the reason. Lands in M4a, after M2b removed the
     `has_app` gate (before that, the composer no-ops in the test binary and the red test would pass vacuously).

## Milestones

Each milestone is one executor's work. "Lanes" names what the milestone must leave green; always add the cadence in
`AGENTS.md` § "Checker script" (`--fast` while iterating, plain `pnpm check` at the end) and `--include-slow` where
noted.

### Before M0: a create lands only in panes on its own volume

**Scope.** One standalone bug fix on today's code, its own commit (`fix(create): …`), landed before M0 so M0 pins the
fixed behavior. `emit_synthetic_entry_diff` finds its listings with the volume-less `find_listings_for_path`, so a
mkdir, mkfile, or paste-as-file into a local `/tmp` also inserts a row into an SFTP, WebDAV, or S3 pane that shows a
remote `/tmp`. Replace the lookup with `find_listings_for_path_on_volume(Some(volume_id), parent_path)`, using the id
the function already derives (`None` → `root`).

**Why the id is right.** The frontend passes the pane's own `volumeId` (`file-operation-commands.ts`, the mkdir and
mkfile dialogs, `paste_clipboard_as_file`), `create_*_core` writes through that same id, and the pane's listing carries
it, so an external disk's pane matches under its own id. Only volume-less callers fall back to `root`, and they also
wrote through `root`. A pane on a DIFFERENT local volume that shows the same directory stops getting the instant row and
gets it from its FSEvents watch (~200 ms), like any outside write. `ListingPath::on_volume` spells local paths verbatim,
so the match is otherwise unchanged. (Verified 2026-10-11 on `7be64c4ff` by reading `create.rs`, `paste_clipboard.rs`,
and the TS callers.)

**Test plan.** Red first in `write_operations/create/tests`: a `TestListing` on `root` and one on a second volume id,
both at the same real temp dir; `emit_synthetic_entry_diff(None, …)` adds the row to the `root` listing only (red today:
both gain it). This path has no `has_app` gate, so the cell runs in the test binary today.

**DONE.** The cell red then green; `create.rs` no longer names `find_listings_for_path`; lanes: `desktop-rust-tests`,
`desktop-rust-clippy`.

Executor note: the fix left the volume-less wrapper `find_listings_for_path` with no production caller, so it went (dead
code fails clippy); its tests call `find_listings_for_path_on_volume(None, …)`, which `listing_watch_coverage` already
used. Public API's `listings_for_local_path` replaces that `None` form.

### M0: Characterize what the seams will cut through

**Scope.** Tests only, app-side, against today's code. One allowed production change: extracting the side-effect half of
`TauriListingEventSink::emit_error` and the spawned task's result handling in `list_directory_start_streaming` into a
plain function both call, so a test can drive it without an `AppHandle`. Zero behavior delta.

**Intentions.** Turn every behavior M2 through M4 relocate into a pin, so "the adapter kept it" is a test result.

- The terminal-reaction matrix of one streaming listing, per outcome: success, cancel at each of the four points (before
  read, during read, after read, at cache insert), `PermissionDenied`, `NeedsPassword`, a damaged archive, a generic
  error with provider enrichment, and the restricted-empty root. For each: the event sequence, and which of MCP-ring
  record, analytics category, `restricted_paths` record, `restricted_paths` clear, and `note_root_failure` fired.
  Notably, `clear_path` runs on cancel and on the restricted-empty root today (both return `Ok(())`).
- Close ordering: a lifecycle observer's `listing_closed` sees the listing already gone from the cache, and a detached
  watch arm that lands after `list_directory_end` is dropped.
- Overlay rows survive all FIVE read paths (streaming, `notify_full_refresh`, the respell re-read, the watcher's
  `handle_directory_change`, the test-only sync start), and a `Replaced` change records zero overlay rows.
- The watcher's incremental path publishes removes, then adds, then modifies, carries tags forward on a modify, and
  falls back to a full re-read over 500 events or on an `Any` / `Other` kind.
- A golden ordering for a fixed corpus (NFC and NFD spellings of one name, numerics, `_` prefixes, case pairs) under two
  locales, so the collator's move from a global to an injected locale is provably identical.
- The documented "three read paths" in `listing/CLAUDE.md` becomes five in the docs, since the survey found two more.
- A listing opened in the TEST binary arms a watch on a watchable volume (today unconditional, through
  `start_watching_detached`), so M2a's observer move can't silently drop arming from the tests that depend on it
  (`local_posix_test::test_listing_watch_coverage_flips_with_watcher_lifecycle`, `caching_reaper_test`,
  `archive_watch_integration_test`).
- `flush_all_watchers` returns only after every listing's re-read landed in the cache.
- `emit_synthetic_entry_diff` on a path the listing already holds emits nothing (today's `insert_entry_sorted` duplicate
  guard). M4a's behavior change 6 flips this pin on purpose, in its own commit.
- Poison: none. Don't pin poisoned-lock behavior; M3a changes it deliberately (see there).

**Landmines.** `analytics::events::capture` may have no test observer; if it doesn't, pin the category string the
extracted function computes rather than adding a global hook. Use the existing `CollectorListingEventSink`; don't build
a second collector. Timing pins use `wait_until_async` and held flush windows, never sleeps (`desktop-rust-test-sleep`).

**Test plan.** Each new test fails when its pinned behavior is broken by hand (swap the remove/add order, drop one
`decorate` call, skip the `clear_path`), and that red is noted in the commit body.

**DONE.** New pins green; red-first verified per pin; lanes: `desktop-rust-tests`, `desktop-rust-clippy`,
`desktop-rust-test-sleep`, `desktop-rust-fixed-temp-dir`, `desktop-rust-no-hand-rolled-fixture`, `file-length`.

Executor notes:

- The extraction is `listing/listing_end.rs`: `finish_streaming_listing(events, effects, …)` plus a small
  `ListingEndEffects` trait (MCP ring, `restricted_paths` record/clear, `note_root_failure`) that `AppListingEndEffects`
  answers, so the matrix records every effect without a global hook; `error_analytics_category` is the pinned half of
  `emit_error`'s analytics (no test observer exists for `capture`). M2a moves all of it into `TauriListingEvents`.
  `streaming.rs` would have crossed `file-length`'s 800 with it inside. The restricted-empty root records into the MCP
  ring from inside `read_directory_with_progress`, so its cell reads the real ring by listing id.
- `flush_all_watchers` is `playwright-e2e`-only and never compiles in the test lane, so the pin is on what it awaits:
  `handle_directory_change` returns with its re-read landed (`watcher_incremental_test.rs`).
- The `Replaced` arm sits behind the `has_app` gate in the test binary, so its zero-overlay-rows cell drives the
  per-listing half (`publish_replacement(…, 0)`), as `caching_replacement_test.rs` does. M2b can lift it to the arm.
- "A detached arm after `list_directory_end` is dropped" was already pinned by
  `watcher_test::a_detached_arm_that_lost_the_race_leaves_no_watch_behind`; not duplicated.
- The test-binary arming pins are macOS-only, like every other FSEvents cell in `watcher_test.rs`.

### M1: Vocabulary moves down; the local reader moves out

**Scope.**

1. Re-point every `file_system::listing::{FileEntry, metadata::FileEntry, metadata::TagRef, ExtendedMetadata}` import in
   the app at `cmdr_fs::entry`, and drop the `metadata` alias and those re-exports. Inside `listing/`, also re-point
   every app path that is only a `cmdr_fs` re-export (`crate::ignore_poison`, and the `crate::file_system::volume::`
   items that are `cmdr_fs::volume` items: `Volume`, `VolumeError`, `DirectoryChange`, the friendly-error helpers, …) at
   `cmdr_fs` directly, so M2c's grep counts only real app reaches.
2. Move `reading.rs` and `mutation.rs` (and their tests) next to the local backend under `file_system/volume/backends/`
   (they are what `LocalPosixVolume`, the watcher, `tags.rs`, and `create.rs` read through).
   `patch_listing_after_local_mutation` keeps calling `notify_directory_changed` for now.
3. Into `cmdr-fs`: `benchmark.rs` (as `cmdr_fs::benchmark`), `archive_boundary_candidate` (into `archive_format.rs`;
   `cmdr-archive`'s `boundary.rs` calls it and drops its own copy), `mount_is_local_disk` (into `filesystem_kind.rs`),
   the pure scratch-name rules `could_be_hidden_from_listings`, `is_safe_save_name`, and
   `hidden_with_settings(name, show_staging, show_safe_save)` (into `staging`), `RoutedKind` and `ResolvedVolume` (into
   `volume`, with `VolumeManager` returning them), and `unique_test_id` (into `testing`).

**Intentions.** Every later milestone then imports vocabulary from the crate both sides share, and the engine's boundary
shrinks to the questions that genuinely need the app.

**Landmines.** `cmdr-archive`'s root re-exports may shrink: lower its ceiling (tightening never needs asking). Keep
`#[cfg(any(test, feature = "testing"))]` on anything gated, never `cfg(test)`, in `cmdr-fs`. The `file-length` and
`jscpd-rust` entries for `reading.rs` re-path. `cmdr-fs` is uncapped by design, so nothing to raise there.

**Test plan.** Moved tests move unchanged apart from paths, including `operations_test.rs`'s `RealFileSystemProvider`
cells, which test the local reader, not the engine. `cargo check -p cmdr-fs` and `-p cmdr-archive` build alone.

**Docs.** What `listing/DETAILS.md` says about `reading.rs` and `mutation.rs` moves to the local backend's docs in the
same commit (single-source), leaving a one-line pointer; M6 doesn't have to find it later.

**DONE.**
`rg -n 'crate::(benchmark|ignore_poison|file_system::index_provider)|archive_boundary_candidate|crate::file_system::staging::(could_be|is_safe_save|hidden_with)' apps/desktop/src-tauri/src/file_system/listing --glob '!*test*'`
finds only `cmdr_fs::archive_format::archive_boundary_candidate` uses, and `listing/` holds no `reading.rs` or
`mutation.rs` (`cmdr_archive::path_targets_archive_file` legitimately stays until M2a); lanes: `desktop-rust-tests`,
`desktop-rust-clippy`, `desktop-rust-clippy-linux`, `desktop-rust-rustdoc`, `index-crate-isolation`,
`desktop-rust-module-cycles` (slow), `file-length`, `desktop-rust-jscpd`, `desktop-bindings-fresh`.

Executor notes:

- The reader landed as `volume/backends/local_posix/{reading,mutation}.rs` (submodules of the backend, the way `scan.rs`
  / `streams.rs` sit); `backends` re-exports `get_single_entry` and `list_directory_core` for the watcher, `tags.rs`,
  `create.rs`, and tests. `operations_test.rs` split: reader cells → `local_posix/reading_test.rs`, the cancel and
  volume-start cells stay.
- `archive_boundary_candidate` is no longer re-exported by `cmdr-archive` (callers name `cmdr_fs::archive_format`);
  root-promise ceiling 35 → 34. `ResolvedVolume` / `RoutedKind` likewise left `volume::manager`'s re-exports; app
  callers use `crate::file_system::volume::…`, `listing/` uses `cmdr_fs::volume::…`.
- `crate::file_system::{FileEntry, ExtendedMetadata}` re-exports went too (not just `listing::`'s), so one path remains.
- Fixed a pre-existing M0 Linux-only unused import in `lifecycle_order_test.rs` that failed `desktop-rust-clippy-linux`.

### M2a: The seams exist; events and the runtime go through them

**Scope.** Create `file_system/listing/host/` with all eight traits in their final shape (§ "Seam design"), their public
no-op defaults, and a process-wide host slot that the app's new `listing_host::install` fills (the interim shape
`cmdr-index` used: accessors resolve a slot today and become handle fields in M3a with no call-site churn). Create the
app module `listing_host/` with `install` and the events adapter; `lib.rs` calls `listing_host::install` early in
`setup()` (before `volume_host::install`). In this milestone, route through the seams:

- Every event: the seven streaming events, `directory-diff`, `listing-respelled`, all via `ListingEvents`. The wire
  structs move to `listing_host/events.rs` unchanged. The M0-extracted reaction function moves into
  `TauriListingEvents::finished`, taking `cmdr_archive::path_targets_archive_file` with it. `WATCHER_MANAGER.app_handle`
  is no longer read from `listing/`, EXCEPT the `has_app` gate in `notify_directory_changed`, which M2b removes as its
  own behavior change (keep it reading the same flag until then).
- Every spawn: through the host runtime handle (update `commands/settings.rs`'s stale `tokio::spawn` comment).
- `listing/listing_host.rs` (`AppListings`) moves to `crate::listing_host::backends`, so there is one `listing_host`.

**Intentions.** After M2a, the only `tauri` left in `listing/` production code is the `has_app` read.

**Landmines.**

- **A read before `install` must be loud, never a silent permanent fallback.** `index()` uses `get_or_init`, so an early
  read wins the `OnceLock` forever and `install` can only log; for listings that would mean an engine whose events go
  nowhere, and every pane would sit on "loading". In non-test builds, a read before `install` is a `log_error!` plus the
  same report `index_host::install` makes; the event-less fallback exists only under `any(test, feature = "testing")`.
- **Adapters resolve the app per call, never at construction.** `listing_host::install` runs before
  `index_host::install` (line order in `lib.rs::setup`), so an adapter that captured `index()` at build would win
  `INDEX`'s `OnceLock` with an unconfigured index. Same for the volume manager.

**Test plan.** M0's matrix passes unchanged against `TauriListingEvents`' reaction function. A new cell builds a host
with `RecordingEvents` and checks the event sequence of a streaming listing end to end.

**DONE.** `rg -n 'tauri' apps/desktop/src-tauri/src/file_system/listing --glob '!*test*'` finds only the `has_app` read;
`desktop-bindings-fresh` zero diff; lanes: `desktop-rust-tests`, `desktop-rust-clippy`, `desktop-rust-clippy-linux`,
`desktop-rust-tests-linux`, `desktop-ipc-unused`, `desktop-rust-discarded-outcome`, `desktop-rust-lock-poison`.

Executor notes:

- The slot (`host::ListingSeams`, filled by `host::install`) holds only events and the runtime; the other six seams
  exist as traits and `No*` defaults under a module-level `#[expect(dead_code)]` that M2c drops once all are routed.
  `ListingOverlay` moved into `host/overlays.rs` (`listing_overlays.rs` re-exports it; its registry stays until M2b);
  `ListingObserver` + `OpenedListing` sit beside today's `ListingLifecycle` until M2b swaps them. `ListingId` exists
  (`listing_id.rs`) because the events seam takes it; engine internals still pass `&str` until later milestones.
- The read reports `finished(Complete | Cancelled)` itself at today's emit points; a refused read is reported by its
  task (`streaming::report_failure`) after the cancel token is gone, as the error was. `Complete.path` is the path as
  asked (what `clear_restricted` always got) and the adapter derives the listed path from `stored_path` for the
  restricted-root hint, so both stay byte-identical. Log-only deltas: `listing_done` now also logs for the
  restricted-empty root, and complete/cancel's `clear_restricted` runs before the token is dropped (unobservable).
- The app has no `testing` feature, so the test-only fallback is `cfg(test)`; M5 widens it. A pre-`install` read outside
  tests warns (`log-error-macro` bans a raw `log::error!`, and the engine can't name the app's macro) and answers from a
  separate detached value without claiming the slot; `install` then returns `ReadBeforeInstall`, which
  `listing_host::install` reports with `log_error!`.
- The M0 matrix runs against `TauriListingEvents::with(WireCollector, RecordingEffects)`. Its restricted-root cell now
  sees the MCP record through the effects seam (it read the real ring before), so its effect list is
  `[RecentError, ClearRestricted]`. Engine tests record facts with `host::testing::RecordingEvents`.
- `bindings.ts` changed in two doc comments only (the `settings.rs` comment this milestone rewrites, and
  `ListingRespelledEvent`'s, whose intra-doc link can't resolve from `listing_host/events.rs`); no name or shape moved.
- `desktop-rust-module-cycles` still warns `cmdr::file_system` [6] vs allowlist [2] (`cached_listing`, `caching`,
  `name_filter`, `operations`, `visible_rows`, `watcher`), present before M2a: `name_filter → operations`
  (`ListingLookupError`, `check_expected`) plus `operations ↔ watcher`. M2b's observer move cuts `operations → watcher`.

### M2b: Overlays and observers become host lists; the watcher is an observer

**Scope.**

- `listing_overlays.rs` and `listing_lifecycle.rs` fold into `host/overlays.rs` and `host/observers.rs`; their
  registries become the slot's lists. `listing_host::install` now also takes the git overlay, then the observers in
  order: the watcher, git arming, index sizes. `git::overlay::register`, `git::arming::register`, and
  `listing_index_sizes::start`'s registration call become constructors handed to `install`.
- The watcher becomes an observer (behavior change 2, own commit, M0's close-ordering pin updated with the reason).
- The `has_app` gate goes (behavior change 1, own commit).

**Intentions.** After M2b, `rg 'tauri' file_system/listing` finds nothing outside tests, and the lifecycle is one
ordered list.

**Landmines.** Removing the `has_app` gate makes `notify_directory_changed` live in the test binary for the first time:
write-operation and local-backend tests that hold a `TestListing` on a directory they then mutate will start seeing
patches, and `FullRefresh` arms now spawn background re-reads onto the fallback runtime that can outlive their test. Run
the whole `desktop-rust-tests` lane, and fix a test only by understanding what it asserted.

- **Test registration stays global until M4b.** There is one process-wide slot until M3a, so a per-test host is not
  possible yet: `overlay_refresh_test`, git `overlay_tests`, and `arming_tests` keep registering into the slot's lists
  (first registration per id wins, as today). Per-test engines arrive with the handle.
- **The fallback must include the watcher observer.** Today every listing on a watchable volume arms a watch with no
  registration at all; if the test-binary fallback wires observers only when `install` runs, the M0 arming pin goes red,
  or worse, a test that asserts "no watch" passes vacuously.
- **Only the observer VALUES move to `install`.** `listing_index_sizes::start` keeps starting its worker where it does
  today (it needs the `AppHandle` and its channel); `git::overlay::register` / `arming::register` become constructors.

**Test plan.** Behavior changes 1 and 2 each get their red cell before their commit: (1) a `TestListing` on a directory
that a `DirectoryChange::Added` names gains the row (red while the `has_app` gate stands); (2) a recording observer
registered after the watcher sees the watch already disarmed AND the listing already gone from the cache on close (M0's
close-ordering pin, updated with the reason).

**DONE.** `rg -n 'tauri' apps/desktop/src-tauri/src/file_system/listing --glob '!*test*'` finds nothing;
`rg -n 'listing_overlays|listing_lifecycle' apps/desktop/src-tauri/src` finds nothing; lanes as M2a plus
`desktop-svelte-e2e-playwright` (watch arming moved, and E2E is where a pane that stops updating shows).

Executor notes:

- The slot's overlay and observer lists sit behind an `RwLock` only so tests can still add to the one shared slot
  (`host::testing::register_overlay` / `register_observer`; `git::overlay::register` and `git::arming::register` survive
  as `#[cfg(test)]` helpers over them). The test fallback is app-side, `listing_host::test_seams()` (detached plus the
  watch observer), reached from `host/testing.rs`; M3a's `listings()` fallback replaces it.
- `IndexSizeListings` still reads volume id and spelling back from the cache on open, using `opened.id`: the read is
  also its liveness check (a close racing the open leaves nothing in `OPEN`), so switching to `OpenedListing`'s facts is
  a behavior change. M3a must keep a liveness check when that reach goes (it'd hit the wrong engine in a test).
- `decorate`'s blocking gather now spawns on `host::runtime()` (it was ambient `tokio::task::spawn_blocking`).
- Behavior change 2's red cell needed a probe, since an observer after the watcher sees the same thing either way:
  `stop_watching` calls `seam_test_support::note_disarm` (`cfg(test)`), recording whether the cache still held the
  listing at the disarm. `Heard::Closed` also gained `watching`.
- `desktop-rust-module-cycles`: `cmdr::file_system` is gone (entry dropped), but the remaining
  `name_filter → operations` tangle now reports one level down as `cmdr::file_system::listing` [4] (`cached_listing`,
  `name_filter`, `operations`, `visible_rows`), un-allowlisted (warn). M2c/M3a should cut `name_filter → operations`
  (`ListingLookupError`, `check_expected`).
- The DONE `tauri` grep matches only prose in `listing/DETAILS.md`; with `--type rust` it finds nothing.

### M2c: The rest of the app reaches go through the seams

**Scope.** `ListingVolumes` (every `get_volume_manager()` in `listing/`, mount adoption plus `why_unregistered` as one
`unregistered` call), `ListingIndex` (every `index_host::index()`), the app composer `listing_host::directory_changed`
(index apply, badge invalidation, then the panes), `ForegroundLeases`, `ReadingLocale`, `MountTable`, and scratch
visibility as engine state set by `file_system::staging`'s setters (which become forwarders, so settings live-apply is
untouched).

**Intentions.** After M2c, production code in `listing/` names nothing in the app crate outside `listing/` itself and
`cmdr_fs`. That is the precondition for the move, reached while still in the app, where a mistake is a normal compile
error.

**Landmines.** `notify_directory_changed` runs the index apply BEFORE any pane work (the SMB watcher fires with no pane
open, and the enrich must see fresh sizes); the composer keeps that order and the engine must not call the index apply
itself. `ListingPath::on_volume` reads the registry before taking the cache lock; keep it outside the lock with the seam
too. `stalled_on`'s symlink probe must stay on a blocking thread under the 500 ms bound. The collator cache keys by
locale string; pulling the locale per sort must not rebuild a collator per sort.

**Test plan.** `stall_test` and `streaming_test` run against `FakeVolumes`-shaped answers where they used the global
registry only to resolve. M0's collation golden passes with `ReadingLocale` fixed per test.

**DONE.**
`rg -n 'crate::' apps/desktop/src-tauri/src/file_system/listing --glob '!*test*' | rg -v 'crate::file_system::listing'`
finds nothing; lanes as M2a plus `desktop-rust-module-cycles` (slow) and `desktop-rust-cfg-gate`.

Executor notes:

- The slot (`host::ListingSeams`) now holds all eight seams; `listing_host::with_app_answers` wires `AppVolumes`,
  `AppListingIndex`, `AppLeases`, `AppLocale`, and `AppMountTable` (one file each in `listing_host/`), and
  `test_seams()` is that plus the watch observer, with detached events and runtime. So tests still list through the real
  registry, index, intl lock, and mount table: `stall_test` / `streaming_test` keep the global registry and
  `collation_test` the intl lock until M4b's per-test engines (one shared slot can't take a per-test fake).
- Scratch visibility is `host::ScratchVisibility` in the slot (two atomics), read through `host::scratch_visibility()`
  inside `visible_rows.rs` as before. Threading it as an argument read once per operation is left to M3a, which rewrites
  those functions anyway. `staging`'s setters forward (read, flip one flag, write), its getters and
  `is_hidden_from_listings` are `#[cfg(test)]`.
- The composer is `listing_host::directory_changed`; `AppListings`, git wiring, and the local mutation patch call it,
  and `notify_directory_changed` is panes only. Tests calling the engine function directly no longer run the index apply
  (they asserted panes only).
- The `#[cfg(test)]` sync start (`list_directory_start_with_volume`) no longer calls `note_root_failure` on a refused
  read; that is the events adapter's policy, and no test relied on it.
- `brief_columns.rs` (+ test) moved to `font_metrics/` now, unchanged apart from paths, so the DONE grep holds; M4a
  still switches it to `with_rows`. `fuzzy_jump.rs`'s inline tests moved to `fuzzy_jump_test.rs` for the same grep.
- The `cmdr::file_system::listing` tangle is cut: `name_filter.rs` is now only the pure `NameFilter`, and the pane
  setting moved to `quick_filter.rs`. Routing the stall probe through `host::mounts()` closed `host ↔ stalled_on`, so
  `StalledOn` now lives in `host/events.rs` (re-exported from `stalled_on`). `module-cycles` shows nothing in
  `listing/`; the tangles it still warns about (`menu`, `licensing`, `write_operations`, `volumes`, `cmdr-index`,
  `cmdr-s3`) came in with main.
- `ListingEnd`'s `Cancelled.volume_id` / `volume_id()` / `path()` and `OpenedListing`'s `volume_id` / `stored_path`
  carry targeted `dead_code` expects (nothing in the app reads them yet); M3a and M5 drop them.

### M3a: The engine owns its state (`Listings`)

**Scope.** Introduce `Listings` (an `Arc<Inner>`), `ListingsBuilder`, and `Listings::detached()`. Every static in the
survey list becomes a field; the host slot becomes the handle's fields; every free function in `listing/` becomes a
method or takes `&Inner`. `listing_host::install` builds the engine and stores it in a `OnceLock`;
`listing_host::listings()` hands it out, falling back to the app-wired, event-less build only under
`any(test, feature = "testing")` (M2a landmine: in the app a pre-`install` read is loud). For callers outside `listing/`
(and the tests), keep the old free functions as thin `pub(crate)` shims over `listing_host::listings()`, each marked
`// shim: removed in M4b`.

**Intentions.** Mechanical de-static with zero behavior delta outside a poisoned lock; the test suite stays as it is and
passes through the shims.

**Landmines.** Code inside `listing/` must use `self` / its `Inner`, never the app accessor: an engine method that
reaches for the global would quietly work in the app and break for every other consumer.

- **Lock-poison is the one deliberate delta here, and only on a poisoned lock.** The 22 allowlisted sites are
  `.read().ok()?` / `if let Ok(..) = .write()` shapes that today SKIP their work on poison (the streaming insert
  silently drops the listing, then emits `listing-complete` for it). Convert them all to `*_ignore_poison`, never
  `.expect`: a panic would turn one poisoned cache into a crash on every later IPC. Name the change in the commit body;
  healthy runs see no difference. Let the check shrink the allowlist to zero for these files.
- **`start` registers the cancel token synchronously before it returns**, as `list_directory_start_streaming` does
  today, so a `cancel` that follows immediately still finds the read.
- **Background tasks must not keep the engine alive forever.** The orphan reaper's loop holds a `Weak<Inner>` and ends
  when the upgrade fails; per-event tasks (the 50 ms diff flush, a refresh) may hold a strong `Arc` because they end.
  Otherwise a dropped test engine leaks a reaper per test and the "two engines side by side" test can't show the first
  one went away.
- The orphan reaper's spawn moves to `Listings::start_orphan_reaper`, called from `lib.rs` where
  `start_orphan_listing_reaper` is today. `reconciled_cache`'s `RwLockReadGuard<'static>` becomes a guard borrowed from
  `&Inner`.

**Test plan.** Whole suite green through the shims, unchanged. One new test builds two engines side by side and shows a
listing in one is invisible to the other.

**DONE.**
`rg -n '^\s*(pub(\([a-z]+\))?\s+)?static\s|LazyLock|thread_local!' apps/desktop/src-tauri/src/file_system/listing --glob '!*test*'`
finds nothing (a `const` is fine; `'static` bounds don't match); `lock-poison-allowlist.json` has no `listing/` entry;
lanes: `desktop-rust-tests`, `desktop-rust-clippy`, `desktop-rust-lock-poison`, `desktop-rust-tests-linux`.

### M3b: Rules become types

**Scope.** Three refactors, one commit each, all behavior-preserving except change 4.

1. **`Prepared` entries.** One function in `read/pipeline.rs` runs enrich (skipped for a routed volume), then the
   overlays, and returns `Prepared { entries, overlay_rows }`, constructible nowhere else. `CachedListing::new` and the
   replace path accept only `Prepared`. The five `decorate` sites collapse into it, which retires the "in all three read
   paths" rule in favor of a compile error. `Replaced` uses the same function (behavior change 4). The watcher's
   `std::fs` fallback (no volume) gets `OverlayRows::Unchanged` through the same function.
2. **The publish token.** `enqueue_diff` becomes reachable only through a value borrowed from the cache write guard, so
   "allocate revisions and enqueue under the cache write lock, cache then queue" can't be violated; dropping a listing's
   queue on `end` uses the same token.
3. **`ReadSession`.** The streaming read's foreground hold, cancel token, and stall watch live in one struct created at
   the read's first statement, so the hold lives exactly as long as the read and can't be bound to `_`.

Also hoist the remaining per-entry seam calls (the incremental path's per-entry `enrich`) to one call per batch.

**Intentions.** Each `❗`/`❌` line in `listing/CLAUDE.md` that this makes unrepresentable shrinks to a one-line pointer
at the type.

**Landmines.** `OverlayRows::Unchanged` must still mean "the stored count describes these entries"; don't let `Prepared`
force a recount where today's code keeps it. The token must not hold the queue mutex across the cache lock in the
reverse order anywhere (`end` takes cache then queue today).

**Test plan.** Red first for change 4's escalation cell (a test overlay whose `applies_to` matches an in-memory volume;
the `Replaced` arm must re-read and the overlay rows must be present). M0's overlay and close-ordering pins stay green.
Deleting the overlay step from the pipeline fails compilation or the M0 overlay pins, never neither.

**DONE.** `rg -n 'decorate\(' apps/desktop/src-tauri/src --glob '!*test*'` finds the definition and exactly one call, in
the pipeline function; lanes as M3a plus `desktop-rust-module-cycles` (slow).

### M3c: One re-read path, ordered by read generation

**Scope.** The engine gains the watcher's half of a re-read, and full re-reads get their generation. One commit each:

1. `Listings::refresh`, `begin_read` + `refresh_from_entries`, and `observe_children` (§ "Public API"), lifted out of
   `file_system/watcher.rs`'s `handle_directory_change` and `handle_directory_change_incremental`. The watcher keeps the
   OS half (debounce, rebasing, the stat, the over-500 / unknown-kind fallback decision, the `std::fs` read on
   `RefreshOutcome::NoVolume`, `directory-deleted`, `stop_watching`) and calls these. `flush_all_watchers` awaits
   `refresh` per watch, then flushes pending diffs, as today. Zero behavior delta.
2. Behavior change 3 (the per-listing read generation), red tests first.
3. Behavior change 5 (`refresh_index_sizes` enriches outside the cache lock), red tests first. It needs change 3's
   generation, which is why it lands here and not in M3b.

**Intentions.** One place owns "a full re-read lands", so the generation check has exactly one replace step to guard.

**Landmines.** `handle_directory_change` stays as the app-side wrapper over `refresh` (it maps `DirectoryGone` to
`directory-deleted` + `stop_watching` and `NoVolume` to the `std::fs` hand-over), because the ⌘R command
(`refresh_listing_within`), the three watcher spawns, `flush_all_watchers`, and three tests (`watcher_test.rs`,
`overlay_refresh_test.rs`) call it; none of them handles a `RefreshOutcome` itself. `refresh` returns only once its read
landed or was superseded; `flush_all_watchers` (E2E) and the ⌘R command's 2 s `TimedOut` wait both depend on that.
`PermissionDenied` stays silent and other read errors stay a warn log, as today. The watcher's `coverage_for_listings`
stays app-side.

**Test plan.** M0's watcher-incremental and `flush_all_watchers` pins pass through the new methods unchanged before
commit 2. Change 3's two cells and change 5's two cells (§ "Behavior changes, called out"), each red first.

**DONE.** `file_system/watcher.rs` names no `get_listing_entries`, `update_listing_entries`, `insert_entry_sorted`,
`remove_entries_by_paths`, `update_entry_sorted`, `carry_forward_tags`, `has_entry`, `enrich(`, or `decorate`
(`rg -n 'get_listing_entries|update_listing_entries|insert_entry_sorted|remove_entries_by_paths|update_entry_sorted|carry_forward_tags|has_entry|enrich\(|decorate' apps/desktop/src-tauri/src/file_system/watcher.rs`
finds nothing); change 3's and change 5's four cells each seen red, then green; lanes: `desktop-rust-tests`,
`desktop-rust-clippy`, `desktop-rust-tests-linux`, `desktop-rust-test-sleep`, `desktop-svelte-e2e-playwright` (the
watcher's re-read and `flush_file_watcher` are what E2E leans on).

### M4a: The app programs against the public API

**Scope.** Write the crate-to-be's API as a `pub` re-export list in `listing/mod.rs` (§ "Public API"), make every other
`listing/` submodule private (`mod`, not `pub mod`; `host` stays public, and `caching_test_support` stays
`#[cfg(test)] pub(crate)` until M4b), and move every production caller onto the re-exports:
`commands/file_system/listing.rs` (pure pass-throughs; `list_directory_start_streaming` builds its
`StreamingListingStartResult` app-side), `ipc.rs` registrations, `listing_index_sizes` (`update_index_sizes`,
`folder_entries_named`, `contains`, `location`), `multi_rename/session.rs` (`entries_named`), `mcp/executor/tags.rs`
(`listings_for_path_on_volume` + `apply_tags`), git wiring and arming, the write-operation oracle callers,
`commands/search.rs` (`collator()`), `agent/tools/read/pane_listing.rs`, and the watcher's remaining reaches
(`ListingId` wrapping, membership). The watcher's re-read already moved in M3c.

Behavior change 6, own commit, red test first: `create.rs::emit_synthetic_entry_diff` becomes the local backend's
`patch_listing_after_local_mutation(volume_id, parent, MutationEvent::Created(name))` (`name` is `new_path`'s file name,
`parent` the expanded parent the caller already passes), so `create.rs` and `paste_clipboard.rs` stop naming the engine.

`brief_columns.rs` moves to `font_metrics/brief_columns.rs` and reads rows through `with_rows`. Production shims are
deleted; test-only shims stay `#[cfg(test)]`. `file_system::staging`'s two setters and getters stop being production
API: `lib.rs` and the two settings commands call `listings().set_scratch_visibility` directly (reading
`scratch_visibility()` to flip one flag), the functions shrink to `#[cfg(test)]` shims for `ShowTempsGuard`, and M4b
deletes both with the shared state they guarded. No forwarder outlives the refactor.

**Intentions.** Nothing outside `listing/` names a `listing::` path other than the re-export list, and the compiler
enforces it.

**Landmines.** `listing_index_sizes` writes sizes without dropping the row map (`update_index_sizes_by_path` keeps both
maps); the API method must keep that, or every index event rebuilds a 300,000-row map. Commands keep their
`blocking_with_timeout` wrappers app-side; the engine never sees a deadline. If this milestone runs long, split at the
natural seam: commit the create re-route and `brief_columns` first, then the caller migration and the module privacy.

**Test plan.** Change 6's cell (§ "Behavior changes, called out"). Everything else is a pure re-route: the suite passes
unchanged apart from import paths.

**DONE.** It compiles with every `listing/` submodule but `host` private (so no caller outside can name
`listing::<module>::`); `rg -n 'emit_synthetic_entry_diff' apps/desktop/src-tauri/src` finds nothing; lanes:
`desktop-rust-tests`, `desktop-rust-clippy`, `desktop-rust-tests-linux`, `desktop-bindings-fresh`, `desktop-ipc-unused`,
`file-length`, `desktop-rust-jscpd`, plus `desktop-svelte-e2e-playwright` (the pane, mkdir's cursor-land, and Brief
columns all moved).

### M4b: Tests own their engine

**Scope.** Write the `testing` fixtures (`TestEngine` builder with `FakeVolumes`, `RecordingEvents`, `RecordingLeases`,
a fixed `ReadingLocale`; `TestListing` over a given `&Listings`; `WatchCoverageVolume`) and convert every test in
`listing/` to build its own engine. Move the app-coupled cells listed in the survey to `listing_host/tests/` as
integration tests against the installed adapters. Convert the ~20 outside test files that use `caching_test_support` to
the fixtures over `listing_host::listings()`. Delete every remaining shim and `caching_test_support.rs`.

**Intentions.** The crate's suite runs fully parallel with no shared state, which deletes the `TestListingGuard`
machinery and the "cache-wide assertions need unique paths" rule from the docs.

**Landmines.** The app-side tests still share the one installed engine, so they keep unique ids (`unique_test_id` from
`cmdr_fs::testing`). `nextest-filter-coverage`: `.config/nextest.toml` lines 408–409 name
`file_system::listing::caching_reaper_test::` and `archive_watch_integration_test::`; update the atoms to wherever those
tests land, in this same commit. Keep every moved test's name so `git log --follow` and the nextest atoms stay legible.

- **A converted test must still assert what it asserted.** Before converting a file, list per test which real piece (the
  volume manager's routing, the watcher, the intl lock, the real index) it leans on and which fake replaces it. When the
  real piece WAS the point, the test stays app-side in `listing_host/tests/` instead of passing vacuously against a
  fake. Known splits: `caching_reaper_test`'s "listing and watcher go together" cells become a crate cell (the reaper
  ends through `end`, and a `RecordingObserver` hears the close after the cache removal) plus an app cell (the FSEvents
  watch is gone); `collation_test` drops the intl lock for `FixedLocale` in the crate, plus one app cell proving
  `AppLocale` follows `intl::set_language_preference`; `stall_test` cells that read a real mount move behind
  `FakeMountTable`, plus one app cell per OS for the `getfsstat` / `/proc/mounts` adapter.
- **Each adapter gets at least one app-side cell** against the real subsystem (volumes, events, index, leases, locale,
  mount table, and the observer order), so the seams are proven on both ends, not only through fakes.

**Test plan.** Test count before and after matches (excluding deleted guard self-tests, and counting each split as its
parts); `TestListingGuard`'s three isolation failure modes have no analogue left to test in the crate.

**DONE.** No `listing_host::listings()` call inside `listing/` tests; lanes: `desktop-rust-tests`,
`desktop-rust-tests-linux`, `nextest-filter-coverage`, `desktop-rust-test-sleep`, `desktop-rust-fixed-temp-dir`,
`desktop-rust-no-hand-rolled-fixture`, `--include-slow` once at the end.

### M5: The move

**Scope.** `git mv apps/desktop/src-tauri/src/file_system/listing crates/cmdr-listing/src` (then the `src/*/` grouping
from § "Target crate layout"), `Cargo.toml` with the dependency comments the other crates carry, workspace member,
`scripts/check/checks/inputs.go` entry, `index-crate-isolation` guard list plus a measured `surfaceGuardedCrates` entry
with its item-by-item rationale in the crate `DETAILS.md` § "The public surface is capped", `#![deny(missing_docs)]`
with docs written for every public item, the `testing` and `playwright-e2e` features (the app's `playwright-e2e`
forwards), the crate-local `clippy.toml`, the app's `Cargo.toml` (`cmdr-listing` in; `icu_collator` and
`icu_locale_core` out, since nothing else in the app uses them), and every `crate::file_system::listing` path in the app
rewritten to `cmdr_listing`. Fix the docs' relative links so the docs lanes pass; the content rewrite is M6.

**Intentions.** By M4b the code is already crate-shaped, so this is path rewriting, manifests, and registrations.

**Landmines.** Verify that clippy reads a member-level `clippy.toml` (it searches the package's manifest directory
first) and that the copy carries the root's config; if not, fall back to a `#[deny(clippy::disallowed_methods)]` that
the root config can't express and record the gap in the crate `DETAILS.md`. `specta` stays pinned to the exact app
version. `cargo check -p cmdr-listing` must build alone, and `cargo tree -p cmdr-listing -e normal` must not show
`rt-multi-thread` coming from this crate's own manifest. `module-cycles`: compare the new crate's homes with what
`cmdr::file_system` showed before the move; a new home means an edge to cut, not a number to add. `cargo-deny` sees a
new member; `cargo-machete` / `cargo-udeps` will flag the ICU crates if they stay in the app manifest.

- `file-length-allowlist.json`'s `caching.rs` entry carries a TODO split (full-refresh machinery, the oracle, the
  lookups and patch helpers); the target layout's `read/refresh.rs`, `engine/oracle.rs`, `engine/intake.rs`, and
  `cache/mutate.rs` ARE that split, so the entry goes rather than re-pathing. Same test for every other listing entry in
  `file-length` and `jscpd-rust`: re-path only what the new layout didn't already fix.

**Test plan.** Everything that passed in M4b passes, now as two test binaries; `desktop-bindings-fresh` zero diff.

**DONE.** Lanes: full `pnpm check`, then `--include-slow` (`index-crate-isolation`, `workspace-member-coverage`,
`desktop-rust-module-cycles`, `desktop-rust-cargo-deny`, `desktop-rust-cargo-machete`, `desktop-rust-cargo-udeps`,
`desktop-rust-rustdoc`, the docs lanes, `nextest-filter-coverage`), and `desktop-svelte-e2e-linux` once.

### M6: The docs move with the code

**Scope.** Crate `CLAUDE.md` (must-knows only, 300–400 words, the module map, the seam rule, the `cfg(test)` rule, "the
host is a value") and `DETAILS.md` (today's listing `DETAILS.md`, rewritten for crate paths and the handle, plus § "The
public surface is capped" and a short "driving the engine without Cmdr" section showing `Listings::builder()` with the
defaults). `src/host/CLAUDE.md` + `DETAILS.md` (the seam list, what each replaces, where the app answers it). App
`listing_host/CLAUDE.md` + `DETAILS.md` (the adapters, the composer's order, the observer order). Update
`docs/architecture.md` (one pointer per side), `AGENTS.md`'s crate list, `file_system/CLAUDE.md` and `DETAILS.md`
(watcher rules now reference the observer and `observe_children`), `volume/DETAILS.md`,
`crates/cmdr-fs/src/volume/ host/DETAILS.md` (who answers `ListingHost`), `docs/testing.md`,
`docs/guides/benchmarking-file-loading.md`, the frontend `DETAILS.md` files that cite `listing/DETAILS.md` sections, and
`mcp/DETAILS.md` (where the ring is fed).

Shape, per `docs/doc-system.md`:

- Every new `CLAUDE.md` ends with the canonical closing-pointer line
  (``<payload>: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.``),
  and `src/host/` follows `crates/cmdr-index/src/indexing/host/CLAUDE.md`'s shape (must-knows, then a one-line-per-file
  module map).
- Reachability: the crate `CLAUDE.md` and the app's `listing_host/CLAUDE.md` are linked from `docs/architecture.md`;
  `src/host/CLAUDE.md` from the crate `CLAUDE.md`'s module map. Each `DETAILS.md` from its sibling. That's what keeps
  `docs-reachable` green without touching the resident bundle.
- Single-source homes for content that today lives in module doc comments: `listing_overlays.rs`'s "❌ never move this
  into a `Volume` impl" rationale and `listing_lifecycle.rs`'s "arming belongs to the backend" rationale go to
  `src/host/DETAILS.md` (the trait's rustdoc keeps one line and a pointer); Brief columns' "font metrics in a Rust
  cache" decision goes to `font_metrics/`'s `DETAILS.md`; the composer order and the observer order live ONLY in
  `listing_host/DETAILS.md`, and the crate docs point there rather than restating them.
- `C.md` lines that M3b's types retired (the three-read-paths rule, "never bind the lease to `_`", "cache then queue")
  become one-line pointers at `Prepared`, `ReadSession`, and the publish token, or go if the type says it all.

**Intentions.** Every invariant in the footer has exactly one canonical home; rules that became types become a pointer
at the type.

**Landmines.** `resident-doc-budget`: the `AGENTS.md` crate line grows by one name, nothing more. Describe the current
shape only; no "used to live in the app". `docs-section-refs` catches renamed `§` anchors cited from elsewhere.

**DONE.** Lanes: `claude-md-length`, `claude-md-details-sibling`, `docs-reachable`, `docs-dead-links`,
`docs-section-refs`, `docs-link-text`, `resident-doc-budget`, `claude-md-reminder`. Then the playbook's close-out: an
adversarial conformance review against the footer, and a docs audit reading every commit body.

## Invariants this refactor must honor

Carried from `file_system/listing/CLAUDE.md` (where each now lives is M6's job):

1. Neither a row number nor a path indexes `entries`; `CachedListing::rows` is the ONLY filter point, on read; by-path
   callers use `index_of_path` / `indices_of_paths`; a mutating caller resolves before `entries_mut`.
2. Mutation owners allocate revisions and enqueue stamped batches under the cache write lock, cache then queue; the
   emitter coalesces transport only; indices are pane rows; scratch drift reconciles before mutation or guarded index
   consumption, then the expected revision is checked.
3. Quick-filter changes share that revision and drop superseded batches under the cache write lock.
4. Refreshes of one directory are serialized, and (after change 3) no full re-read lands after a newer one did.
5. Overlay rows are folded in between enrich and sort on every read path, and a decorated listing never answers the
   fresh-listing oracle.
6. Observers hear a close after the cache removal.
7. The orphan reaper keys on `last_accessed_ms`, bumped by reads, patches, and the heartbeat, never by
   `refresh_index_sizes`.
8. A streaming read holds a foreground lease for its whole body.
9. A listing never aborts a read; cancel detaches; a stall reports and keeps waiting.
10. Full refreshes from OS threads spawn on the injected runtime, never `tokio::spawn`.
11. One comparator (`entry_comparator` over `SortableEntry`); a sort change bumps the frontend's `cacheGeneration`.
12. Names collate by Unicode collation with a raw-bytes tiebreak; a `NameKey` is never persisted.
13. A listing's path is a `ListingPath` built only through the volume's spelling.
14. Only a pane open or a Finder drop/paste resolves a foreign spelling.
15. New listing state hangs off the handle, never a static.
16. Finder tags are deferred; every modify path carries tags forward before storing.

Added by this refactor:

17. `cmdr-listing` has no `tauri`, `tauri-specta`, or `cmdr` in its tree (`index-crate-isolation`), and its public
    surface is capped.
18. Every app concern arrives through a `host` seam; every seam degrades to a no-op and has a `testing` fake.
19. A seam is called per read or per mutation, never per entry.
20. The engine reports facts; wire payloads, `tauri_specta` derives, and user-facing words live app-side, and the
    generated bindings don't change.
21. `index → badges → panes` is the order every directory change is applied in, owned by the app's composer.
22. Observers run in registration order: the FSEvents watcher, the git portal, index sizes.
23. Gate behavior on `any(test, feature = "testing")`, never `cfg(test)` alone.
24. No seam, observer, or overlay runs under an engine lock (after change 5, with no exception); `with_rows` is the one
    public closure run under the cache lock, and it never re-enters `Listings`. Everything else crosses as data.
25. Background loops hold the engine weakly; a dropped `Listings` stops its reaper, and `listing_host()` holds it weakly
    too, so no `VolumeHost` keeps it alive.
26. A listing id is a `ListingId` in every Rust signature; only the IPC boundary sees a `String`.

## Review round 1 (correctness and behavior preservation)

- Loud rules: added "no seam/observer/overlay under an engine lock; API closures never re-enter" (the index-size
  observer reads the cache from `listing_opened`).
- Decision 1: corrected the claim that `TestListingGuard` serializes tests; `ShowTempsGuard` is the real serializer.
- Survey: scratch visibility is read inside `CachedListing` methods (even outside the lock), so it becomes a passed
  `Copy` value; `entry_comparator` / `sort_entries` aren't pure (they call `active_collator()`), so they take
  `&NameCollator`.
- Survey: added missed callers (`create.rs` synthetic diff, `commands/e2e.rs` flush, `commands/settings.rs` comment, the
  `listing_host` name collision, three test-support users).
- Seam 5: observers run outside locks and receive volume id + stored path, so `IndexSizeListings` stops reaching back
  into a possibly wrong engine.
- Behavior change 3: replaced "take the turnstile" (early return breaks E2E flush, a stalled read blocks ⌘R forever, raw
  vs `ListingPath` keys differ on MTP) with a per-listing read generation checked at replace time.
- Behavior change 4: `Replaced` is synchronous and `decorate` async; it now gates on sync `applies_to` and escalates to
  a re-read instead of reordering patches.
- M0: added pins for test-binary watch arming, `flush_all_watchers` landing, and the synthetic-diff duplicate guard.
- M2a: test registration stays global until M4b; the fallback includes the watcher observer; a pre-`install` read is
  loud and test-only; adapters resolve `index()` per call; only observer values move to `install`; `AppListings` moves
  into `crate::listing_host::backends`.
- M3a: lock-poison conversion is a named poisoned-state delta (`*_ignore_poison`, never `.expect`); `start` registers
  its cancel token synchronously; the reaper holds `Weak<Inner>`.
- M4a: `refresh` returns only after its read landed; the synthetic-diff re-route keeps insert-if-absent or asks first.
- Invariants: 4 reworded for the generation check; added 24 and 25.

## Review round 2 (verify round 1's designs, architecture and API, docs and tests)

- Loud rules: round 1's "today's code already honors this" was wrong: `refresh_listing_index_sizes` enriches under the
  cache write lock; named it behavior change 5 (M3b) instead of letting the rule silently fail.
- Behavior change 3: pinned WHEN a read takes its generation (after the turn, before `list_directory`; a `ReadTicket`
  before the app's `std::fs` fallback; generation 0 at insert; patches don't bump), with a second red cell for the
  queued-refresh case an early generation would break.
- Behavior change 4: the `Replaced` arm gets its `&dyn Volume` from the sync `ListingVolumes::get` outside locks, and
  the escalation IS the `FullRefresh` arm.
- Behavior change 5 (new): `refresh_index_sizes` enriches clones outside the lock and writes back by path, red-tested
  with a gated index fake.
- Decision 7: `ListingId` newtype (listing id and volume id were confusable `&str`s side by side), wrapped at the IPC
  boundary so bindings don't move.
- Decision 8: `Listings::builder(volumes, events)` makes the two seams a consumer can't do without compile-time
  required.
- Decision 9: the engine provides a weak `cmdr_fs::ListingHost` so a non-Tauri consumer wires backends in one line
  without an `Arc` cycle; the app adapter keeps only the composer.
- Public API: `update_index_sizes` takes `IndexSizes` data instead of a `&mut FileEntry` closure; `folder_entries_named`
  / `entries_named` return owned rows; lookups return `Vec<ListingId>`; the volume-less lookup is renamed
  `listings_for_local_path`; added `scratch_visibility()`.
- Seam 5: `listing_opened(&OpenedListing)` takes one struct, not positional `&str`s.
- Target layout: one `testing` fake per seam, as decision 2 promises (`FakeIndex`, `StaticOverlay`, `RecordingObserver`,
  `FixedLocale`, `FakeMountTable` were missing).
- M1: the local reader's docs and `operations_test`'s `RealFileSystemProvider` cells move with it.
- M2a / M3b: explicit red cells for behavior changes 1, 2, 4, and 5 in the milestone that lands each.
- M4a: renumbered the synthetic-diff upsert option to change 6, and flagged for David that its volume-less lookup can
  patch a remote pane showing the same path; `staging`'s scratch setters stop being production API (no forwarder
  survives).
- M4b: a converted test must keep asserting what it asserted, with named splits (reaper, collation, stall) and one
  app-side cell per adapter.
- M6: closing-pointer line, reachability path for each new doc, single-source homes for rationale now in module doc
  comments, and retired rules becoming pointers at their types.
- Invariants: 24 tightened (only `with_rows` runs under the lock), 25 extended to `listing_host()`, added 26.

## Review round 3 (verify round 2's designs, settle the create question, executability)

- `create.rs` (round 2's open question, decided): the volume-less match is a bug, fixed as a standalone commit before M0
  with the volume-scoped lookup. Round 2's worry doesn't hold: the frontend passes the pane's own `volumeId`, the create
  writes through it, and the pane's listing carries it. The re-route through the local mutation patch (upsert, index
  apply, badge invalidation) became behavior change 6 in M4a, after M2b removes the `has_app` gate that would make its
  red test vacuous.
- Behavior change 5: verified enrich writes only seven fields on non-symlink dirs, so it clones dirs only (now a stated
  `ListingIndex` contract). By-path write-back was unsafe against a re-read that publishes mid-enrich (staler sizes over
  fresher ones); it now drops the result when the read generation moved, and therefore moved to M3c after change 3.
  Named that today's version also drops both maps per index event.
- `refresh` can't hand over the `std::fs` fallback unless it says the volume is gone: `RefreshOutcome` gained `NoVolume`
  (and the rest of its variants are spelled out). Round 2's M4a landmine said the unregistered exit is silent; it falls
  back today, so that line went. `begin_read` returns `Option`.
- Verified sound: `Replaced` taking `&dyn Volume` from `ListingVolumes::get` (no new lock edge), the weak `ListingHost`,
  `ListingId`, `builder(volumes, events)` (no-op defaults are now public types so the test fallback can name
  `NoEvents`), `update_index_sizes` taking data. `keep_alive` returns `Vec<ListingId>`; every `ListingEvents` method
  takes the id.
- Executability: M2a was two executors' work, split into M2a (events, runtime, `AppListings`) and M2b (overlays,
  observers, behavior changes 1 and 2); old M2b is M2c. M4a carried the watcher rewrite and change 3; those moved to a
  new M3c with change 5. M1 now re-points `crate::ignore_poison` and the `cmdr_fs` re-exports, which M2c's grep would
  otherwise trip on.
- DONE criteria made mechanical: exact `rg` commands for M1, M2a, M2b, M2c, M3a (the old `'static '` grep matched trait
  bounds), M3b, and M3c; M4a is compiler-enforced by making every submodule but `host` private.
- Lanes: verified every named lane exists in `scripts/check/checks/registry.go`; added `desktop-svelte-e2e-playwright`
  where watcher or create behavior moves (M2b, M3c, M4a) and `desktop-rust-jscpd` to M4a. M5 drops the `caching.rs`
  `file-length` entry, since the target layout is its TODO split.

## Review round 4 (round 3's additions: M3c, `RefreshOutcome`, the `create.rs` fix, behavior change 6)

- Behavior change 6, upsert: verified safe for mkdir, mkfile, and paste-as-file (no create overwrites, so a row at the
  path is the watcher's or a stale one; `insert_entry_sorted` re-checks under the write lock, so a race can't
  duplicate). Wrote the argument into the change. Pinned `name` to `new_path.file_name()` so the row's spelling stays
  today's. Corrected "index apply is a no-op for `root`": it isn't off `root`, since indexed `LocalExternal` drives get
  the upsert too; M4a pins its idempotence or skips it for creates.
- Behavior change 5: "the read generation moved" now names the PUBLISHED generation. Comparing against the last one
  handed out would drop sizes whenever an in-flight re-read later failed or was superseded.
- M3c: `handle_directory_change` stays as the app wrapper that interprets `RefreshOutcome` (six production and test
  callers, ⌘R among them). DONE grep is now an exact `rg` that also catches `has_entry` and `enrich(` (the watcher's
  classification and enrich move into `observe_children`), and the four behavior-change cells are part of DONE.
- Verified sound: the pre-M0 `create.rs` fix (volume-scoped lookup with the id the function already derives),
  `RefreshOutcome`'s variants against every exit of today's `handle_directory_change` (including `NoVolume` → the
  `std::fs` fallback with `OverlayRows::Unchanged`), and M3c as one executor's work that compiles and passes per commit.
