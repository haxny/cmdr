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

## Keep (keep.rs, context.rs)

`keep_destination(policy, target caps, versions, rel_path)`: `TrashElseVersions` → `Trash` when `has_trash`, else the
versions folder; `VersionsAlways` → the folder; `TrashOnly` without a Trash → `Refused(NoTrash)` (the row is left alone,
never deleted). A folder destination is `<versions dir>/<rel parent>` (`kept_dir`), so names keep their folder
structure; a `rel_path` with an empty, `.`, or `..` part is `Refused(EscapesFolder)`.

`versions_root(policy, target_root, profile, stamp, attempt)`: `TargetRoot` is `<target root>/<folder_name>/<stamp>`
(`Place::Target`, a rename away); `folder_name` must be a plain relative path (`EscapesFolder`) under `.cmdr-sync/`
(`BadTemplate`), because the scanner skips only `.cmdr-sync` and Mirror would otherwise see kept items as right-only.
`Custom` is an absolute local-disk template (`Place::LocalDisk`) with `{root}` (the target root), `{profile}` (the name
with `/` turned into `-`, or "No profile"), and `{date}`; the stamp folder is appended when `{date}` is absent, so runs
never mix. Empty parts collapse (so `{root}` of `/` adds nothing); a `.` or `..` part, or a target root that climbs, is
`EscapesFolder`; unknown or unclosed placeholders and relative templates are `BadTemplate`. `attempt > 0` names the
stamp folder `<stamp> (attempt+1)`. `VersionsRoot::trusted` is the folder the person picked (the target root, or the
custom template up to its first placeholder); the run checks everything between it and the dated folder. `staging_root`
is the same-volume folder replaced files wait in: the policy's own when it is in the target root, else
`.cmdr-sync/versions` there. Laws: `kept_paths_never_escape_the_versions_folder` in `keep_test.rs`.

`RunContext::new` resolves both sides' versions and staging folders and refuses a custom folder inside either sync root
(`InsideSyncFolder`: it would be compared and synchronized like the person's files). Only a root whose `RunSide` says
`local_namespace` (the local disk or a mounted share, where the custom folder's path means the same file) takes part: a
remote root's path (`/R` on an SFTP server) never holds a local folder, whatever its spelling. `with_attempt(side, n)`
renames that side's dated folders for a retry.

## Run (steps.rs, execute.rs, runner.rs, runner/keeping.rs)

`steps(plan, run, ctx)` turns the rows `gate` let run into groups: `MakeFolders` per (place, side) first (staging and
versions subfolders, and target folders of split one-sided folders whose children are copied, parents first; made with
missing parents), then per landing folder (direction, folder) its `KeepAside` groups (each replaced file into the
staging folder on its own volume) right before its `Copy`, then `Remove` per (side, destination). Rows whose keep is
refused (by the final policy or the staging path) go to `refused` and get no step. Clash, type-clash, and unreadable
rows never get a step, whatever their action says.

Before any step, `execute` claims every dated folder the steps use with `EngineBatch::ClaimFolder` (made only if absent;
`Skipped` when something is there), bumping `attempt` (up to 100) until each is fresh, so a run never mixes into an
older one. First it observes each folder between `VersionsRoot::trusted` and the dated folder: a symlink or a file there
refuses everything kept in it (`VersionsFolderUnsafe`); `MakeFolders` never makes anything under a refused or failed
folder, and items kept there read `CouldNotDo`.

Every item is observed again right before its batch and must read exactly as the snapshot did (files: kind, size, time;
folders: the whole `SubtreeFacts`, digest included, but not the folder's own time, which Finder touches by browsing); a
mismatch is `ChangedSinceCompare`. File re-checks use size and time only, so on a volume that lists no times (some MTP
and WebDAV servers) a same-size edit passes. Before a keep-aside both the copy's source and the target are checked;
before a copy the landing spot must be empty; before any move into a versions folder the spot there must be empty
(`VersionsSpotTaken`). Every source goes to the engine as a `BoundSource` with the snapshot's facts, so the engine's own
`ExpectedSources` re-check covers the window between observe and act. `Copy`, `Move`, and `MoveBack` land with
`ConflictResolution::Skip`: an occupied spot comes back `Skipped`, never overwritten.

Decision: replaced files always go aside first into the staging folder on their own volume (a rename, so `MoveBack` can
always undo it), and only once the copy has landed does the policy apply (Trash, or a move to the custom folder);
anything that can't happen then leaves the file in the staging folder, which keeps it too. A copy that doesn't land
(failure, change, a blocked folder, a cancel or engine stop between keep and copy) moves its file back, unless something
now sits in its place: then `KeptNotReplaced`. A later engine option (`ConflictResolution::OverwriteKeeping`: copy to a
temp, then swap) could fold this into one step; deferred past phase 1.

Decision: no `expected_targets` twin in the engine. Every target a run replaces or removes is first the source of a keep
batch (`Move` or `Trash`), so the binding covers targets too.

Every copy row of the run that didn't end `Done` (failed, changed, refused, blocked, unplaced, never started) holds a
removal on its target side when the removal sits beside it (same parent folder) or reads like its source anywhere on
that side (kind, size, and time; folders: file count and bytes; a fact unknown on either side counts as a match, so the
hold errs toward waiting): in Mirror a rename, or a move to another folder, is a copy plus a removal, and the removal
must not run alone (`HeldAfterFailedCopy`, `src/runner/holds.rs`). One changing log file at the root holds only its
neighbours, not the whole side.

A source the engine never answered for in a batch it didn't stop counts as `Failed`. When the engine stops a batch (the
one-item policy and move-back batches included), unanswered sources are observed: a keep or removal source found at its
kept spot, or whose place no longer reads as the snapshot (gone, or something new there), counts as kept (a removal is
`Done`, a replaced file comes back with the leftovers, a staged file trashed by the policy is not counted as kept), a
copy whose landing spot is filled is `Done`. Rows the run never reached are `NotStarted`. Edge: an engine stop plus an
edit in place during that batch reads as kept (for a replacement then `KeptNotReplaced`), never as untouched; the report
errs toward "look in the versions folder". A Trash removal has no kept spot to look at, so after a stopped batch a
source that is gone for any reason (someone else deleted it too) reads as trashed (`Done`): the report can't tell the
Trash from another delete, and says `Done` rather than claim the file is still there.

Cancel (the in-process `SyncControl`) is checked between batches, between items while re-checking, and before each
engine call; the rest read `NotStarted`. The app's adapter also forwards it into a running engine operation (`run`).
Decision: a move back (`MoveBack`) and a replaced file's keep policy go through `SyncEngine::run_to_end` instead, which
never forwards the cancel: stopping either half way could strand the old file in the staging folder. Restoring set-aside
files runs after a cancel too.

`RunReport::empty_folders` lists target folders this run created (absent when observed just before `MakeFolders`) for
copies that then all didn't land; they stay, and the result screen can show them. Dated folders the run claimed but kept
nothing in (an earlier attempt's staging folder, a staging folder the Trash policy emptied) are removed again at the end
with `EngineBatch::RemoveEmptyFolders`, children first, one folder per batch. Contract: a single non-recursive `rmdir`,
never the volume's recursive delete, never following a symlink; hidden files (`.DS_Store`, AppleDouble `._*`) count as
content; a non-empty folder is `Failed` and stays. Before each one the run observes the folder and skips it unless it is
a real folder with all-zero `SubtreeFacts` (the scan's counts skip OS junk, so this is the second line of defence; the
engine's `rmdir` is the first). Only an engine `Done` counts as removed: `Skipped` (the engine found nothing there,
right after the observe saw it) is listed as left rather than guessed. A folder skipped or refused stays and is listed
in `RunReport::leftover_folders`. The fake flags any `rmdir` of a snapshot folder or of a folder the run didn't make.
`kept_bytes` counts what ended in a versions or staging folder (the result screen shows that size), not what went to the
Trash.

Crash: a kill between setting a replaced file aside and its copy landing leaves the old file in
`.cmdr-sync/versions/<stamp>/…`, recorded nowhere, and the next compare shows it missing from its folder (a copy in
Mirror, a gap in Update). Nothing is lost; the file sits in the versions folder. Finding stale staging folders on the
next run and offering to put their files back is phase 2.

Law: `execute_proptests.rs` runs random plans in every mode, with the default or a custom versions folder, against
random edits before or between batches and a random in-process cancel or engine stop; the fake flags any move, trash, or
copy of something that differs from the snapshot (anywhere beneath it), any landing on an occupied spot, any `rmdir` of
a snapshot folder, and anything lost, and every replacement row that isn't `Done` must have its old file back in place
or read `KeptNotReplaced`.

## Profiles (profile.rs)

`effective(defaults, options, overrides)` is the one answer to "what does this compare and run use": the profile's
override when it has one, else the Settings value the dialog passed as `SyncDefaults`. `time_offset_secs` has no
Settings value, so it is the profile's own. Defaults of `SyncOptions` follow the user's decisions: both ways, date and
size, subfolders on, the OS junk in the exclude list.

`validate` returns the first `ProfileProblem` (empty name, a mask list `MaskFilter::parse` refuses, a tolerance override
over 60 s, a removal percentage outside 1 to 100); `Unreadable` is the store's, for an entry that doesn't read as a
profile at all. `check_keep_policy` runs the versions-folder rules of `keep.rs` against a stand-in root, so Settings can
show `BadTemplate` and `EscapesFolder` next to the field; `InsideSyncFolder` needs the real roots and stays a run-time
refusal. `roots_conflict` compares path components on one volume only (`/a` and `/ab` don't conflict).
