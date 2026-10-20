# Restic volume: adversarial conformance review

Review of the committed restic work (#422) against `docs/specs/restic-volume-plan.md` § "Loud rules" and § "Invariants
register". Scope: `git diff main...HEAD` at `8c13cab9ca` (crate `cmdr-restic`, the unlock seam and dialog, routing,
overlay, guards, walkers, index lifecycle, Keychain). Uncommitted work (frontend kind and progress, copy-out e2e) was
out of scope. Method: read every path named below; rustic facts checked against `rustic_core-0.13.0` and the `smb2`
source. 2026-10-20.

Summary: one high, four medium, eight low. No path to a write against the repo was found (the adapter refuses every
`WriteBackend` call, and every write seam that knows git now knows restic). No password reaches a log, a mirror, an
error, or a tool result. No error-string matching in production code.

## High

### 1. A cancelled or timed-out repo unlock can still keep the password and save it to the Keychain

- **Where**: `crates/cmdr-restic/src/volume.rs:522-531` (`PasswordLocked::unlock`),
  `crates/cmdr-restic/src/hub.rs:242-249` (`ResticRepos::unlock`), `crates/cmdr-fs/src/volume/secret_store.rs:111-127`
  (`keep_signed_in_secret`), `apps/desktop/src-tauri/src/commands/file_system/unlock.rs:169-183` (the cancel and budget
  `select!`).
- **Invariants**: 8 ("a cancelled unlock caches nothing and saves nothing") and 11 (Keychain saves only after a verified
  unlock the person still wants). The `PasswordLocked` contract says cancel-safe by drop; `unlock.rs`'s header relies on
  it.
- **Failure**: the host cancels by DROPPING the future. Two await points sit after the scrypt open has returned:
  1. `hub.rs:245-247` commits the password into `slot.password`, THEN awaits `slot.opened.lock()`. That tokio mutex is
     held by any concurrent `opened()` across its own scrypt plus parent reads (a listing that started with a stale
     stored password, a stall retry), so the unlock can sit there for seconds with the password already kept. A cancel
     then drops it with the password in memory.
  2. `volume.rs:530` awaits `keep_password`, which runs the Keychain write on `spawn_blocking`. Dropping the await
     doesn't stop the blocking task: the password lands in the Keychain after the dialog said "cancelled". A first
     Keychain write can wait on a macOS access prompt, so the 30 s `UNLOCK_BUDGET` can expire here too and report
     `TimedOut` ("nothing was kept") while the save completes. The frontend's follow-up `forget_container_password`
     (`unlock-flow.svelte.ts:333-337`) clears only the in-memory copy; the next listing asks the store (miss memo was
     cleared by the unlock) and silently unlocks from the entry the person cancelled.
- **Fix**: make the commit and the save non-droppable as one unit. For example, run open, commit, and save inside one
  spawned task that re-checks the cancel token before each commit step (commit only if not cancelled; skip the save if
  cancelled), and have `unlock_within` wait for that task's verdict rather than dropping it. At minimum, set the
  password and the opened repo in one step with no await between, and check the token before `keep_password`.
- **Confidence**: high for the Keychain-after-cancel path (the `spawn_blocking` is plain in `secret_store.rs`); high for
  the lock-wait window.
- **Status**: Fixed in `50f01c2ff3`. `PasswordLocked::unlock` no longer saves: it commits the password (and the opened
  repo, without waiting on another open's lock) with no await before the answer. The Keychain write moved to a new
  `remember_password`, which `unlock_within` calls only after an `Unlocked` nobody cancelled, awaited to the end outside
  the cancel and the budget; a Cancel landing as the unlock answers makes the container forget the password. Cells in
  `lifecycle_tests.rs` and `unlock_test.rs`.

## Medium

### 2. A selection mixing the `Browse snapshots` row with real repo files reads everything through one volume

- **Where**: `crates/cmdr-operations/src/routing.rs:104-132` (`resolve_source_volume`: "the first path decides").
- **Failure**: in a repo folder, Select all then copy to another drive. The row sorts first (`B` before the lowercase
  dirs), so `<repo>/Browse snapshots` decides the source volume: every real file (`config`, `data/…`) is then read
  through `ResticVolume`, whose `classify` rejects paths outside its root (`NotFound` per file), while the row's own
  pre-scan walks every snapshot (tens of TB logical) and forces an index load. With another sort, the first path is a
  real file and the row is read through the parent (`NotFound`). A MOVE of the same selection with a real file first
  moves the real files and fails on the row: a partial op. The comment's premise ("every path shares the same archive,
  the same repo, or none") doesn't hold for an overlay row beside its siblings. Git's category rows inside `.git/` share
  the shape, but "copy my backup repo's contents" makes it a common restic flow.
- **Fix**: resolve the route of EVERY source cheaply (the lexical gate first, then `resolve` only for gated paths) and
  refuse a straddling batch with a typed refusal, or have the frontend drop overlay rows from a mixed selection the way
  delete already refuses (`first_routed` over all sources).
- **Confidence**: high on the backend shape; medium on the exact frontend sort order.
- **Status**: Fixed in the frontend by restic-frontend-7 in `b9cc45b177`: Select all and invert leave out overlay rows
  (restic's and git's), so the common flow never builds a mixed batch. The backend backstop (a hand-picked mixed
  selection, an agent) waits on a decision about its typed refusal: no existing `WriteOperationError` words a straddling
  copy honestly.

### 3. Bare future drops on the parent volume, which the house rules forbid for device backends

- **Where**: `crates/cmdr-restic/src/hub.rs:153` (`confirm_repo`: `tokio::time::timeout` around
  `parent.list_directory`), `crates/cmdr-restic/src/index_load.rs:261-265` (`run`: `select!` dropping the `index/`
  listing on cancel).
- **Rule**: `apps/desktop/src-tauri/src/commands/CLAUDE.md`: "❌ never a bare `tokio::time::timeout` (it drops the
  future and wedges an MTP phone mid-transaction)". The crate's own prefetch says "❌ never aborted mid-read" for the
  same reason, and the overlay deliberately applies on MTP and ADB.
- **Failure**: a repo on a phone, reached by a typed or restored path (memo miss): the confirm listing takes over 5 s on
  a slow device, and the timeout drops it mid-transaction. Same for a pane cancel while the load lists `index/`.
- **Fix**: `cmdr_fs::deadline` (detaches) for the confirm, and detach the `index_files` listing on cancel the way
  `prefetch` detaches its reads (spawn it, stop awaiting the handle).
- **Confidence**: high that the futures are dropped; the wedge itself is the documented MTP behavior, not reproduced.
- **Status**: Fixed in `3fa2a0f5b8`. The confirm listing runs on its own task and the deadline only stops the wait
  (detached); the index load's `index/` listing detaches on a cancel the same way. Cells:
  `a_slow_confirm_detaches_its_listing_and_remembers_undecided_briefly`,
  `a_cancel_during_the_index_listing_lets_the_listing_finish`.

### 4. A confirm that can't decide fails OPEN on write seams

- **Where**: `crates/cmdr-restic/src/hub.rs:153-159` (`Ok(Err(_)) | Err(_) => return false`),
  `apps/desktop/src-tauri/src/file_system/volume/manager/restic_routing.rs:59-64`, consumed by
  `crates/cmdr-operations/src/routed_writes.rs:24-36` (`route_serving`).
- **Failure**: memo cold (an agent's `mkdir`, a restored session, a second spelling of the path), and the repo folder's
  listing errors or outruns the 5 s deadline (a NAS waking its disks). `resolve` passes through, `route_serving` answers
  `None`, and the write goes to the parent: `create_directory` puts a real `Browse snapshots/` folder inside the user's
  repo, which the view then shadows forever. The plan names this outcome for the local fast path; the async seams have
  the same hole when the confirm is inconclusive rather than negative.
- **Fix**: give `confirm_repo` a three-way answer (`Repo`, `NotARepo`, `Unknown`). Reads may pass through on `Unknown`;
  `route_serving` (and the other write gates) refuse on `Unknown` with the typed unreachable refusal.
- **Confidence**: high on the code path; the trigger needs a slow or failing parent at the wrong moment.
- **Status**: Fixed in `3fa2a0f5b8` + `c71f47165b`. `confirm_repo` answers `Option<bool>` (`None` undecided, remembered
  5 s); `resolve` marks an undecided passthrough `ResolvedVolume::unconfirmed`, and `route_serving` fails closed on it.
  Cell: `a_write_refuses_when_the_route_cant_decide` (app `restic_guard_tests.rs`).

### 5. A repo deleted or replaced in place keeps serving its old opened repo until restart

- **Where**: `crates/cmdr-restic/src/hub.rs:318-339` (`opened` reuses the bound repo whenever the parent `Arc` matches),
  `hub.rs:305-311` (`refused` resets only on `WrongPassword`), `hub.rs:198-210` (a `Repo` confirmation is kept for
  good), `crates/cmdr-restic/src/backend.rs:80` (`snapshots/` missing lists as empty).
- **Failure**: (a) the repo folder is deleted after browsing: the root of the view lists as EMPTY (missing `snapshots/`
  is an empty list) instead of `NotFound`, and the memo keeps routing the path. (b) It's replaced by another repo at the
  same path (`restic init` again, a repo copied over it): the cached open holds the old master key, so every snapshot
  refresh fails to decrypt (`Unreadable`), and nothing drops the cached open, so the view stays broken until the app
  restarts. The index path heals (new ids miss `covers`, the reload opens afresh), the root listing doesn't.
- **Fix**: on any refresh failure other than a parent `VolumeError`, drop `slot.opened` (and the index) so the next call
  re-opens; bind the cached open to the repo's config id (`repo.config().id`) and re-open when it changes; answer
  `NotFound` when `config` is gone.
- **Confidence**: medium (the decrypt failure on a replaced repo is inferred from rustic's key handling, not run).
- **Status**: Fixed in `cbd8de0928`. A refresh that can't decrypt drops the cached open and the index and opens once
  more; no snapshots and no `config` answers `NotFound` and drops the memo's yes. Binding to the config id wasn't
  needed: the retry heals a replaced repo on the first listing. Cells in `lifecycle_tests.rs`.

## Low

### 6. The scrypt ceiling still allows minutes of CPU and 1 GiB per derivation, stacked across retries

`crates/cmdr-restic/src/backend.rs:263-285`: up to 32 keys at cost `N·r·p ≤ 2^23` (1 GiB, ~8x rustic's default each),
tried sequentially by rustic (`find_key_in_backend`). On a crafted repo one typed password costs up to 32 such
derivations on a blocking thread that the 30 s budget can't stop; each retry from the dialog starts another, and the
index load opens again. Invariant 15 holds per derivation, not per attempt. Fix: cap the summed cost across offered keys
(and, say, at most two keys above rustic's default cost).

- **Status**: Fixed in `2f70699c71`: the offered keys' summed cost is capped at four times the per-key ceiling
  (`MAX_TOTAL_SCRYPT_COST`).

### 7. Key-file and index prefetch reads trust the listing's size

`backend.rs:125-130` skips a key file only when the LISTING says it's over 64 KiB, then reads it with no bound
(`read_stream(path, None)`); `index_load.rs:291` and `:368-382` decide and size the prefetch from listing sizes too. A
backend that reports no size (`size: None` becomes 0) lets a crafted multi-GB key file be read whole before any password
is checked. Fix: pass `stop_after: Some(MAX_KEY_FILE_BYTES + 1)` and skip on overflow; cap `read_one` at the listed size
plus slack.

- **Status**: Fixed in `2f70699c71`: a key file is read with a 64 KiB bound whatever the listing says; the prefetch
  holds each index file only up to its listed size plus 64 KiB (or the budget when sizeless) and lets rustic read one
  that runs past.

### 8. A 0.5 GB index is freed while holding the slot's std mutex, on an async worker

`crates/cmdr-restic/src/index_slot.rs:161` (`drop_when_idle`), `:134` (`evict_indexes`), `:91` (replacing `ready`):
`index.ready = None` drops the last `Arc<IndexedRepo>` (2.4M entries plus the 32 MB blob cache) under
`lock(&slot.index)` inside a runtime task. Every listing of that repo blocks on the lock meanwhile, and the worker
stalls for the deallocation. Fix: `take()` the `Ready` under the lock and drop it after, on `spawn_blocking`.

- **Status**: Fixed in `2f70699c71`: every place an index leaves its slot `take()`s it and frees it on the blocking pool
  (`index_slot::free_off_the_runtime`). No cell: a drop's length isn't observable from a test.

### 9. `latest` is pinned by a 30 s window, not by the reads that need it

`crates/cmdr-restic/src/hub.rs:653-667` (`LatestPin::rebind`), `read_stream.rs:43-45`. A copy out of `latest/` that
parks on a person (a name-clash prompt) or on one chunk read longer than `latest_quiet` lets a root refresh (the other
pane, an agent) rebind `latest`; the copy's remaining files then come from the newer snapshot: the mixed restore the
plan forbids. Needs a new backup plus a root listing inside that gap. Fix: hold a counted pin for the life of any stream
or scan opened via `latest`, and rebind only at zero.

- **Status**: Not fixed, on purpose. A pin held by the read stream would cover a copy parked mid-file, but not the gap
  BETWEEN files where a copy parks on a name-clash prompt, which is the likelier trigger; the honest fix is an
  operation-scoped pin from the copy engine (`cmdr-operations`), which belongs with the copy-out work. The remaining
  risk needs a new backup plus a root refresh inside a 30 s pause; a dated folder is the exact pin. Worth a follow-up
  issue.

### 10. Confirming lists a whole candidate folder, and a timeout isn't remembered

`hub.rs:153`: the confirm is a full `list_directory` of the folder holding `Browse snapshots`, uncapped (unlike the
overlay's and the local guard's 32-entry cap). A real `Browse snapshots` folder inside a 100k-entry folder on a slow
share pays up to the 5 s deadline on EVERY `resolve` under it, since a timeout memoizes nothing (rule 2's hot-path
budget). Fix: memoize `Unknown` briefly for reads (see finding 4), and treat a listing past `MAX_REPO_ROOT_ENTRIES` as
"not a repo".

- **Status**: Fixed in `3fa2a0f5b8`: an undecided confirm is remembered for 5 s, and `is_restic_repo` answers a listing
  of more than 32 entries as not a repo on its length alone.

### 11. Held SMB pack readers outlive the browse by the index's lifetime

`backend.rs:163-252`: up to eight pack handles per indexed repo stay open until the index drops (five idle minutes).
They open with `FILE_SHARE_DELETE` (`smb2` `tree.rs:2699-2703`), so deletes succeed, but on Samba and Windows a deleted
file stays delete-pending until its last handle closes, so removing the repo's `data/xx` folders over the direct session
right after browsing can answer "folder not empty". Invariant 5 says the real files stay deletable. Fix: close held
readers after a short idle (seconds), independent of the index.

- **Status**: Fixed in `9e7ecef742`: held readers close once none was used for 5 s (`READER_IDLE`), independent of the
  index.

### 12. English detail strings ride typed errors out of the crate

`repo.rs:46-49` and `:202-205`, `index_load.rs:357-360`: `Unreadable { detail }` (rustic's `display_log`) and "the
blocking worker stopped: …" / "an index read stopped: …" become `VolumeError::IoError { message }`. They're diagnostics,
not copy, but `IoError.message` is what the frontend's technical-details line shows. Rule 3 asks for typed answers; fine
if the frontend never renders `message` outside technical details, worth a check in the frontend.

- **Status**: Not changed. `IoError.message` is the technical-details line, the house shape for diagnostics every
  backend uses, and the frontend words the typed error itself; replacing rustic's `display_log` with a typed reason
  would drop the only detail a bug report has. Worth a check in the frontend that `message` never renders outside
  technical details.

### 13. Root refreshes aren't single-flight

`hub.rs:384-413`: every root listing refreshes, and two concurrent ones (a stalled listing's second attempt, two panes)
each read `snapshots/` and each rebind `latest`. Cheap at nine snapshots; a repo with thousands of snapshots on SMB pays
twice, and rustic's `update_all_snapshots` reads them on the global rayon pool. Fix: share an in-flight refresh per
slot.

- **Status**: Fixed in `7bd10b0074`: one refresh per repo at a time, and a caller that asked while one ran takes its
  list.

## Checked and holding

- Zero writes: `WriteBackend` refuses all three methods (`backend.rs:411-423`); no seam hands a repo path to a writer.
- Write guards: `RoutedKind::{takes_writes, can_give_up_files}` match exhaustively; `route_serving` and `first_routed`
  cover rename, create, numbered create, bulk rename, rename-as-move, delete, and trash through the lexical gate
  (`path_routes_over_its_parent` includes restic); the local fast path and the agent's rename plan ask confirmed
  predicates. Move out is refused before the destination resolves.
- Walkers: the overlay reaches pane listings only; the oracle declines unrouted view paths; ⌥⇧⏎ skips view rows.
- Rustic calls sit on `spawn_blocking` (open, refresh, index, tree, `open_file`, `read_file_at`, `classify`); no
  `block_on` is reachable from a runtime worker.
- Hub locks: no hub-wide lock across scrypt, Keychain, or a parent read; lock order (`index` → `snapshots`; `repos`
  released before any slot lock) has no cycle.
- Index load: one per repo, waiters join under the same lock that cancels, a stalled retry joins, progress is bytes, and
  a phase change resets the stall watch.
- Keychain: service `restic://` can't collide with an SMB account (pinned in `network/keychain.rs`); lookups go through
  `secret_store`, misses are memoized, and the MCP path never seeds an entry.
- Leftmost confirmed boundary: boundary indexes are comparable across the three routes, and an unconfirmed candidate
  hands over.
