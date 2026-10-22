# Date added and Last opened sorts

**A fresh download with an old mtime doesn't land on top of Downloads, because Cmdr can't sort by Date added.** This
plan adds `Added` and `Opened` sort columns beside `Created`, a single "displayed date" rule, a per-volume
`supported_dates` capability with a requested-vs-effective sort, and (gated on a benchmark) moves the local listing to
`getattrlistbulk`. Tracking issue: [#691](https://github.com/vdavid/cmdr/issues/691), from
[cmdr-reports#79](https://github.com/vdavid/cmdr-reports/issues/79).

## Open questions for David

1. **Last opened's cost.** Finder's "Last opened" lives in the `com.apple.lastuseddate#PS` xattr, which no bulk call
   returns: it's one `getxattr` per entry. If the benchmark note shows that costs more than ~10% of a 100k listing,
   pick: (a) pay it (every entry always carries Opened), or (b) read Opened only for a listing whose effective sort is
   Opened, plus the visible range for tooltips (Opened then shows in a tooltip once the row has been on screen). The
   plan assumes (a) until the note says otherwise.
2. **View > Sort by has no checkmarks today** (`menu_bar.rs:319-326` are plain items). The requirements say the menu
   keeps the REQUESTED column checked. Add radio-style checks to the whole submenu as part of this effort, or leave the
   menu bar unchecked and let only the header's Sort by menu show a check? The plan assumes the latter.

## Loud rules (every executor)

- ❗ **The date the UI shows comes from ONE pure function**, `displayedDateColumn(effectiveSortBy)`. ❌ No second
  derivation in a view, the measurer, SelectionInfo, a tooltip, or the a11y helper. ❌ No "last date sort" memory of any
  kind.
- ❗ **The requested sort is never mutated by a fallback.** Pane state, the persisted `sortBy`, View > Sort by, the
  command state, and MCP's `sort` field keep the requested column. Only the backend's EFFECTIVE column orders rows.
- ❗ **The fallback rule lives in ONE Rust function** (`EffectiveSort::resolve`), and every sort site in `cmdr-listing`
  and `sort_search_results` sorts through it. The frontend never computes "effective"; it reads it off the listing.
- ❗ **A backend fills `created_at` / `added_at` / `opened_at` only when its `supported_dates` claims that date.**
  Pinned by a `volume::conformance` assertion every backend runs.
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
  mirrored by hand in `apps/desktop/src/lib/file-explorer/types.ts:416`, `lib/commands/types.ts:45`, and `bindings.ts`.
  Default orders: `types.ts:422`. Persisted parse: `lib/app-status-store.ts:141`.
- **One comparator**: `entry_comparator` (`sorting.rs:220`) over `SortableEntry` (`sorting.rs:74`), implemented by
  `FileEntry`, the private `Keyed`, and `SearchSortRow` (`src-tauri/src/commands/search.rs:369`, modified only;
  `created_at` answers `None`).
- **Sort sites** that read a column: `streaming.rs:340` (first read), `operations.rs:94` (test seam),
  `operations.rs:524` (resort, which writes `listing.sort_by` at `:525`), `pipeline.rs:65`, `mutate.rs:43,188`,
  `reread.rs:185`, `refresh.rs:202`, `caching.rs:80,121` (the listings handed to refresh). `CachedListing::sort_by`
  (`cached_listing.rs:125`) is one field, so requested and effective are the same thing today.
- **`FileEntry`** (`crates/cmdr-fs/src/entry.rs:118`) already has `added_at`, `opened_at`, and
  `extended_metadata_loaded`, plus an `ExtendedMetadata` struct (`entry.rs:414`). All dead in production:
  - `reading.rs::list_directory` (`:61-100`) is `testing`-gated and is the only filler; `get_extended_metadata_batch`
    (`:410-444`) has no caller (hence the file-wide `dead_code` allow at `:8-11`).
  - `macos_metadata.rs:73-75` reads Opened from `NSURLContentAccessDateKey`, which is atime. ❌ Wrong source.
  - Every backend and FE adapter sets `extended_metadata_loaded: true` and nothing reads it (`file-list-utils.ts:147`,
    `parent-entry.ts:26`, `entries-snapshot.ts:62`, `SearchResultsView.svelte:153`, `selection-info-feed.svelte.ts:254`,
    `listing_host/backends.rs:103`, and the backend crates).
- **The local lister**: `list_directory_core_impl` (`crates/cmdr-local/src/reading.rs:168`) is `read_dir` plus one
  `lstat` per entry (`process_dir_entry`, `:345`), one extra `stat` per symlink, `UF_HIDDEN` from `st_flags`, `/.hidden`
  at a volume root, `inode` only for hardlinked files, `permissions = st_mode` (type bits included), physical size
  `st_blocks * 512`. `get_single_entry` (`:273`) is the single-path twin the watcher's re-stat, `mutation.rs`, and
  `create.rs` use.
- **Change detection ignores every date but mtime**: `diff.rs:247` (`is_entry_modified`) and `mutate.rs:197-199`
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
  `ResortResult`, and `sort_search_results`'s answer.
- **`EffectiveSort` is a type, not a convention.** `entry_comparator` and `sort_entries` take an `EffectiveSort`
  (column + order + dir mode) whose only constructor is `EffectiveSort::resolve(requested, order, dir_mode, supported)`.
  Rule: if `requested` is Created / Added / Opened and `supported` lacks it, the column is Modified; otherwise it's
  `requested`. A sort site that forgets the fallback then doesn't compile.
- **`CachedListing` keeps the requested column plus the volume's `SupportedDates`**, captured from the RESOLVED volume
  at read time (so a routed archive declares its own). `effective()` derives the column; ❌ no stored effective field to
  drift.
- **`supported_dates` is a `Volume` predicate, not a `VolumeCapabilities` field.** The engine is its only reader (the FE
  learns the effective column from the listing), and `capabilities.rs` says unread fields drift. Signature
  `fn supported_dates(&self, path: &Path) -> SupportedDates`, default `SupportedDates::NONE` (Modified only), so a new
  backend can't claim a date it never fills; the conformance assertion catches a backend that fills one it didn't claim.
  The path parameter exists because the root volume can list a directory on another filesystem; backends other than
  local ignore it.
- **Local volumes ask the filesystem**: `getattrlist(ATTR_VOL_ATTRIBUTES)` on the listed directory, checking
  `validattr.commonattr` for `ATTR_CMN_CRTIME` and `ATTR_CMN_ADDEDTIME`, cached per `fsid`. Opened is supported where
  Added is (APFS, HFS+): those keep xattrs natively; on FAT/exFAT an xattr is an AppleDouble `._` sidecar and per-entry
  reads there are too costly. Linux: Created when `statx` birth time is available on the root probe, never Added or
  Opened. `FilesystemKind` is the test oracle, ❌ not the decider.
- **Search results support Modified only.** `SearchSortRow` comes from the index, which stores no creation, added, or
  opened time. A search pane asked for Added falls back like any volume, with the same hint. Extending the index is out
  of scope.
- **The Full header's Sort by menu is a NATIVE popup** (muda), like every row, tab, and breadcrumb menu: OS-native,
  VoiceOver-ready, keyboard-anchored through `MenuAnchor`. Items: Date modified, Date created, Date added, Last opened,
  the requested one checked. A pick on a NEW column sorts by it at its default order; picking the checked one does
  nothing. The pick returns as a typed event carrying the PANE, because a right-click on the unfocused pane's header
  must sort that pane (the `menu-sort` path only knows the focused one).
- **Full date header click** calls `handleSortChange(pane, column)` with `column` = the requested sort when it's a date
  (Modified, Created, Added, Opened), else `modified`. In a fallback that toggles the requested column's order, which is
  also the effective one's, so the click visibly flips the rows and the requested column survives.
- **The fallback is discoverable on three channels**, none of them tooltip-only:
  - The Full date header shows an `info` `StatusGlyph` after its label, and the requested column's Brief header button
    carries the same glyph. Its label (VoiceOver) and tooltip read the fallback copy.
  - A user-initiated sort that lands in a fallback raises an info toast for the pane (`addToastForPane`, the
    `refusesSizeSort` shape) and announces through `FilePaneAPI.announce`. Navigating into an unsupporting volume with a
    date sort already requested stays quiet apart from the glyph: a toast on every folder change is noise.
  - The header's accessible name says it (§ M8).
  - Draft copy (David reviews): "This location doesn’t record when items were added, so Cmdr sorts by date modified
    here." One key per date (`fileExplorer.sortFallback.created|added|opened`).
- **Labels**: column headers `Added`, `Opened` (short, like `Created`); tooltip lines `Created`, `Date added`,
  `Last opened`, `Last modified` (Finder's words); menu items `Date added`, `Last opened`. The `lastMoved` key is
  REPLACED by a new `fileExplorer.dateTooltip.dateAdded` key so every locale re-translates it.
- **Watcher and refresh**: added time changes only when an item lands in a folder, which the watcher already sees as a
  new entry, so a re-stat carries it. Last opened is an xattr write by LaunchServices; whether that reaches Cmdr's
  watcher as a modify is to be VERIFIED (M3), not assumed. Acceptable outcome if it doesn't: a stale Opened until the
  next re-read or navigation, documented with an evidence anchor. ❌ No polling.
- **`..` shows no date**, as today (`createParentEntry` carries none). The dead `currentDirModifiedAt` prop goes.

## Milestones

### M1: characterize the local listing

- **Scope**: tests only, in `crates/cmdr-local/src/reading_test.rs` (and the listing-crate files named below). Pin,
  field for field, what `list_directory_core` and `get_single_entry` produce today on a real scratch dir: a file, a dir,
  an empty file, a hardlinked file (`inode` set) and a plain one (`inode` `None`), a symlink to a dir (`is_directory`,
  `icon_id`), a symlink to a file, a broken symlink (the minimal entry at `reading.rs:202-211`), a `chflags hidden`
  file, `/.hidden` at a volume root and not below one, a non-UTF-8 name, a sparse file and a `ditto --hfsCompression`
  file (`physical_size`), a pre-1970 mtime (`None`), a permission-denied entry, `permissions` including the `S_IFMT`
  bits, owner and group names, `is_package`, `is_archive`. Assert `get_single_entry(p) == listed entry` for every case
  (one helper comparing every field, so a new field joins automatically: compare serialized JSON).
  - Plus a cell in `crates/cmdr-listing/src/sorting_test.rs` pinning Created's `None` arms, and one pinning that
    `update_entry_sorted` (`mutate.rs:186`) re-places a row only on size, mtime, or kind today (it will change in M5;
    mark it as the regression anchor M5 flips).
- **Intentions**: make the `getattrlistbulk` swap provable. Pin reality, not the plan. Every assertion must fail when
  its field is perturbed (try one by hand per category: red first).
- **Landmines**:
  - `process_dir_entry` stats the TARGET only for `is_directory` (`reading.rs:349-353`) but takes everything else from
    the link (`:355-359`). Pin that split, it's easy to "fix" by accident.
  - Owner names come from a process-wide cache (`entry.rs:14`); compare names, ❌ never assume uid strings.
  - A test needing FAT or exFAT uses `testing::disk_images` (macOS), ❌ never its own `hdiutil` (`cmdr-fs/CLAUDE.md`).
    Add one FAT32 or exFAT image cell: it's the fallback path M2 must keep.
  - Gate macOS-only cells with `#[cfg(target_os = "macos")]`; the suite runs on Linux CI too.
- **Test plan**: these ARE the tests. All green on today's code; each verified red against a perturbed field.
- **DONE**: suites green, `pnpm check --fast`, `pnpm check clippy`. Committed alone.

### M2: the bulk lister (GATED on the benchmark note)

- **Gate**: `docs/notes/getattrlistbulk-listing-benchmarks-2026-10.md` must show `getattrlistbulk` (with
  `ATTR_CMN_ADDEDTIME`) at least as fast as `read_dir` + `lstat` at 100k entries, warm and cold, and give the per-entry
  cost of the Opened xattr. Go only if it's surely doable and surely faster. ❗ The note didn't exist when this plan was
  written; the lead folds its numbers in here before spawning M2, and settles open question 1.
- **Scope (go)**: in `crates/cmdr-local/src/reading.rs` (or a new `reading/bulk.rs` beside it, macOS only), a
  `getattrlistbulk` lister producing exactly M1's `FileEntry`s plus `added_at`, and `opened_at` from
  `getxattr(..., XATTR_NOFOLLOW)` of `com.apple.lastuseddate#PS` (a `timespec`: `tv_sec` then `tv_nsec`, little endian;
  verify the layout on disk and anchor it). `get_single_entry` gets the same fields from one `getattrlist` (+ the
  xattr), so a re-stat matches a listing. `list_directory_core_with_tally` keeps feeding the tally per entry.
  - Fallback: on `ENOTSUP` / `EINVAL` from `getattrlistbulk`, or a filesystem whose `ATTR_VOL_ATTRIBUTES` lacks the
    attributes, take today's `read_dir` path. ❌ Every other errno reaches the caller (the `rename_local_exclusive`
    discipline in `cmdr-local/DETAILS.md`).
  - Linux keeps `read_dir` unchanged.
- **Scope (no-go, the alternative)**: keep `read_dir` + `lstat`, and on a filesystem that supports Added, add one
  `getattrlist(ATTR_CMN_ADDEDTIME)` per entry plus the Opened `getxattr`. Two extra syscalls per entry; acceptable only
  if the note's per-entry numbers keep a 100k listing within ~10% of today. Same `get_single_entry` parity.
- **Intentions**: dates present for every local entry at no meaningful cost; byte-identical everything else (M1 green
  without edits, apart from the two new fields).
- **Landmines**:
  - `permissions` is the full `st_mode` (type bits included): rebuild it from `ATTR_CMN_OBJTYPE` +
    `ATTR_CMN_ACCESSMASK`, and M1's cell proves it.
  - `physical_size` is `st_blocks * 512`; `ATTR_FILE_ALLOCSIZE` may disagree on compressed or cloned files. M1's
    compressed-file cell decides; if they differ, keep `st_blocks` semantics.
  - `getattrlistbulk` never follows links: a symlink still needs its one target `stat` for `is_directory`.
  - Per-entry `ATTR_CMN_ERROR` maps to today's minimal entry, ❌ not a failed listing.
  - `ATTR_CMN_ADDEDTIME` may be absent from an entry's returned bitmap (never moved, old volume): `None`, ❌ never 0.
  - Dataless (iCloud evicted) files: request no attribute that materializes them; verify with an evicted file if one is
    at hand, else note it as unverified.
  - The capped public surface (`cmdr-local/DETAILS.md` § "The public surface is capped", `index-crate-isolation`): keep
    new items `pub(crate)`.
  - Every syscall is `unsafe` FFI: each block gets a concrete `// SAFETY:` (`clippy::undocumented_unsafe_blocks`). The
    attribute buffer is variable-length and packed in request order; parse it with bounds checks, ❌ never a transmute.
  - The xattr read on a network mount can block: this lister only runs for local volumes, and `supported_dates` already
    says no Opened on anything but APFS / HFS+.
- **Test plan**: M1 green unchanged. New red-then-green cells: `added_at` is set for a fresh file and changes after a
  `rename` into another folder; `opened_at` reads a hand-written `lastuseddate#PS` xattr; the FAT image takes the
  fallback and yields `None` for both; `get_single_entry` parity holds for both fields. A Criterion or
  `benchmark::log_event` measurement at 100k recorded in the note.
- **DONE**: as M1, plus the measured numbers appended to the benchmark note.

### M3: `SupportedDates` across every volume, and the two-phase leftovers removed

- **Scope**:
  - `cmdr-fs`: `SupportedDates { created, added, opened }` (`specta::Type`, `NONE` const), the `Volume::supported_dates`
    predicate (default `NONE`), and a `volume::conformance` assertion: no listed entry carries a date its volume doesn't
    claim. Wire the assertion into every backend's conformance test.
  - Declare per backend after reading each mapping, ❌ never from memory: local (M2's probe), SMB (`mapping.rs:36,47`
    fills created), MTP (`directory_ops.rs:897`), S3 (`query.rs:256`), WebDAV (`creationdate`, server-dependent: claim
    it, entries may still be `None`), git (`tree.rs`, `log.rs`, `virtual_listing.rs` set created, often = mtime: decide
    and say why), SFTP, NFS, ADB, archive, restic, `InMemoryVolume` (configurable, so tests can play a volume without
    Added), and the app's mock provider. Record the matrix in `crates/cmdr-fs/DETAILS.md`.
  - Remove `ExtendedMetadata`, `extended_metadata_loaded` (Rust and `FileEntry` on the FE),
    `get_extended_metadata_batch`, `macos_metadata.rs`, the `dead_code` allow, the `testing`-only enrichment in
    `list_directory`, and the `check-type-drift.ts` exemption. Update every fixture that set the field
    (`rg extendedMetadataLoaded`, about 40 FE test files and the backend crates listed in § "The code today").
  - Verify the Opened watcher question: open a file through LaunchServices (`open`) inside a watched folder and see
    whether the pane's entry updates. Record the answer with an evidence anchor in `cmdr-listing/DETAILS.md`.
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
- **DONE**: gates incl. clippy; `rg -n 'extended_metadata|extendedMetadata|ExtendedMetadata|macos_metadata'` empty.

### M4: sort columns and effective sort in the backend

- **Scope**:
  - `SortColumn::Added`, `SortColumn::Opened`; `SortableEntry::added_at` / `opened_at` (search rows answer `None`);
    `compare_by_column` arms matching Created's `None` handling; the AlwaysByName comment at `sorting.rs:238-241` names
    the new date columns.
  - `EffectiveSort` + `resolve` (§ Decisions); `entry_comparator` and `sort_entries` take it. Every sort site listed in
    § "The code today" goes through it.
  - `CachedListing` stores `supported_dates` (from the resolved volume, at `streaming.rs:331` and the test seam at
    `operations.rs:88`) and exposes `effective()`. A full re-read that re-resolves a volume refreshes it; if that moves
    the effective column, the listing resorts and the FE hears it (same event as a resort; find the existing re-read
    notification and extend it rather than adding a channel).
  - `ListingCompleteEvent` (`listing_host/events.rs:52`) and `ResortResult` (`operations.rs:451`) gain
    `effective_sort_by: SortColumn`.
  - `is_entry_modified` (`diff.rs:247`) and `sort_relevant_changed` (`mutate.rs:197`) include `created_at`, `added_at`,
    `opened_at`. Make `sort_relevant_changed` ask "did the EFFECTIVE column's key change" (plus kind), so it can't fall
    behind the enum again.
  - `sort_search_results` (`search.rs:424`) resolves with `SupportedDates::NONE` and answers
    `{ order, effectiveSortBy }`.
  - MCP backend: `view.rs:76` and the schema accept `added`, `opened`; regenerate the tools fixture.
  - `pnpm bindings:regen`, and the FE compiles: extend both `SortColumn` unions, `defaultSortOrders` (descending for
    both), `parseSortColumn`, and give the FE call sites a placeholder only where the compiler forces one (M5 owns
    them).
- **Intentions**: rows are always in the effective order everywhere (cache, inserts, diffs, search, MCP), and the
  requested column is what's stored and echoed.
- **Landmines**:
  - `operations.rs:525` writes `listing.sort_by = sort_by`: that must stay the REQUESTED column.
  - `caching.rs:61-80,111-121` and `refresh.rs` hand `(listing_id, sort_by, …)` tuples around: pass the listing's
    requested column and supported dates (or an `EffectiveSort`), ❌ never a pre-resolved column that then gets
    persisted somewhere.
  - `update_entry_sorted` resolves the index BEFORE `entries_mut` (`cmdr-listing/CLAUDE.md`); don't reorder while
    editing the predicate.
  - A sort change bumps the FE's `cacheGeneration` (`cmdr-listing/CLAUDE.md`); an effective-only change must too.
  - Flip M1's `update_entry_sorted` anchor: a `created_at` change under a Created sort now re-places the row.
- **Test plan** (red → green): `resolve` truth table; a listing on an `InMemoryVolume` without Added, requested Added,
  orders by mtime and reports `effective_sort_by = Modified` while `sort_by` stays Added; resort to Name and back;
  `update_entry_sorted` under Added re-places on an `added_at` change; diff reports an `opened_at` change; search sort
  with Added answers Modified's order and says so; MCP `sort` accepts `added`.
- **DONE**: gates incl. clippy.

### M5: frontend plumbing, commands, and menus

- **Scope**:
  - Pane state gains `effectiveSortBy`, swapped in atomically with the rows it describes: from
    `listing-loader.ts::handleListingComplete`, `applyRowResult` (resort), and the snapshot store's sort result. ❌ Not
    persisted. Expose it on `FilePaneAPI` for MCP.
  - Commands `sort.byAdded`, `sort.byOpened`: `COMMAND_IDS`, `command-registry.ts` / `commands/sources/main-window.ts`,
    `sort-handlers.ts`, `SortableHeader.svelte:48` map, `listener-setup.ts:262` map, Rust `SORT_BY_ADDED_ID` /
    `SORT_BY_OPENED_ID` (`command_map.rs`, unmapped like Created, extend the test at `:700-707`), `menu_bar.rs` items
    after Date created (untracked, no shortcut), `menu_handlers.rs:464-475`, `macos_appkit.rs:237` icons, the menu bar
    snapshot tests (`menu_bar_test.rs`), `pnpm intl:native-strings`.
  - MCP: `pane-mcp-sync.svelte.ts` `sortFieldMap` gains the two; pane state adds the effective column when it differs
    (for example `sortFallback: "modified"`), rendered in `panes.rs`; rows gain `created`, `added`, `opened` ISO strings
    when present. `mcp-listeners.ts` `parseSortColumn` accepts them.
  - Fallback toast + announce on a user-initiated sort (§ Decisions), in `sort-operations.ts`'s `install`, comparing the
    result's `effectiveSortBy` with the requested column. Copy keys `fileExplorer.sortFallback.*`.
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
  map, and the MCP state line; Rust menu snapshot and command map tests.
- **DONE**: gates incl. clippy.

### M6: the displayed date in Full mode

- **Scope**:
  - New `apps/desktop/src/lib/file-explorer/displayed-date.ts`: `displayedDateColumn(effectiveSortBy)`,
    `dateOf(entry, column)`, the header label key, the tooltip label key, and the a11y phrase key per date column. Pure,
    fully tested. Add it to `file-explorer/CLAUDE.md`'s map.
  - Full cells (`FullList.svelte:813`), header label (`FullListHeader.svelte:126-135`, still a `SortableHeader`, active
    when the requested sort is a date), and the measurer (`measure-column-widths.ts:324,429`) all read it. The header's
    width includes the label of the SHOWN date, the chevron (M7), and the fallback glyph.
  - New Full date-cell tooltip and the reworked `buildDateTooltip`: every date the entry carries (in the order Created,
    Date added, Last opened, Last modified), the displayed one suffixed by a new i18n'd ` (displayed)` key. Replace
    `lastMoved` with `dateAdded` (§ Decisions).
  - Full rows' accessible name: `details.modified` becomes `details.date: { column, text }` with one aside phrase per
    column (`rowModified` plus new `rowCreated`, `rowAdded`, `rowOpened`) in `ASIDE_SLOTS`.
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
- **DONE**: gates.

### M7: Full header Sort by menu and click semantics

- **Scope**: `show_date_sort_menu(window, pane, requested, anchor)` in `src-tauri/src/commands/menu.rs` with its builder
  in `src-tauri/src/menu/` (check items, requested checked, `menu.sort.*` labels incl. new `dateAdded` / `lastOpened`),
  a typed `DateSortMenuPick { pane, column }` event from `on_menu_event`, and the FE listener calling the pick semantics
  (§ Decisions). Right-click anywhere on the Full header opens it at the pointer; a chevron button inside the date
  header opens it anchored under itself (Enter / Space), and it's part of the pane-chrome toolbar walk. Header click
  semantics (§ Decisions).
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
- **Test plan**: Rust menu builder test (items, check state); Vitest for the click column choice (requested date,
  fallback, non-date sort) and the pick handler; one Playwright cell for right-click → pick → order, only if no unit
  test reaches the native menu round trip.
- **DONE**: gates incl. clippy; `desktop-e2e-playwright` if a cell was added.

### M8: Brief mode and SelectionInfo

- **Scope**: Brief header gets Added and Opened `SortableHeader`s after Created (`BriefList.svelte:991-998`), with the
  fallback glyph on the requested one in a fallback; SelectionInfo's date (`SelectionInfo.svelte:207`) and tooltip read
  `displayed-date.ts`; drop the dead `currentDirModifiedAt` prop; Brief rows' accessible name gains the displayed date
  (the same `details.date`, after the name).
- **Intentions**: Brief shows and announces the same date Full would.
- **Landmines**:
  - Seven `flex: 1` buttons in a 25%-wide pane ellipsize. Check the narrowest pane at 100% and the largest text size; if
    labels become unreadable, propose a fix to David (screenshot) rather than shrinking type.
  - Nothing visible in Brief may wait on the width IPC (`views/CLAUDE.md`).
  - SelectionInfo's display mode derives from props (`selection/CLAUDE.md`); pass `effectiveSortBy`, ❌ no mode prop.
- **Test plan**: `SelectionInfo` cells (Added shown under an Added sort, Modified in a fallback with the glyph);
  `BriefList.a11y.test.ts` row names; header button presence and order.
- **DONE**: gates.

### M9: VoiceOver pass and docs

- **Scope**:
  - Header accessible names: the Full date header says the shown date and its sort state ("Added, sorted descending");
    in a fallback, the requested column's name says it's sorted by date modified here (extend `columns.sortState` or add
    a fallback variant). The chevron's name ("Choose a date to sort by", draft). Run the existing a11y suites
    (`*.a11y.test.ts`) and add cells for the fallback names.
  - Docs: `cmdr-local/DETAILS.md` (the lister, supported dates, the xattr), `cmdr-listing/DETAILS.md` (effective sort,
    the Opened watcher finding), `cmdr-fs/DETAILS.md` (the matrix), `views/` and `selection/` C+D.md (the displayed-date
    rule as a must-know), `docs/notes/file-metadata-tiers.md` (Added and Opened are no longer deferred),
    `src-tauri/src/menu/DETAILS.md` (sort items, the header menu). Then wipe-readiness: this spec's durable intent lives
    beside the code.
- **Intentions**: nothing essential lives only in a tooltip; the next agent finds every rule beside the code.
- **Landmines**: `claude-md-length` (600-word alarm), `docs-reachable`, `docs-dead-links`; describe current state, ❌ no
  milestone tags.
- **Test plan**: a11y cells; `pnpm check --fast` covers the doc checks.
- **DONE**: gates; an adversarial conformance review against the register below.

## Invariants register

1. The shown date is computed in exactly one place, `displayedDateColumn`, and every surface (Full cell, Full header
   label and width, Brief SelectionInfo, both tooltips' ` (displayed)` marker, both views' row names) reads it.
2. The requested sort is never mutated by a fallback: pane state, persistence, View > Sort by, command state, the header
   menu's check, and MCP's `sort` show the requested column.
3. Every row ordering in `cmdr-listing` and `sort_search_results` goes through `EffectiveSort::resolve`; the type makes
   a bypass a compile error.
4. `CachedListing` stores requested + supported dates, never a derived effective column.
5. A backend fills a date only when `supported_dates` claims it (conformance-pinned); `supported_dates` defaults to
   Modified only.
6. A local re-stat (`get_single_entry`) yields the same `FileEntry` a listing does, field for field (M1's parity
   helper).
7. A change to any date the effective sort reads re-places the row and emits a diff.
8. The Full date column's measured width covers the shown label, the chevron, the fallback glyph, and the shown cells.
9. Leaving an unsupporting volume restores the requested order with no extra state: nothing was ever changed.
10. A fallback is announced to VoiceOver and visible without hovering.
