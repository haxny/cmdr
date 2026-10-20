# Restic repos as a read-only volume: the plan after the foundation

Tracks [#422](https://github.com/vdavid/cmdr/issues/422). Why: browsing a restic backup on macOS today means
`restic mount` (macFUSE, a kernel extension) or `restic restore --include` with guessed paths. This plan builds
everything after the foundation: the read-only `ResticVolume`, the host route plus overlay row, the unlock flow with
Keychain, copy-out, the walker guarantee, a NAS measurement, and docs.

The issue body is the spec and holds the research (rustic_core's storage trait, dependency cost, detection rule, crawler
hazard). This file doesn't restate it; read the issue first: `gh issue view 422 --json body,comments`.

## Decisions already made (David, on the issue)

- **An overlay row next to the real repo files**, like the git portal's category rows. The repo folder browses like a
  normal folder; the row is extra magic.
- **Snapshot folder names**: `2026-10-12 21.00.03 (a1b2c3d4)`, local time plus the short id.
- **Remember the password in Keychain: on by default.**
- **A `latest` folder** next to the dated snapshots.

## Decisions this plan makes (flag to David at close-out, don't block on them)

- **The row's name is `Browse snapshots`**, the issue's own example. It's a PATH COMPONENT, so it stays English in every
  locale, exactly like git's `branches` / `tags`: history, favorites, MCP, and the agent all address paths, and a
  translated component would change the path when the language changes. `FileEntry` has no display-name field, and
  adding one would touch sorting, quick search, type-to-jump, and rename for one row. Everything else the feature says
  (dialog, errors, progress, tooltip) goes through the i18n catalog, English only (the translator agent fills the rest).
  It sorts first among the repo's lowercase dirs, and it can't clash with the real `snapshots/` dir.
- **No settings toggle** (git has `showVirtualGitPortal`). Restic users opted in by having a repo. Add one later only if
  someone asks.
- **A parked hub owns the heavy state, ❌ not the volume.** `ResticVolume` is thin (hub + repo key + parent volume),
  like `GitPortalVolume` over `GitPortal`. The hub holds passwords, opened repos, and indexes with its own memory cap,
  so the routing LRU evicting a cheap volume never drops a 500 MB index or forgets a password.
- **The password signal stays `VolumeError::NeedsPassword { wrong_attempt }`** with no new "container" field. The
  frontend already classifies a pane's kind from its path (`volume-capabilities.ts`), so it picks the repo wording from
  the same `isVirtualResticPath` it uses for the read-only capability. This keeps the change out of every archive
  mapping site.
- **Unlock verifies before the dialog closes, for a repo.** Archive passwords are stored blind and checked on the next
  read; a repo password is checked by opening the repo (the ~1 s scrypt), with a spinner in the dialog, and a wrong one
  re-arms the dialog in place.

## Loud rules (every milestone)

1. **❌ No `stat` on the hot path.** The route gate is lexical; the overlay decides from the entries the listing already
   has; confirmation is memoized per repo.
2. **Every `rustic_core` call runs on `VolumeHost::runtime().spawn_blocking`**, ❌ never on an async worker.
3. **❌ No English a user reads inside `cmdr-restic`.** Typed errors and typed metadata only; the host words them.
4. **❌ Never match on rustic's error text.** Map from typed error kinds; where one isn't distinguishable by type, say
   so in `crates/cmdr-restic/DETAILS.md` with a test pinning the behavior.
5. **Zero writes to the repo, ever.** The adapter's `WriteBackend` refuses everything, and a recording-adapter test pins
   zero write calls across open, list, index, browse, and read. The rustic cache writes only into the host's cache dir.
6. **Real repo files stay ordinary files**: listable, copyable, deletable, indexable. ❌ Never add a guard on them (the
   git portal's `.git/config` guard once half-deleted repos: `apps/desktop/src-tauri/src/file_system/git/CLAUDE.md`).
7. **No walker except a pane and a selection-driven scan ever enters the virtual view.** 100 snapshots of a 500 GB Mac
   are ~50 TB of logical files.
8. **Shared worktree discipline.** Several agents work in this ONE worktree at once. Commit with explicit paths
   (`git commit <paths>`), ❌ never `git add -A` / `git commit -a`: another agent's half-done files are in the tree.
   Touch only the files your milestone owns (listed per milestone). A generated file
   (`apps/desktop/src/lib/ipc/bindings.ts`, `apps/desktop/src/lib/intl/keys.gen.ts`, `Cargo.lock`) is regenerated and
   committed only by the milestone the plan names as its owner at that moment; ask the lead before regenerating one
   otherwise.
9. Strip milestone tags (`M3`, "Milestone 5") from code, comments, test names, and docs before reporting done
   (`docs/guides/multi-agent-refactors.md`).

## What the foundation provides (assumed; verify against the code before starting)

The foundation agent is building issue steps 1–2 in `crates/cmdr-restic` on `rustic_core` 0.13.0 (the issue researched
0.12.0; re-check every API claim below against 0.13.0's source in `~/.cargo/registry/src/*/rustic_core-0.13.0/`).
Assume, and adapt to what actually landed:

- **Storage adapter**: a `ReadBackend` + refusing `WriteBackend` over `(Arc<dyn Volume>, repo root, runtime handle)`,
  with ranged reads through the parent's `read_range` and a whole-file fallback.
- **Open + unlock**: something like `open_repo(parent, root, host, password) -> Result<OpenRepo, ResticError>` with a
  typed wrong-password variant, mapping to `VolumeError::NeedsPassword { wrong_attempt: true }`.
- **Snapshots**: a listing helper returning snapshot metadata, a `snapshot_dir_name(&snapshot, &tz) -> String` naming
  helper, and a `latest` helper.
- **Detection**: a pure `is_repo_listing(names)`-shaped function (a regular file `config`, dirs `data`, `index`, `keys`,
  `snapshots`; `locks` optional).
- **Fixtures** behind the `testing` feature. If they only build in-memory repos, the volume milestone adds an on-disk
  writer (testing-only), because the app's overlay and walker tests need a repo on a real disk under `LocalPosixVolume`.
- The `index-crate-isolation` entry exists (`scripts/check/checks/index-crate-isolation.go`, `cmdr-restic` already in
  the guard list).

## The shape

```
<repo>/                                  real dir on any volume (local, SMB, SFTP, WebDAV, S3, MTP, ADB)
├── config, data/, index/, keys/, …      real files, parent volume's, editable as always
└── Browse snapshots/                    OVERLAY row in a PANE listing only (no inode)
    ├── latest/                          ROUTED: ResticVolume (root = <repo>/Browse snapshots)
    ├── 2026-10-12 21.00.03 (a1b2c3d4)/
    │   └── Users/veszelovszki/…          that snapshot's tree, from rustic tree nodes
    └── …
```

- **The two seams, no hooks**, mirroring the git portal (`apps/desktop/src-tauri/src/file_system/git/CLAUDE.md`,
  `crates/cmdr-git/DETAILS.md` § "The portal is a routed volume",
  `apps/desktop/src-tauri/src/file_system/volume/DETAILS.md` § "Resolving a path: the two routes"): a ROUTE in
  `VolumeManager::resolve` for `<repo>/Browse snapshots` and below, and a listing OVERLAY that adds the
  `Browse snapshots` row to a pane's listing of `<repo>`. ❌ No volume backend names restic.
- **Leftmost boundary wins** across the three routes. Today git is checked first, then archives
  (`apps/desktop/src-tauri/src/file_system/volume/manager/routing.rs` `VolumeManager::resolve`), because a `.zip` inside
  a git snapshot isn't on disk. Restic adds two new nestings: a `.zip` or a `.git/branches` inside a restic snapshot
  (backed-up files) and a restic repo inside a git snapshot. "Check git first" would route
  `…/Browse snapshots/<snap>/proj/.git/branches/…` to a `GitPortalVolume` over a path that isn't on disk. Rule: the
  route whose boundary component appears EARLIEST in the path wins.
- **Confirmation without a stat on every call.** `cmdr_restic::restic_route(path)` (assumed name) is lexical: the first
  `Browse snapshots` component, and the repo root is its parent. `resolve` then confirms the root is a repo through a
  memo the hub keeps (`confirmed_repo_roots`). The overlay seeds that memo for free whenever it contributes a row. A
  miss costs ONE parent listing of `<repo>` (a typed path, a restored session), never a stat per call. Not a repo →
  plain passthrough to the parent volume, so a real folder called `Browse snapshots` elsewhere keeps working. That's
  stricter than git, whose route answers `NotFound` for a non-repo `.git`; restic can afford it because confirmation is
  memoized.
- **Overlay detection reads the entries.** `ListingOverlay::applies_to(volume, path)` is path-only today
  (`crates/cmdr-listing/src/host/overlays.rs`), which can't tell a repo dir. It gains the listing's entries (milestone
  2). Restic's overlay then answers from the five names, skipping listings with more than a small cap of entries (a repo
  root holds six), so a 100k-entry folder costs one length check.

## Crate API the host milestones need (target; the volume milestone builds it)

Keep the root surface tight; each item needs a caller that exists. Intended root promises:

- `ResticRepos` (the hub): `new(host)`, `confirm_repo(parent, root) -> bool` (memoized),
  `note_repo_root(volume_id, root)` (the overlay's seed), `overlay_rows(volume_id, root) -> Vec<FileEntry>` (the one
  row), and the unlock entry point (milestone 5b).
- `ResticVolume::new(hub, volume_id, parent, repo_root)`.
- `restic_route(path) -> Option<PathBuf>` (repo root, lexical) and `VIRTUAL_ENTRY_NAME`.
- `is_repo_listing(...)` from the foundation.
- `test_fixtures` behind `testing`.

The ceiling in `index-crate-isolation.go` goes up with these, with a `reason` per the allowlist rules in
`scripts/check/checks/DETAILS.md` § "Allowlist consent" (a new crate's first ceiling is ours to set; growing it later
needs David).

## Milestones and parallelism

Cap: about three agents at once (`docs/guides/multi-agent-refactors.md`). Waves:

- **Wave 1, parallel**: 1 (NAS measurement; needs the foundation's open + index), 2 (overlay trait), 3 (unlock seam,
  backend).
- **Wave 2, parallel**: 4 (unlock dialog, frontend; after 3), 5a (the volume; after the foundation).
- **Wave 3, parallel**: 5b (index lifecycle + Keychain; after 5a and 3, ideally with 1's numbers), 6 (host route +
  overlay + guards + walker test; after 2 and 5a).
- **Wave 4, parallel**: 7 (frontend kind, row, progress; after 6, and after 4 has committed its i18n strings), 8
  (copy-out, viewer, end to end; after 6).
- **Wave 5**: 9 (docs audit, full check, close-out).

Generated-file owners by wave: `Cargo.lock` (wave 1: milestone 1's dev-deps; wave 3: 6 adds `cmdr-restic` to the app's
`Cargo.toml`), `bindings.ts` (wave 1–2: 3; wave 3: 5b; wave 4: none expected), `keys.gen.ts` (wave 2: 4; wave 4: 7).

---

### 1. Measure on David's NAS repo

**Scope**: a probe under `crates/cmdr-restic/examples/open_cost.rs` (dev-only, never shipped), its dev-deps in
`crates/cmdr-restic/Cargo.toml` `[dev-dependencies]` only, and `docs/notes/restic/open-cost.md` with the numbers. Owns:
those three paths.

**Intentions**: numbers decide the rest. Measure on `/Volumes/naspi/backups/m3-macbook-restic`:

- unlock (`open` with scrypt) time,
- `get_all_snapshots()` time and count,
- `to_indexed_ids()` AND `to_indexed()`: wall time, peak RSS (`/usr/bin/time -l`), bytes read (a counting wrapper on the
  adapter), index file count and total size,
- a tree walk of one snapshot's top three levels, and one 100 MB file read (throughput),
- a second open with rustic's `cache_dir` set versus `no_cache`, and the cache dir's size afterwards,
- all of it twice: through the OS mount (`LocalPosixVolume` from `cmdr-local`) and through a direct SMB session
  (`cmdr-smb`), since users have both. The direct path is the realistic one for a beta user on a share.
- Baseline: `restic -r <path> snapshots` and `restic stats --mode raw-data` timings from `/opt/homebrew/bin/restic`.

**Password**: the sops store holds `RESTIC_PASSWORD_M3_MACBOOK`; the probe reads it through
`RESTIC_PASSWORD_COMMAND='secret RESTIC_PASSWORD_M3_MACBOOK'`-style indirection, ❌ never as a literal on a command line
or in a log. It's David's backup key: the lead confirms with David before the first run. The macOS mount isn't always
up; the direct SMB route needs the NAS credentials from
`~/Library/CloudStorage/Dropbox/obsidian/agents/tooling/smb2-cli.md` (Keychain internet password for `100.127.48.122`).

**Landmines**:

- The probe opens the repo READ-ONLY through the refusing `WriteBackend`; a write attempt is a bug to report, not to
  work around.
- Run each number at least twice and report both; the first SMB read warms the NAS's own cache.
- Posting the numbers on #422 is visible to others: draft the comment, the lead gets David's go.

**Test plan**: none beyond the probe running clean. It's evidence, not code under test.

**DONE**: `docs/notes/restic/open-cost.md` with every number evidence-anchored (rustic_core 0.13.0, macOS version, NAS
over Tailscale or LAN, date), a recommendation for (a) `to_indexed_ids` for browsing versus `to_indexed` up front, (b)
the hub's opened-repo cap, (c) `cache_dir` versus `no_cache`, (d) an idle-drop timeout for the index; plus a draft #422
comment.

---

### 2. Overlays see the listing's entries

**Scope**: `crates/cmdr-listing/src/host/overlays.rs` (trait), `crates/cmdr-listing/src/pipeline.rs`
(`applicable_overlays`, `decorate`, `prepare_handed_over`), `crates/cmdr-listing/src/testing.rs` (`decorate` helper if
its signature moves), `crates/cmdr-listing/src/host/DETAILS.md` § "Overlays", and the git overlay's signature only
(`apps/desktop/src-tauri/src/file_system/git/overlay.rs`, plus its tests' call sites). Owns those.

**Intentions**: `ListingOverlay::applies_to(volume, path, entries: &[FileEntry])`. Both callers already hold the entries
(`decorate` after enrich, `prepare_handed_over` with the handed-over vector). Git ignores the new argument. The contract
stays "cheap and zero-I/O", now also "O(1) for big listings": a contributor that scans entries caps itself.

**Landmines**:

- `prepare_handed_over` is synchronous and runs on a device backend's thread; a restic repo on MTP/ADB makes it answer
  `None` (re-read) for that one directory, which is correct and cheap. Don't make it async.
- An incremental watcher `Added` (someone creates `config` in a folder) doesn't re-run overlays; only a full re-read
  does. That's acceptable (a repo appears by `restic init`, then the next navigation shows the row); write it down in
  the `DETAILS.md` rather than adding a hook.
- The fresh-listing oracle already declines a decorated listing (`crates/cmdr-listing/src/host/overlays.rs` header);
  keep that true.

**Test plan**: a pipeline cell with a stub overlay that applies only when the entries contain a marker name (applies,
doesn't apply, and the handed-over path returns `None` when it applies). Existing git overlay and walker cells stay
green. Red first: the stub can't compile against the old trait, so write the cell, see it fail, then widen.

**DONE**: trait widened, git unchanged in behavior, docs updated, `pnpm check clippy` + the listing crate's tests green.

---

### 3. One unlock seam for every locked container (backend)

**Scope**: the backend half of generalizing the archive password flow.

- New `cmdr-fs` seam: a small trait (say `PasswordLocked`) with `unlock(password, remember) -> UnlockOutcome` (async)
  and `forget_password()`, reached through a `Volume::as_password_locked(&self) -> Option<&dyn PasswordLocked>` default
  `None`. `UnlockOutcome`: `Stored` (archives: kept, verified on the next read), `Unlocked` (verified), `WrongPassword`.
  Files: a new `crates/cmdr-fs/src/volume/password_locked.rs`, `crates/cmdr-fs/src/volume/mod.rs` (the one trait
  method).
- `ArchiveVolume` implements it (`crates/cmdr-archive/src/volume.rs`), answering `Stored` exactly as `set_password` does
  today.
- `apps/desktop/src-tauri/src/commands/file_system/archive.rs`: `with_archive`'s downcast becomes the seam (❌ never a
  downcast list of concrete types; same reasoning as `Volume::routes_over_a_parent`), and the command returns the typed
  outcome. Rename the command and the module where the name now lies (`set_archive_password` →
  `unlock_locked_container`, or similar), and keep `clear_*` as `forget_*`.
- MCP: `apps/desktop/src-tauri/src/mcp/archive_password.rs` (the prompt mirror gains `container: Archive | ResticRepo`,
  supplied by the frontend like `mode`) and `apps/desktop/src-tauri/src/mcp/executor/archive_password.rs` (the tool
  answers from the typed outcome; a `WrongPassword` is a tool result, not a stored secret).
- Owns: those files, plus `bindings.ts` regeneration for this wave.

**Intentions**: one seam, one dialog, one MCP tool, whatever is locked. The restic implementation comes in 5b; this
milestone only needs a stub implementer in tests.

**Landmines**:

- `unlock` for a repo runs ~1 s of scrypt on `spawn_blocking`. The IPC command must be `async` and not
  `blocking_with_timeout`-wrapped at 2 s (`apps/desktop/src-tauri/src/commands/CLAUDE.md` tiers): give it a budget that
  covers a slow NAS open (measure in milestone 1; assume 30 s) and keep it cancelable from the frontend (a cancel drops
  the result: nothing cached, nothing saved to Keychain).
- The MCP tool name `unlock_archive` is agent-facing API. Renaming it (to `unlock`) is cleaner and Cmdr is in beta, but
  it's a visible change: rename, update the tool description and its tests, and list it in the close-out for David.
- ❌ The password never enters the mirror, a log, a result, or an error (`mcp/archive_password.rs` header).
- `remember` is meaningless for an archive today; the seam takes it anyway so the dialog has one call. Archives ignore
  it (and say so in a doc comment).

**Test plan**: unit cells for the command over a stub `PasswordLocked` volume (each outcome), the archive volume's
`Stored` behavior unchanged (existing `volume_test` cells), and the MCP tool's typed answers.

**DONE**: archives behave byte-identically, the seam exists, bindings regenerated, `pnpm check --fast`,
`svelte-tests-related`, and `clippy` green.

---

### 4. The unlock dialog learns repos (frontend)

**Scope**: `apps/desktop/src/lib/file-explorer/pane/archive-password-flow.svelte.ts`, `dialog-props.ts`,
`listing-loader.ts` (the `archiveNeedsPassword` intercept),
`apps/desktop/src/lib/file-operations/transfer/ArchivePasswordDialog.svelte` (+ its tests),
`apps/desktop/src/lib/error-messages/listing-error-messages.ts`, `apps/desktop/src/lib/tauri-commands/archive.ts`, the
dialog gallery fixture (`apps/desktop/src/lib/dialog-gallery/fixtures/archive-password.ts`), and the English catalog
files the new strings land in. Owns those and `keys.gen.ts` for this wave. Rename files where the name now lies, in one
mechanical commit BEFORE the behavior change, so the diff of the change itself stays readable.

**Intentions**:

- The flow takes a `container: 'archive' | 'resticRepo'`, derived from the path with the same lexical
  `isVirtualResticPath` milestone 7 adds to `volume-capabilities.ts` (put the detector in its own leaf file now,
  `apps/desktop/src/lib/file-explorer/restic/path-detection.ts`, mirroring `../git/path-detection.ts`; milestone 7 then
  only imports it).
- Repo mode: a "Remember in Keychain" checkbox, on by default; submit shows a spinner and disables the field while the
  `unlock` call runs; `Unlocked` closes and retries the listing; `WrongPassword` keeps the dialog open with the
  wrong-attempt line; Cancel during the spinner closes at once and tells the backend to drop the result.
- Archive mode: behavior unchanged (`Stored` → close and retry, as today).
- Copy: a repo title and body (the repo's folder name, not "archive"), new listing-error keys for a locked repo, all in
  `en` with good `@key` descriptions. ❌ No translations (root `AGENTS.md` hard rule).

**Landmines**:

- `supplyStoredPassword` (the MCP path) must keep refusing to re-dispatch a transfer (`archive-password-flow.svelte.ts`
  header). Unchanged for repos.
- The listing error state shows behind the dialog during unlock; make sure its repo wording is right, since it's visible
  for the whole second.
- A session restored into a restic path with no Keychain entry raises the dialog at launch. Acceptable; don't add a
  special case, but make sure Cancel leaves a sane pane (the listing error with an unlock affordance, or the repo dir).

**Test plan**: extend `dialog-state.archive-password.svelte.test.ts` and `ArchivePasswordDialog.svelte.test.ts` (or
their renamed successors): repo browse unlock, wrong then right, cancel during spinner, remember default on, archive
paths unchanged. a11y test for the checkbox and the busy state (`transfer-dialogs.a11y.test.ts`). Red first for the
verify-before-close behavior.

**DONE**: one dialog serves both, gallery shows the repo variant, `pnpm check --fast` + `svelte-tests-related` green.

---

### 5a. `ResticVolume`: snapshots, `latest`, and trees

**Scope**: `crates/cmdr-restic/src/` new modules (`hub.rs`, `volume.rs`, `path.rs`, `tree.rs`, `read_stream.rs`, and
whatever the foundation's layout suggests), `crates/cmdr-restic/Cargo.toml` (after milestone 1's dev-deps landed),
`crates/cmdr-restic/CLAUDE.md` + `DETAILS.md`, and the isolation ceiling. Owns the crate from here on (milestone 1 owns
only `examples/`).

**Intentions**:

- `path.rs`: `restic_route(path)` (lexical) and a classifier `<root>` → snapshots root, `<root>/latest[/…]`,
  `<root>/<name>[/…]`. **Resolve a snapshot folder by the `(shortid)` suffix**, not the full name: the time prefix is
  local time, so a timezone change (David travels) or DST would otherwise orphan every path in history and favorites.
  The listing always shows the current name.
- Short ids: 8 hex chars, extended to the shortest unique prefix when two snapshots collide (assume the foundation's
  naming helper does this; verify and add a cell if not).
- Snapshots root listing: one row per snapshot (Modified = snapshot time; Size = the snapshot summary's processed bytes
  when the snapshot carries a summary, else none), plus `latest` (newest snapshot overall by time; with several hosts in
  one repo that's still the newest overall, note it in `DETAILS.md`). The snapshot list is cached in the hub per opened
  repo and refreshed incrementally on every root listing (rustic's update-snapshots API over the cached set, if 0.13.0
  has one; otherwise a `snapshots/` listing diff), so a manual refresh shows a new backup and `latest` stays stable
  between refreshes (a copy out of `latest` mid-backup must not switch snapshots).
- Tree listing from rustic tree nodes: name, kind (dir, file, symlink; device, fifo, and socket as non-directory rows
  with size 0), size, mtime, mode. Symlinks are rows with `is_symlink` and are ❌ never followed. Odd names pass through
  as stored (NFD, spaces, very long).
- Reads: `supports_streaming` true; `open_read_stream` and `read_range` over `open_file` + `read_file_at` in bounded
  chunks (start at 1 MiB; tune with milestone 1's throughput) on `spawn_blocking`. Mode from the node's metadata.
- Trait answers, mirroring `GitPortalVolume` (`crates/cmdr-git/DETAILS.md` § "The portal is a routed volume"):
  `is_writable` false and every mutation at the trait default (`NotSupported`; override `create_directory_all` the way
  git does), `can_watch_listings` false, `listing_watch_coverage` none, `supports_local_fs_access` false, `local_path`
  none, `paths_are_os_visible` false, `routes_over_a_parent` TRUE, `lane_key` / `get_space_info` / `get_space_info_at`
  delegate to the parent, `metadata_caps` `MODE_REPORTER` (add `XATTRS` only if the node carries them and the
  conformance cell agrees), `scan_for_copy` + batch scan through `cmdr_fs::volume::scan_walk::ScanSource`.
- Index: for this milestone, load lazily and synchronously on first tree access (the lifecycle lands in 5b). Browsing
  may use `to_indexed_ids()` if 0.13.0 confirms tree blobs are fully indexed there; reads need `to_indexed()`.

**Landmines**:

- Restic stores Go `os.FileMode` bits in a node's `mode`. Check what rustic_core 0.13.0 exposes (raw Go mode or unix
  mode); reporting Go type bits as a unix mode puts garbage permissions on copied files.
- A node's mtime can be far in the past or have nanoseconds; map the same way `cmdr-archive` maps entry times.
- Restic snapshots of a Mac start at `/`, so the tree under a snapshot folder is `Users/…`. Show as-is.
- A path that isn't in the snapshot is `NotFound`, ❌ never a corrupt-repo error (git's rule,
  `crates/cmdr-git/CLAUDE.md`).
- Every lookup that touches the repo bails with `VolumeGone`-class errors if the parent volume leaves (a NAS dropping);
  follow `crates/cmdr-git/DETAILS.md` § "A disk that goes away mid-read" for the shape.
- The hub's state is a VALUE the app parks once, ❌ never a static (git's `wiring::portal()` pattern).

**Test plan** (crate-only, `cargo check -p cmdr-restic --all-targets` as the inner loop): path classifier cells
(including a stale time prefix resolving by id, and `latest`), naming and sort order, a tree browse fixture with
symlinks, empty dirs, NFD, spaces, and a 255-byte name, ranged reads through a remote-style `InMemoryVolume` parent
counting `read_range` calls, zero writes across a full browse plus read, `cmdr_fs` read-only `Volume` conformance cells
(the shared suite every backend runs), and `assert_writability_matches_the_mutations_offered`.

**DONE**: `ResticVolume` lists and reads a fixture repo end to end inside the crate, crate docs describe it, clippy
green.

---

### 5b. Index lifecycle, honest progress, memory cap, cache, and Keychain

**Scope**: `crates/cmdr-restic/src/` (hub internals, a progress adapter, the `PasswordLocked` impl),
`crates/cmdr-fs/src/volume/types.rs` (`ListingProgress` gains a typed phase), `crates/cmdr-fs/src/volume/host/mod.rs` (a
`cache_dir(backend)` seam beside `state_dir`), and the app's host installation of that seam (find where `state_root` is
installed). Owns those and `bindings.ts` for this wave.

**Intentions**:

- **Single-flight index load per repo**, owned by the hub. Listings that need it wait on it and receive progress; the
  load is cancelled when its last waiter leaves (the pane navigated away or the user cancelled), through a cancel flag
  the storage adapter checks on every read (a refused read aborts rustic's load; map it to `VolumeError::Cancelled`).
- **Honest progress**: total = the index files' count and summed sizes from `list_with_size(Index)`, done = bytes read
  so far, fed through rustic's progress trait (`ProgressBars` or its 0.13.0 equivalent) into the listing's
  `on_progress`. `ListingProgress` gets a typed phase, for example `phase: Option<ListingPhase>` with
  `LoadingIndex { done_bytes, total_bytes, done_files, total_files }`; the frontend words it (milestone 7).
- **Memory**: an LRU of opened repos in the hub with a cap from milestone 1 (assume 2), and an idle drop of the index
  after N minutes unused (assume 10). Eviction keeps the PASSWORD slot (tiny, `Zeroizing`) but drops the index and the
  rustic repo. Separately, the app's routing LRU for restic volumes (milestone 6) bounds registrations only.
- **Unlock**: `PasswordLocked::unlock` opens the repo (scrypt on `spawn_blocking`), answers `Unlocked` or
  `WrongPassword`, and on success with `remember` saves to Keychain through `host.credentials()` (on a blocking task;
  `CredentialStore` may block). Lookup before prompting: a listing that needs a password first tries the store, and only
  then returns `NeedsPassword`. A stored password that no longer works answers `NeedsPassword { wrong_attempt: true }`.
- **Keychain scope**: service `restic`, scope `<volume id>:<repo path>` per the issue. Note in `DETAILS.md` that the
  same share reached through an OS mount and through a direct session has two volume ids and so two entries; acceptable.
- **Cache**: `cache_dir` from the host (macOS `~/Library/Caches/<app id>/restic`, which follows the dev and worktree
  data separation automatically if it comes from Tauri's per-identifier cache dir; verify, since
  `~/.claude/scripts/remove-worktree.sh` only collects `*-dev-<slug>` dirs), or `no_cache`, per milestone 1. A host
  without a cache dir (tests, tools) means `no_cache`, ❌ never rustic's default location.

**Landmines**:

- rustic_core depends on `backon` (retry with backoff). Find out whether a refused read is retried; a cancel that waits
  out a backoff schedule isn't a cancel. If it retries, return an error kind it doesn't retry, and pin cancel latency in
  a test.
- A listing-level timeout or watchdog (backend or frontend) could cut off a 60 s index load on a slow NAS. Find every
  deadline on the listing path for a routed volume and make sure progress keeps it alive or it doesn't apply.
- A snapshot made after the index loaded has trees in index files the hub hasn't read: a tree id missing from the loaded
  index means reload the index (single-flight again), ❌ not a `NotFound`.
- A concurrent `restic prune` on another machine can delete packs mid-browse; map the read failure to a typed error that
  tells the user to refresh, don't panic or retry forever.
- `CredentialStore` has no delete; a wrong stored password is overwritten on the next successful unlock. A "forget saved
  password" action is a follow-up issue, not this milestone.

**Test plan**: cancel during index load (scripted slow parent volume; the load stops within a bound and the next listing
starts fresh), two concurrent listings share one load, LRU eviction drops the index and keeps the password, a new
snapshot triggers an index reload, Keychain: `InMemoryCredentials` lookup-before-prompt, save on remember, no save on
cancel or on `remember: false`, wrong stored password → `wrong_attempt: true`. Zero writes still pinned.

**DONE**: repeat navigation inside a repo costs no second index load, cancel works, memory is capped, clippy and crate
tests green, bindings regenerated.

---

### 6. Host routing, the overlay row, write guards, and the walker guarantee

**Scope** (app + the shared routing vocabulary):

- `crates/cmdr-fs/src/volume/routed.rs`: `RoutedKind::Restic`. Then every exhaustive match the compiler names
  (`apps/desktop/src-tauri/src/file_viewer/open_with_extract.rs` `OpenWithCopySource` gains `Backup` with its copy in
  the catalog; `apps/desktop/src-tauri/src/file_viewer/materialize.rs` error mapping), minimal and correct; milestone 8
  tests them.
- New `apps/desktop/src-tauri/src/file_system/restic/` (`mod.rs`, `overlay.rs`, `wiring.rs` with the parked
  `ResticRepos`, `walker_exposure_tests.rs`, `CLAUDE.md` + `DETAILS.md`), and the overlay installed beside the git one
  where the listing host builds its overlays.
- New `apps/desktop/src-tauri/src/file_system/volume/manager/restic_routing.rs` plus a `restic_lru` on `VolumeManager`
  (`manager.rs`; cap from milestone 1, assume 4 registrations) sharing `touch_routed_lru`; `manager/routing.rs`:
  leftmost-boundary-wins ordering in `resolve` and `resolve_local_only`, and `path_routes_over_its_parent` covers
  restic.
- Write guards that bypass `VolumeManager`: `apps/desktop/src-tauri/src/commands/file_system/write_ops.rs`
  `reject_if_routed_over_a_parent` and `apps/desktop/src-tauri/src/agent/tools/propose/rename/plan.rs` both name the
  archive and git predicates directly; switch them to `path_routes_over_its_parent` (one predicate, no third copy) or
  add restic beside them. `crates/cmdr-operations/src/routing.rs` compares `== Some(RoutedKind::GitPortal)` for "takes
  no writes" and "can't give up its files"; replace those with methods on `RoutedKind` (`takes_writes()`,
  `can_give_up_files()`) matched exhaustively, so the next routed kind is forced to answer. Move out of a restic source
  is refused before anything is copied, like git's.
- `apps/desktop/src-tauri/Cargo.toml`: depend on `cmdr-restic` (owns `Cargo.lock` for this wave).
- `apps/desktop/src-tauri/src/file_system/volume/DETAILS.md` § "Resolving a path: the two routes" becomes three.

**Intentions**: the git portal's two seams, applied to restic, with the improvements above (confirmed routing,
entry-based overlay, leftmost wins). The overlay applies on ANY non-routed volume (local, SMB, SFTP, WebDAV, S3, MTP,
ADB), unlike git's local-only rule, because the repo is read through `Volume`; it never applies on a routed volume
(`routes_over_a_parent()`), so a repo backed up inside a snapshot or zipped in an archive doesn't nest.

**Landmines**:

- `resolve_local_only` (sync; the fresh-listing oracle's) can't do the one-listing confirm for a remote parent. Route on
  a memo hit; on a miss, confirm a LOCAL parent with `std::fs` and pass a remote one through. Then make sure the oracle
  (`crates/cmdr-listing/src/authoritative.rs`, which guards remote archives the same way) declines a restic path that
  passed through unconfirmed, or a parent volume's watch coverage could vouch for a cached virtual listing.
- The overlay row selected and deleted or renamed in the repo dir is a write on a routed path; the guards above refuse
  it. Check what the git portal does when a category row is part of a multi-selection delete, and match it.
- Selecting `Browse snapshots` and copying it is a legal (enormous) restore through the routed volume; the pre-scan must
  stay cancelable. Don't block it.
- The FE keeps the parent volume id for a routed pane; `resolve_path_volume` / `resolve_location` must answer the parent
  drive for a restic path (see `crates/cmdr-archive/DETAILS.md` § "Routing and lifecycle", "Routing id vs display id").
  Registry id: `restic-{hash(volume id + canonical repo root)}`, backend-internal only.
- `routes_over_a_parent` must be true on `ResticVolume`, or `mount_id_for_path` lets it steal every path under the repo
  (`volume/DETAILS.md`, "A routed volume is not a mount").

**Test plan**:

- Routing cells in `restic_routing.rs` modeled on `git_routing.rs`'s: routes below the row, the repo dir and its real
  files stay on the parent, a non-repo folder named `Browse snapshots` passes through, the memo seeded by the overlay
  skips the confirm listing (count parent listings), leftmost wins for `.zip` and `.git/branches` inside a snapshot and
  for a repo inside a git snapshot, LRU cap.
- `walker_exposure_tests.rs` mirroring the git file's eight cells: a volume listing of the repo carries no row; a
  decorated listing shows it; the delete walker of the repo dir never meets it and deletes the repo completely; real
  files stay mutable; the copy scan of the repo dir counts only disk; the fresh-listing oracle declines a decorated repo
  listing; the drive indexer's walk of a folder holding a repo never records a virtual path; folder-size counting
  (`apps/desktop/src-tauri/src/listing_index_sizes/count/mod.rs`) never descends.
- Guard cells: copy into, move into, move out of, delete, rename, mkdir, bulk rename, and an agent rename plan inside
  the view are each refused with the typed read-only refusal; copy out is allowed.

**DONE**: a pane on a repo dir shows the row; entering it lists snapshots via `ResticVolume`; every guard and walker
cell green; `volume/DETAILS.md` describes three routes; clippy + related tests green.

---

### 7. Frontend: the restic kind, the row, and index progress

**Scope**: `apps/desktop/src/lib/file-explorer/pane/volume-capabilities.ts` (a `restic` kind-from-path, read-only, not
favoritable, mirroring `git-portal`), the `restic/` leaf from milestone 4 (`path-detection.ts` + tests, plus a
`CLAUDE.md` + `DETAILS.md` pair), the row's icon (`docs/guides/icons.md`; a lucide history/archive-restore glyph, David
reviews), the pane's loading state rendering `ListingPhase::LoadingIndex` ("Loading the backup index: 48 of 312 MB", a
percent bar), and English strings. Owns those and `keys.gen.ts` for this wave (start after milestone 4 has committed its
strings).

**Intentions**: the pane treats a restic path as read-only browse with copy-out, exactly like the git portal (F-keys
disabled for writes, paste-into refused, drag-out per the backend's `paths_are_os_visible` answer). The index load is
visible, honest, and cancelable with the pane's usual Escape-cancels-loading behavior.

**Landmines**:

- The FE path detector is lexical and must agree with the Rust one (`restic_route`): first `Browse snapshots` component.
  A real folder of that name outside a repo gets the read-only kind in the FE while the backend passes it through;
  accept it (git has the same edge), and note it.
- Copy wording lives in the catalog only; `cmdr/no-raw-user-facing-string` fails a hardcoded sentence.
- Don't judge the progress bar from screenshots (live UI); hand David a throttled repro for QA.

**Test plan**: vitest for the kind table and path detection (mirroring `git/path-detection.test.ts`), the loading-state
component with a scripted phase sequence, a11y for the progress element. One Playwright spec if a fixture repo can be
dropped into the E2E fixture tree cheaply (browse, unlock, enter a snapshot, copy a file out); otherwise list it for the
close-out.

**DONE**: a restic pane is read-only in the UI, progress shows during an index load, `svelte-tests-related` green.

---

### 8. Copy-out, the viewer, and the agent, end to end

**Scope**: tests and small fixes across `crates/cmdr-operations` (transfer through a routed restic source),
`apps/desktop/src-tauri/src/file_viewer/` (F3 on a restic file materializes through the routed stream), and
`apps/desktop/src-tauri/src/agent/tools/read/inspect/` (`inspect_file` rides the routing, no new kind of content
egress). Owns: new test files there, and the minimal production fixes they force (coordinate with the lead before
touching a file milestone 6 still has open).

**Intentions**: copy a file and a folder out of a snapshot with the normal copy flow: temp+rename landing, mtime kept,
mode kept where the destination stores it (`MetadataCaps`, `crates/cmdr-operations/src/transfer/volume/DETAILS.md` §
"Copies keep the source's metadata"), real sizes in progress and ETA, a cancelable pre-scan. Copying out after the hub
evicted the repo lands on `WriteOperationError::ArchiveNeedsPassword` (transfer mode), and the unlock dialog's repo
variant answers it.

**Landmines**:

- `max_concurrent_ops` for the restic volume: start at 1 like the archive and raise only with milestone 1's throughput
  numbers; concurrent `read_file_at` on one opened repo needs `Repository` to be `Sync` in 0.13.0 (verify).
- Hard links in a snapshot are separate nodes; they copy as separate files. Special files (devices, fifos, sockets) are
  skipped with a typed per-item outcome, ❌ not an abort of the whole copy.
- The viewer's materialization budget and the agent's 5 s per-path deadline can both trip on a first read that needs the
  index; the honest answer is the existing typed "unreachable"/timeout, not a hang.

**Test plan**: copy-out of a file and of a folder with mtimes and modes asserted, move-out refused with nothing written,
cancel during the pre-scan of a whole snapshot, transfer-time password prompt after eviction, viewer materialization of
a restic file, `inspect_file` on a restic text file.

**DONE**: the issue's spec § 7 holds end to end, every listed cell green, `pnpm check clippy` + related tests green.

---

### 9. Close-out

**Scope**: docs audit and the full gate. Owns docs only.

- `crates/cmdr-restic/CLAUDE.md` + `DETAILS.md` complete (boundary, public surface with reasons, the hub, index
  lifecycle, the measured numbers linked from `docs/notes/restic/open-cost.md`, every Decision/Why from this plan that
  still holds), `apps/desktop/src-tauri/src/file_system/restic/` C+D.md, the frontend `restic/` C+D.md.
- `docs/architecture.md` (one entry: what + where + pointer), the root `AGENTS.md` crate list (`cmdr-restic` beside
  `cmdr-git`), `crates/cmdr-listing/src/host/DETAILS.md` (overlays now see entries), `volume/DETAILS.md` (three routes).
- `rg -n '\b(M[0-9][a-z]?|Milestone\s*[0-9]|Phase\s*[0-9])\b'` over every touched path.
- `pnpm check --include-slow`, then the lead's `pnpm land -no-wait`.
- An adversarial conformance review against the invariants below, and a docs-audit agent reading every commit body.
- File follow-ups as issues (`claude-created`): rest-server and other non-browsable backends, watching `snapshots/` for
  new backups, a "forget saved password" action, host/tags/paths on snapshot rows if milestone 5a found no surface for
  them, nested archive browsing inside a snapshot, cache size cleanup.
- List for David: the `Browse snapshots` name, the icon, the MCP tool rename, and the numbers.

**DONE**: everything above, and this plan gets wiped per `docs/specs/DETAILS.md` § "Wiping a shipped spec" once landed.

## Invariants register

1. Browsing never writes to the repo: zero `WriteBackend` calls, no lock files, pinned by a recording-adapter test.
2. No `stat` or listing on the hot path: the route gate is lexical, the overlay reads entries in hand, confirmation is
   memoized. A miss costs one parent listing, once per repo.
3. The `Browse snapshots` row reaches a PANE and nothing else; no volume listing contains it.
4. No walker but a pane and a selection-driven, cancelable scan enters the view (`walker_exposure_tests.rs`).
5. Real repo files are ordinary files: listable, copyable, deletable, indexable, with no guard.
6. Every mutation inside the view is refused with a typed read-only refusal, on every seam (`VolumeManager` and the
   seams that bypass it); move out is refused before anything is copied.
7. The leftmost route boundary wins across git, restic, and archive routes.
8. Every `rustic_core` call runs on the blocking pool; every long one (unlock, index load, file read) is cancelable, and
   a cancelled unlock caches nothing and saves nothing.
9. One index load per repo at a time, shared by its waiters, with progress measured in real bytes.
10. Memory is capped by the hub's LRU and idle drop; the routing LRU bounds registrations only and never drops a
    password or an index by itself.
11. Passwords live only in the hub's `Zeroizing` slots and the Keychain; never in a log, a mirror, an error, or a tool
    result. Keychain saves happen only after a verified unlock with `remember` on.
12. A snapshot folder resolves by its short id; the time prefix is display.
13. The crate names no `tauri` / `cmdr`, words nothing a user reads, and never matches on error text.
14. One dialog, one unlock seam, one MCP tool for every locked container.
