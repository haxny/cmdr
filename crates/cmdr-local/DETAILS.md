# cmdr-local details

Pull-tier docs for the local backend: decisions, the reader, space, the rename primitive, the tests, and the capped
surface. Must-knows live in `CLAUDE.md`. The trait shape, capability matrix, and streaming patterns every backend shares
are `apps/desktop/src-tauri/src/file_system/volume/DETAILS.md`.

## Decisions

**Decision**: `LocalPosixVolume` is a crate of its own, like every other backend **Why**: so the write engine can depend
on a crate for every local primitive (the backend, the reader, the no-replace rename, space, the mount probes) without
depending on the app. Folding it into `cmdr-fs` was rejected: that crate is shared vocabulary every backend builds on,
and the local backend brings objc2/NSURL and the live-space memo with it.

**Decision**: `LocalPosixVolume` takes a `VolumeHost` at construction, like every backend crate **Why**: its couplings
were the app's LOCAL machinery rather than a protocol: the pane patch after a mutation, the watch-coverage oracle, and
the panes' name order, plus space, macOS metadata, and the mount probes. The first three are `ListingHost` calls
(`directory_changed`, `local_watch_coverage`, `sort_listing`; the app answers them in
`apps/desktop/src-tauri/src/listing_host/backends.rs`), and the rest is OS-probe code that lives here, so the backend
names nothing in the app. `patch_listing_after_local_mutation` stays the backend's own: it `std::fs`-stats the changed
entry, which no backend on a protocol can do. The host is a required argument, ❗ never a default: a production volume
on `VolumeHost::detached()` would silently turn off the scan oracle's fast path and leave panes stale after every local
copy. The git portal is ❌ not a coupling: it reaches a repo's virtual trees through a route plus a listing overlay, and
`local_posix.rs` names git nowhere (`apps/desktop/src-tauri/src/file_system/git/DETAILS.md` § "Two seams, no hooks").
Seam rationale: `crates/cmdr-fs/src/volume/host/DETAILS.md`.

**Decision**: a `Volume` method's blocking work goes to an ambient `tokio::task::spawn_blocking`, not to
`self.host.runtime()` **Why**: a `Volume` method runs on its CALLER's runtime, as every backend's does (SMB, SFTP,
WebDAV, and archive spawn the same way): the trait is runtime-agnostic, and the caller (the write engine, a pane
listing) already chose which runtime polls the future. Routing only the blocking half to the host's would split one call
across two pools for nothing; in production both are the app runtime. The write engine's own spawn rule
(`crates/cmdr-operations/src/host/CLAUDE.md`) covers the engine, not the backends it drives.

**Decision**: the mount probes the engine and `volumes*/` ask live here, and their callers name this crate **Why**:
they're the same OS-probe code the backend's own answers need (the `getfsstat` snapshot in `mount_table.rs`, `statfs` in
`statfs.rs`, `/proc/mounts` in `linux_mounts.rs`, the filesystem behind a path and whether it's a network one in
`filesystem_kind.rs`), and none of them knows the app. The platform-free classification they feed (`FilesystemKind`,
`MaxFileSize`, `mount_is_local_disk`) is `cmdr_fs::filesystem_kind`. What stays app-side is policy over the snapshot:
which rows the switcher shows, and whose mount is another account's own
(`apps/desktop/src-tauri/src/volumes/mounts.rs::mount_is_private_to_another_user`).

**Decision**: `write_from_stream` `sync_data`s each file (+ best-effort parent-dir fsync) before it returns **Why**:
Every cross-volume copy/move that lands on a local disk (MTP → Local, SMB → Local, USB import) flows through this one
method. A bare `file.flush()` finish is a userspace no-op on a raw `std::fs::File`, so the bytes would sit only in the
OS page cache when the op reports "complete", letting the user eject or sleep and lose data (on a move, from both sides,
since the source delete runs after the copy reports Ok). The `sync_data` (fdatasync) gives the "durable as each file
completes" property the local-FS chunked copy already has (`transfer/chunked_copy.rs`), so a crash mid-batch leaves
earlier files safe. The parent-dir fsync makes the file's directory entry durable too. Both are best-effort on error: a
failure logs under `target: "write_durability"` and continues rather than failing a completed multi-GB transfer at the
final fsync (matching `durability::flush_created_destinations`). Non-local backends (MTP/SMB/InMemory) need no
equivalent: durability there is the device's or server's concern. Pinned by
`local_posix_test::test_write_from_stream_multichunk_is_durable_and_correct` (content-correctness regression guard; the
fdatasync itself isn't observable from a unit test). Unknown-length writes take the same chunked durable path: progress
keeps `StreamLength::Unknown`, and success includes `sync_data` plus the parent-dir fsync.

**Decision**: `LocalPosixVolume` overrides `is_directory` and `entry_kind` with a bare `lstat` **Why**: `get_metadata`
also stats a link's target and looks up owner names, none of which answers "what is at this path". Both answers see a
link as the link, which a move engine depends on (`crates/cmdr-operations/src/transfer/DETAILS.md` § "Symlinks are
opaque to a move"); ❌ don't route either through `get_metadata`, whose `is_directory` is true for a link to a folder.

**Decision**: `LocalPosixVolume` overrides `list_directory_as_source`, and `scan_subtree` filters its walk the same way,
leaving out the AppleDouble sidecars of a FAT or exFAT folder **Why**: the kernel serves `._name` as `name`'s xattrs
there, so a cross-volume copy that also streamed it landed a stray hidden file. The pane's `list_directory` still shows
them, since they're on disk. A level with no `._` name never leaves the async task. `with_sidecar_probe` lets a test
play a FAT volume on APFS. Rule: `crates/cmdr-fs/src/apple_double.rs`; who lists as a source:
`crates/cmdr-operations/src/transfer/volume/DETAILS.md` § "A source is listed as a source".

## A consumer's test build is a test build here too

`cfg(test)` is set only while this crate compiles its OWN tests, so the app's tests would otherwise run the production
shape of everything below. The `testing` feature (the app turns it on through a dev-dependency) widens exactly what the
app's test build needs, the reach items with `any(test, feature = "testing")` and the behavior gates through
`TEST_FAST_PATHS` (the decision below):

- `space::get_volume_space` answers from `statvfs` (§ "Local volume space"): without it, the #357 stall comes back in
  the write engine's transfer and archive move-out suites and the app's ADB suites.
- `PROGRESS_SAMPLE_INTERVAL` is 1 ms (§ "The local reader").
- `LocalPosixVolume::with_sidecar_probe`, which the write engine's `copy_sidecar_tests.rs` plays a FAT folder with.
- `reading::list_directory`, which only the app's test-only `RealFileSystemProvider` calls.

The first two are `cfg!` branches rather than `#[cfg]` items, so both arms compile in every build and neither leaves
dead code behind in the other.

**Decision: the two behavior gates read `TEST_FAST_PATHS` (`lib.rs`), which also requires `debug_assertions`.** Why:
Cargo builds a bench target with its package's dev-dependencies, so the engine's Criterion bench
(`crates/cmdr-operations/benches/transfer_benchmarks.rs`) compiles this crate with `testing` on. Gated on `testing`
alone, the bench measured the fast paths, and `stream_large_file` ran 17% faster than what ships
(`docs/notes/transfer-extraction-baseline.md` § "Criterion, after"). A separate cargo feature can't make the cut: the
engine's own tests need the `statvfs` answer (#357), and one package's tests and benches share one dev-dependency
feature set. The `bench` profile has no `debug_assertions` and every test profile does, so the cut falls exactly between
them. The two reach items (`with_sidecar_probe`, `list_directory`) stay on `testing` alone: the bench needs them to
compile, and they change no behavior. (verified with a temporary
`const _: () = assert!(cfg!(feature = "testing") && !TEST_FAST_PATHS)`:
`cargo bench -p cmdr-operations --bench transfer_benchmarks -- --test` built and passed,
`cargo check -p cmdr-operations --tests` refused it, 2026-10-17)

## The local reader

`reading.rs` is the `std::fs` reader: `list_directory_core` (one `read_dir` plus an `lstat` per entry, built into
`FileEntry`s and sorted by the host's `sort_listing`), `get_single_entry` (one stat), and `get_extended_metadata_batch`.
`LocalPosixVolume` lists and stats through it, and so do the paths that read a local directory with no volume in hand:
the app's watcher (its unregistered-volume fallback and its per-entry re-stat) and `tags.rs`. It's the local backend's
reader, not the listing engine's, which is why it sits here and `cmdr-listing` only sees the `FileEntry`s it returns.
`mutation.rs` is the other half: `patch_listing_after_local_mutation`, what a successful local mutation means for the
listing cache (`apps/desktop/src-tauri/src/file_system/volume/DETAILS.md` § "Mutation notification"). The app's
`create.rs` sends its instant row for mkdir, mkfile, and paste-as-file through it too.

**Progress.** The pane's "Loaded N files…" readout needs `on_progress` fed while the stat loop runs (why it matters:
`crates/cmdr-listing/DETAILS.md` § "Local listing progress"), but the local backend can't take the callback where the
work happens. `on_progress` is `Sync` but not `Send`, and `LocalPosixVolume` runs its stat loop on `spawn_blocking`,
which demands `Send + 'static`. So the two halves are split:

- `list_directory_core_with_tally` publishes into a `ListingTally` as it stats, one relaxed atomic bump per entry,
  unthrottled. That's free next to the `stat` it accompanies.
- `LocalPosixVolume::list_directory` samples the tally every `PROGRESS_SAMPLE_INTERVAL` through
  `cmdr_fs::volume::listing_progress::report_while`, and calls `on_progress` from there. The callback never leaves the
  async task that owns it. The git portal's commit walk uses the same helper.

Two details the helper depends on. Its `select!` is `biased` so a listing that finished during a tick returns its real
result rather than spending another sample on an approximate count. And a snapshot of zero is dropped rather than
emitted, because the blocking pool may not have picked the task up yet and "Loaded 0 files..." is worse than the
"Opening folder..." it would replace.

Putting the throttle in the sampler rather than the stat loop means one place decides how often the number changes, and
it's the place that knows it's driving a UI. `PROGRESS_SAMPLE_INTERVAL` is 200 ms in production and 1 ms in a test
build, so `listing_a_local_directory_reports_progress_while_it_reads` can pin the wiring against a 5,000-entry scratch
dir instead of needing one big enough to outlast a real interval.

SMB and MTP wire `on_progress` through their own listing loops directly, having no `spawn_blocking` hop to cross.

**Metadata tiers.** Tiers 1-2 are fetched eagerly (stat + uid→name), tiers 3-4 deferred. With 50k+ files, each metadata
piece has a different cost: Tier 1 (name, size, dates, permissions) is free from a single `stat()`; Tier 2 (owner name,
symlink target) is ~1 μs and cacheable; Tier 3 (macOS Spotlight/NSURL metadata) costs ~50-100 μs/file; Tier 4 (EXIF,
PDF) costs 1-100 ms+ and reads file content. The full tier table: `docs/notes/file-metadata-tiers.md`.

macOS extended metadata (`addedAt`, `openedAt`) needs `listxattr()` / `getxattr()` beyond the fast `fs::read_dir()` +
`metadata()` path. Available via `get_extended_metadata_batch()` but not wired into the streaming path yet. Finder tags
are deferred too, and `list_directory_core` never reads them: `crates/cmdr-listing/DETAILS.md` § "Finder tags".

## Local volume space

`get_volume_space(path)` uses `NSURLVolumeTotalCapacityKey` and `NSURLVolumeAvailableCapacityForImportantUsageKey`
(falls back to `NSURLVolumeAvailableCapacityKey`). Returns `None` for non-existent paths.

`space.rs` holds it, beside `get_space_info_for_path` (what `Volume::get_space_info` returns) and the `statvfs` reading
Linux uses and macOS falls back to.

**Decision**: a test build answers from `statvfs` (`space::get_space_info_statvfs`), and only the NSURL tests in
`space.rs` call the real `nsurl_volume_space`. **Why**: the first NSURL call in a process runs `CFBundleGetMainBundle`,
which `readdir`s the test binary's own directory. On a bloated `target/debug/deps` (457k entries) with ~16 nextest
processes doing that at once, transfer, archive move-out, and ADB tests reaching the copy pre-flight or
`LocalPosixVolume::get_space_info` blew their 5 s waits (#357). Every space path (`get_space_info_for_path`,
`live_volume_space`, the copy pre-flight, the IPC command) funnels through `get_volume_space`, so that one function is
the seam. "A test build" means this crate's own and the app's (through `testing`, § "A consumer's test build is a test
build here too"). `space::tests::a_test_build_answers_a_space_query_without_nsurl` pins it. Verified 2026-10-14 with a
tripwire on every NSURL resource lookup: none of the 54 tests in the five suites #357 named reaches one.

### Live volume space

`live_space.rs::live_volume_space(path)` is what the space poller reads every 2 s (through `Volume::get_live_space_info`
and the poller's unregistered-path fallback). It returns the same important-usage figure as `get_volume_space`, mostly
at `statfs` cost:

- **Why**: on the boot volume the important-usage query costs ~6.5 ms of CPU (5.9 of it system time, plus work in the
  `deleted` daemon), against ~0.01 ms for `statfs`. The poller asks for the boot volume permanently (the low-space
  check), so that alone was ~0.33% of a core. On external and SMB volumes the query is ~0.02 ms; the cost is
  CacheDelete's purgeable accounting on the boot container.
- **How**: important-usage free = `statfs` free + purgeable, and ordinary writes and deletes move `statfs` and the
  important-usage figure by the same bytes while purgeable holds still. So each filesystem (keyed by mount point) keeps
  an anchor: the last important-usage reading and the `statfs` taken beside it. A live reading is the anchor plus how
  far `statfs` moved since, clamped to the drive.
- **When it asks again** (`plan`): no anchor yet; the anchor is 60 s old; `statfs` drifted by 1/1,000 of the drive (one
  readout step: past that it may be macOS purging, which raises `statfs` free while the important-usage figure holds);
  the total changed; or a write operation settled since (`expect_space_change`, from the app's
  `TauriEventSink::emit_settled`), because deleting a file a Time Machine local snapshot still holds frees purgeable
  space `statfs` never sees.
- **Evidence** (verified on macOS 27.0, a Swift probe sampling all three figures every 2 s for 5 min, 2026-09-27):
  `statfs` free equals `NSURLVolumeAvailableCapacityKey` to the byte, purgeable held at 10.692 GB through 3.3 GB of
  writes, and the anchor-plus-delta figure stayed within 0.4 MB of the real one (one sample 10 MB off: a write inside
  the call). Numbers and before/after CPU: `docs/notes/performance/space-poll-cost-2026-09-27.md`.
- ❌ **Only for readouts that poll.** Copy validation and the transfer pre-flight act on the figure once, so they keep
  calling `get_volume_space`, which always asks.
- The anchors are a process-wide memo (`ANCHORS`), the one static here: a cache of an OS answer the poller and
  `expect_space_change` share, not engine state.

## Local renames are atomic-exclusive

`LocalPosixVolume` routes every non-forced rename through the shared atomic-exclusive primitive,
`rename_local_exclusive`. This applies equally to `/`, attached disks, Dropbox, iCloud, and other local POSIX roots
registered with non-root volume IDs. Forced renames retain normal POSIX replacement semantics because the caller
explicitly authorized replacement.

**There is exactly one implementation of it**, and the write engine's `overwrite::rename_no_replace` (`cmdr-operations`)
is a one-line delegation to it, as is the in-flight sweep's local landing. Both names have to stay one function: while
they were two, only the copy side degraded on a filesystem lacking the kernel flag, so the pane's own renames failed on
every mounted SMB share while a copy landing in the same folder succeeded (ERR-8RFN4, app 0.45.1).

**The kernel flag is not universal.** `renamex_np(RENAME_EXCL)` and `renameat2(RENAME_NOREPLACE)` are APFS, HFS+, and
ext4; macOS's smbfs answers `ENOTSUP` (errno 45, verified twice against Synology SMB mounts: macOS 26.6.2 from the
ERR-8RFN4 bundle's log, and macOS 27.0 by a direct `renamex_np` probe where APFS took the flag, the mount returned errno
45, and the check-then-rename fallback landed, 2026-09-17), and other non-native mounts answer `EINVAL` or `ENOSYS`.
Those three degrade to a `symlink_metadata` check then a plain `rename(2)`; ❌ every other errno reaches the caller
unchanged, or a refusal the filesystem meant becomes a second, quieter way to fail. The degradation is racy and
deliberately so: a lost race refuses, where a bare `rename(2)` would clobber silently and unrecoverably.

**FAT32 and exFAT are NOT on the degraded path**, which is worth knowing before sizing any work that calls itself "the
USB-stick case": both take the flag and answer `EEXIST` (verified on macOS 27.0 by a direct `renamex_np` probe against
freshly created FAT32 and exFAT disk images alongside HFS+ and APFS controls, all four identical, 2026-09-19). What
actually reaches the fallback is macOS-mounted SMB and FUSE mounts.

**The check is three-valued, and ❗ that is the whole point.** `name_state` folds the `symlink_metadata` result into
`Occupied` / `Free` / `Unknown(err)`, and `rename_if_free` renames on `Free` alone. A stat that failed for any reason
other than the name being free has proved nothing about the name, so `Unknown` refuses and carries the filesystem's own
errno out. Collapsing it into "free" (which `.is_ok()` did) turns every transient stat failure into a silent,
unrecoverable clobber, on exactly the flaky mounts that reach this path in the first place. The enum exists so the fold
can't be written as a bool again.

The `ENOTSUP` path can't be reached on a filesystem a test can mount, and `testing::disk_images` is not for minting a
FAT image, so `rename_exclusive_with` takes the attempt as a parameter and `local_posix_test.rs` injects the errno. The
`Unknown` arm needs no seam at all: a stat and the rename beside it resolve the same path through the same permissions,
so nothing a test can set up fails one without failing the other, and only a transient failure separates them.
`local_posix_test.rs` hands `rename_if_free` the verdict directly instead.

## Testing

- `local_posix_test.rs`: real-FS cells (write ops, symlinks, copy, space info, the rename's degradation) on scratch
  dirs, against a detached or recording host.
- `local_posix_conformance_test.rs`: the shared `cmdr_fs::volume::conformance` assertions this backend can run, the way
  every crate backend keeps its own. The roster and what each defends: `crates/cmdr-fs/DETAILS.md` § "The shared
  assertions in `volume::conformance`".
- `reading_test.rs`: the reader on its own; the inline `tests` modules in `space.rs`, `live_space.rs`, `mount_table.rs`,
  and `linux_mounts.rs` pin the probes.
- `reading_parity_test.rs`: what the lister yields today, field for field, each listed entry equal to its
  `get_single_entry` re-stat (every field, through `Debug`). Each field a write path acts on also gets a cell against
  `lstat` itself, since parity alone can't see both readers going wrong together. Every cell runs once per entry of its
  `LISTERS`, so a second lister is proven by adding it there. The one known difference: `/.hidden` at a volume root,
  which the re-stat never applies. Its `real_image` module (a mount point's parent, the fixture on HFS+) runs in
  `pnpm check disk-images`.
- **The cells that need the app's real host stay app-side**, in
  `apps/desktop/src-tauri/src/listing_host/tests/local_posix_host_test.rs`: the watch-coverage answer following the real
  FSEvents watcher, the panes' name order, and a mutation reaching the pane through the app's composer. They assert on
  `listing_host::backends::AppListings`, so they live beside it.

## The public surface is capped

`index-crate-isolation` holds this crate to its measured surface (12 root promises, nine public modules, 39 items, no
handle), with no headroom. A backend's API is the `Volume` trait, `cmdr-fs`'s promise, so everything counted serves an
app read of a local disk that doesn't go through a volume:

- **The root**: `LocalPosixVolume`, `rename_local_exclusive` (the engine's landings and the in-flight sweep), and
  `rename_volume_error` (the app's single rename, which names the right path in its error).
- **`reading`** (`list_directory_core`, `get_single_entry`; `list_directory` is `testing`-gated) and **`mutation`**
  (`patch_listing_after_local_mutation`): the watcher, `tags.rs`, and `create.rs`'s instant row.
- **`space`** (`get_space_info_for_path`, `get_volume_space`) and **`live_space`** (`live_volume_space`,
  `expect_space_change`): the index's tier-2 progress, the copy pre-flight, the IPC command, and the space poller.
- **The mount probes**: `filesystem_kind` (seven, counting `probe_mount_for_path`'s three platform arms), `mount_table`
  (four), `statfs` (three), `nsurl` (two), and `linux_mounts` (16, the `/proc/mounts` and `mountinfo` readers
  `volumes_linux/` builds on). The counter reads source, so each platform's module counts whatever the build target.

A new item names which of these callers it serves, or goes behind `testing`.
