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
