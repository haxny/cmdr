# Synchronize folders (app side) details

## sync-profiles.json

```json
{
  "_schemaVersion": 1,
  "entries": [ { "id": "…", "name": "Photos", "left": { "volumeId": "root", "path": "/a" }, "right": { … },
                 "options": { … }, "showFilters": { … }, "restoreShowFilters": false, "overrides": { … } } ],
  "lastOptions": [ { "left": { … }, "right": { … }, "options": { … } } ]
}
```

- **Schema rules.** Version 1 reads and writes this shape. A file with another version (a newer build's, say) is set
  aside by `state_recovery`, not read half; the list starts empty and the next save writes the current version, so a
  newer build's data survives only in the set-aside copy. A future migration hook belongs in `read`, between the
  version check and the envelope: accept versions below the current one and upgrade their entries before returning. A
  field added later is additive: an older build keeps it through a save because entries are raw JSON objects (see
  below), but drops `lastOptions` entries it can't read. Bump the version only for a change an older build must not
  touch.
- **Why raw values.** The typed `SyncProfile` is the contract of THIS build. Keeping the stored object and merging the
  typed fields into it (`merge`, recursively for objects, replacing everything else) keeps what a newer build added
  inside `options` or `overrides` too. The one trap is a serde-tagged value (`KeepPolicy::location`): merging a
  `custom` location over a stored `targetRoot` would leave both keys, which no build reads. `merged` therefore reads
  the result back and falls back to the typed value whole when it isn't exactly the saved profile.
- **Why not `settings.json`.** It is sparse and per-setting (a value exists only when changed, with migrations keyed by
  setting id). Profiles are ordered records with their own lifecycle, validation, and recovery.
- **Replace rules.** `save_in` finds the target entry by id, else by name (trimmed, lowercased). A match keeps its
  place; any other entry with the saved name is removed, so a list can't hold two profiles with one name.
- **Recovery.** `StateFile::SyncProfiles` names the file in the recovery notice (`stateRecovery.syncProfiles.title`). A
  killed write's `.json.tmp` leftover is removed at load.
- **Per-profile overrides** (`ProfileOverrides`) are `None` to follow Settings. The profile editor, with the inherited
  value greyed next to each override, arrives with the dialog task.
- **Roots.** `roots_conflict` (same volume only, component-wise, case-sensitive) is for the dialog to refuse nested or
  identical roots. Whether two spellings name one folder on a case-insensitive volume is the volume's business, not
  decided here.
- **Write order and failure.** `edit_at` holds `disk` across the change and its write (lock order `disk`, then
  `cached`), so two windows can't write out of order and the file keeps the newest change. A write that doesn't land
  leaves the change in memory and answers `WriteFailed`, which every command passes on typed (`ProfileSaveRefusal` or
  `ProfileEditRefusal`); the dialog says it isn't saved.
- **Rename vs save.** `rename_in` refuses a name another profile has (`NameTaken`); `save_in` replaces in place. The
  dialog asks, then saves over.
- **No id.** An entry without a string `id` is listed `Invalid`/`Unreadable` under the key `no-id-<place>`, which
  delete, rename, and move accept, so it can be removed.
- **Limits.** `effective` clamps frontend-supplied Settings values (tolerance 0 to 60 s, removal percentage 1 to 99,
  count up to 1,000,000); 100% would disable the guard, so Settings stops at 99.
