# Restic volume: adversarial review, round two

Review of the restic work since `56dd11f457` (#422, PR #632), at `b60db61a18`: the streaming index feed and the rustic
pool (#630), the wrong-password dialog feedback, "Forget backup password" (#627), snapshot metadata on rows (#628), the
`snapshots/` watcher (#626), and the transfer hold `Volume::hold_for_transfer` (#629). Method: read every path named
below, rustic facts checked against `rustic_core-0.13.0`'s source. Round one and its fixes:
`docs/notes/restic/conformance-review.md`. 2026-10-21.

Summary: one high, two medium, six low, one info. No path to a write against the repo, no rustic fan-out left on rayon's
global pool, no hold reachable from a queued op, and no lock-order cycle among the hold count, the index slot, and the
hub's locks.

## High

### 1. `latest` moves under a paused copy, and the watcher now moves it with nobody pressing refresh

- **Where**: `crates/cmdr-restic/src/hub.rs:775-782` (`LatestPin::rebind`: idle after 30 s untouched),
  `crates/cmdr-restic/src/index_slot.rs:125-141` (`RepoSlot::hold_for_transfer` counts on the index only),
  `crates/cmdr-restic/src/read_stream.rs:93` (only a chunk read touches `latest`),
  `apps/desktop/src-tauri/src/file_system/restic/snapshot_watch.rs:343-369` (the re-list), doc claim at
  `apps/desktop/src-tauri/src/file_system/restic/DETAILS.md:52-55`.
- **Failure**: a copy out of `latest/` pauses for more than 30 s (Pause, or a name-clash prompt the person leaves up).
  It holds no read stream between files, and a stream parked mid-file reads no chunk, so nothing touches `latest`. A
  scheduled backup lands; the watch re-lists the root pane; `rebind` sees `latest` idle and moves it. The copy resumes
  and every remaining file comes from the newer snapshot: the mixed restore the crate's `latest` rule exists to prevent,
  silently. Round one left this open (finding 9) because it needed a manual refresh inside the pause and because the
  honest fix was "an operation-scoped pin from the copy engine". Both premises changed: the watch supplies the refresh
  automatically, and #629 added exactly that operation-scoped seam, but the hold pins the index and not `latest`. The
  app DETAILS says "During a copy out of `latest` the new dated folder appears and `latest` stays put", which holds only
  while the copy keeps reading.
- **Fix**: have `rebind` keep `latest` while the slot has any transfer hold (`index.transfer_holds > 0`), still moving
  at once when its snapshot is gone. It's conservative (a copy out of a dated folder also freezes `latest`), which is
  harmless. Add a cell next to `a_new_backup_during_a_read_under_latest_leaves_latest_alone` with a held, idle copy and
  `latest_quiet` shortened.
- **Confidence**: high. The cell that pins "leaves `latest` alone" opens a stream inside the quiet period; nothing
  covers a pause.
- **Status**: fixed as proposed. `LatestPin::rebind` moves `latest` only when its snapshot is gone while
  `RepoSlot::is_held_for_transfer`; pinned by `a_paused_copy_out_of_latest_keeps_it_until_the_copy_settles`.

## Medium

### 2. Work in flight can undo a "Forget backup password", and the forget can hang before the Keychain delete

- **Where**: `crates/cmdr-restic/src/hub.rs:316-326` (`forget_saved_password`), `:376-397` (`opened`), `:442-503`
  (`snapshots`), `:272-293` (`unlock`), `crates/cmdr-restic/src/index_slot.rs:76-114` (`Installed::finish`),
  `apps/desktop/src-tauri/src/commands/file_system/unlock.rs` (`forget_saved_container_password`, no budget).
- **Failure**: the forget clears each piece once, but nothing stops a caller that already passed its password check from
  committing afterwards:
  1. `opened()` reads the password at `:383`, releases the `opened` lock, then re-takes it at `:385` and opens. The
     forget's `*slot.opened.lock().await = None` (`:322`) can run in that gap; the open then commits a fresh `Bound` at
     `:393`. The root lists again with no password, until eviction or restart. Trigger: forget within the 1–3 s of a
     first, Keychain-fed open (navigating into the view, or a stored-password listing after an eviction).
  2. A refresh that took its `repo` before the forget writes `slot.snapshots` back at `:498` after the forget cleared
     it.
  3. The forget doesn't touch `index.loading`. A load with a waiter the pane's navigation doesn't drop (the other pane,
     a copy) finishes and `finish` installs `ready` (`index_slot.rs:95`). Together with (1) or (2), every path inside a
     snapshot reads without asking; alone it pins ~0.5 GB for the idle window.
  4. `ResticRepos::unlock` in flight (the MCP `unlock` tool while the person forgets from the palette) commits the
     password at `:281` and clears `store_missed`, then the host's `remember_password` writes the Keychain entry the
     forget just deleted. Separately, the forget waits on the `opened` lock (`:322`) BEFORE deleting the Keychain entry
     (`:325`), and the IPC command has no budget: an open wedged on a dead NAS holds that lock, so the forget never
     answers, no toast appears, and the entry stays in the Keychain.
- **Fix**: a per-slot lock epoch (`AtomicU64`). The forget bumps it first, deletes the store entry, then clears the
  memory state with `try_lock` semantics. Every commit captures the epoch before its first await (`password()`,
  `unlock`, `opened`, `snapshots`, `start_load` into `Installed`) and commits only if it's unchanged. The forget also
  takes `index.loading` and cancels it. Bound the IPC with the unlock seam's budget.
- **Confidence**: medium-high on (1)–(3) from the code; (4) needs two actors. The hang follows from the lock order.

### 3. A direct SMB share's watcher re-lists on any change in any folder above the repo, which contradicts the doc

- **Where**: `apps/desktop/src-tauri/src/file_system/restic/snapshot_watch.rs:159-176` (`directory_changed`:
  `key.snapshots_dir().starts_with(parent_path)`), `apps/desktop/src-tauri/src/listing_host/mod.rs:191` (the composer
  drops the `DirectoryChange`), the SMB watcher's per-entry reports at `crates/cmdr-smb/src/volume/watcher.rs:202-260`,
  doc at `apps/desktop/src-tauri/src/file_system/restic/DETAILS.md:43-45`.
- **Failure**: the prefix match is meant for an overflow reported on a folder above (the doc says "or an overflow report
  on a folder above it"), but it fires for every `Added`/`Removed`/`Modified` whose parent is the repo folder, its
  parent, or the share root. So a file saved next to the repo, any write at the share root, Cmdr's own SMB writes
  (`mutation.rs:370-388`), and restic's own backup (the `index/` and `data/` folders' times change, reported with parent
  `<repo>`) each trigger a full `snapshots/` refresh on a fresh rustic pool. No debounce on this path: a busy ancestor
  folder keeps one refresh running back to back for as long as the pane is open, which is polling at network speed. The
  same path reaches a phone's event loop (MTP reports through the composer too).
- **Fix**: pass the change to `snapshot_watch::directory_changed`; match `parent_path == snapshots_dir` for entry
  changes, and the prefix only for `FullRefresh`. A short debounce on this path, like the `notify` one's 300 ms, would
  also fold the temp-then-rename pair into one re-list.
- **Confidence**: high on the code; medium on how often a given NAS reports folder-time changes on the parent.
- **Status**: fixed as proposed. The composer passes the change; an entry change counts only when its folder IS
  `snapshots/`, and only a `FullRefresh` counts from a folder above. Every re-list now waits 300 ms for the burst to
  settle, on both sources. Cells: `only_a_change_in_snapshots_or_an_overflow_above_it_counts` (200 unrelated reports,
  zero heard) and `a_burst_in_snapshots_relists_once` (20 changes, one re-list).

## Low

### 4. The "known gap" is accurate, but its fix needs no new hub method

`crates/cmdr-restic/src/hub.rs:458-465` and `:498-502`; doc at
`apps/desktop/src-tauri/src/file_system/restic/DETAILS.md:56-59`. The DETAILS describes the race correctly: a manual
refresh that began listing `snapshots/` before the new file landed finishes after the watch asked, `read_at > asked`
holds, and the watch's re-list takes the older list. The cause is that `read_at` is stamped when a refresh ENDS.
Stamping it when the refresh BEGINS (before `opened()`) makes "reuse a list only if its refresh started after I asked"
exact, inside the hub, with no change to the capped surface. Cost: a caller that asked while a refresh ran runs one more
after it (later waiters still share that one). The doc's "needs a new hub method on the capped surface" is wrong.
Confidence: high.

### 5. An index read reserves its listed size up front

`crates/cmdr-restic/src/index_load.rs:329`: `Vec::with_capacity(start.listed)`, and a read ahead starts at any listed
size when the window holds nothing (`index_feed.rs:268`). A hostile or broken listing (an S3 or WebDAV size field, a
sparse index file) reserves that much before a byte arrives. Past `isize::MAX` it panics the read task (the load fails,
typed); a huge but valid size can fail the allocation, which aborts the process. Round one's finding 7 bounded the old
prefetch by the listed size plus slack, so this is the same trust in a new place. Fix: reserve `min(listed, window)` and
let the `Vec` grow. Confidence: medium (whether the allocation aborts depends on the allocator and the size).

### 6. A re-list task that outlives its watch can steer the next watch on the same repo

`apps/desktop/src-tauri/src/file_system/restic/snapshot_watch.rs:343-355`: the loop looks the watch up by key, not
epoch. If the last pane closes during a re-list and a new pane re-arms the same repo, the old task clears the new
watch's `refreshing` (two re-lists at once) or consumes its `again`. Nothing is lost (it re-lists in that case), so it's
a duplicate re-list at worst. Fix: carry the epoch into the task and stop on a mismatch. Confidence: high.
Status: fixed as proposed (the task captures its watch's epoch and returns on a mismatch). No cell: reaching it takes a
close and a re-arm inside one re-list, and the worst case was a duplicate re-list.

### 7. A wedged parent leaves a rustic pool behind per abandoned refresh

`crates/cmdr-restic/src/repo.rs:39-47` and `:152-158`. A pool per call isn't a leak: rayon's threads exit once the
call's work ends. But a snapshot refresh blocked in a parent read that never returns keeps its five threads plus one
blocking thread, and the pane's cancel releases `refreshing`, so the next refresh (the watch from finding 3, a person
pressing refresh) builds another. Threads pile up for as long as the parent hangs. The global pool would have been
starved instead, so this is better than before, not worse. Fix if it matters: build the refresh pool once per hub (not a
static), sized as now. Confidence: medium.

### 8. Forgetting through one route leaves the other route's Keychain entry

`crates/cmdr-restic/src/hub.rs:106-108` (`store_scope` is `<volume id>:<repo path>`), `:325`. The same repo through an
OS mount and through a direct SMB session is two entries (documented). "Forget backup password" deletes the one for the
pane's route and toasts "forgotten"; the next launch through the other route still unlocks silently. Fix: say which
connection in the toast, or have the forget also drop the sibling slot's entry when the host knows both spellings.
Confidence: high on the behavior; whether it's worth fixing is a product call.

### 9. A doc line predates the hold

`crates/cmdr-restic/DETAILS.md:388`: "Copy-out holds the index through its read streams". It also holds it through the
transfer hold for its whole run, paused or not (§ "The index lifecycle" in the same file says so). Point the line at
that bullet.

## Info

### 10. A paused transfer keeps its index, and the cap overshoots, for as long as it stays paused

`crates/cmdr-restic/src/index_slot.rs:168` and `:198`. This is the design (#629), and the overshoot is documented. Worth
knowing for a memory report: a copy left paused overnight, plus browsing a second repo, holds two indexes (1–3.6 GB on
the sizes in `docs/notes/restic/open-cost.md`) until the copy settles.

## Checked and holding

- **The hold**: taken inside each managed body after admission (`copy.rs:329`, `move.rs:230`, `copy_into.rs:414`,
  `fresh_compress.rs:198`), so a queued op holds nothing; a local dropped on every exit, a panic unwind, and a dropped
  task. No volume wraps another, so no forwarding was missed. Taking and dropping it does no I/O.
- **Locks**: `HeldForTransfer::drop` releases `index` before `touch`; `index` → `last_used` is the only nesting
  (`in_use`, `drop_when_idle`), never the reverse. `forget_saved_password` takes `store_lookup` → `opened`; `opened()`
  drops `opened` before `password()` takes `store_lookup`, and `snapshots()` holds only `refreshing` across both. No
  cycle.
- **The window**: read-aheads stop at four and at 16 MiB counted by listed size, then by bytes; a read ahead past its
  size plus 64 KiB is let go of; files go the moment rustic takes them; `close` empties the window on every end. An
  asked file bypasses the window, so a decoder never deadlocks behind it.
- **Cancel**: the last waiter cancels the token; `run` answers at once; reads in flight stop at their next chunk
  (detached); `close` wakes a decoder parked on the condvar; rustic's `stream_all` loop returns on the first `Err` and
  drops the receiver, so the zero-capacity channel's senders stop.
- **The pool**: rustic 0.13.0 fans out only in `stream_list` (index build, `update_all_snapshots`), the index's
  `par_sort`, and commands this crate never calls (`check`, `prune`, `copy`, `repair`). Both reachable ones run inside
  `on_own_pool`.
- **Metadata**: `ResticSnapshotMeta` is facts only; the frontend words it from the catalog; Brief's HTML tooltip escapes
  the repo's host, tags, and paths before joining them.
- **The dialog**: the refusal note is `role="alert"` under the field, re-created per rejection so it's announced again,
  wired with `aria-describedby` and `aria-invalid` (`TextInput` forwards both); the shake is inside
  `prefers-reduced-motion: no-preference`; focus returns to the field with its text selected when a check ends.
- **The watcher**: arms on the view's root only (memo and string work, no I/O), gives the hold back on close and on the
  open/close race, keeps an armed stream only for the same epoch, and never polls a parent with no watcher.
- **Forget, the parts that hold**: the Keychain adapter drops its process cache before the delete; `store_missed` keeps
  a refused delete from reloading the entry this session; the one-shot offer for the same key is cleared.
