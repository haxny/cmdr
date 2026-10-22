# Date added sort

**A fresh download with an old mtime doesn't land on top of Downloads, because Cmdr can't sort by Date added.** This
plan adds an `Added` sort column beside `Created`, a single "displayed date" rule, a per-volume `supported_dates`
capability with a requested-vs-effective sort, and moves the local listing on APFS and HFS+ to `getattrlistbulk`, which
is faster AND brings the added date for free. Tracking issue: [#691](https://github.com/vdavid/cmdr/issues/691), from
[cmdr-reports#79](https://github.com/vdavid/cmdr-reports/issues/79).

"Last opened" is not in this effort: its xattr costs ~17 µs per file (12× a 100k listing) and isn't Finder's whole
source (Spotlight's `kMDItemLastUsedDate` is). Findings and next steps:
[#694](https://github.com/vdavid/cmdr/issues/694).

## Open questions for David

1. **Sandbox-protected entries change shape.** An entry whose `lstat` gets `EPERM` (`/private/var/db/...`, SIP and
   sandbox paths) lists today as a dateless "file" row; `getattrlistbulk` returns its real attributes, so it lists as
   the folder it is. The plan takes bulk's answer (more honest, and opening it still says "no permission"). Say if you'd
   rather keep today's row.

## Loud rules (every executor)

- ❗ **The date the UI shows comes from ONE pure function**, `displayedDateColumn(effectiveSortBy)`. ❌ No second
  derivation in a view, the measurer, SelectionInfo, a tooltip, or the a11y helper. ❌ No "last date sort" memory of any
  kind.
- ❗ **The requested sort is never mutated by a fallback.** Pane state, the persisted `sortBy`, View > Sort by, the
  command state, and MCP's `sort` field keep the requested column. Only the backend's EFFECTIVE column orders rows.
- ❗ **The fallback rule lives in ONE Rust function** (`EffectiveSort::resolve`), and every sort site in `cmdr-listing`
  and `sort_search_results` sorts through it. The frontend never computes "effective"; it reads it off the listing.
- ❗ **A backend fills `created_at` / `added_at` only when its `supported_dates` claims that date.** Pinned by a
  `volume::conformance` assertion every backend runs.
- ❗ **The local lister never drops, invents, or reshapes an entry.** Every `FileEntry` field but `added_at` stays
  byte-identical to today's (M1's cells), and `is_symlink` above all: delete trusts a local listing's link bit to stop
  descending (`EntryKind::of`, `cmdr-fs/src/volume/entry_kind.rs:26`; `listings_mark_links`), so a link reported as a
  plain folder deletes its TARGET's children.
- ❗ **The renderer and the measurer stay one contract** (`views/CLAUDE.md`): the Full date column's header label,
  chevron, fallback glyph, and cell text are measured from the same helpers they render from.
- ❌ Never translate a string you add: English plus a good `@key` description. Native menu strings need
  `pnpm intl:native-strings`.
- ❌ No new shortcuts (Created has none either).
- Strip milestone tags from code, comments, test names, and docs before each commit
  (`docs/guides/multi-agent-refactors.md`).
- Gates per milestone: `pnpm check --fast && pnpm check svelte-tests-related`, plus `pnpm check clippy` when Rust moved.
  Run commands in the foreground only.

## The code today (fresh look, 2026-10-22)

- **Sort columns**: `SortColumn { Name, Extension, Size, Modified, Created }` (`crates/cmdr-listing/src/sorting.rs:18`),
  mirrored by hand in `apps/desktop/src/lib/file-explorer/types.ts:416`, `lib/commands/types.ts:45`,
  `routes/(main)/explorer-api.ts:173,176` (inline unions), and `bindings.ts`. Default orders: `types.ts:422`. Persisted
  parse: `lib/app-status-store.ts:141`.
- **One comparator**: `entry_comparator` (`sorting.rs:220`) over `SortableEntry` (`sorting.rs:74`), implemented by
  `FileEntry`, the private `Keyed`, and `SearchSortRow` (`src-tauri/src/commands/search.rs:369`, modified only;
  `created_at` answers `None`).
- **Sort sites** that read a column: `streaming.rs:340` (first read), `operations.rs:94` (test seam),
  `operations.rs:524` (resort, which writes `listing.sort_by` at `:525`), `pipeline.rs:65`, `mutate.rs:43,188`, and
  `reread.rs:182-188`. That last one is `update_listing_entries`, THE landing step of every full re-read: `reread.rs`
  `refresh`, the watcher's `notify_full_refresh_locked` (`refresh.rs:139`), a backend's handed-over entries, and the
  backend-swap respell (`foreign_path.rs:180-209`, which re-resolves the volume). It sorts by `listing.sort_by` under
  the cache lock. The `(listing_id, sort_by, …)` tuples from `caching.rs:61-80,103-121` and `refresh.rs:190-205` are
  dead weight: every consumer destructures `(listing_id, ..)` (`caching.rs:171-205`, `refresh.rs:178`,
  `foreign_path.rs:139`). `CachedListing::sort_by` (`cached_listing.rs:125`) is one field, so requested and effective
  are the same thing today.
- **`ResortResult`** (`operations.rs:451`) answers both `resort_listing` and `set_listing_include_hidden`
  (`operations.rs:148`); the FE applies both through `applyRowResult` (`hidden-files-resync.ts` too).
- **`DirectoryDiffBatch`** (`diff.rs:126`) carries rows only; `publish_changes` (`cached_listing.rs:460`) publishes
  nothing when the diff is empty.
- **`FileEntry`** (`crates/cmdr-fs/src/entry.rs:118`) already has `added_at`, a last-use date field, and
  `extended_metadata_loaded`, plus an `ExtendedMetadata` struct (`entry.rs:414`). All dead in production (`added_at`
  stays and gets filled; the rest goes in M3):
  - `reading.rs::list_directory` (`:61-100`) is `testing`-gated and is the only filler; `get_extended_metadata_batch`
    (`:410-444`) has no caller (hence the file-wide `dead_code` allow at `:8-11`).
  - `macos_metadata.rs:73-75` fills a last-use date from `NSURLContentAccessDateKey`, which is atime: the wrong source.
  - Every backend and FE adapter sets `extended_metadata_loaded: true` and nothing reads it (`file-list-utils.ts:147`,
    `parent-entry.ts:26`, `entries-snapshot.ts:62`, `SearchResultsView.svelte:153`, `selection-info-feed.svelte.ts:254`,
    `listing_host/backends.rs:103`, and the backend crates).
- **The local lister**: `list_directory_core_impl` (`crates/cmdr-local/src/reading.rs:168`) is `read_dir` plus one
  `lstat` per entry (`process_dir_entry`, `:345`), one extra `stat` per symlink, `UF_HIDDEN` from `st_flags`, `/.hidden`
  at a volume root, `inode` only for hardlinked files, `permissions = st_mode` (type bits included), physical size
  `st_blocks * 512`. `get_single_entry` (`:273`) is the single-path twin the watcher's re-stat, `mutation.rs`, and
  `create.rs` use. Error shape today:
  - `read_dir` collects every name first (`:189`); an iteration error fails the WHOLE listing (`entry?`, `:195`).
  - The minimal row (`:198-210`: no size, no dates, `file` or `symlink-broken` icon) is reached only when
    `process_dir_entry` returns `None`: `file_type()` or `lstat` failed (dir readable but not searchable, the entry
    vanished between `readdir` and `lstat`, `EPERM`). A broken symlink is NOT one: its `lstat` succeeds, so it lists as
    a full link entry with `is_directory: false`.
  - `get_single_entry` never applies `/.hidden` (it has no `is_volume_root`), so a watcher re-stat of a `/.hidden`-named
    entry at a volume root clears its `is_hidden` today. Display only; pinned as is in M1, not fixed here.
- **Who reads a local listing's fields** (`LocalPosixVolume::list_directory`, `local_posix.rs:327`, and `get_metadata` →
  `get_single_entry`, `:381`):
  - Delete's scan descends only `EntryKind::Directory` (`cmdr-operations/src/delete/volume_scan.rs:72-78`), trusting the
    local `is_symlink` (`hidden_links.rs:20-38`, `listings_mark_links`). Copy scans skip links the same way
    (`scan.rs:121`, `scan_walker.rs:391`); transfer cleanup recurses on `is_directory || is_symlink` (`cleanup.rs:560`).
  - Hardlink dedup for the source footprint reads `inode` (`scan.rs:196-200`, `scan_walker.rs:419`); rollback's
    self-collision check calls two paths the SAME file when their `inode`s match
    (`cmdr-operation-log/src/rollback/ self_collision.rs:100`).
  - Conflict and safety reads by name and kind: `transfer/volume/preflight.rs:531`, `safety_oracle.rs:58,82`,
    `cleanup.rs:379,436,552`, `look_alike.rs:231`, `journal.rs:385`, `archive_edit/remote.rs:441`.
  - The copy-scan source filter (`local_posix/scan.rs:153-178`) drops AppleDouble `._` names by `name`.
  - The app: the watcher's incremental re-stat (`watcher.rs:525`) and std-fs re-read (`:622`), `tags.rs`,
    `mutation.rs:30`, `create.rs`'s instant row, multi-rename. The index does NOT read this lister; it has its own
    `getattrlistbulk` walker (`cmdr-index/src/indexing/scanner/walker/bulk_read.rs`).
- **Change detection ignores every date but mtime**: `diff.rs:248` (`is_entry_modified`) and `mutate.rs:197-199`
  (`sort_relevant_changed`). So today a Created sort doesn't re-place a row whose `created_at` changed. Same for the new
  columns unless fixed.
- **Frontend date surfaces**:
  - Full cells: `FullList.svelte:813` (`formattedDate(file.modifiedAt)`), row name `:845` (`modified: date.text`).
  - Full header: `FullListHeader.svelte:126-135` (column `modified`, label `columns.modified`).
  - Measurer: `measure-column-widths.ts:324` (header from `columns.modified` + `chromeFor('modified')`) and `:429`
    (cells from `modifiedAt`).
  - Brief header: `BriefList.svelte:983-998` (Modified, Created), every button `flex: 1` with ellipsis (`:1196`).
  - SelectionInfo: `SelectionInfo.svelte:207-208` (date from `modifiedAt`, tooltip via `buildDateTooltip`). Its
    `currentDirModifiedAt` prop (`:62`) is always `undefined` (`FilePane.svelte:2359`): dead.
  - Tooltip: `buildDateTooltip` (`selection-info-utils.ts:147`), with `dateTooltip.lastMoved` = "Last moved (“added”)".
  - Row a11y: `file-row-a11y.ts:70,102,128` (`details.modified`, `rowModified` aside).
  - Header a11y: `SortableHeader.svelte:68` (`columns.sortState`), command map `:48-54`.
- **Commands and menus**: `sort.byCreated` exists in the palette and View > Sort by (`menu_bar.rs:323`, untracked, no
  shortcut), routed through `menu-sort` (`menu_handlers.rs:464-475`, `listener-setup.ts:262`), deliberately unmapped in
  `command_map.rs` (test at `:700-707`). The macOS icon is set in `macos_appkit.rs:237`.
- **Context menus** are native muda popups (`src-tauri/src/commands/menu.rs`, `popup_context_menu` at `:79`, anchors for
  keyboard paths), the pick returning as a typed event (the tab menu's shape, `:411-433`).
- **MCP**: `sort` tool accepts `name|ext|size|modified|created` (`mcp/executor/view.rs:76`, schema
  `mcp/tool_registry/schemas/view.rs:35`, fixture `mcp/tests/fixtures/tools_list_snapshot.json`); pane state reports
  `sort: "field:order"` (`mcp/resources/panes.rs:211`) from `pane-mcp-sync.svelte.ts:149`; rows carry `modified` only
  (`:194`).
- **Capabilities**: `VolumeCapabilities` (`crates/cmdr-fs/src/volume/capabilities.rs`) is the IPC-published fold, "only
  what a consumer outside the backend acts on".

## Decisions

- **Effective sort is computed in the backend, at sort time.** Why: the listing cache, the watcher's sorted inserts,
  re-reads, search snapshots, and MCP all have to agree on row order, and the cache is the one place that orders rows
  ("smart backend, thin frontend"). The FE reads the result: `effectiveSortBy` rides on `listing-complete`,
  `ResortResult`, `sort_search_results`'s answer, and (when a re-read moves it) the `DirectoryDiffBatch` that lands the
  re-sorted rows. `listing-complete` is also what puts the glyph up on navigating into an unsupporting volume while
  already sorted by a date, so the FE never needs the volume's dates before a listing lands.
- **`EffectiveSort` is a type, not a convention.** `entry_comparator` and `sort_entries` take an `EffectiveSort`
  (column + order + dir mode) whose only constructor is `EffectiveSort::resolve(requested, order, dir_mode, supported)`.
  Rule: if `requested` is Created / Added and `supported` lacks it, the column is Modified; otherwise it's `requested`.
  A sort site that forgets the fallback then doesn't compile.
- **`CachedListing` keeps the requested column plus the volume's `SupportedDates`**, captured from the RESOLVED volume
  at read time (so a routed archive declares its own). `effective()` derives the column; ❌ no stored effective field to
  drift. The dates travel on `Prepared` from `prepare(ReadBy::Volume { .. })`, so the first read and every re-read
  (including the respell after a smbfs mount is swapped for direct SMB) refresh them in ONE place,
  `update_listing_entries`. `ReadBy::StdFs` and handed-over entries carry none and keep the listing's.
- **`supported_dates` is a `Volume` predicate, not a `VolumeCapabilities` field.** The engine is its only reader (the FE
  learns the effective column from the listing), and `capabilities.rs` says unread fields drift. Signature: async like
  the trait's other per-path methods (`Pin<Box<dyn Future<Output = SupportedDates> + Send + '_>>`, path in), default
  `SupportedDates::NONE` (Modified only), so a new backend can't claim a date it never fills; the conformance assertion
  catches a backend that fills one it didn't claim. The path parameter exists because the root volume can list a
  directory on another filesystem; backends other than local ignore it. Async because the local probe is a syscall on
  the listed directory and a smbfs mount can stall it: it runs in `spawn_blocking` beside the listing, the way the
  sidecar probe does (`local_posix/scan.rs:164-178`), ❌ never on an async worker or under the cache lock.
- **Local volumes ask the filesystem**: `getattrlist(ATTR_VOL_ATTRIBUTES)` on the listed directory, checking
  `validattr.commonattr` for `ATTR_CMN_CRTIME` and `ATTR_CMN_ADDEDTIME`, cached per `fsid` (measured: APFS and HFS+
  claim Added; smbfs, exFAT, and FAT32 don't; benchmark note § "Volume support"). Linux: Created when `statx` returns a
  birth time for the listed directory, never Added. Precedent for a per-device probe:
  `cmdr_fs::apple_double::volume_keeps_sidecars`. `FilesystemKind` is the test oracle, ❌ not the decider.
- **The local lister picks bulk or `read_dir` per listing, from one `fstatfs` on the directory fd it already opened**:
  bulk when `f_fstypename` is `apfs` or `hfs`, `read_dir` for everything else. Why each alternative loses:
  - A capability probe (`VOL_CAP_INT_ATTRLIST`) says yes for smbfs, where bulk is 2.2× SLOWER (smbfs answers `lstat`
    from its attribute cache, bulk re-enumerates over the wire), and the VFS emulates bulk on exFAT/FAT with nothing
    gained and a FAT physical-size gap. Speed, not support, is the question, and only the type name answers it.
  - `MNT_LOCAL` alone admits exFAT, FAT, NTFS, and FUSE.
  - A per-`fsid` cache is unnecessary: `fstatfs` on the open fd costs about a microsecond against 150 ms per 100k, it
    answers the root volume listing a directory on another filesystem by construction, and it can't go stale on a
    remount that reuses an `fsid`. (`supported_dates` keeps its `fsid` cache: its probe is a bigger `getattrlist`.)
  - Allowlist, not denylist: an unknown filesystem gets today's proven path.
- **Search results support Modified only.** `SearchSortRow` comes from the index, which stores no creation or added
  time. A search pane asked for Added falls back like any volume, with the same hint. Extending the index is out of
  scope.
- **The Full header's Sort by menu is a NATIVE popup** (muda), like every row, tab, and breadcrumb menu: OS-native,
  VoiceOver-ready, keyboard-anchored through `MenuAnchor`. Items: Date modified, Date created, Date added, the requested
  one checked (none in a search pane's relevance order). A pick on a NEW column sorts by it at its default order;
  picking the checked one does nothing. The pick returns as a typed event carrying the PANE, because a right-click on
  the unfocused pane's header must sort that pane (the `menu-sort` path only knows the focused one). Right-click on
  EITHER view's header opens it at the pointer, so the two views behave alike; only Full gets the chevron, since Brief
  already shows a button per date.
- **View > Sort by gets checkmarks** on the whole submenu (columns and order), showing the focused pane's REQUESTED
  sort, kept in step on focus change, tab change, pane swap, and every sort change. Same `CheckMenuItem` + FE push shape
  as the per-pane view-mode items (`update_view_mode_menu`, `commands/menu_state.rs:83`; `pushViewMenuState`,
  `pane/pane-accessors.svelte.ts:125`). § M5b.
- **Full date header click** calls `handleSortChange(pane, column)` with `column` = the requested sort when it's a date
  (Modified, Created, Added), else `modified`. In a fallback that toggles the requested column's order, which is also
  the effective one's, so the click visibly flips the rows and the requested column survives.
- **The fallback is discoverable on three channels**, none of them tooltip-only:
  - The Full date header shows an `info` `StatusGlyph` after its label, and the requested column's Brief header button
    carries the same glyph. The glyph sits INSIDE the `SortableHeader` button (a new optional `fallbackNote` prop),
    `aria-hidden`, with `tooltip={null}`: the button's `aria-label` replaces its content's name anyway, so the button
    carries the note in both its name and its tooltip (note above the command name), and two tooltips don't stack.
  - A user-initiated sort that lands in a fallback raises an info toast for the pane (`addToastForPane`, the
    `refusesSizeSort` shape, `pane/sort-operations.ts:99`). ❌ No extra `FilePaneAPI.announce`: the toast stack already
    speaks through its own polite live region (`ui/toast/ToastContainer.vsr.svelte.test.ts`), so a second message would
    read twice. Navigating into an unsupporting volume with a date sort already requested stays quiet apart from the
    glyph: a toast on every folder change is noise.
  - The header's accessible name says it (§ M9).
  - Draft copy (David reviews): "This location doesn’t record when items were added, so Cmdr sorts by date modified
    here." One key per date (`fileExplorer.sortFallback.created|added`), used by the toast, the glyph's tooltip, and the
    header name.
- **Labels**: column header `Added` (short, like `Created`); tooltip lines `Created`, `Date added`, `Last modified`
  (Finder's words); menu item `Date added`. The `lastMoved` key is REPLACED by a new
  `fileExplorer.dateTooltip.dateAdded` key so every locale re-translates it; delete `lastMoved` from `en` AND every
  locale file in the same commit (deleting isn't translating), or `desktop-message-keys-unused` fails.
- **Every new key**, English only, each with an `@key` description saying where it shows and how short it must be
  (native menu keys also say which menu, since no screenshot can show one: `docs/guides/i18n.md`):
  - `fileExplorer.columns.added` (header label, both views; "Keep it short", like `columns.created`, whose description,
    "(Brief view)", now also covers Full).
  - `fileExplorer.columns.dateSortMenu` (the chevron's name and tooltip, draft "Choose a date to sort by").
  - `fileExplorer.columns.sortStateFallback` (`{state} ({fallback})`, the header name in a fallback, so translators own
    the join; § M9).
  - `fileExplorer.dateTooltip.dateAdded`, `fileExplorer.dateTooltip.displayedLabel` (`{label} (displayed)`: a wrapper,
    ❌ never a bare ` (displayed)` suffix concatenated in code, so a language can put the word first).
  - `fileExplorer.list.rowCreated` / `.rowAdded` (`created {date}` …, `ASIDE_SLOTS` phrases).
  - `fileExplorer.sortFallback.created` / `.added`.
  - `menu.sort.dateAdded` (native: View > Sort by AND the header popup), then `pnpm intl:native-strings` (from
    `apps/desktop/`; `desktop-native-strings-fresh` fails on a stale table).
  - `commands.sortByAdded.label` ("Sort by date added"), plus `command-registry.parity.test.ts:97`'s English table.
  - Copy rules: sentence case, no "error"/"failed", no em dashes, curly apostrophes as the catalog uses.
- **Watcher and refresh**: added time changes only when an item lands in a folder, which the watcher already sees as a
  new entry, so a re-stat carries it: `get_single_entry` fills `added_at` exactly as the listing does (M2). ❌ No
  polling.
- **`..` shows no date**, as today (`createParentEntry` carries none). The dead `currentDirModifiedAt` prop goes.

## Milestones

### M1: characterize the local listing

- **Scope**: tests only, in `crates/cmdr-local/src/reading_test.rs` (and the listing-crate files named below). Pin,
  field for field, what `list_directory_core` and `get_single_entry` produce today on a real scratch dir: a file, a dir,
  an empty file, a hardlinked file (`inode` set) and a plain one (`inode` `None`), a symlink to a dir (`is_symlink` AND
  `is_directory`, `icon_id`), a symlink to a file, a broken symlink (a FULL link entry with `is_directory: false`, ❌
  not the minimal row: its `lstat` succeeds), a `chflags hidden` file, `/.hidden` at a volume root and not below one, a
  non-UTF-8 name (Linux only: APFS refuses one with `EILSEQ`), a sparse file and a `ditto --hfsCompression` file
  (`physical_size`), a pre-1970 mtime (`None`), `permissions` including the `S_IFMT` bits, owner and group names,
  `is_package`, `is_archive`. Assert `get_single_entry(p) == listed entry` for every case (one helper comparing every
  field, so a new field joins automatically: compare serialized JSON), with ONE named exception: `/.hidden` names at a
  volume root, where `get_single_entry` says not hidden (§ "The code today").
  - The minimal row: a `0444` directory (readable, not searchable) lists every name as today's minimal row and
    `get_single_entry` on a child errors. This is the case `getattrlistbulk` fails whole with `EACCES` (M2's runtime
    fallback), and it runs as a normal user on Linux CI too.
  - macOS only: listing `/` (firmlinks: `Users`, `Applications`, `Library`, …) and the parent of a
    `testing::disk_images` mount point both equal `get_single_entry` per entry. Bulk reports what these COVER; the cells
    catch a missed re-stat.
  - Write the helper so M2 can run every cell against both listers (a lister parameter, defaulting to today's).
  - Plus a cell in `crates/cmdr-listing/src/sorting_test.rs` pinning Created's `None` arms, and one pinning that
    `update_entry_sorted` (`mutate.rs:186`) re-places a row only on size, mtime, or kind today (it changes in M4; mark
    it as the regression anchor M4 flips).
- **Intentions**: make the `getattrlistbulk` swap provable. Pin reality, not the plan. Every assertion must fail when
  its field is perturbed (try one by hand per category: red first).
- **Landmines**:
  - `process_dir_entry` stats the TARGET only for `is_directory` (`reading.rs:349-353`) but takes everything else from
    the link (`:355-359`). Pin that split, it's easy to "fix" by accident.
  - Owner names come from a process-wide cache (`entry.rs:14`); compare names, ❌ never assume uid strings.
  - The fields write paths act on are `is_symlink`, `is_directory`, `inode`, `size`, and `name` (§ "The code today",
    "Who reads a local listing's fields"): every one needs a cell that goes red when perturbed, not only a JSON diff.
  - A test needing FAT or exFAT uses `testing::disk_images` (macOS), ❌ never its own `hdiutil` (`cmdr-fs/CLAUDE.md`).
    Add one FAT32 or exFAT image cell: it's the fallback path M2 must keep.
  - Gate macOS-only cells with `#[cfg(target_os = "macos")]`; the suite runs on Linux CI too.
- **Test plan**: these ARE the tests. All green on today's code; each verified red against a perturbed field.
- **DONE**: suites green, `pnpm check --fast`, `pnpm check clippy`. Committed alone.

### M2: the bulk lister on APFS and HFS+

- **Why it's a go**: measured in `docs/notes/getattrlistbulk-listing-benchmarks-2026-10.md` with the prototype in
  `crates/cmdr-local/examples/listing_bench/`: at 100k on APFS, 1.4–1.6× faster warm (241 → 156 ms) and 2.5–2.7×
  cold-ish (750–893 → 274–362 ms), `added_at` included at no marginal cost, zero field differences across 100k fixtures,
  `/usr/bin`, `/Applications`, `/System/Library/Frameworks`, `~/Library`, HFS+, exFAT, and smbfs once the guards below
  were in. (The alternative, `read_dir` + one `getattrlist` per entry, costs 2.5× today's listing.)
- **Scope**: a new private `crates/cmdr-local/src/reading/bulk.rs` (`#[cfg(target_os = "macos")]`), promoted from the
  prototype's `bulk.rs:135-340` (`list_fd`, `parse_entry`, `to_file_entry`, `type_bits`, the readers) with the fixes
  below. Linux keeps `read_dir` unchanged.
  - **Dispatch** in `list_directory_core_impl`: open the directory as an `OwnedFd`
    (`O_RDONLY | O_DIRECTORY | O_CLOEXEC`), `fstatfs` it, bulk when `f_fstypename` is `apfs` or `hfs`, else today's
    `read_dir` (§ Decisions). One `bulk_eligible(&statfs)` predicate serves the lister and `get_single_entry`.
  - **Entry building is shared, not copied**: lift `process_dir_entry`'s body into one
    `entry_from_lstat(path, name, &dot_hidden)` that also owns the minimal-row rule (`reading.rs:198-210`). The
    `read_dir` loop, bulk's per-entry fallback, and nothing else call it.
  - **`get_single_entry`** keeps its `lstat` path and, when `statfs` of the PARENT is bulk-eligible (`statfs` on the
    path itself follows a final symlink), adds one
    `getattrlist(ATTR_CMN_RETURNED_ATTRS | ATTR_CMN_ADDEDTIME, FSOPT_NOFOLLOW | FSOPT_PACK_INVAL_ATTRS)` (prototype
    `added_time_by_path`, `bulk.rs:366-401`; +3.5 µs). The watcher's re-stat then carries the same `added_at` a listing
    does (invariant 6).
  - `list_directory_core_with_tally` keeps feeding the tally per entry, from both paths.
  - **The harness stays, pointed at production**: a `testing`-gated reach item
    (`reading::list_directory_with(path, listings, Lister::Bulk | Lister::ReadDir)`; testing items don't count against
    the cap, `cmdr-local/DETAILS.md` § "The public surface is capped") replaces the prototype parser in the example.
    `listing_bench` keeps `gen`, `parity` (now production bulk vs production `read_dir`), `bench`, `volcaps`, and the
    per-path probes it prices (`added_time_by_path`, `read_last_used` for #694), so the note's numbers stay re-runnable
    against shipped code. The example's dev build already has `testing` on (`Cargo.toml:54-56`). Update the note's
    "Reproducing" and its first paragraph (it calls the code benchmark-only).
- **Intentions**: Added for every APFS/HFS+ entry at negative cost; everything else byte-identical (M1 green on both
  listers without edits, apart from `added_at`).
- **Landmines** (each bit the prototype; `file:line` is the prototype's fix):
  - **Offsets come from the REQUESTED attribute list, never the returned one** (`bulk.rs:236-256`).
    `FSOPT_PACK_INVAL_ATTRS` packs every requested attribute, an unsupported one as zeros with its returned bit clear;
    walking the returned set shifted later fields by 16 bytes. The returned set decides validity only.
  - `ATTR_CMN_ERROR` packs right after the returned set, before the name (`:217-219`); otherwise bitmap order, so
    `ATTR_FILE_ALLOCSIZE` precedes `ATTR_FILE_DATALENGTH` (`:264-266`). The dir group packs for `VDIR` only, the file
    group for everything else, symlinks included (`:257-267`).
  - **Mount points and firmlinks report the directory they cover** (stub owner, mode, and times). Detect with
    `ATTR_DIR_MOUNTSTATUS & DIR_MNTSTATUS_MNTPOINT` or `ATTR_CMN_FLAGS & SF_FIRMLINK` (`:289-299`) and build those
    entries WHOLLY from `get_single_entry`, `added_at` included (the prototype kept the stub's, which breaks invariant
    6). If that re-stat fails, the minimal row, ❌ never the stub.
  - **`EACCES` on a readable but unsearchable dir fails the WHOLE call** where `read_dir` lists minimal rows: fall back
    to `read_dir` at runtime, as for `ENOTSUP` / `EINVAL`. Any other errno reaches the caller, as an iteration error
    does today (`reading.rs:195`).
  - `permissions = (ATTR_CMN_ACCESSMASK & 0o7777) | type_bits(ATTR_CMN_OBJTYPE)` (`:319`, `:329-340`): APFS's mask
    already carries `S_IFMT`, masking makes it right either way.
  - `physical_size` from `ATTR_FILE_ALLOCSIZE` matched `st_blocks * 512` on APFS and HFS+ (compressed frameworks
    included); FAT32 reads 0, one more reason FAT stays on `read_dir`. M1's compressed and sparse cells guard it.
  - `/.hidden` must reach bulk entries too: the prototype's `is_hidden` (`:322`) skips it, and parity passed only
    because `/` has no `/.hidden`.
  - `getattrlistbulk` never follows links: a symlink still gets its one target `stat` for `is_directory` (`:301`), and
    `is_symlink` comes from `VLNK` alone.
  - Buffer: 64 KiB (measured flat from 16 to 512 KiB), allocated once per listing, reused across calls, `u64`-backed;
    every read is a bounds-checked `get(..)` + `from_le_bytes` (the kernel packs to 4 bytes, so ❌ no aligned casts, no
    transmute). Parse exactly `n` records, each `len >= 24` and inside the buffer.
  - Every `unsafe` block gets a concrete `// SAFETY:` (`clippy::undocumented_unsafe_blocks`). The fd is an `OwnedFd`, so
    no path forgets `close`.
  - Precedent: the index's own reader, `cmdr-index/src/indexing/scanner/walker/bulk_read.rs`, follows the same
    requested-slot rule and degrades a doubtful entry to `symlink_metadata`. Mirror its structure and its
    `bulk_matches_symlink_metadata` differential test; the crates don't depend on each other, so no sharing here.
- **Never drop, invent, or reshape an entry** (the loud rule): the bulk path ends in exactly one of three ways.
  - Every record parsed: return them.
  - A record can't be trusted: `ATTR_CMN_ERROR` set, or a REQUIRED attribute's returned bit clear (`OBJTYPE`, `MODTIME`,
    `OWNERID`, `GRPID`, `ACCESSMASK`, `FLAGS`, `FILEID`, and for non-directories `LINKCOUNT`, `ALLOCSIZE`,
    `DATALENGTH`): that ONE entry goes through `entry_from_lstat`, which yields today's answer or today's minimal row.
    The prototype's shortcut to a minimal row (`:278-285`) invents a dateless row where `lstat` may succeed. `CRTIME`
    and `ADDEDTIME` are optional: clear means `None`, ❌ never 0.
  - The stream itself can't be trusted (implausible length, a name past its record, `EINTR`, `EACCES` / `ENOTSUP` /
    `EINVAL`): discard every bulk entry and list again through `read_dir` from scratch. ❌ Never resume after `EINTR`
    (the man page doesn't say a retry can't skip a batch), ❌ never merge bulk and `read_dir` entries, ❌ never return a
    partial `Vec`.
- **Edge cases, decided**:
  - A name vanishing mid-enumeration drops out (the next call returns 0, measured), where today a vanished name between
    `readdir` and `lstat` becomes a minimal row. The watcher's event reconciles either way; no write path depends on it.
  - Non-UTF-8 names never reach bulk (APFS and HFS+ refuse them, `EILSEQ`); the path is still built from raw bytes and
    the name through `to_string_lossy`, as today. The longest legal name (255 UTF-16 units, ≤ 765 bytes) fits the buffer
    many times over.
  - Cancelation is unchanged: the listing runs to completion on `spawn_blocking` (`local_posix.rs:359-365`), only
    sooner.
  - TCC (Desktop, Documents, Downloads, iCloud Drive): `open(O_DIRECTORY)` is the same gate `opendir` hits, so a denial
    is the same `EPERM` at open. Untested under the app's own TCC identity (the benchmark inherited the terminal's
    grants): on David's QA list, both listers on those four folders in a fresh dev build, granted and not.
  - Dataless (evicted iCloud) files: every requested attribute is metadata the kernel keeps for an evicted file; no
    content or xattr read, so nothing materializes. Enumerating a dataless DIRECTORY materializes it with either lister,
    as today. Verify with one evicted file if at hand (`ls -lO` shows `dataless` before and after a listing), else
    record it as unverified in `cmdr-local/DETAILS.md`.
  - Sandbox `EPERM` entries list as what they are (open question 1).
- **Test plan**: M1's cells green on BOTH listers through `list_directory_with`. New red-then-green cells:
  - Parser cells over hand-built buffers: an invalid attribute in the MIDDLE of the requested set (the smbfs and exFAT
    shape that broke the prototype), a dir record with no file group, a symlink record with one, an `ATTR_CMN_ERROR`
    record, a missing required bit (→ `entry_from_lstat`), a truncated record (→ `read_dir` restart).
  - The `0444` dir takes the runtime fallback and yields M1's minimal rows.
  - `added_at` is set for a fresh file and changes after a `rename` into another folder; `get_single_entry` matches the
    listing's `added_at`, mount points and `/`'s firmlinks included; a FAT or exFAT image goes through `read_dir` with
    `added_at: None`.
  - Rerun `listing_bench parity` (zero differences) and `bench` at 100k against production; append both to the note.
- **DONE**: as M1, plus the note updated. `macos_metadata.rs` and `reading::list_directory`'s enrichment stay until M3.

### M3: `SupportedDates` across every volume, and the two-phase leftovers removed

- **Scope**:
  - `cmdr-fs`: `SupportedDates { created, added }` (`specta::Type`, `NONE` const), the `Volume::supported_dates`
    predicate (default `NONE`), and a `volume::conformance` assertion: no listed entry carries a date its volume doesn't
    claim. Wire the assertion into every backend's conformance test.
  - Declare per backend after reading each mapping, ❌ never from memory: local (M2's probe), SMB (`mapping.rs:36,47`
    fills created), MTP (`directory_ops.rs:897`), S3 (`query.rs:256`), WebDAV (`creationdate`, server-dependent: claim
    it, entries may still be `None`), git (`tree.rs`, `log.rs`, `virtual_listing.rs` set created, often = mtime: decide
    and say why), SFTP, NFS, ADB, archive, restic, `InMemoryVolume` (configurable, so tests can play a volume without
    Added), and the app's mock provider. Record the matrix in `crates/cmdr-fs/DETAILS.md`.
  - Remove `ExtendedMetadata`, `extended_metadata_loaded` and `opened_at` (Rust `FileEntry`, the FE `FileEntry`,
    `bindings.ts`), `get_extended_metadata_batch`, `macos_metadata.rs` (and its `CLAUDE.md` module-map line), the
    `dead_code` allow, the `testing`-only enrichment in `list_directory`, and the `check-type-drift.ts` exemption.
    Update every fixture that set either field (`rg 'extendedMetadataLoaded|openedAt|opened_at'`, about 40 FE test files
    and the backend crates listed in § "The code today").
- **Intentions**: honest, per-volume dates before anything sorts by them; dead code gone so nobody wires the atime bug
  back in.
- **Landmines**:
  - `Volume::capabilities()` is a pure fold and ❌ must not be overridden (`cmdr-fs/CLAUDE.md`); this predicate stays
    out of it on purpose (§ Decisions).
  - `#![deny(missing_docs)]` in `cmdr-fs`.
  - An un-upgraded SMB share is a `LocalPosixVolume` over smbfs: its probe must answer for smbfs (likely Created only),
    and the direct `SmbVolume` that replaces it may answer differently. Note where a pane's listing re-resolves the
    volume (`refresh.rs`, `reread.rs`) for M5.
  - Removing a `FileEntry` field changes `bindings.ts`: `pnpm bindings:regen`.
- **Test plan**: conformance assertion red first against a deliberately lying `InMemoryVolume`; per-backend declaration
  cells where a backend has a mapping test already.
- **DONE**: gates incl. clippy;
  `rg -n 'extended_metadata|extendedMetadata|ExtendedMetadata|macos_metadata|opened_at|openedAt'` empty, the
  `listing_bench` example included (drop its `nsurl` method, `main.rs:121`, and its `opened_at` handling,
  `main.rs:238-246`; its xattr probe returns a bare value).

### M4: sort columns and effective sort in the backend

- **Scope**:
  - `SortColumn::Added`; `SortableEntry::added_at` (search rows answer `None`); a `compare_by_column` arm matching
    Created's `None` handling; the AlwaysByName comment at `sorting.rs:238-241` names the new date column.
  - `EffectiveSort` + `resolve` (§ Decisions); `entry_comparator` and `sort_entries` take it. Every sort site listed in
    § "The code today" goes through it.
  - `CachedListing` stores `supported_dates` (from the resolved volume, at `streaming.rs:331` and the test seam at
    `operations.rs:88`) and exposes `effective()`. `Prepared` carries the read's dates (§ Decisions) and
    `update_listing_entries` (`reread.rs:156`) writes them before it sorts. When that moves the effective column, the
    batch it publishes carries `effective_sort_by: Some(column)` (new optional field on `DirectoryDiffBatch`,
    `diff.rs:126`), and it publishes that batch even when the row diff is empty (`publish_changes` skips empty diffs
    today), or a one-row folder never tells the FE its displayed date moved. Coalescing keeps the last `Some`.
  - Drop the dead sort fields from the `(listing_id, sort_by, …)` tuples (§ "The code today"): `CachedListing` is the
    only holder of sort state, so nothing can hand a resolved column around to be persisted.
  - `ListingCompleteEvent` (`listing_host/events.rs:52`) and `ResortResult` (`operations.rs:451`) gain
    `effective_sort_by: SortColumn`. `ResortResult` is also `set_listing_include_hidden`'s answer (`operations.rs:148`):
    fill it from `listing.effective()` there too.
  - `is_entry_modified` (`diff.rs:248`) includes `created_at` and `added_at` regardless of the sort: the tooltip shows
    every date, so a row whose Created moved needs its Modify.
  - Replace `sort_relevant_changed` (`mutate.rs:197-199`) with `sort_key_changed(old, new, &EffectiveSort)` in
    `sorting.rs`, beside `compare_by_column`, reading the same `SortableEntry` accessors the comparator reads for that
    column, plus `is_directory`. For Size that includes `known_dir_size` (`recursive_size` + `recursive_size_complete`),
    which today's predicate misses: a directory whose recursive size moved under a LikeFiles or Mixed Size sort stays
    put. A false positive costs a harmless re-place; a false negative is a misplaced row, so when unsure answer true.
    Its one caller is `update_entry_sorted`, reached from `notify_modified` (`caching.rs:270`) and `observe_children`
    (`reread.rs:254`).
  - `sort_search_results` (`search.rs:424`) resolves with `SupportedDates::NONE` and answers
    `{ order, effectiveSortBy }`.
  - MCP backend: `view.rs:76` and the schema accept `added`; regenerate the tools fixture.
  - `pnpm bindings:regen`, and the FE compiles: extend both `SortColumn` unions and `explorer-api.ts:173,176`,
    `defaultSortOrders` (descending), `parseSortColumn`, and give the FE call sites a placeholder only where the
    compiler forces one (M5 owns them).
- **Intentions**: rows are always in the effective order everywhere (cache, inserts, diffs, search, MCP), and the
  requested column is what's stored and echoed.
- **Landmines**:
  - `operations.rs:525` writes `listing.sort_by = sort_by`: that must stay the REQUESTED column.
  - Only `effective()` feeds a comparator; `listing.sort_by` read directly anywhere but the echo to the FE is the bug
    this type exists to stop. `rg 'listing.sort_by'` after the change should show only the resort write and the echo.
  - `update_entry_sorted` resolves the index BEFORE `entries_mut` (`cmdr-listing/CLAUDE.md`); don't reorder while
    editing the predicate.
  - A sort change bumps the FE's `cacheGeneration` (`cmdr-listing/CLAUDE.md`); an effective-only change must too.
  - Flip M1's `update_entry_sorted` anchor: a `created_at` change under a Created sort now re-places the row.
- **Test plan** (red → green): `resolve` truth table; a listing on an `InMemoryVolume` without Added, requested Added,
  orders by mtime and reports `effective_sort_by = Modified` while `sort_by` stays Added; resort to Name and back;
  `update_entry_sorted` under Added re-places on an `added_at` change; a directory's `recursive_size` change re-places
  it under a LikeFiles Size sort; diff reports an `added_at` change; a re-read whose volume now claims Added (swap the
  `InMemoryVolume` between reads) publishes a batch with `effective_sort_by` even for a one-row listing; search sort
  with Added answers Modified's order and says so; MCP `sort` accepts `added`.
- **DONE**: gates incl. clippy.

### M5: frontend plumbing, commands, and menus

- **Scope**:
  - Pane state gains `effectiveSortBy`, swapped in atomically with the rows it describes, from every place the pane's
    row space changes hands:
    - `listing-loader.ts::handleListingComplete`, in the synchronous step beside `setTotalCount` (`:659`), before its
      first `await` (`findFileIndex`, `:666`).
    - `applyRowResult` (`pane/types.ts:221`), which serves both resort and the hidden-files toggle.
    - The pane's `directory-diff` handler, from a batch's `effectiveSortBy` when present, applied with that batch.
    - `getSwapState` / `adoptListing` (`listing-loader.ts:810-835`): a pane swap trades listings with no backend call,
      so the effective column travels in `SwapState` with them.
    - The snapshot store: `sortSnapshot` (`search/snapshot-sort.svelte.ts:106-110`) stores the answer's
      `effectiveSortBy` beside `sort` in `applySnapshotSort`. A snapshot in relevance order (`sort === null`) has no
      effective column, and the displayed date is Modified.
    - ❌ Not persisted. Expose it on `FilePaneAPI` for MCP.
  - Command `sort.byAdded`: `COMMAND_IDS`, `command-registry.ts` / `commands/sources/main-window.ts`,
    `sort-handlers.ts`, `SortableHeader.svelte:48` map, `listener-setup.ts:262` map, Rust `SORT_BY_ADDED_ID`
    (`command_map.rs`, unmapped like Created, extend the test at `:700-707`), a `menu_bar.rs` item after Date created
    (untracked, no shortcut), `menu_handlers.rs:464-475`, `macos_appkit.rs:237` icons, the menu bar snapshot tests
    (`menu_bar_test.rs`), `pnpm intl:native-strings`.
  - MCP: `pane-mcp-sync.svelte.ts` `sortFieldMap` gains `added`; pane state adds the effective column when it differs
    (for example `sortFallback: "modified"`), rendered in `panes.rs`; rows gain `created` and `added` ISO strings when
    present. `mcp-listeners.ts` `parseSortColumn` accepts `added`.
  - Fallback toast on a user-initiated sort (§ Decisions; the toast's live region is the announcement), in
    `sort-operations.ts`'s `install`, comparing the result's `effectiveSortBy` with the requested column. Copy keys
    `fileExplorer.sortFallback.*`.
- **Intentions**: keyboard and MCP reach the new sorts end to end; the FE has the one piece of state every display
  surface derives from.
- **Landmines**:
  - Retained rows after navigation are paint-only (`views/CLAUDE.md`); `effectiveSortBy` must stay with them until the
    new rows land, or the old rows render with the new folder's date.
  - The snapshot pane couples six points (`pane/CLAUDE.md`); its sort lives in the snapshot store, ❌ not the tab.
  - `shortcuts-store.ts:363` lists shortcut-bound sort commands; the new ones aren't.
  - `DualPaneExplorer.svelte` / `FilePane.svelte` are at their size cap: new logic goes in a `*.svelte.ts` / `*.ts`
    helper.
- **Test plan**: Vitest for the sort-operations fallback toast (fires on user sort, silent on navigation), the listener
  map, and the MCP state line; invariant 2 in a fallback (requested Added, effective Modified): a swap, a tab's
  persisted `sortBy`, the next `resortListing` call, and MCP's `sort` all carry Added, and `adoptListing` installs the
  partner's effective column; a diff batch with `effectiveSortBy` updates the pane; Rust menu snapshot and command map
  tests.
- **DONE**: gates incl. clippy.

### M5b: View > Sort by checkmarks

- **Scope**:
  - `menu_spec.rs`: a new `CheckRole::Sort(SortCheck)` (`Column(..)` per sort column, `Order(Ascending|Descending)`);
    the Sort by rows in `menu_bar.rs:319-326` become `check(...)` rows, accelerators unchanged (⌘3–⌘6 stay on Name,
    Extension, Date modified, Size).
  - ❗ Name, Extension, Date modified, and Size are TRACKED (rebindable): `update_menu_item_accelerator`
    (`accelerators.rs:233`) removes the item and reinserts a fresh plain `MenuItem` (`:255`), and `MenuItemEntry.item`
    is a `MenuItem<R>` (`mod.rs:337`). As is, the first rebind would silently turn a check item back into a plain one.
    Make `MenuItemEntry.item` hold either kind (Tauri's `MenuItemKind<R>` or a two-variant enum) so the swap recreates
    the SAME kind and copies `is_checked()`; `apply_menu_item_states` (`item_states.rs:205-213`) keeps greying it. Keep
    `CheckSpec` carrying the tracking the four items need.
  - `MenuState` gains the focused pane's requested sort (`Option<(SortColumn, SortOrder)>`; `None` = a search pane in
    relevance order: no column checked). `sync_sort_check_states` sets every sort item's check from it, finding each
    handle by id at call time (`sort_submenu.get(id)`, verify the Tauri 2.12 API) so a rebind's replacement is never a
    stale handle. ❌ No second handle store beside `items`.
  - A typed `update_sort_menu(sort: Option<{ column: SortColumn, order: SortOrder }>)` command in
    `commands/menu_state.rs` beside `update_view_mode_menu`, typed (❌ no string parsing like that one's `"full"`).
  - Click: muda has ALREADY toggled the clicked item when `on_menu_event` hears of it (`menu_handlers.rs:276`), so the
    sort arm (`menu_handlers.rs:464-475`) calls `sync_sort_check_states` FIRST (undoing the toggle; also the
    refused-over-dialog path), then emits `MenuSort` as today. The check moves when the FE's sort lands and re-pushes.
  - The FE push: ONE `$effect` in a new `pane/sort-menu-sync.svelte.ts` (`DualPaneExplorer` / `FilePane` are at their
    size cap) deriving the focused pane's requested `{ sortBy, sortOrder }` (the active tab's, or the snapshot store's
    for a search pane) and pushing on change, deduped. One derivation covers focus, tab switch, swap, palette, menu,
    header, and MCP sorts; ❌ no push call sprinkled per path. A fallback never moves it (invariant 2).
  - A language rebuild (`rebuild.rs:40-48`) builds a fresh bar: seed the sort checks from `MenuState`, as it already
    does for show hidden.
- **Intentions**: the menu bar always shows what the focused pane ASKED for, through one state and one writer.
- **Landmines**:
  - The SF Symbol icons (`macos_appkit.rs:237`) and display accelerators are re-applied after an accelerator swap
    (`settings.rs:374-376`); keep that for the check kind too. Check that AppKit draws the checkmark beside an item that
    also has an image.
  - Mnemonics (`mnemonics.rs`) are per submenu; adding Date added shifts Linux letters. The menu bar snapshot shows it.
- **Test plan**: `menu_bar_test.rs` snapshot (rows read `check`, both platforms); a pure `sort_check_states(state)` →
  `[(id, bool)]` cell per column, order, and `None`; Vitest for `sort-menu-sync` (pushes on focus change, tab switch,
  sort change, swap; one push per distinct value; a fallback pushes the requested column). Live: rebind ⌘3, then check
  Name stays a checked check item (David's QA list, since muda panics off the main thread in tests).
- **DONE**: gates incl. clippy; `pnpm intl:native-strings` if labels moved.

### M6: the displayed date in Full mode

- **Scope**:
  - New `apps/desktop/src/lib/file-explorer/displayed-date.ts`: `displayedDateColumn(effectiveSortBy)` (taking
    `SortColumn | null`, null being a search pane in relevance order), `dateOf(entry, column)`, the header label key,
    the tooltip label key, and the a11y phrase key per date column. Pure, fully tested. Add it to
    `file-explorer/CLAUDE.md`'s map.
  - Full cells (`FullList.svelte:813`), header label (`FullListHeader.svelte:126-135`, still a `SortableHeader`, active
    when the requested sort is a date), and the measurer (`measure-column-widths.ts:324,429`) all read it. The header's
    width includes the label of the SHOWN date, the chevron (M7), and the fallback glyph.
  - New Full date-cell tooltip and the reworked `buildDateTooltip`: every date the entry carries (in the order Created,
    Date added, Last modified; a date the entry lacks, or its volume doesn't claim, has no line), the displayed one's
    label wrapped in `displayedLabel` (§ Decisions). Today's shape stays: `{label}: {segments}` lines joined by `<br>`,
    each date's age-tier spans from `renderSegments` (`selection-info-utils.ts:147-162`); run the translated labels
    through `escapeHtml` (`tooltip.ts:231`), since the tooltip renders `html`. Replace `lastMoved` with `dateAdded` (§
    Decisions).
  - Full rows' accessible name: `details.modified` becomes `details.date: { column, text }` with one aside phrase per
    column (`rowModified` plus new `rowCreated`, `rowAdded`) in `ASIDE_SLOTS`.
  - The fallback glyph on the Full date header (§ Decisions).
- **Intentions**: one rule drives every Full surface, and the column fits whatever it shows.
- **Landmines**:
  - `HEADER_CHROME_ACTIVE/INACTIVE` mirror `SortableHeader`'s gap + caret (`views/CLAUDE.md`); the chevron and glyph
    need their own mirrored constants, in the same file.
  - `holdSizeColumnWidth` (`measure-column-widths.ts:449`) assumes the date width only follows `next`; a sort change
    changes the date label, which is a "different measurement" trigger, so check the caller re-measures on
    `effectiveSortBy` change.
  - The tooltip action has no lazy form (`lib/tooltip/tooltip.ts:484`): building four formatted dates per rendered row
    per render is waste. Add a lazy `html` (built on show) or build on `mouseenter`; keep it out of the row's `{@const}`
    block.
  - IPC sends `None` as `null`: guard with `!= null` (`measure-column-widths.ts:425-428`).
  - WCAG 2.5.3: the visible date text stays bare in the name, the phrase words in parentheses
    (`file-row-a11y.ts:87-91`).
- **Test plan**: `displayed-date.test.ts` (every column, fallback cases); measurer cells proving the width grows for a
  longer shown label and includes the glyph; tooltip ordering and suffix; `file-row-a11y.test.ts` phrases per column; a
  `mountFullList()` cell rendering Added cells under an Added effective sort.
- **DONE**: gates. Then hand David a screenshot-ready state (a pane sorted by Added on APFS, and one on a FAT image in
  the fallback, light and dark) for the header label, glyph, and column width; ❌ the agent doesn't judge the layout
  itself.

### M7: Full header Sort by menu and click semantics

- **Scope**: `show_date_sort_menu(window, pane, requested, anchor)` in `src-tauri/src/commands/menu.rs` with its builder
  in `src-tauri/src/menu/` (check items, requested checked, `menu.sort.*` labels incl. the new `dateAdded`), a typed
  `DateSortMenuPick { pane, column }` event from `on_menu_event`, and the FE listener calling the pick semantics (§
  Decisions). Right-click anywhere on either view's `.header-row` opens it at the pointer (no anchor). In Full, a
  chevron button beside the date header opens it anchored under itself (`MenuAnchor { x: rect.left, y: rect.bottom }`,
  viewport CSS px) on click, Enter, or Space. Header click semantics (§ Decisions).
  - The chevron: a `<button type="button">` with `aria-haspopup="menu"`, `aria-expanded` true while the show command is
    in flight (the popup blocks in Rust until it closes, `tab-operations.ts:205-207`), `<Icon name="chevron-down">`
    `aria-hidden`, name and tooltip `fileExplorer.columns.dateSortMenu`. `FieldHistoryChevron.svelte` is the house
    precedent for the look. ❌ No `tabindex`: it's a `button`, so it joins the header toolbar's ←/→ walk on its own
    (`CONTROL_SELECTOR`, `pane-chrome.ts`), right after the date header in DOM order.
  - It handles Enter / Space in `keydown` with `preventDefault`, as `SortableHeader` does
    (`SortableHeader.svelte:83-88`): a synthesized click would bubble to the pane's `onclick` and pull focus back to the
    list, and the next press should find focus still on the chevron.
  - Both share the date track in a wrapper span, like `.header-name-ext` (`FullListHeader.svelte:70-90`), so the grid
    keeps its columns.
- **Intentions**: the mouse reaches every date sort in Full mode without leaving the header, and the keyboard reaches
  the same menu.
- **Landmines**:
  - `focus_for_context_menu` is load-bearing (`menu.rs:31-54`), and `popup()` returns before the click arrives: deliver
    the pick by event, ❌ never a sync channel (`menu.rs:411-416`).
  - The anchor is LOGICAL CSS pixels (`menu.rs:69-78`).
  - Pane chrome keeps focus and its keys, ❌ no roving `tabindex` (`pane/CLAUDE.md`); read `pane/DETAILS.md` § Pane
    chrome for how ←/→ find buttons before adding the chevron.
  - `SortableHeader` is a `<button>`; the chevron is a sibling button, ❌ not nested.
  - A search-results pane's header cycles a third click to relevance (`clearsSortLabel`); the date header there must
    keep that.
  - ❗ The popup's item ids must differ from the bar's `SORT_BY_*_ID`: `on_menu_event` routes popup clicks through the
    same handler, and a bar id would sort the FOCUSED pane through `menu-sort`. Give them their own ids carrying the
    pane and column, parsed into `DateSortMenuPick`.
  - The date column is right-aligned (`align="right"`, label at the right edge, caret to its left); where the chevron
    sits (left of the caret, or right of the label, pushing the label off the cells' edge) is a visual call. Build the
    left-of-caret version and hand David a screenshot-ready state.
- **Test plan**: Rust menu builder test (items, check state, ids distinct from the bar's); Vitest for the click column
  choice (requested date, fallback, non-date sort), the pick handler (sorts the event's pane, ignores the checked
  column), the chevron's keydown (opens with an anchor, no click bubbles, focus stays), `aria-expanded`, and the
  pane-chrome walk reaching the chevron; one Playwright cell for right-click → pick → order, only if no unit test
  reaches the native menu round trip.
- **DONE**: gates incl. clippy; `desktop-e2e-playwright` if a cell was added.

### M8: Brief mode and SelectionInfo

- **Scope**: Brief header gets an Added `SortableHeader` after Created (`BriefList.svelte:991-998`), with the fallback
  glyph on the requested one in a fallback; SelectionInfo's date (`SelectionInfo.svelte:207`) and tooltip read
  `displayed-date.ts`; drop the dead `currentDirModifiedAt` prop. Brief rows' accessible name stays name-only, as today
  (`BriefList.svelte:1059-1063` passes no `details`): a Brief row shows no cells, and the date reaches VoiceOver through
  SelectionInfo, whose date line is visible text.
- **Intentions**: Brief shows the same date Full would.
- **Landmines**:
  - Six `flex: 1` buttons (`BriefList.svelte:1196`) in a 25%-wide pane ellipsize, and the glyph must not: give it
    `flex-shrink: 0` beside the ellipsizing `.label`. ❌ Don't judge it from the agent's own render: hand David a
    screenshot-ready state (narrowest pane, 100% and the largest text size, English and German, whose labels run
    longest) and let him pick any fix.
  - Nothing visible in Brief may wait on the width IPC (`views/CLAUDE.md`).
  - SelectionInfo's display mode derives from props (`selection/CLAUDE.md`); pass `effectiveSortBy`, ❌ no mode prop.
- **Test plan**: `SelectionInfo` cells (Added shown under an Added sort, Modified in a fallback with the glyph); header
  button presence and order.
- **DONE**: gates.

### M9: VoiceOver pass and docs

- **Scope**:
  - Header accessible names, each starting with the visible label (WCAG 2.5.3, `SortableHeader.svelte:63-70`):
    - Sorted, no fallback: `columns.sortState` as today, "Added, sorted descending".
    - Not sorted: "Added, not sorted" (today's `other` arm).
    - Fallback: `columns.sortStateFallback` around the state and the date's `sortFallback.*` sentence. Full: "Modified,
      sorted descending (This location doesn’t record …)", the label being the SHOWN date. Brief: the requested button,
      "Added, sorted descending (…)".
    - The chevron: "Choose a date to sort by" (draft), with `aria-haspopup="menu"` so VoiceOver says "menu button".
  - Run the existing a11y suites (`*.a11y.test.ts`, `full-list.a11y.test.ts`, `file-row-a11y.test.ts`) and add cells for
    every name above, plus a `@guidepup/virtual-screen-reader` cell that the fallback toast is spoken once.
  - Listening check for David (VoiceOver): the date header walk, the chevron's menu, and a fallback sort.
  - Docs: `cmdr-local/DETAILS.md` and `CLAUDE.md` (the two listers and how one is picked, the never-drop rule as a
    must-know, supported dates, the unverified TCC/dataless notes; M2 writes the lister part), `cmdr-listing/DETAILS.md`
    (effective sort), `cmdr-fs/DETAILS.md` (the matrix), `views/` and `selection/` C+D.md (the displayed-date rule as a
    must-know), `docs/notes/file-metadata-tiers.md` (Added is no longer deferred; last-use stays deferred, #694),
    `src-tauri/src/menu/DETAILS.md` (sort items, the header menu). Then wipe-readiness: this spec's durable intent lives
    beside the code.
- **Intentions**: nothing essential lives only in a tooltip; the next agent finds every rule beside the code.
- **Landmines**: `claude-md-length` (600-word alarm), `docs-reachable`, `docs-dead-links`; describe current state, ❌ no
  milestone tags.
- **Test plan**: a11y cells; `pnpm check --fast` covers the doc checks.
- **DONE**: gates; an adversarial conformance review against the register below.

## Invariants register

1. The shown date is computed in exactly one place, `displayedDateColumn`, and every surface (Full cell, Full header
   label and width, Brief SelectionInfo, both tooltips' ` (displayed)` marker, Full rows' names) reads it.
2. The requested sort is never mutated by a fallback: pane state, persistence, View > Sort by's checkmarks, command
   state, the header menu's check, and MCP's `sort` show the requested column.
3. Every row ordering in `cmdr-listing` and `sort_search_results` goes through `EffectiveSort::resolve`; the type makes
   a bypass a compile error.
4. `CachedListing` stores requested + supported dates, never a derived effective column.
5. A backend fills a date only when `supported_dates` claims it (conformance-pinned); `supported_dates` defaults to
   Modified only.
6. A local re-stat (`get_single_entry`) yields the same `FileEntry` a listing does, field for field, on either lister
   (M1's parity helper; the one exception is `/.hidden`, § "The code today").
7. A change to any date the effective sort reads re-places the row and emits a diff.
8. The Full date column's measured width covers the shown label, the chevron, the fallback glyph, and the shown cells.
9. Leaving an unsupporting volume restores the requested order with no extra state: nothing was ever changed.
10. A fallback is announced to VoiceOver and visible without hovering.
11. The local lister never drops, invents, or reshapes an entry: a doubtful record re-stats through `entry_from_lstat`,
    a doubtful stream re-lists through `read_dir` from scratch, and bulk runs only on APFS and HFS+.
