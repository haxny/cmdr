# cmdr-sync

Synchronize folders, the pure part: what a volume can do, how names pair up across volumes, the one answer to "is this
copy newer?", and the walk that lists a folder on any `Volume`. No `tauri`, depends on `cmdr-fs` only.

## Module map

- **caps.rs**: `SyncCaps::of(&dyn Volume)` and `backend_facts(BackendKind)` (case sensitivity, mtime precision).
- **key.rs**: `NameRule` (`FoldCase` / `ExactCase`, NFC always) and `EntryKey`, the path of name keys.
- **compare.rs**: `EntryFacts`, `CompareRules`, `classify`. Tests sit beside each file as `*_test.rs`.
- **filter.rs**: `MaskFilter`, TC sync masks. **snapshot.rs**: `Snapshot`, `Entry`. **control.rs**: `SyncControl`.
- **scan.rs**: `scan()`. **content.rs**: `compare_contents()`, byte-by-byte for equal-size pairs.

## Must-knows

- ❌ **`classify` is shared with ⇧F2** (`cmdr-listing`'s compare). A change here moves Compare folders too; run
  `cargo nextest run -p cmdr-listing compare` with it.
- **`classify` never yields `NameClash` or `Unreadable`.** The planner sets those.
- **A missing time never claims "newer"**: sizes decide. Only a difference we can see counts.
- **Names pair by key, operations use each side's own spelling.** `NameRule::key` is for matching, ❌ never for a path
  handed to a volume.
- **The scanner never stats a child**: one listing per folder, and the listing's facts are the facts. Excluded folders
  are pruned before they are listed.
- **A folder in `Snapshot::unreadable` is unknown, not empty**: nothing beneath it may be removed on this snapshot's
  word.
- **`Junk` is not `Out`**: junk entries vanish from the snapshot uncounted; every `Out` entry counts as held back.
  `.cmdr-sync` is neither: the scanner skips it silently.
- **Content compare answers every job**: a job cancel stopped answers `Pending`, never nothing.
- **The crate never deletes permanently.** Phase 1 has no delete step at all.
- **No `cmdr-operations` dependency**: `cmdr-listing` depends on this crate and the operations test build depends on
  `cmdr-listing`, so a link back would be a cycle. The executor will sit behind a trait.
- No `tauri`, enforced by `index-crate-isolation`.

The classify table, the per-backend facts with evidence, and the dependency decision: `DETAILS.md`.
