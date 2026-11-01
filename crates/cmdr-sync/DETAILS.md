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
`cmdr-listing`. A direct dependency from here on `cmdr-operations` would close a package cycle. The executor will be
written against a small trait the app implements over `Operations`.

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
(`MaskFilter::admits` still answers `Out` for it). A symlink to a folder is filtered as a file, as it is treated. A
folder with `held_back > 0` or `unreadable > 0` is non-atomic: the planner must not copy or remove it whole.

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
5. One-sided folder with `held_back`, `unreadable`, or a clash beneath it: no row; its children are rows (non-atomic).
   Otherwise one row for the whole subtree, `bytes = subtree.bytes`.
6. Anything else: `classify`.

Name clash, type clash, and unreadable rows only ever take `Skip`. `older_overwrites_newer` is derived from the action:
`CopyRight` on `NewerRight` or `CopyLeft` on `NewerLeft` (`settle` recomputes it, and `bytes`, after every change).
`bytes` is what the action copies or removes; with no action, the larger side.

`Content` rules mark an equal-size file pair `Pending` (an `Unknown` row) when both volumes can read content;
`content_jobs` lists them and `reclassify` folds the answers in. A row whose action differs from its default (changed by
you) keeps it; the rest follow the new default.

Decision: `Plan` also carries both sides' `SyncCaps`, so `reclassify` can keep defaults off read-only sides without the
snapshots.

## Overrides (overrides.rs)

An action is legal when the mode allows it (Compare only: any; Both ways: no removals; Update: no removals, no `←`;
Mirror: no `←`, no `←×`), the category allows it (`=` only on equal rows, `?` only on unknown ones, copies and removals
never on clash or unreadable rows), the side it reads from or removes has an entry, and the side it writes to can write.
`apply_overrides` is all-or-nothing. `bulk_changes` drops illegal, no-op, and unknown rows silently. `Reverse` flips a
copy only when both sides exist and turns a removal into the copy that restores it; a one-sided copy would have to
become a removal, so it stays.

## Guards (guards.rs)

- `MaxRemove { side }`: files removed on a side (a folder row counts its `subtree.files`) above `max_remove_percent` of
  that side's `file_count()`, or above `max_remove_count` (0 is off). `RemoveAnyway` releases.
- `EmptyRoot { side }`: `Snapshot::is_empty()`. Holds every removal on either side with no way to release it, and holds
  copies into that side until `CopyIntoEmpty` (the plan's ruling: an empty root is most likely an unmounted mount
  point).
- `SentinelMissing { side }`: the sentinel name (keyed with the pair's `NameRule`) isn't a non-folder entry of that
  root. Holds every action. The sentinel has to survive the scan's masks and the hidden-files toggle.
- Skew: pairs of non-folder entries with both times; each gap (right minus left, after the offset) rounds to the nearest
  900 s step when within the tolerance of one. Reported when one non-zero step up to 26 h covers ≥ 90% of pairs.
  `offset_secs` is how far the right runs ahead; its negation is the time offset that cancels it.
- `gate` splits acting rows into `run` and `held` (sentinel first, then empty root, then max remove); `=`, `·`, and `?`
  rows are in neither.

## View (view.rs)

`row_class` follows the action (`→`, `←` for copies left and removals on either side, `=`, `?` for `Undecided` or a
skipped unknown row, `≠` for any other skip). A row shows when its class filter, its presence filter (duplicates or
singles), and the warnings filter all let it. Decision: hiding never changes an action (TC and Krusader drop hidden rows
from the run); `PlanSummary::hidden_actions` counts what still runs unseen. Rows group under their parent folder; the
root's rows come first without a header, every other visible group gets "folder n of N".

## Planner laws (plan_proptests.rs)

Generated pairs (up to three levels over three names, files, symlinks, folders, held-back counts, unreadable folders,
clashes, a tenth of cases with one side blank) and a pure `apply_plan` model of a run check: determinism, no removals in
Update and Both ways, nothing written left in Update and Mirror, re-planning after a run is all `=` (Mirror), copies
nothing (Update, Both ways), names only grow (Both ways), no default action on `?`, clash, or unreadable rows, protected
entries (under unreadable keys, non-atomic folders) survive every mode, the warning flag is exact, overrides and bulk
keys never yield an illegal action, and guards trip exactly at their thresholds.
