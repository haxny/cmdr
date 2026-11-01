# cmdr-sync

Synchronize folders, the pure part: what a volume can do, how names pair up across volumes, the one answer to "is this
copy newer?", and the walk that lists a folder on any `Volume`. No `tauri`, depends on `cmdr-fs` only.

## Module map

- **caps.rs**: `SyncCaps::of(&dyn Volume)` and `backend_facts(BackendKind)` (case sensitivity, mtime precision).
- **key.rs**: `NameRule` (`FoldCase` / `ExactCase`, NFC always) and `EntryKey`, the path of name keys.
- **compare.rs**: `EntryFacts`, `CompareRules`, `classify`. Tests sit beside each file as `*_test.rs`.
- **filter.rs**: `MaskFilter`, TC sync masks. **snapshot.rs**: `Snapshot`, `Entry`. **control.rs**: `SyncControl`.
- **scan.rs**: `scan()`. **content.rs**: `compare_contents()`, byte-by-byte for equal-size pairs.
- **plan.rs**: `plan()`, `reclassify()`, the mode table. **overrides.rs**: legal actions, bulk keys. **guards.rs**:
  `check()`, `gate()`. **view.rs**: show-filters, folder headers, summary. Laws: `plan_proptests.rs`.
- **profile.rs**: `SyncProfile`, `effective()` (Settings plus the profile's overrides), `validate()`,
  `roots_conflict()`, `check_keep_policy()`. The store is the app's.
- **keep.rs**, **context.rs**: Trash or versions folder paths, `RunContext`. **steps.rs**: plan → ordered steps.
  **execute.rs** + **runner.rs** (+ `src/runner/keeping.rs`): `execute()` over the `SyncEngine` trait; fake and laws in
  `execute_fake.rs`, `execute_proptests.rs`.

## Must-knows

- ❌ **`classify` is shared with ⇧F2** (`cmdr-listing`'s compare). A change here moves Compare folders too; run
  `cargo nextest run -p cmdr-listing compare` with it.
- **`classify` never yields `NameClash` or `Unreadable`.** The planner sets those.
- **A missing time never claims "newer"**: sizes decide. Only a difference we can see counts.
- **Names pair by key, operations use each side's own spelling** (`Plan::heads` for folders). `NameRule::key` is for
  matching, ❌ never for a path handed to a volume.
- **The scanner never stats a child**: one listing per folder, and the listing's facts are the facts. Excluded folders
  are pruned before they are listed.
- **A folder in `Snapshot::unreadable` is unknown, not empty**: nothing beneath it may be removed on this snapshot's
  word.
- **`Junk` is not `Out`**: junk entries vanish from the snapshot uncounted; every `Out` entry counts as held back.
  `.cmdr-sync` is neither: the scanner skips it silently.
- **Never act on partial knowledge**: rows at or under an unreadable key, name clashes, and type clashes are `Skip` and
  take no other action; a one-sided folder with anything held back, unreadable, app-owned, or clashing beneath it is
  split into its children, never copied or removed whole. `?` rows never get an automatic action.
- **Hiding a row never changes its action**; the summary counts hidden actions.
- **Removals are always held beside an empty root**, no acknowledgement releases them. `gate` re-runs `check`.
- **The crate never deletes permanently.** `EngineBatch` has no delete variant (only `rmdir` of empty folders the run
  made); replaced and removed items go to the Trash or a versions folder, or are left alone (`KeepRefusal`).
- **Every item is re-observed right before its batch** (a folder by `subtree_now`, digest included); any difference is
  `ChangedSinceCompare`, never a widened action. A replaced file waits on its own volume until its copy lands, else goes
  back; a copy that didn't land holds removals beside it or of its old name.
- **A move back and a keep policy go through `SyncEngine::run_to_end`**, which a cancel never stops (a stranded old
  file). Every other batch may stop on cancel.
- **No `cmdr-operations` dependency** (a package cycle through `cmdr-listing`): the app implements `SyncEngine`.
- **`profile::effective` is the one place "inherit or override" is decided.** `time_offset_secs` is per profile and
  never inherited. A profile that fails `validate` is shown greyed by the store, never dropped.
- No `tauri`, enforced by `index-crate-isolation`.

The classify table, the per-backend facts with evidence, and the dependency decision: `DETAILS.md`.
