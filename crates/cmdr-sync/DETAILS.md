# cmdr-sync details

## classify (compare.rs)

Mode-independent; `left` and `right` are `Option<&EntryFacts>`.

| Situation                            | Category                                                 | Rung          |
| ------------------------------------ | -------------------------------------------------------- | ------------- |
| One side only                        | `LeftOnly` / `RightOnly`                                 | `Presence`    |
| Kinds differ (file, folder, symlink) | `TypeClash`                                              | `Presence`    |
| Two folders                          | `Equal`                                                  | `Presence`    |
| `SizeOnly`, or `ignore_date`         | `Equal` / `Different` by size; unknown size is `Unknown` | `SizeOnly`    |
| `DateAndSize`, a time missing        | sizes decide as above, never `Newer*`                    | `SizeOnly`    |
| `DateAndSize`, dates differ          | `NewerLeft` / `NewerRight`                               | `SizeAndTime` |
| `DateAndSize`, dates equal           | `Equal` (sizes match or unknown), else `Different`       | `SizeAndTime` |
| `Content`, sizes known and unequal   | as `DateAndSize`                                         | as there      |
| `Content`, sizes known and equal     | per `ContentState`, see below                            | `Content`     |

With equal known sizes, `Content` maps `Same` to `Equal`, `Differs` to the newer side by date (`Different` under
`ignore_date` or equal dates), and `NotAsked`, `Pending`, `Unreadable` to `Unknown`. `same_size` on the verdict says
whether both sizes are known and equal.

Dates are equal when `|left - (right + right_offset_secs)| <= tolerance_secs`, or when `ignore_hour_shift` is on and the
gap is exactly 3600 s (the FAT/exFAT daylight-saving shift). `widened_for` raises the tolerance to the coarser side's
`mtime_precision_secs`.

Folder-versus-folder is `Equal` here: contents are the scanner's business. A symlink compares by its own listing facts.
⇧F2 forces `kind: File` on both sides so a symlink beside a file keeps its old date comparison.

## Backend facts (caps.rs)

| Backend                                              | Case sensitive                            | mtime step           |
| ---------------------------------------------------- | ----------------------------------------- | -------------------- |
| `Local`                                              | no on macOS (APFS default), yes elsewhere | 1 s                  |
| `Smb`, `Mtp`, `Adb`                                  | no                                        | 1 s                  |
| `Sftp`, `Webdav`, `S3`, `Nfs`, `GitPortal`, `Restic` | yes                                       | 1 s                  |
| `Archive`                                            | yes                                       | 2 s (zip's DOS time) |

(Derived from each backend's server or format semantics, not measured on a device; the per-backend `Volume` impls are
the anchor. `FileEntry::modified_at` is whole seconds everywhere, so 1 s is the floor.) `can_read_content` is
`supports_export()`.

## Decision: no `cmdr-operations` dependency

`cmdr-listing` must depend on this crate (shared `classify`), and `cmdr-operations`' test build depends on
`cmdr-listing`. A direct dependency from here on `cmdr-operations` would close a package cycle. The executor runs
against `SyncEngine` (§ Run), which the app implements over `Operations`. The trait is also what makes the executor
testable: `execute_fake.rs` records every batch and every broken promise.

## `EntryKey::is_within`

Inclusive: a key is within itself. Callers that want strict descendants compare for inequality too.

## Masks (filter.rs)

TC sync syntax. Masks split on spaces or `;`, `"…"` keeps spaces, `*` and `?` are the only wildcards, one `|` starts the
excludes, the editable exclude list is appended after it. A trailing `/` means folders only; a leading `/` or a `/`
inside anchors the mask to the whole path (backslashes are read as `/`, so masks pasted from Total Commander work) from
the root, otherwise it matches the name at any depth. Everything is compared on `fold_name` of each side's own spelling,
so masks ignore case and Unicode form whatever the pair's `NameRule`.

Verdict order: `.cmdr-sync` at any level (and under it) is `Out`; an excluded ancestor folder excludes its contents; a
file that no include matches is `Out` (folders are always walked, so an include like `*.jpg` still reaches
`photos/a.jpg`); an exclude from `JUNK_MASKS` answers `Junk`, any other exclude `Out`. An empty mask (a bare `|`, `""`,
a blank entry of the exclude list) is refused with its position rather than read as "match everything".

## Scanner (scan.rs)

Recursive, one `list_directory_for_scan` per folder, asking `ScanBoundary::dir` first so Cancel and Pause land before
the round trip. Admission per child, in order: `only` (top level), `subfolders` off (folders are not entries at all),
the filter (`Out` counts held back, `Junk` vanishes), hidden while hidden is off (held back). Survivors group by name
key: a group of two or more goes to `clashes` and out of `entries`, and a clashing folder is not descended. A symlink to
a folder is an entry, never followed.

`SubtreeFacts` is recursive: files and symlinks, their sizes, newest mtime, and held-back items beneath. A folder with
nothing admitted inside, after its own empty subfolders drop, is left out unless `empty_folders`. Its held-back count
still flows to its parent. A subfolder that won't list keeps its entry, joins `unreadable` with an `UnreadableReason`
(permission, not found, unreachable, other; mapped from the typed `VolumeError`), and has zeroed facts except
`unreadable: 1`, which `SubtreeFacts` adds up through every ancestor; the root that won't list is `RootUnreachable`. The
root's own held-back count has nowhere to live (no root entry) and is dropped. `InMemoryVolume` lists a missing folder
as empty, so unreachable-root tests use a double that refuses the path.

## Content compare (content.rs)

Each side keeps the unconsumed rest of its last chunk, since the two streams chunk differently. The loop stops at the
first differing byte or length mismatch. Cancel and pause are checked before every chunk; a stopped job answers
`Pending`. Progress counts bytes as read, and a pair decided early adds the rest of its size so the bar ends full.
Workers: one when both volumes are `Local` (two disks seeking against each other), else four.

The scanner skips the app-owned `.cmdr-sync` folder before the filter runs: no entry, no held-back count, at any depth
(`MaskFilter::admits` still answers `Out` for it), but each one counts in `SubtreeFacts::app_owned` of its folder and
every ancestor. A symlink to a folder is filtered as a file, as it is treated. A folder with `held_back`, `unreadable`,
or `app_owned` above zero is non-atomic: the planner must not copy or remove it whole.

`SubtreeFacts::digest` is a wrapping sum of one hash per admitted child (its own spelling, kind, size, and time; a
folder hashes its name with its own digest, an unreadable one with a marker), so a rename or a same-size swap that the
counts can't see still changes every folder above it. `DefaultHasher` is only stable within one build, which is all a
compare-then-run needs; the digest is never stored. `subtree_now(side, volume, path, rel, options, control)` walks one
folder with the scan's own code and options and returns the same `SubtreeFacts`, digest included: the app's
`SyncEngine::observe` answers folders with it.

## Planner (plan.rs)

The mode table is the phase 1 design's section 6.4 (linked from upstream issue #871), implemented as `default_action`;
`Mode::ALL` lists the columns. A table action that would write to a side whose `SyncCaps::can_write` is off becomes
`Skip` (the row's `default_action` already says so). `Undecided` is never a default: `?` rows are `Skip` and show in the
`?` class.

| Category                               | Compare only | Both ways | Update → | Mirror →                   |
| -------------------------------------- | ------------ | --------- | -------- | -------------------------- |
| left only, newer left                  | ·            | →         | →        | →                          |
| right only                             | ·            | ←         | ·        | ×→ (guarded)               |
| newer right                            | ·            | ←         | ·        | → (older overwrites newer) |
| different                              | ·            | ·         | ·        | →                          |
| equal                                  | =            | =         | =        | =                          |
| unknown, type / name clash, unreadable | ·            | ·         | ·        | ·                          |

Keys are the union of both sides' entries and clashes, walked in key order (parents before children, each subtree
contiguous). Per key, first match wins:

1. At or under an `unreadable` key on either side: one `Unreadable` row that covers everything beneath it.
2. A clash key on either side: one `NameClash` row (the clashing side's entry is `None`; its spellings are in
   `Snapshot::clashes`), covering everything beneath.
3. Both sides folders: no row; the folder heads its children.
4. Kinds differ: one `TypeClash` row covering the folder side's subtree.
5. One-sided folder with `held_back`, `unreadable`, or `app_owned` in its subtree, or a clash or unreadable key beneath
   it (checked directly, not only through the counters): no row; its children are rows (non-atomic). Otherwise one row
   for the whole subtree, `bytes = subtree.bytes`.
6. Anything else: `classify`.

Name clash, type clash, and unreadable rows only ever take `Skip`. `older_overwrites_newer` is derived from the action:
`CopyRight` on `NewerRight` or `CopyLeft` on `NewerLeft` (`settle` recomputes it, and `bytes`, after every change).
`bytes` is what the action copies or removes; with no action, the larger side.

`Content` rules mark an equal-size file pair `Pending` (an `Unknown` row) when both volumes can read content;
`content_jobs` lists them and `reclassify` folds the answers in, touching `Pending` rows only (never a clash, type
clash, or unreadable row). A row whose action you changed keeps it while it stays legal; one the answer made illegal
(`?` on a row that turned out equal) is reset to its new default and listed in `Reclassified::reset`. The rest follow
the new default.

`Plan::heads` lists every folder that heads rows (on both sides, or split), with each side's own `rel_path`. Every
ancestor of a row is one, so a copy finds the target's own spelling of each folder on its way (an NFD folder on APFS
paired with its NFC twin on SMB lands in the twin, not a new folder beside it), and knows which folders are missing.

Decision: `Plan` carries both sides' `SyncCaps`, so `reclassify` keeps defaults off read-only sides without the
snapshots, and overrides are judged by the plan's own caps (the override API takes none from callers).

Decision: an app-owned `.cmdr-sync` below the root (another pair's version history) makes its folders non-atomic, so
Mirror never trashes it along with a whole folder. OS junk (`.DS_Store`, `._*`, `Thumbs.db`) is not counted and travels
with whole folders, as in Finder.

## Overrides (overrides.rs)

An action is legal when the mode allows it (Compare only: any; Both ways: no removals; Update: no removals, no `←`;
Mirror: no `←`, no `←×`; else `IllegalForMode`), the category allows it (`=` only on equal rows, `?` only on unknown
ones, copies and removals never on clash, type-clash, or unreadable rows; else `IllegalForCategory`), the side it reads
from or removes has an entry (`NothingToCopy`), and the side it writes to can write (`SideReadOnly`). `apply_overrides`
is all-or-nothing. `bulk_changes` drops illegal, no-op, and unknown rows silently. `Reverse` flips a copy only when both
sides exist and turns a removal into the copy that restores it; a one-sided copy would have to become a removal, so it
stays.

Contract: `bulk_changes` receives only rows the person selected AND can see under the current show-filters. Within that
selection `→`, `←`, and remove-target also reach `?` rows: selecting them and pressing the key is the explicit choice
they wait for.

## Guards (guards.rs)

- `MaxRemove { side }`: files removed on a side (a folder row counts its `subtree.files`) above `max_remove_percent` of
  that side's `file_count()`, or above `max_remove_count` (0 is off). `RemoveAnyway` releases.
- `EmptyRoot { side }`: `Snapshot::is_empty()`. Holds every removal on either side with no way to release it, and holds
  copies into that side until `CopyIntoEmpty` (the plan's ruling: an empty root is most likely an unmounted mount
  point).
- `SentinelMissing { side }`: the sentinel name (keyed with the pair's `NameRule`) isn't a non-folder entry of that
  root. Holds every action. The sentinel has to survive the scan's masks and the hidden-files toggle.
- Skew: pairs of non-folder entries with both times; each gap (right minus left, after the offset) rounds to the nearest
  900 s step when within the tolerance of one. Reported when one non-zero step up to 26 h covers ≥ 90% of at least 10
  pairs. With the hour-shift option on, pairs exactly ±3600 s apart are left out (already equal). `offset_secs` is how
  far the right runs ahead; its negation is the time offset that cancels it.
- `gate(plan, left, right, settings, acknowledged)` runs `check` itself on exactly the plan it is handed, so a report
  made before the person changed actions can't wave new removals through. It splits acting rows into `run` and `held`
  (sentinel first, then empty root, then max remove); `=`, `·`, and `?` rows are in neither.

## View (view.rs)

`row_class` follows the action (`→`, `←` for copies left and removals on either side, `=`, `?` for `Undecided` or a
skipped unknown row, `≠` for any other skip). A row shows when its class filter, its presence filter (duplicates or
singles), and the warnings filter all let it. Decision: hiding never changes an action (TC and Krusader drop hidden rows
from the run); `PlanSummary::hidden_actions` counts what still runs unseen. Rows group under their parent folder; the
root's rows come first without a header, every other visible group gets "folder n of N".

`SideTotals` splits new copies (`copy_files`, `copy_bytes`) from replacements of an existing file (`replace_files`,
`replace_bytes`), so the confirm can say where replaced files go. Removals count files plus `removed_folders` (empty
ones included); max remove still divides by files. Clash, type-clash, and unreadable rows count as `not_comparable`,
never as different or unique.

## Planner laws (plan_proptests.rs)

Generated pairs (up to three levels over three names: files, symlinks, folders, held-back counts, unreadable folders,
clashes, app-owned folders, read-only sides, and a tenth of cases with one side blank) and a pure `apply_plan` model of
a run check: determinism, no removals in Update and Both ways, nothing written left in Update and Mirror, re-planning
after a run is all `=` (Mirror) or copies nothing (Update, Both ways) when the targets take writes, names only grow
(Both ways), no default action on `?`, clash, or unreadable rows, protected entries (under unreadable keys, non-atomic
folders) survive every mode, also after random overrides and bulk keys through the public API (with the source side in
Mirror and Update and unchosen `?` rows untouched), the warning flag is exact, overrides never yield an illegal action,
and guards trip exactly at their thresholds on overridden plans.

## Keep (keep.rs)

`keep_destination(policy, target caps, versions, rel_path)`: `TrashElseVersions` → `Trash` when `has_trash`, else the
versions folder; `VersionsAlways` → the folder; `TrashOnly` without a Trash → `Refused(NoTrash)` (the row is left alone,
never deleted). A folder destination is `<versions dir>/<rel parent>`, so names keep their folder structure; a
`rel_path` with an empty, `.`, or `..` part is `Refused(EscapesFolder)`.

`versions_root(policy, target_root, profile, stamp, attempt)`: `TargetRoot` is `<target root>/<folder_name>/<stamp>`
(`Place::Target`, a rename away); `folder_name` must be a plain relative path. `Custom` is an absolute local-disk
template (`Place::LocalDisk`) with `{root}` (the target root without its leading `/`), `{profile}` (the name with `/`
turned into `-`, or "No profile"), and `{date}`; the stamp folder is appended when `{date}` is absent, so runs never
mix. Unknown or unclosed placeholders and relative templates are `BadTemplate`; a `..` or empty part after expansion (or
in the target root) is `EscapesFolder`. `attempt > 0` names the stamp folder `<stamp> (attempt+1)`; the caller bumps it
when the folder already exists. Laws: `kept_paths_never_escape_the_versions_folder` in `keep_test.rs`.

Gotcha: a `folder_name` outside `.cmdr-sync` is compared like any other folder (the scanner only skips `.cmdr-sync`), so
Mirror would see the kept items as right-only. Settings should keep it under `.cmdr-sync`.

## Run (steps.rs, execute.rs, runner.rs)

`steps(plan, run, ctx)` turns the rows `gate` let run into groups, in this order: `MakeFolders` per (place, side)
(versions folders that will receive something, and target folders of split one-sided folders whose children are copied,
parents first; made with missing parents), `KeepAside` per (side, destination) (the target of every copy that replaces
something), `Copy` per (direction, landing folder), `Remove` per (side, destination). Rows whose keep is refused go to
`refused` and get no step (their copy doesn't run either). Clash, type-clash, and unreadable rows never get a step,
whatever their action says.

`execute` runs the steps one batch each. Pause is awaited and Cancel checked between batches. Before each batch every
item is observed again and must read exactly as the snapshot did (files: kind, size, time; folders: the whole
`SubtreeFacts`, digest included, but not the folder's own time, which Finder touches by browsing); a mismatch is
`ChangedSinceCompare` and the item leaves the batch. Before a keep-aside both the copy's source and the target are
checked, so a doomed copy never costs the target its file; before a copy the landing spot must be empty. Every source
goes to the engine as a `BoundSource` with the snapshot's facts, so the engine's own `ExpectedSources` re-check covers
the window between observe and act, and copies land with `ConflictResolution::Skip`.

Decision: no `expected_targets` twin in the engine. Every target a run replaces or removes is first the source of a keep
batch (`Move` or `Trash`), so the binding covers targets too.

A replacement's kept item goes back (`MoveBack`) when its copy doesn't land for any reason, including a cancel between
the keep and the copy, unless something now sits in its place. From the Trash nothing can come back: the row reads
`KeptNotReplaced` (the old file is in the Trash, the new one didn't land), as does a move back that couldn't run. A copy
the engine answers `Failed` holds every removal on that side under the copy's parent folder (`HeldAfterFailedCopy`): in
Mirror a rename is a copy plus a removal, and the removal must not run alone. A folder `MakeFolders` couldn't make turns
everything landing or kept beneath it into `CouldNotDo`. A source the engine never answered for in a batch it didn't
stop counts as `Failed`. Rows the run never reached are `NotStarted`.

`RunReport::kept_bytes` counts what went into versions folders (the result screen shows that size), minus anything moved
back. Law: `execute_proptests.rs` runs random plans in every mode against random edits before or between batches; the
fake flags any move, trash, or copy of something that differs from the snapshot (anywhere beneath it), any landing on an
occupied spot, and anything lost.
