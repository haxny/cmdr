# Ask Cmdr: volume-aware reads, honest misses, one share as one share, and connecting

Implementation plan for sections 1 and 2 of #749 (minus `propose_server`, which the suggested-ops redesign plans). Loop:
`docs/guides/multi-agent-refactors.md`. One executor agent per milestone, sequential; each lands green on its own.

## The headline acceptance test

A transcript-replay fixture test, built up as each layer lands and green at the end: the volume facts of prod
conversation 179, as fixtures.

- The Tailscale share `smb-100-127-48-122-445-naspi-…` is saved, not connected, last root `/Volumes/naspi`, with an
  on-disk drive index holding `/papers/Rymdskottkärra AB/invoices paid` and its children.
- The LAN share `smb-192-168-1-111-445-naspi-…` is live at `/Volumes/naspi-1`, with NO drive index, its filesystem
  holding the same folder (a temp dir standing in for the mount).
- Importance: the Tailscale share's store (`importance-smb-100-127-48-122-…db`) holds `invoices paid`, scored from
  visits; the LAN share's store holds no row for it. So the row the model sees can only reach the LAN share by
  re-addressing, which is what the test proves.
- Both share rows record one server GUID, one share name, and one username.

The story it replays: `important_folders` returns `invoices paid` as a location on the LAN share (`via` the Tailscale
row), and `list_dir` on that location returns the folder's children from a live read (`source: live`). Each layer adds
its slice: M2 the resolver and the miss for the transcript's bare guess, M3 the live read on the unindexed LAN share, M7
the offline answer with the LAN share gone, M8 the re-addressing that ties them together.

## Loud rules

- **One location shape.** A tool INPUT takes the flat pair `path` + optional `volumeId` (no nested object: nothing
  breaks for AI clients, and the schema prefix stays small). A tool OUTPUT carries `location`, the wire shape of
  `crate::location::Location`, `{ "volumeId": "...", "path": "..." }`, so a result's two fields are a valid next input.
  `path` is always the APP path (the same string the panes, `search` rows, `inspect_file`, and `propose_suggestions`
  use). ❌ Never a second pair type, ❌ never a share-relative path on the wire.
- **A miss is an `Ok` result carrying a typed token**, the `memory_write` refusal pattern (`agent/tools/DETAILS.md` §
  The two tools that write): `view::dispatch` flattens a `ToolError` to `{ problem }`, which a model can't branch on. ❌
  Never an empty listing, and ❌ never `authoritative: true` beside "not in the index".
- **Reuse the shipped cores**: `resolve_location`'s protocol dispatch, `volume_listing::complete`, `index_read_path`,
  `connect_saved_place`, search's read-only index load, `inspect_file`'s runner. Move a core DOWN when `agent/` can't
  reach it (`commands/` is above `agent/`); ❌ never copy it.
- **Schema text is prefix.** Every milestone that touches a schema or description re-pins `FIXED_PROMPT_OVERHEAD_TOKENS`
  (`agent/chat/budget.rs`, 6,263 today, pinned by `agent/chat/context/cost_tests.rs`) and updates the number in
  `agent/chat/DETAILS.md`. Keep descriptions terse; the contract lives in `DETAILS.md`, not in the schema.
- **`list_dir` and `search` are shared with AI clients**, and favorites leaving `volumes` changes `cmdr://state`.
  Regenerate `mcp/tests/fixtures/tools_list_snapshot.json` and any resource snapshot; ❌ never hand-edit one.
- **Characterize, then convert.** M1 pins today's two lying outputs red-first before anything is reshaped.
- **Strip milestone tags** from code, comments, test names, and docs before landing
  (`rg -n '\b(M[0-9][a-z]?|Milestone\s*[0-9]|Phase\s*[0-9])\b'`).
- **Foreground commands only**, in every executor.
- **Hands off concurrent efforts**: the importance-quality agent on this branch, the suggested-ops work (owns
  `propose_suggestions`), and the `worktree-ask-cmdr-tool-transparency` session (`ask-cmdr-labels.ts`). See §
  Coordination.

## What the code does today (fresh read, 2026-10-26)

Evidence: prod `main.db` conversation 179 (read-only `sqlite3`), the prod data dir, plus the code below.

- `list_dir` (`agent/tools/read/listing.rs::execute_list_dir`) takes only `path` and routes by path
  (`index().list_children` → `list_dir_children` → `volume_id_for_local_path`). `/Volumes/naspi/papers` isn't a live
  mount, so it routed to `root`, which has no row there, and `coverage(enabled, freshness, indexed: false)` still set
  `authoritative` from root's freshness: the model read `{"authoritative": true, "indexStatus": "fresh"}` beside "This
  folder isn't in the drive index" three times.
- `list_volumes` (`read/volumes.rs`, over `mcp/resources/volumes.rs::snapshot_volumes_from`) gave the Tailscale share
  `connectionState: saved`, `mountPath: /Volumes/naspi` (its LAST mount path, from `known-shares.json`), and
  `indexStatus: fresh`. The fresh is root's: `snapshot_volumes_from` resolves status by path for every row whose path
  starts with `/`, and the dead mount path routes to `root`. That row is what sent the model to `/Volumes/naspi/...`.
- The live LAN share (`/Volumes/naspi-1`, `direct`) reads `indexStatus: off`, and it truly has NO drive index: the prod
  `drive-index/` dir holds only `index-root.db` and `index-smb-100-127-48-122-…db`. Its importance DB (58,350 folders)
  exists without one. So every index-only read dead-ends on the share the user actually has connected.
- Favorites ride `volumes` as `kind: local` (`kind_for_location` has no favorite arm). "Internal storage" is a favorite
  (`favorites.json`, path `mtp://pixel/Internal storage`): the Pixel's MTP storage, saved as a favorite, so it has no
  mount path and no index. `cmdr://state` already renders favorites in their own `favorites:` section from the same
  listing, so they appear twice there too.
- `important_folders` / `folder_importance` (`read/importance.rs`) emit the importance store's own path space:
  volume-RELATIVE (`/papers/...` on SMB, `/Shared` on `cloud-google-drive`), plus a volume id no other tool accepts.
  `snapshot_path` looks a bare path up in EVERY scored volume, so `/papers` can match whichever volume has it.
- One NAS share has at least three ids: `smb-100-127-48-122-…` (Tailscale), `smb-192-168-1-111-…` (LAN),
  `smb-naspolya-smb-tcp-local-4-…` (Bonjour, an importance DB with no listing row). `known-shares.json` holds a share
  row for each of the first two. The SMB2 server GUID is parsed (`smb2::NegotiatedParams::server_guid`, shown in
  `commands/smb_diagnostics.rs`) and stored nowhere. Why the index isn't KEYED on it: `search/DETAILS.md` § Why the
  index isn't keyed on a stable server identity (still true; this plan reconciles after the fact, as that section
  suggests).
- Search already reads an unmounted volume's index: `search/volumes.rs::load_volume_blocking` opens `index-{id}.db`
  read-only, mount root from the DB's `volume_path` meta (`usable_mount_root`). The drive index's own read path
  (`get_read_pool_for`) has no pool for a stopped volume (`crates/cmdr-index/DETAILS.md` § A stop retires too).
- Connecting a saved place: `commands/servers.rs::connect_saved_place(volume_id, attempt_id, secret, username)` covers
  SMB (`smb_saved_shares::connect_saved_share`), SFTP, WebDAV, S3, and NFS, answering `ServerConnectOutcome` or
  `SavedPlaceRefusal::{NoSuchServer, AlreadyConnected}`. The frontend's `connect-flow.ts` is its one caller and picks
  one of three arms by the volume's standing. The ai-client tools `connect_to_server` and `upgrade_smb_to_direct` are
  `Access::Write` and neither connects a saved place.
- Access gate: `agent/tools/view.rs::access_is_dispatchable` (`Read | Propose | Memory`), allowlists in
  `mcp/tests/tool_registry_tests/access.rs`, `Access` in `mcp/tool_registry/mod.rs`. The chat turn awaits a tool call
  with no cancel check (`chat/runtime/turn.rs`), and `chat/runtime/dispatch.rs` takes no token.
- `propose_suggestions` already takes `{volumeId, path}` objects (`destination`, `selector.root`) with app paths: the
  precedent for the output shape.

## Decisions

1. **Locations are app paths; input is flat `path` + optional `volumeId`; output always carries `location`.** App path,
   because it's what every other tool the model holds speaks and what the user sees; a share-relative path would make
   the model translate before every `inspect_file` or move. `volumeId` optional on input, because a model holding
   `~/Downloads` or `/Volumes/naspi-1/papers` should simply work. Translation to each store's own path space
   (index-relative, importance-relative) happens inside the tool, at one seam.
2. **A not-connected volume's locations live under its last root.** The app already treats a saved share place's path as
   its last mount path (`docs/specs/saved-smb-shares.md` § Pins and the switcher). The resolver checks `path` against
   the NAMED volume's root. When a named volume's root moved since the model got the location (a remount landed on
   `/Volumes/naspi-2`), the resolver rebases the path onto the new root and answers with `rebasedFrom: <old path>`; a
   path under neither a known earlier root nor the current one is `pathOutsideVolume`. "Known earlier roots" are the
   saved row's last mount path from before this mount plus every root the volume held this session, kept in memory.
3. **A bare path resolves to a LIVE volume or to nothing.** Live volumes (the protocol dispatch, deepest root wins),
   then `root` for a path the boot disk really holds. A bare path under a dead mount point (`/Volumes/naspi/papers` with
   nothing mounted there) is `noVolumeAtPath`; only an explicit `volumeId` reaches a not-connected volume. ❌ No "saved
   last root" tier: two saved shares can share one last root, and a reused mount point would resolve to the wrong one.
4. **A named not-connected volume RESOLVES; whether it answers is the tool's call.** The resolver returns
   `Ok(ResolvedLocation { live: false, … })` for it, and a tool emits `volumeNotConnected` only when none of its sources
   (decision 8) can answer. The resolver's own misses are `unknownVolume`, `pathOutsideVolume`, and `noVolumeAtPath`.
   **Every not-connected miss carries `nearby` and `sameShareAs`, and each `nearby` entry carries a ready
   `candidate: { volumeId, path }`** with the requested suffix already joined onto that volume's root
   (`/Volumes/naspi/papers` → `{ smb-192-…, /Volumes/naspi-1/papers }`), so the next call is a copy, not arithmetic.
   `nearby`: volumes whose root's last segment shares the requested `/Volumes/<name>` stem, else every remote volume,
   capped at 8; each `{ volumeId, name, kind, connectionState, root, candidate }`. A proven same-user live sibling
   (decision 8) re-addresses the read with `via` instead of missing at all.
5. **`list_volumes` rows carry `root` instead of `mountPath`**: the app-path prefix every location on that volume starts
   with, present for scheme roots too (`sftp://…`, `mtp://…`), and the last root for a not-connected one (its
   `connectionState` says so). `mountPath` existed to address `search`, and `search` gains `volumeId`. `cmdr://state`'s
   YAML keeps `mountPath` (ai-client contract).
6. **Favorites leave `volumes` in BOTH `list_volumes` and `cmdr://state`.** `cmdr://state` already has a `favorites:`
   section, so listing them twice is a bug for AI clients too. Recorded in `mcp/DETAILS.md`; the rest of `cmdr://state`
   stays byte-identical. `list_volumes` gains `favorites: [{ name, path, location? }]`: the raw `path` always, and a
   `location` only when it resolves to a containing volume. An unreachable one (the phone favorite) keeps its raw path,
   has no `location`, and carries `reachable: false`.
7. **`sameShareAs` is proof-based**: two share rows are the same share when their recorded server GUIDs match and their
   share names match case-insensitively. An all-zero GUID is NO GUID (`smb2` fills `Guid::ZERO` before a negotiate
   lands). Fallback for rows saved before the GUID was recorded: a background, unauthenticated NEGOTIATE-only probe of
   the saved address, run when two or more share rows share a share name and any lacks a GUID, bounded (3 s), ❌ never
   inside a tool call. The probe only FILLS a missing GUID and never overwrites one, so a hotel network answering on the
   saved LAN IP can't poison it; a negotiate inside an authenticated session does overwrite, as the latest truth. ❌ No
   fuzzy "possibly the same" field.
8. **Preference is per read: live index → live readdir → fresher offline index.** Across a same-share group, a read
   addressed to one member is answered by the first member (and source) that can answer it in that order, and says so
   (`via: { volumeId }`, `source`). Redirection across members needs both rows to record the SAME username: another
   account can see different files on the same share. Without it, the read answers for the requested volume and
   `sameShareAs` stays a hint.
9. **`list_dir` answers from where the folder actually is**, marked by `source`:
   - `index`: the volume's live drive index lists the folder (today's path).
   - `live`: the volume is live but has no drive index, or its index doesn't list this folder. ONE bounded readdir
     through the volume's provider, on `inspect_file`'s runner pattern (a dedicated blocking thread, a deadline,
     abandoned past it, the turn's cancel). Folder sizes are unknown here (never a zero), and the result says so. Its
     own typed misses: `needsDiskAccess`, `unreachable`, `notFound`, `excluded`.
   - `offlineIndex`: the volume isn't connected and its on-disk index lists the folder, read-only,
     `authoritative: false` (the D7 promise, "answering about unmounted volumes").
10. **`connect_volume` is `Access::Session`, on turns the user started.** It brings a SAVED place to life by volume id
    with stored credentials only, following the volume's real standing the way `connect-flow.ts`'s three arms do. The
    gate reads a TYPED turn origin (`agent/chat/DETAILS.md` names four: `text`, `wake`, `outcomes`, `resume`): `Session`
    dispatches on `text` and `resume` and is refused, typed, on `wake` and `outcomes`, because a background connect can
    spin up a sleeping NAS's disks for a digest nobody asked for (principle 2). Typed, so it holds if someone later
    stops wrapping follow-ups. AI-client exposure follows parity: if `select_volume` already brings a saved place to
    life for them, `connect_volume` stays agent-only, else it's `[AiClient, Agent]`.
11. **What "changes no files or config" means for `Session`**: no file under the user's control, no server, account,
    credential, pin, or setting is added or edited. The saved place's own connection bookkeeping (last connected time,
    last mount path, volume id refresh) updates exactly as a user's Enter on the row updates it, and the OS creates the
    mount point. A Keychain dialog on a user-started turn is acceptable; a missing credential raises nothing.
12. **One root rule.** A volume's root is one function's answer: the live mount (or scheme) root when registered, else
    the index DB's `volume_path` meta, else the saved row's last mount path. `list_volumes`, the resolver, the
    importance translation, and `search`'s offline reader all call it, so two tools never disagree about where a
    volume's paths start.
13. **Volume facts come from in-memory sources** (the volume manager's live set, the known-shares and server stores,
    index meta, the kernel's cached mount table via `volume_listing::network_mount_under`'s `MNT_NOWAIT` read). The full
    `volume_listing::complete` listing (up to 2 s on a wedged mount) is taken only to build a miss's `nearby`.
14. **One offline vocabulary.** A not-connected volume with an on-disk index reads the same in `list_volumes` and in
    `list_dir`'s coverage: `indexStatus: stale` plus `asOf` (the last completed scan, spoken), with `connectionState`
    saying it isn't connected. No `offline` flag, no new token: `stale` already means "indexed earlier, may have
    drifted", which is exactly the truth.
15. **Which member `connect_volume` dials.** A member whose same-username sibling is live answers `alreadyConnected`
    with that sibling's `location` and `via`, no dial. When no member is live, it dials the same-username member most
    recently connected (`known-shares.json`'s `lastConnectedAt`), whichever member the model named, and answers with
    `via` when that's a different one. Recency is the one signal Cmdr has for "the address that works from here"; it
    doesn't sniff networks. QA step 3 expects the LAN member because it's David's most recent connect at home.

## Milestones

### M1: Pin the two lies, then fix them

**Scope**

- `read/listing.rs::coverage`: `authoritative` is `indexed && freshness.is_authoritative()`.
- `mcp/resources/volumes.rs::snapshot_volumes_from`: a row that carries a `connection_state` resolves its index status
  by volume id (`index().volume_status(&loc.id)`), never by its path. A local disk keeps path routing (the external
  drive case that comment documents).

**Intentions.** The model trusted two booleans that lied. Fixing them first makes every later milestone's honest-miss
copy land on outputs that are already true.

**Landmines**

- `coverage` may have other callers (`codegraph_callers coverage`); the fix applies to all, which is the point, but read
  each.
- A saved share's status by id reads `off` (no registered index) even though an on-disk index exists. That's honest
  until M4 switches it to decision 14's `stale` + `asOf`; don't invent a token here.
- `cmdr://state` renders the same snapshot: its `indexStatus` for saved rows changes too. Check `mcp/resources/` tests
  and any E2E that reads `cmdr://state` volumes.

**Test plan.** Red first: a `build_list_dir` test with `indexed: false` on a fresh volume asserting
`authoritative: false`; a `snapshot_volumes_from` test (macOS-gated, like `a_saved_server_reaches_the_state_resource`)
with a saved share whose recorded path isn't mounted, asserting it doesn't read `fresh`. Watch both fail, then fix.

**DONE.** Both tests red then green; `pnpm check --fast`, `pnpm check clippy`, touched nextest green; no schema change.

### M2: The location core, and `list_dir` speaks it

**Scope**

- A resolver core, pure over injected volume facts: `LocationInput { volume_id: Option<String>, path: String }` →
  `Result<ResolvedLocation, LocationMiss>`. `ResolvedLocation` carries the `Location` plus what tools need (kind, root,
  live or not, which index sources exist, `rebased_from`, `via`). A named not-connected volume resolves `Ok` with
  `live: false` (decision 4). `LocationMiss` variants: `UnknownVolume`, `PathOutsideVolume { root }`,
  `NoVolumeAtPath { nearby }`. A shared helper builds the tool-level `volumeNotConnected` miss (`nearby`, `sameShareAs`)
  for whichever tool runs out of sources. Tilde expansion moves in here (one place). Decisions 2, 3, 4, and 12.
  `sameShareAs` stays empty until M8 fills it.
- A facts shell per decision 13: in-memory sources first, the full listing only when building `nearby`. It owns the
  earlier-roots recorder (decision 2): it listens to mount events (volume registered, root moved) and keeps every root
  each volume held this session, seeded at startup with each saved row's last mount path.
- The one root function (decision 12), here, so every later milestone calls it.
- `list_dir`: input `path` + optional `volumeId`; result `path` becomes `location`; misses are
  `{ miss: <token>, location, detail, nearby?, sameShareAs? }`. Index reads go through on-volume variants
  (`get_dir_stats_on_volume` exists; add `list_dir_children_on_volume` and an `Index` handle method) so the resolved
  volume, not a second path routing, picks the index. `source: index` on every answer; a resolved not-connected volume
  answers `volumeNotConnected` until M7 gives it an offline source (the only source change later milestones make).
- The acceptance test's first slice: the transcript's bare `/Volumes/naspi/papers` answers `noVolumeAtPath` with a
  `candidate` on the LAN share.
- Docs: `agent/tools/DETAILS.md` gains § The location contract (shapes, resolution order, misses, `candidate`);
  `tools/CLAUDE.md` gets one must-know line pointing there; `docs/architecture.md` gets the resolver core's map line;
  decision D5's state line in `agent/DETAILS.md` (the `Location` vocabulary is now built).

**Intentions.** One resolver every tool shares, so "where is this" is answered once and a miss reads the same from every
tool. The pure core is where the tests live; the facts shell is thin.

**Landmines**

- `resolve_location`'s dispatch (`commands/volumes.rs::resolve_path_to_volume`) lives in `commands/`, ABOVE `agent/`.
  Move the core down (beside `crate::location`, or into `volume_listing`) and leave the command a pass-through.
  `location.rs` may become `location/mod.rs` + `location/resolve.rs`.
- Firmlinks: `index_read_path` callers normalize (`firmlinks::normalize_path`) first; the on-volume list variant must
  too.
- MTP and ADB app paths are scheme paths; `index_read_path_pure` already strips them. Don't special-case.
- `ListDirResult` and its schema are shared with AI clients; regenerate `tools_list_snapshot.json`.

**Test plan.** Pure resolver table tests: live SMB mount path → that volume; the transcript's bare
`/Volumes/naspi/papers` → `NoVolumeAtPath` whose `nearby` holds both shares, the LAN one with `candidate`
`/Volumes/naspi-1/papers`; the same path WITH the Tailscale `volumeId` → `Ok` with `live: false`, on the Tailscale
volume; a named live volume whose root moved from `/Volumes/naspi-1` to `/Volumes/naspi-2` (recorded by the facts
shell's mount-event listener) → resolved with `rebasedFrom`; a path under neither root → `PathOutsideVolume`;
`~/Downloads` → `root`; an unknown id → `UnknownVolume`; a reused mount point (a saved share's last root now a live
mount of a DIFFERENT volume) → the bare path resolves to the live one, and the saved share's `volumeId` with that path
resolves `live: false` on the saved share, never the live one. `list_dir` shaper tests: the `volumeNotConnected` miss
JSON carries `nearby` and `sameShareAs`.

**DONE.** Resolver + `list_dir` wired, the acceptance test's first slice green, overhead re-pinned, snapshot
regenerated, docs updated, all gates green.

### M3: `list_dir` reads live when the index can't

**Scope**

- Decision 9's `live` source: when the resolved volume is live and either has no drive index or its index doesn't list
  the folder, ONE readdir through the volume's provider (`VolumeManager` → `Volume::list_directory`), on a dedicated
  blocking thread with a deadline (`inspect_file`'s `runner.rs` pattern; reuse its config shape), abandoned past it, and
  raced against the turn's cancel (below).
- Rows: names, kinds, file sizes and mtimes from the listing; folder sizes unknown (`SizeClaim::unknown`, no `"0 B"`);
  paged and cut by `fit_to_result_budget` like the index path. `coverage` carries `source: live`, the folder's own
  recursive total absent, and a note that folder sizes need an index.
- **Typed outcomes** (decision 9): `needsDiskAccess` (the FDA gate refused a TCC-protected target), `unreachable` (the
  deadline passed, or another live read on this volume is stuck), `notFound`, `excluded`. Documented in
  `tools/DETAILS.md` § The location contract.
- **Exclusions before reading.** Apply `search/excludes.rs`'s system/cache tier and the user's `excludeDirNames` to the
  target before the readdir, and answer `excluded` for a target inside one, so a live read never shows the model a
  folder the index deliberately left out (which keeps the "no new egress" claim true).
- **The canonical target is what gets gated.** Canonicalize the target (on the runner thread, under the deadline: a
  symlink can point into `~/Documents` or at a hung mount) and run the FDA gate and the exclusions against the canonical
  path. A target whose canonical path lands on a different volume than the resolved one is refused (`notFound`, with the
  detail naming the other volume), never followed.
- **Hostile folders.** A fixed cap of 10,000 entries read; rows sorted within the cap, `truncated: true` and a
  lower-bound `total` when it bites; an `offset` past the cap is refused with a detail telling the model to narrow (a
  subfolder, `search`). One in-flight live read per volume: while one is stuck, the next call on that volume answers
  `unreachable` at once, so abandoned threads can't pile up behind a hung mount.
- **Cancel.** This is the first place a tool call can block for long, so the cancel token reaches the live read here:
  thread the turn's token as far as this handler needs (or race the call in `turn.rs`), and M9a generalizes the same
  seam to every dispatch rather than adding a second one.
- `tools/CLAUDE.md`'s "handlers never touch a live filesystem, except `inspect_file`" must-know names both handlers and
  why: `list_dir` reads live only where no index can answer, once per call, on the runner's bounds.
- The acceptance test's second slice: `list_dir` with the LAN share's `volumeId` and
  `/Volumes/naspi-1/papers/…/invoices paid` returns its children from a temp-dir stand-in, `source: live`.

**Intentions.** The share the user has connected is always listable, indexed or not.

**Landmines**

- **TCC.** A live readdir of a TCC-protected path (`~/Downloads`, `~/Documents`) before the FDA decision stacks macOS
  prompts (`src-tauri/CLAUDE.md`). Gate the canonical target first; refused, answer `needsDiskAccess` rather than read.
- **Hung mounts.** A network readdir can block for minutes; the runner abandons the thread rather than wait, the same
  posture as `inspect_file` (`agent/tools/DETAILS.md` § The runner). ❌ Never `std::fs` on the async thread. The
  per-volume in-flight slot is released only when the abandoned thread actually returns.
- The 10,000 cap stops the READ, not just the page: use the provider's streaming listing if it has one, so a
  200,000-entry folder never materializes whole.
- No egress change: names and metadata already ride every turn, and the exclusions keep the live read inside what the
  index would have shown.

**Test plan.** Shaper tests for the live coverage block and unknown folder sizes; a runner test with an injected slow
lister proving the deadline answers `unreachable` and abandons; a second call while the first is stuck answers
`unreachable` at once; a cancel test (an injected slow lister, the turn cancelled mid-read: the call answers `cancelled`
and the slot frees when the thread returns); the FDA-gate test answering `needsDiskAccess`; a symlink into a protected
folder and a symlink onto another volume, both refused without reading; an excluded target (`node_modules`, a user
`excludeDirNames` entry) answering `excluded`; a 10,001-entry folder answering `truncated` and an offset past the cap
refused; the acceptance slice.

**DONE.** Gates green, overhead re-pinned if the description moved, `tools/CLAUDE.md` + `DETAILS.md` updated.

### M4: `list_volumes` reshaped, and the read-only index opener

**Scope**

- The read-only index opener in `cmdr-index`, reused by M7: open `index-{id}.db` read-only with no live instance, read
  its meta (`volume_path`, the last completed scan), and later answer path queries. Move `search`'s offline load
  (`search/volumes.rs::load_volume_blocking`'s non-root arm, `usable_mount_root`) onto it and onto the one root function
  if that's a clean swap; if not, say why in the commit and note it in `search/DETAILS.md`.
- Rows: `mountPath` → `root` (decision 5, through the root function). A not-connected row with an on-disk drive index
  reads decision 14's `indexStatus: stale` + `asOf` (spoken, from the opener's meta read, without registering the
  volume); M1's `off` stays for one with none. Rows of one same-share group sit together, most recently connected first
  (the grouping arrives with M8; the ordering rule lands here for the rows `known-shares.json` already relates by share
  name).
- `VolumeSummary` feeds both `cmdr://state` and `list_volumes`: make the per-consumer output explicit (the YAML builder
  keeps `mountPath` and its current fields; the agent mapper emits `root`, `asOf`, and later `sameShareAs`), so a field
  added for one consumer can't silently reach the other.
- Favorites out of `snapshot_volumes_from` for both consumers (decision 6), recorded in `mcp/DETAILS.md`; `list_volumes`
  gains `favorites: [{ name, path, location? }]` (decision 6), resolved through M2's core.
- Registry description updated: no more "mountPath is the path to put in search's scope".

**Intentions.** `list_volumes` becomes the map a model plans from: real volumes, each with the root its locations start
with, and how live and how fresh it is.

**Landmines**

- `app_state` embeds the same `to_volume_snapshots` mapper; its output changes too (intended).
- `cmdr://state` tests that count `volumes:` entries change when favorites leave; the `favorites:` section must still
  list all of them, including the phone one.
- The opener must not open a read pool or stand up an index instance (the "registered is not indexed" trap in
  `VolumeIndexStatus`), and must not create, migrate, or rebuild a DB (`SQLITE_OPEN_READ_ONLY`, no schema check that
  deletes on mismatch). It lives in `cmdr-index` with no `tauri` (`index-crate-isolation`).
- Retired connections (`crates/cmdr-index/DETAILS.md` § A stop retires too): the opener hands out a per-call connection
  that closes with the call; ❌ never one cached per thread.

**Test plan.** Opener tests in `cmdr-index`: reads `volume_path` and the last scan from a stopped volume's DB, and
refuses (typed) a missing or newer-schema DB without touching it. Mapper tests: a saved share row has `root` and no
`mountPath`; a saved share with an on-disk index reads `stale` + `asOf`; a favorite never appears in `volumes`; the
phone favorite appears in `favorites` with its raw `path`, no `location`, and `reachable: false`; an SFTP row has a
scheme `root`; the `cmdr://state` YAML is byte-identical apart from the favorites leaving.

**DONE.** Gates green, overhead re-pinned, snapshots regenerated, `read/volumes.rs` docs, `tools/DETAILS.md` catalog
line, and `mcp/DETAILS.md` updated.

### M5: Importance tools on locations

Wait for the importance-quality agent's `important_folders` changes to land on this branch first.

**Scope**

- `folder_importance`: input `path` + optional `volumeId`; the resolver picks ONE volume, the app path is translated to
  that volume's importance path space (strip the root), and only that volume's store is read. ❌ No more scan of every
  volume.
- `important_folders`: `volume` → `volumeId`; every row emits `location` (importance-relative path joined onto the
  volume's root by the root function). `under` (added by the other agent) takes `path` + the call's `volumeId`.
- A scored volume with no listing row (the Bonjour-id importance DB) can't be placed: its rows are left out of the
  all-volumes ranking and the overview names it with `unplaced: true`, ❌ never a row with an invented root.

**Intentions.** An importance row is something the model can hand straight to `list_dir` or `propose_suggestions`.

**Landmines**

- `cloud-google-drive` importance paths are relative to the drive's root while its drive index is `root`'s: the
  translation uses the importance volume's root, not the index's routing.
- `record_visit` stores `location.path` (an app path) while scoring stores index-relative paths; check whether visits on
  a non-root volume are translated before they're written. If not, file a bug; don't fix it here.
- The other agent's row shape (signals, collapse) is the base; add `location`, don't restructure.

**Test plan.** Pure shaper tests: an SMB row `/papers/x` on a volume rooted at `/Volumes/naspi-1` emits
`/Volumes/naspi-1/papers/x`; an unplaced volume yields no rows; `folder_importance` on volume A never reads volume B
(two stores holding the same relative path).

**DONE.** Gates green, overhead re-pinned, docs updated.

### M6: `search`, `inspect_file`, and the pane listing on the same resolver

**Scope**

- `search`: optional `volumeId` (no scope ⇒ that volume's root; with a scope, every include path must resolve to it).
  Scope paths resolve through M2's core, so a bare scope under a dead mount is a typed miss in the result (beside
  `unresolvedScopes`, which keeps meaning "no row for this path"), ❌ not a walk of nothing. A `volumeId` naming a
  not-connected volume answers from its on-disk index as search does today, through the root function.
- `inspect_file`: keeps `paths: string[]` (a location's path half; 200 objects would be a large call for no gain), each
  row echoes `volumeId`, and a path under a dead mount point is a new row status `noVolumeAtPath` (the resolver's
  `nearby` once per call, not per row) instead of `missing`.
- `list_pane_files`: its parent echoes as a `location`.

**Intentions.** Every tool that can be handed a place answers a wrong or unreachable one the same way.

**Landmines**

- `search` is shared with AI clients and its coverage contract is subtle (`mcp/executor/DETAILS.md` § The search
  result); add, don't reinterpret. `resolve_target` keeps refusing a scope that spans volumes.
- `inspect_file` resolves owners via `VolumeManager::mount_id_for_path` already; use the resolver only to classify the
  rows that would otherwise be `missing`, so the hot path pays nothing when every path exists.

**Test plan.** `search` with `volumeId` and no scope covers that volume; a not-connected `volumeId` answers from the
on-disk index; a bare scope under a dead mount answers `noVolumeAtPath`; an `inspect_file` row under a dead mount reads
`noVolumeAtPath`.

**DONE.** Gates green, snapshot regenerated, overhead re-pinned, `mcp/executor/DETAILS.md` and `tools/DETAILS.md`
updated.

### M7: Offline index reads for `list_dir`

**Scope**

- M4's opener grows path queries for a volume with no live instance: resolve the index-relative path, list children,
  read dir stats. Exposed on the `Index` handle; the root comes from the root function.
- `list_dir` on a resolved not-connected volume whose on-disk index lists the folder answers `source: offlineIndex`:
  `coverage` carries `indexStatus: stale`, `authoritative: false`, `asOf`, and a note to connect for a live answer.
- The acceptance test's third slice: with the LAN share gone, `list_dir` on the Tailscale `volumeId` returns
  `invoices paid`'s children from the on-disk index, `source: offlineIndex`.

**Intentions.** The NAS is answerable while it's asleep, honestly dated. The model can list destination folders before
deciding whether a connect is even needed.

**Landmines**

- The opener's rules from M4 hold (per-call connection, read-only, no rebuild).
- A volume that comes live mid-call: read-only WAL readers don't race the lifecycle's writer.
- Walked roots and space don't exist offline; `VolumeBlock` space comes from the poller's cache only, as today.

**Test plan.** A `cmdr-index` test that builds an index DB, stops the volume, and lists through the offline reader
(including a not-listed directory answering `None`); a `build_list_dir` shaper test for the offline coverage block; the
acceptance slice.

**DONE.** Gates green; `crates/cmdr-index` docs (`indexing/DETAILS.md`), `tools/DETAILS.md` § The honesty contract, and
decision D7's state line in `agent/DETAILS.md` updated.

### M8: One share, one share: server identity, `sameShareAs`, and preference

**Scope**

- Record the SMB2 server GUID at every authenticated direct-session negotiate (find the seam in `crates/cmdr-smb` where
  the session comes up; `NegotiatedParams::server_guid`), into the share's `known-shares.json` row (`serverGuid`,
  `serde(default)`, overwriting: the latest authenticated truth) and an in-memory map for live volumes with no row. An
  all-zero GUID is never written.
- The fallback probe (decision 7): `smb2`'s `Connection::connect(addr, timeout)` + `negotiate()` + `params()`, no
  session setup, no credentials. Runs once in the background when the condition holds (after startup's listing settles,
  and when a share row is added), bounded, logged at debug, writing only into a row with no GUID.
- `list_volumes` rows gain `sameShareAs: [volumeId]` (absent when empty); the resolver fills misses' `sameShareAs`.
- The resolver applies decision 8 per read: a read on any group member is answered by the first member and source in
  `live index → live readdir → fresher offline index` order, with `via` and `source`, only across members recording the
  same username. `important_folders` re-addresses a member's rows onto the preferred member's root and dedupes by
  location, keeping the higher score; the overview marks members with `sameShareAs`.
- The acceptance test's final slice: `important_folders` returns `invoices paid` on the LAN share `via` the Tailscale
  row, and `list_dir` on it returns the children, `source: live`. The full story green.
- Docs: `saved-smb-shares.md` § The store gains `serverGuid` and its two writers; `crates/cmdr-smb` docs name the GUID
  seam; `search/DETAILS.md` § Why the index isn't keyed… gains one line pointing at the reconciliation that now exists.

**Intentions.** The model never again picks the 220-folder index over the live share, and never has to know there were
two.

**Landmines**

- The GUID identifies the BOX, not the share: pair it with the share name, always.
- The probe dials a saved address: no credentials, ❌ never from a tool handler. A Tailscale address that's down costs
  one bounded timeout in the background.
- A NAS restore or replacement can change the GUID: an authenticated negotiate overwrites the stored one, the probe
  never does, and `sameShareAs` follows the stored values only.
- **The Docker Samba fixtures may share one GUID.** Samba keeps its server GUID in `secrets.tdb`, so containers built
  from one image that bakes that file in would all answer the same GUID (unverified; check `test/smb-servers/` first).
  If they do, a test that needs two distinct servers asserts the GUIDs differ before relying on it, and the
  `test/smb-servers/` README notes it.
- `known-shares.json` has several writers (`saved-smb-shares.md` § The store); the GUID write is an upsert on the
  existing row's key and touches no other field.

**Test plan.** Pure grouping tests (GUID + case-insensitive share name; same GUID different share ⇒ not grouped; no GUID
or an all-zero one ⇒ not grouped; same share, different usernames ⇒ grouped but never redirected); a probe that finds a
row already holding a GUID writes nothing; per-read preference tests over all three sources; an `important_folders`
re-address + dedupe test; a probe test against the Docker SMB fixtures asserting the GUID comes back with no
credentials; the full acceptance test.

**DONE.** Gates green (plus the SMB integration test), overhead re-pinned if schema text moved, docs above updated.

### M9a: `Access::Session`, typed origins, and cancellation through dispatch

**Scope**

- `Access::Session` in `mcp/tool_registry/mod.rs`, documented like `Memory` (a deliberate widening, hand-allowlisted).
- `EXPECTED_SESSION_TOOL_NAMES` in `access.rs`, EMPTY here, mirroring the Propose and Memory lists, plus the test that
  every `Session` entry is listed there. ❌ No `connect_volume` entry, stub, `ToolId`, or label in this milestone: a
  tool in the view that can't do what its description says is a lie the model acts on. The gate and allowlist are tested
  with a test-only `Session` entry (`#[cfg(test)]`, never in the shipped view).
- `view.rs`: `access_is_dispatchable` admits `Session`; the per-variant test grows the arm. A TYPED turn origin (`text`
  / `wake` / `outcomes` / `resume`) is threaded to the gate: `Session` dispatches on `text` and `resume`; `wake` and
  `outcomes` get a typed `refusedInBackground`.
- Cancel through dispatch for EVERY tool call: generalize the seam M3 opened for the live read (one seam, not a second),
  so Stop drops any in-flight tool call and records `cancelled`.
- Invariant 7 rewritten in `agent/DETAILS.md`: "The agent proposes; only the user approves. Approval originates in the
  frontend as a user action, and there is no tool that approves. Its writes are its own memory folder, jailed and
  hand-allowlisted, and, on a turn the user started, opening a session to a place the user saved, with credentials the
  user stored (`Access::Session`, hand-allowlisted), which adds or edits no file, server, account, credential, pin, or
  setting. Because the user started that turn, a system dialog the connect raises (a Keychain access prompt) is
  acceptable; a missing credential answers `needsCredentials` and raises nothing." Plus principle 3's exception list,
  `agent/CLAUDE.md` and `tools/CLAUDE.md` must-knows, `mcp/CLAUDE.md`'s "[agent] entries are Read, Propose, or Memory"
  line, `docs/architecture.md` (the `Session` tier), and `docs/security.md` (check how it describes the agent's write
  surface and add the line). `refuse_unavailable`'s copy waits for M9b: it must never claim a power the view lacks.

**Intentions.** The policy lands and is proven before any dial exists: the riskiest change to the agent's promise
reviewed on its own.

**Landmines**

- Origins: read `agent/chat/DETAILS.md` § turn origins before threading; follow-ups are wrapped today, and the gate must
  not depend on that.
- Cancellation must not leave a tool result missing: a cancelled call still records a typed `cancelled` result, so the
  thread's call/result pairing stays whole (`chat/DETAILS.md`'s persistence rules).
- No content egress; no `CLOUD_AI_CONSENT_VERSION` bump (invariant 8). Say so in the commit.

**Test plan.** Gate tests per origin with the test-only `Session` entry; an unlisted `Session` entry fails the allowlist
test; the shipped agent view holds no `Session` tool; a cancelled turn records `cancelled` for an in-flight call of any
tool (injected slow handler), and M3's live-read cancel test still passes through the generalized seam.

**DONE.** Gates green, no schema change in the shipped view (so no overhead move), every doc above updated, the
invariant text reviewed by the lead before landing.

### M9b: `connect_volume`

**Scope**

- The tool enters the view: registry entry (`access: Session`), `EXPECTED_SESSION_TOOL_NAMES = ["connect_volume"]`,
  `ToolId::ConnectVolume` with its `KNOWN` entry and wire name (`agent/llm/types.rs`), `EXPECTED_AGENT_TOOL_NAMES`, a
  rail label and its two catalog keys (`askCmdr.tool.connectVolume.doing` / `.done`, English only with `@key`
  descriptions; the translator agent does the rest), and `refuse_unavailable`'s copy now saying Ask Cmdr can connect a
  saved server.
- AI-client parity: confirm whether `select_volume` (`mcp/tool_registry/table.rs`) on a saved place already connects it.
  Yes ⇒ `consumers: [Agent]`; no ⇒ `[AiClient, Agent]` and the snapshot moves. Record the answer in `mcp/DETAILS.md`.
- **Same-share groups first** (decision 15): a member whose same-username sibling is live answers `alreadyConnected`
  with the sibling's `location` and `via`, no dial; with no member live, the dial target is the most recently connected
  same-username member, answered with `via` when it isn't the one named.
- `connect_volume { volumeId }` then follows the chosen volume's real standing, mirroring `connect-flow.ts`'s three arms
  so the agent can't take a move the frontend refuses to (re-dialing a registered volume registers a SECOND one, and
  dialing one the backoff loop owns races it):
  - registered and live → `alreadyConnected` with `location` (the root), no dial.
  - registered and `disconnected` → no dial: emit the event that makes the frontend subscribe its `smbReconnectManager`
    to that volume (connect-flow's arm 1), then await the volume's state for up to about 10 s, cancellable by the turn.
    Answers `connected` (with `location`) or `stillReconnecting`, whose `detail` tells the model to tell the user it's
    still reconnecting, ❌ never to poll. Decision: recovery keeps its one owner in the frontend, triggered by event;
    moving the backoff loop into the backend is a separate refactor this plan doesn't take.
  - registered and `needs_sign_in` → `needsCredentials`, no dial.
  - not registered (a `saved` row, never mounted this session) → the only arm that dials, through
    `connect_saved_place`'s core (moved below `commands/` if `agent/` can't reach it) with `secret: None`,
    `username: None`. Answers
    `{ outcome: connected | alreadyConnected | stillReconnecting | needsCredentials | needsApproval | rejected | unreachable | notSaved | refusedInBackground | cancelled, location?, detail }`.
    `connected` echoes the volume's root as a `location` (it may have moved: `naspi-2`). `needsApproval` covers host
    keys and certificates: the user answers those in the app.
- **Backend single-flight per place id** for connecting a saved place, in the moved core so the frontend's dial and the
  agent's share it: a second caller awaits the first's outcome instead of dialing. Today the volume registers only after
  the mount lands (`smb_saved_shares.rs`'s connect path), so `AlreadyConnected` can't guard the window between an Enter
  and a rail call.
- The dial's own attempt is cancelled when the call is dropped (M9a's token), so Stop ends a 30 s dial and answers
  `cancelled`.

**Intentions.** "The NAS isn't connected" stops being a dead end, without giving the agent any power to add a host, type
a password, or approve a key, which is what an injected filename would want.

**Landmines**

- **Prompts.** A Keychain access dialog on a turn the user started is acceptable. A MISSING credential answers
  `needsCredentials` with no dialog of any kind (QA step 4). The SMB mount already sets NoUI on every mount
  (`network/mount.rs`), so NetFS raises nothing.
- `attempt_id` is the frontend's cancel handle; generate one, and make sure nothing waits on a frontend cancel that will
  never come.
- The reconnect event must reach the same handler connect-flow's arm 1 uses, so the banner, the switcher dot, and the
  backoff agree; ❌ never a second reconnect loop. With no frontend window listening (a closed main window), the await
  times out into `stillReconnecting`, which is honest.
- `ServerConnectOutcome` grows over time: map it exhaustively (no `_ =>`), so a new variant is a compile error here.
- The rail label file is also being reworked by the tool-transparency session (§ Coordination).

**Test plan.** Same-share tests: a member with a live same-username sibling answers `alreadyConnected` `via` it and
never dials; with none live, two saved members dial the more recently connected one; a different-username sibling is
never picked. A standing test per arm (live, `disconnected`, `needs_sign_in`, saved), asserting only the saved arm dials
and the `disconnected` arm emits the event once. The reconnect await: an injected state flip inside the window answers
`connected`, none answers `stillReconnecting`, a cancel answers `cancelled`. Outcome mapping over every
`ServerConnectOutcome` variant. Single-flight: two concurrent connects of one saved place produce one dial and the same
outcome. An SMB Docker-fixture test on a saved share with a stored password (`connected`) and without one
(`needsCredentials`, no dialog). The labels test covers the new tool.

**DONE.** Gates green, overhead re-pinned, snapshot regenerated if shared, `network/` and `servers/` docs updated for
the moved core and single-flight.

### M10: Close-out

**Scope.** Adversarial conformance review against the invariants footer and the agent invariants register; a docs-audit
agent over every commit body and touched `CLAUDE.md` / `DETAILS.md`; `pnpm check` then `pnpm check --only-slow`, both
backgrounded; the QA script for David.

**QA script for David** (prod-like data, the NAS on LAN and Tailscale both known):

1. With only the LAN share connected, ask "what's in my invoices paid folder on the NAS?": Ask Cmdr lists it from
   `/Volumes/naspi-1/...`, read live.
2. Disconnect everything and ask again: it answers from the offline index with a date, and offers to connect.
3. At home, say "connect it": the rail shows the connect label, the switcher shows the LAN share (`naspi on naspolya`,
   the most recently connected member) live, and the listing follows, read live.
4. Remove the Keychain password for the share and repeat step 3: it says it needs your password, and nothing pops up.

## Invariants footer

1. Every location on the wire is an app path; inputs take `path` + optional `volumeId`, outputs carry `location`, and
   every result echoes the locations it answered for.
2. A place that resolves to nothing, or to something unreachable, is a typed miss with `nearby` (each with a ready
   `candidate`) and `sameShareAs`; no tool returns an empty listing or `authoritative: true` for ground it didn't read.
3. Every `list_dir` answer names its `source` (`index` / `live` / `offlineIndex`), and a read answered from anywhere but
   the requested volume's live index says so (`via`, `asOf`).
4. A handler touches a live filesystem only in `inspect_file` and `list_dir`'s live source: bounded, abandoned on
   deadline, cancelled by Stop, FDA-gated and exclusion-filtered on the canonical target, capped at 10,000 entries, one
   in flight per volume.
5. `sameShareAs` is only ever proven (non-zero GUID + share name); redirection also needs a matching username; the probe
   never overwrites a stored GUID.
6. A bare path never reaches a not-connected volume; only an explicit `volumeId` does. Every tool reads a volume's root
   from the one root function.
7. No tool handler dials a network host except `connect_volume`, and it dials only a saved, unregistered place with
   stored credentials, on a `text` or `resume` turn, single-flighted with the frontend's own dial, and cancelled by
   Stop. A missing credential raises no dialog.
8. Agent invariant 7 and its allowlists are the only place `Access::Session` is granted; every `Session` tool is named
   in `EXPECTED_SESSION_TOOL_NAMES`.
9. No new content egress, so no consent bump; every schema change re-pins the fixed prompt overhead.

## Coordination

- **Importance-quality agent (this branch)**: M5 starts after its `important_folders` changes land, and adds `location`
  to its row shape rather than restructuring it; its `under` filter takes `path` + the call's `volumeId`. M1–M4 don't
  touch its files.
- **Suggested-ops work (after #747 lands; owns `propose_suggestions`)**: this plan doesn't touch `propose_suggestions`.
  The seam it takes: M2's resolver to validate `destination` and `selector.root`, answering the same typed miss
  (`volumeNotConnected` with `nearby` and `candidate`) so a not-connected destination refuses whole. `propose_server`
  stays `Propose` and does NOT use M9a's `Session` precedent: a direct server write would let an injected filename add
  an attacker host and harvest the next password (#749 § 1).
- **Tool-transparency session (`worktree-ask-cmdr-tool-transparency`)**: M9b adds one entry to `ask-cmdr-labels.ts` and
  two catalog keys. Whichever lands second rebases onto the other's label shape.
- **#749 § 5 (system prompt)**: tool descriptions carry the not-connected, live, and offline wording; any prompt line
  ("say when a destination's volume isn't connected; offer `connect_volume`") belongs to the § 5 effort, which owns
  `system.md`.

## Settled in review

- App-path locations, flat input, `location` on output (decisions 1 and 4).
- Flagged redirection to a PROVEN sibling with the same username, per read across sources (decision 8).
- `connect_volume` on user-started turns only, with the bookkeeping carve-out (decisions 10 and 11), split into a policy
  milestone and a dial milestone.
- Favorites leave `volumes` for AI clients too (decision 6).
