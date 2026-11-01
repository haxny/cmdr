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
